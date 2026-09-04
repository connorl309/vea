// Errors are just strings. Layers that know the line number prefix it
// themselves (see `at`). The CLI prints whatever bubbles up.

pub type Result<T> = std::result::Result<T, String>;

/// `return Err(asm_err!("bad thing {}", x))`
#[macro_export]
macro_rules! asm_err {
    ($($t:tt)*) => { format!($($t)*) };
}

/// Prefix a message with its source line, e.g. `at(12, "unknown mnemonic")`.
pub fn at(line: usize, msg: impl std::fmt::Display) -> String {
    format!("line {line}: {msg}")
}

/// Print a finished error message to stderr with a splash of colour.
pub fn report(msg: &str) {
    eprintln!("[ assembler ]: \x1b[1;31merror:\x1b[0m {msg}");
}