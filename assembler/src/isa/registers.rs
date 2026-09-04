// Register file

/// Number of general-purpose registers.
pub const COUNT: u8 = 32;

/// Register width in bits.
pub const BITS: u32 = 64;

pub struct RegDef {
    pub index: u8,
    pub canonical: &'static str,
    pub aliases: &'static [&'static str],
}

/// Filled in programmatically below so this stays a single knob.
pub fn all() -> &'static [RegDef] {
    use std::sync::OnceLock;
    static REGS: OnceLock<Vec<RegDef>> = OnceLock::new();
    REGS.get_or_init(|| {
        (0..COUNT)
            .map(|i| RegDef {
                index: i,
                canonical: Box::leak(format!("r{i}").into_boxed_str()),
                aliases: alias_table(i),
            })
            .collect()
    })
}

/// Per-register ABI aliases. Empty until the ABI is decided.
fn alias_table(index: u8) -> &'static [&'static str] {
    match index {
        // 30 => &["lr", "ra"],
        // 31 => &["sp"],
        _ => &[],
    }
}

/// Resolve a register name (canonical or alias) to its index.
pub fn lookup(name: &str) -> Option<u8> {
    let lname = name.to_ascii_lowercase();
    all().iter().find_map(|r| {
        if r.canonical == lname || r.aliases.contains(&lname.as_str()) {
            Some(r.index)
        } else {
            None
        }
    })
}

/// Canonical name for an index, for listings and future disassembly.
pub fn name(index: u8) -> &'static str {
    all()
        .get(index as usize)
        .map(|r| r.canonical)
        .unwrap_or("invalid register index")
}