// assembler/mod.rs
/**
 * The assembler submodule is responsible for parsing and assembling input
 * source files into their bytecode representation. It enforces syntax and
 * should remove the possibility of non-sim-modified valid programs to not
 * crash the simulation unexpectedly.
*/

/*

                            INSTRUCTION FORMAT

VEA is ***Big Endian***. This means everything will read left-to-right as
is the case with English.

Every instruction in VEA is somewhat-variable-length. The instruction bytes
shall encode the full instruction length, reserved flag bits that impact
per-instruction behavior, register destinations/sources, and possible immediates.
Instructions using immediates shall be truncated as much as possible to keep
byte count small if possible.

This means that, for example, an immediate add with a value of 0x5 should only
be assembled to denote 1 byte of immediate follows, in which case the architecture
(when performing work on this value) will expand immediates into their "corrected"
64-bit equivalent internally. This is so we don't have to worry about size changes
all over structures.

The general format is as follows, listed in big endian byte order:

[   opcode byte    ][   opinfo   ][  rd/rb/imm*  ][  rs1/imm*  ][   rs2/imm*   ]

opinfo is special. In all instructions it is composing the byte of [   length[4] | flags[4]    ].
The length is the size of the whole instruction in bytes. It includes the opcode and opinfo bytes.
It does not include the alignment padding.
Other interpretations are left open as this project develops.

The bytes marked with * are optional and presence depends on the instruction itself. For example, a
raw JMP instruction to some hardcoded 64-bit address would take up more byte space than a CALL <reg>,
while a register base + displacement store could use ALL possible bytes.

*/

use crate::isa::*;
use std::collections::HashMap;

// Branch variants to keep opcode bloat low.
pub const BR_ALWAYS: u8 = 0x0; // b     : unconditional
pub const BR_EQ: u8     = 0x1; // beq   : Z
pub const BR_NE: u8     = 0x2; // bne   : !Z
pub const BR_LT: u8     = 0x3; // blt   : N != V
pub const BR_GE: u8     = 0x4; // bgte  : N == V
pub const BR_GT: u8     = 0x5; // bgt   : !Z && (N == V)
pub const BR_LE: u8     = 0x6; // blte  : Z || (N != V)
// Branch flags[4]. Bits [2:0] are the predicate above. Bit 3 is set if the
// target is an immediate. It is clear if the target is a register.
pub const BR_MASK: u8 = 0b0111;
pub const BR_IMM: u8  = 0b1000;

pub const OPINFO_FLAG_ALSO_IMMEDIATE: u8 = 0b0001;
// Load/store flags[4]. Bit 0 is OPINFO_FLAG_ALSO_IMMEDIATE: set => [base + disp],
// clear => [base + index reg]. Size is bits [2:1] and defaults to 8 bytes.
pub const LS_SIZE_D: u8 = 0b0000; // 8 bytes (default, no suffix)
pub const LS_SIZE_B: u8 = 0b0010; // 1 byte
pub const LS_SIZE_H: u8 = 0b0100; // 2 bytes
pub const LS_SIZE_W: u8 = 0b0110; // 4 bytes
pub const LS_SEXT:   u8 = 0b1000; // sign-extend the loaded value (narrow loads)
// Encodings (big-endian bytes; store keeps the value in the first register slot):
//   ld    r5, [r6]         -> 40 41 05 06       ; default 8-byte, disp 0
//   ld.w  r1, [r2 + 0x10]  -> 40 57 01 02 10    ; 4-byte access, 1-byte displacement
//   ld.sb r3, [r4 + r7]    -> 40 5A 03 04 07    ; indexed, sign-extended byte
//   st    [r8 - 8], r9     -> 41 51 09 08 F8    ; 8-byte store, disp -8

#[derive(Debug, Clone, Copy)]
enum Op {
    NULL, // NOP and HALT
    R__OR__I,
    R_R__OR__R_I,
    R_R_R__OR__R_R_IMM, // depending on flags, can be either r = r OP r, *or* r = r OP imm
}

