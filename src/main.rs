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


fn main() {
    let tokens = lexer::tokenize("add x20, x19, x18");
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();

    let mut symbol_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
    symbol_table.build(&statements).expect("Symbol table build failed");

    let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
    match assembler.assemble(&statements, &symbol_table) {
        Ok(()) => {
            let mut p = Processor::new(config::TEXT_BASE, config::DATA_BASE, config::STACK_BASE, config::STACK_SIZE);
            p.load(&assembler.text_bin, &assembler.data_bin);
            p.show_state();
//            println!("{}", p.memory_dump());
        }
        Err(errors) => {
            eprintln!("Assembly failed with {} error(s):", errors.len());
            for error in errors {
                eprintln!("  Line {}: {}", error.line, error.message);
            }
            std::process::exit(1);
        }
    }
}
