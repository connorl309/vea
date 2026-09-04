use crate::memory;

// The pipeline stages. Placeholder as the microarchitecture is not designed
// yet, so the count and names will change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Fetch(u64),
    Decode,
    Execute(), // tbd on enum variant.. should we have internal control bits/flags exposed here?
    Memory(memory::Access, u64, Option<u64>), // Memory state represents (access type, address, <value>)
    Writeback,
}

// Pipeline structure itself should carry some of the more microarchitectural
// info than anything else. TBD. 
pub struct ProcessorPipeline {
    pub state: Stage,
}
