// The instruction table
//
// Columns:
//   mnemonic  what you type
//   opcode    the opcode byte
//   form      operand shape (isa/format.rs) - also says if there's an immediate
//   flags     the FLAGS nibble to emit; lets `ldb` / `ldbu` share an opcode
//   writes    which payload register slot (0-based, source order) the result
//             is written back to, or None if this instruction writes no
//             register. Every other populated slot is implicitly a read.
//             There's no instruction here that both reads and writes the
//             same slot, so this one fact is enough to derive rd/rs1/rs2
//             read/write control signals for the whole table.
//   summary   brief description
//
// Immediate width is not declared here; it's inferred from the value.

use super::format::Form;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstrDef {
    pub mnemonic: &'static str,
    pub opcode: u8,
    pub form: Form,
    pub flags: u8,
    pub writes: Option<u8>,
    pub summary: &'static str,
}

#[rustfmt::skip]
pub static INSTRUCTIONS: &[InstrDef] = &[
    row("nop",   0x00, Form::Nullary, 0, None,    "do nothing"),

    // moves / constants
    row("mov",   0x02, Form::RR,      0, Some(0), "rd = rs"),
    row("movi",  0x03, Form::RI,      0, Some(0), "rd = imm"),

    // integer ALU, register form
    row("add",   0x10, Form::RRR,     0, Some(0), "rd = rs1 + rs2"),
    row("sub",   0x11, Form::RRR,     0, Some(0), "rd = rs1 - rs2"),
    row("and",   0x12, Form::RRR,     0, Some(0), "rd = rs1 & rs2"),
    row("or",    0x13, Form::RRR,     0, Some(0), "rd = rs1 | rs2"),
    row("xor",   0x14, Form::RRR,     0, Some(0), "rd = rs1 ^ rs2"),
    row("nor",   0x15, Form::RRR,     0, Some(0), "rd = ~(rs1 | rs2)"),
    row("shl",   0x16, Form::RRR,     0, Some(0), "rd = rs1 << rs2"),
    row("shr",   0x17, Form::RRR,     0, Some(0), "rd = rs1 >> rs2 (logical)"),
    row("sar",   0x18, Form::RRR,     0, Some(0), "rd = rs1 >> rs2 (arithmetic)"),

    // integer ALU, immediate form
    row("addi",  0x20, Form::RRI,     0, Some(0), "rd = rs + imm"),
    row("andi",  0x21, Form::RRI,     0, Some(0), "rd = rs & imm"),
    row("ori",   0x22, Form::RRI,     0, Some(0), "rd = rs | imm"),
    row("xori",  0x23, Form::RRI,     0, Some(0), "rd = rs ^ imm"),
    row("shli",  0x24, Form::RRI,     0, Some(0), "rd = rs << imm"),
    row("shri",  0x25, Form::RRI,     0, Some(0), "rd = rs >> imm (logical)"),
    row("sari",  0x26, Form::RRI,     0, Some(0), "rd = rs >> imm (arithmetic)"),

    // set-if-predicate: rd = (rs1 OP rs2) ? 1 : 0
    row("slt",   0x30, Form::RRR,     0, Some(0), "rd = rs1 < rs2  (signed)"),
    row("sltu",  0x31, Form::RRR,     0, Some(0), "rd = rs1 < rs2  (unsigned)"),
    row("seq",   0x32, Form::RRR,     0, Some(0), "rd = rs1 == rs2"),
    row("sne",   0x33, Form::RRR,     0, Some(0), "rd = rs1 != rs2"),
    row("sle",   0x34, Form::RRR,     0, Some(0), "rd = rs1 <= rs2 (signed)"),
    row("sleu",  0x35, Form::RRR,     0, Some(0), "rd = rs1 <= rs2 (unsigned)"),
    row("sge",   0x36, Form::RRR,     0, Some(0), "rd = rs1 >= rs2 (signed)"),
    row("sgeu",  0x37, Form::RRR,     0, Some(0), "rd = rs1 >= rs2 (unsigned)"),

    // compare-and-branch: if (rs1 OP rs2) pc = target
    row("beq",   0x40, Form::RRI,     0, None,    "branch if rs1 == rs2"),
    row("bne",   0x41, Form::RRI,     0, None,    "branch if rs1 != rs2"),
    row("blt",   0x42, Form::RRI,     0, None,    "branch if rs1 < rs2  (signed)"),
    row("bltu",  0x43, Form::RRI,     0, None,    "branch if rs1 < rs2  (unsigned)"),
    row("bge",   0x44, Form::RRI,     0, None,    "branch if rs1 >= rs2 (signed)"),
    row("bgeu",  0x45, Form::RRI,     0, None,    "branch if rs1 >= rs2 (unsigned)"),

    // unconditional control flow
    row("jmp",   0x48, Form::I,       0, None,    "pc = target"),
    row("jr",    0x49, Form::R,       0, None,    "pc = rd"),
    row("call",  0x4A, Form::I,       0, None,    "push return addr; pc = target"),
    row("callr", 0x4B, Form::R,       0, None,    "push return addr; pc = rd"),
    row("ret",   0x4C, Form::Nullary, 0, None,    "pop return addr into pc"),

    // load / store: address = rb + signed disp
    row("ld",    0x50, Form::RMem,    0, Some(0), "rd = mem64[rb + disp]"),
    row("ldw",   0x51, Form::RMem,    1, Some(0), "rd = sext(mem32[rb + disp])"),
    row("ldwu",  0x51, Form::RMem,    0, Some(0), "rd = zext(mem32[rb + disp])"),
    row("ldh",   0x52, Form::RMem,    1, Some(0), "rd = sext(mem16[rb + disp])"),
    row("ldhu",  0x52, Form::RMem,    0, Some(0), "rd = zext(mem16[rb + disp])"),
    row("ldb",   0x53, Form::RMem,    1, Some(0), "rd = sext(mem8[rb + disp])"),
    row("ldbu",  0x53, Form::RMem,    0, Some(0), "rd = zext(mem8[rb + disp])"),
    row("st",    0x54, Form::RMem,    0, None,    "mem64[rb + disp] = rs"),
    row("stw",   0x55, Form::RMem,    0, None,    "mem32[rb + disp] = rs"),
    row("sth",   0x56, Form::RMem,    0, None,    "mem16[rb + disp] = rs"),
    row("stb",   0x57, Form::RMem,    0, None,    "mem8[rb + disp] = rs"),

    // system
    row("halt",  0xFF, Form::Nullary, 0, None,    "stop the machine"),
];

