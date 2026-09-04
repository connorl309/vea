use std::sync::{LazyLock, RwLock};

// Size of the simulated memory, in bytes
pub const MEM_SIZE: usize = 64 * 1024;

// This is kinda like how it'll work on real hardware if I actually thought about
// designing some form of concurrency management in hardware. But it's here just in case.
// Oo, fancy!
static MEMORY: LazyLock<RwLock<Memory>> = LazyLock::new(|| RwLock::new(Memory::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

// Width of a single load or store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Byte,
    Half,
    Word,
    Long,
}

impl Width {
    // Byte length of an access of this width.
    pub const fn bytes(self) -> u64 {
        match self {
            Width::Byte => 1,
            Width::Half => 2,
            Width::Word => 4,
            Width::Long => 8,
        }
    }
}

// An access that could not be completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    OutOfBoundsAccess { addr: u64, len: usize },
}

// The whole memory image. The program is loaded at address 0; the rest is
// zero-filled. Private on purpose - go through the module functions.
#[derive(Debug, Clone)]
struct Memory {
    // `MEM_SIZE` bytes, big-endian.
    bytes: Vec<u8>,
    // Byte length of the program image at address 0.
    image_len: usize,
}

impl Memory {
    fn new() -> Self {
        Memory { bytes: vec![0u8; MEM_SIZE], image_len: 0 }
    }
}

// --- shared-memory interface -------------------------------------------------

// Read `num_bytes` bytes from the shared memory starting at `addr`, in address
// order.
pub fn read(addr: u64, num_bytes: u64) -> Result<Vec<u8>, Fault> {
    let (lo, hi) = span(addr, num_bytes).ok_or(Fault::OutOfBoundsAccess {
        addr,
        len: num_bytes as usize,
    })?;
    Ok(MEMORY.read().unwrap().bytes[lo..hi].to_vec())
}

// Write `data` to the shared memory starting at `addr`, in address order.
pub fn write(addr: u64, data: &[u8]) -> Result<(), Fault> {
    let (lo, hi) = span(addr, data.len() as u64).ok_or(Fault::OutOfBoundsAccess {
        addr,
        len: data.len(),
    })?;
    MEMORY.write().unwrap().bytes[lo..hi].copy_from_slice(data);
    Ok(())
}

// Load a program image at address 0, zero the rest, and record its length.
pub fn load_image(image: &[u8]) {
    let n = image.len().min(MEM_SIZE);
    let mut mem = MEMORY.write().unwrap();
    mem.bytes.iter_mut().for_each(|b| *b = 0);
    mem.bytes[..n].copy_from_slice(&image[..n]);
    mem.image_len = n;
}

// Byte length of the program image currently sitting at address 0.
pub fn image_len() -> usize {
    MEMORY.read().unwrap().image_len
}

// A copy of the whole image - for the UI's hex view.
pub fn snapshot() -> Vec<u8> {
    MEMORY.read().unwrap().bytes.clone()
}

// Turn an (addr, len) access into a validated `lo..hi` byte range, or `None`
// if it would run off the end of memory.
fn span(addr: u64, len: u64) -> Option<(usize, usize)> {
    let end = addr.checked_add(len)?;
    (end <= MEM_SIZE as u64).then(|| (addr as usize, end as usize))
}
