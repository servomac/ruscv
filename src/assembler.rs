use std::collections::HashMap;

use crate::parser::{Statement, StatementKind, Operand, MemoryOffset};
use crate::symbols::SymbolTable;

#[derive(Debug, Clone, PartialEq)]
pub struct AssemblerError {
    pub line: usize,
    pub message: String,
}

impl AssemblerError {
    fn new(line: usize, message: String) -> Self {
        Self { line, message }
    }
}

pub struct DebugInfo {
    pub address_to_source: HashMap<u32, SourceMapping>,
}

pub struct SourceMapping {
    pub raw_text: String,
    pub line: usize,
    pub section: String,
}

pub struct Assembler {
    pub text_bin: Vec<u8>,
    pub data_bin: Vec<u8>,
    pub debug_info: DebugInfo,
}

impl Assembler {
    pub fn new() -> Self {
        Self {
            text_bin: Vec::new(),
            data_bin: Vec::new(),
            debug_info: DebugInfo { address_to_source: HashMap::new() },
        }
    }

    pub fn assemble(&mut self, statements: &[Statement], sym_table: &SymbolTable) -> Result<(), Vec<AssemblerError>> {
        let mut current_pc = 0x0040_0000; // TODO duplicated in symbols.rs
        let mut data_pc = 0x1001_0000;
        let mut current_section = ".text";
        let mut errors = Vec::new();

        for stmt in statements {
            let addr = if current_section == ".text" { current_pc } else { data_pc };

            self.debug_info.address_to_source.insert(addr, SourceMapping {
                line: stmt.line,
                raw_text: stmt.to_string(),
                section: current_section.to_string(),
            });

            match &stmt.kind {
                StatementKind::Instruction(name, ops) => {
                    match encode_instruction(name, ops, sym_table, current_pc) {
                        Ok(bytes) => {
                            self.text_bin.extend_from_slice(&bytes.to_le_bytes());
                            current_pc += 4;
                        }
                        Err(msg) => {
                            errors.push(AssemblerError::new(stmt.line, msg));
                        }
                    }
                }
                StatementKind::Directive(name, ops) => {
                    if name == ".text" || name == ".data" {
                        current_section = name.as_str();
                        continue; // No bytes to emit for section directives
                    }
                    match emit_data_bytes(name, ops) {
                        Ok(bytes) => {
                            self.data_bin.extend_from_slice(&bytes);
                            data_pc += bytes.len() as u32;
                        }
                        Err(msg) => {
                            errors.push(AssemblerError::new(stmt.line, msg));
                        }
                    }
                }
                _ => {}
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(())
    }


}

fn encode_instruction(name: &str, ops: &[Operand], sym_table: &SymbolTable, current_pc: u32) -> Result<u32, String> {
    match name {
        // R-type | Opcode: 0x33 | Format: funct7, rs2, rs1, funct3, rd, opcode
        "add"   => encode_r_type(0x33, 0x0, 0x00, ops),
        "sub"   => encode_r_type(0x33, 0x0, 0x20, ops),
        "sll"   => encode_r_type(0x33, 0x1, 0x00, ops),
        "slt"   => encode_r_type(0x33, 0x2, 0x00, ops),
        "sltu"  => encode_r_type(0x33, 0x3, 0x00, ops),
        "xor"   => encode_r_type(0x33, 0x4, 0x00, ops),
        "srl"   => encode_r_type(0x33, 0x5, 0x00, ops),
        "sra"   => encode_r_type(0x33, 0x5, 0x20, ops),
        "or"    => encode_r_type(0x33, 0x6, 0x00, ops),
        "and"   => encode_r_type(0x33, 0x7, 0x00, ops),

        // I-type | Opcode: 0x13 for ALU, 0x03 for Loads, 0x67 for JALR
        "addi"  => encode_i_type(0x13, 0x0, ops, sym_table, current_pc),
        "slti"  => encode_i_type(0x13, 0x2, ops, sym_table, current_pc),
        "sltiu" => encode_i_type(0x13, 0x3, ops, sym_table, current_pc),
        "xori"  => encode_i_type(0x13, 0x4, ops, sym_table, current_pc),
        "ori"   => encode_i_type(0x13, 0x6, ops, sym_table, current_pc),
        "andi"  => encode_i_type(0x13, 0x7, ops, sym_table, current_pc),
        "slli"  => encode_i_shift(0x13, 0x1, 0x00, ops), // Special: uses shift amount
        "srli"  => encode_i_shift(0x13, 0x5, 0x00, ops),
        "srai"  => encode_i_shift(0x13, 0x5, 0x20, ops),

        "lb"    => encode_i_type(0x03, 0x0, ops, sym_table, current_pc),
        "lh"    => encode_i_type(0x03, 0x1, ops, sym_table, current_pc),
        "lw"    => encode_i_type(0x03, 0x2, ops, sym_table, current_pc),
        "lbu"   => encode_i_type(0x03, 0x4, ops, sym_table, current_pc),
        "lhu"   => encode_i_type(0x03, 0x5, ops, sym_table, current_pc),

        "jalr"  => encode_i_type(0x67, 0x0, ops, sym_table, current_pc),

        // S-type | Opcode: 0x23
        "sb"    => encode_s_type(0x23, 0x0, ops, sym_table, current_pc),
        "sh"    => encode_s_type(0x23, 0x1, ops, sym_table, current_pc),
        "sw"    => encode_s_type(0x23, 0x2, ops, sym_table, current_pc),

        // B-type | Opcode: 0x63
        "beq"   => encode_b_type(0x63, 0x0, ops, sym_table, current_pc),
        "bne"   => encode_b_type(0x63, 0x1, ops, sym_table, current_pc),
        "blt"   => encode_b_type(0x63, 0x4, ops, sym_table, current_pc),
        "bge"   => encode_b_type(0x63, 0x5, ops, sym_table, current_pc),
        "bltu"  => encode_b_type(0x63, 0x6, ops, sym_table, current_pc),
        "bgeu"  => encode_b_type(0x63, 0x7, ops, sym_table, current_pc),

        // TODO U-type | Opcode: 0x37 LUI, 0x17 AUIPC
        //"lui"   => encode_u_type(0x37, ops, sym_table, current_pc),
        //"auipc" => encode_u_type(0x17, ops, sym_table, current_pc),

        // J-type | Opcode: 0x6F
        "jal"   => encode_j_type(0x6F, ops, sym_table, current_pc),

        // System and Miscellaneous
        "ecall"  => Ok(0x00000073),
        "ebreak" => Ok(0x00100073),
        "fence"  => Ok(0x0000000F), // TODO Simplified for this example

        _ => Err(format!("Unsupported instruction '{}'", name)),
    }
}

fn encode_r_type(opcode: u8, funct3: u8, funct7: u8, ops: &[Operand]) -> Result<u32, String> {
    if let [Operand::Register(rd), Operand::Register(rs1), Operand::Register(rs2)] = ops {
        Ok(((funct7 as u32) << 25) | ((*rs2 as u32) << 20) | ((*rs1 as u32) << 15) | ((funct3 as u32) << 12) | ((*rd as u32) << 7) | (opcode as u32))
    } else {
        Err("Invalid operands for R-type instruction: expected 3 registers (rd, rs1, rs2)".to_string())
    }
}

fn encode_i_type(opcode: u8, funct3: u8, ops: &[Operand], _sym_table: &SymbolTable, _current_pc: u32) -> Result<u32, String> {
    if let [Operand::Register(rd), Operand::Register(rs1), Operand::Immediate(imm)] = ops {
        let imm_val = *imm; // resolve_immediate(*imm, sym_table, current_pc);
        Ok(((imm_val as u32) << 20) | ((*rs1 as u32) << 15) | ((funct3 as u32) << 12) | ((*rd as u32) << 7) | (opcode as u32))
    } else {
        Err("Invalid operands for I-type instruction: expected register, register, immediate".to_string())
    }
}

fn encode_i_shift(
    opcode: u8,
    funct3: u8,
    funct7: u8,
    ops: &[Operand]
) -> Result<u32, String> {
    if let [Operand::Register(rd), Operand::Register(rs1), Operand::Immediate(shamt)] = ops {
        if *shamt < 0 || *shamt > 31 {
            return Err(format!("Shift amount {} out of range (0-31)", shamt));
        }

        let instruction = ((funct7 as u32) << 25) | // Control bits (e.g. 0x20 for srai)
                          ((*shamt as u32) << 20) | // Shift amount
                          ((*rs1 as u32) << 15)   | // Source register
                          ((funct3 as u32) << 12) | // Shift type
                          ((*rd as u32) << 7)     | // Destination register
                          (opcode as u32);          // 0x13

        Ok(instruction)
    } else {
        Err("Invalid operands for shift instruction: expected rd, rs1, shamt".to_string())
    }
}

fn encode_s_type(
    opcode: u8,
    funct3: u8,
    ops: &[Operand],
    sym_table: &SymbolTable,
    _current_pc: u32
) -> Result<u32, String> {
    // Note: The usual order in RISC-V is sw rs2, offset(rs1)
    if let [Operand::Register(rs2), Operand::Memory { offset, reg }] = ops {
        // 1. Resolve the immediate (can be label or number)
        let imm_val = match offset {
            MemoryOffset::Immediate(val) => *val,
            MemoryOffset::Label(name) => {
                sym_table.get_address(name)
                    .ok_or(format!("Unknown label '{}'", name))? as i32
            }
        };

        // 2. Extract immediate bits (12 bits)
        let imm = (imm_val as u32) & 0xFFF;
        let imm_11_5 = (imm >> 5) & 0x7F; // 7 upper bits
        let imm_4_0 = imm & 0x1F;         // 5 lower bits

        // 3. Pack everything into the 32-bit word
        let instruction = (imm_11_5 << 25)      | // imm[11:5]
                          ((*rs2 as u32) << 20) | // rs2
                          ((*reg as u32) << 15) | // rs1 (base register)
                          ((funct3 as u32) << 12) | // funct3
                          (imm_4_0 << 7)        | // imm[4:0]
                          (opcode as u32);        // opcode

        Ok(instruction)
    } else {
        Err("Invalid operands for S-type instruction: expected reg, offset(reg)".to_string())
    }
}

fn encode_b_type(opcode: u8, funct3: u8, ops: &[Operand], _sym_table: &SymbolTable, _current_pc: u32) -> Result<u32, String> {
    if let [Operand::Register(rs1), Operand::Register(rs2), Operand::Immediate(imm)] = ops {
        let imm_val = *imm; // TODO review resolve_immediate(*imm, sym_table, current_pc);
        let imm_12 = (imm_val >> 12) & 0x1;
        let imm_10_5 = (imm_val >> 5) & 0x3F;
        let imm_4_1 = (imm_val >> 1) & 0xF;
        let imm_11 = (imm_val >> 11) & 0x1;
        Ok(((imm_12 as u32) << 31) | ((imm_10_5 as u32) << 25) | ((*rs2 as u32) << 20) | ((*rs1 as u32) << 15) | ((funct3 as u32) << 12) | ((imm_4_1 as u32) << 8) | ((imm_11 as u32) << 7) | (opcode as u32))
    } else {
        Err("Invalid operands for B-type instruction: expected register, register, immediate".to_string())
    }
}

fn encode_j_type(opcode: u8, ops: &[Operand], _sym_table: &SymbolTable, _current_pc: u32) -> Result<u32, String> {
    if let [Operand::Register(rd), Operand::Immediate(imm)] = ops {
        let imm_val = *imm; // TODO review resolve_immediate(*imm, sym_table, current_pc);
        let imm_20 = (imm_val >> 20) & 0x1;
        let imm_10_1 = (imm_val >> 1) & 0x3FF;
        let imm_11 = (imm_val >> 11) & 0x1;
        let imm_19_12 = (imm_val >> 12) & 0xFF;
        Ok(((imm_20 as u32) << 31) | ((imm_19_12 as u32) << 12) | ((imm_11 as u32) << 20) | ((imm_10_1 as u32) << 21) | ((*rd as u32) << 7) | (opcode as u32))
    } else {
        Err("Invalid operands for J-type instruction: expected register, immediate".to_string())
    }
}

fn emit_data_bytes(name: &str, ops: &[Operand]) -> Result<Vec<u8>, String> {
    match name {
        ".word" => {
            if let Some(Operand::Immediate(val)) = ops.get(0) {
                Ok(val.to_le_bytes().to_vec())
            } else {
                Err("Invalid operand for .word directive: expected immediate value".to_string())
            }
        }
        _ => Err(format!("Unsupported directive '{}'", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemble_simple_program() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("add".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(2),
                    Operand::Register(3),
                ]),
                line: 1,
            },
            Statement {
                kind: StatementKind::Directive(".data".to_string(), vec![]),
                line: 2,
            },
            Statement {
                kind: StatementKind::Directive(".word".to_string(), vec![
                    Operand::Immediate(42),
                ]),
                line: 3,
            },
        ];
        assembler.assemble(&statements, &sym_table).expect("Assembly should succeed");
        assert_eq!(assembler.text_bin.len(), 4);
        assert_eq!(assembler.data_bin.len(), 4);

        assert_eq!(
            assembler.text_bin,
            vec![
                0b10110011, // Byte 0: rd[0] + opcode
                0b00000000, // Byte 1: rs1[0] + funct3 + rd[4:1]
                0b00110001, // Byte 2: rs2[4:1] + rs1[4:1]
                0b00000000, // Byte 3: funct7 + rs2[0]
            ]
        );
        assert_eq!(assembler.data_bin, vec![0x2A, 0x00, 0x00, 0x00]); // .word 42
    }

    #[test]
    fn test_unsupported_instruction() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("mul".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(2),
                    Operand::Register(3),
                ]),
                line: 5,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 5);
        assert!(errors[0].message.contains("Unsupported instruction 'mul'"));
    }

    #[test]
    fn test_invalid_r_type_operands() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("add".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(2),
                    Operand::Immediate(5), // Should be a register
                ]),
                line: 10,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 10);
        assert!(errors[0].message.contains("Invalid operands for R-type"));
    }

    #[test]
    fn test_invalid_i_type_operands() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("lw".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(2),
                    Operand::Register(3), // Should be immediate
                ]),
                line: 15,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 15);
        assert!(errors[0].message.contains("Invalid operands for I-type"));
    }

    #[test]
    fn test_invalid_s_type_operands() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("sw".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(2), // Should be memory operand
                ]),
                line: 20,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 20);
        assert!(errors[0].message.contains("Invalid operands for S-type"));
    }

    #[test]
    fn test_invalid_b_type_operands() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("beq".to_string(), vec![
                    Operand::Register(1),
                    Operand::Immediate(5), // Should be register
                    Operand::Immediate(100),
                ]),
                line: 25,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 25);
        assert!(errors[0].message.contains("Invalid operands for B-type"));
    }

    #[test]
    fn test_invalid_j_type_operands() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("jal".to_string(), vec![
                    Operand::Immediate(100), // Missing destination register
                ]),
                line: 30,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 30);
        assert!(errors[0].message.contains("Invalid operands for J-type"));
    }

    #[test]
    fn test_unsupported_directive() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Directive(".float".to_string(), vec![
                    Operand::Immediate(42),
                ]),
                line: 35,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 35);
        assert!(errors[0].message.contains("Unsupported directive '.float'"));
    }

    #[test]
    fn test_invalid_directive_operands() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Directive(".word".to_string(), vec![
                    Operand::Register(1), // Should be immediate
                ]),
                line: 40,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 40);
        assert!(errors[0].message.contains("Invalid operand for .word directive"));
    }

    #[test]
    fn test_multiple_errors() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("mul".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(2),
                    Operand::Register(3),
                ]),
                line: 1,
            },
            Statement {
                kind: StatementKind::Instruction("add".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(2),
                    Operand::Register(3),
                ]),
                line: 2,
            },
            Statement {
                kind: StatementKind::Instruction("div".to_string(), vec![
                    Operand::Register(4),
                    Operand::Register(5),
                    Operand::Register(6),
                ]),
                line: 3,
            },
            Statement {
                kind: StatementKind::Directive(".float".to_string(), vec![
                    Operand::Immediate(42),
                ]),
                line: 4,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        // Should collect all 3 errors (mul, div, .float), but not the valid add
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].line, 1);
        assert!(errors[0].message.contains("Unsupported instruction 'mul'"));
        assert_eq!(errors[1].line, 3);
        assert!(errors[1].message.contains("Unsupported instruction 'div'"));
        assert_eq!(errors[2].line, 4);
        assert!(errors[2].message.contains("Unsupported directive '.float'"));

        // Verify that the valid instruction was assembled
        assert_eq!(assembler.text_bin.len(), 4);
    }

    #[test]
    fn test_assemble_i_type_instruction() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("addi".to_string(), vec![
                    Operand::Register(19),
                    Operand::Register(20),
                    Operand::Immediate(8),
                ]),
                line: 1,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_ok());
        let instructions = assembler.text_bin;
        assert_eq!(instructions.len(), 4);
        assert_eq!(
            instructions[0..4],
            vec![
                // Byte 0: rd[0] (1) + opcode (0010011)
                0b10010011,
                // Byte 1: rs1[0] (0) + funct3 (000) + rd[4:1] (1001)
                0b00001001,
                // Byte 2: imm[3:0] (1000) + rs1[4:1] (1010)
                0b10001010,
                // Byte 3: imm[11:4] (00000000)
                0b00000000,
        ]);
    }

    #[test]
    fn test_assemble_i_type_instruction_with_negative_immediate() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("addi".to_string(), vec![
                    Operand::Register(19),
                    Operand::Register(20),
                    Operand::Immediate(-8),
                ]),
                line: 1,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_ok());
        let instructions = assembler.text_bin;
        assert_eq!(instructions.len(), 4);
        assert_eq!(
            instructions[0..4],
            vec![
                // Byte 0: rd[0] (1) + opcode (0010011)
                0b10010011,
                // Byte 1: rs1[0] (0) + funct3 (000) + rd[4:1] (1001)
                0b00001001,
                // Byte 2: imm[3:0] (1000) + rs1[4:1] (1010)
                0b10001010,
                // Byte 3: imm[11:4] (00000000)
                0b11111111,
        ]);
    }

    #[test]
    fn test_s_instruction_with_unknown_label() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("sw".to_string(), vec![
                    Operand::Register(19),
                    Operand::Memory {
                        offset: MemoryOffset::Label("unknown".to_string()),
                        reg: 0
                    },
                ]),
                line: 1,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 1);
        assert!(errors[0].message.contains("Unknown label 'unknown'"));
    }

    #[test]
    fn test_encoding_of_i_shift_instruction() {
        let mut assembler = Assembler::new();
        let sym_table = SymbolTable::new();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("srai".to_string(), vec![
                    Operand::Register(10),
                    Operand::Register(11),
                    Operand::Immediate(4),
                ]),
                line: 1,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_ok());
        let instructions = assembler.text_bin;
        assert_eq!(instructions.len(), 4);
        // srai x10, x11, 4
        // opcode=0x13, rd=10, funct3=0x5, rs1=11, shamt=4, funct7=0x20
        assert_eq!(
            instructions,
            vec![
                0b00010011, // 19  (rd[0]=0 + opcode=0x13)
                0b11010101, // 213 (rs1[0]=1 + funct3=101 + rd[4:1]=0101)
                0b01000101, // 69  (shamt[3:0]=0100 + rs1[4:1]=0101)
                0b01000000, // 64  (funct7=0100000 + shamt[4]=0)
            ]
        );
    }
}