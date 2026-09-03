//! The machine definition. Everything the assembler knows about the ISA lives
//! under here; the rest of the crate treats it as read-only.

pub mod format;
pub mod framing;
pub mod opcodes;
pub mod registers;

pub use format::Form;
pub use opcodes::{INSTRUCTIONS, InstrDef};

/// One-line-per-instruction dump for `asm --list-isa`.
pub fn listing() -> String {
    let mut out = String::new();
    for i in INSTRUCTIONS {
        out.push_str(&format!(
            "{:<6} {:#04x}  {:<8} flags={:#03x}  {}\n",
            i.mnemonic,
            i.opcode,
            format!("{:?}", i.form),
            i.flags,
            i.summary,
        ));
    }
    out
}