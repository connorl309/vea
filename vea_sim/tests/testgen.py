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
    begins with '=' is a directive; its text is what follows the '='.
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
            return code, body[1:].strip()
    return code, None


def strip_labels(code):
    while True:
        m = LABEL_RE.match(code)
        if not m:
            return code
        code = code[m.end():]


def parse_asserts(text, where):
    out = {}
    for tok in text.split():
        m = ASSERT_RE.match(tok)
        if not m:
            raise GenError(f"{where}: bad assertion '{tok}' (want rK=VALUE)")
        reg = int(m.group(1))
        if reg >= 32:
            raise GenError(f"{where}: register r{reg} out of range 0..31")
        out[reg] = int(m.group(2), 0) & U64
    return out


class Program:
    def __init__(self):
        self.inline = {}       # instr_index (1-based) -> {reg: value}
        self.inline_line = {}  # instr_index -> source line
        self.at = {}           # retired count -> {reg: value}
        self.at_line = {}      # retired count -> source line
        self.final = {}        # {reg: value}
        self.final_line = 0


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
            prog.at.setdefault(target, {}).update(parse_asserts(body, where))
            prog.at_line[target] = lineno
        elif directive.startswith("final"):
            after = directive[len("final"):].lstrip()
            if not after.startswith(":"):
                raise GenError(f"{where}: 'final' must be written 'final:'")
            prog.final.update(parse_asserts(after[1:], where))
            prog.final_line = lineno
        else:
            if instr_count == 0:
                raise GenError(f"{where}: checkpoint before the first instruction")
            prog.inline.setdefault(instr_count, {}).update(parse_asserts(directive, where))
            prog.inline_line[instr_count] = lineno

    return prog


# ---- Rust emission -----------------------------------------------------------

def reg_slice(regs):
    return "&[" + ", ".join(f"({r}, {regs[r]})" for r in sorted(regs)) + "]"


def emit_program(src_name, test_name, prog):
    inline_rows = "\n".join(
        f"    ({idx}, {prog.inline_line[idx]}, {reg_slice(prog.inline[idx])}),"
        for idx in sorted(prog.inline)
    )
    at_rows = "\n".join(
        f"    ({count}, {prog.at_line[count]}, {reg_slice(prog.at[count])}),"
        for count in sorted(prog.at)
    )
    final_row = f"({prog.final_line}, {reg_slice(prog.final)})"

    return f"""// @generated by tests/testgen.py from tests/{src_name} - do not edit.
// Regenerate with: python3 tests/testgen.py

use std::sync::Mutex;

use vea_sim::{{assembler, memory, onestep, shared}};

// Tests in one file share a process and the global simulator memory, so they
// run one at a time. Separate .s files are separate test binaries (separate
// processes) and do not contend.
static LOCK: Mutex<()> = Mutex::new(());

const SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/{src_name}"));

// (1-based instruction index, source line, [(reg, value)]) - checked every time
// that instruction retires.
const INLINE: &[(usize, u32, &[(usize, u64)])] = &[
{inline_rows}
];

// (retired-instruction count, source line, [(reg, value)]) - from `;= at N:`.
const AT_COUNT: &[(u64, u32, &[(usize, u64)])] = &[
{at_rows}
];

// (source line, [(reg, value)]) - from `;= final:`, checked once halted.
const FINAL: (u32, &[(usize, u64)]) = {final_row};

const CAP: u64 = 100_000;

#[test]
fn {test_name}() {{
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    memory::reset();
    let (image, rows) = assembler::assemble_listing(SOURCE).expect("program assembles");
    memory::load(0, &image).expect("image loads");
    let mut cpu = onestep::Processor::new();

    // Resolve each annotated instruction index to the address it loaded at.
    let inline: Vec<(u64, u32, &[(usize, u64)])> = INLINE
        .iter()
        .map(|&(idx, line, checks)| (rows[idx - 1].addr, line, checks))
        .collect();

    let mut steps = 0u64;
    while !cpu.halted() && steps < CAP {{
        let pc = cpu.pc;
        cpu.cycle(1).expect("instruction executes without a fault");
        steps += 1;
        let (regs, completed) =
            shared::with(|s| (s.snapshot.regs, s.snapshot.completed_instrs));

        for &(addr, line, checks) in &inline {{
            if addr == pc {{
                for &(reg, want) in checks {{
                    assert_eq!(regs[reg], want, "line {{line}}: r{{reg}} (pc {{addr:#x}})");
                }}
            }}
        }}
        for &(count, line, checks) in AT_COUNT {{
            if count == completed {{
                for &(reg, want) in checks {{
                    assert_eq!(regs[reg], want, "line {{line}}: r{{reg}} at instruction {{count}}");
                }}
            }}
        }}
    }}

    assert!(cpu.halted(), "program did not halt within {{CAP}} instructions");

    let regs = shared::with(|s| s.snapshot.regs);
    let (line, checks) = FINAL;
    for &(reg, want) in checks {{
        assert_eq!(regs[reg], want, "line {{line}}: r{{reg}} at halt");
    }}
}}
"""


def md5(data):
    return hashlib.md5(data).hexdigest()


def emit_manifest(rows):
    return f"""// @generated by tests/testgen.py - do not edit.
// Regenerate with: python3 tests/testgen.py

use std::io::Write;
use std::path::Path;
use std::process::{{Command, Stdio}};

// (file name, md5sum) for every tests/*.s when the tests were generated.
const SOURCES: &[(&str, &str)] = &[
{rows}
];

// Shell out to md5sum (same digest the generator baked in).
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
"""


def ident(stem):
    name = re.sub(r"[^0-9A-Za-z]", "_", stem)
    return name if name[:1].isalpha() or name[:1] == "_" else f"t_{name}"


def main():
    sources = sorted(glob.glob(os.path.join(HERE, "*.s")))

    for stale in glob.glob(os.path.join(HERE, "gen_*.rs")):
        os.remove(stale)

    manifest_rows = []
    for path in sources:
        src_name = os.path.basename(path)
        stem = os.path.splitext(src_name)[0]
        try:
            prog = parse_program(path)
        except GenError as e:
            print(f"testgen: {e}", file=sys.stderr)
            return 1
        with open(os.path.join(HERE, f"gen_{stem}.rs"), "w") as f:
            f.write(emit_program(src_name, ident(stem), prog))
        with open(path, "rb") as f:
            manifest_rows.append(f'    ("{src_name}", "{md5(f.read())}"),')
        checks = len(prog.inline) + len(prog.at) + (1 if prog.final else 0)
        print(f"testgen: {src_name} -> gen_{stem}.rs ({checks} checkpoints)")

    with open(os.path.join(HERE, "gen_manifest.rs"), "w") as f:
        f.write(emit_manifest("\n".join(manifest_rows)))
    print(f"testgen: {len(sources)} program(s), gen_manifest.rs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
