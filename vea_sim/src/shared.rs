// shared.rs

/**
 * The bridge between the simulator and the interactive UI.
 *
 * `onestep` publishes a plain-data snapshot of the machine here after every
 * `cycle`; the `ui` module reads it back when it draws. Neither side hands the
 * other any of its own types: the simulator never sees a widget, and the UI
 * never touches the `Processor` internals. One process-global `Mutex` holds it
 * for the life of the run, mirroring how `memory` keeps its store.
 */

use std::sync::Mutex;

use crate::assembler::ListingRow;
use crate::isa::NUM_REGS;

// The four architectural condition-code bits, unpacked for the UI.
#[derive(Clone, Copy)]
pub struct Flags {
    pub z: bool,
    pub n: bool,
    pub c: bool,
    pub v: bool,
}

impl Flags {
    const CLEAR: Flags = Flags { z: false, n: false, c: false, v: false };
}

// One occupied pipeline latch: the pc of the instruction sitting there and a
// short human-readable description of what it holds. Only nstep populates
// these - the UI never sees a `DecodedOp` or a `Commit`, just the string each
// one formats itself as.
#[derive(Clone)]
pub struct StageSlot {
    pub pc: u64,
    pub desc: String,
}

// What's currently in flight in nstep's pipeline, one slot per latch. A slot
// of `None` is a bubble - nothing fetched/decoded/executed that cycle.
#[derive(Clone)]
pub struct Pipeline {
    pub if_id: Option<StageSlot>,
    pub id_ex: Option<StageSlot>,
    pub ex_wb: Option<StageSlot>,
}

// Everything the UI shows about the running machine
#[derive(Clone)]
pub struct Snapshot {
    pub pc: u64,
    pub regs: [u64; NUM_REGS],
    pub flags: Flags,
    pub completed_instrs: u64,
    // Clock cycles spent to reach `completed_instrs`. Comparable across
    // engines: onestep charges 4 per instruction (fetch/decode/execute/
    // writeback done serially, one "cycle"), nstep counts the actual ticks -
    // same total work, so this is where pipelining's payoff actually shows.
    pub cycles: u64,
    pub halted: bool,
    // Set when the last `cycle` ended on a fault; carries the message.
    pub fault: Option<String>,
    // `Some` only from nstep. onestep retires one instruction per cycle and
    // has nothing pipeline-shaped to report, so this is how the UI tells
    // which engine published the snapshot without a separate flag.
    pub pipeline: Option<Pipeline>,
}

impl Snapshot {
    const INITIAL: Snapshot = Snapshot {
        pc: 0,
        regs: [0; NUM_REGS],
        flags: Flags::CLEAR,
        completed_instrs: 0,
        cycles: 0,
        halted: false,
        fault: None,
        pipeline: None,
    };
}

// The whole shared state: the loaded program plus the latest machine snapshot.
pub struct Shared {
    // Bumped on every mutation so the UI can skip redraws when nothing changed.
    pub generation: u64,
    pub snapshot: Snapshot,
    pub program: Vec<ListingRow>,
    pub load_addr: u64,
    // Path the program was loaded from, for the UI to label.
    pub source: Option<String>,
}

impl Shared {
    const INITIAL: Shared = Shared {
        generation: 0,
        snapshot: Snapshot::INITIAL,
        program: Vec::new(),
        load_addr: 0,
        source: None,
    };
}

static SHARED: Mutex<Shared> = Mutex::new(Shared::INITIAL);

// Read the shared state under the lock. Keep the closure short: the UI thread
// holds this only for the length of one frame.
pub fn with<R>(f: impl FnOnce(&Shared) -> R) -> R {
    f(&SHARED.lock().unwrap())
}

// Replace the published machine snapshot. Called from `Processor::cycle`.
pub fn publish(snapshot: Snapshot) {
    let mut s = SHARED.lock().unwrap();
    s.generation = s.generation.wrapping_add(1);
    s.snapshot = snapshot;
}

// Install a freshly assembled program, the address it was loaded at, and the
// path it came from. Called whenever the UI opens or reloads a file.
pub fn install_program(program: Vec<ListingRow>, load_addr: u64, source: String) {
    let mut s = SHARED.lock().unwrap();
    s.generation = s.generation.wrapping_add(1);
    s.program = program;
    s.load_addr = load_addr;
    s.source = Some(source);
}
