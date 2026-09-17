use crate::error;
use crate::nstep::{Commit, Processor};

impl Processor {
    // WB: commit whatever Execute decided
    pub fn writeback(&mut self) -> error::Result<()> {
        let Some(latch) = self.ex_wb.take() else {
            return Ok(());
        };

        match latch.commit {
            Commit::Nothing => {}
            Commit::Reg { rd, value } => self.regs[rd] = value as u64,
            Commit::Flags { z, n, c, v } => self.cc.set(z, n, c, v),
            Commit::Halt => self.halted = true,
        }

        self.completed_instrs += 1;
        Ok(())
    }
}
