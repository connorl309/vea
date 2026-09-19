use crate::assembler::{
    BR_ALWAYS, BR_EQ, BR_GE, BR_GT, BR_IMM, BR_LE, BR_LT, BR_MASK, BR_NE, LS_SEXT,
    OPINFO_FLAG_ALSO_IMMEDIATE,
};
use crate::isa;
use crate::nstep::{Commit, DecodedOp, ExWb, IdEx, Processor};
use crate::{error, sim_err};

impl Processor {
    // ID consumes the byte buffer from fetch and figures out
    // what instruction we're about to execute and its parameter values.
    //
    // Taken branches redirect Fetch right here instead of waiting for a later
    // stage. This should be re-evaluated when working on RTL.
    pub fn decode(&mut self) -> error::Result<()> {
        let Some(frame) = self.if_id.take() else {
            self.id_ex = None;
            return Ok(());
        };
        let op = self.decode_op(frame.pc, &frame.bytes)?;
        if let DecodedOp::Branch { taken: true, target } = op {
            self.redirect_fetch(target);
        }
        if op == DecodedOp::Halt {
            self.halt_pending = true;
        }
        self.id_ex = Some(IdEx { pc: frame.pc, op });
        Ok(())
    }

    // Almost (effectively) identical to onestep.
    fn decode_op(&self, pc: u64, bytes: &[u8; isa::MAX_INSN_BYTES]) -> error::Result<DecodedOp> {
        let opcode = bytes[0];
        let opinfo = bytes[1];

        Ok(match opcode {
            0x00 => DecodedOp::Nop,
            0xFF => DecodedOp::Halt,

            // mov / not: rd, then a register or sign-extended immediate
            0x01 | 0x14 => {
                let rd = self.reg_at(pc, bytes, 2)?;
                let (src, _) = self.source(bytes, 3, opinfo);
                if opcode == 0x01 { DecodedOp::Mov { rd, src } } else { DecodedOp::Not { rd, src } }
            }

            // cmp / cmp.s: a named register against a register or immediate
            0x20 | 0x21 => {
                let a = self.reg_value(self.reg_at(pc, bytes, 2)?) as i64;
                let (b, _) = self.source(bytes, 3, opinfo);
                DecodedOp::Cmp { a, b, signed: opcode == 0x21 }
            }

            // all ALU ops: rd, rs1, then a register or immediate. The op itself
            // is just the opcode's low nibble, same as onestep's alu().
            0x10..=0x13 | 0x15..=0x1A => {
                let rd = self.reg_at(pc, bytes, 2)?;
                let a = self.reg_value(self.reg_at(pc, bytes, 3)?) as i64;
                let (b, _) = self.source(bytes, 4, opinfo);
                DecodedOp::Alu { rd, nibble: opcode & 0x0F, a, b }
            }

            // b / beq / bne / blt / bge / bgt / ble: figure out both taken and
            // target now. An immediate is relative to this frame's own pc; a
            // bare register is always absolute.
            0x30 => {
                let (z, n, v) = self.cc_bits();
                DecodedOp::Branch {
                    taken: predicate(opinfo & BR_MASK, z, n, v)?,
                    target: self.branch_target(pc, bytes, opinfo, false)?,
                }
            }

            // jmp: same target math, just unconditional and absolute
            0x31 => DecodedOp::Branch {
                taken: true,
                target: self.branch_target(pc, bytes, opinfo, true)?,
            },

            // ld / st: dest/src register, base register, then a displacement
            // immediate or an index register. Width and sign-extend live in
            // the flag nibble, same layout as onestep's mem().
            0x40 | 0x41 => {
                let flags = opinfo & 0x0F;
                let reg = self.reg_at(pc, bytes, 2)?;
                let base = self.reg_value(self.reg_at(pc, bytes, 3)?);
                let (offset, _) = self.source(bytes, 4, opinfo);
                let addr = (base as i64).wrapping_add(offset) as u64;
                let width = match (flags >> 1) & 0b11 {
                    0b00 => 8,
                    0b01 => 1,
                    0b10 => 2,
                    _ => 4,
                };
                if opcode == 0x40 {
                    DecodedOp::Load { rd: reg, addr, width, sext: flags & LS_SEXT != 0 }
                } else {
                    DecodedOp::Store { addr, width, value: self.reg_value(reg) }
                }
            }

            // trap: the whole immediate payload is the exception vector
            0xFE => DecodedOp::Trap { vector: imm_at(bytes, 2, isa::imm_len(opinfo, 2)) as u64 },

            _ => return sim_err!("illegal opcode {opcode:#04x} reached decode at {pc:#018x}"),
        })
    }

