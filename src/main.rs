mod config;
mod lexer;

mod parser;
use parser::Parser;

mod symbols;
use symbols::SymbolTable;

mod assembler;
use assembler::Assembler;

mod processor;
use processor::Processor;

mod tui;

fn main() -> Result<(), std::io::Error> {
    let p = Processor::new(config::TEXT_BASE, config::DATA_BASE, config::STACK_BASE, config::STACK_SIZE);
    tui::run(p)
}
