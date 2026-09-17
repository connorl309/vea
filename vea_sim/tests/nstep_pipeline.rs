// Whole-program regression for the pipelined simulator, cross-checked against
// onestep instead of hand-derived expected values. onestep is the trusted
// reference (see onestep/mod.rs and tests/gen_sim_tests.rs, which already
// verifies it instruction-by-instruction against these same fixtures); nstep
// just adds pipeline latency on top of the identical instruction semantics,
// so given enough cycles the two must land on exactly the same architectural
// state. This is a much stronger check than re-deriving expected registers by
// hand - it exercises forwarding, branch redirect timing, and the halt drain
// against real, non-trivial programs instead of only the small hand-built
// cases in nstep::pipeline_tests.

use vea_sim::{assembler, memory, nstep, onestep, shared};

const CAP: u64 = 200_000;

macro_rules! src {
    ($name:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/", $name))
    };
}

fn run_onestep(source: &str) -> shared::Snapshot {
    memory::reset();
    let (image, _) = assembler::assemble_listing(source).expect("assembles");
    memory::load(0, &image).expect("loads");
    let mut cpu = onestep::Processor::new();
    let mut steps = 0u64;
    while !cpu.halted() && steps < CAP {
        cpu.cycle(1).expect("onestep executes without a fault");
        steps += 1;
    }
    assert!(cpu.halted(), "onestep did not halt within {CAP} instructions");
    shared::with(|s| s.snapshot.clone())
}

fn run_nstep(source: &str) -> shared::Snapshot {
    memory::reset();
    let (image, _) = assembler::assemble_listing(source).expect("assembles");
    memory::load(0, &image).expect("loads");
    let mut cpu = nstep::Processor::new();
    let mut steps = 0u64;
    while !cpu.halted() && steps < CAP {
        cpu.cycle(1).expect("nstep executes without a fault");
        steps += 1;
    }
    assert!(cpu.halted(), "nstep did not halt within {CAP} cycles");
    shared::with(|s| s.snapshot.clone())
}

// Run the same source on both simulators and demand identical final state.
// `memory::reset()` happens inside each run, so this needs its own guard
// spanning both - a test interleaving with this one under the same mutex
// could otherwise see one simulator's image mid-load.
fn cross_check(name: &str, source: &str) {
    let _seq = memory::test_guard();

    let one = run_onestep(source);
    let n = run_nstep(source);

    assert_eq!(n.regs, one.regs, "{name}: register file mismatch");
    assert_eq!(n.pc, one.pc, "{name}: final pc mismatch");
    assert_eq!(n.completed_instrs, one.completed_instrs, "{name}: retired-instruction count mismatch");
    assert_eq!((n.flags.z, n.flags.n, n.flags.c, n.flags.v),
        (one.flags.z, one.flags.n, one.flags.c, one.flags.v), "{name}: flags mismatch");
}

#[test]
fn bitops() {
    cross_check("bitops", src!("bitops.s"));
}

#[test]
fn branch_predicates() {
    cross_check("branch_predicates", src!("branch_predicates.s"));
}

#[test]
fn bubble_sort() {
    cross_check("bubble_sort", src!("bubble_sort.s"));
}

#[test]
fn factorial() {
    cross_check("factorial", src!("factorial.s"));
}

#[test]
fn fibonacci() {
    cross_check("fibonacci", src!("fibonacci.s"));
}

#[test]
fn mem_stress() {
    cross_check("mem_stress", src!("mem_stress.s"));
}

#[test]
fn memcpy() {
    cross_check("memcpy", src!("memcpy.s"));
}

#[test]
fn run_all_instrs_all_modes() {
    cross_check("run_all_instrs_all_modes", src!("run_all_instrs_all_modes.s"));
}

#[test]
fn sample_basics() {
    cross_check("sample_basics", src!("sample_basics.s"));
}

#[test]
fn signed_vs_unsigned() {
    cross_check("signed_vs_unsigned", src!("signed_vs_unsigned.s"));
}

#[test]
fn strlen() {
    cross_check("strlen", src!("strlen.s"));
}

#[test]
fn sum_1_to_n() {
    cross_check("sum_1_to_n", src!("sum_1_to_n.s"));
}
