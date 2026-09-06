// One file per pipeline stage. Each stage owns the latch it produces (the
// latch's fields ARE the flip-flops for that clock boundary - nothing in a
// latch is derived or recomputed later) plus the function that fills it from
// the previous stage's latch. `valid` on a latch is the bubble bit: false
// means "nothing here, ignore the rest" (reset state, a stalled slot, or a
// branch-flushed slot).

pub mod decode;
pub mod fetch;
pub mod execute;
pub mod writeback;

pub use decode::*;
pub use fetch::*;
pub use execute::*;