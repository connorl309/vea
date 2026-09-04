// Property test: generate random-but-valid instructions as source text,
// assemble them, and compare against bytes built a second, independent way
// (no calls into the crate's encoder or framing helpers).
//
// On failure the panic prints the exact source that broke - paste it into a
// focused test to reproduce.

use asm::assemble;
use asm::isa::Form;
use asm::isa::opcodes::INSTRUCTIONS;

// Independent minimal width: 0/1/2/4/8 bytes, signed-or-unsigned reading.
fn ref_width(v: i128) -> u8 {
    if v == 0 {
        0
    } else if (-0x80..=0xFF).contains(&v) {
        1
    } else if (-0x8000..=0xFFFF).contains(&v) {
        2
    } else if (-0x8000_0000..=0xFFFF_FFFF).contains(&v) {
        4
    } else {
        8
    }
}

// Independent big-endian encoding via shift-and-mask.
fn ref_be(v: i128, width: u8) -> Vec<u8> {
    (0..width).rev().map(|i| (v >> (i as u32 * 8)) as u8).collect()
}

// Independent frame layout: [opcode][PLEN<<4 | FLAGS][regs][imm].
fn ref_frame(opcode: u8, flags: u8, regs: &[u8], imm: &[u8]) -> Vec<u8> {
    let plen = (regs.len() + imm.len()) as u8;
    let mut v = vec![opcode, (plen << 4) | (flags & 0x0F)];
    v.extend_from_slice(regs);
    v.extend_from_slice(imm);
    v
}

fn rand_reg() -> u8 {
    rand::random_range(0..32)
}

// An immediate somewhere in one of the encodable width buckets.
fn rand_imm() -> i128 {
    match rand::random_range(0..7) {
        0 => 0,
        1 => rand::random_range(-0x80i128..0x100),
        2 => rand::random_range(-0x8000i128..0x1_0000),
        3 => rand::random_range(-0x8000_0000i128..0x1_0000_0000),
        4 => rand::random_range(-(1i128 << 63)..(1i128 << 63)),
        5 => rand::random_range(-500i128..0),
        _ => rand::random_range(0i128..(1i128 << 64)), // full unsigned 64-bit
    }
}

fn render_imm(v: i128) -> String {
    let bare = if v >= 0 && rand::random_range(0..3) == 0 {
        format!("0x{v:x}")
    } else {
        format!("{v}")
    };
    if rand::random() { format!("#{bare}") } else { bare }
}

// Build one instruction: its source line and the bytes it should encode to.
fn gen_one() -> (String, Vec<u8>) {
    let def = &INSTRUCTIONS[rand::random_range(0..INSTRUCTIONS.len())];
    let m = def.mnemonic;
    let (op, fl) = (def.opcode, def.flags);

    match def.form {
        Form::Nullary => (m.to_string(), ref_frame(op, fl, &[], &[])),
        Form::R => {
            let a = rand_reg();
            (format!("{m} r{a}"), ref_frame(op, fl, &[a], &[]))
        }
        Form::RR => {
            let (a, b) = (rand_reg(), rand_reg());
            (format!("{m} r{a}, r{b}"), ref_frame(op, fl, &[a, b], &[]))
        }
        Form::RRR => {
            let (a, b, c) = (rand_reg(), rand_reg(), rand_reg());
            (format!("{m} r{a}, r{b}, r{c}"), ref_frame(op, fl, &[a, b, c], &[]))
        }
        Form::RI => {
            let a = rand_reg();
            let v = rand_imm();
            let bytes = ref_frame(op, fl, &[a], &ref_be(v, ref_width(v)));
            (format!("{m} r{a}, {}", render_imm(v)), bytes)
        }
        Form::RRI => {
            let (a, b) = (rand_reg(), rand_reg());
            let v = rand_imm();
            let bytes = ref_frame(op, fl, &[a, b], &ref_be(v, ref_width(v)));
            (format!("{m} r{a}, r{b}, {}", render_imm(v)), bytes)
        }
        Form::I => {
            let v = rand_imm();
            let bytes = ref_frame(op, fl, &[], &ref_be(v, ref_width(v)));
            (format!("{m} {}", render_imm(v)), bytes)
        }
        Form::RMem => {
            let (a, b) = (rand_reg(), rand_reg());
            let d = rand_imm();
            let inner = if d == 0 {
                format!("r{b}")
            } else if d > 0 {
                format!("r{b} + {d}")
            } else {
                format!("r{b} - {}", -d)
            };
            let bytes = ref_frame(op, fl, &[a, b], &ref_be(d, ref_width(d)));
            (format!("{m} r{a}, [{inner}]"), bytes)
        }
    }
}

#[test]
fn random_programs_round_trip() {
    // `VERBOSE=1 cargo test --test fuzz_encoding -- --nocapture`
    let verbose = std::env::var_os("VERBOSE").is_some();

    for round in 0..500 {
        let count = rand::random_range(1..20);
        let mut src = String::new();
        let mut expected: Vec<u8> = Vec::new();

        for _ in 0..count {
            let (line, mut frame) = gen_one();
            while expected.len() % 2 != 0 {
                expected.push(0x00); // 2-byte instruction alignment
            }
            expected.append(&mut frame);
            src.push_str(&line);
            src.push('\n');
        }

        let got = assemble(&src)
            .unwrap_or_else(|e| panic!("\n--- source ---\n{src}--- error ---\n{e}\n"))
            .bytes;

        if verbose {
            println!("--- round {round} ({count} instrs) ---");
            println!("=> {}\n", hex(&got));
        }

        assert_eq!(got, expected, "\n--- source ---\n{src}");
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x} ")).collect()
}
