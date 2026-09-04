// Source text -> a flat byte image starting at address 0.
//
// One pass, instruction by instruction:
//   - a label records the current address
//   - an instruction is encoded straight away; if it references a label the
//     immediate is left as 8 zero bytes and a fixup is queued
//   - between instructions we pad to 2-byte alignment
//
// Then a second short walk over the queued fixups writes each label's
// address (big-endian, 64-bit) into the slot that was reserved for it.

use std::collections::HashMap;

use crate::encode;
use crate::err::{Result, at};
use crate::isa::framing::{INSTR_ALIGN, OPCODE_PAD_NOP};
use crate::parser;

pub struct Object {
    pub bytes: Vec<u8>,
    /// label -> address, sorted by address
    pub symbols: Vec<(String, u64)>,
}

struct PendingFixup {
    /// absolute offset of the immediate slot in `bytes`
    at: usize,
    symbol: String,
    line: usize,
}

pub fn assemble(src: &str) -> Result<Object> {
    let program = parser::parse(src)?;

    let mut bytes: Vec<u8> = Vec::new();
    let mut labels: HashMap<String, u64> = HashMap::new();
    let mut fixups: Vec<PendingFixup> = Vec::new();

    for item in &program.items {
        match &item.kind {
            crate::ast::ItemKind::Label(name) => {
                let addr = align_up(bytes.len());
                if labels.insert(name.clone(), addr as u64).is_some() {
                    return Err(at(item.line, format!("duplicate label `{name}`")));
                }
            }
            crate::ast::ItemKind::Instr(ins) => {
                pad_to_align(&mut bytes);
                let enc = encode::build(ins).map_err(|e| at(item.line, e))?;
                let base = bytes.len();
                if let Some(f) = enc.fixup {
                    fixups.push(PendingFixup {
                        at: base + f.at,
                        symbol: f.symbol,
                        line: item.line,
                    });
                }
                bytes.extend(enc.bytes);
            }
        }
    }

    // backfill label addresses
    for f in fixups {
        let addr = labels
            .get(&f.symbol)
            .ok_or_else(|| at(f.line, format!("undefined label `{}`", f.symbol)))?;
        bytes[f.at..f.at + encode::ADDR_WIDTH].copy_from_slice(&addr.to_be_bytes());
    }

    let mut symbols: Vec<(String, u64)> = labels.into_iter().collect();
    symbols.sort_by_key(|(_, addr)| *addr);

    Ok(Object { bytes, symbols })
}

fn align_up(n: usize) -> usize {
    let a = INSTR_ALIGN as usize;
    n.div_ceil(a) * a
}

fn pad_to_align(bytes: &mut Vec<u8>) {
    while bytes.len() as u64 % INSTR_ALIGN != 0 {
        bytes.push(OPCODE_PAD_NOP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asm(src: &str) -> Vec<u8> {
        assemble(src).expect("assembles").bytes
    }

    #[test]
    fn nop() {
        assert_eq!(asm("nop"), vec![0x00, 0x00]);
    }

    #[test]
    fn addi_hand_check() {
        // addi r1, r0, 0x100  ->  20 40 01 00 01 00
        assert_eq!(asm("addi r1, r0, 0x100"), vec![0x20, 0x40, 0x01, 0x00, 0x01, 0x00]);
    }

    #[test]
    fn immediate_width_follows_value() {
        assert_eq!(asm("addi r1, r0, 5"), vec![0x20, 0x30, 0x01, 0x00, 0x05]);
        assert_eq!(asm("addi r1, r0, 0"), vec![0x20, 0x20, 0x01, 0x00]);
    }

    #[test]
    fn alignment_pad_between_odd_frames() {
        // add r1,r2,r3 = 5 bytes, then a pad byte, then nop
        assert_eq!(
            asm("add r1, r2, r3\nnop"),
            vec![0x10, 0x30, 0x01, 0x02, 0x03, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn label_address_is_backfilled_64bit() {
        // start: nop           -> 00 00           at 0
        // loop:  jmp loop      -> 48 80 <8 bytes> at 2, loop = 2
        let out = asm("start:\n  nop\nloop:\n  jmp loop\n");
        assert_eq!(&out[..2], &[0x00, 0x00]);
        assert_eq!(&out[2..4], &[0x48, 0x80]);
        assert_eq!(&out[4..12], &[0, 0, 0, 0, 0, 0, 0, 2]);
    }

    #[test]
    fn mem_operand() {
        assert_eq!(asm("ld r1, [r2]"), vec![0x50, 0x20, 0x01, 0x02]);
        assert_eq!(asm("ld r1, [r2 + 4]"), vec![0x50, 0x30, 0x01, 0x02, 0x04]);
    }

    #[test]
    fn undefined_label_is_an_error() {
        assert!(assemble("jmp nowhere").is_err());
    }
}
