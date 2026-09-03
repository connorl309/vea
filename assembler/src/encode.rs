//! Turn one parsed instruction into bytes. Immediate width is chosen here, by
//! looking at the value - never declared in the ISA table.

use std::collections::HashMap;

use crate::ast::{Instr, Operand};
use crate::err::Result;
use crate::isa::format::Form;
use crate::isa::{framing, opcodes};

/// Symbol table: label name -> address.
pub type Symbols = HashMap<String, i128>;

/// Smallest byte count that can hold `v`, read as either signed or unsigned:
/// 0, 1, 2, 4, or 8. Returns 16 for values that don't fit in 64 bits so the
/// caller can reject them.
pub fn min_width(v: i128) -> u8 {
    if v == 0 {
        return 0;
    }
    for w in [1u8, 2, 4, 8] {
        let bits = w as u32 * 8;
        let lo = -(1i128 << (bits - 1));
        let hi = (1i128 << bits) - 1;
        if (lo..=hi).contains(&v) {
            return w;
        }
    }
    16
}

fn be_bytes(v: i128, width: u8) -> Vec<u8> {
    v.to_be_bytes()[16 - width as usize..].to_vec()
}

/// Full frame for `ins`. `syms` must already hold every referenced label.
pub fn build(ins: &Instr, syms: &Symbols) -> Result<Vec<u8>> {
    let def = opcodes::by_mnemonic(&ins.mnemonic)
        .ok_or_else(|| asm_err!("unknown instruction `{}`", ins.mnemonic))?;

    let (regs, imm) = operands_for(def.form, &ins.operands, syms)?;

    let imm_bytes = match imm {
        None => Vec::new(),
        Some(v) => {
            let w = min_width(v);
            if w > 8 {
                return Err(asm_err!("immediate {v} does not fit in 64 bits"));
            }
            be_bytes(v, w)
        }
    };

    framing::frame(def.opcode, def.flags, &regs, &imm_bytes)
}

/// Encoded length of `ins` in bytes (no alignment padding).
pub fn size(ins: &Instr, syms: &Symbols) -> Result<usize> {
    Ok(build(ins, syms)?.len())
}

/// Match operands against the form, returning (register bytes, immediate value).
fn operands_for(form: Form, ops: &[Operand], syms: &Symbols) -> Result<(Vec<u8>, Option<i128>)> {
    let want = |n: usize| -> Result<()> {
        if ops.len() == n {
            Ok(())
        } else {
            Err(asm_err!("expected {n} operand(s), got {}", ops.len()))
        }
    };

    let reg = |o: &Operand| -> Result<u8> {
        match o {
            Operand::Reg(r) => Ok(*r),
            _ => Err(asm_err!("expected a register")),
        }
    };

    let imm = |o: &Operand| -> Result<i128> {
        match o {
            Operand::Int(v) => Ok(*v),
            Operand::Sym(s) => syms
                .get(s)
                .copied()
                .ok_or_else(|| asm_err!("undefined symbol `{s}`")),
            _ => Err(asm_err!("expected an immediate")),
        }
    };

    use Form::*;
    Ok(match form {
        Nullary => {
            want(0)?;
            (vec![], None)
        }
        R => {
            want(1)?;
            (vec![reg(&ops[0])?], None)
        }
        RR => {
            want(2)?;
            (vec![reg(&ops[0])?, reg(&ops[1])?], None)
        }
        RRR => {
            want(3)?;
            (vec![reg(&ops[0])?, reg(&ops[1])?, reg(&ops[2])?], None)
        }
        RI => {
            want(2)?;
            (vec![reg(&ops[0])?], Some(imm(&ops[1])?))
        }
        RRI => {
            want(3)?;
            (vec![reg(&ops[0])?, reg(&ops[1])?], Some(imm(&ops[2])?))
        }
        I => {
            want(1)?;
            (vec![], Some(imm(&ops[0])?))
        }
        RMem => {
            want(2)?;
            let rd = reg(&ops[0])?;
            match &ops[1] {
                Operand::Mem { base, disp } => (vec![rd, *base], Some(*disp)),
                _ => return Err(asm_err!("expected `[rb + disp]`")),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_by_value() {
        assert_eq!(min_width(0), 0);
        assert_eq!(min_width(5), 1);
        assert_eq!(min_width(255), 1);
        assert_eq!(min_width(256), 2);
        assert_eq!(min_width(-1), 1);
        assert_eq!(min_width(-129), 2);
        assert_eq!(min_width(0xFFFF_FFFF), 4);
        assert_eq!(min_width(0x1_0000_0000), 8);
    }

    #[test]
    fn big_endian_truncation() {
        assert_eq!(be_bytes(0x0100, 2), vec![0x01, 0x00]);
        assert_eq!(be_bytes(-1, 1), vec![0xFF]);
    }
}
