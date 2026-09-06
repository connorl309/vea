use std::ops::Shr;

use asm::isa::{FLAG_IMM, framing};

use crate::{logger, processor::{Core, Trap}};
use super::decode::*;

// A memory access is folded into EX (there is no MEM stage) and occupies the
// stage for a fixed number of cycles: `MEM_ACCESS_CYCLES - 1` stall cycles with
// `stall_execute` held, then the access itself on the last one.
pub const MEM_ACCESS_CYCLES: u32 = 3;

// EX latch (feeds WB).
//
// There is no separate MEM stage: memory access is folded into EX.
// Register reads are assumed 'instant' in the same cycle, for all intents and purposes.
// Memory reads/writes will take a fixed 3-cycle stall.
#[derive(Debug, Default, Clone)]
pub struct ExWbLatch {
    pub dest_reg_info: Option<(u8, u64)>, // [reg, value] if there is a reg write to do.
    pub dest_mem_info: Option<(u64, u64)>, // store [addr], [value]
    pub set_pc_to: Option<u64>, // if this is Some(x) then pc <- x
}

/**
 * Decode is responsible for taking the input IdExWbLatch structure
 * and actually working on executing the "stuff" needed to do here.
 * For anything register-register we can mostly accomplish it in
 * one shot here, excepting memory operations.
 *
 * The ALU, in silicon, will eventually be a fast parallel prefix
 * adder/subtractor in conjunction with the other more basic bitwise
 * operations. TODO: Implement mul/divide
 * 
 * For simulation purposes we should *eventually* model a dcache
 * but I'm not doing that yet.
 *
 */
