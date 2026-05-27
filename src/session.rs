use crate::{
    assembler, config, lexer, parser,
    processor::{Processor, StepError},
    pseudo, symbols,
};

pub enum CompileError {
    Lex(lexer::LexError),
    Parse(parser::ParseError),
    Pseudo(String),
    Symbol(String),
    Assemble(Vec<assembler::AssemblerError>),
}

pub struct Session {
    pub processor: Processor,
    pub debug_info: Option<assembler::DebugInfo>,
    pub prev_registers: [u32; config::NUM_REGISTERS],
}

impl Session {
    pub fn new() -> Self {
        let processor = Processor::new();
        let prev_registers = *processor.registers();
        Self {
            processor,
            debug_info: None,
            prev_registers,
        }
    }

    pub fn load_source(&mut self, source: &str) -> Result<(), CompileError> {
        let tokens = lexer::tokenize(source).map_err(CompileError::Lex)?;
        let statements = parser::Parser::new(tokens)
            .parse()
            .map_err(CompileError::Parse)?;
        let statements =
            pseudo::expand(statements).map_err(|e| CompileError::Pseudo(e.to_string()))?;
        let mut sym = symbols::SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        sym.build(&statements)
            .map_err(|e| CompileError::Symbol(e.to_string()))?;
        let program = assembler::Assembler::new(config::TEXT_BASE, config::DATA_BASE)
            .assemble(&statements, &sym)
            .map_err(CompileError::Assemble)?;

        self.processor = Processor::new();
        self.processor.load(&program.text_bin, &program.data_bin);
        self.prev_registers = *self.processor.registers();
        self.debug_info = Some(program.debug_info);
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), StepError> {
        self.prev_registers = *self.processor.registers();
        self.processor.step()
    }

    pub fn run_to_halt(&mut self) -> StepError {
        self.prev_registers = *self.processor.registers();
        loop {
            match self.processor.step() {
                Ok(()) => {}
                Err(e) => return e,
            }
        }
    }

    pub fn drain_uart(&mut self) -> Vec<u8> {
        self.processor.drain_uart()
    }
}
