use asm::isa::framing;

use crate::memory;
use crate::processor::{Core, Trap};

// IF/ID latch, which is basically just splitting up
// the bytes of any possible opcode into structures we know
// exist (opcodes, reg sources/dests, immediate bytes, etc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IfIdLatch {
    pub valid: bool,
    pub pc: u64,
    // opcode[byte0], plen|flags[byte1], then up to PLEN_MAX payload bytes.
    pub bytes: [u8; 2 + framing::PLEN_MAX]
}

impl Default for IfIdLatch {
    fn default() -> Self {
        IfIdLatch { valid: false, pc: 0, bytes: [0; 2 + framing::PLEN_MAX] }
    }
}

impl Core {
    // Fetch the instruction frame sitting at `pc` into an IF/ID latch. We don't
    // know the real frame length yet - that's in byte 1, which nobody has
    // decoded - so we just grab the widest a frame can ever be (2 + PLEN_MAX)
    // and let decode handle interpreting the remaining byte values.
    //
    // In silicon this is where the icache would sit. For now it's a straight
    // read out of the fake memory object.
    pub fn fetch(&self) -> Result<IfIdLatch, Trap> {
        // Bit 0 of the PC is a reserved tag bit and every frame is 2-byte
        // aligned, so an odd PC means something upstream corrupted it.
        if self.pc % framing::INSTR_ALIGN != 0 {
            return Err(Trap::MisalignedPc { pc: self.pc });
        }

        // Clamp the window to whatever is actually left above `pc`.
        let want = (2 + framing::PLEN_MAX) as u64;
        let have = (memory::MEM_SIZE as u64).saturating_sub(self.pc).min(want) as usize;

        let mut bytes = [0u8; 2 + framing::PLEN_MAX];
        if have > 0 {
            let chunk = memory::read(self.pc, have as u64).map_err(Trap::Memory)?;
            bytes[..have].copy_from_slice(&chunk);
        }

        Ok(IfIdLatch { valid: true, pc: self.pc, bytes })
    }
}
