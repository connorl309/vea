// ui/view.rs
//
// Every widget the TUI draws. Reads the shared snapshot and the current `App`
// state and paints a frame; it never mutates the simulator and only stashes
// layout metrics back onto `App` for the key handler.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::isa::NUM_REGS;
use crate::memory;
use crate::shared::{Shared, Snapshot, StageSlot};

use super::app::{App, Mode, Pane, Radix};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;

// A terminal at least this wide shows all three panes at once; narrower falls
// back to a single tabbed pane.
const WIDE: u16 = 92;

pub fn draw(f: &mut Frame, app: &mut App, sh: &Shared) {
    // Hand the key handler what it needs to follow PC and clamp scrolling.
    app.cur_pc = sh.snapshot.pc;
    app.prog_len = sh.program.len();

    let area = f.area();
    let rows = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(0),    // panes
        Constraint::Length(1), // status / command line
        Constraint::Length(1), // keybar (always visible)
    ])
    .split(area);

    header(f, rows[0], app, sh);
    if area.width >= WIDE {
        wide_body(f, rows[1], app, sh);
    } else {
        tab_body(f, rows[1], app, sh);
    }
    status(f, rows[2], app, &sh.snapshot);
    keybar(f, rows[3]);

    if app.show_help {
        help(f, area);
    }
}

