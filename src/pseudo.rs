use crate::lexer::ModifierKind;
use crate::parser::{MemoryOffset, Operand, Statement, StatementKind};

// Describes how to build one output operand from the input operand list.
#[derive(Clone, Copy)]
enum OutOp {
    In(usize), // take ops[i] unchanged
    Reg(u8),   // inject a fixed register
    Imm(i32),  // inject a fixed immediate
}

enum Expansion {
    // Single base instruction; operands are built from a static OutOp template.
    Fixed {
        base: &'static str,
        arity: usize,
        out: &'static [OutOp],
    },
    // Anything that needs runtime logic (value-dependent, multi-instruction,
    // or conditional pass-through based on operand type).
    Custom(fn(&str, Vec<Operand>, usize) -> Result<Vec<Statement>, String>),
}

use Expansion::{Custom, Fixed};
use OutOp::{Imm, In, Reg};

static PSEUDO_TABLE: &[(&str, Expansion)] = &[
    // Zero-operand aliases
    ("nop",   Fixed { base: "addi",   arity: 0, out: &[Reg(0), Reg(0), Imm(0)]  }),
    ("ret",   Fixed { base: "jalr",   arity: 0, out: &[Reg(0), Reg(1), Imm(0)]  }),
    // Two-register arithmetic / logical aliases
    ("mv",    Fixed { base: "addi",   arity: 2, out: &[In(0), In(1), Imm(0)]    }),
    ("not",   Fixed { base: "xori",   arity: 2, out: &[In(0), In(1), Imm(-1)]   }),
    ("neg",   Fixed { base: "sub",    arity: 2, out: &[In(0), Reg(0), In(1)]    }),
    ("seqz",  Fixed { base: "sltiu",  arity: 2, out: &[In(0), In(1), Imm(1)]    }),
    ("snez",  Fixed { base: "sltu",   arity: 2, out: &[In(0), Reg(0), In(1)]    }),
    ("sltz",  Fixed { base: "slti",   arity: 2, out: &[In(0), In(1), Imm(0)]    }),
    ("sgtz",  Fixed { base: "slt",    arity: 2, out: &[In(0), Reg(0), In(1)]    }),
    // Branch-compare-zero: (rs, target) → base_branch rs, x0, target
    ("beqz",  Fixed { base: "beq",    arity: 2, out: &[In(0), Reg(0), In(1)]    }),
    ("bnez",  Fixed { base: "bne",    arity: 2, out: &[In(0), Reg(0), In(1)]    }),
    ("bgez",  Fixed { base: "bge",    arity: 2, out: &[In(0), Reg(0), In(1)]    }),
    ("bltz",  Fixed { base: "blt",    arity: 2, out: &[In(0), Reg(0), In(1)]    }),
    ("blez",  Fixed { base: "bge",    arity: 2, out: &[Reg(0), In(0), In(1)]    }),
    ("bgtz",  Fixed { base: "blt",    arity: 2, out: &[Reg(0), In(0), In(1)]    }),
    // Branch aliases: operand swap maps the pseudo-condition to a real branch
    ("bgt",   Fixed { base: "blt",    arity: 3, out: &[In(1), In(0), In(2)]     }),
    ("ble",   Fixed { base: "bge",    arity: 3, out: &[In(1), In(0), In(2)]     }),
    ("bgtu",  Fixed { base: "bltu",   arity: 3, out: &[In(1), In(0), In(2)]     }),
    ("bleu",  Fixed { base: "bgeu",   arity: 3, out: &[In(1), In(0), In(2)]     }),
    // Jump shorthands
    ("j",     Fixed { base: "jal",    arity: 1, out: &[Reg(0), In(0)]           }),
    ("jr",    Fixed { base: "jalr",   arity: 1, out: &[Reg(0), In(0), Imm(0)]   }),
    // CSR aliases
    ("csrr",  Fixed { base: "csrrs",  arity: 2, out: &[In(0), In(1), Reg(0)]    }),
    ("csrw",  Fixed { base: "csrrw",  arity: 2, out: &[Reg(0), In(0), In(1)]    }),
    ("csrwi", Fixed { base: "csrrwi", arity: 2, out: &[Reg(0), In(0), In(1)]    }),
    ("csrs",  Fixed { base: "csrrs",  arity: 2, out: &[Reg(0), In(0), In(1)]    }),
    ("csrc",  Fixed { base: "csrrc",  arity: 2, out: &[Reg(0), In(0), In(1)]    }),
    ("csrsi", Fixed { base: "csrrsi", arity: 2, out: &[Reg(0), In(0), In(1)]    }),
    ("csrci", Fixed { base: "csrrci", arity: 2, out: &[Reg(0), In(0), In(1)]    }),
    // Complex expansions that need runtime logic
    ("li",    Custom(expand_li)),
    ("la",    Custom(expand_la)),
    ("call",  Custom(expand_call)),
    ("tail",  Custom(expand_tail)),
    ("lb",    Custom(expand_load_pseudo)),
    ("lh",    Custom(expand_load_pseudo)),
    ("lw",    Custom(expand_load_pseudo)),
    ("sb",    Custom(expand_store_pseudo)),
    ("sh",    Custom(expand_store_pseudo)),
    ("sw",    Custom(expand_store_pseudo)),
    ("jal",   Custom(expand_jal)),
    ("jalr",  Custom(expand_jalr)),
];

