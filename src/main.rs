mod config;
mod lexer;
mod parser;
mod symbols;
mod assembler;
mod processor;
mod pseudo;
mod tui;

fn main() -> Result<(), std::io::Error> {
    let args: Vec<String> = std::env::args().collect();
    let initial_file = args.get(1).cloned();
    tui::run(initial_file)
}
