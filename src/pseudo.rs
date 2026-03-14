use crate::lexer::ModifierKind;
use crate::parser::{Operand, Statement, StatementKind};

pub fn expand(statements: Vec<Statement>) -> Result<Vec<Statement>, String> {
    let mut expanded_statements = Vec::new();
    for statement in statements {
        expanded_statements.extend(expand_statement(&statement)?);
    }
    Ok(expanded_statements)
}

// Given an statement, return it as [statement] if it is not a pseudo-instruction.
// If it is a pseudo-instruction, expand it to one or more base instructions
// and return the new list of instructions.
fn expand_statement(statement: &Statement) -> Result<Vec<Statement>, String> {
    match &statement.kind {
        StatementKind::Instruction(name, ops) => {
            match name.as_str() {
                "j" => {
                    if ops.len() == 1 {
                        let offset = ops[0].clone();
                        // TODO validate is a valid offset? if not, the assembler will fail later and reference an instruction jal with an invalid offset
                        Ok(vec![Statement {
                            kind: StatementKind::Instruction("jal".to_string(), vec![Operand::Register(0), offset]),
                            line: statement.line,
                        }])
                    } else {
                        // TODO make the error message more similar to those in the assembler.rs i.e. "Invalid operands for J-type instruction"
                        Err(format!("Invalid number of operands for 'j' pseudo-instruction. Expected 1, got {}", ops.len()))
                    }
                }
                "jal" => {
                    if ops.len() == 1 {
                        let offset = ops[0].clone();
                        // TODO validate is a valid offset? if not, the assembler will fail later and reference an instruction jal already expanded
                        Ok(vec![Statement {
                            kind: StatementKind::Instruction("jal".to_string(), vec![Operand::Register(1), offset]),
                            line: statement.line,
                        }])
                    } else {
                        Ok(vec![statement.clone()])
                    }
                }
                "jr" => {
                    if ops.len() == 1 {
                        let rs = ops[0].clone();
                        let rs_reg = match rs {
                            Operand::Register(n) => n,
                            _ => return Err(format!("Invalid operand for 'jr' pseudo-instruction. Expected a register, got {}", rs)),
                        };
                        Ok(vec![Statement {
                            kind: StatementKind::Instruction("jalr".to_string(), vec![
                                Operand::Register(0), Operand::Register(rs_reg), Operand::Immediate(0)
                            ]),
                            line: statement.line,
                        }])
                    } else {
                        Err(format!("Invalid number of operands for 'jr' pseudo-instruction. Expected 1, got {}", ops.len()))
                    }
                }
                "jalr" => {
                    if ops.len() == 1 {
                        let rs = ops[0].clone();
                        let rs_reg = match rs {
                            Operand::Register(n) => n,
                            _ => return Err(format!("Invalid operand for 'jalr' pseudo-instruction. Expected a register, got {}", rs)),
                        };
                        Ok(vec![Statement {
                            kind: StatementKind::Instruction("jalr".to_string(), vec![
                                Operand::Register(1), Operand::Register(rs_reg), Operand::Immediate(0)
                            ]),
                            line: statement.line,
                        }])
                    } else {
                        Ok(vec![statement.clone()])
                    }
                }
                "ret" => {
                    if ops.len() == 0 {
                        Ok(vec![Statement {
                            kind: StatementKind::Instruction("jalr".to_string(), vec![
                                Operand::Register(0),
                                Operand::Register(1),
                                Operand::Immediate(0)
                            ]),
                            line: statement.line,
                        }])
                    } else {
                        Err(format!("Invalid number of operands for 'ret' pseudo-instruction. Expected 0, got {}", ops.len()))
                    }
                }
                "call" => {
                    if ops.len() == 1 {
                        let offset = ops[0].clone();
                        // Validate offset is an Immediate or Label
                        let (offset_high, offset_low) = match offset {
                            Operand::Immediate(imm) => (
                                Operand::Immediate((imm + 0x800) >> 12),
                                Operand::Immediate((imm << 20) >> 20),
                            ),
                            Operand::Label(label) => (
                                Operand::Modifier(ModifierKind::Hi, label.clone()),
                                Operand::Modifier(ModifierKind::Lo, label.clone())
                            ),
                            _ => return Err(format!("Invalid operand for 'call' pseudo-instruction. Expected an immediate or label, got {}", offset)),
                        };
                        Ok(vec![Statement {
                            kind: StatementKind::Instruction("auipc".to_string(), vec![
                                Operand::Register(1),
                                offset_high,
                            ]),
                            line: statement.line,
                        },
                            Statement {
                            kind: StatementKind::Instruction("jalr".to_string(), vec![
                                Operand::Register(1),
                                Operand::Register(1),
                                offset_low,
                            ]),
                            line: statement.line,
                        }])
                    } else {
                        Err(format!("Invalid number of operands for 'call' pseudo-instruction. Expected 1, got {}", ops.len()))
                    }
                }
                "tail" => {
                    if ops.len() == 1 {
                        let offset = ops[0].clone();
                        // Validate offset is an Immediate or Label
                        let (offset_high, offset_low) = match offset {
                            Operand::Immediate(imm) => (
                                Operand::Immediate((imm + 0x800) >> 12),
                                Operand::Immediate((imm << 20) >> 20),
                            ),
                            Operand::Label(label) => (
                                Operand::Modifier(ModifierKind::Hi, label.clone()),
                                Operand::Modifier(ModifierKind::Lo, label.clone())
                            ),
                            _ => return Err(format!("Invalid operand for 'tail' pseudo-instruction. Expected an immediate or label, got {}", offset)),
                        };
                        Ok(vec![Statement {
                            kind: StatementKind::Instruction("auipc".to_string(), vec![
                                Operand::Register(6),
                                offset_high,
                            ]),
                            line: statement.line,
                        },
                            Statement {
                            kind: StatementKind::Instruction("jalr".to_string(), vec![
                                Operand::Register(0),
                                Operand::Register(6),
                                offset_low,
                            ]),
                            line: statement.line,
                        }])
                    } else {
                        Err(format!("Invalid number of operands for 'tail' pseudo-instruction. Expected 1, got {}", ops.len()))
                    }
                }
                _ => Ok(vec![statement.clone()]),
            }
        }
        _ => Ok(vec![statement.clone()]),
    }
}


