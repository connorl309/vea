// TUI application state and the main loop.
//
// This is the working front-end

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::logger;
use crate::memory;
use crate::processor::{self, Core};
use crate::ui;

// Panels that can hold keyboard focus, in Tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Source,
    Disasm,
    Registers,
    Memory,
    Log,
}

impl Panel {
    pub const ORDER: [Panel; 5] = [
        Panel::Source,
        Panel::Disasm,
        Panel::Registers,
        Panel::Memory,
        Panel::Log,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Panel::Source => "Source",
            Panel::Disasm => "Disassembly",
            Panel::Registers => "Registers",
            Panel::Memory => "Memory",
            Panel::Log => "Log",
        }
    }

    pub fn index(self) -> usize {
        Self::ORDER.iter().position(|&p| p == self).unwrap()
    }

    fn shift(self, delta: isize) -> Panel {
        let n = Self::ORDER.len() as isize;
        let i = (self.index() as isize + delta).rem_euclid(n);
        Self::ORDER[i as usize]
    }
}

// Outcome of the most recent assemble, for the status bar.
pub enum Assembly {
    None,
    Ok { bytes: usize, symbols: usize },
    Failed(String),
}

// The bottom input line. Only used to type a path for `o`pen right now.
pub struct Prompt {
    pub label: &'static str,
    pub buffer: String,
}

pub struct App {
    pub core: Core,

    pub source_path: Option<PathBuf>,
    pub source: String,
    pub object: Option<asm::Object>,
    pub assembly: Assembly,

    pub focus: Panel,
    // Vertical scroll offset per panel, indexed by `Panel::index`. For the Log
    // panel this counts lines back from the live tail (0 == following).
    pub scroll: [u16; 5],

    pub prompt: Option<Prompt>,

    quit: bool,
}

impl App {
    pub fn new() -> Self {
        logger::line("ready - press 'o' to open a .s file");
        Self {
            core: processor::Core::new(0),
            source_path: None,
            source: String::new(),
            object: None,
            assembly: Assembly::None,
            focus: Panel::Source,
            scroll: [0; 5],
            prompt: None,
            quit: false,
        }
    }

    // --- lifecycle ---------------------------------------------------------

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let mut redraw = true;
        while !self.quit {
            if redraw {
                terminal.draw(|frame| ui::draw(frame, &self))?;
                redraw = false;
            }
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(key) => {
                        self.on_key(key);
                        redraw = true;
                    }
                    Event::Resize(_, _) => redraw = true,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    // --- source / assembly -----------------------------------------------

    pub fn load_source(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                self.source = text;
                self.source_path = Some(PathBuf::from(path));
                self.scroll = [0; 5];
                logger::line(format!("loaded {path}"));
                self.assemble();
            }
            Err(e) => logger::line(format!("open {path}: {e}")),
        }
    }

    fn reload(&mut self) {
        let Some(path) = self.source_path.clone() else {
            logger::line("no file to reload");
            return;
        };
        self.load_source(&path.to_string_lossy());
    }

    fn assemble(&mut self) {
        match asm::assemble(&self.source) {
            Ok(obj) => {
                self.load_image(&obj.bytes);
                self.assembly = Assembly::Ok {
                    bytes: obj.bytes.len(),
                    symbols: obj.symbols.len(),
                };
                logger::line(format!(
                    "assembled: {} bytes, {} symbol(s)",
                    obj.bytes.len(),
                    obj.symbols.len()
                ));
                self.object = Some(obj);
            }
            Err(msg) => {
                self.assembly = Assembly::Failed(msg.clone());
                self.object = None;
                logger::line(format!("assembly error: {msg}"));
            }
        }
    }

    // Load an assembled image into the shared memory at address 0 and reset
    // the core.
    fn load_image(&mut self, image: &[u8]) {
        memory::load_image(image);
        self.core = processor::Core::new(0);
    }

    // --- input ----------------------------------------------------------

    fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if self.prompt.is_some() {
            self.on_prompt_key(key);
            return;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Char('q') => self.quit = true,

            KeyCode::Tab => self.focus = self.focus.shift(1),
            KeyCode::BackTab => self.focus = self.focus.shift(-1),

            KeyCode::Char('o') => {
                self.prompt = Some(Prompt {
                    label: "open",
                    buffer: String::new(),
                });
            }
            KeyCode::Char('r') => self.reload(),
            KeyCode::Char('R') => {
                self.core = processor::Core::new(0);
                logger::line("core reset");
            }
            KeyCode::Char('s') => logger::line("step: execution is not implemented yet"),

            KeyCode::Char('j') | KeyCode::Down => self.scroll_focused(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_focused(-1),
            KeyCode::Char('d') if ctrl => self.scroll_focused(10),
            KeyCode::Char('u') if ctrl => self.scroll_focused(-10),
            KeyCode::PageDown => self.scroll_focused(10),
            KeyCode::PageUp => self.scroll_focused(-10),
            KeyCode::Char('g') | KeyCode::Home => self.set_scroll(0),
            KeyCode::Char('G') | KeyCode::End => self.set_scroll(u16::MAX),

            _ => {}
        }
    }

    fn on_prompt_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Enter => {
                let path = self
                    .prompt
                    .take()
                    .map(|p| p.buffer.trim().to_string())
                    .unwrap_or_default();
                if !path.is_empty() {
                    self.load_source(&path);
                }
            }
            KeyCode::Backspace => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.push(c);
                }
            }
            _ => {}
        }
    }

    // --- scrolling -----------------------------------------------------

    pub fn scroll_of(&self, panel: Panel) -> u16 {
        self.scroll[panel.index()]
    }

    fn set_scroll(&mut self, v: u16) {
        let max = self.max_scroll(self.focus);
        self.scroll[self.focus.index()] = v.min(max);
    }

    fn scroll_focused(&mut self, delta: i32) {
        // The Log panel scrolls back from the tail, so up/down are inverted:
        // "up" means further into history, "down" means back toward live.
        let delta = if self.focus == Panel::Log { -delta } else { delta };
        let max = self.max_scroll(self.focus) as i32;
        let i = self.focus.index();
        self.scroll[i] = (self.scroll[i] as i32 + delta).clamp(0, max) as u16;
    }

    // Rough upper bound on the scroll offset for a panel, in lines. Overshoot
    // just shows blank space, so an estimate is fine.
    fn max_scroll(&self, panel: Panel) -> u16 {
        let lines = match panel {
            Panel::Source => self.source.lines().count(),
            Panel::Disasm => self.object.as_ref().map_or(0, |o| o.bytes.len() / 2 + 1),
            Panel::Registers => processor::REG_COUNT + 1,
            Panel::Memory => memory::MEM_SIZE / 16,
            // Grows as the log grows, so the scroll range keeps up.
            Panel::Log => logger::len(),
        };
        lines.saturating_sub(1).min(u16::MAX as usize) as u16
    }

}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
