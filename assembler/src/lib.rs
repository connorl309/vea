//! Assembler for the project ISA.
//!
//! The layout is deliberately boring so that adding an instruction is a
//! one-line edit and nothing else has to change:
//!
//!   isa/opcodes.rs    - the instruction table (mnemonic, opcode, form, flags)
//!   isa/registers.rs  - register names and numbers
//!   isa/format.rs     - operand shapes
//!   isa/framing.rs    - how the bytes of one instruction are laid out
//!
//! Everything else (parser, encoder) reads those tables and has no opinion of
//! its own about the ISA.

#[macro_use]
pub mod err;

pub mod isa;

pub mod ast;
pub mod parser;
pub mod encode;
pub mod assemble;
pub mod image;

pub use assemble::{Object, assemble};
pub use err::Result;