// The basic instruction format struct.
#[derive(Debug, Clone)]
pub struct InstructionFormat {
    mnemonic: &'static str,
    opcode: u8,
    inst_type: Op,
    flags: u8
}
// Quick helper for making new instructions...
const fn row(mnemonic: &'static str, opcode: u8, inst_type: Op, flags: u8) -> InstructionFormat {
    InstructionFormat { mnemonic, opcode, inst_type, flags }
}

// Our actual list of instruction formats
pub const INSTRUCTIONS: &[InstructionFormat] = &[
    // Processor level stuff
    row("nop",  0x00, Op::NULL, 0),
    row("trap", 0xFE, Op::NULL, 0), // CPU exception vector in payload byte
    row("halt", 0xFF, Op::NULL, 0),

    // Move
    // rd = rs, OR
    // rd = imm
    row("mov",  0x01,   Op::R_R__OR__R_I, 0),

    // ALU
    // rd = a OP b, where
    // a = rs1
    // b = rs2 or imm
    row("add",  0x10,   Op::R_R_R__OR__R_R_IMM, 0),
    row("sub",  0x11,   Op::R_R_R__OR__R_R_IMM, 0),
    row("and",  0x12,   Op::R_R_R__OR__R_R_IMM, 0),
    row("or",   0x13,   Op::R_R_R__OR__R_R_IMM, 0),
    row("not",  0x14,   Op::R_R__OR__R_I,       0),
    row("xor",  0x15,   Op::R_R_R__OR__R_R_IMM, 0),
    row("shl",  0x16,   Op::R_R_R__OR__R_R_IMM, 0),
    row("shr",  0x17,   Op::R_R_R__OR__R_R_IMM, 0),
    row("sar",  0x18,   Op::R_R_R__OR__R_R_IMM, 0),
    row("mul",  0x19,   Op::R_R_R__OR__R_R_IMM, 0),
    row("div",  0x1A,   Op::R_R_R__OR__R_R_IMM, 0),

    // Comparisons
    // Set condition codes based on variant used.
    row("cmp",  0x20,   Op::R_R__OR__R_I,   0),
    row("cmp.s",0x21,   Op::R_R__OR__R_I,   0),

    // Branch and its combinations
    // We encode the variant into the 4 flag bits to reduce opcode bloat.
    // An immediate target is a signed offset from the branch address itself,
    // at most 32 bits wide, sign extended to 64 bits by the machine.
    row("b",    0x30,   Op::R__OR__I, BR_ALWAYS),
    row("beq",  0x30,   Op::R__OR__I, BR_EQ),
    row("bne",  0x30,   Op::R__OR__I, BR_NE),
    row("blt",  0x30,   Op::R__OR__I, BR_LT),
    row("bgt",  0x30,   Op::R__OR__I, BR_GT),
    row("ble",  0x30,   Op::R__OR__I, BR_LE),
    row("bge",  0x30,   Op::R__OR__I, BR_GE),
    // Jump is separate from branch in that jump will always perform an *absolute*
    // move to the PC as opposed to a relative one.
    row("jmp",  0x31,   Op::R__OR__I, 0),

    // Memory accesses
    // size + sign-extend + addressing mode live in
    // flags[4]. Operands are (reg, base, index-or-disp); `reg` is the loaded
    // dest for ld and the stored value for st. Bare ld/st move 8 bytes.
    row("ld",    0x40,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_D),          // rd  <- mem[base + idx/disp]
    row("ld.b",  0x40,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_B),
    row("ld.h",  0x40,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_H),
    row("ld.w",  0x40,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_W),
    row("ld.sb", 0x40,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_B | LS_SEXT),
    row("ld.sh", 0x40,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_H | LS_SEXT),
    row("ld.sw", 0x40,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_W | LS_SEXT),
    row("st",    0x41,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_D),          // mem[base + idx/disp] <- rs
    row("st.b",  0x41,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_B),
    row("st.h",  0x41,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_H),
    row("st.w",  0x41,  Op::R_R_R__OR__R_R_IMM, LS_SIZE_W),
];

