// CLI wrapper. `asm <input> [-o out] [--hex] [--syms] [--list-isa]`

use std::path::PathBuf;
use std::process::exit;

use asm::{assemble, err, image, isa};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--list-isa") {
        print!("{}", isa::listing());
        return;
    }
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
        return;
    }

    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut hex = false;
    let mut syms = false;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-o" => output = it.next().cloned(),
            "--hex" => hex = true,
            "--syms" => syms = true,
            s if s.starts_with('-') => {
                err::report(&format!("unknown option `{s}`"));
                exit(2);
            }
            s => input = Some(s.to_string()),
        }
    }

    let Some(input) = input else {
        usage();
        exit(2);
    };

    let src = match std::fs::read_to_string(&input) {
        Ok(s) => s,
        Err(e) => {
            err::report(&format!("{input}: {e}"));
            exit(1);
        }
    };

    let obj = match assemble(&src) {
        Ok(o) => o,
        Err(msg) => {
            err::report(&msg);
            exit(1);
        }
    };

    if syms {
        print!("{}", image::symbols(&obj));
    }
    if hex {
        print!("{}", image::hexdump(&obj));
        return;
    }

    let out_path = output.map(PathBuf::from).unwrap_or_else(|| {
        let mut p = PathBuf::from(&input);
        p.set_extension("bin");
        p
    });

    if let Err(e) = std::fs::write(&out_path, &obj.bytes) {
        err::report(&format!("{}: {e}", out_path.display()));
        exit(1);
    }
    println!("wrote {} bytes -> {}", obj.bytes.len(), out_path.display());
}

fn usage() {
    println!(
        "\
usage: asm <input.s> [options]

  -o <file>     output path for the raw image (default: <input>.bin)
  --hex         print a hex dump to stdout instead of writing a file
  --syms        print the symbol table to stdout
  --list-isa    print the instruction table and exit
  -h, --help    this message"
    );
}