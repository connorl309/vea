// ui/app.rs
//
// View state and key handling for the TUI. This half owns nothing about the
// simulator: it decides what the user is looking at and turns keystrokes into
// an `Action` for the event loop to carry out.

use ratatui::crossterm::event::KeyCode;

// Which pane has the keyboard. In the wide layout all three are on screen and
// this picks the scroll target; in the narrow layout it also picks the tab.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Disasm,
    Registers,
    Memory,
}

impl Pane {
    fn next(self) -> Pane {
        match self {
            Pane::Disasm => Pane::Registers,
            Pane::Registers => Pane::Memory,
            Pane::Memory => Pane::Disasm,
        }
    }
    fn prev(self) -> Pane {
        self.next().next()
    }
}

// How the register file is formatted.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Radix {
    Hex,
    Signed,
}

// Normal keys, or typing an address into the memory pane's goto prompt.
pub enum Mode {
    Normal,
    Goto(String),
}

// What a keystroke asks the event loop to do. Anything that touches the
// `Processor` goes through here rather than being done in this module.
pub enum Action {
    Nothing,
    Quit,
    Step(u64),
    ToggleRun,
    Reload,
}

pub struct App {
    pub running: bool,
    pub needs_redraw: bool,
    pub focus: Pane,
    pub follow_pc: bool,
    pub radix: Radix,
    pub mode: Mode,
    pub show_help: bool,
    // vim-style numeric prefix: `500s` steps 500 instructions.
    pub count: Option<u64>,

    pub load_addr: u64,

    // Disassembly pane: the highlighted row.
    pub disasm_sel: usize,
    // Memory pane: the address shown on the first row.
    pub mem_base: u64,

    // Layout metrics the renderer stashes each frame so the key handler can
    // page and follow-PC without knowing the terminal size itself.
    pub cur_pc: u64,
    pub prog_len: usize,
    pub disasm_rows: usize,
    pub mem_stride: u64,
    pub mem_rows: u64,
}

impl App {
    pub fn new(load_addr: u64) -> Self {
        App {
            running: false,
            needs_redraw: true,
            focus: Pane::Disasm,
            follow_pc: true,
            radix: Radix::Hex,
            mode: Mode::Normal,
            show_help: false,
            count: None,
            load_addr,
            disasm_sel: 0,
            mem_base: load_addr & !0xF,
            cur_pc: load_addr,
            prog_len: 0,
            disasm_rows: 8,
            mem_stride: 16,
            mem_rows: 16,
        }
    }

    pub fn on_key(&mut self, code: KeyCode) -> Action {
        if let Mode::Goto(_) = self.mode {
            self.goto_key(code);
            return Action::Nothing;
        }
        if self.show_help {
            self.show_help = false;
            return Action::Nothing;
        }

        // Digits accumulate into a repeat count consumed by the next command.
        if let KeyCode::Char(d @ '0'..='9') = code {
            if d != '0' || self.count.is_some() {
                let acc = self.count.unwrap_or(0).saturating_mul(10) + (d as u64 - '0' as u64);
                self.count = Some(acc.min(100_000_000));
                return Action::Nothing;
            }
        }
        let n = self.count.take().unwrap_or(1);

        match code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('s') | KeyCode::Char('.') => Action::Step(n),
            KeyCode::Char(' ') | KeyCode::Char('c') => Action::ToggleRun,
            KeyCode::Char('R') => Action::Reload,

            KeyCode::Tab => {
                self.focus = self.focus.next();
                Action::Nothing
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                Action::Nothing
            }

            KeyCode::Char('f') => {
                self.follow_pc = !self.follow_pc;
                Action::Nothing
            }
            KeyCode::Char('x') => {
                self.radix = match self.radix {
                    Radix::Hex => Radix::Signed,
                    Radix::Signed => Radix::Hex,
                };
                Action::Nothing
            }
            KeyCode::Char('?') => {
                self.show_help = true;
                Action::Nothing
            }
            KeyCode::Char(':') => {
                self.mode = Mode::Goto(String::new());
                Action::Nothing
            }

            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll(n as i64);
                Action::Nothing
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll(-(n as i64));
                Action::Nothing
            }
            KeyCode::Char('d') | KeyCode::PageDown => {
                self.scroll(self.page());
                Action::Nothing
            }
            KeyCode::Char('u') | KeyCode::PageUp => {
                self.scroll(-self.page());
                Action::Nothing
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.follow_pc = false;
                self.disasm_sel = 0;
                self.mem_base = self.load_addr & self.mem_mask();
                Action::Nothing
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.follow_pc = true;
                self.mem_base = self.cur_pc & self.mem_mask();
                Action::Nothing
            }
            KeyCode::Esc => Action::Nothing,
            _ => Action::Nothing,
        }
    }

    // One visible page of the focused pane, in rows.
    fn page(&self) -> i64 {
        match self.focus {
            Pane::Disasm => self.disasm_rows.max(1) as i64,
            Pane::Memory => self.mem_rows.max(1) as i64,
            Pane::Registers => 0,
        }
    }

    fn mem_mask(&self) -> u64 {
        !(self.mem_stride.max(1) - 1)
    }

    fn scroll(&mut self, rows: i64) {
        match self.focus {
            Pane::Disasm => {
                self.follow_pc = false;
                let sel = self.disasm_sel as i64 + rows;
                let last = self.prog_len.saturating_sub(1) as i64;
                self.disasm_sel = sel.clamp(0, last.max(0)) as usize;
            }
            Pane::Memory => {
                let delta = self.mem_stride.max(1) as i64 * rows;
                self.mem_base = self.mem_base.saturating_add_signed(delta) & self.mem_mask();
            }
            Pane::Registers => {}
        }
    }

    fn goto_key(&mut self, code: KeyCode) {
        let Mode::Goto(buf) = &mut self.mode else { return };
        match code {
            KeyCode::Enter => {
                let digits = buf.trim().trim_start_matches("0x").trim_start_matches("0X");
                if let Ok(addr) = u64::from_str_radix(digits, 16) {
                    self.mem_base = addr & self.mem_mask();
                    self.focus = Pane::Memory;
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) if c.is_ascii_hexdigit() => {
                if buf.len() < 16 {
                    buf.push(c);
                }
            }
            _ => {}
        }
    }
}