pub fn expand(statements: Vec<Statement>) -> Result<Vec<Statement>, String> {
    let mut expanded = Vec::with_capacity(statements.len());
    for stmt in statements {
        expanded.extend(expand_statement(stmt)?);
    }
    Ok(expanded)
}

fn expand_statement(statement: Statement) -> Result<Vec<Statement>, String> {
    let line = statement.line;
    let StatementKind::Instruction(name, ops) = statement.kind else {
        return Ok(vec![statement]);
    };

    for (mnemonic, expansion) in PSEUDO_TABLE {
        if *mnemonic == name.as_str() {
            return match expansion {
                Fixed { base, arity, out } => apply_fixed(&name, base, *arity, out, ops, line),
                Custom(f)                  => f(&name, ops, line),
            };
        }
    }

    Ok(vec![Statement { kind: StatementKind::Instruction(name, ops), line }])
}

fn apply_fixed(
    name: &str,
    base: &'static str,
    arity: usize,
    out_ops: &[OutOp],
    ops: Vec<Operand>,
    line: usize,
) -> Result<Vec<Statement>, String> {
    if ops.len() != arity {
        return Err(format!(
            "'{}' expects {} operand{}, got {}",
            name, arity, if arity == 1 { "" } else { "s" }, ops.len()
        ));
    }
    let built = out_ops.iter().map(|o| match o {
        In(i)  => ops[*i].clone(),
        Reg(n) => Operand::Register(*n),
        Imm(v) => Operand::Immediate(*v),
    }).collect();
    Ok(vec![Statement { kind: StatementKind::Instruction(base.to_string(), built), line }])
}

// --- Custom expanders ---

fn take_ops<const N: usize>(name: &str, ops: Vec<Operand>) -> Result<[Operand; N], String> {
    ops.try_into().map_err(|v: Vec<_>| {
        format!("Invalid number of operands for '{}' pseudo-instruction. Expected {}, got {}", name, N, v.len())
    })
}

fn expand_li(_name: &str, ops: Vec<Operand>, line: usize) -> Result<Vec<Statement>, String> {
    let [rd, imm_op] = take_ops::<2>("li", ops)?;
    let rd_reg = match rd {
        Operand::Register(n) => n,
        _ => return Err(format!("Invalid first operand for 'li': expected a register, got {}", rd)),
    };
    let imm = match imm_op {
        Operand::Immediate(n) => n,
        _ => return Err(format!("Invalid second operand for 'li': expected an immediate, got {}", imm_op)),
    };

    if (-2048..=2047).contains(&imm) {
        Ok(vec![Statement {
            kind: StatementKind::Instruction("addi".to_string(), vec![
                Operand::Register(rd_reg), Operand::Register(0), Operand::Immediate(imm),
            ]),
            line,
        }])
    } else {
        let hi20 = ((imm as i64 + 0x800) >> 12) as i32;
        let lo12 = (imm << 20) >> 20;
        Ok(vec![
            Statement {
                kind: StatementKind::Instruction("lui".to_string(), vec![
                    Operand::Register(rd_reg), Operand::Immediate(hi20),
                ]),
                line,
            },
            Statement {
                kind: StatementKind::Instruction("addi".to_string(), vec![
                    Operand::Register(rd_reg), Operand::Register(rd_reg), Operand::Immediate(lo12),
                ]),
                line,
            },
        ])
    }
}