/*
==========================================================

    ASSEMBLER

    Parse every line, then relax label widths to a fixed
    point so each address is known, then emit bytes. Every
    instruction is padded with zeroes to the next ALIGNMENT
    boundary so a flat image matches the fetch rule
    PC = round_up(PC + length, ALIGNMENT).

    Label references are encoded as the shortest signed
    immediate that fits. A b* target is relative to the
    address of the branch itself and is capped at a 32 bit
    signed offset. jmp and other absolute references are
    capped at 64 bits. The machine sign extends the stored
    bytes back to 64 bits at execution time.

==========================================================
*/

// Largest signed immediate width in bytes for each reference kind.
const REL_MAX: usize = 4;
const ABS_MAX: usize = 8;

// An operand once parsed. Sym is a label whose width grows during relaxation.
enum Slot {
    Reg(u8),
    Num(i64),
    Sym { name: String, width: usize },
}

// One source instruction with the labels that point at it and its address.
struct Insn {
    line: usize,
    text: String,
    fmt: &'static InstructionFormat,
    slots: Vec<Slot>,
    labels: Vec<String>,
    addr: u64,
}

// Byte cost of a slot. Must stay in lockstep with encode().
fn slot_size(s: &Slot) -> usize {
    match s {
        Slot::Reg(_) => 1,
        Slot::Num(v) => imm_bytes(*v).len(),
        Slot::Sym { width, .. } => *width,
    }
}

// Shortest signed width in bytes that holds a resolved label value. The width is
// at least one byte. It is never more than the cap of the kind.
fn sym_width(value: i64, relative: bool) -> Result<usize, String> {
    let cap = if relative { REL_MAX } else { ABS_MAX };
    let n = imm_bytes(value).len().max(1);
    if n > cap {
        let kind = if relative { "relative branch offset" } else { "absolute address" };
        return Err(format!("{kind} {value} does not fit in {cap} signed bytes"));
    }
    Ok(n)
}

// One assembled instruction: where it sits in the flat image, the bytes it
// encoded to (no alignment padding), and the source text it came from. The TUI
// renders these as the disassembly / source view.
#[derive(Debug, Clone)]
pub struct ListingRow {
    pub addr: u64,
    pub bytes: Vec<u8>,
    pub text: String,
}

// Assemble source text into a flat load ready image plus a structured listing,
// one row per instruction.
pub fn assemble_listing(src: &str) -> Result<(Vec<u8>, Vec<ListingRow>), String> {
    let (insns, syms) = lower(src)?;
    let mut image = Vec::new();
    let mut rows = Vec::new();
    for insn in &insns {
        let bytes = encode(insn, &syms).map_err(|e| format!("line {}: {e}", insn.line))?;
        image.extend_from_slice(&bytes);
        let pad = align_up(bytes.len() as u64, ALIGNMENT) as usize - bytes.len();
        image.resize(image.len() + pad, 0);
        rows.push(ListingRow { addr: insn.addr, bytes, text: insn.text.clone() });
    }
    Ok((image, rows))
}

// Assemble source text into a flat load ready image.
// Also returns a one line per instruction listing for debug output.
pub fn assemble(src: &str) -> Result<(Vec<u8>, Vec<String>), String> {
    let (image, rows) = assemble_listing(src)?;
    let listing = rows
        .iter()
        .map(|r| format!("{:08x}  {:<23}  {}", r.addr, hex(&r.bytes), r.text))
        .collect();
    Ok((image, listing))
}

// Assemble and hand back each instruction's own bytes with no alignment padding.
pub fn assemble_raw(src: &str) -> Result<Vec<Vec<u8>>, String> {
    let (insns, syms) = lower(src)?;
    insns
        .iter()
        .map(|insn| encode(insn, &syms).map_err(|e| format!("line {}: {e}", insn.line)))
        .collect()
}

