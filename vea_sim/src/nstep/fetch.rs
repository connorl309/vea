use crate::nstep::Processor;
use crate::{error, sim_err};

impl Processor {
    pub fn fetch(&mut self) -> error::Result<()> {
        return sim_err!("nstep - fetch() not implemented!")
    }
}