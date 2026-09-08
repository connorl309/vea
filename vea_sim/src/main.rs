// CLI shell over the `vea_sim` library.

use std::io::{Read, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use vea_sim::{assembler, memory, ui};

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

fn cmd_asm(args: AsmArgs) -> ExitCode {
    let src = if args.input == "-" {
        let mut s = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut s) {
            eprintln!("asm: stdin: {e}");
            return ExitCode::FAILURE;
        }
        s
    } else {
        match std::fs::read_to_string(&args.input) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("asm: {}: {e}", args.input);
                return ExitCode::FAILURE;
            }
        }
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
