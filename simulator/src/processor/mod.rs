use std::collections::{HashMap, HashSet};

use asm::isa::registers;
use asm::isa::*;

use crate::logger;
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
    // Clock cycles elapsed since the last reset.
    pub cycles: u64,
    // Current core state
    pub state: CoreState,
    // DEBUG: breakpoints; list of (PC, name)
    breakpoints: HashMap<String, u64>,
    // TODO: return-address stack / link register for call/rets

    ifid: IfIdLatch,
    idex: IdExLatch,
    exmem: ExMemLatch,
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
            cycles: 0,
            state: CoreState::Running(pc),
            breakpoints: HashMap::new(),
            ifid: IfIdLatch::default(),
            idex: IdExLatch::as_reset(),
            exmem: ExMemLatch::default(),
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

    // DEBUG: a one-line summary of each pipeline latch, for the UI.
    pub fn pipeline_debug(&self) -> [String; 3] {
        let ifid = if self.ifid.valid {
            format!("IF  pc={:#06x}  op={:#04x}", self.ifid.pc, self.ifid.bytes[0])
        } else {
            "IF  (bubble)".to_string()
        };
        let idex = if self.idex.valid {
            format!(
                "ID  {}  rd={:?} rs1={:?} rs2={:?}  imm={:#x}",
                self.idex.instr.mnemonic,
                self.idex.rd,
                self.idex.rs1,
                self.idex.rs2,
                self.idex.imm_raw,
            )
        } else {
            "ID  (bubble)".to_string()
        };
        [ifid, idex, format!("EX  {:?}", self.exmem)]
    }

    /**
     *      EXECUTION
     *                                |------|
     * The pipeline is IF -> ID -> EX -> MEM -> WB. 
     * One thing to note is that EX can bypass MEM entirely
     * if the instruction does not need to engage with memory.
     * 
     * `step` advances it by
     * `count` whole clock cycles. MEM and WB aren't built yet, so an
     * instruction that reaches EX bottoms out in a todo!() inside `execute` -
     * with a straight-line program that lands three cycles after `step`
     * starts (IF, then ID, then EX).
     */
    pub fn step(&mut self, count: usize) -> Result<u64, Trap> {
        for _ in 0..count {
            if self.halted {
                logger::line("step: core is halted");
                break;
            }
            if let Err(trap) = self.cycle() {
                self.state = CoreState::Exception(trap.clone());
                logger::line(format!("TRAP   {trap:?}"));
                return Err(trap);
            }
        }
        Ok(self.cycles)
    }

    // One clock cycle. Every stage reads the latch the previous stage produced
    // *last* cycle, then all latches update together on the edge - so the
    // next-states are all computed from the current latches before any commit.
    fn cycle(&mut self) -> Result<(), Trap> {
        let ifid_in = self.ifid;
        let idex_in = self.idex;
        let pc_in = self.pc;

        // EX: work the ID/EX latch from last cycle.
        let exmem = self.execute(&idex_in)?;
        // ID: decode the IF/ID latch from last cycle.
        let idex = self.decode(&ifid_in)?;
        // IF: pull the frame at PC.
        let ifid = self.fetch()?;

        // "Clock" edge - latch it all.
        self.exmem = exmem;
        self.idex = idex;
        self.ifid = ifid;
        self.pc = Self::next_pc(&ifid, pc_in);
        self.cycles += 1;

        logger::line(format!(
            "cyc {:>8}  IF[{}]  ID[{}]  EX[{}]  pc->{:#06x}",
            self.cycles,
            frame_tag(&self.ifid),
            latch_tag(self.idex.valid, self.idex.instr.mnemonic),
            latch_tag(idex_in.valid, idex_in.instr.mnemonic),
            self.pc,
        ));
        Ok(())
    }

    // Where PC goes next cycle: past the frame just fetched (2 + PLEN bytes),
    // rounded up to the instruction alignment. No branch redirect yet - EX
    // would supply that once compare-and-branch is implemented.
    fn next_pc(ifid: &IfIdLatch, pc: u64) -> u64 {
        let (plen, _) = framing::unpack_len_flags(ifid.bytes[1]);
        (pc + 2 + plen as u64).next_multiple_of(framing::INSTR_ALIGN)
    }
}

// "add @0x0004" / "(bubble)" - a stage's occupant for the cycle log.
fn latch_tag(valid: bool, mnemonic: &str) -> String {
    if valid { mnemonic.to_string() } else { "bubble".to_string() }
}

fn frame_tag(ifid: &IfIdLatch) -> String {
    format!("op {:#04x} @{:#06x}", ifid.bytes[0], ifid.pc)
}