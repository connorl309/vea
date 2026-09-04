use std::collections::{HashMap, HashSet};

use asm::isa::registers;
use asm::isa::*;

use crate::memory;
pub const REG_COUNT: usize = registers::COUNT as usize;

// The architectural register file.
pub type RegFile = [u64; REG_COUNT];

// Actual execution unit, so this guy will implement basic high levels of each instruction.
// *Currently* we will just execute everything in one "cycle" until I bother to actually rig
// up the pipeline logic with the core state.
#[derive(Debug, Clone)]
pub struct Core {
    // r0..r{REG_COUNT-1}.
    pub regs: RegFile,
    // Program counter. Bit 0 is always 0 and reserved as a tag bit.
    pub pc: u64,
    // Set once a `halt` retires.
    pub halted: bool,
    // Instructions retired since the last reset.
    pub retired: u64,
    // Current core state
    pub state: CoreState,
    // DEBUG: breakpoints; list of (PC, name)
    breakpoints: HashMap<String, u64>,
    // TODO: return-address stack / link register for call/rets
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreState {
    Running(u64), // Running (current PC)
    Stopped, // Hit the HALT instruction
    Exception(Trap)
}

// Something that stops or diverts the core (exceptions, a syscall, etc)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    // A `halt` retired.
    Halt,
    // Opcode/flags had no entry in the ISA table.
    IllegalInstruction { pc: u64 },
    // Frame length disagreed with the opcode table, or operands were malformed.
    MalformedInstruction { pc: u64 },
    // PC was odd. Normally this should be impossible, but if this trips it means I
    // writing this code fucked something up.
    MisalignedPc { pc: u64 },
    // A load or store faulted.
    Memory(crate::memory::Fault),
}

impl Core {
    // Create a new core object. Will evolve over time.
    pub fn new(pc: u64) -> Self {
        Core {
            regs: [0; REG_COUNT],
            pc: pc,
            halted: false,
            retired: 0,
            state: CoreState::Running(pc),
            breakpoints: HashMap::new(),
        }
    }

    /**
     *      DEBUG FUNCTIONS
     * 
     * This section of 'core' functions are for debug purposes and
     * will be either impossible to create or unreasonable to have
     * if this develops to real hardware, such as breakpoints/prints
     * or explicit register modifications.
     */
    pub fn register_breakpoint(&mut self, name: String, address: u64) {
        self.breakpoints.insert(name, address);
    }
    pub fn remove_breakpoint(&mut self, name: String) {
        self.breakpoints.remove(&name);
    }
    pub fn force_reg(&mut self, reg: u8, value: u64) {
        self.regs[reg as usize] = value;
    }

    pub fn step(count: usize) {
        todo!()
    }
}