// Parse every line into instructions tagged with the labels that precede them.
fn lower(src: &str) -> Result<(Vec<Insn>, HashMap<String, u64>), String> {
    let mut insns: Vec<Insn> = Vec::new();
    let mut pending: Vec<String> = Vec::new();

    for (i, raw) in src.lines().enumerate() {
        let ln = i + 1;
        let mut rest = strip_comment(raw).trim();
        while let Some(c) = rest.find(':') {
            let name = rest[..c].trim();
            if !is_ident(name) {
                break;
            }
            pending.push(name.to_string());
            rest = rest[c + 1..].trim();
        }
        if rest.is_empty() {
            continue;
        }
        let mut insn = parse_insn(ln, rest)?;
        insn.labels = std::mem::take(&mut pending);
        insns.push(insn);
    }
    let trailing = pending;

    // Relax label widths until addresses stop moving. Widths only ever grow, so
    // this reaches a fixed point in a couple of rounds.
    let mut syms = HashMap::new();
    for _ in 0..insns.len() + 2 {
        syms.clear();
        let mut addr = 0u64;
        for insn in &mut insns {
            for l in &insn.labels {
                if syms.insert(l.clone(), addr).is_some() {
                    return Err(format!("line {}: duplicate label '{l}'", insn.line));
                }
            }
            insn.addr = addr;
            let size = 2 + insn.slots.iter().map(slot_size).sum::<usize>();
            addr += align_up(size as u64, ALIGNMENT);
        }
        for l in &trailing {
            syms.insert(l.clone(), addr);
        }

        let mut grew = false;
        for insn in &mut insns {
            let relative = insn.fmt.opcode == 0x30;
            let base = insn.addr as i64;
            let line = insn.line;
            for slot in &mut insn.slots {
                let Slot::Sym { name, width } = slot else { continue };
                let target = *syms
                    .get(name)
                    .ok_or(format!("line {line}: unknown label '{name}'"))? as i64;
                let value = if relative { target - base } else { target };
                let need = sym_width(value, relative).map_err(|e| format!("line {line}: {e}"))?;
                if need > *width {
                    *width = need;
                    grew = true;
                }
            }
        }
        if !grew {
            return Ok((insns, syms));
        }
    }
    Err("label layout failed to converge".into())
}

fn parse_insn(ln: usize, text: &str) -> Result<Insn, String> {
    let (mn, tail) = match text.split_once(char::is_whitespace) {
        Some((a, b)) => (a, b.trim()),
        None => (text, ""),
    };
    let fmt = find(mn).ok_or(format!("line {ln}: unknown mnemonic '{mn}'"))?;
    let args: Vec<&str> = if tail.is_empty() { Vec::new() } else { split_args(tail) };
    let slots = build_slots(fmt, &args).map_err(|e| format!("line {ln}: {e}"))?;
    Ok(Insn { line: ln, text: text.to_string(), fmt, slots, labels: Vec::new(), addr: 0 })
}

// Map a mnemonic and its operand strings onto the slot list its format expects.
fn build_slots(fmt: &InstructionFormat, args: &[&str]) -> Result<Vec<Slot>, String> {
    let need = |k: usize| {
        if args.len() == k {
            Ok(())
        } else {
            Err(format!("{} takes {k} operands, got {}", fmt.mnemonic, args.len()))
        }
    };
    // Loads and stores use bracket syntax and are recognised by opcode.
    match fmt.opcode {
        0x40 => {
            need(2)?;
            let (base, off) = mem(args[1])?;
            return Ok(vec![Slot::Reg(reg(args[0])?), Slot::Reg(base), off]);
        }
        0x41 => {
            need(2)?;
            let (base, off) = mem(args[0])?;
            return Ok(vec![Slot::Reg(reg(args[1])?), Slot::Reg(base), off]);
        }
        _ => {}
    }
    match fmt.inst_type {
        Op::NULL => match (fmt.mnemonic, args.len()) {
            ("trap", 1) => Ok(vec![operand(args[0])?]),
            ("trap", 0) => Ok(Vec::new()),
            ("trap", _) => Err("trap takes 0 or 1 operands".into()),
            _ => {
                need(0)?;
                Ok(Vec::new())
            }
        },
        Op::R__OR__I => {
            need(1)?;
            Ok(vec![operand(args[0])?])
        }
        Op::R_R__OR__R_I => {
            need(2)?;
            Ok(vec![Slot::Reg(reg(args[0])?), operand(args[1])?])
        }
        Op::R_R_R__OR__R_R_IMM => {
            need(3)?;
            Ok(vec![Slot::Reg(reg(args[0])?), Slot::Reg(reg(args[1])?), operand(args[2])?])
        }
    }
}

