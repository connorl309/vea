/// Size of the simulated memory, in bytes
pub const MEM_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    Fetch,
}

/// Width of a single load or store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Byte,
    Half,
    Word,
    Long,
}

/// An access that could not be completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    OutOfBounds { addr: u64, len: usize },
    Misaligned { addr: u64, width: Width },
}

/// The whole memory image. The program is loaded at address 0; the rest is
/// zero-filled.
#[derive(Debug, Clone)]
pub struct Memory {
    /// `MEM_SIZE` bytes, big-endian.
    pub bytes: Vec<u8>,
    /// Byte length of the program image at address 0.
    pub image_len: usize,
}