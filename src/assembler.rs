use crate::parser::Statement;
use crate::symbols::SymbolTable;


#[derive(Clone)]
pub struct Instruction {
    asm: String,
    machine_code: u32,
}

pub struct Assembler {
    code: Vec<Instruction>,
}

impl Assembler {
    pub fn new() -> Self {
        Assembler { code: Vec::new() }
    }

    pub fn assemble(&mut self, statements: &[Statement], sym_table: &SymbolTable) -> Vec<Instruction> {
        // TODO implement tokenize lexer ISA etc

        // Dummy implementation for illustration
        self.code.clone()
    }
}
