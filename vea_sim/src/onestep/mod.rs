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
use crate::memory;
use crate::sim_err;

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

    fn tick(&mut self) -> error::Result<()> {
        let pc = self.pc;
        let opcode = memory::read(pc, 1)? as u8;
        if self.halted {
            return sim_err!("tick() called while the simulator is halted");
        }

        // Each arm carries out the instruction and evaluates to its length in
        // bytes; `pc` then advances past it, rounded up to the ISA alignment.
        let len: u64 = match opcode {
            0x00 => 2, // nop

            0xFF => { self.halted = true; 2 } // halt

            0x01 => { let i = self.unary(pc)?; self.set(i.rd, i.src); i.len } // mov  rd = src

            0x14 => { let i = self.unary(pc)?; self.set(i.rd, !i.src); i.len } // not  rd = !src

            // add sub and or xor shl shr sar mul div : rd = a OP b, op in the low nibble
            0x10..=0x13 | 0x15..=0x1A => {
                let i = self.binary(pc)?;
                self.set(i.rd, alu(opcode & 0x0F, i.a, i.b)?);
                i.len
            }

            0xFE => { // trap : exception vector is a one byte immediate
                let vector = fetch_imm(pc + 2, plen(memory::read(pc + 1, 1)? as u8))?;
                return sim_err!("trap {vector:#04x} at PC={pc:#018x} is not implemented");
            }

            _ => return sim_err!("illegal opcode {opcode:#04x} at PC={pc:#018x}"),
        };

        self.completed_instrs += 1;
        self.pc = align_up(pc + len, isa::ALIGNMENT);
        Ok(())
    }

    // Write a signed result back into a register.
    fn set(&mut self, reg: usize, value: i64) {
        self.regs[reg] = value as u64;
    }

    // mov / not / cmp : a destination register then one source operand, which
    // is a register or the sign-extended immediate.
    fn unary(&self, at: u64) -> error::Result<Unary> {
        let opinfo = memory::read(at + 1, 1)? as u8;
        let rd = reg_at(at + 2)?;
        let (src, n) = self.source(at + 3, opinfo)?;
        Ok(Unary { rd, src, len: 3 + n })
    }

    // add .. div : a destination register, a first source register, then a
    // second source operand, which is a register or the sign-extended immediate.
    fn binary(&self, at: u64) -> error::Result<Binary> {
        let opinfo = memory::read(at + 1, 1)? as u8;
        let rd = reg_at(at + 2)?;
        let a = self.get(reg_at(at + 3)?);
        let (b, n) = self.source(at + 4, opinfo)?;
        Ok(Binary { rd, a, b, len: 4 + n })
    }

    // The trailing source operand at `at`: the sign-extended immediate when the
    // ALSO_IMMEDIATE flag is set, otherwise the value of the register named
    // there. Also returns how many payload bytes it consumed.
    fn source(&self, at: u64, opinfo: u8) -> error::Result<(i64, u64)> {
        if opinfo & OPINFO_FLAG_ALSO_IMMEDIATE != 0 {
            Ok((fetch_imm(at, plen(opinfo))? as i64, plen(opinfo) as u64))
        } else {
            Ok((self.get(reg_at(at)?), 1))
        }
    }

    // Read a register as a signed value.
    fn get(&self, reg: usize) -> i64 {
        self.regs[reg] as i64
    }
}

// mov / not / cmp operands.
struct Unary { rd: usize, src: i64, len: u64 }

// add .. div operands.
struct Binary { rd: usize, a: i64, b: i64, len: u64 }

// Immediate payload length, from the opinfo high nibble.
fn plen(opinfo: u8) -> u8 {
    opinfo >> 4
}

// Apply an ALU op (the opcode low nibble) to signed operands.
fn alu(op: u8, a: i64, b: i64) -> error::Result<i64> {
    Ok(match op {
        0x0 => a.wrapping_add(b),
        0x1 => a.wrapping_sub(b),
        0x2 => a & b,
        0x3 => a | b,
        0x5 => a ^ b,
        0x6 => a.wrapping_shl(b as u32),                  // shl
        0x7 => (a as u64).wrapping_shr(b as u32) as i64,  // shr, logical
        0x8 => a.wrapping_shr(b as u32),                  // sar, arithmetic
        0x9 => a.wrapping_mul(b),
        0xA if b == 0 => return sim_err!("divide by zero"),
        0xA => a.wrapping_div(b),
        _ => return sim_err!("no ALU operation for nibble {op:#x}"),
    })
}

// Read one register index from `addr` and confirm it names a real register.
fn reg_at(addr: u64) -> error::Result<usize> {
    let r = memory::read(addr, 1)? as usize;
    if r >= NUM_REGS {
        return sim_err!("register operand r{r} at {addr:#018x} is out of range (0..{NUM_REGS})");
    }
    Ok(r)
}

// Decode an instruction's immediate payload into a value we can work with.
// VEA stores every immediate in the fewest signed bytes that round-trip (see the
// assembler's `imm_bytes`), so the machine widens it back to a full 64 bits by
// sign extension. `at` is the first immediate byte and `len` the payload length
// from the opinfo high nibble; `len == 0` is the immediate value zero.
fn fetch_imm(at: u64, len: u8) -> error::Result<u64> {
    if len > 8 {
        return sim_err!("immediate payload length {len} at {at:#018x} exceeds 8 bytes");
    }
    if len == 0 {
        return Ok(0);
    }
    let mut bits = 0u64;
    for i in 0..len as u64 {
        bits = (bits << 8) | memory::read(at + i, 1)?;
    }
    let shift = 64 - len as u32 * 8;
    Ok((((bits << shift) as i64) >> shift) as u64)
}

// Round `v` up to the next multiple of `a`, matching the assembler's per
// instruction alignment padding and the fetch rule in isa.rs.
fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) / a * a
}