fn expand_la(_name: &str, ops: Vec<Operand>, line: usize) -> Result<Vec<Statement>, String> {
    let [rd, symbol] = take_ops::<2>("la", ops)?;
    let rd_reg = match rd {
        Operand::Register(n) => n,
        _ => return Err(format!("Invalid first operand for 'la' pseudo-instruction. Expected a register, got {}", rd)),
    };
    let symbol = match symbol {
        Operand::Label(s) => s,
        _ => return Err(format!("Invalid second operand for 'la' pseudo-instruction. Expected a label, got {}", symbol)),
    };
    Ok(vec![
        Statement {
            kind: StatementKind::Instruction("auipc".to_string(), vec![
                Operand::Register(rd_reg), Operand::Modifier(ModifierKind::Hi, symbol.clone()),
            ]),
            line,
        },
        Statement {
            kind: StatementKind::Instruction("addi".to_string(), vec![
                Operand::Register(rd_reg), Operand::Register(rd_reg),
                Operand::Modifier(ModifierKind::Lo, symbol),
            ]),
            line,
        },
    ])
}

fn expand_call(_name: &str, ops: Vec<Operand>, line: usize) -> Result<Vec<Statement>, String> {
    let [target] = take_ops::<1>("call", ops)?;
    let (hi, lo) = split_hi_lo(target, "call")?;
    Ok(vec![
        Statement { kind: StatementKind::Instruction("auipc".to_string(), vec![Operand::Register(1), hi]),                         line },
        Statement { kind: StatementKind::Instruction("jalr".to_string(),  vec![Operand::Register(1), Operand::Register(1), lo]),    line },
    ])
}

fn expand_tail(_name: &str, ops: Vec<Operand>, line: usize) -> Result<Vec<Statement>, String> {
    let [target] = take_ops::<1>("tail", ops)?;
    let (hi, lo) = split_hi_lo(target, "tail")?;
    Ok(vec![
        Statement { kind: StatementKind::Instruction("auipc".to_string(), vec![Operand::Register(6), hi]),                         line },
        Statement { kind: StatementKind::Instruction("jalr".to_string(),  vec![Operand::Register(0), Operand::Register(6), lo]),    line },
    ])
}

// lb/lh/lw rd, symbol  (pseudo)  →  auipc rd, %hi(symbol) + l{b|h|w} rd, %lo(symbol)(rd)
// lb/lh/lw rd, offset(rs)        →  pass through as base instruction
fn expand_load_pseudo(name: &str, ops: Vec<Operand>, line: usize) -> Result<Vec<Statement>, String> {
    if ops.len() != 2 || !matches!(ops[1], Operand::Label(_)) {
        return Ok(vec![Statement { kind: StatementKind::Instruction(name.to_string(), ops), line }]);
    }
    let [rd, symbol] = take_ops::<2>(name, ops)?;
    let rd_reg = match rd {
        Operand::Register(n) => n,
        _ => return Err(format!("Invalid first operand for '{}' pseudo-instruction. Expected a register, got {}", name, rd)),
    };
    let Operand::Label(symbol) = symbol else { unreachable!() };
    Ok(vec![
        Statement {
            kind: StatementKind::Instruction("auipc".to_string(), vec![
                Operand::Register(rd_reg), Operand::Modifier(ModifierKind::Hi, symbol.clone()),
            ]),
            line,
        },
        Statement {
            kind: StatementKind::Instruction(name.to_string(), vec![
                Operand::Register(rd_reg),
                Operand::Memory { offset: MemoryOffset::Modifier(ModifierKind::Lo, symbol), reg: rd_reg },
            ]),
            line,
        },
    ])
}

