// ui/term.rs
//
// Terminal lifecycle: raw mode, the alternate screen, and a panic hook that
// puts the terminal back before a panic message prints. Everything crossterm
// touches is confined to this file.

use std::io::{self, Stdout};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

// Enter raw mode + the alternate screen and hand back a ready terminal.
pub fn init() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    install_panic_hook();
    Terminal::new(CrosstermBackend::new(out))
}

// Leave the alternate screen and drop raw mode. Safe to call more than once.
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

// Restore the terminal on the way out of a panic, then run the usual hook so
// the backtrace is still readable.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        previous(info);
    }));
}
