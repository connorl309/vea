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
    // Per-stage stall signals for the current cycle, recomputed every `cycle`.
    // A stalled stage does not latch a fresh result from its input and the
    // stage ahead of it sees a bubble.
    pub stall_fetch: bool,
    pub stall_decode: bool,
    pub stall_execute: bool,
    // EX-internal sequential state for a multi-cycle op: `Some(n)` = the op in
    // EX still needs `n` stall cycles before its work cycle, `None` = EX is
    // combinational this cycle.
    pub ex_pending: Option<u32>,
    // DEBUG: breakpoints; list of (PC, name)
    breakpoints: HashMap<String, u64>,
    // TODO: return-address stack / link register for call/rets

    ifid: IfIdLatch,
    idex: IdExLatch,
    ex: ExLatch,
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
            stall_fetch: false,
            stall_decode: false,
            stall_execute: false,
            ex_pending: None,
            breakpoints: HashMap::new(),
            ifid: IfIdLatch::default(),
            idex: IdExLatch::as_reset(),
            ex: ExLatch::default(),
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
        } else if self.stall_fetch {
            format!("IF  (nop-skip pc={:#06x})", self.ifid.pc)
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
        let ex = match self.ex_pending {
            Some(n) => format!("EX  (mem access, {n} stall cycle(s) left)"),
            None => format!("EX  {:?}", self.ex),
        };
        [ifid, idex, ex]
    }

    /**
     *      EXECUTION
     *
     * The pipeline is IF -> ID -> EX -> WB. There is no MEM stage: memory
     * access is folded into EX, which for a load/store occupies the stage for
     * a fixed MEM_ACCESS_CYCLES. This asserts `stall_execute` and holding the
     * front of the pipe (IF/ID, ID/EX, PC) until the access cycle.
     *
     * `step()` advances the pipeline by `count` whole clock cycles.
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

        // EX: work the ID/EX latch from last cycle. A multi-cycle op (a memory
        // access) asserts `stall_execute`; when it does, the front of the pipe
        // holds - ID/EX keeps the same instruction so it re-enters EX next
        // cycle - and only the EX latch advances (a bubble toward WB).
        let ex = self.execute(&idex_in)?;
        self.ex = ex;

        if !self.stall_execute {
            // ID: decode the IF/ID latch from last cycle.
            let idex = self.decode(&ifid_in)?;
            // IF: pull the frame at PC.
            let ifid = self.fetch()?;

            // "Clock" edge - latch it all.
            self.idex = idex;
            self.ifid = ifid;
            // On a nop-skip cycle fetch produced no frame, so PC just steps over
            // the `00 00` pad (PC += INSTR_ALIGN). Otherwise it moves past the
            // whole frame that was just fetched.
            self.pc = if self.stall_fetch {
                pc_in + framing::INSTR_ALIGN
            } else {
                Self::next_pc(&ifid, pc_in)
            };
        }
        self.cycles += 1;

        logger::line(format!(
            "cyc {:>8}  IF[{}]  ID[{}]  EX[{}]  pc->{:#06x}{}",
            self.cycles,
            frame_tag(&self.ifid),
            latch_tag(self.idex.valid, self.idex.instr.mnemonic),
            latch_tag(idex_in.valid, idex_in.instr.mnemonic),
            self.pc,
            if self.stall_execute {
                "  EX-stall"
            } else if self.stall_fetch {
                "  nop-skip"
            } else {
                ""
            },
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
    if !ifid.valid {
        return format!("nop-skip @{:#06x}", ifid.pc);
    }
    format!("op {:#04x} @{:#06x}", ifid.bytes[0], ifid.pc)
}