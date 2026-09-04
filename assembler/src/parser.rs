// Line-oriented parser. The format is intentionally rigid:
//
//   - one statement per line
//   - a label is `name:` alone on its line, nothing else
//   - everything else is `mnemonic op, op, ...`
//   - `;` or `//` starts a comment
//
// Anything that doesn't fit is an error, not a guess.

use crate::ast::{Instr, Item, ItemKind, Operand, Program};
use crate::err::{Result, at};
use crate::isa::registers;

pub fn parse(src: &str) -> Result<Program> {
    let mut items = Vec::new();

    for (i, raw) in src.lines().enumerate() {
        let line = i + 1;
        let text = strip_comment(raw).trim();
        if text.is_empty() {
            continue;
        }

        // label: `name:` and nothing else
        if let Some(name) = text.strip_suffix(':') {
            let name = name.trim();
            if !is_ident(name) {
                return Err(at(line, format!("not a valid label: `{text}`")));
            }
            items.push(Item { line, kind: ItemKind::Label(name.to_string()) });
            continue;
        }
        if text.contains(':') {
            return Err(at(line, "a label must be alone on its own line"));
        }

        // instruction: mnemonic then comma-separated operands
        let mut split = text.splitn(2, char::is_whitespace);
        let mnemonic = split.next().unwrap().to_ascii_lowercase();
        let rest = split.next().unwrap_or("").trim();
        let operands = if rest.is_empty() {
            Vec::new()
        } else {
            rest.split(',')
                .map(|op| parse_operand(line, op.trim()))
                .collect::<Result<Vec<_>>>()?
        };

        items.push(Item { line, kind: ItemKind::Instr(Instr { mnemonic, operands }) });
    }

    Ok(Program { items })
}

fn parse_operand(line: usize, s: &str) -> Result<Operand> {
    if s.is_empty() {
        return Err(at(line, "empty operand"));
    }

    if let Some(inner) = s.strip_prefix('[') {
        let inner = inner
            .strip_suffix(']')
            .ok_or_else(|| at(line, "unclosed `[` in memory operand"))?;
        return parse_mem(line, inner.trim());
    }

    // `#` in front of an immediate is optional sugar
    let s = s.strip_prefix('#').map(str::trim).unwrap_or(s);

    if let Some(r) = registers::lookup(s) {
        return Ok(Operand::Reg(r));
    }
    if let Some(v) = parse_int(s) {
        return Ok(Operand::Int(v));
    }
    if is_ident(s) {
        return Ok(Operand::Sym(s.to_string()));
    }
    Err(at(line, format!("can't parse operand `{s}`")))
}

// `rb`, `rb + disp`, `rb - disp`
fn parse_mem(line: usize, inner: &str) -> Result<Operand> {
    let (base_str, disp) = match inner.find(['+', '-']) {
        Some(pos) => {
            let sign = if &inner[pos..pos + 1] == "-" { -1 } else { 1 };
            let disp_str = inner[pos + 1..].trim();
            let mag = parse_int(disp_str)
                .ok_or_else(|| at(line, format!("bad displacement `{disp_str}`")))?;
            (inner[..pos].trim(), sign * mag)
        }
        None => (inner, 0),
    };

    let base = registers::lookup(base_str)
        .ok_or_else(|| at(line, format!("`{base_str}` is not a register")))?;
    Ok(Operand::Mem { base, disp })
}

fn strip_comment(s: &str) -> &str {
    let cut = [s.find(';'), s.find("//")].into_iter().flatten().min();
    match cut {
        Some(c) => &s[..c],
        None => s,
    }
}

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

// Parse `42`, `0x2a`, `0b1010`, `0o17`, `-1`, `1_000`.
pub fn parse_int(s: &str) -> Option<i128> {
    let s = s.trim();

    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r.trim_start()),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };

    let lower = rest.to_ascii_lowercase();
    let (radix, digits) = if let Some(h) = lower.strip_prefix("0x") {
        (16, h)
    } else if let Some(b) = lower.strip_prefix("0b") {
        (2, b)
    } else if let Some(o) = lower.strip_prefix("0o") {
        (8, o)
    } else {
        (10, lower.as_str())
    };

    let clean: String = digits.chars().filter(|&c| c != '_').collect();
    if clean.is_empty() {
        return None;
    }
    let mag = i128::from_str_radix(&clean, radix).ok()?;
    Some(if neg { -mag } else { mag })
}
