use asm::isa::{Form, InstrDef, framing};

use crate::processor::Trap;
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

pub fn execute(idex: IdExLatch) -> Result<ExMemLatch, Trap> {
    todo!()
}