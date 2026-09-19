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
use crate::shared;
use crate::sim_err;
use std::fmt;

mod fetch;
mod decode;
mod execute;
mod writeback;
#[cfg(test)]
mod pipeline_tests;

// The five pipeline stages in program order
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Fetch,
    Decode,
    Execute,
    Memory,
    Writeback,
}

// IF/ID pipeline stage
#[derive(Clone)]
pub struct IfId {
    pub pc: u64,
    // A full frame; only the leading `frame_len` bytes are this instruction,
    // the rest is lookahead for Decode.
    pub bytes: [u8; isa::MAX_INSN_BYTES],
}

// ID/EX pipeline stage. By the time an instruction lands here its registers
// and condition codes have already been read, so Execute just works off these
// values instead of reaching back into the register file itself.
#[derive(Clone)]
pub struct IdEx {
    pub pc: u64,
    pub op: DecodedOp,
}

// One decoded instruction, operands and all. This is different from Onestep's
// approach because unlike onestep, I am trying to be much more accurate here,
// so I will explicitly break everything out
#[derive(Clone, Debug, PartialEq)]
pub enum DecodedOp {
    Nop,
    Halt,
    Mov { rd: usize, src: i64 },
    Not { rd: usize, src: i64 },
    Cmp { a: i64, b: i64, signed: bool },
    Alu { rd: usize, nibble: u8, a: i64, b: i64 },
    Branch { taken: bool, target: u64 }, // both b* and jmp resolve here
    Load { rd: usize, addr: u64, width: usize, sext: bool },
    Store { addr: u64, width: usize, value: u64 },
    Trap { vector: u64 },
}

// How the ID/EX pane in the TUI shows what's decoded - short, not a
// disassembly. `alu_symbol` below does the same for the op nibble.
impl fmt::Display for DecodedOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodedOp::Nop => write!(f, "nop"),
            DecodedOp::Halt => write!(f, "halt"),
            DecodedOp::Mov { rd, src } => write!(f, "r{rd} <- {src:#x}"),
            DecodedOp::Not { rd, src } => write!(f, "r{rd} <- !{src:#x}"),
            DecodedOp::Cmp { a, b, signed } => {
                write!(f, "cmp{} {a:#x}, {b:#x}", if *signed { ".s" } else { "" })
            }
            DecodedOp::Alu { rd, nibble, a, b } => {
                write!(f, "r{rd} <- {a:#x} {} {b:#x}", alu_symbol(*nibble))
            }
            DecodedOp::Branch { taken, target } => {
                write!(f, "-> {target:#x} ({})", if *taken { "taken" } else { "not taken" })
            }
            DecodedOp::Load { rd, addr, width, sext } => {
                write!(f, "r{rd} <- [{addr:#x}]{} ({width}B)", if *sext { " sext" } else { "" })
            }
            DecodedOp::Store { addr, width, value } => {
                write!(f, "[{addr:#x}] <- {value:#x} ({width}B)")
            }
            DecodedOp::Trap { vector } => write!(f, "trap {vector:#x}"),
        }
    }
}

// A short symbol for an ALU op's low nibble. Display only - execute.rs has
// its own copy of the actual opcode table.
fn alu_symbol(nibble: u8) -> &'static str {
    match nibble {
        0x0 => "+",
        0x1 => "-",
        0x2 => "&",
        0x3 => "|",
        0x5 => "^",
        0x6 => "<<",
        0x7 => ">>",
        0x8 => ">>a",
        0x9 => "*",
        0xA => "/",
        _ => "?",
    }
}

// EX/WB pipeline stage. Execute has already done whatever math or memory
// access the instruction needed; this is just the leftover architectural
// effect for Writeback to apply. Decode also peeks at this to forward a
// result that hasn't reached `regs`/`cc` yet - see decode.rs.
#[derive(Clone, Copy)]
pub struct ExWb {
    pub pc: u64,
    pub commit: Commit,
}

// What Writeback actually commits. Most instructions land on Reg or Nothing;
// Cmp/cmp.s land on Flags instead of a register. Halt still rides the normal
// three stages to get here. only the "stop fetching anything past this
// point" part happens early, in Decode. Otherwise an in-flight instruction
// ahead of the halt in EX/WB would have its result dropped on the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Commit {
    Nothing,
    Reg { rd: usize, value: i64 },
    Flags { z: bool, n: bool, c: bool, v: bool },
    Halt,
}

