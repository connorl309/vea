// Rendering. `draw` lays out the panels; one function each below.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::{App, Assembly, Panel};
use crate::logger;
use crate::memory;
use crate::processor::{CoreState, REG_COUNT};

pub fn draw(frame: &mut Frame, app: &App) {
    let rows = Layout::vertical([
        Constraint::Min(6),     // body
        Constraint::Length(8),  // live log
        Constraint::Length(1),  // status bar
    ])
    .split(frame.area());

    let cols = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).split(rows[0]);
    let left = Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).split(cols[0]);
    let right = Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).split(cols[1]);

    source(frame, left[0], app);
    disassembly(frame, left[1], app);
    registers(frame, right[0], app);
    memory(frame, right[1], app);
    log_panel(frame, rows[1], app);
    status_bar(frame, rows[2], app);
}

// A bordered block whose colour and title marker reflect focus.
fn panel(title: &str, focused: bool) -> Block<'static> {
    let colour = if focused { Color::Cyan } else { Color::DarkGray };
    let marker = if focused { "▶ " } else { "  " };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colour))
        .title(Span::styled(
            format!("{marker}{title}"),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ))
}

fn dim(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(Color::DarkGray))
}

fn source(frame: &mut Frame, area: Rect, app: &App) {
    let title = match &app.source_path {
        Some(p) => format!("Source: {}", p.display()),
        None => "Source".to_string(),
    };

    let lines: Vec<Line> = if app.source.is_empty() {
        vec![
            Line::from(""),
            Line::from(dim("  no source loaded")),
            Line::from(dim("  press 'o' to open a .s file")),
        ]
    } else {
        app.source
            .lines()
            .enumerate()
            .map(|(i, l)| Line::from(vec![dim(format!("{:>4} │ ", i + 1)), Span::raw(l)]))
            .collect()
    };

    let widget = Paragraph::new(Text::from(lines))
        .block(panel(&title, app.focus == Panel::Source))
        .scroll((app.scroll_of(Panel::Source), 0));
    frame.render_widget(widget, area);
}

fn disassembly(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel("Disassembly", app.focus == Panel::Disasm);

    let lines: Vec<Line> = match &app.object {
        None => vec![Line::from(dim("  nothing assembled"))],
        Some(obj) => {
            let img = &obj.bytes;
            let mut out = Vec::new();
            let mut pos = 0usize;
            while pos + 2 <= img.len() {
                let opcode = img[pos];
                let plen = (img[pos + 1] >> 4) as usize;
                let flags = img[pos + 1] & 0x0F;
                let end = (pos + 2 + plen).min(img.len());
                let raw: String = img[pos..end].iter().map(|b| format!("{b:02x} ")).collect();
                let name = asm::isa::opcodes::by_opcode(opcode, flags)
                    .map(|d| d.mnemonic)
                    .unwrap_or("?");
                out.push(Line::from(vec![
                    dim(format!("{pos:04x}  ")),
                    Span::styled(format!("{raw:<27}"), Style::default().fg(Color::Blue)),
                    Span::raw(name),
                ]));
                pos = end;
                if pos % 2 == 1 {
                    pos += 1; // alignment pad after an odd-length frame
                }
            }
            out.push(Line::from(dim("  operand decoding: TODO")));
            out
        }
    };

    let widget = Paragraph::new(Text::from(lines))
        .block(block)
        .scroll((app.scroll_of(Panel::Disasm), 0));
    frame.render_widget(widget, area);
}

fn registers(frame: &mut Frame, area: Rect, app: &App) {
    let c = &app.core;
    let state = match &c.state {
        CoreState::Running(_) => "running".to_string(),
        CoreState::Stopped => "stopped".to_string(),
        CoreState::Exception(t) => format!("trap {t:?}"),
    };
    let title = format!("Registers: pc={:#08x}  {state}", c.pc);

    let mut lines = Vec::with_capacity(REG_COUNT + 6);
    lines.push(Line::from(dim(format!(
        "  cycle {}   retired {}{}",
        c.cycles,
        c.retired,
        if c.halted { "   HALT" } else { "" },
    ))));
    // The pipeline latches, IF -> ID -> EX (MEM/WB not built yet).
    for tag in c.pipeline_debug() {
        lines.push(Line::from(dim(format!("  {tag}"))));
    }
    lines.push(Line::from(""));
    for (i, v) in c.regs.registers.iter().enumerate() {
        let colour = if *v == 0 { Color::DarkGray } else { Color::White };
        lines.push(Line::from(vec![
            Span::styled(format!(" r{i:<2} "), Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("0x{:08x}_{:08x}", (v >> 32) as u32, *v as u32),
                Style::default().fg(colour),
            ),
        ]));
    }

    let widget = Paragraph::new(Text::from(lines))
        .block(panel(&title, app.focus == Panel::Registers))
        .scroll((app.scroll_of(Panel::Registers), 0));
    frame.render_widget(widget, area);
}

