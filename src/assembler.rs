use std::collections::HashMap;

use crate::parser::{Statement, Operand};
use crate::symbols::SymbolTable;

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

    pub fn assemble(&mut self, statements: &[Statement], sym_table: &SymbolTable) {
        let mut current_pc = 0x0040_0000; // TODO duplicated in symbols.rs
        let mut data_pc = 0x1001_0000;
        let mut current_section = ".text";

        for stmt in statements {
            let addr = if current_section == ".text" { current_pc } else { data_pc };

            self.debug_info.address_to_source.insert(addr, SourceMapping {
                line: 0, // TODO statement needs line, now only in token
                raw_text: " ".to_string(), // TODO statement should implement Display stmt.to_string(),
                section: current_section.to_string(),
            });

            match stmt {
                Statement::Instruction(name, ops) => {
                    let bytes = encode_instruction(name, ops, sym_table, current_pc);
                    self.text_bin.extend_from_slice(&bytes.to_le_bytes());
                    current_pc += 4;
                }
                Statement::Directive(name, ops) => {
                    if name == ".text" || name == ".data" {
                        current_section = name.as_str();
                        continue; // No bytes to emit for section directives
                    }
                    let bytes = emit_data_bytes(name, ops);
                    self.data_bin.extend_from_slice(&bytes);
                    data_pc += bytes.len() as u32;
                }
                _ => {}
            }
        }
    }


}


fn encode_instruction(name: &str, ops: &[Operand], sym_table: &SymbolTable, current_pc: u32) -> u32 {
    match name {
        "add" => encode_r_type(0x33, 0x0, 0x00, ops),
        "sub" => encode_r_type(0x33, 0x0, 0x20, ops),
        "lw" => encode_i_type(0x03, 0x2, ops, sym_table, current_pc),
        "sw" => encode_s_type(0x23, 0x2, ops, sym_table, current_pc),
        "beq" => encode_b_type(0x63, 0x0, ops, sym_table, current_pc),
        "jal" => encode_j_type(0x6F, ops, sym_table, current_pc),
        _ => panic!("Unsupported instruction '{}'", name),
    }
}

fn encode_r_type(opcode: u8, funct3: u8, funct7: u8, ops: &[Operand]) -> u32 {
    if let [Operand::Register(rd), Operand::Register(rs1), Operand::Register(rs2)] = ops {
        ((funct7 as u32) << 25) | ((*rs2 as u32) << 20) | ((*rs1 as u32) << 15) | ((funct3 as u32) << 12) | ((*rd as u32) << 7) | (opcode as u32)
    } else {
        panic!("Invalid operands for R-type instruction");
    }
}

fn encode_i_type(opcode: u8, funct3: u8, ops: &[Operand], sym_table: &SymbolTable, current_pc: u32) -> u32 {
    if let [Operand::Register(rd), Operand::Register(rs1), Operand::Immediate(imm)] = ops {
        let imm_val = resolve_immediate(*imm, sym_table, current_pc);
        ((imm_val as u32) << 20) | ((*rs1 as u32) << 15) | ((funct3 as u32) << 12) | ((*rd as u32) << 7) | (opcode as u32)
    } else {
        panic!("Invalid operands for I-type instruction");
    }
}

fn encode_s_type(opcode: u8, funct3: u8, ops: &[Operand], sym_table: &SymbolTable, current_pc: u32) -> u32 {
    if let [Operand::Register(rs2), Operand::Memory { offset, reg }] = ops {
        let imm_val = resolve_immediate(*offset, sym_table, current_pc);
        ((imm_val as u32 & 0xFE0) << 20) | ((*reg as u32) << 15) | ((funct3 as u32) << 12) | ((imm_val as u32 & 0x1F) << 7) | (opcode as u32)
    } else {
        panic!("Invalid operands for S-type instruction");
    }
}

fn encode_b_type(opcode: u8, funct3: u8, ops: &[Operand], sym_table: &SymbolTable, current_pc: u32) -> u32 {
    if let [Operand::Register(rs1), Operand::Register(rs2), Operand::Immediate(imm)] = ops {
        let imm_val = resolve_immediate(*imm, sym_table, current_pc);
        let imm_12 = (imm_val >> 12) & 0x1;
        let imm_10_5 = (imm_val >> 5) & 0x3F;
        let imm_4_1 = (imm_val >> 1) & 0xF;
        let imm_11 = (imm_val >> 11) & 0x1;
        ((imm_12 as u32) << 31) | ((imm_10_5 as u32) << 25) | ((*rs2 as u32) << 20) | ((*rs1 as u32) << 15) | ((funct3 as u32) << 12) | ((imm_4_1 as u32) << 8) | ((imm_11 as u32) << 7) | (opcode as u32)
    } else {
        panic!("Invalid operands for B-type instruction");
    }
}

fn encode_j_type(opcode: u8, ops: &[Operand], sym_table: &SymbolTable, current_pc: u32) -> u32 {
    if let [Operand::Register(rd), Operand::Immediate(imm)] = ops {
        let imm_val = resolve_immediate(*imm, sym_table, current_pc);
        let imm_20 = (imm_val >> 20) & 0x1;
        let imm_10_1 = (imm_val >> 1) & 0x3FF;
        let imm_11 = (imm_val >> 11) & 0x1;
        let imm_19_12 = (imm_val >> 12) & 0xFF;
        ((imm_20 as u32) << 31) | ((imm_19_12 as u32) << 12) | ((imm_11 as u32) << 20) | ((imm_10_1 as u32) << 21) | ((*rd as u32) << 7) | (opcode as u32)
    } else {
        panic!("Invalid operands for J-type instruction");
    }
}

fn resolve_immediate(imm: i32, sym_table: &SymbolTable, current_pc: u32) -> i32 {
    // TODO if imm is a label, look up in sym_table and calculate offset
    imm
}

fn emit_data_bytes(name: &str, ops: &[Operand]) -> Vec<u8> {
    match name {
        ".word" => {
            if let Some(Operand::Immediate(val)) = ops.get(0) {
                val.to_le_bytes().to_vec()
            } else {
                panic!("Invalid operand for .word directive");
            }
        }
        _ => panic!("Unsupported directive '{}'", name),
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
            Statement::Instruction("add".to_string(), vec![
                Operand::Register(1),
                Operand::Register(2),
                Operand::Register(3),
            ]),
            Statement::Directive(".data".to_string(), vec![]), // Switch to .data section,
            Statement::Directive(".word".to_string(), vec![
                Operand::Immediate(42),
            ]),
        ];
        assembler.assemble(&statements, &sym_table);
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
}