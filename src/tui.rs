use crate::processor::Processor;
use crate::config;
use crate::lexer;
use crate::parser;
use crate::symbols;
use crate::assembler;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tui_textarea::TextArea;

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
    pub processor: Processor,
    pub editor: TextArea<'a>,
    pub active_pane: Pane,
    pub number_format: NumFormat,
    pub mode: RunMode,
    pub registers_scroll: u16,
    pub memory_scroll: u32,
    pub logs: Vec<String>,
    pub should_quit: bool,
}

impl<'a> App<'a> {
    pub fn new() -> App<'a> {
        let mut editor = TextArea::default();
        editor.set_block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title("Code Editor (F2: Load, F5: Run, F10: Step, Tab: Switch)"),
        );

        App {
            processor: Processor::new(config::TEXT_BASE, config::DATA_BASE, config::STACK_BASE, config::STACK_SIZE),
            editor,
            active_pane: Pane::Editor,
            number_format: NumFormat::Hex,
            mode: RunMode::Editing,
            registers_scroll: 0,
            memory_scroll: config::TEXT_BASE,
            logs: vec![],
            should_quit: false,
        }
    }
}

pub fn run() -> Result<(), io::Error> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let app = App::new();
    let res = run_app(&mut terminal, app);

    // restore terminal
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
    let tokens = lexer::tokenize(&source);
    let mut parser = parser::Parser::new(tokens);
    let statements = parser.parse().map_err(|_| "Parse error".to_string())?;

    let mut symbol_table = symbols::SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
    symbol_table.build(&statements).map_err(|_| "Symbol error".to_string())?;

    let mut assembler = assembler::Assembler::new(config::TEXT_BASE, config::DATA_BASE);
    if let Err(errors) = assembler.assemble(&statements, &symbol_table) {
        let mut msg = String::new();
        for err in errors {
            msg.push_str(&format!("Line {}: {}\n", err.line, err.message));
        }
        return Err(msg);
    }

    app.processor = Processor::new(config::TEXT_BASE, config::DATA_BASE, config::STACK_BASE, config::STACK_SIZE);
    app.processor.load(&assembler.text_bin, &assembler.data_bin);
    app.logs.push("Assembly successful! CPU reset and loaded.".to_string());
    app.memory_scroll = config::TEXT_BASE; // scroll to text base by default
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
) -> io::Result<()> {
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
                    // Just Load
                    if app.mode == RunMode::Editing {
                        if let Err(e) = compile_and_load(&mut app) {
                            app.logs.push(format!("Compile Error:\n{}", e));
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

                if key.code == KeyCode::F(5) { // Run
                    if app.mode == RunMode::Editing {
                        if let Err(e) = compile_and_load(&mut app) {
                            app.logs.push(format!("Compile Error:\n{}", e));
                            continue;
                        }
                    }
                    app.mode = RunMode::Running;
                    loop {
                        match app.processor.step() {
                            Ok(_) => {}
                            Err(e) => {
                                app.logs.push(format!("Halted: {:?}", e));
                                app.mode = RunMode::Editing;
                                break;
                            }
                        }
                    }
                    continue;
                }

                if key.code == KeyCode::F(10) { // Step
                    if app.mode == RunMode::Editing {
                        if let Err(e) = compile_and_load(&mut app) {
                            app.logs.push(format!("Compile Error:\n{}", e));
                            continue;
                        }
                        app.mode = RunMode::Stepping;
                    }
                    match app.processor.step() {
                        Ok(_) => {}
                        Err(e) => {
                            app.logs.push(format!("Halted: {:?}", e));
                            app.mode = RunMode::Editing;
                        }
                    }
                    continue;
                }

                match app.active_pane {
                    Pane::Editor => {
                        app.editor.input(key);
                        app.mode = RunMode::Editing;
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
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

mod ui {
    use super::*;
    use ratatui::{
        layout::{Constraint, Direction, Layout},
        style::{Color, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Paragraph},
        Frame,
    };

    pub fn draw(f: &mut Frame, app: &mut App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
            Constraint::Length(3),  // Top bar
            Constraint::Min(10),    // Middle section
            Constraint::Length(10), // Bottom logs
            ])
            .split(f.area());

        // Top bar
        let top_msg = Paragraph::new(format!(
            "Mode: {:?} | Format (F9): {:?} | Pane (Tab): {:?} | PC: 0x{:08x} | Press ESC to quit",
            app.mode, app.number_format, app.active_pane, app.processor.pc()
        ))
        .block(Block::default().borders(Borders::ALL));
        f.render_widget(top_msg, chunks[0]);

        // Middle section
        let middle_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(60), // Editor
                Constraint::Percentage(20), // Registers
                Constraint::Percentage(20), // Memory
            ])
            .split(chunks[1]);

        // Editor
        let editor_style = if app.active_pane == Pane::Editor { Style::default().fg(Color::Yellow) } else { Style::default() };
        app.editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(editor_style)
                .title("Code Editor (F2: Load, F5: Run, F10: Step, Tab: Switch)"),
        );
        f.render_widget(app.editor.widget(), middle_chunks[0]);

        // Registers
        let mut reg_str = String::new();
        let regs = app.processor.registers();
        for i in 0..32 {
            match app.number_format {
                NumFormat::Hex => reg_str.push_str(&format!("x{:<2}: 0x{:08x}\n", i, regs[i])),
                NumFormat::Binary => reg_str.push_str(&format!("x{:<2}: 0b{:032b}\n", i, regs[i])),
                NumFormat::Decimal => reg_str.push_str(&format!("x{:<2}: {:<10}\n", i, regs[i] as i32)),
            }
        }
        let regs_style = if app.active_pane == Pane::Registers { Style::default().fg(Color::Yellow) } else { Style::default() };
        let regs_p = Paragraph::new(reg_str)
            .scroll((app.registers_scroll, 0))
            .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(regs_style)
                .title("Registers"),
        );
        f.render_widget(regs_p, middle_chunks[1]);

        // Memory
        let mem_start = app.memory_scroll;
        let mem_size_words = 16; 
        
        // We use a Vec of Lines so we can color individual addresses, such as the active PC
        let mut mem_lines: Vec<Line> = Vec::new();
        
        for i in 0..mem_size_words {
            let addr = mem_start + (i * 4);
            match app.processor.read_memory_word(addr) {
                Ok(word) => {
                    let formatted = match app.number_format {
                        NumFormat::Hex => format!("0x{:08x}: 0x{:08x}", addr, word),
                        NumFormat::Binary => format!("0x{:08x}: 0b{:032b}", addr, word),
                        NumFormat::Decimal => format!("0x{:08x}: {:<11}", addr, word),
                    };

                    // If this address is the current Program Counter, highlight it in Green
                    if addr == app.processor.pc() {
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

        let mem_style = if app.active_pane == Pane::Memory { Style::default().fg(Color::Yellow) } else { Style::default() };
        let mem_p = Paragraph::new(mem_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(mem_style)
                .title("Memory"),
        );
        f.render_widget(mem_p, middle_chunks[2]);

        // Logs
        let logs_style = if app.active_pane == Pane::Logs { Style::default().fg(Color::Yellow) } else { Style::default() };
        let logs_text = app.logs.join("\n");
        let logs = Paragraph::new(logs_text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(logs_style)
                .title("Execution Logs"),
        );
        f.render_widget(logs, chunks[2]);
    }
}
