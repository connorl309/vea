// isa.rs

/**
 * This file contains various constants/fixed structures
 * about the ISA that may be useful in other code locations.
 */

// How many registers does Vea support?
pub const NUM_REGS: usize = 32;
// Arbitrary constant identifying the PC register which
// is not typically exposed anywhere.
pub const PC_REG: usize = NUM_REGS + 1;
pub struct VeaReg(u8);
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

// How large is the i- and d-cache?
// TODO: Sim modeling for caches
pub const ICACHE_SIZE: usize = usize::MIN;
pub const DCACHE_SIZE: usize = usize::MIN;

// How many cycles will modeled (fake) memory
// stall for in sim?
pub const MEM_READ_DELAY: u64 = 3;
pub const MEM_WRITE_DELAY: u64 = 3;

// 