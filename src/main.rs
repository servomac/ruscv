mod lexer;
use lexer::tokenize;

mod parser;
use parser::Parser;

mod symbols;
use symbols::SymbolTable;

mod assembler;
use assembler::Assembler;

const NUM_REGISTERS: usize = 32;

struct Processor {
    pc: i32,
    registers: [i32; NUM_REGISTERS],
    memory: Vec<i32>,
}

impl Processor {
    fn new(memory_size: usize) -> Self {
        Processor {
            pc: 0,
            registers: [0; NUM_REGISTERS],
            memory: vec![0; memory_size],
        }
    }

    fn show_state(&self) {
        println!("PC: {}", self.pc);
        println!("Registers: {:?}", self.registers);
    }

    fn memory_dump(&self) -> String {
        self.memory.iter()
            .map(|value| format!("{:08X}", value))
            .collect::<Vec<String>>()
            .join("\n")
    }
}



fn main() {
    let tokens = tokenize("add x20, x19, x18");
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();

    let mut symbol_table = SymbolTable::new();
    symbol_table.build(&statements).expect("Symbol table build failed");

    let mut assembler = Assembler::new();
    match assembler.assemble(&statements, &symbol_table) {
        Ok(()) => {
            let p = Processor::new(128);
            p.show_state();
            println!("{}", p.memory_dump());
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
