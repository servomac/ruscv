mod lexer;
use lexer::tokenize;

mod parser;
use parser::Parser;

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

#[derive(Clone)]
struct Instruction {
    asm: String,
    machine_code: u32,
}

struct Assembler {
    code: Vec<Instruction>,
}

impl Assembler {
    fn new() -> Self {
        Assembler { code: Vec::new() }
    }

    fn assemble(&mut self, assembler: &str) -> Vec<Instruction> {
        // TODO implement tokenize lexer ISA etc
        let tokens = tokenize(assembler);
        for token in &tokens {
            println!("Token: {:?}", token);
            self.code.push(Instruction {
                asm: assembler.to_string(),
                machine_code: 0,
            });
        }
        let mut parser = Parser::new(tokens);
        parser.parse();
        // Dummy implementation for illustration
        self.code.clone()
    }
}


fn main() {
    let mut assembler = Assembler::new();
    let instructions = assembler.assemble("add x20, x19, x18");
    let p = Processor::new(128);
    p.show_state();
    println!("{}", p.memory_dump());
}
