#![allow(dead_code)]

mod app;
mod logger;
mod memory;
mod pipeline;
mod processor;
mod ui;

use std::process::ExitCode;

use app::App;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let source_path = args.iter().find(|a| !a.starts_with('-')).cloned();

    let mut app = App::new();
    if let Some(path) = source_path {
        app.load_source(&path);
    }

    match ratatui::run(move |terminal| app.run(terminal)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("simulator: {e}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "\
simulator - TUI for the project ISA

usage: simulator [file.s]

  file.s        assembly source to load and assemble on startup

in-app keys:
  tab / shift-tab    move panel focus
  j / k, up / down    scroll the focused panel
  ctrl-d / ctrl-u    scroll by a page
  g / G              jump to top / bottom
  o                  open a source file
  r                  reload and re-assemble the current file
  s                  step the machine (not implemented yet)
  R                  reset the machine
  q / ctrl-c         quit
";