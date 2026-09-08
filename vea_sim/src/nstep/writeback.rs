use crate::error;
use crate::nstep::Processor;

impl Processor {
    /// WB: not implemented yet. Nothing reaches this stage until Execute fills
    /// the EX/WB latch, so for now it is a no-op the pipeline drains through.
    pub fn writeback(&mut self) -> error::Result<()> {
        Ok(())
    }
}
