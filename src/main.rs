mod config;

mod lexer;
use lexer::tokenize;

mod parser;
use parser::Parser;

mod symbols;
use symbols::SymbolTable;

mod assembler;
use assembler::Assembler;


// TODO: this is not a good way to represent memory, it should be a
// contiguous block of memory with different segments
struct Memory {
    text: Vec<u8>,
    data: Vec<u8>,
    stack: Vec<u8>,
    text_base: u32,
    data_base: u32,
    stack_base: u32,
}

struct Processor {
    pc: u32,
    registers: [u32; config::NUM_REGISTERS],
    memory: Memory,
}

impl Processor {
    fn new(text_base: u32, data_base: u32, stack_base: u32, stack_size: usize) -> Self {
        Processor {
            pc: 0,                      // filled by load
            registers: [0; config::NUM_REGISTERS],
            memory: Memory {
                text: Vec::new(),       // filled by load
                data: Vec::new(),       // filled by load
                stack: vec![0u8; stack_size],  // pre-allocated, grows downward from stack_base
                text_base,
                data_base,
                stack_base,
            },
        }
    }

    fn load(&mut self, text: &Vec<u8>, data: &Vec<u8>) {
        self.memory.text = text.clone();
        self.memory.data = data.clone();
        self.pc = self.memory.text_base;
    }

    fn show_state(&self) {
        println!("PC: {}", self.pc);
        println!("Registers: {:?}", self.registers);
    }

}



fn main() {
    let tokens = tokenize("add x20, x19, x18");
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