    // b* / jmp target
    fn branch_target(
        &self,
        pc: u64,
        bytes: &[u8; isa::MAX_INSN_BYTES],
        opinfo: u8,
        absolute: bool,
    ) -> error::Result<u64> {
        Ok(if opinfo & BR_IMM == 0 {
            self.reg_value(self.reg_at(pc, bytes, 2)?)
        } else {
            // force the add to be signed despite PC tracked as u64
            let offset = imm_at(bytes, 2, isa::imm_len(opinfo, 2));
            if absolute { offset as u64 } else { (pc as i64).wrapping_add(offset) as u64 }
        })
    }

    // The trailing source operand starting at `at`: the sign-extended
    // immediate when ALSO_IMMEDIATE is set, otherwise the register named
    // there. Second return value is how many payload bytes it ate - nobody
    // downstream needs it yet since the frame's own length already got baked
    // into the PC back in Fetch, but it's cheap to hand back anyway.
    fn source(&self, bytes: &[u8], at: usize, opinfo: u8) -> (i64, usize) {
        if opinfo & OPINFO_FLAG_ALSO_IMMEDIATE != 0 {
            let n = isa::imm_len(opinfo, at as u8) as usize;
            (imm_at(bytes, at, n as u8), n)
        } else {
            (self.reg_value(bytes[at] as usize) as i64, 1)
        }
    }

    // Pull one register index out of the frame and make sure it's real. The
    // bytes here came off whatever's in memory at `pc`, not off the assembler,
    // so a garbage index is still a possibility.
    fn reg_at(&self, pc: u64, bytes: &[u8], at: usize) -> error::Result<usize> {
        let r = bytes[at] as usize;
        if r >= isa::NUM_REGS {
            return sim_err!("register operand r{r} at {pc:#018x} is out of range (0..{})", isa::NUM_REGS);
        }
        Ok(r)
    }

    // A register's value, forwarded from EX/WB if that's where it actually
    // lives right now. The instruction immediately ahead of us hasn't reached
    // `regs` yet, so we can't safely blindly read from the register file.
    fn reg_value(&self, reg: usize) -> u64 {
        if let Some(ExWb { commit: Commit::Reg { rd, value }, .. }) = self.ex_wb {
            if rd == reg {
                return value as u64;
            }
        }
        self.regs[reg]
    }

    // Same forwarding
    fn cc_bits(&self) -> (bool, bool, bool) {
        if let Some(ExWb { commit: Commit::Flags { z, n, v, .. }, .. }) = self.ex_wb {
            return (z, n, v);
        }
        (self.cc.zero(), self.cc.neg(), self.cc.overflow())
    }
}

// Same as onestep, just taking the bits directly instead of a ConditionCodes
// so the caller can hand it a forwarded value instead of `self.cc`.
fn predicate(pred: u8, z: bool, n: bool, v: bool) -> error::Result<bool> {
    Ok(match pred {
        BR_ALWAYS => true,
        BR_EQ => z,
        BR_NE => !z,
        BR_LT => n != v,
        BR_GE => n == v,
        BR_GT => !z && n == v,
        BR_LE => z || n != v,
        _ => return sim_err!("unknown branch predicate {pred:#x}"),
    })
}