// Emit the final bytes for one instruction.
fn encode(insn: &Insn, syms: &HashMap<String, u64>) -> Result<Vec<u8>, String> {
    let f = insn.fmt;
    let mark_imm = matches!(f.inst_type, Op::R_R__OR__R_I | Op::R_R_R__OR__R_R_IMM);
    let last = insn.slots.len().saturating_sub(1);
    let mut flags = f.flags & 0xF;
    let mut regs: Vec<u8> = Vec::new();
    let mut imm: Vec<u8> = Vec::new();

    for (i, s) in insn.slots.iter().enumerate() {
        match s {
            Slot::Reg(r) => regs.push(*r),
            Slot::Num(v) => {
                imm.extend(imm_bytes(*v));
                if i == last && mark_imm {
                    flags |= OPINFO_FLAG_ALSO_IMMEDIATE;
                }
            }
            Slot::Sym { name, width } => {
                let target = *syms.get(name).ok_or(format!("unknown label '{name}'"))? as i64;
                // b and its variants are relative to the branch address, jmp is absolute.
                let value = if f.opcode == 0x30 { target - insn.addr as i64 } else { target };
                imm.extend_from_slice(&value.to_be_bytes()[8 - *width..]);
                if i == last && mark_imm {
                    flags |= OPINFO_FLAG_ALSO_IMMEDIATE;
                }
            }
        }
    }
    if matches!(f.inst_type, Op::R__OR__I) && !matches!(insn.slots[0], Slot::Reg(_)) {
        flags |= BR_IMM;
    }
    let total = 2 + regs.len() + imm.len();
    if total > 0xF {
        return Err(format!("instruction of {total} bytes exceeds 15"));
    }
    if f.opcode == 0x30 && imm.len() > REL_MAX {
        return Err("branch offset exceeds a 32 bit signed value".into());
    }
    let mut out = vec![f.opcode, ((total as u8) << 4) | flags];
    out.append(&mut regs);
    out.append(&mut imm);
    Ok(out)
}

// ---------- parsing helpers ----------

fn find(mnemonic: &str) -> Option<&'static InstructionFormat> {
    INSTRUCTIONS.iter().find(|i| i.mnemonic == mnemonic)
}

// A plain (non bracket) operand token.
fn operand(tok: &str) -> Result<Slot, String> {
    let t = tok.trim();
    if let Ok(r) = reg(t) {
        Ok(Slot::Reg(r))
    } else if let Some(v) = imm(t) {
        Ok(Slot::Num(v))
    } else if is_ident(t) {
        Ok(Slot::Sym { name: t.to_string(), width: 1 })
    } else {
        Err(format!("bad operand '{tok}'"))
    }
}

// Parse a [base], [base + disp] or [base + index] address field.
fn mem(field: &str) -> Result<(u8, Slot), String> {
    let inner = field
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or(format!("expected [address], got '{field}'"))?
        .trim();
    match inner.char_indices().find(|&(i, c)| i > 0 && (c == '+' || c == '-')) {
        None => Ok((reg(inner)?, Slot::Num(0))),
        Some((i, sign)) => {
            let base = reg(inner[..i].trim())?;
            let rhs = inner[i + 1..].trim();
            if let Ok(rx) = reg(rhs) {
                if sign == '-' {
                    return Err(format!("cannot subtract an index register in '{field}'"));
                }
                Ok((base, Slot::Reg(rx)))
            } else if let Some(v) = imm(rhs) {
                Ok((base, Slot::Num(if sign == '-' { -v } else { v })))
            } else {
                Err(format!("bad address offset '{rhs}'"))
            }
        }
    }
}

