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

// Byte length of the instruction frame at `bytes[0]`. Every frame is 2 bytes of
// opcode + opinfo; the opinfo high nibble carries the immediate payload length
// and the opcode says how many register-operand bytes sit before it. Fetch
// steps the PC by this; Decode re-derives the operands from the same layout.
pub(crate) fn frame_len(bytes: &[u8]) -> error::Result<u64> {
    let opcode = bytes[0];
    let opinfo = bytes[1];
    let plen = u64::from(opinfo >> 4);
    // trailing source operand: an immediate payload, or one register byte
    let tail = if opinfo & OPINFO_FLAG_ALSO_IMMEDIATE != 0 { plen } else { 1 };

    Ok(2 + match opcode {
        0x00 | 0xFF => 0,                                // nop / halt
        0x01 | 0x14 | 0x20 | 0x21 => 1 + tail,           // mov / not / cmp / cmp.s
        0x10..=0x13 | 0x15..=0x1A => 2 + tail,           // add .. div
        0x40 | 0x41 => 2 + tail,                         // ld / st
        0x30 | 0x31 => if plen == 0 { 1 } else { plen }, // b* / jmp: reg target or immediate
        0xFE => plen,                                    // trap
        _ => return sim_err!("illegal opcode {opcode:#04x} in fetched frame"),
    })
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
            halted: self.halted,
            fault,
        });
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