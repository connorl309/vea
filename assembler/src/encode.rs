// Turn one parsed instruction into bytes.
//
// A literal immediate is sized here from its value. A label reference can't
// be - the address isn't known yet - so it gets an 8-byte zero placeholder
// and a `Fixup` telling the assembler where to write the address later.
// Label immediates are always 64-bit.

use crate::ast::{Instr, Operand};
use crate::err::Result;
use crate::isa::format::Form;
use crate::isa::{framing, opcodes};

/// Width, in bytes, of a label address once backfilled.
pub const ADDR_WIDTH: usize = 8;

pub struct Encoded {
    pub bytes: Vec<u8>,
    /// Set when the instruction references a label.
    pub fixup: Option<Fixup>,
}

pub struct Fixup {
    /// Offset of the immediate field within `bytes`.
    pub at: usize,
    pub symbol: String,
}

/// Smallest byte count that can hold `v`, read as either signed or unsigned:
/// 0, 1, 2, 4, or 8. Returns 16 for values that don't fit in 64 bits.
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

pub fn build(ins: &Instr) -> Result<Encoded> {
    let def = opcodes::by_mnemonic(&ins.mnemonic)
        .ok_or_else(|| asm_err!("unknown instruction `{}`", ins.mnemonic))?;

    let (regs, imm) = operands_for(def.form, &ins.operands)?;

    let (imm_bytes, symbol) = match imm {
        ImmArg::None => (Vec::new(), None),
        ImmArg::Int(v) => {
            let w = min_width(v);
            if w > 8 {
                return Err(asm_err!("immediate {v} does not fit in 64 bits"));
            }
            (be_bytes(v, w), None)
        }
        ImmArg::Sym(name) => (vec![0u8; ADDR_WIDTH], Some(name)),
    };

    let bytes = framing::frame(def.opcode, def.flags, &regs, &imm_bytes)?;
    let fixup = symbol.map(|symbol| Fixup { at: 2 + regs.len(), symbol });
    Ok(Encoded { bytes, fixup })
}

enum ImmArg {
    None,
    Int(i128),
    Sym(String),
}

/// Match operands against the form: (register bytes, immediate argument).
fn operands_for(form: Form, ops: &[Operand]) -> Result<(Vec<u8>, ImmArg)> {
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

    let imm = |o: &Operand| -> Result<ImmArg> {
        match o {
            Operand::Int(v) => Ok(ImmArg::Int(*v)),
            Operand::Sym(s) => Ok(ImmArg::Sym(s.clone())),
            _ => Err(asm_err!("expected an immediate or label")),
        }
    };

    use Form::*;
    Ok(match form {
        Nullary => {
            want(0)?;
            (vec![], ImmArg::None)
        }
        R => {
            want(1)?;
            (vec![reg(&ops[0])?], ImmArg::None)
        }
        RR => {
            want(2)?;
            (vec![reg(&ops[0])?, reg(&ops[1])?], ImmArg::None)
        }
        RRR => {
            want(3)?;
            (vec![reg(&ops[0])?, reg(&ops[1])?, reg(&ops[2])?], ImmArg::None)
        }
        RI => {
            want(2)?;
            (vec![reg(&ops[0])?], imm(&ops[1])?)
        }
        RRI => {
            want(3)?;
            (vec![reg(&ops[0])?, reg(&ops[1])?], imm(&ops[2])?)
        }
        I => {
            want(1)?;
            (vec![], imm(&ops[0])?)
        }
        RMem => {
            want(2)?;
            let rd = reg(&ops[0])?;
            match &ops[1] {
                Operand::Mem { base, disp } => (vec![rd, *base], ImmArg::Int(*disp)),
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
    fn label_ref_reserves_eight_bytes() {
        let e = build(&Instr {
            mnemonic: "jmp".into(),
            operands: vec![Operand::Sym("target".into())],
        })
        .unwrap();
        // opcode + lenbyte + 8 placeholder bytes
        assert_eq!(e.bytes.len(), 2 + ADDR_WIDTH);
        let f = e.fixup.unwrap();
        assert_eq!(f.at, 2);
        assert_eq!(f.symbol, "target");
    }
}
