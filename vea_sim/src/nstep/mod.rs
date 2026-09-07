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

// The five pipeline stages in program order
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Fetch,
    Decode,
    Execute,
    Memory,
    Writeback,
}

// todo - pipe the line