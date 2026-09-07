use crate::nstep::Processor;
use crate::{error, sim_err};

impl Processor {
    pub fn decode(&mut self) -> error::Result<()> {
        return sim_err!("nstep - decode() not implemented!")
    }
}