// Only r0..r31 are nameable. PC is architectural and a program cannot touch it,
// so neither "pc" nor r33 (its internal index) resolves here.
fn reg(tok: &str) -> Result<u8, String> {
    let t = tok.trim().to_ascii_lowercase();
    if let Some(n) = t.strip_prefix('r').and_then(|d| d.parse::<u16>().ok()) {
        if (n as usize) < NUM_REGS {
            return Ok(n as u8);
        }
    }
    Err(format!("bad register '{tok}'"))
}

// Numeric literal to a signed value. Hex carries a `0x` prefix, decimal a `#`
// prefix (`mov r1, #10`, `add r1, r2, -#1`); an unprefixed token is not a
// number and is left for label resolution. A leading `-` negates either form,
// and `#-10` is accepted as well as `-#10`.
fn imm(tok: &str) -> Option<i64> {
    let t = tok.trim();
    let (neg, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, t),
    };
    let mag = if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()?
    } else if let Some(dec) = body.strip_prefix('#') {
        dec.parse::<i64>().ok()?
    } else {
        return None;
    };
    Some(if neg { -mag } else { mag })
}

// Fewest big endian bytes that reproduce v under sign extension. Zero uses none.
fn imm_bytes(v: i64) -> Vec<u8> {
    if v == 0 {
        return Vec::new();
    }
    let all = v.to_be_bytes();
    for n in 1..8usize {
        let sh = 64 - n * 8;
        if (v << sh) >> sh == v {
            return all[8 - n..].to_vec();
        }
    }
    all.to_vec()
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(h) if h.is_ascii_alphabetic() || h == '_' || h == '.')
        && s.chars().all(|x| x.is_ascii_alphanumeric() || x == '_' || x == '.')
}

// Comments run to end of line after `;` or `//`. `#` is not a comment marker:
// it introduces a decimal literal (see `imm`).
fn strip_comment(s: &str) -> &str {
    let cut = [s.find(';'), s.find("//")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(s.len());
    &s[..cut]
}

// Split an operand list on commas that sit outside brackets.
fn split_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) / a * a
}