// How the EX/WB pane in the TUI shows what's about to commit.
impl fmt::Display for Commit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Commit::Nothing => write!(f, "\u{2014}"),
            Commit::Reg { rd, value } => write!(f, "r{rd} <- {value:#x}"),
            Commit::Flags { z, n, c, v } => {
                write!(f, "flags z{} n{} c{} v{}", *z as u8, *n as u8, *c as u8, *v as u8)
            }
            Commit::Halt => write!(f, "halt"),
        }
    }
}

// Byte length of the instruction frame at `bytes[0]`. The high nibble of opinfo
// holds the length. Fetch steps the PC by this length.
// A frame has at least 2 bytes: the opcode and opinfo.
pub(crate) fn frame_len(bytes: &[u8]) -> error::Result<u64> {
    let len = isa::insn_len(bytes[1]);
    if len < 2 {
        return sim_err!("frame length {len} is less than 2 for opcode {:#04x}", bytes[0]);
    }
    Ok(u64::from(len))
}

// The pipelined processor. Fetch, Decode and Execute are wired; Writeback is
// still a stub that drops whatever Execute hands it. `tick()` runs the four
// stages back to front so each one reads the latch the stage ahead of it left
// last cycle before that latch gets overwritten.
pub struct Processor {
    pub pc: u64,
    pub cycles: u64,
    pub completed_instrs: u64,
    halted: bool,
    regs: isa::RegisterFile,
    cc: isa::ConditionCodes,
    if_id: Option<IfId>,
    id_ex: Option<IdEx>,
    ex_wb: Option<ExWb>,
    // Set the moment Decode sees a halt. Fetch checks this and stops feeding
    // the pipe anything new, but `halted` itself doesn't flip until the halt
    // actually commits in Writeback
    halt_pending: bool,
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
            if_id: None,
            id_ex: None,
            ex_wb: None,
            halt_pending: false,
        }
    }

    // Quick halted getter function
    pub fn halted(&self) -> bool { self.halted }

    // Advance up to `clocks` cycles, stopping early on a halt or fault
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
        let fault = outcome.as_ref().err().map(|e| e.to_string());
        self.publish_with(fault);
        outcome
    }

    // "One" clock "cycle". I use air quotes because this is all hand waving.
    fn tick(&mut self) -> error::Result<()> {
        if self.halted {
            return sim_err!("tick() called while the simulator is halted");
        }

        self.writeback()?;
        self.execute()?;
        self.decode()?;
        self.fetch()?;

        self.cycles += 1;
        Ok(())
    }

    // Copy the architectural state onto the shared bus the UI renders from.
    pub fn publish(&self) {
        self.publish_with(None);
    }

    fn publish_with(&self, fault: Option<String>) {
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
            cycles: self.cycles,
            halted: self.halted,
            fault,
            pipeline: Some(self.pipeline()),
        });
    }

    // A snapshot of what's sitting in each latch right now, for the TUI's
    // pipeline pane. IF/ID hasn't been decoded yet, so its description is
    // just the frame's own bytes rather than anything semantic.
    fn pipeline(&self) -> shared::Pipeline {
        let slot = |pc: u64, desc: String| shared::StageSlot { pc, desc };
        shared::Pipeline {
            if_id: self.if_id.as_ref().and_then(|frame| {
                let len = frame_len(&frame.bytes).ok()? as usize;
                Some(slot(frame.pc, hex(&frame.bytes[..len])))
            }),
            id_ex: self.id_ex.as_ref().map(|latch| slot(latch.pc, latch.op.to_string())),
            ex_wb: self.ex_wb.as_ref().map(|latch| slot(latch.pc, latch.commit.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::frame_len;
    use crate::assembler::assemble_raw;
    use crate::isa::MAX_INSN_BYTES;

    // frame_len must agree with the byte count the assembler actually emits.
    fn check(src: &str) {
        let raw = assemble_raw(src).expect("assembles");
        assert_eq!(raw.len(), 1);
        let mut frame = [0u8; MAX_INSN_BYTES];
        frame[..raw[0].len()].copy_from_slice(&raw[0]);
        assert_eq!(frame_len(&frame).unwrap() as usize, raw[0].len(), "for `{src}`");
    }

    #[test]
    fn frame_len_matches_the_assembler() {
        for src in [
            "nop",
            "halt",
            "mov r1, #5",
            "mov r1, r2",
            "mov r1, 0x1122334455667788",
            "not r3, r4",
            "add r2, r1, r1",
            "add r2, r1, #7",
            "cmp r1, r2",
            "cmp.s r1, #-3",
            "div r5, r6, r7",
            "ld r5, [r6]",
            "ld.w r1, [r2 + 0x10]",
            "st [r8 - #8], r9",
            "b 0x08",
            "jmp 0x2000",
        ] {
            check(src);
        }
    }
}