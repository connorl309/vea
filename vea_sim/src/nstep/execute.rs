use crate::nstep::Processor;
use crate::{error, sim_err};

impl Processor {
    pub fn execute(&mut self) -> error::Result<()> {
        return sim_err!("nstep - execute() not implemented!")
    }
}