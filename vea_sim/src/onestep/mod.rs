// The onestep module

/**
 * The onestep module is a one-cycle-per-instruction simulator
 * for Vea. There are no simulated latches or delays. Onestep
 * is to verify correctness of instruction execution implementation.
 * This behavior will be more accurately mirrored in the pipelined simulator.
 */

pub use crate::assembler::*;
pub use crate::isa;
use crate::isa::NUM_REGS;
use crate::error;

// Model of the processor for sim purposes.
pub struct Processor {
    pub pc: u64,
    pub completed_instrs: u64,
    halted: bool,
    regs: isa::RegisterFile,
    cc: isa::ConditionCodes,
}

impl Processor {
    pub fn new() -> Self {
        Processor {
            pc: 0,
            completed_instrs: 0,
            halted: false,
            regs: [0u64; NUM_REGS],
            cc: isa::ConditionCodes::reset()
        }
    }

    // Quick halted getter function
    pub fn halted(&self) -> bool { self.halted }

    // wrapper around `tick` for N times.
    pub fn cycle(&mut self, amount: u64) {
        for _ in 0..amount {
            if self.halted { break }
        }
    }
    fn tick(&mut self) {

    }
}