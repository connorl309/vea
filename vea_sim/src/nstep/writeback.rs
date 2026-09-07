use crate::nstep::Processor;
use crate::{error, sim_err};

impl Processor {
    pub fn writeback(&mut self) -> error::Result<()> {
        return sim_err!("nstep - writeback() not implemented!")
    }
}