// ui/mod.rs

/**
 * The interactive terminal UI for the assembler + onestep simulator.
 *
 * All ratatui and crossterm code lives under this module. The simulator only
 * ever talks to the UI through `crate::shared`; no widget, frame, or event type
 * is threaded back into the `onestep` execution path. The event loop here owns
 * the `Processor` and is the single place that calls `cycle()`.
 */

mod app;
mod term;
mod view;

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::error::{Error, Result};
use crate::memory;
use crate::onestep::Processor;
use crate::shared;

use app::{Action, App};

// Instructions advanced per free-run frame before the loop returns to repaint
// and poll input. Small enough that an infinite program still pauses instantly.
const RUN_BATCH: u64 = 4096;

// Take over the terminal and drive the simulator until the user quits. `image`
// is kept for the reload key; `load_addr` is where it goes in memory.
pub fn run(mut cpu: Processor, image: Vec<u8>, load_addr: u64) -> Result<()> {
    let mut term = term::init().map_err(|e| Error(format!("terminal setup: {e}")))?;
    let mut app = App::new(load_addr);

    let outcome = event_loop(&mut term, &mut app, &mut cpu, &image, load_addr);
    let restored = term::restore().map_err(|e| Error(format!("terminal restore: {e}")));

    outcome.and(restored)
}

fn event_loop(
    term: &mut term::Tui,
    app: &mut App,
    cpu: &mut Processor,
    image: &[u8],
    load_addr: u64,
) -> Result<()> {
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
        if event::poll(timeout).map_err(|e| Error(format!("input: {e}")))? {
            match event::read().map_err(|e| Error(format!("input: {e}")))? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.needs_redraw = true;
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        return Ok(());
                    }
                    match app.on_key(key.code) {
                        Action::Quit => return Ok(()),
                        Action::Step(n) => {
                            let _ = cpu.cycle(n);
                        }
                        Action::ToggleRun => {
                            app.running = !app.running && !cpu.halted();
                        }
                        Action::Reload => {
                            reload(cpu, image, load_addr)?;
                            app.running = false;
                        }
                        Action::Nothing => {}
                    }
                }
                Event::Resize(..) => app.needs_redraw = true,
                _ => {}
            }
        }

        if app.running {
            if cpu.cycle(RUN_BATCH).is_err() || cpu.halted() {
                app.running = false;
            }
        }
    }
}

// Re-load the original image and hand back a fresh processor at the entry point.
fn reload(cpu: &mut Processor, image: &[u8], load_addr: u64) -> Result<()> {
    memory::clear();
    memory::load(load_addr, image)?;
    *cpu = Processor::new();
    cpu.pc = load_addr;
    cpu.publish();
    Ok(())
}
