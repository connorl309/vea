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

        // Each arm carries out the instruction and reports what happens to the
        // program counter. `Step::Next(len)` falls through to the next
        // instruction, `Step::Jump(addr)` redirects control flow.
        let step = match opcode {
            0x00 => Step::Next(2), // nop

            0xFF => { self.halted = true; Step::Next(2) } // halt

            0x01 => { let i = self.unary(pc)?; self.set_reg(i.reg, i.src);  Step::Next(i.len) } // mov  rd = src
            0x14 => { let i = self.unary(pc)?; self.set_reg(i.reg, !i.src); Step::Next(i.len) } // not  rd = !src

            // cmp / cmp.s : set the condition codes from `reg - src`
            0x20 => { let i = self.unary(pc)?; self.compare(self.get_reg(i.reg), i.src, Sign::Unsigned); Step::Next(i.len) }
            0x21 => { let i = self.unary(pc)?; self.compare(self.get_reg(i.reg), i.src, Sign::Signed);   Step::Next(i.len) }

            // add sub and or xor shl shr sar mul div : rd = a OP b, op in the low nibble
            0x10..=0x13 | 0x15..=0x1A => {
                let i = self.binary(pc)?;
                self.set_reg(i.rd, alu(opcode & 0x0F, i.a, i.b)?);
                Step::Next(i.len)
            }

            // b / beq / bne / blt / bge / bgt / ble : predicate in the flag nibble
            // An immediate target is a signed offset from the branch's own
            // address; a register target is an absolute address.
            0x30 => {
                let opinfo = memory::read(pc + 1, 1)? as u8;
                let (dest, len) = self.target(pc, Rel::Relative)?;
                if predicate(opinfo & 0x0F)?(&self.cc) {
                    Step::Jump(dest)
                } else {
                    Step::Next(len)
                }
            }

            // jmp : unconditional, always an absolute target.
            0x31 => Step::Jump(self.target(pc, Rel::Absolute)?.0),

            // ld : reg <- mem[base + (disp | index)], width and sign from the flags
            0x40 => {
                let m = self.mem(pc)?;
                let raw = memory::read(m.addr, m.width)?;
                let value = if m.sext { sign_extend(raw, m.width) } else { raw as i64 };
                self.set_reg(m.reg, value);
                Step::Next(m.len)
            }

            // st : mem[base + (disp | index)] <- reg, low `width` bytes
            0x41 => {
                let m = self.mem(pc)?;
                memory::write(m.addr, m.width, self.regs[m.reg])?;
                Step::Next(m.len)
            }

            // trap : the exception vector is a one byte immediate payload.
            0xFE => {
                let vector = fetch_imm(pc + 2, plen(memory::read(pc + 1, 1)? as u8))?;
                return sim_err!("trap {vector:#04x} at PC={pc:#018x} is not implemented");
            }

            _ => return sim_err!("illegal opcode {opcode:#04x} at PC={pc:#018x}"),
        };

        self.completed_instrs += 1;
        self.pc = match step {
            Step::Next(len) => align_up(pc + len, isa::ALIGNMENT),
            Step::Jump(dest) => dest,
        };
        Ok(())
    }

    // ---- operand decoding -------------------------------------------------

    // mov / not / cmp : the named register then one source operand, which is a
    // register or the sign-extended immediate.
    fn unary(&self, at: u64) -> error::Result<Unary> {
        let opinfo = memory::read(at + 1, 1)? as u8;
        let reg = reg_at(at + 2)?;
        let (src, n) = self.source(at + 3, opinfo)?;
        Ok(Unary { reg, src, len: 3 + n })
    }

    // add .. div : a destination register, a first source register, then a
    // second source operand, which is a register or the sign-extended immediate.
    fn binary(&self, at: u64) -> error::Result<Binary> {
        let opinfo = memory::read(at + 1, 1)? as u8;
        let rd = reg_at(at + 2)?;
        let a = self.get_reg(reg_at(at + 3)?);
        let (b, n) = self.source(at + 4, opinfo)?;
        Ok(Binary { rd, a, b, len: 4 + n })
    }

    // ld / st : the named register (dest for ld, stored value for st), then the
    // effective address `base + offset` where offset is a signed displacement
    // immediate (ALSO_IMMEDIATE set) or an index register (clear). The access
    // width (flag bits [2:1]) and narrow-load sign extension (LS_SEXT) come from
    // the flag nibble too.
    fn mem(&self, at: u64) -> error::Result<Mem> {
        let opinfo = memory::read(at + 1, 1)? as u8;
        let flags = opinfo & 0x0F;
        let reg = reg_at(at + 2)?;
        let base = self.get_reg(reg_at(at + 3)?);
        let (offset, n) = self.source(at + 4, opinfo)?;
        let width = match (flags >> 1) & 0b11 {
            0b00 => 8,
            0b01 => 1,
            0b10 => 2,
            _ => 4,
        };
        Ok(Mem {
            reg,
            addr: (base as u64).wrapping_add(offset as u64),
            width,
            sext: flags & LS_SEXT != 0,
            len: 4 + n,
        })
    }

    // b* / jmp : the single target operand. An immediate (payload length > 0) is
    // a branch offset relative to `at` or an absolute address per `rel`; a bare
    // register (payload length 0) is always an absolute address.
    fn target(&self, at: u64, rel: Rel) -> error::Result<(u64, u64)> {
        let opinfo = memory::read(at + 1, 1)? as u8;
        match plen(opinfo) {
            0 => Ok((self.get_reg(reg_at(at + 2)?) as u64, 3)),
            p => {
                let imm = fetch_imm(at + 2, p)?;
                let dest = match rel {
                    Rel::Relative => at.wrapping_add(imm),
                    Rel::Absolute => imm,
                };
                Ok((dest, 2 + p as u64))
            }
        }
    }

    // The trailing source operand at `at`: the sign-extended immediate when the
    // ALSO_IMMEDIATE flag is set, otherwise the value of the register named
    // there. Also returns how many payload bytes it consumed.
    fn source(&self, at: u64, opinfo: u8) -> error::Result<(i64, u64)> {
        if opinfo & OPINFO_FLAG_ALSO_IMMEDIATE != 0 {
            Ok((fetch_imm(at, plen(opinfo))? as i64, plen(opinfo) as u64))
        } else {
            Ok((self.get_reg(reg_at(at)?), 1))
        }
    }

    // ---- register / flag access ----------------------------------------

    // Read a register as a signed value.
    fn get_reg(&self, reg: usize) -> i64 {
        self.regs[reg] as i64
    }

    // Write a signed result back into a register.
    fn set_reg(&mut self, reg: usize, value: i64) {
        self.regs[reg] = value as u64;
    }

    // Set the condition codes from `a - b`. `cmp` (unsigned) puts the borrow in
    // N and leaves V clear, while `cmp.s` (signed) puts the sign of the difference in
    // N and the signed overflow in V.
    fn compare(&mut self, a: i64, b: i64, sign: Sign) {
        let (ua, ub) = (a as u64, b as u64);
        let (neg, overflow) = match sign {
            Sign::Signed => {
                let (diff, ov) = a.overflowing_sub(b);
                (diff < 0, ov)
            }
            Sign::Unsigned => (ua < ub, false),
        };
        self.cc.set(ua == ub, neg, ua >= ub, overflow);
    }
}

