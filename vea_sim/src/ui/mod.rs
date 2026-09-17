// ui/mod.rs

/**
 * The interactive terminal UI for the assembler + both simulator backends.
 *
 * All ratatui and crossterm code lives under this module. Neither simulator
 * ever sees a widget, frame, or event type - they only talk to the UI through
 * `crate::shared`. `Cpu` below is the one place that knows both `onestep` and
 * `nstep` exist; the event loop just calls its `cycle()`/`halted()` and
 * doesn't care which engine is actually running. `:engine` (or `E`) switches
 * between them, restarting whatever's loaded under the other one.
 *
 * The `:` command line drives everything the simulator can do from inside the
 * TUI: `load <path>`, `reload`, `reset`, `run`, `step [n]`, `goto <addr>`,
 * `pc <addr>`, `engine [onestep|nstep]`, `quit`.
 */

mod app;
mod term;
mod view;

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::assembler;
use crate::error::{Error, Result};
use crate::memory;
use crate::nstep;
use crate::onestep;
use crate::shared;

use app::{Action, App, Engine};

// Instructions advanced per free-run frame before the loop returns to repaint
// and poll input. Small enough that an infinite program still pauses instantly.
const RUN_BATCH: u64 = 4096;

// Take over the terminal and drive the simulator until the user quits. `initial`
// is an optional program to open on startup; the `:load` command opens others.
pub fn run(initial: Option<&str>, load_addr: u64) -> Result<()> {
    let mut term = term::init().map_err(|e| Error(format!("terminal setup: {e}")))?;
    let mut app = App::new(load_addr);
    let mut sim: Option<Sim> = None;

    match initial {
        Some(path) => match Sim::load(path, load_addr, app.engine) {
            Ok(s) => {
                app.info(format!("loaded {path}"));
                sim = Some(s);
            }
            Err(e) => app.error(e.to_string()),
        },
        None => {
            Cpu::new(app.engine).publish();
            app.info("no program \u{2014} type  :load <path>".into());
        }
    }

    let outcome = event_loop(&mut term, &mut app, &mut sim);
    let restored = term::restore().map_err(|e| Error(format!("terminal restore: {e}")));
    outcome.and(restored)
}

fn event_loop(term: &mut term::Tui, app: &mut App, sim: &mut Option<Sim>) -> Result<()> {
    let mut drawn: Option<u64> = None;

    loop {
        let generation = shared::with(|s| s.generation);
        if app.needs_redraw || drawn != Some(generation) {
            term.draw(|f| shared::with(|s| view::draw(f, app, s)))
                .map_err(|e| Error(format!("draw: {e}")))?;
            app.needs_redraw = false;
            drawn = Some(generation);
        }

        // Poll briefly while free-running so the burst loop stays hot; idle
        // longer otherwise so the process is not busy-waiting.
        let timeout = if app.running {
            Duration::from_millis(8)
        } else {
            Duration::from_millis(120)
        };
        if event::poll(timeout).map_err(io_err)? {
            match event::read().map_err(io_err)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.needs_redraw = true;
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        return Ok(());
                    }
                    match app.on_key(key.code) {
                        Action::Nothing => {}
                        Action::Quit => return Ok(()),
                        Action::Step(n) => step(sim, app, n),
                        Action::ToggleRun => toggle_run(sim, app),
                        Action::Reload => reload(sim, app),
                        Action::ToggleEngine => toggle_engine(sim, app),
                        Action::Command(line) => {
                            if command(&line, sim, app) == Flow::Quit {
                                return Ok(());
                            }
                        }
                    }
                }
                Event::Resize(..) => app.needs_redraw = true,
                _ => {}
            }
        }

        if app.running {
            match sim {
                Some(s) if !s.cpu.halted() => {
                    if let Err(e) = s.cpu.cycle(RUN_BATCH) {
                        app.error(e.to_string());
                        app.running = false;
                    } else if s.cpu.halted() {
                        app.running = false;
                    }
                }
                _ => app.running = false,
            }
        }
    }
}

// ---- the simulator backend ---------------------------------------------

// Whichever engine is actually running. Same public shape either way
// (`cycle`, `halted`, `pc`, `publish`) - the event loop and everything below
// it drives this without caring which variant it got.
enum Cpu {
    OneStep(onestep::Processor),
    NStep(nstep::Processor),
}

impl Cpu {
    fn new(engine: Engine) -> Cpu {
        match engine {
            Engine::OneStep => Cpu::OneStep(onestep::Processor::new()),
            Engine::NStep => Cpu::NStep(nstep::Processor::new()),
        }
    }

    fn set_pc(&mut self, addr: u64) {
        match self {
            Cpu::OneStep(p) => p.pc = addr,
            Cpu::NStep(p) => p.pc = addr,
        }
    }

    fn cycle(&mut self, n: u64) -> Result<()> {
        match self {
            Cpu::OneStep(p) => p.cycle(n),
            Cpu::NStep(p) => p.cycle(n),
        }
    }

    fn halted(&self) -> bool {
        match self {
            Cpu::OneStep(p) => p.halted(),
            Cpu::NStep(p) => p.halted(),
        }
    }

    fn publish(&self) {
        match self {
            Cpu::OneStep(p) => p.publish(),
            Cpu::NStep(p) => p.publish(),
        }
    }
}

// ---- the loaded program -------------------------------------------------

// One open program: its processor, the image kept for reload/reset, and where
// it lives in memory.
struct Sim {
    cpu: Cpu,
    image: Vec<u8>,
    path: String,
    load_addr: u64,
}