const fn row(mnemonic: &'static str, opcode: u8, form: Form, flags: u8, writes: Option<u8>, summary: &'static str) -> InstrDef {
    InstrDef { mnemonic, opcode, form, flags, writes, summary }
}

// Look up an instruction by the mnemonic the user typed (case-insensitive).
pub fn by_mnemonic(m: &str) -> Option<&'static InstrDef> {
    let m = m.to_ascii_lowercase();
    INSTRUCTIONS.iter().find(|i| i.mnemonic == m)
}

// Look up by encoded (opcode, flags). Handy for disassembly and tests.
pub fn by_opcode(opcode: u8, flags: u8) -> Option<&'static InstrDef> {
    INSTRUCTIONS.iter().find(|i| i.opcode == opcode && i.flags == flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mnemonics_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for i in INSTRUCTIONS {
            assert!(seen.insert(i.mnemonic), "duplicate mnemonic {}", i.mnemonic);
            assert!(i.flags <= 0x0F, "{} flags out of nibble range", i.mnemonic);
            if let Some(w) = i.writes {
                assert!((w as u32) < i.form.reg_count() as u32, "{} writes out-of-range slot {w}", i.mnemonic);
            }
        }
    }

    #[test]
    fn shared_opcodes_have_distinct_flags() {
        // Same opcode is fine (ldb/ldbu) as long as flags differ.
        for (n, a) in INSTRUCTIONS.iter().enumerate() {
            for b in &INSTRUCTIONS[n + 1..] {
                if a.opcode == b.opcode {
                    assert_ne!(a.flags, b.flags, "{} and {} collide", a.mnemonic, b.mnemonic);
                }
            }
        }
    }
}