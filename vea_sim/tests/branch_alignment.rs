// The PC is always a multiple of 4, so a taken branch to any other address must
// fault in both simulators. A branch that is not taken never loads its target,
// so it must not fault.

use vea_sim::{assembler, memory, nstep, onestep};

const CAP: u64 = 200;

// Run a program until it halts or faults. Returns the fault text, if any.
fn run_onestep(source: &str) -> Option<String> {
    memory::reset();
    let (image, _) = assembler::assemble_listing(source).expect("assembles");
    memory::load(0, &image).expect("loads");
    let mut cpu = onestep::Processor::new();
    for _ in 0..CAP {
        if cpu.halted() {
            return None;
        }
        if let Err(e) = cpu.cycle(1) {
            return Some(e.to_string());
        }
    }
    panic!("onestep neither halted nor faulted within {CAP} instructions");
}

fn run_nstep(source: &str) -> Option<String> {
    memory::reset();
    let (image, _) = assembler::assemble_listing(source).expect("assembles");
    memory::load(0, &image).expect("loads");
    let mut cpu = nstep::Processor::new();
    for _ in 0..CAP {
        if cpu.halted() {
            return None;
        }
        if let Err(e) = cpu.cycle(1) {
            return Some(e.to_string());
        }
    }
    panic!("nstep neither halted nor faulted within {CAP} cycles");
}

fn assert_faults(name: &str, source: &str) {
    let _seq = memory::test_guard();
    for (engine, fault) in [("onestep", run_onestep(source)), ("nstep", run_nstep(source))] {
        let msg = fault.unwrap_or_else(|| panic!("{engine}: {name} must fault"));
        assert!(msg.contains("aligned"), "{engine}: {name} faulted for another reason: {msg}");
    }
    memory::reset();
}

fn assert_halts(name: &str, source: &str) {
    let _seq = memory::test_guard();
    for (engine, fault) in [("onestep", run_onestep(source)), ("nstep", run_nstep(source))] {
        assert_eq!(fault, None, "{engine}: {name} must not fault");
    }
    memory::reset();
}

#[test]
fn jmp_to_a_misaligned_register_faults() {
    assert_faults("jmp r1", "mov r1, #6\njmp r1\nhalt\n");
}

#[test]
fn jmp_to_a_misaligned_immediate_faults() {
    assert_faults("jmp 0x6", "jmp 0x6\nhalt\n");
}

#[test]
fn a_taken_branch_to_a_misaligned_register_faults() {
    assert_faults("beq r1", "mov r1, #6\ncmp r1, r1\nbeq r1\nhalt\n");
}

#[test]
fn a_taken_branch_with_a_misaligned_offset_faults() {
    assert_faults("b 0x6", "b 0x6\nhalt\n");
}

#[test]
fn a_branch_that_is_not_taken_ignores_a_misaligned_target() {
    assert_halts("bne r1", "mov r1, #6\ncmp r1, r1\nbne r1\nhalt\n");
}

#[test]
fn a_branch_to_an_aligned_target_still_works() {
    assert_halts("jmp r1", "mov r1, target\njmp r1\nmov r2, #1\ntarget: halt\n");
}
