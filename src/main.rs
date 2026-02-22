mod config;
mod lexer;
mod parser;
mod symbols;
mod assembler;
mod processor;
mod tui;

fn main() -> Result<(), std::io::Error> {
    tui::run()
}
