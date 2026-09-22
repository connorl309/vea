// CLI shell over the `vea_sim` library.

use std::io::{Read, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use vea_sim::{assembler, memory, onestep, shared, ui};

#[derive(Parser)]
#[command(version, about = "Vea assembler and simulator toolchain")]
struct Cli {
    // No subcommand launches the interactive TUI. It can open a program from
    // its own `:load` prompt, so an input path is optional everywhere.
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    // Assemble a source file into a flat load ready image
    Asm(AsmArgs),
    // Open a program in the interactive TUI (also the default with no arguments)
    Run(RunArgs),
    // Run a program to halt on the onestep simulator and dump its final registers.
    // The RTL testbench uses this as its golden oracle: same program, same check.
    Conform(ConformArgs),
}

#[derive(Parser)]
struct AsmArgs {
    // Source path, or - to read stdin
    input: String,
    // Write raw bytes to this path instead of hex on stdout
    #[arg(short, long)]
    output: Option<String>,
    // Print the address and encoding of every instruction
    #[arg(short, long, visible_alias = "debug")]
    verbose: bool,
}

#[derive(Parser)]
struct ConformArgs {
    // Source path, or - to read stdin
    input: String,
    // Instruction budget before giving up on halting
    #[arg(long, default_value_t = 100_000)]
    cap: u64,
}

#[derive(Parser)]
struct RunArgs {
    // Source path. Omit it and load one from the TUI with `:load <path>`
    input: Option<String>,
    // Address the image is loaded at (decimal, or 0x-prefixed hex)
    #[arg(short, long, default_value = "0", value_parser = parse_addr)]
    load: u64,
}

// Accept both `4096` and `0x1000` for address-style arguments.
fn parse_addr(s: &str) -> std::result::Result<u64, String> {
    let t = s.trim();
    let parsed = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => t.parse(),
    };
    parsed.map_err(|_| format!("not a valid address: {s}"))
}

fn main() -> ExitCode {
    memory::clear();

    let result = match Cli::parse().cmd {
        Some(Cmd::Asm(args)) => return cmd_asm(args),
        Some(Cmd::Run(args)) => ui::run(args.input.as_deref(), args.load),
        Some(Cmd::Conform(args)) => return cmd_conform(args),
        None => ui::run(None, 0),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("vea: {e}");
            ExitCode::FAILURE
        }
    }
}

// Shared by every subcommand that takes a source path: `-` reads stdin, anything
// else is a file path. `who` names the subcommand, for the error prefix.
fn read_source(who: &str, input: &str) -> Result<String, ExitCode> {
    if input == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| {
            eprintln!("{who}: stdin: {e}");
            ExitCode::FAILURE
        })?;
        Ok(s)
    } else {
        std::fs::read_to_string(input).map_err(|e| {
            eprintln!("{who}: {input}: {e}");
            ExitCode::FAILURE
        })
    }
}

fn cmd_asm(args: AsmArgs) -> ExitCode {
    let src = match read_source("asm", &args.input) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let (image, listing) = match assembler::assemble(&src) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("asm: {e}");
            return ExitCode::FAILURE;
        }
    };

    if args.verbose {
        for row in &listing {
            eprintln!("{row}");
        }
        eprintln!("asm: {} instructions, {} bytes", listing.len(), image.len());
    }

    match args.output {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, &image) {
                eprintln!("asm: {path}: {e}");
                return ExitCode::FAILURE;
            }
        }
        None => {
            let _ = writeln!(std::io::stdout(), "{}", assembler::hex(&image));
        }
    }
    ExitCode::SUCCESS
}

// Assembles, runs to halt on the onestep simulator, and prints the final register
// file: one 16-digit hex value per line, r0 first. That is plain $readmemh format,
// so the RTL testbench can load it straight into a 32-entry array and compare.
fn cmd_conform(args: ConformArgs) -> ExitCode {
    let src = match read_source("conform", &args.input) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let image = match assembler::assemble_listing(&src) {
        Ok((image, _rows)) => image,
        Err(e) => {
            eprintln!("conform: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = memory::load(0, &image) {
        eprintln!("conform: {e}");
        return ExitCode::FAILURE;
    }

    let mut cpu = onestep::Processor::new();
    let mut steps = 0u64;
    while !cpu.halted() && steps < args.cap {
        if let Err(e) = cpu.cycle(1) {
            eprintln!("conform: {e}");
            return ExitCode::FAILURE;
        }
        steps += 1;
    }
    if !cpu.halted() {
        eprintln!("conform: did not halt within {} instructions", args.cap);
        return ExitCode::FAILURE;
    }

    let regs = shared::with(|s| s.snapshot.regs);
    let mut out = String::new();
    for r in regs {
        out.push_str(&format!("{r:016x}\n"));
    }
    let _ = write!(std::io::stdout(), "{out}");
    ExitCode::SUCCESS
}