// sb/sh/sw rd, symbol, rt  (pseudo)  →  auipc rt, %hi(symbol) + s{b|h|w} rd, %lo(symbol)(rt)
// sb/sh/sw rd, offset(rs)            →  pass through as base instruction
fn expand_store_pseudo(name: &str, ops: Vec<Operand>, line: usize) -> Result<Vec<Statement>, String> {
    if ops.len() != 3 || !matches!(ops[1], Operand::Label(_)) {
        return Ok(vec![Statement { kind: StatementKind::Instruction(name.to_string(), ops), line }]);
    }
    let [rd, symbol, rt] = take_ops::<3>(name, ops)?;
    let rd_reg = match rd {
        Operand::Register(n) => n,
        _ => return Err(format!("Invalid first operand for '{}' pseudo-instruction. Expected a register, got {}", name, rd)),
    };
    let Operand::Label(symbol) = symbol else { unreachable!() };
    let rt_reg = match rt {
        Operand::Register(n) => n,
        _ => return Err(format!("Invalid third operand for '{}' pseudo-instruction. Expected a register, got {}", name, rt)),
    };
    Ok(vec![
        Statement {
            kind: StatementKind::Instruction("auipc".to_string(), vec![
                Operand::Register(rt_reg), Operand::Modifier(ModifierKind::Hi, symbol.clone()),
            ]),
            line,
        },
        Statement {
            kind: StatementKind::Instruction(name.to_string(), vec![
                Operand::Register(rd_reg),
                Operand::Memory { offset: MemoryOffset::Modifier(ModifierKind::Lo, symbol), reg: rt_reg },
            ]),
            line,
        },
    ])
}

// jal label        (1-op pseudo)  →  jal ra, label
// jal rd, label    (base)         →  pass through
fn expand_jal(name: &str, ops: Vec<Operand>, line: usize) -> Result<Vec<Statement>, String> {
    if ops.len() == 1 {
        let op = ops.into_iter().next().unwrap();
        Ok(vec![Statement {
            kind: StatementKind::Instruction("jal".to_string(), vec![Operand::Register(1), op]),
            line,
        }])
    } else {
        Ok(vec![Statement { kind: StatementKind::Instruction(name.to_string(), ops), line }])
    }
}

// jalr rs          (1-op pseudo)  →  jalr ra, rs, 0
// jalr rd, rs, imm (base)         →  pass through
fn expand_jalr(name: &str, ops: Vec<Operand>, line: usize) -> Result<Vec<Statement>, String> {
    if ops.len() == 1 {
        let rs = ops.into_iter().next().unwrap();
        let rs_reg = match rs {
            Operand::Register(n) => n,
            other => return Err(format!("'jalr' expects a register, got {}", other)),
        };
        Ok(vec![Statement {
            kind: StatementKind::Instruction("jalr".to_string(), vec![
                Operand::Register(1), Operand::Register(rs_reg), Operand::Immediate(0),
            ]),
            line,
        }])
    } else {
        Ok(vec![Statement { kind: StatementKind::Instruction(name.to_string(), ops), line }])
    }
}

