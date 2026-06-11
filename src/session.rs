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

#[cfg(test)]
mod tests {
    use super::*;

    const RA: usize = 1;
    const T0: usize = 5;
    const A0: usize = 10;
    const A1: usize = 11;

    // Assemble `source`, execute `steps` instructions, and return the session for
    // assertions on architectural state. These end-to-end tests are what catch bugs
    // that statement-level pseudo/encoder tests cannot (e.g. la expanding to
    // pc + symbol instead of symbol).
    fn compile_and_run(source: &str, steps: usize) -> Session {
        let mut session = Session::new();
        if let Err(e) = session.load_source(source) {
            let msg = match e {
                CompileError::Lex(e) => format!("lex error at line {}: {}", e.line, e),
                CompileError::Parse(e) => format!("parse error at line {}: {}", e.line, e),
                CompileError::Pseudo(msg) => format!("pseudo error: {}", msg),
                CompileError::Symbol(msg) => format!("symbol error: {}", msg),
                CompileError::Assemble(errors) => errors
                    .iter()
                    .map(|e| format!("asm error at line {}: {}", e.line, e.message))
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            panic!("compilation failed:\n{}", msg);
        }
        for i in 0..steps {
            session
                .step()
                .unwrap_or_else(|e| panic!("step {} failed: {:?}", i + 1, e));
        }
        session
    }

    #[test]
    fn test_la_loads_exact_symbol_address() {
        let session = compile_and_run(
            ".data\nmsg: .word 1\n.text\nstart: la a0, msg\n",
            2, // auipc + addi
        );
        assert_eq!(session.processor.registers()[A0], config::DATA_BASE);
    }

    #[test]
    fn test_la_backward_label_with_negative_pcrel_hi20() {
        // Put the target >2048 bytes behind the la so %pcrel_hi resolves to a
        // negative hi20, which auipc must encode (lui/auipc accept negative values).
        let nops = "nop\n".repeat(600);
        let source = format!(".text\ntarget: {}la a0, target\n", nops);
        let session = compile_and_run(&source, 600 + 2);
        assert_eq!(session.processor.registers()[A0], config::TEXT_BASE);
    }

    #[test]
    fn test_call_reaches_target_and_links_ra() {
        // call occupies 8 bytes (auipc + jalr), nop is at +8, func at +12.
        let session = compile_and_run(
            ".text\nstart: call func\nnop\nfunc: nop\n",
            2, // auipc + jalr
        );
        assert_eq!(session.processor.pc(), config::TEXT_BASE + 12);
        assert_eq!(
            session.processor.registers()[RA],
            config::TEXT_BASE + 8,
            "ra must point at the instruction after the call"
        );
    }

    #[test]
    fn test_tail_reaches_target_without_linking() {
        let session = compile_and_run(".text\nstart: tail func\nnop\nfunc: nop\n", 2);
        assert_eq!(session.processor.pc(), config::TEXT_BASE + 12);
        assert_eq!(session.processor.registers()[RA], 0);
    }

    #[test]
    fn test_load_global_pseudo_reads_data_word() {
        let session = compile_and_run(
            ".data\nval: .word 0x12345678\n.text\nstart: lw a1, val\n",
            2, // auipc + lw
        );
        assert_eq!(session.processor.registers()[A1], 0x12345678);
    }

    #[test]
    fn test_store_global_pseudo_writes_data_word() {
        let session = compile_and_run(
            ".data\nval: .word 0\n.text\nstart: li a0, 42\nsw a0, val, t0\n",
            3, // addi + auipc + sw
        );
        assert_eq!(
            session.processor.read_memory_word(config::DATA_BASE),
            Ok(42)
        );
        // The scratch register holds the auipc result, not zero.
        assert_ne!(session.processor.registers()[T0], 0);
    }

    #[test]
    fn test_li_large_negative_value() {
        // hi20 of -2049 is -1: requires lui to accept negative 20-bit immediates.
        let session = compile_and_run(".text\nstart: li a0, -2049\n", 2);
        assert_eq!(session.processor.registers()[A0], -2049i32 as u32);
    }

    #[test]
    fn test_li_value_with_bit31_set() {
        let session = compile_and_run(".text\nstart: li a0, 0xDEADBEEF\n", 2);
        assert_eq!(session.processor.registers()[A0], 0xDEADBEEF);
    }

    #[test]
    fn test_li_i32_min() {
        let session = compile_and_run(".text\nstart: li a0, -2147483648\n", 2);
        assert_eq!(session.processor.registers()[A0], 0x8000_0000);
    }

    #[test]
    fn test_explicit_pcrel_modifiers_in_source() {
        // Hand-written equivalent of la: the %pcrel_lo pairs with the auipc on the
        // previous line.
        let session = compile_and_run(
            ".data\nmsg: .word 1\n.text\nstart: auipc a0, %pcrel_hi(msg)\naddi a0, a0, %pcrel_lo(msg)\n",
            2,
        );
        assert_eq!(session.processor.registers()[A0], config::DATA_BASE);
    }

    #[test]
    fn test_unsigned_load_global_pseudos() {
        // 0x8081 has bit 7 and bit 15 set: lbu/lhu must zero-extend where
        // lb/lh would sign-extend.
        let session = compile_and_run(
            ".data\nval: .word 0x8081\n.text\nstart: lbu a0, val\nlhu a1, val\n",
            4,
        );
        assert_eq!(session.processor.registers()[A0], 0x81);
        assert_eq!(session.processor.registers()[A1], 0x8081);
    }

    #[test]
    fn test_bare_paren_memory_operand_loads_and_stores() {
        // GAS accepts `(reg)` as `0(reg)`.
        let session = compile_and_run(
            ".data\nval: .word 99\n.text\nstart: la a0, val\nlw a1, (a0)\naddi t0, a1, 1\nsw t0, (a0)\nlw a1, (a0)\n",
            6,
        );
        assert_eq!(session.processor.registers()[T0], 100);
        assert_eq!(session.processor.registers()[A1], 100);
    }

    // The assembler is fed from an interactive editor: garbage input must produce
    // a compile error, never a panic and never a multi-GB allocation.
    #[test]
    fn test_garbage_directive_operands_error_instead_of_panicking() {
        let cases = [
            ".data\n.align 32",
            ".data\n.align -1",
            ".data\n.align 2147483647",
            ".data\n.align -2147483648",
            ".text\n.align 32",
            ".data\n.balign 0",
            ".data\n.balign -3",
            ".data\n.space -1",
            ".data\n.space -2147483648",
            ".data\n.space 2000000000",
            ".text\n.space 2000000000",
            ".data\n.space 67108864", // == DRAM_SIZE: exceeds the data section cap
            ".data\n.byte 256",
            ".data\n.half 65536",
        ];
        for src in cases {
            let mut session = Session::new();
            assert!(
                session.load_source(src).is_err(),
                "expected a compile error for {:?}",
                src
            );
        }
    }

    #[test]
    fn test_valid_space_and_align_still_work() {
        let session = compile_and_run(
            ".data\nbuf: .space 6\n.align 2\nval: .word 42\n.text\nstart: la a0, val\nlw a1, 0(a0)\n",
            3,
        );
        assert_eq!(session.processor.registers()[A0], config::DATA_BASE + 8);
        assert_eq!(session.processor.registers()[A1], 42);
    }
}
