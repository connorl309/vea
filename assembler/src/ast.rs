// The parse tree. One `Item` per source statement: a label alone on its
// line, or an instruction. That's the whole grammar.

pub type Line = usize;

#[derive(Debug)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct Item {
    pub line: Line,
    pub kind: ItemKind,
}

#[derive(Debug)]
pub enum ItemKind {
    Label(String),
    Instr(Instr),
}

#[derive(Debug)]
pub struct Instr {
    pub mnemonic: String,
    pub operands: Vec<Operand>,
}

#[derive(Debug)]
pub enum Operand {
    // `r0`..`r31`
    Reg(u8),
    // a literal: `42`, `0x2a`, `-1`, `'A'`
    Int(i128),
    // a bare name, resolved to a label address later
    Sym(String),
    // `[rb]`, `[rb + disp]`, `[rb - disp]` - displacement is always a literal
    Mem { base: u8, disp: i128 },
}
