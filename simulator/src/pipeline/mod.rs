use crate::{memory};

// The pipeline stages. Placeholder as the microarchitecture is not designed
// yet, so the count and names will change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Fetch(u64),
    Decode,
    Execute(u64), // (executing for PC=...)
    Memory(memory::Access, u64, Option<u64>), // Memory state represents (access type, address, <value>)
    Writeback(),
    Sleep,
}

// Pipeline structure itself should carry some of the more microarchitectural
// info than anything else. TBD. 
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorPipeline {
    pub state: Stage,
    // DEBUG: "forever" history of all the stages and their info for
    // the processor object owning this given Pipeline instance
    pub pl_history: Vec<Stage>,
    // various uarch control flags follow. enjoy
}

impl ProcessorPipeline {
    pub fn new() -> Self {
        ProcessorPipeline { state: Stage::Sleep, pl_history: Vec::new() }
    }
}

impl Stage {
    pub fn invoke(current: Stage) {
        match current {
            Stage::Fetch(pc) => {
                todo!()
            }
            Stage::Decode => todo!(),
            Stage::Execute(pc) => todo!(),
            Stage::Memory(access, addr, val) => todo!(),
            Stage::Writeback() => todo!(),
            Stage::Sleep => {}
        }
    }
}
