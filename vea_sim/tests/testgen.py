#!/usr/bin/env python3
"""
Generate Rust integration tests from annotated Vea assembly programs.

Run it after adding or editing a tests/*.s file:

    python3 tests/testgen.py

For every tests/<name>.s it (over)writes tests/gen_<name>.rs with ONE `#[test]`
that assembles the program, runs the onestep simulator instruction by
instruction, and checks the register snapshot against the `;=` comments as
their source lines retire. It also writes tests/gen_manifest.rs, whose
`generated_is_fresh` test fails if the .s files change without a regenerate.
Stale gen_*.rs files (for a .s that was removed) are deleted.

--------------------------------------------------------------------------------
Checkpoint syntax
--------------------------------------------------------------------------------
A comment whose first token is  ;=  asserts register values. Values are signed
decimal (`-1`, `246`) or `0xHEX` and are compared as the raw 64-bit pattern.

    <instruction>        ;= rK=V ...      every time that instruction retires
    ;= at N: rK=V ...                     after exactly N retired instructions
    ;= final: rK=V ...                    once the program has halted

An inline checkpoint is tied to the address of its instruction, so it is
checked on every pass. For a value that changes across loop iterations, use
`;= at N:` (N counts retired instructions, so it can name one iteration). A
program with no checkpoints still gets a test that asserts it assembles and
halts.
"""

import glob
import hashlib
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

LABEL_RE = re.compile(r"^\s*([A-Za-z_.][A-Za-z0-9_.]*)\s*:")
ASSERT_RE = re.compile(r"^r(\d+)=(-?(?:0x[0-9A-Fa-f]+|\d+))$")
U64 = (1 << 64) - 1


class GenError(Exception):
    pass


def split_comment(line):
    """Return (code, directive_text_or_None), matching the assembler.

    Comments start at the first ';' or '//'. Only a ';' comment whose body
    begins with '=' is a directive; its text is what follows the '=', up to an
    optional second ';' or '//' that starts an ordinary trailing comment.
    """
    semi = line.find(";")
    slash = line.find("//")
    cuts = [i for i in (semi, slash) if i != -1]
    if not cuts:
        return line, None
    cut = min(cuts)
    code = line[:cut]
    if cut == semi:
        body = line[cut + 1:].lstrip()
        if body.startswith("="):
            directive = body[1:]
            for marker in (";", "//"):
                i = directive.find(marker)
                if i != -1:
                    directive = directive[:i]
            return code, directive.strip()
    return code, None


def strip_labels(code):
    while True:
        m = LABEL_RE.match(code)
        if not m:
            return code
        code = code[m.end():]


MEM_RE = re.compile(r"mem@\s*(0x[0-9A-Fa-f]+|\d+)\s*=\s*\[([^\]]*)\]")


def parse_checks(text, where):
    """Split a directive body into ({reg: value}, [(addr, [bytes])])."""
    mem = []
    for m in MEM_RE.finditer(text):
        addr = int(m.group(1), 0)
        data = []
        for part in m.group(2).split(","):
            part = part.strip()
            if not part:
                continue
            v = int(part, 0)
            if not 0 <= v <= 255:
                raise GenError(f"{where}: byte {part} out of range 0..255")
            data.append(v)
        if not data:
            raise GenError(f"{where}: empty memory slice 'mem@{m.group(1)}'")
        mem.append((addr, data))

    regs = {}
    for tok in MEM_RE.sub(" ", text).split():
        m = ASSERT_RE.match(tok)
        if not m:
            raise GenError(f"{where}: bad assertion '{tok}' (want rK=VALUE or mem@ADDR=[..])")
        reg = int(m.group(1))
        if reg >= 32:
            raise GenError(f"{where}: register r{reg} out of range 0..31")
        regs[reg] = int(m.group(2), 0) & U64

    if not regs and not mem:
        raise GenError(f"{where}: checkpoint has no assertions")
    return regs, mem


class Program:
    def __init__(self):
        self.inline = {}          # instr_index (1-based) -> {reg: value}
        self.inline_line = {}     # instr_index -> source line
        self.inline_mem = []      # [(instr_index, source line, addr, [bytes])]
        self.at = {}              # retired count -> {reg: value}
        self.at_line = {}         # retired count -> source line
        self.final = {}           # {reg: value}
        self.final_line = 0
        self.final_mem = []       # [(source line, addr, [bytes])]


