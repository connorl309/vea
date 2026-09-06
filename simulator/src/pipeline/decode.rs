use asm::isa::opcodes::by_opcode;
use asm::isa::{Form, InstrDef, framing};

use crate::processor::Trap;

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
// driven with this cycle (`None` = that port's valid bit is low). The
// immediate is intentionally NOT sign/zero-extended here. The ALU will
// perform all ops at 64 bit sizes then mask off the result as specified
// by plen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IdExLatch {
    pub valid: bool,
    pub pc: u64,
    pub instr: Option<&'static InstrDef>,
    pub rd: Option<u8>,
    pub rs1: Option<u8>,
    pub rs2: Option<u8>,
    pub imm_raw: u64,
    pub imm_width: u8,
    pub mem_op: Option<MemOp>,
}

// Decode one IF/ID latch into an ID/EX latch
pub fn decode(latch: &IfIdLatch) -> Result<IdExLatch, Trap> {
    if !latch.valid {
        return Ok(IdExLatch::default());
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

    // Immediates always load a fixed 8-byte big-endian window starting right
    // after the registers, regardless of imm_width.
    // `reg_count` maxes out at 3 (Form::RRR) and `latch.bytes` is a
    // fixed 17-byte buffer, so this window is always in bounds even though
    // it may read past this instruction's real payload into whatever the
    // icache has next.
    let window: &[u8; 8] = latch.bytes[2 + reg_count..2 + reg_count + 8].try_into().unwrap();

    let mem_op = (instr.form == Form::RMem).then(|| if rd.is_some() { MemOp::Load } else { MemOp::Store });

    Ok(IdExLatch {
        valid: true,
        pc: latch.pc,
        instr: Some(instr),
        rd,
        rs1: reads[0],
        rs2: reads[1],
        imm_raw: u64::from_be_bytes(*window),
        imm_width: imm_width as u8,
        mem_op,
    })
}