use crate::lexer::ModifierKind;
use crate::parser::{MemoryOffset, Operand, Statement, StatementKind};

pub fn expand(statements: Vec<Statement>) -> Result<Vec<Statement>, String> {
    // Preallocate memory for the expanded statements
    let mut expanded_statements = Vec::with_capacity(statements.len());
    for statement in statements {
        expanded_statements.extend(expand_statement(statement)?);
    }
    Ok(expanded_statements)
}

// Given an statement, return it as [statement] if it is not a pseudo-instruction.
// If it is a pseudo-instruction, expand it to one or more base instructions
// and return the new list of instructions.
fn expand_statement(statement: Statement) -> Result<Vec<Statement>, String> {
    let line = statement.line;
    let StatementKind::Instruction(name, ops) = statement.kind else { return Ok(vec![statement]) };

    match name.as_str() {
        "la" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'la' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

            let rd = &ops[0];
            let symbol = &ops[1];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for 'la' pseudo-instruction. Expected a register, got {}", rd)),
            };
            let symbol = match symbol {
                Operand::Label(label) => label.clone(),
                _ => return Err(format!("Invalid second operand for 'la' pseudo-instruction. Expected a label, got {}", symbol)),
            };
            Ok(vec![
                Statement {
                    kind: StatementKind::Instruction("auipc".to_string(), vec![Operand::Register(rd_reg), Operand::Modifier(ModifierKind::Hi, symbol.clone())]),
                    line,
                },
                Statement {
                    kind: StatementKind::Instruction("addi".to_string(), vec![Operand::Register(rd_reg), Operand::Register(rd_reg), Operand::Modifier(ModifierKind::Lo, symbol.clone())]),
                    line,
                }
            ])
        }
        "lb" | "lh" | "lw" => {
            if ops.len() != 2 {
                // If number of operands is not 2, we consider it a base instruction and return it as is.
                // This is not an error because the assembler will fail later and reference an instruction l{b|h|w} with an invalid operand.
                return Ok(vec![Statement { kind: StatementKind::Instruction(name, ops), line }]);
            }
            // auipc rd, symbol[31:12]
            // l{b|h|w} rd, symbol[11:0](rd)
            let rd = &ops[0];
            let symbol = &ops[1];
            // if second operand is a Label, we consider it a pseudo-instruction and expand it.
            // Otherwise, we consider it a base instruction and return it as is.
            let symbol = match symbol {
                Operand::Label(label) => label.clone(),
                _ => return Ok(vec![Statement { kind: StatementKind::Instruction(name, ops), line }])
            };
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for '{}' pseudo-instruction. Expected a register, got {}", name, rd)),
            };
            Ok(vec![
                Statement {
                    kind: StatementKind::Instruction("auipc".to_string(), vec![
                        Operand::Register(rd_reg),
                        Operand::Modifier(ModifierKind::Hi, symbol.clone())
                    ]),
                    line,
                },
                Statement {
                    kind: StatementKind::Instruction(name.to_string(), vec![Operand::Register(rd_reg), Operand::Memory { offset: MemoryOffset::Modifier(ModifierKind::Lo, symbol.clone()), reg: rd_reg }]),
                    line,
                }
            ])
        }
        "sb" | "sh" | "sw" => {
            if ops.len() != 3 {
                // If number of operands is not 3, we consider it a base instruction and return it as is.
                return Ok(vec![Statement { kind: StatementKind::Instruction(name, ops), line }]);
            }
            // Pseudo-instruction: s{b|h|w} rd, symbol, rt
            // Base instructions:  auipc rt, symbol[31:12]
            //                     s{b|h|w} rd, symbol[11:0](rt)
            let rd = &ops[0];
            let symbol = &ops[1];
            let rt = &ops[2];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for '{}' pseudo-instruction. Expected a register, got {}", name, rd)),
            };
            let symbol = match symbol {
                Operand::Label(label) => label.clone(),
                _ => return Err(format!("Invalid second operand for '{}' pseudo-instruction. Expected a label, got {}", name, symbol)),
            };
            let rt_reg = match rt {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid third operand for '{}' pseudo-instruction. Expected a register, got {}", name, rt)),
            };
            Ok(vec![
                Statement {
                    kind: StatementKind::Instruction("auipc".to_string(), vec![
                        Operand::Register(rt_reg),
                        Operand::Modifier(ModifierKind::Hi, symbol.clone())
                    ]),
                    line,
                },
                Statement {
                    kind: StatementKind::Instruction(name.to_string(), vec![
                        Operand::Register(rd_reg),
                        Operand::Memory { offset: MemoryOffset::Modifier(ModifierKind::Lo, symbol.clone()), reg: rt_reg }
                    ]),
                    line,
                }
            ])
        }
        "nop" => {
            if ops.len() == 0 {
                Ok(vec![Statement {
                    kind: StatementKind::Instruction("addi".to_string(), vec![Operand::Register(0), Operand::Register(0), Operand::Immediate(0)]),
                    line,
                }])
            } else {
                Err(format!("Invalid number of operands for 'nop' pseudo-instruction. Expected 0, got {}", ops.len()))
            }
        }
        "li" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'li' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

               let rd = &ops[0];
               let imm_op = &ops[1];
               let rd_reg = match rd {
                   Operand::Register(n) => *n,
                   _ => return Err(format!("Invalid first operand for 'li' pseudo-instruction. Expected a register, got {}", rd)),
               };
               let imm = match imm_op {
                   Operand::Immediate(n) => *n,
                   _ => return Err(format!("Invalid second operand for 'li' pseudo-instruction. Expected an immediate, got {}", imm_op)),
               };

               if (-2048..=2047).contains(&imm) {
                   Ok(vec![Statement {
                       kind: StatementKind::Instruction("addi".to_string(), vec![Operand::Register(rd_reg), Operand::Register(0), Operand::Immediate(imm)]),
                       line,
                   }])
               } else {
                   let hi20 = (imm + 0x800) >> 12;
                   let lo12 = (imm << 20) >> 20;
                   Ok(vec![
                       Statement {
                           kind: StatementKind::Instruction("lui".to_string(), vec![Operand::Register(rd_reg), Operand::Immediate(hi20)]),
                           line,
                       },
                       Statement {
                           kind: StatementKind::Instruction("addi".to_string(), vec![Operand::Register(rd_reg), Operand::Register(rd_reg), Operand::Immediate(lo12)]),
                           line,
                       }
                   ])
               }

        }
        "mv" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'mv' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

            let rd = &ops[0];
            let rs = &ops[1];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for 'mv' pseudo-instruction. Expected a register, got {}", rd)),
            };
            let rs_reg = match rs {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid second operand for 'mv' pseudo-instruction. Expected a register, got {}", rs)),
            };
            Ok(vec![Statement {
                kind: StatementKind::Instruction("addi".to_string(), vec![Operand::Register(rd_reg), Operand::Register(rs_reg), Operand::Immediate(0)]),
                line,
            }])
        }
        "not" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'not' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

            let rd = &ops[0];
            let rs = &ops[1];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for 'not' pseudo-instruction. Expected a register, got {}", rd)),
            };
            let rs_reg = match rs {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid second operand for 'not' pseudo-instruction. Expected a register, got {}", rs)),
            };
            Ok(vec![Statement {
                kind: StatementKind::Instruction("xori".to_string(), vec![Operand::Register(rd_reg), Operand::Register(rs_reg), Operand::Immediate(-1)]),
                line,
            }])
        }
        "neg" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'neg' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

            let rd = &ops[0];
            let rs = &ops[1];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for 'neg' pseudo-instruction. Expected a register, got {}", rd)),
            };
            let rs_reg = match rs {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid second operand for 'neg' pseudo-instruction. Expected a register, got {}", rs)),
            };
            Ok(vec![Statement {
                kind: StatementKind::Instruction("sub".to_string(), vec![
                    Operand::Register(rd_reg), Operand::Register(0), Operand::Register(rs_reg)
                ]),
                line,
            }])
        }
        "seqz" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'seqz' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

            let rd = &ops[0];
            let rs = &ops[1];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for 'seqz' pseudo-instruction. Expected a register, got {}", rd)),
            };
            let rs_reg = match rs {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid second operand for 'seqz' pseudo-instruction. Expected a register, got {}", rs)),
            };
            Ok(vec![Statement {
                kind: StatementKind::Instruction("sltiu".to_string(), vec![
                    Operand::Register(rd_reg), Operand::Register(rs_reg), Operand::Immediate(1)
                ]),
                line,
            }])
        }
        "snez" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'snez' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

            let rd = &ops[0];
            let rs = &ops[1];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for 'snez' pseudo-instruction. Expected a register, got {}", rd)),
            };
            let rs_reg = match rs {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid second operand for 'snez' pseudo-instruction. Expected a register, got {}", rs)),
            };
            Ok(vec![Statement {
                kind: StatementKind::Instruction("sltu".to_string(), vec![
                    Operand::Register(rd_reg), Operand::Register(0), Operand::Register(rs_reg)
                ]),
                line,
            }])
        }
        "sltz" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'sltz' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

            let rd = &ops[0];
            let rs = &ops[1];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for 'sltz' pseudo-instruction. Expected a register, got {}", rd)),
            };
            let rs_reg = match rs {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid second operand for 'sltz' pseudo-instruction. Expected a register, got {}", rs)),
            };
            Ok(vec![Statement {
                kind: StatementKind::Instruction("slti".to_string(), vec![
                    Operand::Register(rd_reg), Operand::Register(rs_reg), Operand::Immediate(0)
                ]),
                line,
            }])
        }
        "sgtz" => {
            if ops.len() != 2 {
                return Err(format!("Invalid number of operands for 'sgtz' pseudo-instruction. Expected 2, got {}", ops.len()));
            }

            let rd = &ops[0];
            let rs = &ops[1];
            let rd_reg = match rd {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid first operand for 'sgtz' pseudo-instruction. Expected a register, got {}", rd)),
            };
            let rs_reg = match rs {
                Operand::Register(n) => *n,
                _ => return Err(format!("Invalid second operand for 'sgtz' pseudo-instruction. Expected a register, got {}", rs)),
            };
            Ok(vec![Statement {
                kind: StatementKind::Instruction("slt".to_string(), vec![
                    Operand::Register(rd_reg), Operand::Register(0), Operand::Register(rs_reg)
                ]),
                line,
            }])
        }
        "j" => {
            if ops.len() == 1 {
                let offset = ops.into_iter().next().unwrap();
                // TODO validate is a valid offset? if not, the assembler will fail later and reference an instruction jal with an invalid offset
                Ok(vec![Statement {
                    kind: StatementKind::Instruction("jal".to_string(), vec![Operand::Register(0), offset]),
                    line,
                }])
            } else {
                // TODO make the error message more similar to those in the assembler.rs i.e. "Invalid operands for J-type instruction"
                Err(format!("Invalid number of operands for 'j' pseudo-instruction. Expected 1, got {}", ops.len()))
            }
        }
        "jal" => {
            if ops.len() == 1 {
                let offset = ops.into_iter().next().unwrap();
                // TODO validate is a valid offset? if not, the assembler will fail later and reference an instruction jal already expanded
                Ok(vec![Statement {
                    kind: StatementKind::Instruction("jal".to_string(), vec![Operand::Register(1), offset]),
                    line,
                }])
            } else {
                Ok(vec![Statement { kind: StatementKind::Instruction(name, ops), line }])
            }
        }
        "jr" => {
            if ops.len() == 1 {
                let rs = ops.into_iter().next().unwrap();
                let rs_reg = match rs {
                    Operand::Register(n) => n,
                    _ => return Err(format!("Invalid operand for 'jr' pseudo-instruction. Expected a register, got {}", rs)),
                };
                Ok(vec![Statement {
                    kind: StatementKind::Instruction("jalr".to_string(), vec![
                        Operand::Register(0), Operand::Register(rs_reg), Operand::Immediate(0)
                    ]),
                    line,
                }])
            } else {
                Err(format!("Invalid number of operands for 'jr' pseudo-instruction. Expected 1, got {}", ops.len()))
            }
        }
        "jalr" => {
            if ops.len() == 1 {
                let rs = ops.into_iter().next().unwrap();
                let rs_reg = match rs {
                    Operand::Register(n) => n,
                    _ => return Err(format!("Invalid operand for 'jalr' pseudo-instruction. Expected a register, got {}", rs)),
                };
                Ok(vec![Statement {
                    kind: StatementKind::Instruction("jalr".to_string(), vec![
                        Operand::Register(1), Operand::Register(rs_reg), Operand::Immediate(0)
                    ]),
                    line,
                }])
            } else {
                Ok(vec![Statement { kind: StatementKind::Instruction(name, ops), line }])
            }
        }
        "ret" => {
            if ops.is_empty() {
                Ok(vec![Statement {
                    kind: StatementKind::Instruction("jalr".to_string(), vec![
                        Operand::Register(0),
                        Operand::Register(1),
                        Operand::Immediate(0)
                    ]),
                    line,
                }])
            } else {
                Err(format!("Invalid number of operands for 'ret' pseudo-instruction. Expected 0, got {}", ops.len()))
            }
        }
        "call" => {
            if ops.len() == 1 {
                let offset = ops.into_iter().next().unwrap();
                // Validate offset is an Immediate or Label
                let (offset_high, offset_low) = match offset {
                    Operand::Immediate(imm) => (
                        Operand::Immediate(((imm + 0x800) >> 12) as i32),
                        Operand::Immediate((imm << 20) >> 20),
                    ),
                    Operand::Label(label) => (
                        Operand::Modifier(ModifierKind::Hi, label.clone()),
                        Operand::Modifier(ModifierKind::Lo, label)
                    ),
                    _ => return Err(format!("Invalid operand for 'call' pseudo-instruction. Expected an immediate or label, got {}", offset)),
                };
                Ok(vec![Statement {
                    kind: StatementKind::Instruction("auipc".to_string(), vec![
                        Operand::Register(1),
                        offset_high,
                    ]),
                    line,
                },
                    Statement {
                    kind: StatementKind::Instruction("jalr".to_string(), vec![
                        Operand::Register(1),
                        Operand::Register(1),
                        offset_low,
                    ]),
                    line,
                }])
            } else {
                Err(format!("Invalid number of operands for 'call' pseudo-instruction. Expected 1, got {}", ops.len()))
            }
        }
        "tail" => {
            if ops.len() == 1 {
                let offset = ops.into_iter().next().unwrap();
                // Validate offset is an Immediate or Label
                let (offset_high, offset_low) = match offset {
                    Operand::Immediate(imm) => (
                        Operand::Immediate(((imm + 0x800) >> 12) as i32),
                        Operand::Immediate((imm << 20) >> 20),
                    ),
                    Operand::Label(label) => (
                        Operand::Modifier(ModifierKind::Hi, label.clone()),
                        Operand::Modifier(ModifierKind::Lo, label)
                    ),
                    _ => return Err(format!("Invalid operand for 'tail' pseudo-instruction. Expected an immediate or label, got {}", offset)),
                };
                Ok(vec![Statement {
                    kind: StatementKind::Instruction("auipc".to_string(), vec![
                        Operand::Register(6),
                        offset_high,
                    ]),
                    line,
                },
                    Statement {
                    kind: StatementKind::Instruction("jalr".to_string(), vec![
                        Operand::Register(0),
                        Operand::Register(6),
                        offset_low,
                    ]),
                    line,
                }])
            } else {
                Err(format!("Invalid number of operands for 'tail' pseudo-instruction. Expected 1, got {}", ops.len()))
            }
        }
        _ => Ok(vec![Statement { kind: StatementKind::Instruction(name, ops), line }]),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_no_pseudoinstruction() {
        let statement = Statement {
            kind: StatementKind::Instruction("add".to_string(), vec![Operand::Register(1), Operand::Register(2), Operand::Register(3)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("add".to_string(), vec![Operand::Register(1), Operand::Register(2), Operand::Register(3)]));
    }

    #[test]
    fn test_expand_la() {
        let statement = Statement {
            kind: StatementKind::Instruction("la".to_string(), vec![Operand::Register(1), Operand::Label("label".to_string())]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("auipc".to_string(), vec![Operand::Register(1), Operand::Modifier(ModifierKind::Hi, "label".to_string())]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("addi".to_string(), vec![Operand::Register(1), Operand::Register(1), Operand::Modifier(ModifierKind::Lo, "label".to_string())]));
    }

    #[test]
    fn test_expand_la_invalid_parameters() {
        // invalid number of parameters
        let statement = Statement {
            kind: StatementKind::Instruction("la".to_string(), vec![Operand::Immediate(1), Operand::Immediate(2), Operand::Immediate(3)]),
            line: 1,
        };
        let expanded = expand_statement(statement);
        assert!(expanded.is_err());
        assert_eq!(expanded.unwrap_err(), "Invalid number of operands for 'la' pseudo-instruction. Expected 2, got 3");

        // TODO maybe the following error messages should be more specific and say Immediate(1) or Register(..) instead of the display

        // invalid first parameter, expected register
        let statement = Statement {
            kind: StatementKind::Instruction("la".to_string(), vec![Operand::Immediate(1), Operand::Label("label".to_string())]),
            line: 1,
        };
        let expanded = expand_statement(statement);
        assert!(expanded.is_err());
        assert_eq!(expanded.unwrap_err(), "Invalid first operand for 'la' pseudo-instruction. Expected a register, got 1");
        // invalid second parameter, expected label
        let statement = Statement {
            kind: StatementKind::Instruction("la".to_string(), vec![Operand::Register(1), Operand::Register(2)]),
            line: 1,
        };
        let expanded = expand_statement(statement);
        assert!(expanded.is_err());
        assert_eq!(expanded.unwrap_err(), "Invalid second operand for 'la' pseudo-instruction. Expected a label, got x2");
    }

    #[test]
    fn test_expand_lb() {
        let statement = Statement {
            kind: StatementKind::Instruction("lb".to_string(), vec![Operand::Register(3), Operand::Label("label".to_string())]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("auipc".to_string(), vec![Operand::Register(3), Operand::Modifier(ModifierKind::Hi, "label".to_string())]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("lb".to_string(), vec![Operand::Register(3), Operand::Memory { offset: MemoryOffset::Modifier(ModifierKind::Lo, "label".to_string()), reg: 3 }]));
    }

    #[test]
    fn test_expand_sb() {
        let statement = Statement {
            kind: StatementKind::Instruction("sb".to_string(), vec![Operand::Register(3), Operand::Label("label".to_string()), Operand::Register(4)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("auipc".to_string(), vec![Operand::Register(4), Operand::Modifier(ModifierKind::Hi, "label".to_string())]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("sb".to_string(), vec![Operand::Register(3), Operand::Memory { offset: MemoryOffset::Modifier(ModifierKind::Lo, "label".to_string()), reg: 4 }]));
    }

    #[test]
    fn test_expand_sb_if_its_base_instruction() {
        // sb is a base instruction, so it should not be expanded if the operands are correct
        // example: sb x1, 0(x2)
        let statement = Statement {
            kind: StatementKind::Instruction("sb".to_string(), vec![Operand::Register(3), Operand::Memory { offset: MemoryOffset::Immediate(0), reg: 2 }]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("sb".to_string(), vec![Operand::Register(3), Operand::Memory { offset: MemoryOffset::Immediate(0), reg: 2 }]));
    }

    #[test]
    fn test_expand_j() {
        let statement = Statement {
            kind: StatementKind::Instruction("j".to_string(), vec![Operand::Immediate(10)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("jal".to_string(), vec![Operand::Register(0), Operand::Immediate(10)]));
    }

    #[test]
    fn test_expand_jal() {
        let statement = Statement {
            kind: StatementKind::Instruction("jal".to_string(), vec![Operand::Label("loop".to_string())]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
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
        let expanded = expand_statement(statement).unwrap();
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
        let expanded = expand_statement(statement).unwrap();
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
        let expanded = expand_statement(statement).unwrap();
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

    #[test]
    fn test_expand_li_small() {
        let statement = Statement {
            kind: StatementKind::Instruction("li".to_string(), vec![Operand::Register(1), Operand::Immediate(100)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("addi".to_string(), vec![
            Operand::Register(1), Operand::Register(0), Operand::Immediate(100)]));
    }

    #[test]
    fn test_expand_li_large() {
        let statement = Statement {
            kind: StatementKind::Instruction("li".to_string(), vec![Operand::Register(1), Operand::Immediate(0x12345678)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 2);
        // hi20 = (0x12345678 + 0x800) >> 12 = 0x12345
        // lo12 = (0x12345678 << 20) >> 20 = 0x678
        assert_eq!(expanded[0].kind, StatementKind::Instruction("lui".to_string(), vec![Operand::Register(1), Operand::Immediate(0x12345)]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("addi".to_string(), vec![Operand::Register(1), Operand::Register(1), Operand::Immediate(0x678)]));
    }

    #[test]
    fn test_expand_li_negative_small() {
        let statement = Statement {
            kind: StatementKind::Instruction("li".to_string(), vec![Operand::Register(1), Operand::Immediate(-100)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("addi".to_string(), vec![Operand::Register(1), Operand::Register(0), Operand::Immediate(-100)]));
    }

    #[test]
    fn test_expand_li_large_bit11_set() {
        // 0x12345ABC — lo = 0xABC, bit 11 is SET → +0x800 correction triggers
        let statement = Statement {
            kind: StatementKind::Instruction("li".to_string(),
                vec![Operand::Register(1), Operand::Immediate(0x12345ABC_u32 as i32)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 2);
        // hi = (0x12345ABC + 0x800) >> 12 = 0x12346  ← note: 0x12346, not 0x12345
        // lo = sign_extend(0xABC) = -1348
        assert_eq!(
            expanded[0].kind,
            StatementKind::Instruction("lui".to_string(), vec![Operand::Register(1), Operand::Immediate(0x12346)])
        );
        assert_eq!(
            expanded[1].kind,
            StatementKind::Instruction("addi".to_string(), vec![
                Operand::Register(1), Operand::Register(1), Operand::Immediate(-1348)
            ])
        );
    }

    #[test]
    fn test_expand_basic_pseudo_instructions() {
        let test_cases = vec![
            ("nop", vec![], "addi", vec![Operand::Register(0), Operand::Register(0), Operand::Immediate(0)]),
            ("mv", vec![Operand::Register(11), Operand::Register(12)], "addi", vec![Operand::Register(11), Operand::Register(12), Operand::Immediate(0)]),
            ("not", vec![Operand::Register(11), Operand::Register(12)], "xori", vec![Operand::Register(11), Operand::Register(12), Operand::Immediate(-1)]),
            ("neg", vec![Operand::Register(11), Operand::Register(12)], "sub", vec![Operand::Register(11), Operand::Register(0), Operand::Register(12)]),
            ("seqz", vec![Operand::Register(11), Operand::Register(12)], "sltiu", vec![Operand::Register(11), Operand::Register(12), Operand::Immediate(1)]),
            ("snez", vec![Operand::Register(11), Operand::Register(12)], "sltu", vec![Operand::Register(11), Operand::Register(0), Operand::Register(12)]),
            ("sltz", vec![Operand::Register(11), Operand::Register(12)], "slti", vec![Operand::Register(11), Operand::Register(12), Operand::Immediate(0)]),
            ("sgtz", vec![Operand::Register(11), Operand::Register(12)], "slt", vec![Operand::Register(11), Operand::Register(0), Operand::Register(12)]),
        ];

        for (name, ops, expected_name, expected_ops) in test_cases {
            let statement = Statement {
                kind: StatementKind::Instruction(name.to_string(), ops),
                line: 1,
            };
            let expanded = expand_statement(statement).unwrap();
            assert_eq!(expanded.len(), 1, "Failed expansion for {}", name);
            assert_eq!(expanded[0].kind, StatementKind::Instruction(expected_name.to_string(), expected_ops), "Mismatch for {}", name);
        }
    }
}