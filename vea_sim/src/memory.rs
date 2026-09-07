// memory.rs

/**
 * A flat, byte addressed main memory shared by the whole simulator. Any thread
 * may call read and write. Storage is paged the top 48 bits of an address pick
 * a 64K page and the low 16 bits index into it *in the simulator*. Pages are allocated on first
 * write, so a program that touches a few kilobytes does not allocate the whole
 * 64 bit space. Bytes in a page that was never written read back as zero. Multi
 * byte accesses are big endian to match VEA, work across page boundaries, and
 * return a Fault if the width is unsupported or the access runs off the top of
 * the 64 bit space.
 *
 * The store is process global. Tests that write to it must not run in parallel
 * with each other. Keep such assertions inside a single test function, or gate
 * them behind a shared lock, or revisit this idea for later implementation testing
 * (i.e. multiple read ports from dram).
 *
 */

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::error::Result;

pub const PAGE_BITS: u32 = 16;
pub const PAGE_SIZE: usize = 1 << PAGE_BITS;

static MEM: Mutex<BTreeMap<u64, Box<[u8]>>> = Mutex::new(BTreeMap::new()); // avoid lazylock

// Split an address into its page number and its offset within that page.
fn split(addr: u64) -> (u64, usize) {
    (addr >> PAGE_BITS, (addr as usize) & (PAGE_SIZE - 1))
}

fn blank_page() -> Box<[u8]> {
    vec![0u8; PAGE_SIZE].into_boxed_slice()
}

// Widths the machine can move in a single access.
fn check_width(width: usize) -> Result<()> {
    match width {
        1 | 2 | 4 | 8 => Ok(()),
        _ => crate::sim_err!("access width must be 1, 2, 4 or 8 bytes, got {width}"),
    }
}

// Reject an access of `len` bytes starting at `addr` that runs past the top of
// the address space.
fn check_range(addr: u64, len: usize) -> Result<()> {
    if len > 0 && addr.checked_add(len as u64 - 1).is_none() {
        return crate::sim_err!("access at {addr:#018x} for {len} bytes runs past the address space");
    }
    Ok(())
}

// Clear all of simulated memory.
pub fn clear() {
    MEM.lock().unwrap().clear();
}

// Read `width` bytes at `addr`, big endian, zero extended into a u64.
pub fn read(addr: u64, width: usize) -> Result<u64> {
    check_width(width)?;
    check_range(addr, width)?;
    let mem = MEM.lock().unwrap();
    let mut buf = [0u8; 8];
    for (i, slot) in buf[8 - width..].iter_mut().enumerate() {
        let (page, off) = split(addr.wrapping_add(i as u64));
        if let Some(p) = mem.get(&page) {
            *slot = p[off];
        }
    }
    Ok(u64::from_be_bytes(buf))
}

// Write the low `width` bytes of `val` at `addr`, big endian.
pub fn write(addr: u64, width: usize, val: u64) -> Result<()> {
    check_width(width)?;
    check_range(addr, width)?;
    let mut mem = MEM.lock().unwrap();
    for (i, byte) in val.to_be_bytes()[8 - width..].iter().enumerate() {
        let (page, off) = split(addr.wrapping_add(i as u64));
        mem.entry(page).or_insert_with(blank_page)[off] = *byte;
    }
    Ok(())
}

// Copy a byte image into memory starting at `addr`. Used to load a program.
pub fn load(addr: u64, bytes: &[u8]) -> Result<()> {
    check_range(addr, bytes.len())?;
    let mut mem = MEM.lock().unwrap();
    for (i, b) in bytes.iter().enumerate() {
        let (page, off) = split(addr.wrapping_add(i as u64));
        mem.entry(page).or_insert_with(blank_page)[off] = *b;
    }
    Ok(())
}

// Drop every stored byte. Call between programs or between tests.
pub fn reset() {
    MEM.lock().unwrap().clear();
}

// Read `len` bytes starting at `addr` for display. Unmapped bytes read back as
// zero. Bytes that would fall past the top of the address space are dropped, so
// the result can be shorter than `len`. Unlike `read` this never faults, it is
// only meant to feed a hex view.
pub fn dump(addr: u64, len: usize) -> Vec<u8> {
    let mem = MEM.lock().unwrap();
    (0..len as u64)
        .map_while(|i| addr.checked_add(i))
        .map(|a| {
            let (page, off) = split(a);
            mem.get(&page).map_or(0, |p| p[off])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_load_reset() -> Result<()> {
        reset();

        // never written reads as zero
        assert_eq!(read(0x1000, 8)?, 0);

        // big endian round trip at each width
        write(0x2000, 1, 0xAB)?;
        assert_eq!(read(0x2000, 1)?, 0xAB);

        write(0x2010, 2, 0x1234)?;
        assert_eq!(read(0x2010, 2)?, 0x1234);
        assert_eq!(read(0x2010, 1)?, 0x12); // most significant byte sits first

        write(0x2020, 4, 0xDEAD_BEEF)?;
        assert_eq!(read(0x2020, 4)?, 0xDEAD_BEEF);

        write(0x2030, 8, 0x0123_4567_89AB_CDEF)?;
        assert_eq!(read(0x2030, 8)?, 0x0123_4567_89AB_CDEF);
        assert_eq!(read(0x2034, 4)?, 0x89AB_CDEF); // low word of that store

        // a write only touches the bytes it names
        write(0x3000, 2, 0xFFFF)?;
        assert_eq!(read(0x2FFF, 1)?, 0);
        assert_eq!(read(0x3002, 1)?, 0);

        // bulk image load
        load(0x4000, &[0xCA, 0xFE, 0xBA, 0xBE])?;
        assert_eq!(read(0x4000, 4)?, 0xCAFE_BABE);

        // an access that straddles a page boundary still round trips
        write(0xFFFE, 8, 0x1122_3344_5566_7788)?;
        assert_eq!(read(0xFFFE, 8)?, 0x1122_3344_5566_7788);
        assert_eq!(read(0x1_0000, 2)?, 0x3344); // the bytes that landed in page 1

        reset();
        assert_eq!(read(0x2030, 8)?, 0);
        assert_eq!(read(0x4000, 4)?, 0);
        Ok(())
    }

    #[test]
    fn rejects_unsupported_width() {
        assert!(read(0, 3).is_err());
    }

    #[test]
    fn rejects_access_past_the_top() {
        assert!(read(u64::MAX, 8).is_err());
    }
}
