use asm::isa::framing;

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
