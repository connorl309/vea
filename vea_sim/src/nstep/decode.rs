use crate::error;
use crate::nstep::Processor;
use crate::sim_err;

impl Processor {
    // todo: decode
    pub fn decode(&mut self) -> error::Result<()> {
        if let Some(frame) = self.if_id.take() {
            return sim_err!("nstep decode() not implemented (frame at {:#018x})", frame.pc);
        }
        Ok(())
    }
}