impl Core {
    pub fn execute(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {
        // `stall_execute` is a combinational output of this stage. A memory op
        // still mid-access re-asserts it below; every other path leaves it low.
        self.stall_execute = false;

        // A bubble in EX does nothing, and latches should
        // not update.
        if !idex.valid {
            let mut default = ExWbLatch::default();
            return Ok(ExWbLatch::default());
        }

        // Now - which opcode does what! The big logic block. Opcodes are laid
        // out in contiguous ranges by family, so each arm
        // is just the range for one family handing off to a helper below.
        match idex.instr.opcode {
            // machine control: nop (0x00), halt (0xFF)
            framing::OPCODE_PAD_NOP | framing::OPCODE_HALT => self.exec_system(idex),

            // moves / constants: mov, movi (both opcode 0x02, FLAG_IMM picks movi)
            0x02 => self.exec_move(idex),

            // integer ALU: add/sub/and/or/xor/nor/shl/shr/sar, plus the -i
            // immediate forms on the same opcodes (FLAG_IMM set)
            0x10..=0x18 => self.exec_alu(idex),

            // set-if-predicate: slt/sltu/seq/sne/sle/sleu/sge/sgeu, plus the
            // -i immediate forms on the same opcodes (FLAG_IMM set)
            0x30..=0x37 => self.exec_set_predicate(idex),

            // compare-and-branch: beq/bne/blt/bltu/bge/bgeu, plus the -i
            // immediate forms on the same opcodes (FLAG_IMM set)
            0x40..=0x45 => self.exec_branch(idex),

            // unconditional control flow: jmp/jr/call/callr/ret
            0x48..=0x4C => self.exec_jump(idex),

            // loads - address = rb + signed disp
            0x50..=0x53 => self.exec_load(idex),

            // stores - address = rb + signed disp
            0x54..=0x57 => self.exec_store(idex),

            // decode already vets the opcode against the table, just a CYA
            _ => Err(Trap::IllegalInstruction { pc: idex.pc }),
        }
    }

    // nop does nothing; halt stops the machine.
    fn exec_system(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {
        let _ = idex;
        if idex.instr.opcode == 0xFF {
            self.halted = true;
            logger::line(format!("HALT encountered at PC={:#08x}", idex.pc));
        }
        Err(Trap::Halt)
    }

    // rd = rs (mov) or rd = imm (movi). `movi` is Form::RI - no source
    // register - so only `rd` is required here; decode already rejects a `mov`
    // frame that is missing its source operand (plen < reg_count).
    fn exec_move(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {
        let Some(rd) = idex.rd else {
            return Err(Trap::MalformedInstruction { pc: idex.pc });
        };

        let move_value: u64 = if idex.instr.flags & FLAG_IMM != 0 {
            idex.imm_raw
        } else {
            self.read_reg(idex.rs1, idex.pc)?
        };

        Ok(ExWbLatch {
            dest_reg_info: Some((rd, move_value)),
            dest_mem_info: None,
            set_pc_to: None,
        })
    }

    // rd = rs1 OP rs2, or rd = rs1 OP imm for the -i forms (same opcode, with
    // FLAG_IMM set). The ALU works at the full 64-bit register width.
    fn exec_alu(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {
        let value_a: u64 = self.read_reg(idex.rs1, idex.pc)?;
        // Second operand: rs2, or the instruction's immediate for a -i form.
        let value_b: u64 = if idex.instr.flags & FLAG_IMM != 0 {
            idex.imm_raw
        } else {
            self.read_reg(idex.rs2, idex.pc)?
        };
        let rd = idex.rd.unwrap();

        // Special case the NOT instruction
        let result: u64 = if idex.instr.opcode == 0x15 { !value_a } else {
            // Operation is the opcode's low nibble
            match idex.instr.opcode & 0xF {
                0x0 => Ok((value_a as i64 + value_b as i64) as u64),               // add
                0x1 => Ok((value_a as i64).saturating_sub(value_b as i64) as u64), // sub
                0x2 => Ok(value_a & value_b),                                      // and
                0x3 => Ok(value_a | value_b),                                      // or
                0x4 => Ok(value_a ^ value_b),                                      // xor
                0x5 => Ok(!(value_a | value_b)),                                   // nor
                0x6 => Ok(value_a << value_b),                                     // shl
                0x7 => Ok(value_a.shr(value_b)),                                   // shr (logical)
                0x8 => Ok((value_a as i64).shr(value_b) as u64),                   // sar (arithmetic)
                _ => return Err(Trap::IllegalInstruction { pc: idex.pc }),
            }?
        };

        Ok(ExWbLatch { dest_reg_info: Some((rd, result)), dest_mem_info: None, set_pc_to: None })
    }

    // rd = (rs1 OP rs2) ? 1 : 0, or rd = (rs1 OP imm) ? 1 : 0 for the -i forms.
    // The -i forms share the opcode; pick the second operand off `FLAG_IMM`
    // (`idex.instr.flags & asm::isa::FLAG_IMM`), same idea as exec_alu.
    fn exec_set_predicate(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {

        todo!("slt/sltu/seq/sne/sle/sleu/sge/sgeu (+ slti/... -i forms)")
    }

    // Register form: if (rs1 OP rs2) pc = imm (the branch target / label).
    // -i form (FLAG_IMM set): if (rs1 OP imm) pc = rs2, i.e. the comparand is
    // the immediate and rs2 holds the target address.
    fn exec_branch(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {

        todo!("beq/bne/blt/bltu/bge/bgeu (+ beqi/... -i forms)")
    }

    // jmp/call carry the target in the immediate; jr/callr take it in a
    // register; ret pops it off the return-address stack.
    fn exec_jump(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {

        todo!("jmp/jr/call/callr/ret")
    }

    // rd = mem[rb + disp], sign- or zero-extended per the flags nibble. The
    // access is held for a fixed latency first (see `mem_access_tick`); only the
    // data path below is still a stub.
    fn exec_load(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {
        if self.mem_access_tick().is_none() {
            return Ok(ExWbLatch::default());
        }

        todo!("ld/ldw/ldwu/ldh/ldhu/ldb/ldbu")
    }

    // mem[rb + disp] = rs & store_width_mask
    fn exec_store(&mut self, idex: &IdExWbLatch) -> Result<ExWbLatch, Trap> {
        if self.mem_access_tick().is_none() {
            return Ok(ExWbLatch::default());
        }

        todo!("st/stw/sth/stb")
    }

    // Advance the fixed memory-access latency for the load/store currently in
    // EX. Returns `None` on a stall cycle - EX drives a bubble to WB and, via
    // `stall_execute`, the front of the pipe (IF/ID, ID/EX, PC) holds - and
    // `Some(())` on the cycle the access should actually be performed.
    fn mem_access_tick(&mut self) -> Option<()> {
        let remaining = self
            .ex_pending
            .unwrap_or(MEM_ACCESS_CYCLES.saturating_sub(1));
        if remaining > 0 {
            self.ex_pending = Some(remaining - 1);
            self.stall_execute = true;
            return None;
        }
        self.ex_pending = None;
        Some(())
    }
}
