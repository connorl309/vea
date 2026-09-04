// TUI application state and the main loop.
//
// This is the working front-end

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

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
}

impl Panel {
    pub const ORDER: [Panel; 4] = [Panel::Source, Panel::Disasm, Panel::Registers, Panel::Memory];

    pub fn title(self) -> &'static str {
        match self {
            Panel::Source => "Source",
            Panel::Disasm => "Disassembly",
            Panel::Registers => "Registers",
            Panel::Memory => "Memory",
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
    // Vertical scroll offset per panel, indexed by `Panel::index`.
    pub scroll: [u16; 4],

    pub log: Vec<String>,
    pub prompt: Option<Prompt>,

    quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            core: processor::Core::new(0),
            source_path: None,
            source: String::new(),
            object: None,
            assembly: Assembly::None,
            focus: Panel::Source,
            scroll: [0; 4],
            log: vec!["ready - press 'o' to open a .s file".to_string()],
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
                self.scroll = [0; 4];
                self.note(format!("loaded {path}"));
                self.assemble();
            }
            Err(e) => self.note(format!("open {path}: {e}")),
        }
    }

    fn reload(&mut self) {
        let Some(path) = self.source_path.clone() else {
            self.note("no file to reload");
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
                self.note(format!(
                    "assembled: {} bytes, {} symbol(s)",
                    obj.bytes.len(),
                    obj.symbols.len()
                ));
                self.object = Some(obj);
            }
            Err(msg) => {
                self.assembly = Assembly::Failed(msg.clone());
                self.object = None;
                self.note(format!("assembly error: {msg}"));
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
                self.note("core reset");
            }
            KeyCode::Char('s') => self.note("step: execution is not implemented yet"),

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
        };
        lines.saturating_sub(1).min(u16::MAX as usize) as u16
    }

    // --- misc ---------------------------------------------------------

    fn note(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
        let overflow = self.log.len().saturating_sub(200);
        if overflow > 0 {
            self.log.drain(..overflow);
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
