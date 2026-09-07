// Crate root for the Vea assembler + simulator.
//
// Everything lives here so the binary (`src/main.rs`) and the integration tests
// under `tests/` share one library. `main.rs` is just the CLI shell.

pub mod assembler;
pub mod error;
pub mod isa;
pub mod memory;
pub mod nstep;
pub mod onestep;
pub mod shared;
pub mod ui;

pub use error::{Error, Result};
