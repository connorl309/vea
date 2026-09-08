// error.rs

/*
 * Project wide error handling, kept deliberately tiny.
 *
 * There is one error type. It carries an owned message and nothing else.
 * Every fallible operation returns `crate::Result<T>`. To fail, call
 * `sim_err!` with a format string, from anywhere, no imports:
 *
 *   return sim_err!("address {addr:#x} is unmapped");
 *
 *   if masks > 0xF {
 *       return sim_err!("condition code mask {masks:#06b} has undefined bits");
 *   }
 *
 * `sim_err!` expands to `Err(Error(format!(..)))`, so it is the whole return
 * value / tail expression, not something you wrap in `Err(..)` yourself.
 */

use std::fmt;

// The single error type. Its `Display` output is the message.
pub struct Error(pub String);

// Every fallible operation in the simulator returns this.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

// Build a failed `Result` from a format string, on the spot.
//
// `return sim_err!("address {addr:#x} is unmapped");`
#[macro_export]
macro_rules! sim_err {
    ($($arg:tt)*) => {
        ::core::result::Result::Err($crate::error::Error(::std::format!($($arg)*)))
    };
}