fn split_hi_lo(offset: Operand, pseudo_name: &str) -> Result<(Operand, Operand), String> {
    match offset {
        Operand::Immediate(imm) => Ok((
            Operand::Immediate(((imm as i64 + 0x800) >> 12) as i32),
            Operand::Immediate((imm << 20) >> 20),
        )),
        Operand::Label(label) => Ok((
            Operand::Modifier(ModifierKind::Hi, label.clone()),
            Operand::Modifier(ModifierKind::Lo, label),
        )),
        _ => Err(format!("Invalid operand for '{}': expected an immediate or label, got {}", pseudo_name, offset)),
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
    fn test_expand_la_invalid_operand_count() {
        let statement = Statement {
            kind: StatementKind::Instruction("la".to_string(), vec![Operand::Immediate(1), Operand::Immediate(2), Operand::Immediate(3)]),
            line: 1,
        };
        let result = expand_statement(statement);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid number of operands for 'la' pseudo-instruction. Expected 2, got 3");
    }

    #[test]
    fn test_expand_la_invalid_first_operand() {
        let statement = Statement {
            kind: StatementKind::Instruction("la".to_string(), vec![Operand::Immediate(1), Operand::Label("label".to_string())]),
            line: 1,
        };
        let result = expand_statement(statement);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid first operand for 'la' pseudo-instruction. Expected a register, got 1");
    }

    #[test]
    fn test_expand_la_invalid_second_operand() {
        let statement = Statement {
            kind: StatementKind::Instruction("la".to_string(), vec![Operand::Register(1), Operand::Register(2)]),
            line: 1,
        };
        let result = expand_statement(statement);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid second operand for 'la' pseudo-instruction. Expected a label, got x2");
    }

    #[test]
    fn test_expand_lb_base_instruction() {
        // lb a0, 4(sp) — base instruction, should pass through unchanged
        let statement = Statement {
            kind: StatementKind::Instruction("lb".to_string(), vec![
                Operand::Register(10),
                Operand::Memory { offset: MemoryOffset::Immediate(4), reg: 2 }
            ]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction(
            "lb".to_string(),
            vec![
                Operand::Register(10),
                Operand::Memory { offset: MemoryOffset::Immediate(4), reg: 2 }
            ]
        ));
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
    fn test_expand_lb_invalid_first_operand() {
        // second operand is a label (pseudo form) but first is not a register
        let statement = Statement {
            kind: StatementKind::Instruction("lb".to_string(), vec![
                Operand::Immediate(1),
                Operand::Label("label".to_string())
            ]),
            line: 1,
        };
        assert!(expand_statement(statement).is_err());
    }

    #[test]
    fn test_expand_sb() {
        let statement = Statement {
            kind: StatementKind::Instruction("sb".to_string(), vec![Operand::Register(3), Operand::Label("label".to_string()), Operand::Register(4)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("auipc".to_string(), vec![
            Operand::Register(4), Operand::Modifier(ModifierKind::Hi, "label".to_string())]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("sb".to_string(), vec![
            Operand::Register(3), Operand::Memory { offset: MemoryOffset::Modifier(ModifierKind::Lo, "label".to_string()), reg: 4 }]));
    }

    #[test]
    fn test_expand_sb_base_instruction() {
        // sb x1, 0(x2) - base instruction, should pass through unchanged
        let statement = Statement {
            kind: StatementKind::Instruction("sb".to_string(), vec![
                Operand::Register(3), Operand::Memory { offset: MemoryOffset::Immediate(0), reg: 2 }]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("sb".to_string(), vec![
            Operand::Register(3), Operand::Memory { offset: MemoryOffset::Immediate(0), reg: 2 }]));
    }

    #[test]
    fn test_expand_li_boundary_small_positive() {
        // 2047 is the last value fitting in 12-bit signed => single addi
        let expanded = expand_statement(Statement {
            kind: StatementKind::Instruction("li".to_string(), vec![
                Operand::Register(1), Operand::Immediate(2047)
            ]),
            line: 1,
        }).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("addi".to_string(), vec![
            Operand::Register(1), Operand::Register(0), Operand::Immediate(2047)
        ]));
    }

    #[test]
    fn test_expand_li_boundary_small_negative() {
        // -2048 is the last negative value fitting in 12-bit signed => single addi
        let expanded = expand_statement(Statement {
            kind: StatementKind::Instruction("li".to_string(), vec![
                Operand::Register(1), Operand::Immediate(-2048)
            ]),
            line: 1,
        }).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("addi".to_string(), vec![
            Operand::Register(1), Operand::Register(0), Operand::Immediate(-2048)
        ]));
    }

    #[test]
    fn test_expand_li_boundary_first_large_positive() {
        // 2048 = 0x800 is first value outside 12-bit range; bit 11 is SET => two instructions
        // hi = (0x800 + 0x800) >> 12 = 1,  lo = -2048
        // sanity: (1 << 12) + (-2048) = 0x1000 - 0x800 = 0x800 = 2048
        let expanded = expand_statement(Statement {
            kind: StatementKind::Instruction("li".to_string(), vec![
                Operand::Register(1), Operand::Immediate(2048)
            ]),
            line: 1,
        }).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("lui".to_string(), vec![
            Operand::Register(1), Operand::Immediate(1)
        ]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("addi".to_string(), vec![
            Operand::Register(1), Operand::Register(1), Operand::Immediate(-2048)
        ]));
    }

    #[test]
    fn test_expand_li_boundary_first_large_negative() {
        // -2049 is first negative value outside 12-bit range => two instructions
        // lo = 0x7FF = 2047 (bit 11 clear, positive), hi = -1
        // sanity: (-1 << 12) + 2047 = -4096 + 2047 = -2049
        let expanded = expand_statement(Statement {
            kind: StatementKind::Instruction("li".to_string(), vec![
                Operand::Register(1), Operand::Immediate(-2049)
            ]),
            line: 1,
        }).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].kind, StatementKind::Instruction("lui".to_string(), vec![
            Operand::Register(1), Operand::Immediate(-1)
        ]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("addi".to_string(), vec![
            Operand::Register(1), Operand::Register(1), Operand::Immediate(2047)
        ]));
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
    fn test_expand_li_max_i32() {
        let statement = Statement {
            kind: StatementKind::Instruction("li".to_string(), vec![Operand::Register(1), Operand::Immediate(0x7FFFFFFF)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        assert_eq!(expanded.len(), 2);
        // hi20 = (0x7FFFFFFF + 0x800) >> 12 = 0x80000 (wrapping)
        // lo12 = (0x7FFFFFFF << 20) >> 20 = -1
        assert_eq!(expanded[0].kind, StatementKind::Instruction("lui".to_string(), vec![
            Operand::Register(1), Operand::Immediate(0x80000u32 as i32)]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("addi".to_string(), vec![
            Operand::Register(1), Operand::Register(1), Operand::Immediate(-1)]));
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
    fn test_expand_call_immediate_bit11_set() {
        // validates the +0x800 correction in call/tail immediate path
        let statement = Statement {
            kind: StatementKind::Instruction("call".to_string(),
                vec![Operand::Immediate(0x12800)]),
            line: 1,
        };
        let expanded = expand_statement(statement).unwrap();
        // hi = (0x12800 + 0x800) >> 12 = 0x13
        // lo = -2048
        assert_eq!(expanded[0].kind, StatementKind::Instruction("auipc".to_string(),
            vec![Operand::Register(1), Operand::Immediate(0x13)]));
        assert_eq!(expanded[1].kind, StatementKind::Instruction("jalr".to_string(),
            vec![Operand::Register(1), Operand::Register(1), Operand::Immediate(-2048)]));
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
            ("beqz", vec![Operand::Register(11), Operand::Label("label".to_string())], "beq", vec![Operand::Register(11), Operand::Register(0), Operand::Label("label".to_string())]),
            ("bnez", vec![Operand::Register(11), Operand::Label("label".to_string())], "bne", vec![Operand::Register(11), Operand::Register(0), Operand::Label("label".to_string())]),
            ("blez", vec![Operand::Register(11), Operand::Label("label".to_string())], "bge", vec![Operand::Register(0), Operand::Register(11), Operand::Label("label".to_string())]),
            ("bgez", vec![Operand::Register(11), Operand::Label("label".to_string())], "bge", vec![Operand::Register(11), Operand::Register(0), Operand::Label("label".to_string())]),
            ("bltz", vec![Operand::Register(11), Operand::Label("label".to_string())], "blt", vec![Operand::Register(11), Operand::Register(0), Operand::Label("label".to_string())]),
            ("bgtz", vec![Operand::Register(11), Operand::Label("label".to_string())], "blt", vec![Operand::Register(0), Operand::Register(11), Operand::Label("label".to_string())]),
            ("bgt", vec![Operand::Register(11), Operand::Register(12), Operand::Label("label".to_string())], "blt", vec![Operand::Register(12), Operand::Register(11), Operand::Label("label".to_string())]),
            ("ble", vec![Operand::Register(11), Operand::Register(12), Operand::Label("label".to_string())], "bge", vec![Operand::Register(12), Operand::Register(11), Operand::Label("label".to_string())]),
            ("bgtu", vec![Operand::Register(11), Operand::Register(12), Operand::Label("label".to_string())], "bltu", vec![Operand::Register(12), Operand::Register(11), Operand::Label("label".to_string())]),
            ("bleu", vec![Operand::Register(11), Operand::Register(12), Operand::Label("label".to_string())], "bgeu", vec![Operand::Register(12), Operand::Register(11), Operand::Label("label".to_string())]),
            ("j", vec![Operand::Immediate(10)], "jal", vec![Operand::Register(0), Operand::Immediate(10)]),
            ("j", vec![Operand::Label("label".to_string())], "jal", vec![Operand::Register(0), Operand::Label("label".to_string())]),
            ("jal", vec![Operand::Label("label".to_string())], "jal", vec![Operand::Register(1), Operand::Label("label".to_string())]),
            ("jr", vec![Operand::Register(1)], "jalr", vec![Operand::Register(0), Operand::Register(1), Operand::Immediate(0)]),
            ("jalr", vec![Operand::Register(11)], "jalr", vec![Operand::Register(1), Operand::Register(11), Operand::Immediate(0)]),
            ("ret", vec![], "jalr", vec![Operand::Register(0), Operand::Register(1), Operand::Immediate(0)]),
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
