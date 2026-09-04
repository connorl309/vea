use crate::{memory};

// The pipeline stages. Placeholder as the microarchitecture is not designed
// yet, so the count and names will change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Fetch(u64),
    Decode,
    Execute(u64), // (executing for PC=...)
    Memstage(memory::Access, u64, Option<u64>), // Memory state represents (access type, address, <value>)
    Writeback(),
    Sleep,
}
