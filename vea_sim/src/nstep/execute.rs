use crate::error;
use crate::nstep::Processor;

impl Processor {
    /// EX: not implemented yet. Nothing reaches this stage until Decode fills
    /// the ID/EX latch, so for now it is a no-op the pipeline drains through.
    pub fn execute(&mut self) -> error::Result<()> {
        Ok(())
    }
}