def parse_program(path):
    prog = Program()
    instr_count = 0
    with open(path) as f:
        lines = f.readlines()

    for lineno, raw in enumerate(lines, 1):
        code, directive = split_comment(raw)
        if strip_labels(code).strip():
            instr_count += 1
        if directive is None:
            continue

        where = f"{os.path.basename(path)}:{lineno}"
        if directive.startswith("at ") or directive.startswith("at\t"):
            if ":" not in directive:
                raise GenError(f"{where}: 'at N:' needs a colon")
            head, _, body = directive[3:].partition(":")
            try:
                target = int(head.strip(), 0)
            except ValueError:
                raise GenError(f"{where}: 'at' wants a number, got '{head.strip()}'")
            if target < 1:
                raise GenError(f"{where}: 'at {target}' must be >= 1")
            regs, mem = parse_checks(body, where)
            if mem:
                raise GenError(f"{where}: memory checks are only allowed inline or in 'final:'")
            prog.at.setdefault(target, {}).update(regs)
            prog.at_line[target] = lineno
        elif directive.startswith("final"):
            after = directive[len("final"):].lstrip()
            if not after.startswith(":"):
                raise GenError(f"{where}: 'final' must be written 'final:'")
            regs, mem = parse_checks(after[1:], where)
            if regs:
                prog.final.update(regs)
                prog.final_line = lineno
            prog.final_mem.extend((lineno, addr, data) for addr, data in mem)
        else:
            if instr_count == 0:
                raise GenError(f"{where}: checkpoint before the first instruction")
            regs, mem = parse_checks(directive, where)
            if regs:
                prog.inline.setdefault(instr_count, {}).update(regs)
                prog.inline_line[instr_count] = lineno
            prog.inline_mem.extend((instr_count, lineno, addr, data) for addr, data in mem)

    return prog


# ---- Rust emission -----------------------------------------------------------

OUTPUT = "gen_sim_tests.rs"


def md5(data):
    return hashlib.md5(data).hexdigest()


def reg_slice(regs):
    return "&[" + ", ".join(f"({r}, {regs[r]})" for r in sorted(regs)) + "]"


PRELUDE = '''// @generated by tests/testgen.py - do not edit.
// Regenerate with: python3 tests/testgen.py
//
// One #[test] per tests/*.s program. Each assembles its source, runs the
// onestep simulator one instruction at a time, and checks the register and
// memory snapshot against that program's `;=` comments as their lines retire.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use vea_sim::{assembler, memory, onestep, shared};

// The simulator's memory is process-global, so the program tests run one at a
// time under this lock.
static LOCK: Mutex<()> = Mutex::new(());

const CAP: u64 = 100_000;

// Register checks: (1-based instruction index, source line, &[(reg, value)]).
type Regs = &'static [(usize, u32, &'static [(usize, u64)])];
// `;= at N:` register checks: (retired-instruction count, source line, &[(reg, value)]).
type AtRegs = &'static [(u64, u32, &'static [(usize, u64)])];
// Memory checks: (1-based instruction index, source line, addr, &[expected bytes]).
type Mem = &'static [(usize, u32, u64, &'static [u8])];

macro_rules! src {
    ($name:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/", $name))
    };
}

#[derive(Default)]
struct Prog {
    source: &'static str,
    inline: Regs,                                   // checked each time that instruction retires
    inline_mem: Mem,                                // ditto, a memory slice
    at_count: AtRegs,                               // checked after N retired instructions
    final_regs: (u32, &'static [(usize, u64)]),     // (line, checks) - once halted
    final_mem: &'static [(u32, u64, &'static [u8])], // (line, addr, bytes) - once halted
}

// Assemble `p.source`, run it to halt (or the cap), and assert every checkpoint.
fn check(p: Prog) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    memory::reset();
    let (image, rows) = assembler::assemble_listing(p.source).expect("program assembles");
    memory::load(0, &image).expect("image loads");
    let mut cpu = onestep::Processor::new();

    // Resolve each annotated instruction index to the address it loaded at.
    let addr_of = |idx: usize| rows[idx - 1].addr;
    let inline: Vec<(u64, u32, &[(usize, u64)])> =
        p.inline.iter().map(|&(i, line, c)| (addr_of(i), line, c)).collect();
    let inline_mem: Vec<(u64, u32, u64, &[u8])> =
        p.inline_mem.iter().map(|&(i, line, a, b)| (addr_of(i), line, a, b)).collect();

    let mut steps = 0u64;
    while !cpu.halted() && steps < CAP {
        let pc = cpu.pc;
        cpu.cycle(1).expect("instruction executes without a fault");
        steps += 1;
        let (regs, completed) =
            shared::with(|s| (s.snapshot.regs, s.snapshot.completed_instrs));

        for &(at, line, checks) in &inline {
            if at == pc {
                for &(reg, want) in checks {
                    assert_eq!(regs[reg], want, "line {line}: r{reg} (pc {at:#x})");
                }
            }
        }
        for &(at, line, addr, want) in &inline_mem {
            if at == pc {
                assert_eq!(
                    memory::dump(addr, want.len()).as_slice(), want,
                    "line {line}: mem@{addr:#x}"
                );
            }
        }
        for &(count, line, checks) in p.at_count {
            if count == completed {
                for &(reg, want) in checks {
                    assert_eq!(regs[reg], want, "line {line}: r{reg} at instruction {count}");
                }
            }
        }
    }

    assert!(cpu.halted(), "program did not halt within {CAP} instructions");

    let regs = shared::with(|s| s.snapshot.regs);
    let (line, checks) = p.final_regs;
    for &(reg, want) in checks {
        assert_eq!(regs[reg], want, "line {line}: r{reg} at halt");
    }
    for &(line, addr, want) in p.final_mem {
        assert_eq!(
            memory::dump(addr, want.len()).as_slice(), want,
            "line {line}: mem@{addr:#x} at halt"
        );
    }
}
'''

