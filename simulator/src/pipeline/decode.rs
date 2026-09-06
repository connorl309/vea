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
pub struct IdExWbLatch {
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

impl IdExWbLatch {
    pub fn as_reset() -> Self {
        IdExWbLatch { valid: false, pc: 0, instr: &INSTRUCTIONS[0], rd: None, rs1: None, rs2: None, imm_raw: 0, imm_width: 0, mem_op: None }
    }
}

impl Core {
    // Decode one IF/ID latch into an ID/EX latch
    pub fn decode(&self, latch: &IfIdLatch) -> Result<IdExWbLatch, Trap> {
        if !latch.valid {
            return Ok(IdExWbLatch::as_reset());
        }

        let opcode = latch.bytes[0];
        let (plen, flags) = framing::unpack_len_flags(latch.bytes[1]);
        let instr = by_opcode(opcode, flags).ok_or(Trap::IllegalInstruction { pc: latch.pc })?;

        let reg_count = instr.form.reg_count() as usize;
        if (plen as usize) < reg_count {
            return Err(Trap::MalformedInstruction { pc: latch.pc });
        }

        // Real immediate width. A form that carries an immediate is allowed a
        // zero-width one - a literal `0` (or a `#0` displacement) encodes to no
        // payload bytes - so only the other direction, trailing bytes on a form
        // with no immediate, is malformed.
        let imm_width = plen as usize - reg_count;
        if (imm_width > 0 && !instr.form.has_imm()) || !matches!(imm_width, 0 | 1 | 2 | 4 | 8) {
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

        Ok(IdExWbLatch {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn latch(bytes: &[u8]) -> IfIdLatch {
        let mut buf = [0u8; 2 + framing::PLEN_MAX];
        buf[..bytes.len()].copy_from_slice(bytes);
        IfIdLatch { valid: true, pc: 0, bytes: buf }
    }

    #[test]
    fn zero_width_immediate_is_not_malformed() {
        // addi r1, r0, 0  ->  10 21 01 00   (RRI + FLAG_IMM, plen=2, no imm bytes)
        let idex = Core::new(0).decode(&latch(&[0x10, 0x21, 0x01, 0x00])).expect("decodes");
        assert_eq!(idex.instr.mnemonic, "addi");
        assert_eq!((idex.imm_raw, idex.imm_width), (0, 0));
    }

    #[test]
    fn immediate_setif_decodes_dest_and_source() {
        // slti r1, r2, 5  ->  30 31 01 02 05
        let idex = Core::new(0).decode(&latch(&[0x30, 0x31, 0x01, 0x02, 0x05])).expect("decodes");
        assert_eq!(idex.instr.mnemonic, "slti");
        assert_eq!((idex.rd, idex.rs1, idex.rs2), (Some(1), Some(2), None));
        assert_eq!((idex.imm_raw, idex.imm_width), (5, 1));
    }

    #[test]
    fn immediate_branch_reads_both_register_slots() {
        // beqi r3, r4, 7  ->  40 31 03 04 07   (rs in slot 0, target rt in slot 1)
        let idex = Core::new(0).decode(&latch(&[0x40, 0x31, 0x03, 0x04, 0x07])).expect("decodes");
        assert_eq!(idex.instr.mnemonic, "beqi");
        assert_eq!((idex.rd, idex.rs1, idex.rs2), (None, Some(3), Some(4)));
        assert_eq!((idex.imm_raw, idex.imm_width), (7, 1));
    }

    #[test]
    fn trailing_bytes_on_a_no_immediate_form_are_malformed() {
        // nop (Form::Nullary) with a stray payload byte: plen=1, reg_count=0
        let err = Core::new(0).decode(&latch(&[0x00, 0x10, 0xAA])).unwrap_err();
        assert!(matches!(err, Trap::MalformedInstruction { .. }));
    }
}
