use asm::isa::{Form, InstrDef, framing};

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
        // Our biggest problem is identifying whether or not rd/rs1/rs2 actually do anything.
        // We can pull out all these values from the regfile just in case, since rd can also serve
        // as the base value in stores.
        self.regs.read_rd_port_config = idex.rd;
        self.regs.read_rs1_port_config = idex.rs1;
        self.regs.read_rs2_port_config = idex.rs2;

        let rd_rb: u64 = self.regs.read_port(idex.rd);
        let rs1: u64 = self.regs.read_port(idex.rs1);
        let rs2: u64 = self.regs.read_port(idex.rs2);

        todo!()
    }
}