FRESHNESS = '''
// ---- freshness -----------------------------------------------------------

// (file name, md5sum) for every tests/*.s when these tests were generated.
const SOURCES: &[(&str, &str)] = &[
{rows}
];

// Shell out to md5sum (the same digest the generator baked in).
fn md5(data: &[u8]) -> String {{
    let mut child = Command::new("md5sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn md5sum");
    child.stdin.take().unwrap().write_all(data).expect("write to md5sum");
    let out = child.wait_with_output().expect("md5sum output");
    assert!(out.status.success(), "md5sum exited with {{}}", out.status);
    String::from_utf8(out.stdout)
        .expect("md5sum utf8")
        .split_whitespace()
        .next()
        .expect("md5sum digest")
        .to_string()
}}

#[test]
fn generated_is_fresh() {{
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut actual: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("read tests/")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("s"))
        .map(|p| {{
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, md5(&std::fs::read(&p).expect("read .s")))
        }})
        .collect();
    actual.sort();
    let expected: Vec<(String, String)> =
        SOURCES.iter().map(|&(n, h)| (n.to_string(), h.to_string())).collect();
    assert_eq!(
        actual, expected,
        "\\ntests/*.s changed since the generated tests were built - run: python3 tests/testgen.py\\n"
    );
}}
'''


def byte_slice(data):
    return "&[" + ", ".join(str(b) for b in data) + "]"


def slice_field(name, rows):
    if not rows:
        return None
    body = ",\n".join(f"            {r}" for r in rows)
    return f"        {name}: &[\n{body},\n        ],"


def emit_test(src_name, test_name, prog):
    fields = [f'        source: src!("{src_name}"),']

    fields.append(slice_field("inline", [
        f"({idx}, {prog.inline_line[idx]}, {reg_slice(prog.inline[idx])})"
        for idx in sorted(prog.inline)
    ]))
    fields.append(slice_field("inline_mem", [
        f"({idx}, {ln}, {addr:#x}, {byte_slice(data)})"
        for idx, ln, addr, data in sorted(prog.inline_mem)
    ]))
    fields.append(slice_field("at_count", [
        f"({count}, {prog.at_line[count]}, {reg_slice(prog.at[count])})"
        for count in sorted(prog.at)
    ]))
    if prog.final:
        fields.append(f"        final_regs: ({prog.final_line}, {reg_slice(prog.final)}),")
    fields.append(slice_field("final_mem", [
        f"({ln}, {addr:#x}, {byte_slice(data)})"
        for ln, addr, data in sorted(prog.final_mem)
    ]))

    body = "\n".join(f for f in fields if f)
    return (
        f"\n#[test]\nfn {test_name}() {{\n"
        f"    check(Prog {{\n{body}\n        ..Default::default()\n    }});\n}}\n"
    )


def emit_monolith(programs, manifest_rows):
    parts = [PRELUDE]
    for src_name, test_name, prog in programs:
        parts.append(emit_test(src_name, test_name, prog))
    parts.append(FRESHNESS.format(rows="\n".join(manifest_rows)))
    return "".join(parts)


def ident(stem):
    name = re.sub(r"[^0-9A-Za-z]", "_", stem)
    return name if name[:1].isalpha() or name[:1] == "_" else f"t_{name}"


def main():
    sources = sorted(glob.glob(os.path.join(HERE, "*.s")))

    # Parse everything first so a bad file leaves the existing output untouched.
    parsed, errors = [], []
    for path in sources:
        try:
            parsed.append((path, parse_program(path)))
        except GenError as e:
            errors.append(str(e))
    if errors:
        for e in errors:
            print(f"testgen: {e}", file=sys.stderr)
        return 1

    for stale in glob.glob(os.path.join(HERE, "gen_*.rs")):
        os.remove(stale)

    programs, manifest_rows = [], []
    for path, prog in parsed:
        src_name = os.path.basename(path)
        programs.append((src_name, ident(os.path.splitext(src_name)[0]), prog))
        with open(path, "rb") as f:
            manifest_rows.append(f'    ("{src_name}", "{md5(f.read())}"),')
        checks = len(prog.inline) + len(prog.at) + (1 if prog.final else 0)
        print(f"testgen: {src_name} ({checks} checkpoints)")

    with open(os.path.join(HERE, OUTPUT), "w") as f:
        f.write(emit_monolith(programs, manifest_rows))
    print(f"testgen: {len(programs)} program(s) -> {OUTPUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