// TODO Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_j() {
        let statement = Statement {
            kind: StatementKind::Instruction("j".to_string(), vec![Operand::Immediate(10)]),
            line: 1,
        };
        let expanded = expand_statement(&statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("jal".to_string(), vec![Operand::Register(0), Operand::Immediate(10)]));
    }

    #[test]
    fn test_expand_jal() {
        let statement = Statement {
            kind: StatementKind::Instruction("jal".to_string(), vec![Operand::Label("loop".to_string())]),
            line: 1,
        };
        let expanded = expand_statement(&statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction(
            "jal".to_string(),
            vec![Operand::Register(1), Operand::Label("loop".to_string())]
        ));
    }

    #[test]
    fn test_expand_jalr() {
        let statement = Statement {
            kind: StatementKind::Instruction("jalr".to_string(), vec![Operand::Register(9)]),
            line: 1,
        };
        let expanded = expand_statement(&statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction(
            "jalr".to_string(),
            vec![Operand::Register(1), Operand::Register(9), Operand::Immediate(0)]
        ));
    }

    #[test]
    fn test_expand_call() {
        let statement = Statement {
            kind: StatementKind::Instruction("call".to_string(), vec![Operand::Label("loop".to_string())]),
            line: 1,
        };
        let expanded = expand_statement(&statement).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].kind, StatementKind::Instruction(
            "auipc".to_string(),
            vec![Operand::Register(1), Operand::Modifier(ModifierKind::Hi, "loop".to_string())]
        ));
        assert_eq!(expanded[1].kind, StatementKind::Instruction(
            "jalr".to_string(),
            vec![Operand::Register(1), Operand::Register(1), Operand::Modifier(ModifierKind::Lo, "loop".to_string())]
        ));
    }

    #[test]
    fn test_expand_tail() {
        let statement = Statement {
            kind: StatementKind::Instruction("tail".to_string(), vec![Operand::Label("loop".to_string())]),
            line: 1,
        };
        let expanded = expand_statement(&statement).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].kind, StatementKind::Instruction(
            "auipc".to_string(),
            vec![Operand::Register(6), Operand::Modifier(ModifierKind::Hi, "loop".to_string())]
        ));
        assert_eq!(expanded[1].kind, StatementKind::Instruction(
            "jalr".to_string(),
            vec![Operand::Register(0), Operand::Register(6), Operand::Modifier(ModifierKind::Lo, "loop".to_string())]
        ));
    }
}