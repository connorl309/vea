use asm::isa::opcodes::by_opcode;
use asm::isa::{Form, INSTRUCTIONS, InstrDef, framing};

use crate::processor::{Core, Trap};

use super::fetch::IfIdLatch;

// A memory instruction's direction. Not the address - that isn't known until
// Execute computes rb + disp, so it isn't decode's job to hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemOp {
    Load,
    Store,
}

// ID/EX latch.
// `rd`/`rs1`/`rs2` are the (up to) one
// write-port and two read-port register indices a real regfile would be
// driven with this cycle (`None` = that port's valid bit is low).
// `imm_raw` is the instruction's immediate as a plain host u64: its
// `imm_width` payload bytes read big-endian and right-aligned, so a 2-byte
// `0xABCD` arrives as `0xABCD`, not `0xABCD00_00000000`. It is intentionally
// NOT sign/zero-extended here - EX does that per-op, keying off `imm_width`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdExLatch {
    pub valid: bool,
    pub pc: u64,
    pub instr: &'static InstrDef,
    pub rd: Option<u8>,
    pub rs1: Option<u8>,
    pub rs2: Option<u8>,
    pub imm_raw: u64,
    pub imm_width: u8,
    pub mem_op: Option<MemOp>,
}

impl IdExLatch {
    pub fn as_reset() -> Self {
        IdExLatch { valid: false, pc: 0, instr: &INSTRUCTIONS[0], rd: None, rs1: None, rs2: None, imm_raw: 0, imm_width: 0, mem_op: None }
    }
}

impl Core {
    // Decode one IF/ID latch into an ID/EX latch
    pub fn decode(&self, latch: &IfIdLatch) -> Result<IdExLatch, Trap> {
        if !latch.valid {
            return Ok(IdExLatch::as_reset());
        }

        let opcode = latch.bytes[0];
        let (plen, flags) = framing::unpack_len_flags(latch.bytes[1]);
        let instr = by_opcode(opcode, flags).ok_or(Trap::IllegalInstruction { pc: latch.pc })?;

        let reg_count = instr.form.reg_count() as usize;
        if (plen as usize) < reg_count {
            return Err(Trap::MalformedInstruction { pc: latch.pc });
        }

        // Real immediate width
        let imm_width = plen as usize - reg_count;
        if (imm_width > 0) != instr.form.has_imm() || !matches!(imm_width, 0 | 1 | 2 | 4 | 8) {
            return Err(Trap::MalformedInstruction { pc: latch.pc });
        }

        // Parse out the possible regs used in this instruction. One byte each,
        // right after the opcode/plen|flags header.
        let mut reads = [None; 2];
        let mut next_read = 0;
        for i in 0..reg_count {
            let reg = latch.bytes[2 + i] & framing::REG_INDEX_MASK;
            if instr.writes == Some(i as u8) {
                continue;
            }
            reads[next_read] = Some(reg);
            next_read += 1;
        }
        let rd = instr.writes.map(|w| latch.bytes[2 + w as usize] & framing::REG_INDEX_MASK);

        // Immediate: exactly the `imm_width` payload bytes (0/1/2/4/8), which sit
        // right after the register bytes, read big-endian and right-aligned into
        // a u64. Only the real payload is touched - nothing is pulled in from the
        // next frame - and the host sees the plain numeric value. `imm_width` is
        // carried alongside for EX to sign/zero-extend.
        // Bounds: `imm_start + imm_width == 2 + plen`, and plen is a 4-bit field
        // (<= 15), so this never runs past the 17-byte frame buffer.
        let imm_start = 2 + reg_count;
        let mut imm_bytes = [0u8; 8];
        imm_bytes[8 - imm_width..].copy_from_slice(&latch.bytes[imm_start..imm_start + imm_width]);
        let imm_raw = u64::from_be_bytes(imm_bytes);

        let mem_op = (instr.form == Form::RMem).then(|| if rd.is_some() { MemOp::Load } else { MemOp::Store });

        Ok(IdExLatch {
            valid: true,
            pc: latch.pc,
            instr: instr,
            rd,
            rs1: reads[0],
            rs2: reads[1],
            imm_raw,
            imm_width: imm_width as u8,
            mem_op,
        })
    }
}