fn memory(frame: &mut Frame, area: Rect, app: &App) {
    let bytes = memory::snapshot();
    let image_len = memory::image_len();
    let title = format!("Memory: image {} B / {} B", image_len, bytes.len());
    let block = panel(&title, app.focus == Panel::Memory);

    // The image is huge, so page it by hand rather than build every line.
    let visible = area.height.saturating_sub(2).max(1) as usize;
    let start = app.scroll_of(Panel::Memory) as usize;

    let mut lines = Vec::with_capacity(visible);
    for row in start..start + visible {
        let base = row * 16;
        if base >= bytes.len() {
            break;
        }
        let chunk = &bytes[base..(base + 16).min(bytes.len())];
        let hex: String = chunk
            .iter()
            .enumerate()
            .map(|(i, b)| if i == 8 { format!(" {b:02x} ") } else { format!("{b:02x} ") })
            .collect();
        let ascii: String = chunk
            .iter()
            .map(|b| if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '.' })
            .collect();
        let in_image = base < image_len;
        let body = if in_image { Color::White } else { Color::DarkGray };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{base:04x}  "),
                Style::default().fg(if in_image { Color::Cyan } else { Color::DarkGray }),
            ),
            Span::styled(format!("{hex:<49}"), Style::default().fg(body)),
            dim(format!("│{ascii}│")),
        ]));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn log_panel(frame: &mut Frame, area: Rect, app: &App) {
    let all = logger::snapshot();
    let total = all.len();

    // `scroll_of(Log)` is lines back from the live tail; 0 == following.
    let visible = area.height.saturating_sub(2).max(1) as usize;
    let back = (app.scroll_of(Panel::Log) as usize).min(total.saturating_sub(1));
    let end = total - back;
    let start = end.saturating_sub(visible);

    let title = if back == 0 {
        format!("Log: {total}")
    } else {
        format!("Log: {total}  (-{back})")
    };

    let lines: Vec<Line> = all[start..end].iter().map(|m| Line::from(m.as_str())).collect();

    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(panel(&title, app.focus == Panel::Log)),
        area,
    );
}

fn status_bar(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(prompt) = &app.prompt {
        let line = Line::from(vec![
            Span::styled(
                format!(" {}> ", prompt.label),
                Style::default().bg(Color::Cyan).fg(Color::Black),
            ),
            Span::raw(" "),
            Span::raw(prompt.buffer.as_str()),
            Span::styled("_", Style::default().fg(Color::Cyan)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let (label, style) = match &app.assembly {
        Assembly::None => (
            " no program ".to_string(),
            Style::default().bg(Color::Blue).fg(Color::White),
        ),
        Assembly::Ok { bytes, symbols } => (
            format!(" OK  {bytes} B  {symbols} sym "),
            Style::default().bg(Color::Green).fg(Color::Black),
        ),
        Assembly::Failed(e) => (
            format!(" ERROR  {e} "),
            Style::default().bg(Color::Red).fg(Color::White),
        ),
    };

    let keys = "  [Tab] focus  [o]pen  [r]eload  [s]tep  [S]tep-10  [R]eset  [j/k] scroll  [q]uit";
    let tail = match &app.notice {
        Some(n) => Span::styled(format!("  {n}"), Style::default().fg(Color::Yellow)),
        None => dim(keys),
    };
    let line = Line::from(vec![
        Span::styled(label, style),
        Span::styled(
            format!(" {} ", app.focus.title()),
            Style::default().bg(Color::DarkGray).fg(Color::White),
        ),
        tail,
    ]);
    frame.render_widget(Paragraph::new(line), area);
}