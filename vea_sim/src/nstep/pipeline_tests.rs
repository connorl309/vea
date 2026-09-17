use super::Processor;
use crate::assembler::assemble_listing;
use crate::memory;

fn load(src: &str) -> Processor {
    let (image, _) = assemble_listing(src).expect("assembles");
    memory::reset();
    memory::load(0, &image).expect("loads");
    Processor::new()
}

// Fetch -> Decode -> Execute -> Writeback is 4 cycles deep, so the very first
// instruction can't retire before the 4th tick, and every instruction after
// it retires exactly one cycle later than the one before - one instruction
// per cycle, forever, since nothing here stalls.
#[test]
fn an_instruction_retires_on_its_fourth_cycle_then_one_per_cycle() {
    let _seq = memory::test_guard();
    let mut p = load("mov r1, #1\nmov r2, #2\nmov r3, #3\nhalt\n");

    for cycle in 1..=3 {
        p.tick().unwrap();
        assert_eq!(p.completed_instrs, 0, "nothing should retire before cycle 4 (at cycle {cycle})");
    }
    p.tick().unwrap();
    assert_eq!(p.completed_instrs, 1, "mov r1 retires on cycle 4");
    assert_eq!(p.regs[1], 1);

    p.tick().unwrap();
    assert_eq!(p.completed_instrs, 2, "one more retires every cycle after fill");
    assert_eq!(p.regs[2], 2);

    p.tick().unwrap();
    assert_eq!(p.completed_instrs, 3);
    assert_eq!(p.regs[3], 3);

    memory::reset();
}

#[test]
fn halt_lets_in_flight_work_land_before_stopping() {
    let _seq = memory::test_guard();
    let mut p = load("mov r1, #1\nhalt\n");

    p.cycle(20).expect("no fault");
    assert!(p.halted());
    assert_eq!(p.regs[1], 1, "mov's result must not be dropped by the halt behind it");
    // mov (cycle 4) + halt (cycle 5) = 2 retired instructions.
    assert_eq!(p.completed_instrs, 2);

    memory::reset();
}

#[test]
fn nothing_past_a_halt_ever_executes() {
    let _seq = memory::test_guard();
    let mut p = load("mov r1, 0x2000\nmov r2, 0xAB\nhalt\nst [r1], r2\n");
    // Seed the address the store would target with a sentinel distinct from
    // anything the store could actually produce (r2 = 0xAB). If the store
    // after halt ever ran, this would get clobbered.
    memory::write(0x2000, 8, 0xDEAD_BEEF_CAFE_BABE).expect("seeds");

    p.cycle(20).expect("no fault");
    assert!(p.halted());
    assert_eq!(
        memory::dump(0x2000, 8),
        0xDEAD_BEEF_CAFE_BABEu64.to_be_bytes().to_vec(),
        "the store after halt must never run"
    );

    memory::reset();
}

// Same case, but the instruction behind halt is one that would fault (divide
// by zero) if it ever reached Execute. If halt didn't actually stop the front
// end, this would surface as an unexpected error instead of a clean halt.
#[test]
fn a_faulting_instruction_past_halt_never_gets_the_chance() {
    let _seq = memory::test_guard();
    let mut p = load("halt\ndiv r1, r2, r3\n");

    p.cycle(20).expect("halt must not let the div behind it fault");
    assert!(p.halted());

    memory::reset();
}

// Backward branch
#[test]
fn a_backward_branch_loop_runs_to_completion() {
    let _seq = memory::test_guard();
    let mut p = load("mov r1, #3\nloop: sub r1, r1, #1\ncmp r1, #0\nbne loop\nhalt\n");

    p.cycle(200).expect("no fault");
    assert!(p.halted(), "loop must terminate and reach halt");
    assert_eq!(p.regs[1], 0);

    memory::reset();
}

// Conditional branch
#[test]
fn conditional_branch_changes_architectural_state_when_taken() {
    let _seq = memory::test_guard();

    let mut not_taken = load("mov r1, #1\ncmp r1, #0\nbeq skip\nmov r2, #99\nskip: halt\n");
    not_taken.cycle(200).expect("no fault");
    assert!(not_taken.halted());
    assert_eq!(not_taken.regs[2], 99, "beq not taken: the skipped mov still runs");
    memory::reset();

    let mut taken = load("mov r1, #0\ncmp r1, #0\nbeq skip\nmov r2, #99\nskip: halt\n");
    taken.cycle(200).expect("no fault");
    assert!(taken.halted());
    assert_eq!(taken.regs[2], 0, "beq taken: the skipped mov must not run");
    memory::reset();
}

// A register-target jump
#[test]
fn register_target_jump_lands_where_the_register_points() {
    let _seq = memory::test_guard();
    let mut p = load("mov r1, target\njmp r1\nmov r2, #1\ntarget: mov r3, #7\nhalt\n");

    p.cycle(200).expect("no fault");
    assert!(p.halted());
    assert_eq!(p.regs[2], 0, "the instruction jmp skipped over must not run");
    assert_eq!(p.regs[3], 7);

    memory::reset();
}

#[test]
fn store_then_load_round_trip() {
    let _seq = memory::test_guard();
    let mut p = load("mov r1, 0x3000\nmov r2, 0x1122\nst [r1], r2\nld r3, [r1]\nhalt\n");

    p.cycle(200).expect("no fault");
    assert!(p.halted());
    // Plain st is 8 bytes wide, big endian, so 0x1122 lands in the low two
    // bytes of that window
    assert_eq!(memory::dump(0x3000, 8), vec![0, 0, 0, 0, 0, 0, 0x11, 0x22]);
    assert_eq!(p.regs[3], 0x1122);

    memory::reset();
}

// A narrow, sign-extended load has to come back through the whole pipe with
// the sign bit actually extended, not just the raw byte.
#[test]
fn signed_narrow_load_sign_extends_through_writeback() {
    let _seq = memory::test_guard();
    let mut p = load("mov r1, 0x3000\nmov r2, #-1\nst.b [r1], r2\nld.sb r3, [r1]\nhalt\n");

    p.cycle(200).expect("no fault");
    assert!(p.halted());
    assert_eq!(p.regs[3] as i64, -1);

    memory::reset();
}

// Faults propagate out of `cycle()` as an Err
#[test]
fn divide_by_zero_faults_the_cycle_call() {
    let _seq = memory::test_guard();
    let mut p = load("mov r2, #0\ndiv r1, r2, r2\nhalt\n");

    let err = p.cycle(20);
    assert!(err.is_err(), "dividing by zero must fault, not silently continue");

    memory::reset();
}

// A jump off into nowhere has to fault fetch, same as onestep running off the
// end of its program.
#[test]
fn jumping_off_mapped_memory_faults() {
    let _seq = memory::test_guard();
    let mut p = load("jmp 0x100000\nhalt\n");

    let err = p.cycle(20);
    assert!(err.is_err(), "a jump into an unmapped page must fault");

    memory::reset();
}
