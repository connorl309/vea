use crate::nstep::{Commit, DecodedOp, ExWb, Processor};
use crate::{error, memory, sim_err};

impl Processor {
    // EX: do the actual work Decode set up
    pub fn execute(&mut self) -> error::Result<()> {
        let Some(latch) = self.id_ex.take() else {
            self.ex_wb = None;
            return Ok(());
        };

        let commit = run(latch.pc, latch.op)?;
        self.ex_wb = Some(ExWb { pc: latch.pc, commit });
        Ok(())
    }
}

// Everything here is already-resolved operand values from Decode
fn run(pc: u64, op: DecodedOp) -> error::Result<Commit> {
    Ok(match op {
        DecodedOp::Nop => Commit::Nothing,
        // Decode already stopped Fetch; this still has to reach Writeback
        // normally so anything ahead of it in the pipe commits first.
        DecodedOp::Halt => Commit::Halt,

        DecodedOp::Mov { rd, src } => Commit::Reg { rd, value: src },
        DecodedOp::Not { rd, src } => Commit::Reg { rd, value: !src },

        DecodedOp::Cmp { a, b, signed } => {
            let (z, n, c, v) = compare(a, b, signed);
            Commit::Flags { z, n, c, v }
        }

        DecodedOp::Alu { rd, nibble, a, b } => Commit::Reg { rd, value: alu(nibble, a, b)? },

        // Already handled in decode
        DecodedOp::Branch { .. } => Commit::Nothing,

        DecodedOp::Load { rd, addr, width, sext } => {
            let raw = memory::read(addr, width)?;
            let value = if sext { sign_extend(raw, width) } else { raw as i64 };
            Commit::Reg { rd, value }
        }
        DecodedOp::Store { addr, width, value } => {
            memory::write(addr, width, value)?;
            Commit::Nothing
        }

        DecodedOp::Trap { vector } => {
            return sim_err!("trap {vector:#04x} at pc {pc:#018x} is not implemented");
        }
    })
}

// Same rule as onestep's compare(): `cmp` (unsigned) puts the borrow in N and
// leaves V clear; `cmp.s` (signed) puts the sign of `a - b` in N and the
// signed overflow in V. Returns (z, n, c, v).
fn compare(a: i64, b: i64, signed: bool) -> (bool, bool, bool, bool) {
    let (ua, ub) = (a as u64, b as u64);
    let (neg, overflow) = if signed {
        let (diff, ov) = a.overflowing_sub(b);
        (diff < 0, ov)
    } else {
        (ua < ub, false)
    };
    (ua == ub, neg, ua >= ub, overflow)
}

// Same ALU table as onestep's alu()
fn alu(nibble: u8, a: i64, b: i64) -> error::Result<i64> {
    Ok(match nibble {
        0x0 => a.wrapping_add(b),
        0x1 => a.wrapping_sub(b),
        0x2 => a & b,
        0x3 => a | b,
        0x5 => a ^ b,
        0x6 => a.wrapping_shl(b as u32),                 // shl
        0x7 => (a as u64).wrapping_shr(b as u32) as i64, // shr, logical
        0x8 => a.wrapping_shr(b as u32),                 // sar, arithmetic
        0x9 => a.wrapping_mul(b),
        0xA if b == 0 => return sim_err!("divide by zero"),
        0xA => a.wrapping_div(b),
        _ => return sim_err!("no ALU operation for nibble {nibble:#x}"),
    })
}

// Sign-extend the low `width` bytes of `v` to a full signed 64-bit value.
fn sign_extend(v: u64, width: usize) -> i64 {
    let shift = 64 - width as u32 * 8;
    ((v << shift) as i64) >> shift
}