// A branch predicate
fn predicate(pred: u8) -> error::Result<fn(&isa::ConditionCodes) -> bool> {
    Ok(match pred {
        BR_ALWAYS => |_| true,
        BR_EQ => |cc: &isa::ConditionCodes| cc.zero(),
        BR_NE => |cc: &isa::ConditionCodes| !cc.zero(),
        BR_LT => |cc: &isa::ConditionCodes| cc.neg() != cc.overflow(),
        BR_GE => |cc: &isa::ConditionCodes| cc.neg() == cc.overflow(),
        BR_GT => |cc: &isa::ConditionCodes| !cc.zero() && cc.neg() == cc.overflow(),
        BR_LE => |cc: &isa::ConditionCodes| cc.zero() || cc.neg() != cc.overflow(),
        _ => return sim_err!("unknown branch predicate {pred:#x}"),
    })
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

// What happens to the PC after an instruction.
enum Step {
    Next(u64), // advance past `len` instruction bytes
    Jump(u64), // continue at this address
}

// How a branch immediate is interpreted.
enum Rel {
    Relative, // signed offset from the instruction address
    Absolute, // address as-is
}

// How a compare treats its operands.
enum Sign {
    Signed,
    Unsigned,
}

// mov / not / cmp operands.
struct Unary { reg: usize, src: i64, len: u64 }

// add .. div operands.
struct Binary { rd: usize, a: i64, b: i64, len: u64 }

// ld / st operands, address already resolved.
struct Mem { reg: usize, addr: u64, width: usize, sext: bool, len: u64 }

// Immediate payload length, from the opinfo high nibble.
fn plen(opinfo: u8) -> u8 {
    opinfo >> 4
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
    Ok(sign_extend(bits, len as usize) as u64)
}

// Sign-extend the low `bytes` bytes of `v` to a full signed 64-bit value.
fn sign_extend(v: u64, bytes: usize) -> i64 {
    let shift = 64 - bytes as u32 * 8;
    ((v << shift) as i64) >> shift
}

// Round `v` up to the next multiple of `a`, matching the assembler's per
// instruction alignment padding and the fetch rule in isa.rs.
fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) / a * a
}