// Space separated lowercase hex, handy for tests and CLI output.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Check every case and report all failures together, so a single bad row
    // never masks the ones after it.
    fn all<T>(cases: &[T], check: impl Fn(&T) -> Result<(), String>) {
        let fails: Vec<String> = cases.iter().filter_map(|c| check(c).err()).collect();
        assert!(fails.is_empty(), "\n{}", fails.join("\n"));
    }

    // Every mnemonic in INSTRUCTIONS, with the operand forms that shift the
    // encoding, and the exact bytes each must produce (no alignment padding).
    const ENCODE: &[(&str, &str)] = &[
        // processor control
        ("nop", "00 20"),
        ("halt", "ff 20"),
        ("trap", "fe 20"),
        ("trap 0x0d", "fe 30 0d"),
        // mov, register then immediate, including a negative literal
        ("mov r1, r2", "01 40 01 02"),
        ("mov r1, 0x1234", "01 51 01 12 34"),
        ("mov r5, -#1", "01 41 05 ff"),
        // alu, three register form
        ("add r1, r2, r3", "10 50 01 02 03"),
        ("sub r1, r2, r3", "11 50 01 02 03"),
        ("and r1, r2, r3", "12 50 01 02 03"),
        ("or r1, r2, r3", "13 50 01 02 03"),
        ("xor r1, r2, r3", "15 50 01 02 03"),
        ("shl r1, r2, r3", "16 50 01 02 03"),
        ("shr r1, r2, r3", "17 50 01 02 03"),
        ("sar r1, r2, r3", "18 50 01 02 03"),
        ("mul r1, r2, r3", "19 50 01 02 03"),
        ("div r1, r2, r3", "1a 50 01 02 03"),
        ("not r1, r2", "14 40 01 02"),
        // alu, immediate form (decimal `#`) and multi byte truncation (hex)
        ("add r1, r2, #5", "10 51 01 02 05"),
        ("sub r1, r1, #1", "11 51 01 01 01"),
        ("and r1, r2, 0xff", "12 61 01 02 00 ff"),
        // compare, unsigned and signed, plus the immediate zero edge
        ("cmp r1, r2", "20 40 01 02"),
        ("cmp.s r1, r2", "21 40 01 02"),
        ("cmp r1, #0", "20 31 01"),
        // branch, every predicate sits in the flag nibble, bit 3 marks an immediate
        ("b 0x08", "30 38 08"),
        ("beq 0x08", "30 39 08"),
        ("bne 0x08", "30 3a 08"),
        ("blt 0x08", "30 3b 08"),
        ("bge 0x08", "30 3c 08"),
        ("bgt 0x08", "30 3d 08"),
        ("ble 0x08", "30 3e 08"),
        ("b r1", "30 30 01"),
        ("beq r1", "30 31 01"),
        // jump is absolute
        ("jmp r7", "31 30 07"),
        ("jmp 0x1000", "31 48 10 00"),
        // load, size and sign in the flag nibble, displacement mode
        ("ld r5, [r6]", "40 41 05 06"),
        ("ld.b r5, [r6]", "40 43 05 06"),
        ("ld.h r5, [r6]", "40 45 05 06"),
        ("ld.w r5, [r6]", "40 47 05 06"),
        ("ld.sb r5, [r6]", "40 4b 05 06"),
        ("ld.sh r5, [r6]", "40 4d 05 06"),
        ("ld.sw r5, [r6]", "40 4f 05 06"),
        ("ld r5, [r6 + 0x10]", "40 51 05 06 10"),
        ("ld r5, [r6 + r7]", "40 50 05 06 07"),
        // store, no sign variants, value goes in the first register slot
        ("st [r6], r5", "41 41 05 06"),
        ("st.b [r6], r5", "41 43 05 06"),
        ("st.h [r6], r5", "41 45 05 06"),
        ("st.w [r6], r5", "41 47 05 06"),
        ("st [r6 - #4], r5", "41 51 05 06 fc"),
        ("st [r6 + r7], r5", "41 50 05 06 07"),
    ];

    // A whole program and its padded image. Exercises labels, the relative
    // branch offset, and per instruction alignment.
    const IMAGE: &[(&str, &str)] = &[
        ("loop: sub r1, r1, #1\n b loop", "11 51 01 01 01 00 00 00 30 38 f8 00"),
        ("start: nop\n jmp start", "00 20 00 00 31 38 00 00"),
        ("b done\n done: nop", "30 38 04 00 00 20 00 00"),
    ];

    // Sources the assembler must reject.
    const REJECT: &[&str] = &[
        "add r1, r2",     // wrong operand count
        "add r1, r2, 5",  // decimal literal without the # prefix
        "mov r99, r1",    // register out of range
        "mov pc, r1",     // PC is architectural and not nameable
        "mov r33, r1",    // ...nor is its internal index
        "frobnicate r1",  // unknown mnemonic
        "b nowhere",      // unknown label
    ];

    #[test]
    fn instruction_encoding_matches_reference() {
        all(ENCODE, |&(src, want)| {
            let raw = assemble_raw(src).map_err(|e| format!("{src}: {e}"))?;
            match raw.as_slice() {
                [bytes] if hex(bytes) == want => Ok(()),
                [bytes] => Err(format!("{src}: got [{}] want [{want}]", hex(bytes))),
                _ => Err(format!("{src}: expected one instruction, got {}", raw.len())),
            }
        });
    }

    #[test]
    fn image_layout_matches_reference() {
        all(IMAGE, |&(src, want)| {
            let (image, _) = assemble(src).map_err(|e| format!("{src}: {e}"))?;
            (hex(&image) == want)
                .then_some(())
                .ok_or_else(|| format!("{src}: got [{}] want [{want}]", hex(&image)))
        });
    }

    #[test]
    fn invalid_sources_are_rejected() {
        all(REJECT, |&src| match assemble(src) {
            Err(_) => Ok(()),
            Ok(_) => Err(format!("{src}: assembled but should have been rejected")),
        });
    }
}