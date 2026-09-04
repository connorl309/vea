// How the bytes of a single instruction are laid out. If the encoding frame
// ever changes, this is the only file that should need touching.
//
// Frame:  [opcode:1] [PLEN:4 | FLAGS:4] [payload: PLEN bytes]
//
//   - Total length is 2 + PLEN, i.e. 2..=17 bytes.
//   - PLEN counts the whole payload: one byte per register operand (in
//     source order) followed by a contiguous big-endian immediate of
//     0/1/2/4/8 bytes.
//   - PLEN is the high nibble of byte 2, FLAGS the low nibble.
//   - FLAGS meaning is per-opcode; the assembler emits the constant carried
//     by the chosen mnemonic (see isa/opcodes.rs).
//
// Instructions are 2-byte aligned. When a frame ends on an odd address the
// layout layer emits an 0x00 pad byte before the next instruction; that pad
// is not part of either frame.

use crate::err::Result;

// Padding byte; also NOP
pub const OPCODE_PAD_NOP: u8 = 0x00;

// HALTs the processor
pub const OPCODE_HALT: u8 = 0xFF;

// PLEN is high 4 bits of byte1.
pub const PLEN_MASK: u8 = 0xF0;

// FLAGS is low 4 bits of byte1.
pub const FLAGS_MASK: u8 = 0x0F;

// Largest payload PLEN can describe (4 bits).
pub const PLEN_MAX: usize = (PLEN_MASK >> 4) as usize;

// Instruction start alignment, in bytes.
pub const INSTR_ALIGN: u64 = 2;

// Register operand byte: index in the low 5 bits, top 3 reserved (zero).
pub const REG_INDEX_MASK: u8 = 0x1F;

// Build byte 2 from PLEN and FLAGS.
pub const fn pack_len_flags(plen: u8, flags: u8) -> u8 {
    (plen << 4) | (flags & 0x0F)
}

// Split byte 2 back into (PLEN, FLAGS).
pub const fn unpack_len_flags(b: u8) -> (u8, u8) {
    (b >> 4, b & 0x0F)
}

// One register operand byte.
pub const fn reg_byte(index: u8) -> u8 {
    index & REG_INDEX_MASK
}

// Lay out one full instruction frame. `imm` must already be big-endian and
// 0/1/2/4/8 bytes long. Returns exactly `2 + PLEN` bytes (no alignment pad).
pub fn frame(opcode: u8, flags: u8, regs: &[u8], imm: &[u8]) -> Result<Vec<u8>> {
    if !matches!(imm.len(), 0 | 1 | 2 | 4 | 8) {
        return Err(asm_err!("immediate must be 0/1/2/4/8 bytes, got {}", imm.len()));
    }
    if flags & !FLAGS_MASK != 0 {
        return Err(asm_err!("flags nibble {flags:#x} out of range"));
    }

    let plen = regs.len() + imm.len();
    if plen > PLEN_MAX {
        return Err(asm_err!("payload {plen} bytes exceeds PLEN max {PLEN_MAX}"));
    }

    let mut out = Vec::with_capacity(2 + plen);
    out.push(opcode);
    out.push(pack_len_flags(plen as u8, flags));
    out.extend(regs.iter().map(|&r| reg_byte(r)));
    out.extend_from_slice(imm);
    Ok(out)
}