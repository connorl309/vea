use asm::isa::framing;

use crate::processor::{Core, Trap};
use super::decode::*;

// EX/MEM latch
// For non-memory instructions this will push things down
// to writeback and latch results as needed. For memory
// instructions things will stall as needed to provide time to
// read/write data. For simulation purposes we will assume
// register file reads happen instantly and writes latch
// their write value at the end of the current cycle.
#[derive(Debug, Default, Clone)]
pub struct ExMemLatch {

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
 * For simulation purposes memory operations will stall for 3 cycles.
 *
 * For simulation purposes we should *eventually* model a dcache
 * but I'm not doing that yet.
 *
 */
impl Core {
    pub fn execute(&mut self, idex: &IdExLatch) -> Result<ExMemLatch, Trap> {
        // A bubble in EX does nothing and drives no read ports.
        if !idex.valid {
            self.regs.read_rd_port_config = None;
            self.regs.read_rs1_port_config = None;
            self.regs.read_rs2_port_config = None;
            return Ok(ExMemLatch::default());
        }

        // Our biggest problem is identifying whether or not rd/rs1/rs2 actually do anything.
        // We can pull out all these values from the regfile just in case, since rd can also serve
        // as the base value in stores.
        self.regs.read_rd_port_config = idex.rd;
        self.regs.read_rs1_port_config = idex.rs1;
        self.regs.read_rs2_port_config = idex.rs2;

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
            0x02..=0x03 => self.exec_move(idex, rs1),

            // integer ALU - register form 0x10..=0x18, immediate form
            // 0x20..=0x26: add/sub/and/or/xor/nor/shl/shr/sar (+ the -i forms)
            0x10..=0x18 | 0x20..=0x26 => self.exec_alu(idex, rs1, rs2),

            // set-if-predicate: slt/sltu/seq/sne/sle/sleu/sge/sgeu
            0x30..=0x37 => self.exec_set_predicate(idex, rs1, rs2),

            // compare-and-branch: beq/bne/blt/bltu/bge/bgeu
            0x40..=0x45 => self.exec_branch(idex, rs1, rs2),

            // unconditional control flow: jmp/jr/call/callr/ret
            0x48..=0x4C => self.exec_jump(idex, rs1),

            // loads - address = rb + signed disp
            0x50..=0x53 => self.exec_load(idex, rs1),

            // stores - address = rb + signed disp
            0x54..=0x57 => self.exec_store(idex, rd_rb, rs1, rs2),

            // decode already vets the opcode against the table, so this is only
            // reachable if that check and this match ever drift apart.
            _ => Err(Trap::IllegalInstruction { pc: idex.pc }),
        }
    }

    // --- one helper per opcode family -----------------------------------
    // Each of these fills an EX/MEM latch (or redirects control flow). All
    // stubbed for now - the match above is the wiring, these are the meat.

    // nop does nothing; halt stops the machine.
    fn exec_system(&mut self, idex: &IdExLatch) -> Result<ExMemLatch, Trap> {
        let _ = idex;
        todo!("nop / halt")
    }

    // rd = rs (mov) or rd = imm (movi).
    fn exec_move(&mut self, idex: &IdExLatch, rs: u64) -> Result<ExMemLatch, Trap> {
        let _ = (idex, rs);
        todo!("mov / movi")
    }

    // rd = rs1 OP rs2, or rd = rs1 OP imm for the -i forms. The ALU works at
    // 64 bits and the result is masked to plen further down the pipe.
    fn exec_alu(&mut self, idex: &IdExLatch, rs1: u64, rs2: u64) -> Result<ExMemLatch, Trap> {
        let _ = (idex, rs1, rs2);
        todo!("add/sub/and/or/xor/nor/shl/shr/sar (+ immediate forms)")
    }

    // rd = (rs1 OP rs2) ? 1 : 0.
    fn exec_set_predicate(&mut self, idex: &IdExLatch, rs1: u64, rs2: u64) -> Result<ExMemLatch, Trap> {
        let _ = (idex, rs1, rs2);
        todo!("slt/sltu/seq/sne/sle/sleu/sge/sgeu")
    }

    // if (rs1 OP rs2) pc = pc + imm.
    fn exec_branch(&mut self, idex: &IdExLatch, rs1: u64, rs2: u64) -> Result<ExMemLatch, Trap> {
        let _ = (idex, rs1, rs2);
        todo!("beq/bne/blt/bltu/bge/bgeu")
    }

    // jmp/call carry the target in the immediate; jr/callr take it in a
    // register; ret pops it off the return-address stack.
    fn exec_jump(&mut self, idex: &IdExLatch, target_reg: u64) -> Result<ExMemLatch, Trap> {
        let _ = (idex, target_reg);
        todo!("jmp/jr/call/callr/ret")
    }

    // rd = mem[rb + disp], sign- or zero-extended per the flags nibble. Memory
    // ops stall 3 cycles in the sim (see the module comment).
    fn exec_load(&mut self, idex: &IdExLatch, rb: u64) -> Result<ExMemLatch, Trap> {
        let _ = (idex, rb);
        todo!("ld/ldw/ldwu/ldh/ldhu/ldb/ldbu")
    }

    // mem[rb + disp] = rs, truncated to the store width.
    fn exec_store(&mut self, idex: &IdExLatch, rb: u64, rs1: u64, rs2: u64) -> Result<ExMemLatch, Trap> {
        let _ = (idex, rb, rs1, rs2);
        todo!("st/stw/sth/stb")
    }
}
