use crate::parser::{DirectiveKind, Operand, Section, Statement, StatementKind};
use std::collections::HashMap;

pub struct SymbolTable {
    symbols: HashMap<String, u32>,
    text_base: u32,
    data_base: u32,
}

impl SymbolTable {
    pub fn new(text_base: u32, data_base: u32) -> Self {
        Self {
            symbols: HashMap::new(),
            text_base,
            data_base,
        }
    }

    pub fn build(&mut self, statements: &[Statement]) -> Result<(), String> {
        let mut text_offset: u32 = 0;
        let mut data_offset: u32 = 0;

        let mut current_section = Section::Text;

        for stmt in statements {
            match &stmt.kind {
                StatementKind::Directive(DirectiveKind::Text, _) => {
                    current_section = Section::Text;
                }
                StatementKind::Directive(DirectiveKind::Data, _) => {
                    current_section = Section::Data;
                }

                StatementKind::Label(name) => {
                    let address = if current_section == Section::Text {
                        self.text_base + text_offset
                    } else {
                        self.data_base + data_offset
                    };

                    self.add_label(name.clone(), address)?;
                }

                StatementKind::Instruction(_, _) => {
                    if current_section == Section::Text {
                        text_offset += 4;
                    } else {
                        return Err("Instruction found in .data section".to_string());
                    }
                }

                StatementKind::Directive(kind, operands) => {
                    let current_pc = if current_section == Section::Text {
                        self.text_base + text_offset
                    } else {
                        self.data_base + data_offset
                    };

                    let size = self.calculate_directive_size(kind, operands, current_pc)?;

                    if current_section == Section::Text {
                        text_offset += size;
                    } else {
                        data_offset += size;
                    }
                }
            }
        }
        Ok(())
    }

    // Size in bytes that the directive will occupy in memory
    fn calculate_directive_size(
        &self,
        kind: &DirectiveKind,
        operands: &[Operand],
        current_pc: u32,
    ) -> Result<u32, String> {
        match kind {
            DirectiveKind::Align => {
                if let Some(Operand::Immediate(pow)) = operands.get(0) {
                    let alignment = 2u32.pow(*pow as u32);
                    let aligned_pc = (current_pc + alignment - 1) & !(alignment - 1);
                    Ok(aligned_pc - current_pc)
                } else {
                    Err("Directive .align requires a power of 2 parameter".into())
                }
            }
            DirectiveKind::Word => Ok((operands.len() as u32) * 4),
            DirectiveKind::Half => Ok((operands.len() as u32) * 2),
            DirectiveKind::Byte => Ok(operands.len() as u32),
            DirectiveKind::Ascii => {
                let mut total = 0;
                for op in operands {
                    if let Operand::StringLiteral(s) = op {
                        total += s.len() as u32;
                    } else {
                        return Err("Directive .ascii requires a string literal".into());
                    }
                }
                Ok(total)
            }
            DirectiveKind::Asciz => {
                let mut total = 0;
                for op in operands {
                    if let Operand::StringLiteral(s) = op {
                        total += s.len() as u32 + 1; // +1 for null terminator
                    } else {
                        return Err("Directive .asciz requires a string literal".into());
                    }
                }
                Ok(total)
            }
            DirectiveKind::Space => {
                if let Some(Operand::Immediate(n)) = operands.get(0) {
                    Ok(*n as u32)
                } else {
                    Err("Directive .space requires an immediate value".into())
                }
            }
            DirectiveKind::Balign => {
                if let Some(Operand::Immediate(n)) = operands.get(0) {
                    if *n < 1 {
                        return Ok(0);
                    }
                    let alignment = *n as u32;
                    let aligned_pc = (current_pc + alignment - 1) & !(alignment - 1);
                    Ok(aligned_pc - current_pc)
                } else {
                    Err("Directive .balign requires a byte-count parameter".into())
                }
            }
            DirectiveKind::Unknown(name) => Err(format!("Unknown directive '{}'", name)),
            // Section switches and no-ops are handled before calculate_directive_size is called.
            DirectiveKind::Text | DirectiveKind::Data | DirectiveKind::Globl => Ok(0),
        }
    }

    pub fn get_address(&self, label: &str) -> Option<u32> {
        self.symbols.get(label).cloned()
    }

    pub fn add_label(&mut self, label: String, address: u32) -> Result<(), String> {
        if self.symbols.contains_key(&label) {
            Err(format!("Error: Duplicated label '{}'", label))
        } else {
            self.symbols.insert(label, address);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    use super::*;
    use crate::config;

    #[test]
    fn test_symbol_table() {
        let source = "
            .data
            msg: .asciz \"Hi!\"
            num: .word 42

            .text
            main:
                addi x1, x0, 42
            final:

            .data
            text: .asciz \"This is a test\"
        ";

        let tokens = tokenize(source).unwrap();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        let mut sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        sym_table.build(&statements).unwrap();

        assert_eq!(sym_table.get_address("main"), Some(config::TEXT_BASE));
        assert_eq!(sym_table.get_address("final"), Some(config::TEXT_BASE + 4)); // 4 bytes for the instruction
        assert_eq!(sym_table.get_address("msg"), Some(config::DATA_BASE));
        assert_eq!(sym_table.get_address("num"), Some(config::DATA_BASE + 4)); // 3 bytes for the string "Hi!" + 1 for \0
        assert_eq!(
            sym_table.get_address("text"),
            Some(config::DATA_BASE + 4 + 4)
        ); // 4 bytes for the word
    }

    #[test]
    fn test_symbol_table_with_align() {
        let source = r#"
            .data
            .string "Hi"
            .align 4
            my_aligned_label: .byte 0xFF
        "#;

        let tokens = tokenize(source).unwrap();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        let mut sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        sym_table.build(&statements).unwrap();

        assert_eq!(
            sym_table.get_address("my_aligned_label"),
            Some(config::DATA_BASE + 0x10)
        ) // "Hi\0" = 3 bytes, then padded to 16-byte boundary (2^4)
    }

    #[test]
    fn test_unknown_label_returns_none() {
        let mut sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        assert_eq!(sym_table.get_address("nonexistent"), None);
    }

    #[test]
    fn test_duplicated_label() {
        let source = r#"
            .data
            msg: .asciz "Hi!"
            msg: .asciz "Hello!"
        "#;

        let tokens = tokenize(source).unwrap();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        let result = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE).build(&statements);
        assert!(result.is_err());
    }
}
