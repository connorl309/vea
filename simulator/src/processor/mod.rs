use std::collections::{HashMap, HashSet};

use asm::isa::registers;
use asm::isa::*;

use crate::memory;
use crate::pipeline::*;
pub const REG_COUNT: usize = registers::COUNT as usize;
const ICACHE_SIZE: usize = 16 * (asm::isa::framing::PLEN_MAX + 2);

// The icache which defaults empty.
const ICACHE: [u8; ICACHE_SIZE] = [0u8; ICACHE_SIZE];

#[derive(Debug, Clone, Default)]
pub struct RegisterFile {
    // The actual bank of registers enumerated 0..REG_COUNT
    pub registers: [u64; REG_COUNT],
    // We support 3 read ports/1 write port to the regfile.
    // There are implicit transport lanes coming from the register
    // file control logic for write data input/read data outputs.
    pub write_port_config: Option<(u8, u64)>, // write(reg, val)
    pub read_rd_port_config: Option<u8>,
    pub read_rs1_port_config: Option<u8>,
    pub read_rs2_port_config: Option<u8>,
}

impl RegisterFile {
    // Combinational read: drive a port with a register index and the value
    // drops out the same cycle. An unconfigured port (`None`) settles to zero,
    // same as the read mux would with nothing selected.
    pub fn read_port(&self, sel: Option<u8>) -> u64 {
        match sel {
            Some(i) => self.registers[i as usize],
            None => 0,
        }
    }
}

// Actual execution unit tracking. Shares a lot of stuff with the pipeline module
#[derive(Debug, Clone)]
pub struct Core {
    // r0..r{REG_COUNT-1}.
    pub regs: RegisterFile,
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

    // pub pipeline: ProcessorPipeline,
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
            regs: RegisterFile::default(),
            pc: pc,
            halted: false,
            retired: 0,
            state: CoreState::Running(pc),
            breakpoints: HashMap::new(),
            // pipeline: ProcessorPipeline::new(),
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
        self.regs.registers[reg as usize] = value;
    }

    pub fn step(count: usize) {
        todo!()
    }
}