// Sign-extend the low `len` big-endian bytes at `bytes[at..]` back to a full
// i64. Same rule as onestep's fetch_imm, just reading the frame Fetch already
// pulled in instead of going back to memory for it.
fn imm_at(bytes: &[u8], at: usize, len: u8) -> i64 {
    if len == 0 {
        return 0;
    }
    let mut bits = 0u64;
    for i in 0..len as usize {
        bits = (bits << 8) | bytes[at + i] as u64;
    }
    let shift = 64 - len as u32 * 8;
    ((bits << shift) as i64) >> shift
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::assemble_listing;
    use crate::memory;

    fn load(src: &str) -> Processor {
        let (image, _) = assemble_listing(src).expect("assembles");
        memory::reset();
        memory::load(0, &image).expect("loads");
        Processor::new()
    }

    // Fetch then decode - one call, one instruction pushed into ID/EX.
    fn step(p: &mut Processor) -> DecodedOp {
        p.fetch().unwrap();
        p.decode().unwrap();
        p.id_ex.take().expect("frame decoded").op
    }

    #[test]
    fn nop_and_halt_carry_no_operands() {
        let _seq = memory::test_guard();
        let mut p = load("nop\nhalt\n");
        assert_eq!(step(&mut p), DecodedOp::Nop);
        assert_eq!(step(&mut p), DecodedOp::Halt);
        memory::reset();
    }

    #[test]
    fn mov_reads_a_register_or_an_immediate() {
        let _seq = memory::test_guard();
        let mut p = load("mov r1, #5\nmov r2, r1\n");
        assert_eq!(step(&mut p), DecodedOp::Mov { rd: 1, src: 5 });
        p.regs[1] = 5;
        assert_eq!(step(&mut p), DecodedOp::Mov { rd: 2, src: 5 });
        memory::reset();
    }

    #[test]
    fn not_hands_execute_the_source_unnegated() {
        let _seq = memory::test_guard();
        let mut p = load("not r3, r4\n");
        p.regs[4] = 0x0F;
        assert_eq!(step(&mut p), DecodedOp::Not { rd: 3, src: 0x0F });
        memory::reset();
    }

    #[test]
    fn cmp_signedness_follows_the_mnemonic() {
        let _seq = memory::test_guard();
        let mut p = load("cmp r1, r2\ncmp.s r1, #-3\n");
        p.regs[1] = 5;
        p.regs[2] = 9;
        assert_eq!(step(&mut p), DecodedOp::Cmp { a: 5, b: 9, signed: false });
        assert_eq!(step(&mut p), DecodedOp::Cmp { a: 5, b: -3, signed: true });
        memory::reset();
    }

    #[test]
    fn alu_ops_read_both_operands_and_keep_the_op_nibble() {
        let _seq = memory::test_guard();
        let mut p = load("add r2, r1, r1\nsub r1, r1, #1\ndiv r5, r6, r7\n");
        p.regs[1] = 5;
        p.regs[6] = 20;
        p.regs[7] = 4;
        assert_eq!(step(&mut p), DecodedOp::Alu { rd: 2, nibble: 0x0, a: 5, b: 5 });
        assert_eq!(step(&mut p), DecodedOp::Alu { rd: 1, nibble: 0x1, a: 5, b: 1 });
        assert_eq!(step(&mut p), DecodedOp::Alu { rd: 5, nibble: 0xA, a: 20, b: 4 });
        memory::reset();
    }

    #[test]
    fn loads_and_stores_resolve_the_effective_address() {
        let _seq = memory::test_guard();
        let mut p = load(
            "ld r5, [r6]\nld.w r1, [r2 + 0x10]\nld.sb r3, [r4]\nst [r8 - #8], r9\n",
        );
        p.regs[6] = 0x100;
        p.regs[2] = 0x200;
        p.regs[4] = 0x300;
        p.regs[8] = 0x400;
        p.regs[9] = 0xAB;
        assert_eq!(step(&mut p), DecodedOp::Load { rd: 5, addr: 0x100, width: 8, sext: false });
        assert_eq!(step(&mut p), DecodedOp::Load { rd: 1, addr: 0x210, width: 4, sext: false });
        assert_eq!(step(&mut p), DecodedOp::Load { rd: 3, addr: 0x300, width: 1, sext: true });
        // base - 8, so the address math has to actually be signed
        assert_eq!(step(&mut p), DecodedOp::Store { addr: 0x3F8, width: 8, value: 0xAB });
        memory::reset();
    }

    #[test]
    fn load_indexed_by_a_register_reads_it_at_decode_time() {
        let _seq = memory::test_guard();
        let mut p = load("ld r5, [r6 + r7]\n");
        p.regs[6] = 0x100;
        p.regs[7] = 0x20;
        assert_eq!(step(&mut p), DecodedOp::Load { rd: 5, addr: 0x120, width: 8, sext: false });
        memory::reset();
    }

    #[test]
    fn conditional_branch_resolves_taken_from_the_flags() {
        let _seq = memory::test_guard();

        let mut p = load("beq 0x08\n");
        assert_eq!(step(&mut p), DecodedOp::Branch { taken: false, target: 8 });
        memory::reset();

        let mut p = load("beq 0x08\n");
        p.cc = isa::ConditionCodes::new(isa::ConditionCodes::ZERO_MASK).unwrap();
        assert_eq!(step(&mut p), DecodedOp::Branch { taken: true, target: 8 });
        memory::reset();
    }

    #[test]
    fn branch_offset_walks_backward_past_its_own_pc() {
        let _seq = memory::test_guard();
        let mut p = load("loop: sub r1, r1, #1\nb loop\n");
        step(&mut p); // sub, at address 0
        assert_eq!(step(&mut p), DecodedOp::Branch { taken: true, target: 0 });
        memory::reset();
    }

    #[test]
    fn branch_to_a_register_is_always_absolute() {
        let _seq = memory::test_guard();
        let mut p = load("b r1\n");
        p.regs[1] = 0xCAFE_BABE_DEAD_BEEF;
        assert_eq!(step(&mut p), DecodedOp::Branch { taken: true, target: 0xCAFE_BABE_DEAD_BEEF });
        memory::reset();
    }

    #[test]
    fn jmp_is_unconditional_and_absolute() {
        let _seq = memory::test_guard();
        let mut p = load("jmp 0x2000\n");
        assert_eq!(step(&mut p), DecodedOp::Branch { taken: true, target: 0x2000 });
        memory::reset();
    }

    #[test]
    fn no_frame_means_no_decode() {
        let _seq = memory::test_guard();
        let mut p = load("halt\n");
        p.decode().unwrap();
        assert!(p.id_ex.is_none(), "nothing was in IF/ID to decode");
        memory::reset();
    }
}
