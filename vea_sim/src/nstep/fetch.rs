use crate::isa;
use crate::nstep::{frame_len, IfId, Processor};
use crate::{error, memory, sim_err};

impl Processor {
    // IF: pull the frame at `pc` straight out of memory into the IF/ID latch
    // and step `pc` past it.
    pub fn fetch(&mut self) -> error::Result<()> {
        // Decode saw a halt behind us - nothing past it should ever enter the
        // pipe, so just stop bringing in new frames and let the rest drain.
        if self.halt_pending {
            self.if_id = None;
            return Ok(());
        }

        let pc = self.pc;
        // Only the opcode byte must be mapped; a short frame can sit against the
        // end of the image. Off mapped memory means control flow left the program.
        if !memory::is_mapped(pc) {
            self.if_id = None;
            return sim_err!("instruction fetch at unmapped address {pc:#018x}");
        }

        let mut bytes = [0u8; isa::MAX_INSN_BYTES];
        for (i, b) in bytes.iter_mut().enumerate() {
            let a = pc.wrapping_add(i as u64);
            *b = if memory::is_mapped(a) { memory::read(a, 1).unwrap_or(0) as u8 } else { 0 };
        }

        self.pc = isa::align_up(pc + frame_len(&bytes)?, isa::ALIGNMENT);
        self.if_id = Some(IfId { pc, bytes });
        Ok(())
    }

    // Point Fetch at `target` and drop everything speculative behind it.
    // Decode calls this the instant a branch resolves taken, which happens
    // before this same cycle's fetch() runs
    pub(crate) fn redirect_fetch(&mut self, target: u64) {
        self.pc = target;
        self.if_id = None;
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
    fn fetch_delivers_the_frame_in_one_call() {
        let _seq = memory::test_guard();
        let mut p = load("nop\nnop\nhalt\n");

        p.fetch().unwrap();
        let f = p.if_id.take().expect("frame delivered");
        assert_eq!(f.pc, 0);
        assert_eq!(f.bytes[0], 0x00, "opcode byte of nop");
        assert_eq!(p.pc, 4, "past the 2-byte frame, aligned to 4");

        memory::reset();
    }

    #[test]
    fn walks_frames_by_their_own_length() {
        let _seq = memory::test_guard();
        let mut p = load("mov r1, #5\nadd r2, r1, r1\nhalt\n");

        let mut seen = Vec::new();
        for _ in 0..3 {
            p.fetch().unwrap();
            seen.push(p.if_id.take().expect("frame delivered").pc);
        }
        // mov r1,#5 -> 4 bytes; add r2,r1,r1 -> 5 bytes, aligned up to 12.
        assert_eq!(seen, vec![0, 4, 12]);
        memory::reset();
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
