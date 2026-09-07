// error.rs

/*
 * Project wide error handling.
 *
 * A module declares its own error type, writes a Display impl that is the
 * message, and adds an empty `impl SimError`. Functions return crate::Result
 * and the value converts into the shared Error through `?` with nothing extra
 * at the call site.
 *
 *   sim_error!(Halt, "executed a halt instruction");       // no payload
 *
 *   #[derive(Debug)]
 *   struct BadAddr(u64);
 *   impl std::fmt::Display for BadAddr { ... }
 *   impl crate::error::SimError for BadAddr {}
 *
 *   return Err(sim_err!("address {addr:#x} is unmapped")); // one off
 */

use std::fmt;

/// Marker for a module local error. Its Display output is the message.
pub trait SimError: fmt::Display + fmt::Debug + Send + Sync + 'static {}

/// The single error type that crosses module boundaries. Any SimError turns into
/// one through `?`.
pub struct Error(Box<dyn SimError>);

impl<E: SimError> From<E> for Error {
    fn from(e: E) -> Self {
        Error(Box::new(e))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl std::error::Error for Error {}

/// Every fallible operation in the simulator returns this.
pub type Result<T> = std::result::Result<T, Error>;

/// A ready made error that just carries an owned message. Build it with sim_err!.
#[derive(Debug)]
pub struct Message(pub String);

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl SimError for Message {}

/// Declare a payload free error type whose message is fixed.
///
/// `sim_error!(Halt, "executed a halt instruction");`
#[macro_export]
macro_rules! sim_error {
    ($name:ident, $message:literal) => {
        #[derive(Debug)]
        pub struct $name;

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str($message)
            }
        }

        impl $crate::error::SimError for $name {}
    };
}

/// Build an Error from a format string, on the spot.
///
/// `return Err(sim_err!("address {addr:#x} is unmapped"));`
#[macro_export]
macro_rules! sim_err {
    ($($arg:tt)*) => {
        $crate::error::Error::from($crate::error::Message(::std::format!($($arg)*)))
    };
}