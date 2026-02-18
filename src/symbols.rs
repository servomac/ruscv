use std::collections::HashMap;
use crate::parser::{Statement, StatementKind, Operand};

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

        let mut current_section = ".text";

        for stmt in statements {
            match &stmt.kind {
                StatementKind::Directive(name, _) if name == ".text" || name == ".data" => {
                    current_section = name.as_str();
                }

                StatementKind::Label(name) => {
                    let address = if current_section == ".text" {
                        self.text_base + text_offset
                    } else {
                        self.data_base + data_offset
                    };

                    self.add_label(name.clone(), address)?;
                }

                StatementKind::Instruction(_, _) => {
                    if current_section == ".text" {
                        text_offset += 4;
                    } else {
                        // Opcional: Error si hay instrucciones en sección de datos
                        return Err("Error: Instruction found on .data section".to_string());
                    }
                }

                StatementKind::Directive(name, operands) => {
                    let current_pc = if current_section == ".text" {
                        self.text_base + text_offset
                    } else {
                        self.data_base + data_offset
                    };

                    let size = self.calculate_directive_size(name, operands, current_pc)?;

                    if current_section == ".text" {
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
    fn calculate_directive_size(&self, name: &str, operands: &[Operand], current_pc: u32) -> Result<u32, String> {
        match name {
            ".align" => {
                if let Some(Operand::Immediate(pow)) = operands.get(0) {
                    let alignment = 2u32.pow(*pow as u32);
                    let aligned_pc = (current_pc + alignment - 1) & !(alignment - 1);
                    Ok(aligned_pc - current_pc)
                } else {
                    Err("Directive .align requieres a power of 2 parameter".into())
                }
            },
            ".word"  => Ok((operands.len() as u32) * 4),
            ".half"  => Ok((operands.len() as u32) * 2),
            ".byte"  => Ok(operands.len() as u32),
            ".ascii" | ".asciz" | ".string" => {
                let mut total = 0;
                let has_null = name != ".ascii";

                for op in operands {
                    if let Operand::StringLiteral(s) = op {
                        total += s.len() as u32;
                        if has_null { total += 1; }
                    } else {
                        return Err(format!("Directive {} requires a string literal", name));
                    }
                }
                Ok(total)
            },
            // TODO review
            ".space" => {
                if let Some(Operand::Immediate(n)) = operands.get(0) {
                    Ok(*n as u32)
                } else {
                    Err("Directive .space requires an inmediate value".into())
                }
            },
            _ => Ok(0),
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

        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        let mut sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        sym_table.build(&statements).unwrap();

        assert_eq!(sym_table.get_address("main"), Some(config::TEXT_BASE));
        assert_eq!(sym_table.get_address("final"), Some(config::TEXT_BASE + 4)); // 4 bytes for the instruction
        assert_eq!(sym_table.get_address("msg"), Some(config::DATA_BASE));
        assert_eq!(sym_table.get_address("num"), Some(config::DATA_BASE + 4)); // 3 bytes for the string "Hi!" + 1 for \0
        assert_eq!(sym_table.get_address("text"), Some(config::DATA_BASE + 4 + 4)); // 4 bytes for the word
    }

    #[test]
    fn test_symbol_table_with_align() {
        let source = r#"
            .data
            .string "Hi"
            .align 4
            my_aligned_label: .byte 0xFF
        "#;

        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        let mut sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        sym_table.build(&statements).unwrap();

        assert_eq!(sym_table.get_address("my_aligned_label"), Some(config::DATA_BASE + 0x10)) // 3 for "Hi" + 1 for \0, then aligned to 4 bytes
    }

    #[test]
    fn test_duplicated_label() {
        let source = r#"
            .data
            msg: .asciz "Hi!"
            msg: .asciz "Hello!"
        "#;

        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        let mut sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        assert!(sym_table.build(&statements).is_err());
    }
}
