use crate::config;
use crate::processor::StepError;
use crate::session::{CompileError, Session};

use ratatui::crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use ratatui_textarea::{TextArea, CursorMove};

#[derive(Debug, PartialEq)]
pub enum Pane {
    Editor,
    Registers,
    Memory,
    Logs,
}

#[derive(Debug, PartialEq)]
pub enum NumFormat {
    Hex,
    Binary,
    Decimal,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum RunMode {
    Editing,
    Stepping,
    Running,
}

pub struct App<'a> {
    pub session: Session,
    pub editor: TextArea<'a>,
    pub active_pane: Pane,
    pub number_format: NumFormat,
    pub mode: RunMode,
    pub registers_scroll: u16,
    pub memory_scroll: u32,
    pub logs: Vec<String>,
    pub logs_scroll: u16,
    pub should_quit: bool,
    pub error_line: Option<usize>,
    pub memory_pane_height: u16,
    // Holds any UART bytes not yet terminated by a newline.
    uart_leftover: String,
}

impl<'a> App<'a> {
    pub fn new(initial_file: Option<String>) -> App<'a> {
        let mut logs = Vec::new();
        let editor = if let Some(path) = initial_file {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                    logs.push(format!("Loaded file: {}", path));
                    TextArea::new(lines)
                }
                Err(e) => {
                    logs.push(format!("Error loading file {}: {}", path, e));
                    TextArea::default()
                }
            }
        } else {
            TextArea::default()
        };

        App {
            session: Session::new(),
            editor,
            active_pane: Pane::Editor,
            number_format: NumFormat::Hex,
            mode: RunMode::Editing,
            registers_scroll: 0,
            memory_scroll: config::TEXT_BASE,
            logs,
            logs_scroll: u16::MAX,
            should_quit: false,
            error_line: None,
            memory_pane_height: 20,
            uart_leftover: String::new(),
        }
    }
}

pub fn run(initial_file: Option<String>) -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(initial_file);
    let res = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn compile_and_load(app: &mut App) -> Result<(), String> {
    let source = app.editor.lines().join("\n");
    match app.session.load_source(&source) {
        Ok(()) => {
            app.error_line = None;
            app.memory_scroll = config::TEXT_BASE;
            app.logs.push("Assembly successful! CPU reset and loaded.".to_string());
            app.logs_scroll = u16::MAX;
            Ok(())
        }
        Err(CompileError::Lex(e)) => {
            jump_to_error_line(app, e.line);
            Err(format!("Line {}: {}", e.line, e))
        }
        Err(CompileError::Parse(e)) => {
            jump_to_error_line(app, e.line);
            Err(format!("Line {}: {}", e.line, e))
        }
        Err(CompileError::Pseudo(msg)) => Err(format!("Pseudo-instruction error: {}", msg)),
        Err(CompileError::Symbol(msg)) => Err(format!("Symbol error: {}", msg)),
        Err(CompileError::Assemble(errors)) => {
            let first_line = errors.first().map(|e| e.line).unwrap_or(0);
            let mut msg = String::new();
            for err in &errors {
                msg.push_str(&format!("Line {}: {}\n", err.line, err.message));
            }
            jump_to_error_line(app, first_line);
            Err(msg)
        }
    }
}

