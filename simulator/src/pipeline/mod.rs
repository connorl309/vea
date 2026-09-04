/// The pipeline stages. Placeholder as the microarchitecture is not designed
/// yet, so the count and names will change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Fetch,
    Decode,
    Execute,
    Memory,
    Writeback,
}
