// isa.rs

/**
 * This file contains various constants/fixed structures
 * about the ISA that may be useful in other code locations.
 */

// To what byte-alignment is every instruction?
// For now, it is 4 byte aligned, so every fetch
// will PC = (PC + plen) rounded up to the next multiple of 4.
pub const ALIGNMENT: u64 = 0x4;
// How many registers does Vea support?
pub const NUM_REGS: usize = 32;
pub type RegisterFile = [u64; NUM_REGS];
// Arbitrary constant identifying the PC register which
// is not typically exposed anywhere.
pub const PC_REG: usize = NUM_REGS + 1;
// What is a register in Vea? (wrapper around a u8...)
pub struct VeaReg(u8);
// What are our condition codes?
pub struct ConditionCodes {
    data: u8
}

// The widest instruction frame in VEA.
// Fetch always pulls a whole frame so Decode sees every byte it might need.
pub const MAX_INSN_BYTES: usize = 13;

// Instruction cache: a sliding window of program bytes Fetch streams through,
// refilled when the PC nears the end. See nstep::icache.
// TODO: d-cache modeling for loads/stores
pub const ICACHE_SIZE: usize = 512;
pub const DCACHE_SIZE: usize = usize::MIN;

// How many cycles will modeled (fake) memory
// stall for in sim?
pub const MEM_READ_DELAY: u64 = 3;
pub const MEM_WRITE_DELAY: u64 = 3;

/*
==========================================================

    IMPLEMENTATIONS

==========================================================
*/
impl VeaReg {
    pub fn is_valid(&self) -> bool {
        self.0 < NUM_REGS as u8 || self.0 == PC_REG as u8
    }
    pub fn name(&self) -> String {
        if self.0 == PC_REG as u8 {
            String::from("PC")
        } else {
            format!("r{}", self.0)
        }
    }
    pub fn idx(&self) -> usize { self.0 as usize }
}

// Round `v` up to the next multiple of `a`. Instruction addresses advance by
// this rule with `a == ALIGNMENT`, matching the assembler's per-instruction
// padding.
pub fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) / a * a
}

// Condition Codes
impl ConditionCodes {
    pub const ZERO_MASK: u8 = 0b1000;
    pub const NEG_MASK: u8 = 0b0100;
    pub const CARRY_MASK: u8 = 0b0010;
    pub const OVERFLOW_MASK: u8 = 0b0001;

    pub fn new(masks: u8) -> crate::error::Result<Self> {
        if masks > 0xF {
            return crate::sim_err!("condition code mask {masks:#06b} has undefined bits set");
        }
        Ok(ConditionCodes { data: masks })
    }
    pub fn reset() -> Self {
        ConditionCodes { data: 0 }
    }

    // Overwrite all four flags at once as needed
    pub fn set(&mut self, zero: bool, neg: bool, carry: bool, overflow: bool) {
        let bit = |on, mask| if on { mask } else { 0 };
        self.data = bit(zero, Self::ZERO_MASK)
            | bit(neg, Self::NEG_MASK)
            | bit(carry, Self::CARRY_MASK)
            | bit(overflow, Self::OVERFLOW_MASK);
    }

    pub fn zero(&self) -> bool { self.data & Self::ZERO_MASK != 0 }
    pub fn neg(&self) -> bool { self.data & Self::NEG_MASK != 0 }
    pub fn carry(&self) -> bool { self.data & Self::CARRY_MASK != 0 }
    pub fn overflow(&self) -> bool { self.data & Self::OVERFLOW_MASK != 0 }
}