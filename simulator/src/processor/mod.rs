use asm::isa::registers;
pub const REG_COUNT: usize = registers::COUNT as usize;

/// The architectural register file.
pub type RegFile = [u64; REG_COUNT];

/// State that persists from one instruction to the next.
#[derive(Debug, Clone)]
pub struct Core {
    /// r0..r{REG_COUNT-1}.
    pub regs: RegFile,
    /// Program counter. Bit 0 is always 0 and reserved as a tag bit.
    pub pc: u64,
    /// Set once a `halt` retires.
    pub halted: bool,
    /// Instructions retired since the last reset.
    pub retired: u64,
    // TODO: return-address stack / link register for call/rets
}

/// Architectural reset state (all zero)
pub const RESET: Core = Core {
    regs: [0; REG_COUNT],
    pc: 0,
    halted: false,
    retired: 0,
};

/// Something that stops or diverts the core mid-stream. Placeholder set - the
/// real taxonomy depends on the memory and privilege models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    /// A `halt` retired.
    Halt,
    /// Opcode/flags had no entry in the ISA table.
    IllegalInstruction { pc: u64 },
    /// Frame length disagreed with the opcode table, or operands were malformed.
    MalformedInstruction { pc: u64 },
    /// PC was odd.
    MisalignedPc { pc: u64 },
    /// A load or store faulted.
    Memory(crate::memory::Fault),
}