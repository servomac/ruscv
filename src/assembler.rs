use std::collections::HashMap;

use crate::parser::{Statement, StatementKind, Operand, MemoryOffset, Section, DirectiveKind};
use crate::lexer::ModifierKind;
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
    pub section: Section,
}

pub struct Assembler {
    pub text_bin: Vec<u8>,
    pub data_bin: Vec<u8>,
    pub debug_info: DebugInfo,
    pub text_base: u32,
    pub data_base: u32,
}

impl Assembler {
    pub fn new(text_base: u32, data_base: u32) -> Self {
        Self {
            text_bin: Vec::new(),
            data_bin: Vec::new(),
            debug_info: DebugInfo { address_to_source: HashMap::new() },
            text_base,
            data_base,
        }
    }

    pub fn assemble(&mut self, statements: &[Statement], sym_table: &SymbolTable) -> Result<(), Vec<AssemblerError>> {
        let mut current_pc = self.text_base;
        let mut data_pc = self.data_base;
        let mut current_section = Section::Text;
        let mut errors = Vec::new();

        for stmt in statements {
            let addr = if current_section == Section::Text { current_pc } else { data_pc };

            self.debug_info.address_to_source.insert(addr, SourceMapping {
                line: stmt.line,
                raw_text: stmt.to_string(),
                section: current_section,
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
                StatementKind::Directive(kind, ops) => {
                    match kind {
                        DirectiveKind::Text => { current_section = Section::Text; continue; }
                        DirectiveKind::Data => { current_section = Section::Data; continue; }
                        DirectiveKind::Align => {
                            if let Some(Operand::Immediate(pow)) = ops.get(0) {
                                let alignment = 2u32.pow(*pow as u32);
                                let padding = (alignment - (addr % alignment)) % alignment;
                                let padding_bytes = vec![0u8; padding as usize];
                                if current_section == Section::Text {
                                    self.text_bin.extend_from_slice(&padding_bytes);
                                    current_pc += padding;
                                } else {
                                    self.data_bin.extend_from_slice(&padding_bytes);
                                    data_pc += padding;
                                }
                            } else {
                                errors.push(AssemblerError::new(stmt.line, "Directive .align requires an immediate value".to_string()));
                            }
                            continue;
                        }
                        _ => {
                            match emit_data_bytes(kind, ops) {
                                Ok(bytes) => {
                                    // GNU AS allows data directives in .text; emit to the active section.
                                    if current_section == Section::Text {
                                        self.text_bin.extend_from_slice(&bytes);
                                        current_pc += bytes.len() as u32;
                                    } else {
                                        self.data_bin.extend_from_slice(&bytes);
                                        data_pc += bytes.len() as u32;
                                    }
                                }
                                Err(msg) => {
                                    errors.push(AssemblerError::new(stmt.line, msg));
                                }
                            }
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

        // I-type | Opcode: 0x13 for ALU, 0x03 for Loads, 0x67 for jalr
        "addi"  => encode_i_type(0x13, 0x0, ops, sym_table),
        "slti"  => encode_i_type(0x13, 0x2, ops, sym_table),
        "sltiu" => encode_i_type(0x13, 0x3, ops, sym_table),
        "xori"  => encode_i_type(0x13, 0x4, ops, sym_table),
        "ori"   => encode_i_type(0x13, 0x6, ops, sym_table),
        "andi"  => encode_i_type(0x13, 0x7, ops, sym_table),
        "slli"  => encode_i_shift(0x13, 0x1, 0x00, ops), // Special: uses shift amount
        "srli"  => encode_i_shift(0x13, 0x5, 0x00, ops),
        "srai"  => encode_i_shift(0x13, 0x5, 0x20, ops),

        "lb"    => encode_i_type(0x03, 0x0, ops, sym_table),
        "lh"    => encode_i_type(0x03, 0x1, ops, sym_table),
        "lw"    => encode_i_type(0x03, 0x2, ops, sym_table),
        "lbu"   => encode_i_type(0x03, 0x4, ops, sym_table),
        "lhu"   => encode_i_type(0x03, 0x5, ops, sym_table),

        "jalr"  => encode_i_type(0x67, 0x0, ops, sym_table),

        // S-type | Opcode: 0x23
        "sb"    => encode_s_type(0x23, 0x0, ops, sym_table),
        "sh"    => encode_s_type(0x23, 0x1, ops, sym_table),
        "sw"    => encode_s_type(0x23, 0x2, ops, sym_table),

        // B-type | Opcode: 0x63
        "beq"   => encode_b_type(0x63, 0x0, ops, sym_table, current_pc),
        "bne"   => encode_b_type(0x63, 0x1, ops, sym_table, current_pc),
        "blt"   => encode_b_type(0x63, 0x4, ops, sym_table, current_pc),
        "bge"   => encode_b_type(0x63, 0x5, ops, sym_table, current_pc),
        "bltu"  => encode_b_type(0x63, 0x6, ops, sym_table, current_pc),
        "bgeu"  => encode_b_type(0x63, 0x7, ops, sym_table, current_pc),

        // U-type | Opcode: 0x37 lui, 0x17 auipc
        "lui"   => encode_u_type(0x37, ops, sym_table),
        "auipc" => encode_u_type(0x17, ops, sym_table),

        // J-type | Opcode: 0x6F
        "jal"   => encode_j_type(0x6F, ops, sym_table, current_pc),

        // CSR instructions (Zicsr) | Opcode: 0x73
        "csrrw"  => encode_csr(0x1, ops),
        "csrrs"  => encode_csr(0x2, ops),
        "csrrc"  => encode_csr(0x3, ops),
        "csrrwi" => encode_csr_imm(0x5, ops),
        "csrrsi" => encode_csr_imm(0x6, ops),
        "csrrci" => encode_csr_imm(0x7, ops),

        // System and Miscellaneous
        "ecall"   => Ok(0x00000073),
        "ebreak"  => Ok(0x00100073),
        "fence"   => Ok(0x0FF0000F),
        "fence.i" => Ok(0x0000100F),
        "mret"    => Ok(0x30200073),
        "sret"    => Ok(0x10200073),
        "wfi"     => Ok(0x10500073),

        _ => Err(format!("Unsupported instruction '{}'", name)),
    }
}

fn csr_addr(op: &Operand) -> Result<u32, String> {
    match op {
        Operand::Immediate(v) => Ok(*v as u32 & 0xFFF),
        Operand::Label(name) => match name.as_str() {
            "mstatus"  => Ok(0x300), "misa"     => Ok(0x301),
            "mie"      => Ok(0x304), "mtvec"    => Ok(0x305),
            "mscratch" => Ok(0x340), "mepc"     => Ok(0x341),
            "mcause"   => Ok(0x342), "mtval"    => Ok(0x343),
            "mip"      => Ok(0x344), "mhartid"  => Ok(0xF14),
            "sstatus"  => Ok(0x100), "sie"      => Ok(0x104),
            "stvec"    => Ok(0x105), "sscratch" => Ok(0x140),
            "sepc"     => Ok(0x141), "scause"   => Ok(0x142),
            "stval"    => Ok(0x143), "sip"      => Ok(0x144),
            "cycle"    => Ok(0xC00), "time"     => Ok(0xC01),
            "instret"  => Ok(0xC02),
            _ => Err(format!("Unknown CSR '{}'", name)),
        },
        _ => Err("CSR operand must be an immediate or CSR name".to_string()),
    }
}

fn encode_csr(funct3: u8, ops: &[Operand]) -> Result<u32, String> {
    if let [Operand::Register(rd), csr_op, Operand::Register(rs1)] = ops {
        let csr = csr_addr(csr_op)?;
        Ok((csr << 20) | ((*rs1 as u32) << 15) | ((funct3 as u32) << 12) | ((*rd as u32) << 7) | 0x73)
    } else {
        Err("Invalid operands for CSR instruction: expected rd, csr, rs1".to_string())
    }
}

fn encode_csr_imm(funct3: u8, ops: &[Operand]) -> Result<u32, String> {
    if let [Operand::Register(rd), csr_op, Operand::Immediate(uimm)] = ops {
        let csr = csr_addr(csr_op)?;
        if *uimm < 0 || *uimm > 31 {
            return Err(format!("CSR immediate {} out of range (0-31)", uimm));
        }
        Ok((csr << 20) | ((*uimm as u32) << 15) | ((funct3 as u32) << 12) | ((*rd as u32) << 7) | 0x73)
    } else {
        Err("Invalid operands for CSR immediate instruction: expected rd, csr, uimm".to_string())
    }
}

fn encode_r_type(opcode: u8, funct3: u8, funct7: u8, ops: &[Operand]) -> Result<u32, String> {
    if let [Operand::Register(rd), Operand::Register(rs1), Operand::Register(rs2)] = ops {
        Ok(((funct7 as u32) << 25) | ((*rs2 as u32) << 20) | ((*rs1 as u32) << 15) | ((funct3 as u32) << 12) | ((*rd as u32) << 7) | (opcode as u32))
    } else {
        Err("Invalid operands for R-type instruction: expected 3 registers (rd, rs1, rs2)".to_string())
    }
}

fn encode_i_type(opcode: u8, funct3: u8, ops: &[Operand], sym_table: &SymbolTable) -> Result<u32, String> {
    let (rd, rs1, base_op) = match (opcode, ops) {
        // load: rd, offset(rs1)
        (0x03, [Operand::Register(rd), mem @ Operand::Memory { reg, .. }]) => {
            (*rd, *reg, mem)
        },
        // alu immediate and jalr: rd, rs1, imm
        (0x13 | 0x67, [Operand::Register(rd), Operand::Register(rs1), imm]) => {
            (*rd, *rs1, imm)
        },
        // TODO: jalr with memory offset, is a pseudo-instruction, so allow both: 3 parameters with inmediate or 2 with memory offset
        _ => return Err("Invalid operands for I-type instruction".to_string()),
    };

    let imm_val = resolve_any_immediate(base_op, sym_table)?;

    if imm_val < -2048 || imm_val > 2047 {
        return Err(format!("Immediate value {} out of range for 12-bit field", imm_val));
    }

    let instruction = ((imm_val as u32 & 0xFFF) << 20) |
                      ((rs1 as u32) << 15)            |
                      ((funct3 as u32) << 12)         |
                      ((rd as u32) << 7)              |
                      (opcode as u32);
    Ok(instruction)
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
) -> Result<u32, String> {
    // Note: The usual order in RISC-V is sw rs2, offset(rs1)
    if let [Operand::Register(rs2), Operand::Memory { offset, reg }] = ops {
        // Resolve the immediate (can be label or number)
        let imm_val = resolve_memory_offset(offset, sym_table)?;

        if imm_val < -2048 || imm_val > 2047 {
            return Err(format!("Immediate value {} out of range for 12-bit field", imm_val));
        }

        let imm = (imm_val as u32) & 0xFFF;
        let imm_11_5 = (imm >> 5) & 0x7F; // 7 upper bits
        let imm_4_0 = imm & 0x1F;         // 5 lower bits

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

fn resolve_memory_offset(offset: &MemoryOffset, sym_table: &SymbolTable) -> Result<i32, String> {
    match offset {
        MemoryOffset::Immediate(val) => Ok(*val),
        MemoryOffset::Label(name) => sym_table.get_address(name)
            .map(|addr| addr as i32)
            .ok_or_else(|| format!("Unknown label '{}'", name)),
        MemoryOffset::Modifier(kind, name) => resolve_modifier(kind, name, sym_table),
    }
}

fn resolve_any_immediate(op: &Operand, sym_table: &SymbolTable) -> Result<i32, String> {
    match op {
        // For example in addi x1, x2, 10
        Operand::Immediate(val) => Ok(*val),

        // For example in addi x1, x2, symbol
        Operand::Label(name) => sym_table.get_address(name)
            .map(|addr| addr as i32)
            .ok_or_else(|| format!("Unknown label '{}'", name)),

        // For example in lw x1, 4(x2) o lw x1, symbol(x2)
        Operand::Memory { offset, .. } => resolve_memory_offset(offset, sym_table),

        // For example in addi x1, x2, %hi(symbol)
        Operand::Modifier(kind, name) => resolve_modifier(kind, name, sym_table),

        _ => Err("This operand do not contain a numeric value or a label".to_string()),
    }
}

fn resolve_modifier(kind: &ModifierKind, name: &str, sym_table: &SymbolTable) -> Result<i32, String> {
    let addr = sym_table.get_address(name)
        .ok_or_else(|| format!("Unknown label '{}'", name))?;

    match kind {
        ModifierKind::Hi => {
            // %hi(addr) = (addr + 0x800) >> 12
            // 0x800 is the offset to make the address positive and sign-extend it to 32 bits
            Ok(((addr as i64 + 0x800) >> 12) as i32)
        }
        ModifierKind::Lo => {
            Ok(((addr << 20) as i32) >> 20)
        }
    }
}

fn encode_b_type(opcode: u8, funct3: u8, ops: &[Operand], sym_table: &SymbolTable, current_pc: u32) -> Result<u32, String> {
    if let [Operand::Register(rs1), Operand::Register(rs2), Operand::Label(label)] = ops {
        let label_addr = sym_table.get_address(label)
                .ok_or_else(|| format!("Unknown label '{}'", label))?;
        let offset = (label_addr as i32) - (current_pc as i32);
        if offset < -4096 || offset > 4094 {
            return Err(format!("Branch target offset {} out of range", offset));
        }
        if offset % 2 != 0 {
            return Err(format!("Branch target offset {} must be a multiple of 2", offset));
        }

        let imm = offset as u32;
        let b12    = (imm >> 12) & 0x1;
        let b11    = (imm >> 11) & 0x1;
        let b10_5  = (imm >> 5)  & 0x3F;
        let b4_1   = (imm >> 1)  & 0xF;

        let instruction = (b12 << 31)   |
                          (b10_5 << 25) |
                          ((*rs2 as u32) << 20) |
                          ((*rs1 as u32) << 15) |
                          ((funct3 as u32) << 12) |
                          (b4_1 << 8)   |
                          (b11 << 7)    |
                          (opcode as u32);

        Ok(instruction)
    } else {
        Err("Invalid operands for B-type instruction: expected register, register, label".to_string())
    }
}

fn encode_u_type(
    opcode: u8,
    ops: &[Operand],
    sym_table: &SymbolTable,
) -> Result<u32, String> {
    if let [Operand::Register(rd), imm_op] = ops {
        let val = resolve_any_immediate(imm_op, sym_table)?;
        // TODO review this because i'm not completely sure how the U-immediate is represented in the instruction encoding
        let imm_u32 = val as u32;
        Ok((imm_u32 << 12) | ((*rd as u32) << 7) | (opcode as u32))
    } else {
        Err("Invalid operands for U-type instruction: expected register, immediate/label".to_string())
    }
}

fn encode_j_type(
    opcode: u8,
    ops: &[Operand],
    sym_table: &SymbolTable,
    current_pc: u32
) -> Result<u32, String> {
    if let [Operand::Register(rd), imm_op] = ops {
        let val = resolve_any_immediate(imm_op, sym_table)?;
        let offset = (val as i32) - (current_pc as i32);

        if offset < -1048576 || offset > 1048574 {
            return Err(format!("Jump target offset {} out of range", offset));
        }

        let imm_20 = (offset >> 20) & 0x1;
        let imm_10_1 = (offset >> 1) & 0x3FF;
        let imm_11 = (offset >> 11) & 0x1;
        let imm_19_12 = (offset >> 12) & 0xFF;

        let instruction = ((imm_20 as u32) << 31)   |
                          ((imm_19_12 as u32) << 12) |
                          ((imm_11 as u32) << 20)   |
                          ((imm_10_1 as u32) << 21) |
                          ((*rd as u32) << 7) |
                          (opcode as u32);
        Ok(instruction)
    } else {
        Err("Invalid operands for J-type instruction: expected register, immediate/label".to_string())
    }
}

fn emit_data_bytes(kind: &DirectiveKind, ops: &[Operand]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    match kind {
        DirectiveKind::Byte => {
            for op in ops {
                match op {
                    Operand::Immediate(val) => bytes.push(*val as u8),
                    _ => return Err("Invalid operand for .byte: expected immediate".to_string()),
                }
            }
        }
        DirectiveKind::Half => {
            for op in ops {
                match op {
                    Operand::Immediate(val) => bytes.extend_from_slice(&(*val as u16).to_le_bytes()),
                    _ => return Err("Invalid operand for .half: expected immediate".to_string()),
                }
            }
        }
        DirectiveKind::Word => {
            for op in ops {
                match op {
                    Operand::Immediate(val) => bytes.extend_from_slice(&(*val as u32).to_le_bytes()),
                    _ => return Err("Invalid operand for .word: expected immediate".to_string()),
                }
            }
        }
        DirectiveKind::Ascii => {
            for op in ops {
                match op {
                    Operand::StringLiteral(s) => bytes.extend_from_slice(s.as_bytes()),
                    _ => return Err("Invalid operand for .ascii: expected string literal".to_string()),
                }
            }
        }
        DirectiveKind::Asciz => {
            for op in ops {
                match op {
                    Operand::StringLiteral(s) => {
                        bytes.extend_from_slice(s.as_bytes());
                        bytes.push(0);
                    }
                    _ => return Err("Invalid operand for .asciz: expected string literal".to_string()),
                }
            }
        }
        DirectiveKind::Space => {
            if let Some(Operand::Immediate(val)) = ops.get(0) {
                if *val < 0 {
                    return Err(".space requires a positive value".to_string());
                }
                bytes.resize(bytes.len() + *val as usize, 0);
            } else {
                return Err(".space requires an immediate value".to_string());
            }
        }
        DirectiveKind::Unknown(name) => return Err(format!("Unsupported directive '{}'", name)),
        // Section switches and .align are handled before emit_data_bytes is called.
        DirectiveKind::Text | DirectiveKind::Data | DirectiveKind::Align => unreachable!(),
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    #[test]
    fn test_assemble_simple_program() {
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
                kind: StatementKind::Directive(DirectiveKind::Data, vec![]),
                line: 2,
            },
            Statement {
                kind: StatementKind::Directive(DirectiveKind::Word, vec![
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        let statements = vec![
            Statement {
                kind: StatementKind::Directive(DirectiveKind::Unknown(".float".to_string()), vec![
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
    fn test_csr_and_fence_instructions() {
        let source = "csrrs a0, mhartid, x0\ncsrrw x0, mtvec, t0\nfence\nfence.i\nmret\nsret\nwfi";
        let tokens = crate::lexer::tokenize(source).unwrap();
        let mut parser = crate::parser::Parser::new(tokens);
        let stmts = parser.parse().unwrap();
        let expanded = crate::pseudo::expand(stmts).unwrap();
        let sym_table = crate::symbols::SymbolTable::new(0, 0);
        let mut asm = Assembler::new(0, 0);
        asm.assemble(&expanded, &sym_table).expect("should assemble");

        let words: Vec<u32> = asm.text_bin.chunks(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();

        assert_eq!(words[0], 0xF1402573, "csrrs a0, mhartid, x0");
        assert_eq!(words[1], 0x30529073, "csrrw x0, mtvec, t0");
        assert_eq!(words[2], 0x0FF0000F, "fence");
        assert_eq!(words[3], 0x0000100F, "fence.i");
        assert_eq!(words[4], 0x30200073, "mret");
        assert_eq!(words[5], 0x10200073, "sret");
        assert_eq!(words[6], 0x10500073, "wfi");
    }

    #[test]
    fn test_csr_pseudo_instructions() {
        let source = "csrr a0, mhartid\ncsrw mtvec, t0\ncsrwi mie, 8";
        let tokens = crate::lexer::tokenize(source).unwrap();
        let mut parser = crate::parser::Parser::new(tokens);
        let stmts = parser.parse().unwrap();
        let expanded = crate::pseudo::expand(stmts).unwrap();
        let sym_table = crate::symbols::SymbolTable::new(0, 0);
        let mut asm = Assembler::new(0, 0);
        asm.assemble(&expanded, &sym_table).expect("should assemble");

        let words: Vec<u32> = asm.text_bin.chunks(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();

        // csrr a0, mhartid → csrrs a0, mhartid (0xF14), x0
        assert_eq!(words[0], 0xF1402573, "csrr a0, mhartid");
        // csrw mtvec, t0 → csrrw x0, mtvec (0x305), t0
        assert_eq!(words[1], 0x30529073, "csrw mtvec, t0");
        // csrwi mie, 8 → csrrwi x0, mie (0x304), 8
        assert_eq!(words[2], 0x30445073, "csrwi mie, 8");
    }

    #[test]
    fn test_invalid_directive_operands() {
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        let statements = vec![
            Statement {
                kind: StatementKind::Directive(DirectiveKind::Word, vec![
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
        assert!(errors[0].message.contains("Invalid operand for .word"));
    }

    #[test]
    fn test_multiple_errors() {
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
                kind: StatementKind::Directive(DirectiveKind::Unknown(".float".to_string()), vec![
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
    fn test_modifier_assembly() {
        let mut assembler = Assembler::new(0, 0);
        let mut sym_table = SymbolTable::new(0, 0);

        // my_label at 0x12800 (bit 11 is 1)
        sym_table.add_label("my_label".to_string(), 0x12800).unwrap();

        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("lui".to_string(), vec![
                    Operand::Register(1),
                    Operand::Modifier(ModifierKind::Hi, "my_label".to_string()),
                ]),
                line: 1,
            },
            Statement {
                kind: StatementKind::Instruction("addi".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(1),
                    Operand::Modifier(ModifierKind::Lo, "my_label".to_string()),
                ]),
                line: 2,
            },
            Statement {
                kind: StatementKind::Instruction("lw".to_string(), vec![
                    Operand::Register(2),
                    Operand::Memory {
                        offset: MemoryOffset::Modifier(ModifierKind::Lo, "my_label".to_string()),
                        reg: 1,
                    },
                ]),
                line: 3,
            },
        ];

        assembler.assemble(&statements, &sym_table).expect("Assembly should succeed");

        // LUI x1, %hi(0x12800) -> %hi = (0x12800 + 0x800) >> 12 = 0x13
        // Result: 0x000130B7
        assert_eq!(u32::from_le_bytes(assembler.text_bin[0..4].try_into().unwrap()), 0x000130B7);

        // ADDI x1, x1, %lo(0x12800) -> %lo = 0x12800 & 0xFFF = 0x800 (signed -2048)
        // Result: 0x80008093
        assert_eq!(u32::from_le_bytes(assembler.text_bin[4..8].try_into().unwrap()), 0x80008093);

        // LW x2, %lo(0x12800)(x1) -> %lo = 0x800
        // I-type: imm[11:0]=0x800, rs1=1, funct3=010, rd=2, opcode=0000011
        // 0x80000000 | 0x8000 | 0x2000 | 0x100 | 0x03
        assert_eq!(u32::from_le_bytes(assembler.text_bin[8..12].try_into().unwrap()), 0x8000A103);
    }

    #[test]
    fn test_assemble_i_type_instruction() {
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
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

    #[test]
    fn test_encoding_of_b_type_instruction() {
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let mut sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        sym_table.add_label("target".to_string(), config::TEXT_BASE + 0x10).unwrap();
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("beq".to_string(), vec![
                    Operand::Register(1),
                    Operand::Register(2),
                    Operand::Label("target".to_string()),
                ]),
                line: 1,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_ok());
        let instructions = assembler.text_bin;
        assert_eq!(instructions.len(), 4);
        // beq x1, x2, target (offset = target - current_pc = 0x0040_0010 - 0x0040_0000 = 16)
        // opcode=0x63, funct3=0x0, rs1=1, rs2=2, imm=16
        assert_eq!(
            instructions,
            vec![
                0b01100011, // 0x63: imm[11] + opcode
                0b10001000, // 0x88: rs1[0] + funct3 + imm[4:1]
                0b00100000, // 0x20: rs2[3:0] + rs1[4:1]
                0b00000000, // 0x00: imm[12] + imm[10:5] + rs2[4]
            ]
        );
    }

    #[test]
    fn test_encoding_of_u_type_instruction() {
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        let statements = vec![
            Statement {
                kind: StatementKind::Instruction("lui".to_string(), vec![
                    Operand::Register(5),
                    Operand::Immediate(0xF1),
                ]),
                line: 1,
            },
        ];

        let result = assembler.assemble(&statements, &sym_table);
        assert!(result.is_ok());
        let instructions = assembler.text_bin;
        assert_eq!(instructions.len(), 4);
        // lui x5, 0x12345
        // opcode=0x37, rd=5, imm=0xF1
        assert_eq!(
            instructions,
            vec![
                0b10110111, // 0x37: rd[0] + opcode
                0b00010010, // 0x00: imm[19:12] + rd[4:1]
                0b00001111, // 0x00: imm[11:4]
                0b00000000, // 0x00: imm[31:20]
            ]
        );
    }

    #[test]
    fn test_extended_directives() {
        let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
        let mut sym_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
        let source = r#"
            .data
            .byte 1, 2, 3
            .half 0x1234, 0x5678
            .word 0xDEADBEEF
            .asciz "RISC-V"
            .align 2
            .word 42
        "#;
        let tokens = crate::lexer::tokenize(source).unwrap();
        let mut parser = crate::parser::Parser::new(tokens);
        let statements = parser.parse().unwrap();
        sym_table.build(&statements).unwrap();

        assembler.assemble(&statements, &sym_table).expect("Assembly should succeed");

        assert_eq!(assembler.data_bin.len(), 24);
        assert_eq!(assembler.data_bin[0..3], [1, 2, 3]);
        assert_eq!(assembler.data_bin[3..7], [0x34, 0x12, 0x78, 0x56]);
        assert_eq!(assembler.data_bin[7..11], [0xEF, 0xBE, 0xAD, 0xDE]);
        assert_eq!(assembler.data_bin[11..18], *b"RISC-V\0");
        assert_eq!(assembler.data_bin[18..20], [0, 0]);
        assert_eq!(assembler.data_bin[20..24], [42, 0, 0, 0]);
    }
}
