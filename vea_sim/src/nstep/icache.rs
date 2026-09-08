// nstep/icache.rs

/**
 * Instruction cache for the pipelined sim. One contiguous window of program
 * bytes, [base, base + ICACHE_SIZE), that Fetch streams through by PC.
 *
 * A frame whose bytes all land inside the window is a hit and costs nothing.
 * Anything else - a branch out of the window, or a PC near enough the end that
 * the next frame would run off it - slides the window forward to start at the
 * current PC, reloads it from memory, and charges the miss penalty. Bytes
 * behind the new base are gone.
 *
 * No lines, no tags, no associativity. A queue of bytes indexed by PC.
 */

use crate::isa::{ICACHE_SIZE, MAX_INSN_BYTES};
use crate::memory;

pub struct ICache {
    base: u64,
    valid: bool,
    bytes: [u8; ICACHE_SIZE],
}

impl ICache {
    pub fn new() -> Self {
        ICache { base: 0, valid: false, bytes: [0; ICACHE_SIZE] }
    }

    // Drop the window. Fetch calls this on a branch redirect.
    pub fn flush(&mut self) {
        self.valid = false;
    }

    // Does the whole frame at `addr` sit inside the current window?
    fn covers(&self, addr: u64) -> bool {
        self.valid
            && addr >= self.base
            && addr.saturating_add(MAX_INSN_BYTES as u64)
                <= self.base.saturating_add(ICACHE_SIZE as u64)
    }

    // The fill function will fill the output slice with the data buffer
    // corresponding to that address. If there is a stall, return the stall count
    // in the u64 result in .1
    pub fn fill(&mut self, addr: u64, miss_penalty: u64) -> ([u8; MAX_INSN_BYTES], u64) {
        let stall = if self.covers(addr) {
            0
        } else {
            self.base = addr;
            for (i, b) in self.bytes.iter_mut().enumerate() {
                let a = addr.wrapping_add(i as u64);
                *b = if memory::is_mapped(a) {
                    memory::read(a, 1).unwrap_or(0) as u8
                } else {
                    0
                };
            }
            self.valid = true;
            miss_penalty
        };

        let off = (addr - self.base) as usize;
        let mut frame = [0u8; MAX_INSN_BYTES];
        frame.copy_from_slice(&self.bytes[off..off + MAX_INSN_BYTES]);
        (frame, stall)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::assemble_listing;

    #[test]
    fn hit_after_first_fill_then_slide_near_the_end() {
        let _seq = memory::test_guard();
        let (image, _) = assemble_listing("nop\n".repeat(200).as_str()).unwrap();
        memory::reset();
        memory::load(0, &image).unwrap();

        let mut ic = ICache::new();

        // Cold: slides to 0, charges the penalty.
        let (_, stall) = ic.fill(0, 3);
        assert_eq!(stall, 3);
        assert_eq!(ic.base, 0);

        // Anything still inside [0, ICACHE_SIZE) is free.
        let (_, stall) = ic.fill(4, 3);
        assert_eq!(stall, 0);

        // A PC close enough to the end that a full frame would overflow forces
        // another slide.
        let near_end = (ICACHE_SIZE - MAX_INSN_BYTES + 1) as u64;
        let near_end = near_end - near_end % 4;
        let (_, stall) = ic.fill(near_end, 3);
        assert_eq!(stall, 3);
        assert_eq!(ic.base, near_end);

        memory::reset();
    }

    #[test]
    fn flush_forces_a_refill() {
        let _seq = memory::test_guard();
        memory::reset();
        memory::load(0, &[0u8; 64]).unwrap();

        let mut ic = ICache::new();
        assert_eq!(ic.fill(0, 3).1, 3);
        assert_eq!(ic.fill(0, 3).1, 0);
        ic.flush();
        assert_eq!(ic.fill(0, 3).1, 3);

        memory::reset();
    }
}
