use crate::isa;
use crate::nstep::{frame_len, IfId, Processor};
use crate::{error, memory, sim_err};

impl Processor {
    // IF: pull the frame at `pc` from the i-cache into the IF/ID latch and step
    // `pc` past it. A refill costs MEM_READ_DELAY cycles.
    pub fn fetch(&mut self) -> error::Result<()> {
        if self.fetch_stall > 0 {
            self.fetch_stall -= 1;
            if self.fetch_stall > 0 {
                self.if_id = None;
                return Ok(());
            }
        }

        let pc = self.pc;
        // Only the opcode byte must be mapped; a short frame can sit against the
        // end of the image. Off mapped memory means control flow left the program.
        if !memory::is_mapped(pc) {
            self.if_id = None;
            return sim_err!("instruction fetch at unmapped address {pc:#018x}");
        }

        let (bytes, penalty) = self.icache.fill(pc, isa::MEM_READ_DELAY);
        if penalty > 0 {
            self.fetch_stall = penalty;
            self.if_id = None;
            return Ok(());
        }

        self.pc = isa::align_up(pc + frame_len(&bytes)?, isa::ALIGNMENT);
        self.if_id = Some(IfId { pc, bytes });
        Ok(())
    }

    // Point Fetch at `target` and drop everything speculative behind it. Branch
    // resolution calls this from the back of the pipe.
    #[allow(dead_code)] // TODO wire up when Execute/Writeback resolve branches
    pub(crate) fn redirect_fetch(&mut self, target: u64) {
        self.pc = target;
        self.if_id = None;
        self.fetch_stall = 0;
        self.icache.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::assemble_listing;

    fn load(src: &str) -> Processor {
        let (image, _) = assemble_listing(src).expect("assembles");
        memory::reset();
        memory::load(0, &image).expect("loads");
        Processor::new()
    }

    #[test]
    fn cold_frame_stalls_then_delivers() {
        let _seq = memory::test_guard();
        let mut p = load("nop\nnop\nhalt\n");

        for _ in 0..isa::MEM_READ_DELAY {
            p.fetch().unwrap();
            assert!(p.if_id.is_none(), "bubble while the refill is outstanding");
            assert_eq!(p.pc, 0, "fetch pointer parked during the stall");
        }

        p.fetch().unwrap();
        let f = p.if_id.take().expect("frame delivered");
        assert_eq!(f.pc, 0);
        assert_eq!(f.bytes[0], 0x00, "opcode byte of nop");
        assert_eq!(p.pc, 4, "past the 2-byte frame, aligned to 4");

        memory::reset();
    }

    #[test]
    fn walks_frames_and_hits_the_warm_window() {
        let _seq = memory::test_guard();
        let mut p = load("mov r1, #5\nadd r2, r1, r1\nhalt\n");

        let mut seen = Vec::new();
        for _ in 0..64 {
            p.fetch().unwrap();
            if let Some(f) = p.if_id.take() {
                seen.push(f.pc);
                if f.bytes[0] == 0xFF {
                    break;
                }
            }
        }
        // mov r1,#5 -> 4 bytes; add r2,r1,r1 -> 5 bytes, aligned up to 12.
        assert_eq!(seen, vec![0, 4, 12]);
        // Only the first fetch missed.
        assert_eq!(p.fetch_stall, 0);
    }

    #[test]
    fn unmapped_fetch_faults() {
        let _seq = memory::test_guard();
        memory::reset();
        let mut p = Processor::new();
        p.pc = 0x4000;
        assert!(p.fetch().is_err());
    }
}