impl Sim {
    fn load(path: &str, load_addr: u64, engine: Engine) -> Result<Sim> {
        let src = std::fs::read_to_string(path).map_err(|e| Error(format!("{path}: {e}")))?;
        let (image, rows) = assembler::assemble_listing(&src).map_err(Error)?;
        shared::install_program(rows, load_addr, path.to_string());
        let cpu = boot(&image, load_addr, engine)?;
        Ok(Sim { cpu, image, path: path.to_string(), load_addr })
    }

    fn reboot(&mut self, engine: Engine) -> Result<()> {
        self.cpu = boot(&self.image, self.load_addr, engine)?;
        Ok(())
    }
}

// Load `image` at `load_addr` into a cleared memory and hand back a processor
// of the given engine sitting at the entry point, with an initial snapshot
// published.
fn boot(image: &[u8], load_addr: u64, engine: Engine) -> Result<Cpu> {
    memory::clear();
    memory::load(load_addr, image)?;
    let mut cpu = Cpu::new(engine);
    cpu.set_pc(load_addr);
    cpu.publish();
    Ok(cpu)
}

// ---- actions ----------------------------------------------------------

fn step(sim: &mut Option<Sim>, app: &mut App, n: u64) {
    match sim {
        Some(s) => {
            if let Err(e) = s.cpu.cycle(n) {
                app.error(e.to_string());
            }
        }
        None => app.error("no program loaded".into()),
    }
}

fn toggle_run(sim: &mut Option<Sim>, app: &mut App) {
    match sim {
        Some(s) if !s.cpu.halted() => app.running = !app.running,
        Some(_) => app.error("program has halted \u{2014} :reset to run again".into()),
        None => app.error("no program loaded".into()),
    }
}

fn reload(sim: &mut Option<Sim>, app: &mut App) {
    app.running = false;
    match sim {
        Some(s) => match Sim::load(&s.path, s.load_addr, app.engine) {
            Ok(fresh) => {
                app.info(format!("reloaded {}", fresh.path));
                *s = fresh;
            }
            Err(e) => app.error(e.to_string()),
        },
        None => app.error("no program loaded".into()),
    }
}

// Switch backends. With a program open this restarts it fresh under the new
// engine - the two keep entirely different internal state (nstep's pipeline
// latches have no onestep equivalent), so there's no sensible way to carry
// an in-progress run across the swap.
fn set_engine(engine: Engine, sim: &mut Option<Sim>, app: &mut App) {
    app.engine = engine;
    app.running = false;
    match sim {
        Some(s) => match s.reboot(engine) {
            Ok(()) => app.info(format!("switched to {} (restarted)", engine.label())),
            Err(e) => app.error(e.to_string()),
        },
        None => app.info(format!("engine set to {} \u{2014} no program loaded", engine.label())),
    }
}

fn toggle_engine(sim: &mut Option<Sim>, app: &mut App) {
    set_engine(app.engine.toggled(), sim, app);
}

// ---- command line ----------------------------------------------------

#[derive(PartialEq, Eq)]
enum Flow {
    Stay,
    Quit,
}

fn command(line: &str, sim: &mut Option<Sim>, app: &mut App) -> Flow {
    let line = line.trim();
    let (cmd, rest) = match line.split_once(char::is_whitespace) {
        Some((c, r)) => (c, r.trim()),
        None => (line, ""),
    };

    match cmd {
        "" => {}
        "q" | "quit" | "exit" => return Flow::Quit,
        "load" | "l" | "open" | "e" => load(rest, sim, app),
        "reload" | "r" => reload(sim, app),
        "reset" => match sim {
            Some(s) => match s.reboot(app.engine) {
                Ok(()) => {
                    app.running = false;
                    app.info("reset".into());
                }
                Err(e) => app.error(e.to_string()),
            },
            None => app.error("no program loaded".into()),
        },
        "run" | "continue" => toggle_run(sim, app),
        "s" | "step" => step(sim, app, rest.parse().unwrap_or(1)),
        "goto" | "g" => match parse_addr(rest) {
            Some(addr) => {
                app.goto_memory(addr);
                app.info(format!("memory @ {addr:#x}"));
            }
            None => app.error(format!("bad address: {rest}")),
        },
        "pc" => match (sim.as_mut(), parse_addr(rest)) {
            (Some(s), Some(addr)) => {
                s.cpu.set_pc(addr);
                s.cpu.publish();
                app.info(format!("pc = {addr:#x}"));
            }
            (None, _) => app.error("no program loaded".into()),
            (_, None) => app.error(format!("bad address: {rest}")),
        },
        "engine" | "eng" => match rest {
            "" => toggle_engine(sim, app),
            "onestep" | "one" | "1" => set_engine(Engine::OneStep, sim, app),
            "nstep" | "n" => set_engine(Engine::NStep, sim, app),
            other => app.error(format!("unknown engine: {other} (try onestep or nstep)")),
        },
        "follow" => {
            app.follow_pc = true;
            app.info("follow-PC on".into());
        }
        other => {
            // Bare-path convenience: `:path/to/prog.s`
            if rest.is_empty() && std::path::Path::new(other).is_file() {
                load(other, sim, app);
            } else {
                app.error(format!("unknown command: {other}"));
            }
        }
    }
    Flow::Stay
}

fn load(path: &str, sim: &mut Option<Sim>, app: &mut App) {
    if path.is_empty() {
        app.error("usage: load <path>".into());
        return;
    }
    match Sim::load(path, app.load_addr, app.engine) {
        Ok(s) => {
            app.running = false;
            app.info(format!("loaded {path}"));
            *sim = Some(s);
        }
        Err(e) => app.error(e.to_string()),
    }
}

fn parse_addr(s: &str) -> Option<u64> {
    let t = s.trim();
    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => t.parse().ok(),
    }
}

fn io_err(e: std::io::Error) -> Error {
    Error(format!("input: {e}"))
}
