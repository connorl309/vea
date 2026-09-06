use std::ops::Shr;

use asm::isa::framing;

use crate::processor::{Core, Trap};
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
pub struct ExLatch {
    pub dest_reg_info: Option<(u8, u64)>, // [reg, value] if there is a reg write to do.
    pub dest_mem_info: Option<(u64, u64)> // store [addr], [value]
}

/**
 * Decode is responsible for taking the input IdExLatch structure
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
    pub fn execute(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {
        // `stall_execute` is a combinational output of this stage. A memory op
        // still mid-access re-asserts it below; every other path leaves it low.
        self.stall_execute = false;

        // A bubble in EX does nothing and drives no read ports.
        if !idex.valid {
            return Ok(ExLatch::default());
        }

        // Our biggest problem is identifying whether or not rd/rs1/rs2 actually do anything.
        // We can pull out all these values from the regfile just in case, since rd can also serve
        // as the base value in stores.
        let rd_rb: u64 = self.regs.read_port(idex.rd);
        let rs1: u64 = self.regs.read_port(idex.rs1);
        let rs2: u64 = self.regs.read_port(idex.rs2);

        // Now - which opcode does what! The big logic block. Opcodes are laid
        // out in contiguous ranges by family, so each arm
        // is just the range for one family handing off to a helper below.
        match idex.instr.opcode {
            // machine control: nop (0x00), halt (0xFF)
            framing::OPCODE_PAD_NOP | framing::OPCODE_HALT => self.exec_system(idex),

            // moves / constants: mov, movi
            0x02..=0x03 => self.exec_move(idex),

            // integer ALU - register form 0x10..=0x18, immediate form
            // 0x20..=0x26: add/sub/and/or/xor/nor/shl/shr/sar (+ the -i forms)
            0x10..=0x18 | 0x20..=0x26 => self.exec_alu(idex),

            // set-if-predicate: slt/sltu/seq/sne/sle/sleu/sge/sgeu
            0x30..=0x37 => self.exec_set_predicate(idex),

            // compare-and-branch: beq/bne/blt/bltu/bge/bgeu
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
    fn exec_system(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {
        let _ = idex;
        if idex.instr.opcode == 0xFF {
            self.halted = true;
        }
        todo!("nop / halt")
    }

    // rd = rs (mov) or rd = imm (movi).
    fn exec_move(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {
        // This assert should never fire.
        assert!(idex.rd.is_some());
        return match idex.instr.opcode {
            0x20 => { // mov rd,rs
                let rs_value = self.regs.read_port(idex.rs1);
                let rd = idex.rd.unwrap();
                Ok(ExLatch {
                    dest_reg_info: Some((rd, rs_value)),
                    dest_mem_info: None,
                })
            }
            0x21 => { // mov rd, imm
                let imm_val = idex.imm_raw;
                let rd = idex.rd.unwrap();
                Ok(ExLatch { dest_reg_info: Some((rd, imm_val)), dest_mem_info: None })
            }
            _ => panic!("Error: exec_move() was called in the execute pipeline stage, but the provided opcode for this stage wasn't a move!")
        };
    }

    // rd = rs1 OP rs2, or rd = rs1 OP imm for the -i forms. The ALU works at
    // 64 bits and the result is masked to plen further down the pipe.
    fn exec_alu(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {
        // Any ALU instruction is at least going to have a source register.
        let value_a: u64 = self.regs.read_port(idex.rs1);
        // The second operand will either be another register or an immediate.
        // Since I did not design this ISA super well this check should suffice,
        // as the only inputs to this function should trigger if idex == alu instruction.
        let value_b: u64 = if (idex.instr.opcode & 0x20) != 0 { idex.imm_raw } else { self.regs.read_port(idex.rs2) };
        let rd = idex.rd.unwrap();
        // Now do the actual ALU operation
        return match (idex.instr.opcode & 0xF) {
            0x0 => { // adds
                Ok(ExLatch {
                    dest_reg_info: Some((rd, (value_a as i64 + value_b as i64) as u64)),
                    dest_mem_info: None
                })
            }
            0x1 => { // subtracts
                Ok(ExLatch {
                    dest_reg_info: Some((rd, (value_a as i64).saturating_sub(value_b as i64) as u64)),
                    dest_mem_info: None,
                })
            }
            0x2 => { // and
                Ok(ExLatch {
                    dest_reg_info: Some((rd, value_a & value_b)),
                    dest_mem_info: None,
                })
            }
            0x3 => { // or
                Ok(ExLatch {
                    dest_reg_info: Some((rd, value_a | value_b)),
                    dest_mem_info: None,
                })
            }
            0x4 => { // xor
                Ok(ExLatch {
                    dest_reg_info: Some((rd, value_a ^ value_b)),
                    dest_mem_info: None,
                })
            }
            0x5 => { // shl
                Ok(ExLatch {
                    dest_reg_info: Some((rd, value_a << value_b)),
                    dest_mem_info: None,
                })
            }
            0x6 => { // shr (logical)
                Ok(ExLatch {
                    dest_reg_info: Some((rd, value_a.shr(value_b))),
                    dest_mem_info: None,
                })
            }
            0x7 => { // sar (arithmetic)
                Ok(ExLatch {
                    dest_reg_info: Some((rd, (value_a as i64).shr(value_b) as u64)),
                    dest_mem_info: None,
                })
            }
            _ => Err(Trap::IllegalInstruction { pc: idex.pc })
        }
    }

    // rd = (rs1 OP rs2) ? 1 : 0.
    fn exec_set_predicate(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {

        todo!("slt/sltu/seq/sne/sle/sleu/sge/sgeu")
    }

    // if (rs1 OP rs2) pc = pc + imm.
    fn exec_branch(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {

        todo!("beq/bne/blt/bltu/bge/bgeu")
    }

    // jmp/call carry the target in the immediate; jr/callr take it in a
    // register; ret pops it off the return-address stack.
    fn exec_jump(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {

        todo!("jmp/jr/call/callr/ret")
    }

    // rd = mem[rb + disp], sign- or zero-extended per the flags nibble. The
    // access is held for a fixed latency first (see `mem_access_tick`); only the
    // data path below is still a stub.
    fn exec_load(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {
        if self.mem_access_tick().is_none() {
            return Ok(ExLatch::default());
        }

        todo!("ld/ldw/ldwu/ldh/ldhu/ldb/ldbu")
    }

    // mem[rb + disp] = rs, truncated to the store width.
    fn exec_store(&mut self, idex: &IdExLatch) -> Result<ExLatch, Trap> {
        if self.mem_access_tick().is_none() {
            return Ok(ExLatch::default());
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
