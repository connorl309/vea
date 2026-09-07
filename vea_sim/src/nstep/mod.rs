// The nstep module

/**
 * The nstep module is the pipelined simulator. Where `onestep` retires one
 * whole instruction per call and has no notion of time, `nstep` models a
 * classic in-order pipeline: several instructions are in flight at once and
 * `tick()` advances the machine by a single clock edge.
 */

pub use crate::assembler::*;
pub use crate::isa;
use crate::error;
use crate::memory;
use crate::shared;
use crate::sim_err;

mod fetch;
mod decode;
mod execute;
mod writeback;

// The five pipeline stages in program order
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Fetch,
    Decode,
    Execute,
    Memory,
    Writeback,
}

// Model of the pipelined processor. TODO: the pipeline
pub struct Processor {
    pub pc: u64,
    pub cycles: u64,
    pub completed_instrs: u64,
    halted: bool,
    regs: isa::RegisterFile,
    cc: isa::ConditionCodes,
}

impl Processor {
    pub fn new() -> Self {
        Processor {
            pc: 0,
            cycles: 0,
            completed_instrs: 0,
            halted: false,
            regs: [0u64; isa::NUM_REGS],
            cc: isa::ConditionCodes::reset(),
        }
    }

    // Quick halted getter function
    pub fn halted(&self) -> bool { self.halted }

    // Advance the machine by up to `clocks` cycles
    pub fn cycle(&mut self, clocks: u64) -> error::Result<()> {
        let mut outcome = Ok(());
        for _ in 0..clocks {
            if self.halted {
                break;
            }
            if let Err(e) = self.tick() {
                outcome = Err(e);
                break;
            }
        }
        self.publish();
        outcome
    }

    // One clock edge. TODO: evaluate WB, EX, ID, IF against the pipeline
    // registers latched last cycle, then latch this cycle's results.
    fn tick(&mut self) -> error::Result<()> {
        if !memory::is_mapped(self.pc) {
            return sim_err!("instruction fetch at unmapped address {:#018x}", self.pc);
        }

        // Pipeline stages in code are executed backwards from their
        // physical layout
        self.writeback()?;
        self.execute()?;
        self.decode()?;
        self.fetch()?;

        self.cycles += 1;
        sim_err!("nstep::tick is not implemented yet")
    }

    // Copy the architectural state onto the shared bus the UI renders from.
    pub fn publish(&self) {
        shared::publish(shared::Snapshot {
            pc: self.pc,
            regs: self.regs,
            flags: shared::Flags {
                z: self.cc.zero(),
                n: self.cc.neg(),
                c: self.cc.carry(),
                v: self.cc.overflow(),
            },
            completed_instrs: self.completed_instrs,
            halted: self.halted,
            fault: None,
        });
    }
}