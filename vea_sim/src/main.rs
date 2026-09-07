pub mod assembler;
pub mod isa;
pub mod memory;
pub mod onestep;

use std::io::{Read, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "Vea assembler and simulator toolchain")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

// New subcommands slot in here.
#[derive(Subcommand)]
enum Cmd {
    /// Assemble a source file into a flat load ready image
    Asm(AsmArgs),
}

#[derive(Parser)]
struct AsmArgs {
    /// Source path, or - to read stdin
    input: String,
    /// Write raw bytes to this path instead of hex on stdout
    #[arg(short, long)]
    output: Option<String>,
    /// Print the address and encoding of every instruction
    #[arg(short, long, visible_alias = "debug")]
    verbose: bool,
}

fn main() -> ExitCode {
    match Cli::parse().cmd {
        Cmd::Asm(args) => cmd_asm(args),
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