// Drain UART output into the logs pane. Complete lines (terminated by '\n') are pushed
// immediately. Incomplete lines are held in `leftover` until a newline arrives or
// `flush_partial` is true (used at halt, so the last line is never silently dropped).
fn drain_uart_to_logs(app: &mut App, flush_partial: bool) {
    let bytes = app.session.drain_uart();
    if bytes.is_empty() {
        return;
    }
    app.uart_leftover.push_str(&String::from_utf8_lossy(&bytes));
    // Take ownership of the accumulated text so we can split and write back freely.
    let text = std::mem::take(&mut app.uart_leftover);
    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        if lines.peek().is_some() {
            // A '\n' follows — this is a complete line.
            app.logs.push(format!("UART: {}", line));
            app.logs_scroll = u16::MAX;
        } else {
            // Last segment: no '\n' yet — keep it or flush depending on caller.
            if flush_partial && !line.is_empty() {
                app.logs.push(format!("UART: {}", line));
                app.logs_scroll = u16::MAX;
            } else {
                app.uart_leftover = line.to_string();
            }
        }
    }
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
) -> io::Result<()>
where
    io::Error: From<B::Error>,
{
    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == event::KeyEventKind::Press {
                if key.code == KeyCode::Esc {
                    app.should_quit = true;
                }

                if app.should_quit {
                    return Ok(());
                }

                if key.code == KeyCode::Tab {
                    app.active_pane = match app.active_pane {
                        Pane::Editor => Pane::Registers,
                        Pane::Registers => Pane::Memory,
                        Pane::Memory => Pane::Logs,
                        Pane::Logs => Pane::Editor,
                    };
                    continue;
                }

                if key.code == KeyCode::F(2) {
                    if app.mode == RunMode::Editing {
                        if let Err(e) = compile_and_load(&mut app) {
                            app.logs.push(format!("Compile Error:\n{}", e));
                            app.logs_scroll = u16::MAX;
                        }
                    }
                    continue;
                }

                if key.code == KeyCode::F(9) {
                    app.number_format = match app.number_format {
                        NumFormat::Hex => NumFormat::Binary,
                        NumFormat::Binary => NumFormat::Decimal,
                        NumFormat::Decimal => NumFormat::Hex,
                    };
                    continue;
                }

                if key.code == KeyCode::F(5) {
                    if app.mode == RunMode::Editing {
                        if let Err(e) = compile_and_load(&mut app) {
                            app.logs.push(format!("Compile Error:\n{}", e));
                            app.logs_scroll = u16::MAX;
                            continue;
                        }
                    }
                    app.mode = RunMode::Running;
                    let halt = app.session.run_to_halt();
                    drain_uart_to_logs(&mut app, true);
                    app.logs.push(format!("Halted: {}", format_step_error(&halt)));
                    app.logs_scroll = u16::MAX;
                    app.mode = RunMode::Editing;
                    move_cursor_to_pc(&mut app);
                    maybe_follow_pc_in_memory(&mut app);
                    continue;
                }

                if key.code == KeyCode::F(10) {
                    if app.mode == RunMode::Editing {
                        if let Err(e) = compile_and_load(&mut app) {
                            app.logs.push(format!("Compile Error:\n{}", e));
                            app.logs_scroll = u16::MAX;
                            continue;
                        }
                        app.mode = RunMode::Stepping;
                    }
                    match app.session.step() {
                        Ok(_) => {
                            drain_uart_to_logs(&mut app, false);
                            move_cursor_to_pc(&mut app);
                            maybe_follow_pc_in_memory(&mut app);
                        }
                        Err(e) => {
                            drain_uart_to_logs(&mut app, true);
                            app.logs.push(format!("Halted: {}", format_step_error(&e)));
                            app.logs_scroll = u16::MAX;
                            app.mode = RunMode::Editing;
                        }
                    }
                    continue;
                }

                match app.active_pane {
                    Pane::Editor => {
                        app.editor.input(key);
                        app.mode = RunMode::Editing;
                        app.error_line = None;
                    }
                    Pane::Registers => {
                        match key.code {
                            KeyCode::Up => app.registers_scroll = app.registers_scroll.saturating_sub(1),
                            KeyCode::Down => app.registers_scroll = app.registers_scroll.saturating_add(1).min(31),
                            _ => {}
                        }
                    }
                    Pane::Memory => {
                        match key.code {
                            KeyCode::Up => app.memory_scroll = app.memory_scroll.saturating_sub(4),
                            KeyCode::Down => app.memory_scroll = app.memory_scroll.wrapping_add(4),
                            KeyCode::Char('t') | KeyCode::Char('T') => app.memory_scroll = config::TEXT_BASE,
                            KeyCode::Char('d') | KeyCode::Char('D') => app.memory_scroll = config::DATA_BASE,
                            KeyCode::Char('s') | KeyCode::Char('S') => app.memory_scroll = config::STACK_BASE.saturating_sub(64),
                            KeyCode::Char('c') | KeyCode::Char('C') => app.memory_scroll = app.session.processor.pc(),
                            _ => {}
                        }
                    }
                    Pane::Logs => {
                        match key.code {
                            KeyCode::Up => app.logs_scroll = app.logs_scroll.saturating_sub(1),
                            KeyCode::Down => app.logs_scroll = app.logs_scroll.saturating_add(1),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

fn maybe_follow_pc_in_memory(app: &mut App) {
    let pc = app.session.processor.pc();
    let visible_bytes = (app.memory_pane_height as u32) * 4;
    let in_view = pc >= app.memory_scroll
        && pc < app.memory_scroll.saturating_add(visible_bytes);
    if !in_view {
        app.memory_scroll = pc;
    }
}

fn jump_to_error_line(app: &mut App, line: usize) {
    app.error_line = Some(line);
    if line > 0 {
        app.editor.move_cursor(CursorMove::Jump((line - 1) as u16, 0));
    }
}

fn move_cursor_to_pc(app: &mut App) {
    if let Some(ref debug_info) = app.session.debug_info {
        if let Some(mapping) = debug_info.address_to_source.get(&app.session.processor.pc()) {
            if mapping.line > 0 {
                app.editor.move_cursor(CursorMove::Jump((mapping.line - 1) as u16, 0));
            }
        }
    }
}

fn format_step_error(e: &StepError) -> String {
    match e {
        StepError::Ebreak => "ebreak".to_string(),
        StepError::IllegalInstruction => "illegal instruction".to_string(),
        StepError::MemoryFault(f) => format!("memory fault: {:?}", f),
    }
}

mod ui {
    use super::*;
    use ratatui::{
        layout::{Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Paragraph},
        Frame,
    };

    const ABI_NAMES: [&str; 32] = [
        "zero", "ra",  "sp",  "gp",  "tp",  "t0",  "t1",  "t2",
        "s0",   "s1",  "a0",  "a1",  "a2",  "a3",  "a4",  "a5",
        "a6",   "a7",  "s2",  "s3",  "s4",  "s5",  "s6",  "s7",
        "s8",   "s9",  "s10", "s11", "t3",  "t4",  "t5",  "t6",
    ];

    pub fn draw(f: &mut Frame, app: &mut App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(10),
            ])
            .split(f.area());

        let dim   = Style::default().fg(Color::DarkGray);
        let key   = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        let val   = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
        let sep   = Span::styled("  │  ", dim);
        let mode_color = match app.mode {
            RunMode::Editing  => Color::Gray,
            RunMode::Stepping => Color::Green,
            RunMode::Running  => Color::Yellow,
        };
        let mode_label = match app.mode {
            RunMode::Editing  => "Editing",
            RunMode::Stepping => "Stepping",
            RunMode::Running  => "Running",
        };
        let fmt_label = match app.number_format {
            NumFormat::Hex     => "Hex",
            NumFormat::Binary  => "Bin",
            NumFormat::Decimal => "Dec",
        };
        let top_line = Line::from(vec![
            Span::styled(" PC ", dim),
            Span::styled(format!("0x{:08x}", app.session.processor.pc()), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            sep.clone(),
            Span::styled(mode_label, Style::default().fg(mode_color).add_modifier(Modifier::BOLD)),
            sep.clone(),
            Span::styled(fmt_label, val),
            sep.clone(),
            Span::styled("F2", key), Span::styled(" Load  ", dim),
            Span::styled("F5", key), Span::styled(" Run  ", dim),
            Span::styled("F10", key), Span::styled(" Step  ", dim),
            Span::styled("F9", key), Span::styled(" Number format  ", dim),
            Span::styled("Tab", key), Span::styled(" Switch pane  ", dim),
            Span::styled("Esc", key), Span::styled(" Quit", dim),
        ]);
        let top_bar = Paragraph::new(top_line)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(top_bar, chunks[0]);

        let middle_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .split(chunks[1]);

        let editor_style = if app.active_pane == Pane::Editor { Style::default().fg(Color::Yellow) } else { Style::default() };
        app.editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(editor_style)
                .title("Code Editor"),
        );
        if app.error_line.is_some() {
            app.editor.set_cursor_line_style(Style::default().bg(Color::Red).fg(Color::White));
        } else if app.mode == RunMode::Stepping {
            app.editor.set_cursor_line_style(Style::default().bg(Color::DarkGray));
        } else {
            app.editor.set_cursor_line_style(Style::default());
        }
        f.render_widget(&app.editor, middle_chunks[0]);

        let regs = app.session.processor.registers();
        let stepping = app.mode != RunMode::Editing;
        let mut reg_lines: Vec<Line> = Vec::new();
        for i in 0..32 {
            let value_str = match app.number_format {
                NumFormat::Hex     => format!("0x{:08x}", regs[i]),
                NumFormat::Binary  => format!("0b{:032b}", regs[i]),
                NumFormat::Decimal => format!("{:>11}", regs[i] as i32),
            };
            let label = format!("{:>3} {:4}", format!("x{}", i), ABI_NAMES[i]);
            let text = format!("{}  {}", label, value_str);
            let changed = stepping && regs[i] != app.session.prev_registers[i];
            let style = if changed {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            reg_lines.push(Line::from(Span::styled(text, style)));
        }
        let regs_style = if app.active_pane == Pane::Registers { Style::default().fg(Color::Yellow) } else { Style::default() };
        let regs_p = Paragraph::new(reg_lines)
            .scroll((app.registers_scroll, 0))
            .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(regs_style)
                .title("Registers"),
        );
        f.render_widget(regs_p, middle_chunks[1]);

        let mem_start = app.memory_scroll;
        let mem_size_words = middle_chunks[2].height.saturating_sub(2) as u32;
        app.memory_pane_height = mem_size_words as u16;

        let mut mem_lines: Vec<Line> = Vec::new();

        for i in 0..mem_size_words {
            let addr = mem_start + (i * 4);
            match app.session.processor.read_memory_word(addr) {
                Ok(word) => {
                    let formatted = match app.number_format {
                        NumFormat::Hex => format!("0x{:08x}: 0x{:08x}", addr, word),
                        NumFormat::Binary => format!("0x{:08x}: 0b{:032b}", addr, word),
                        NumFormat::Decimal => format!("0x{:08x}: {:<11}", addr, word),
                    };

                    if addr == app.session.processor.pc() {
                        mem_lines.push(Line::from(vec![Span::styled(
                            formatted,
                            Style::default().bg(Color::DarkGray).fg(Color::Green),
                        )]));
                    } else {
                        mem_lines.push(Line::from(formatted));
                    }
                }
                Err(_) => {
                    if i == 0 {
                        mem_lines.push(Line::from(Span::styled(
                            "Unallocated Memory Range",
                            Style::default().fg(Color::Red),
                        )));
                    }
                    break;
                }
            }
        }

        let stack_start = config::STACK_BASE.saturating_sub(config::STACK_SIZE as u32);
        let section = if mem_start >= config::DATA_BASE {
            "data"
        } else if mem_start >= config::TEXT_BASE {
            "text"
        } else if mem_start >= stack_start && mem_start < config::STACK_BASE {
            "stack"
        } else {
            "unmapped"
        };

        let mem_style = if app.active_pane == Pane::Memory { Style::default().fg(Color::Yellow) } else { Style::default() };
        let mem_p = Paragraph::new(mem_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(mem_style)
                .title(format!("Memory (.{}) (0x{:08x})", section, mem_start)),
        );
        f.render_widget(mem_p, middle_chunks[2]);

        let logs_text = app.logs.join("\n");
        let total_log_lines: u16 = app.logs.iter()
            .map(|l| l.lines().count().max(1))
            .sum::<usize>() as u16;
        let logs_visible = chunks[2].height.saturating_sub(2);
        let max_scroll = total_log_lines.saturating_sub(logs_visible);
        app.logs_scroll = app.logs_scroll.min(max_scroll);
        let logs_style = if app.active_pane == Pane::Logs { Style::default().fg(Color::Yellow) } else { Style::default() };
        let logs = Paragraph::new(logs_text)
            .scroll((app.logs_scroll, 0))
            .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(logs_style)
                .title("Execution Logs (↑↓ to scroll)"),
        );
        f.render_widget(logs, chunks[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_load_file() {
        let app = App::new(Some("Cargo.toml".to_string()));
        assert!(!app.editor.lines().is_empty());
        assert!(app.logs[0].contains("Loaded file: Cargo.toml"));
    }

    #[test]
    fn test_app_load_non_existent_file() {
        let app = App::new(Some("non_existent_file.asm".to_string()));
        assert_eq!(app.editor.lines().len(), 1);
        assert!(app.logs[0].contains("Error loading file"));
    }
}
