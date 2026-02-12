use std::collections::HashMap;
use crate::parser::{Statement, Operand};

pub struct SymbolTable {
    symbols: HashMap<String, u32>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    pub fn build(&mut self, statements: &[Statement]) -> Result<(), String> {
        let text_base: u32 = 0x0040_0000;
        let data_base: u32 = 0x1001_0000;

        let mut text_offset: u32 = 0;
        let mut data_offset: u32 = 0;

        let mut current_section = ".text";

        for stmt in statements {
            match stmt {
                Statement::Directive(name, _) if name == ".text" || name == ".data" => {
                    current_section = name.as_str();
                }

                Statement::Label(name) => {
                    if self.symbols.contains_key(name) {
                        return Err(format!("Error: Duplicated label '{}'", name));
                    }

                    let address = if current_section == ".text" {
                        text_base + text_offset
                    } else {
                        data_base + data_offset
                    };

                    self.symbols.insert(name.clone(), address);
                }

                Statement::Instruction(_, _) => {
                    if current_section == ".text" {
                        text_offset += 4;
                    } else {
                        // Opcional: Error si hay instrucciones en sección de datos
                        return Err("Error: Instruction found on .data section".to_string());
                    }
                }

                Statement::Directive(name, operands) => {
                    let current_pc = if current_section == ".text" {
                        text_base + text_offset
                    } else {
                        data_base + data_offset
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
}

#[cfg(test)]
mod tests {
    use crate::lexer::tokenize;
    use crate::parser::{Parser, Statement, Operand};

    use super::*;

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

        let mut sym_table = SymbolTable::new();
        sym_table.build(&statements).unwrap();

        assert_eq!(sym_table.get_address("main"), Some(0x0040_0000));
        assert_eq!(sym_table.get_address("final"), Some(0x0040_0004)); // 4 bytes for the instruction
        assert_eq!(sym_table.get_address("msg"), Some(0x1001_0000));
        assert_eq!(sym_table.get_address("num"), Some(0x1001_0004)); // 3 bytes for the string "Hi!" + 1 for \0
        assert_eq!(sym_table.get_address("text"), Some(0x1001_0004 + 4)); // 4 bytes for the word
    }

    #[test]
    fn text_symbol_table_with_align() {
        let source = "
            .data
            .string \"Hi\"
            .align 4
            my_aligned_label: .byte 0xFF
        ";

        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        let mut sym_table = SymbolTable::new();
        sym_table.build(&statements).unwrap();

        assert_eq!(sym_table.get_address("my_aligned_label"), Some(0x1001_0010)) // 3 for "Hi" + 1 for \0, then aligned to 4 bytes
    }
}