fn header(f: &mut Frame, area: Rect, app: &App, sh: &Shared) {
    let s = &sh.snapshot;
    let (label, color) = if s.fault.is_some() {
        ("FAULT", Color::Red)
    } else if s.halted {
        ("HALTED", Color::Yellow)
    } else if app.running {
        ("RUNNING", Color::Green)
    } else {
        ("PAUSED", Color::Gray)
    };
    let name = sh.source.as_deref().unwrap_or("(no program)");
    let follow = if app.follow_pc { "follow" } else { "free" };
    let cpi = s.cycles as f64 / s.completed_instrs.max(1) as f64;

    let m = memory::stats();
    let mem = if m.pages == 0 {
        "mem \u{2014}".to_string()
    } else {
        format!(
            "mem {} \u{00b7} {} pg \u{00b7} hi {:#x}",
            human_bytes(m.bytes),
            m.pages,
            m.high
        )
    };

    let line = Line::from(vec![
        Span::styled(
            " VEA ",
            Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  {name}   ")),
        Span::styled(
            format!("{label:<8}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("[{}]  ", app.engine.label()), Style::default().fg(Color::Magenta)),
        Span::raw(format!("pc {:#018x}   instr {}   ", s.pc, s.completed_instrs)),
        Span::styled(
            format!("cycles {}  (cpi {cpi:.2})   ", s.cycles),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(mem, Style::default().fg(Color::Gray)),
        Span::styled(format!("   [{follow}]"), Style::default().fg(DIM)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn human_bytes(n: u64) -> String {
    const K: u64 = 1024;
    if n >= K * K {
        format!("{} MiB", n / (K * K))
    } else if n >= K {
        format!("{} KiB", n / K)
    } else {
        format!("{n} B")
    }
}

// The line just above the keybar: the command line while typing, otherwise the
// fault reason, the last command result, or a pending repeat count.
fn status(f: &mut Frame, area: Rect, app: &App, s: &Snapshot) {
    let line = if let Mode::Command(buf) = &app.mode {
        Line::from(vec![
            Span::styled(":", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw(format!("{buf}\u{2588}")),
        ])
    } else if let Some(err) = &s.fault {
        Line::from(Span::styled(
            format!(" \u{26a0} fault: {err}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else if let Some(msg) = &app.message {
        let color = if app.message_is_error { Color::Red } else { Color::Green };
        Line::from(Span::styled(format!(" {msg}"), Style::default().fg(color)))
    } else if let Some(count) = app.count {
        Line::from(Span::styled(
            format!(" count: {count}"),
            Style::default().fg(Color::Gray),
        ))
    } else {
        Line::from("")
    };
    f.render_widget(Paragraph::new(line), area);
}

// Always-on keybinding strip. `?` opens the fuller reference with the command
// list; this stays put so the bindings are never a mystery.
fn keybar(f: &mut Frame, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    let key = |k: &str, d: &str| {
        [
            Span::styled(format!(" {k} "), Style::default().fg(Color::Black).bg(DIM)),
            Span::styled(format!(" {d}"), Style::default().fg(Color::Gray)),
            Span::raw("   "),
        ]
    };
    for pair in [
        ("q", "quit"),
        ("?", "keys"),
        (":", "command"),
        ("s", "step"),
        ("space", "run"),
        ("Tab", "pane"),
        ("j/k", "scroll"),
        ("d/u", "page"),
        ("g/G", "start/pc"),
        ("f", "follow"),
        ("x", "radix"),
        ("R", "reload"),
        ("E", "engine"),
    ] {
        spans.extend(key(pair.0, pair.1));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn wide_body(f: &mut Frame, area: Rect, app: &mut App, sh: &Shared) {
    let cols = Layout::horizontal([Constraint::Min(30), Constraint::Length(48)]).split(area);
    disasm(f, cols[0], app, sh);
    let right = Layout::vertical([
        Constraint::Length(22),
        Constraint::Length(5),
        Constraint::Min(4),
    ])
    .split(cols[1]);
    registers(f, right[0], app, &sh.snapshot);
    pipeline(f, right[1], app, &sh.snapshot);
    memory(f, right[2], app, &sh.snapshot);
}

fn tab_body(f: &mut Frame, area: Rect, app: &mut App, sh: &Shared) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);

    let mut tabs: Vec<Span> = Vec::new();
    for (name, pane) in [
        ("disasm", Pane::Disasm),
        ("registers", Pane::Registers),
        ("pipeline", Pane::Pipeline),
        ("memory", Pane::Memory),
    ] {
        let style = if app.focus == pane {
            Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM)
        };
        tabs.push(Span::styled(format!(" {name} "), style));
        tabs.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(tabs)), rows[0]);

    match app.focus {
        Pane::Disasm => disasm(f, rows[1], app, sh),
        Pane::Registers => registers(f, rows[1], app, &sh.snapshot),
        Pane::Pipeline => pipeline(f, rows[1], app, &sh.snapshot),
        Pane::Memory => memory(f, rows[1], app, &sh.snapshot),
    }
}

// IF/ID, ID/EX, EX/WB - what nstep currently has in flight. onestep has no
// pipeline, so `s.pipeline` is `None` and this just explains that instead.
fn pipeline(f: &mut Frame, area: Rect, app: &App, s: &Snapshot) {
    let block = panel(" pipeline ", app.focus == Pane::Pipeline);

    let Some(p) = &s.pipeline else {
        let hint = Paragraph::new(vec![
            Line::from(""),
            Line::from("  onestep retires one instruction per"),
            Line::from("  cycle \u{2014} no pipeline stages to show"),
        ])
        .block(block);
        f.render_widget(hint, area);
        return;
    };

    let stage_line = |name: &str, slot: &Option<StageSlot>| -> Line<'static> {
        match slot {
            Some(slot) => Line::from(vec![
                Span::styled(format!("{name:<6}"), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:#010x}  ", slot.pc), Style::default().fg(Color::Gray)),
                Span::styled(slot.desc.clone(), Style::default().fg(Color::White)),
            ]),
            None => Line::from(vec![
                Span::styled(format!("{name:<6}"), Style::default().fg(DIM)),
                Span::styled("bubble", Style::default().fg(DIM)),
            ]),
        }
    };

    let lines = vec![
        stage_line("IF/ID", &p.if_id),
        stage_line("ID/EX", &p.id_ex),
        stage_line("EX/WB", &p.ex_wb),
    ];
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn disasm(f: &mut Frame, area: Rect, app: &mut App, sh: &Shared) {
    let block = panel(" disassembly ", app.focus == Pane::Disasm);

    if sh.program.is_empty() {
        app.disasm_rows = 0;
        let hint = Paragraph::new(vec![
            Line::from(""),
            Line::from("  no program loaded"),
            Line::from(""),
            Line::from(vec![
                Span::raw("  press "),
                Span::styled(":", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::raw(" then  "),
                Span::styled("load <path>", Style::default().fg(ACCENT)),
            ]),
        ])
        .block(block);
        f.render_widget(hint, area);
        return;
    }

    let pc = sh.snapshot.pc;
    let pc_row = sh.program.iter().position(|r| r.addr == pc);

    if app.follow_pc {
        if let Some(i) = pc_row {
            app.disasm_sel = i;
        }
    }
    if app.disasm_sel >= sh.program.len() {
        app.disasm_sel = sh.program.len() - 1;
    }
    app.disasm_rows = area.height.saturating_sub(2) as usize;

    let items: Vec<ListItem> = sh
        .program
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let here = Some(i) == pc_row;
            let gutter = if here { "\u{25b6} " } else { "  " };
            let bytes = r
                .bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            let text = format!("{gutter}{:08x}  {bytes:<23}  {}", r.addr, r.text);
            let style = if here {
                Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(DIM));
    let mut state = ListState::default();
    state.select(Some(app.disasm_sel));
    f.render_stateful_widget(list, area, &mut state);
}

fn registers(f: &mut Frame, area: Rect, app: &App, s: &Snapshot) {
    let inner_w = area.width.saturating_sub(2) as usize;
    let cell = match app.radix {
        Radix::Hex => 22,
        Radix::Signed => 26,
    };
    let cols = (inner_w / cell).clamp(1, 4);
    let per_col = NUM_REGS.div_ceil(cols);

    let mut lines: Vec<Line> = Vec::new();
    for row in 0..per_col {
        let mut spans: Vec<Span> = Vec::new();
        for col in 0..cols {
            let idx = row + col * per_col;
            if idx >= NUM_REGS {
                continue;
            }
            let v = s.regs[idx];
            let body = match app.radix {
                Radix::Hex => format!("r{idx:<2} {v:016x}  "),
                Radix::Signed => format!("r{idx:<2} {:>20}  ", v as i64),
            };
            let style = if v == 0 {
                Style::default().fg(DIM)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(body, style));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(format!("pc {:016x}", s.pc)));
    lines.push(flags_line(s));

    f.render_widget(
        Paragraph::new(lines).block(panel(" registers ", app.focus == Pane::Registers)),
        area,
    );
}

fn flags_line(s: &Snapshot) -> Line<'static> {
    let flag = |name: &str, on: bool| {
        let style = if on {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM)
        };
        Span::styled(format!("{name}{}  ", on as u8), style)
    };
    Line::from(vec![
        Span::raw("flags  "),
        flag("Z", s.flags.z),
        flag("N", s.flags.n),
        flag("C", s.flags.c),
        flag("V", s.flags.v),
    ])
}

fn memory(f: &mut Frame, area: Rect, app: &mut App, s: &Snapshot) {
    let inner_w = area.width.saturating_sub(2) as usize;
    let inner_h = area.height.saturating_sub(2).max(1) as usize;

    // address ("00000000: ") + n * "xx " + gap + n ascii chars
    let fits = |n: usize| 10 + n * 3 + 1 + n <= inner_w;
    let stride = if fits(16) {
        16
    } else if fits(8) {
        8
    } else {
        4
    };
    app.mem_stride = stride as u64;
    app.mem_rows = inner_h as u64;

    let base = app.mem_base & !(stride as u64 - 1);
    let data = memory::dump(base, inner_h * stride);

    let mut lines: Vec<Line> = Vec::new();
    for (row, chunk) in data.chunks(stride).enumerate() {
        let addr = base + (row * stride) as u64;
        let mut spans = vec![Span::styled(format!("{addr:08x}: "), Style::default().fg(DIM))];
        for (i, b) in chunk.iter().enumerate() {
            let style = if addr + i as u64 == s.pc {
                Style::default().fg(Color::Black).bg(ACCENT)
            } else if *b == 0 {
                Style::default().fg(DIM)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(format!("{b:02x} "), style));
        }
        spans.push(Span::raw(" "));
        let ascii: String = chunk
            .iter()
            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
            .collect();
        spans.push(Span::styled(ascii, Style::default().fg(Color::Gray)));
        lines.push(Line::from(spans));
    }

    let title = format!(" memory  {base:#010x} ");
    f.render_widget(
        Paragraph::new(lines).block(panel(&title, app.focus == Pane::Memory)),
        area,
    );
}

fn help(f: &mut Frame, area: Rect) {
    if area.width < 44 || area.height < 14 {
        return;
    }
    let w = 58.min(area.width - 4);
    let h = 18.min(area.height - 4);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    let keys = [
        ("s  .", "step (prefix a count, e.g. 500s)"),
        ("space  c", "run / pause"),
        ("R", "reload the program from disk"),
        ("E", "switch onestep / nstep (restarts)"),
        ("Tab", "cycle panes"),
        ("j k  arrows", "scroll the focused pane"),
        ("d u", "page down / up"),
        ("g  G", "jump to start / to PC"),
        ("f", "toggle follow-PC"),
        ("x", "registers hex / signed"),
        (":", "command line"),
        ("q", "quit"),
    ];
    let mut lines: Vec<Line> = keys
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!("  {k:<12}"), Style::default().fg(ACCENT)),
                Span::raw(*d),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  commands  ", Style::default().fg(ACCENT)),
        Span::raw("load  reload  reset  run  step  goto  pc  engine  quit"),
    ]));

    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(" keys  (any key closes) "),
        ),
        rect,
    );
}

fn panel(title: &str, focused: bool) -> Block<'static> {
    let border = if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIM)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(Color::White),
        ))
}
