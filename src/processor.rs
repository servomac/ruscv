use crate::config;

// TODO: this is not a good way to represent memory, it should be a
// contiguous block of memory with different segments;
// view the read_byte and write_byte methods to see how memory is accessed.
struct Memory {
    text: Vec<u8>,
    data: Vec<u8>,
    stack: Vec<u8>,
    text_base: u32,
    data_base: u32,
    stack_base: u32,
}

#[derive(Debug, PartialEq)]
enum MemoryFault {
    OutOfBounds { address: u32 },
    WriteToReadOnly { address: u32 },           // TODO
    UnalignedAccess { address: u32 },           // TODO
    ExecuteFromNonExecutable { address: u32 },  // TODO: check in fetch
}

impl Memory {
    fn read_byte(&self, address: u32) -> Result<u8, MemoryFault> {
        if address >= self.text_base && address < self.text_base + self.text.len() as u32 {
            Ok(self.text[(address - self.text_base) as usize])
        } else if address >= self.data_base && address < self.data_base + self.data.len() as u32 {
            Ok(self.data[(address - self.data_base) as usize])
        } else if address >= self.stack_base && address < self.stack_base + self.stack.len() as u32 {
            Ok(self.stack[(address - self.stack_base) as usize])
        } else {
            Err(MemoryFault::OutOfBounds { address })
        }
    }

    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), MemoryFault> {
        if address >= self.text_base && address < self.text_base + self.text.len() as u32 {
            self.text[(address - self.text_base) as usize] = value;
        } else if address >= self.data_base && address < self.data_base + self.data.len() as u32 {
            self.data[(address - self.data_base) as usize] = value;
        } else if address >= self.stack_base && address < self.stack_base + self.stack.len() as u32 {
            self.stack[(address - self.stack_base) as usize] = value;
        } else {
            return Err(MemoryFault::OutOfBounds { address });
        }
        Ok(())
    }

    fn write_half(&mut self, address: u32, value: u16) -> Result<(), MemoryFault> {
        let byte0 = value as u8;
        let byte1 = (value >> 8) as u8;
        self.write_byte(address, byte0)?;
        self.write_byte(address + 1, byte1)?;
        Ok(())
    }

    fn write_word(&mut self, address: u32, value: u32) -> Result<(), MemoryFault> {
        let byte0 = value as u8;
        let byte1 = (value >> 8) as u8;
        let byte2 = (value >> 16) as u8;
        let byte3 = (value >> 24) as u8;
        self.write_byte(address, byte0)?;
        self.write_byte(address + 1, byte1)?;
        self.write_byte(address + 2, byte2)?;
        self.write_byte(address + 3, byte3)?;
        Ok(())
    }

    fn read_half(&self, address: u32) -> Result<u16, MemoryFault> {
        let byte0 = self.read_byte(address)?;
        let byte1 = self.read_byte(address + 1)?;
        Ok((byte1 as u16) << 8 | (byte0 as u16))
    }

    fn read_word(&self, address: u32) -> Result<u32, MemoryFault> {
        let byte0 = self.read_byte(address)?;
        let byte1 = self.read_byte(address + 1)?;
        let byte2 = self.read_byte(address + 2)?;
        let byte3 = self.read_byte(address + 3)?;
        Ok(
            (byte3 as u32) << 24 |
            (byte2 as u32) << 16 |
            (byte1 as u32) << 8 |
            (byte0 as u32)
        )
    }
}

pub struct Processor {
    pc: u32,
    registers: [u32; config::NUM_REGISTERS],
    memory: Memory,
}

#[derive(Debug, PartialEq)]
enum StepError {
    IllegalInstruction,
    MemoryFault(MemoryFault),
    Ebreak,
}

impl From<MemoryFault> for StepError {
    // This allows to use the ? operator and handle the conversion from MemoryFault to StepError
    fn from(fault: MemoryFault) -> Self {
        StepError::MemoryFault(fault)
    }
}

#[derive(Debug, PartialEq)]
enum Instruction {
    // R-type: register op register
    Add  { rd: usize, rs1: usize, rs2: usize },
    Sub  { rd: usize, rs1: usize, rs2: usize },
    And  { rd: usize, rs1: usize, rs2: usize },
    Or   { rd: usize, rs1: usize, rs2: usize },
    Xor  { rd: usize, rs1: usize, rs2: usize },
    Sll  { rd: usize, rs1: usize, rs2: usize },
    Srl  { rd: usize, rs1: usize, rs2: usize },
    Sra  { rd: usize, rs1: usize, rs2: usize },
    Slt  { rd: usize, rs1: usize, rs2: usize },
    Sltu { rd: usize, rs1: usize, rs2: usize },

    // I-type: register op immediate
    Addi  { rd: usize, rs1: usize, imm: i32 },
    Andi  { rd: usize, rs1: usize, imm: i32 },
    Ori   { rd: usize, rs1: usize, imm: i32 },
    Xori  { rd: usize, rs1: usize, imm: i32 },
    Slli  { rd: usize, rs1: usize, shamt: u32 },
    Srli  { rd: usize, rs1: usize, shamt: u32 },
    Srai  { rd: usize, rs1: usize, shamt: u32 },
    Slti  { rd: usize, rs1: usize, imm: i32 },
    Sltiu { rd: usize, rs1: usize, imm: i32 },

    // Loads
    Lb  { rd: usize, rs1: usize, imm: i32 },
    Lh  { rd: usize, rs1: usize, imm: i32 },
    Lw  { rd: usize, rs1: usize, imm: i32 },
    Lbu { rd: usize, rs1: usize, imm: i32 },
    Lhu { rd: usize, rs1: usize, imm: i32 },

    // S-type: stores
    Sb { rs1: usize, rs2: usize, imm: i32 },
    Sh { rs1: usize, rs2: usize, imm: i32 },
    Sw { rs1: usize, rs2: usize, imm: i32 },

    // B-type: branches
    Beq  { rs1: usize, rs2: usize, imm: i32 },
    Bne  { rs1: usize, rs2: usize, imm: i32 },
    Blt  { rs1: usize, rs2: usize, imm: i32 },
    Bge  { rs1: usize, rs2: usize, imm: i32 },
    Bltu { rs1: usize, rs2: usize, imm: i32 },
    Bgeu { rs1: usize, rs2: usize, imm: i32 },

    // U-type
    Lui   { rd: usize, imm: i32 },
    Auipc { rd: usize, imm: i32 },

    // J-type
    Jal  { rd: usize, imm: i32 },
    Jalr { rd: usize, rs1: usize, imm: i32 },

    // System
    Ecall,
    Ebreak,
}

impl Processor {
    pub fn new(text_base: u32, data_base: u32, stack_base: u32, stack_size: usize) -> Self {
        Processor {
            pc: 0,                      // filled by load
            registers: [0; config::NUM_REGISTERS],
            memory: Memory {
                text: Vec::new(),       // filled by load
                data: Vec::new(),       // filled by load
                stack: vec![0u8; stack_size],  // pre-allocated, grows downward from stack_base
                text_base,
                data_base,
                stack_base,
            },
        }
    }

    pub fn load(&mut self, text: &Vec<u8>, data: &Vec<u8>) {
        self.memory.text = text.clone();
        self.memory.data = data.clone();
        self.pc = self.memory.text_base;
    }

    pub fn step(&mut self) -> Result<(), StepError> {
        // TODO return StepResult for the visibility outside the processor? i.e. UI?
        // separation of concerns vs monitoring
        let memory_instruction = self.fetch()?;
        let instruction = self.decode(memory_instruction)?;
        self.execute(instruction)?;
        Ok(())
    }

    fn fetch(&self) -> Result<u32, StepError> {
        // TODO handle overflow as well as negative offsets MemoryFaults
        let offset = (self.pc - self.memory.text_base) as usize;

        // obtain 4 bytes representing the instruction
        let bytes = self.memory.text.get(offset..offset + 4)
            .ok_or(MemoryFault::OutOfBounds { address: self.pc })?;

        // assemble 4 bytes into u32, assuming little endian
        let instruction = u32::from_le_bytes(bytes.try_into().unwrap());
        Ok(instruction)
    }

    fn decode(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let opcode = memory_instruction & 0x7F;

        match opcode {
            0b0110011 => self.decode_r_type(memory_instruction),
            0b0010011 => self.decode_i_type(memory_instruction),
            0b0000011 => self.decode_load_type(memory_instruction),
            0b0100011 => self.decode_s_type(memory_instruction),
            0b1100011 => self.decode_b_type(memory_instruction),
            0b1101111 => self.decode_j_type(memory_instruction), // jal
            0b1100111 => self.decode_jalr_type(memory_instruction), // jalr
            0b0110111 => self.decode_u_type(memory_instruction), // lui
            0b0010111 => self.decode_u_type(memory_instruction), // auipc
            0b1110011 => self.decode_system_type(memory_instruction), // ecall, ebreak
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_r_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rd = ((memory_instruction >> 7) & 0x1F) as usize;
        let rs1 = ((memory_instruction >> 15) & 0x1F) as usize;
        let rs2 = ((memory_instruction >> 20) & 0x1F) as usize;

        let func3 = (memory_instruction >> 12) & 0x7;
        let func7 = (memory_instruction >> 25) & 0x7F;

        match (func3, func7) {
            (0x0, 0x0) => Ok(Instruction::Add { rd, rs1, rs2 }),
            (0x0, 0x20) => Ok(Instruction::Sub { rd, rs1, rs2 }),
            (0x4, 0x00) => Ok(Instruction::Xor { rd, rs1, rs2 }),
            (0x6, 0x00) => Ok(Instruction::Or { rd, rs1, rs2 }),
            (0x7, 0x00) => Ok(Instruction::And { rd, rs1, rs2 }),
            (0x1, 0x00) => Ok(Instruction::Sll { rd, rs1, rs2 }),
            (0x5, 0x00) => Ok(Instruction::Srl { rd, rs1, rs2 }),
            (0x5, 0x20) => Ok(Instruction::Sra { rd, rs1, rs2 }),
            (0x2, 0x00) => Ok(Instruction::Slt { rd, rs1, rs2 }),
            (0x3, 0x00) => Ok(Instruction::Sltu { rd, rs1, rs2 }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_i_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rd = ((memory_instruction >> 7) & 0x1F) as usize;
        let rs1 = ((memory_instruction >> 15) & 0x1F) as usize;
        let imm = (memory_instruction as i32) >> 20;  // arithmetic shift propagates sign

        let func3 = (memory_instruction >> 12) & 0x7;

        match func3 {
            0x0 => Ok(Instruction::Addi { rd, rs1, imm }),
            0x4 => Ok(Instruction::Xori { rd, rs1, imm }),
            0x6 => Ok(Instruction::Ori { rd, rs1, imm }),
            0x7 => Ok(Instruction::Andi { rd, rs1, imm }),
            0x1 => self.decode_i_shift(memory_instruction),
            0x5 => self.decode_i_shift(memory_instruction),
            0x2 => Ok(Instruction::Slti { rd, rs1, imm }),
            0x3 => Ok(Instruction::Sltiu { rd, rs1, imm }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_i_shift(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rd = ((memory_instruction >> 7) & 0x1F) as usize;
        let rs1 = ((memory_instruction >> 15) & 0x1F) as usize;
        let func7 = (memory_instruction >> 25) & 0x7F;  // bits 31:25
        let shamt = (memory_instruction >> 20) & 0x1F;  // bits 24:20
        let func3 = (memory_instruction >> 12) & 0x7;

        match (func3, func7) {
            (0x1, 0x0) => Ok(Instruction::Slli { rd, rs1, shamt }),
            (0x5, 0x0) => Ok(Instruction::Srli { rd, rs1, shamt }),
            (0x5, 0x20) => Ok(Instruction::Srai { rd, rs1, shamt }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_load_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rd = ((memory_instruction >> 7) & 0x1F) as usize;
        let rs1 = ((memory_instruction >> 15) & 0x1F) as usize;
        let imm = (memory_instruction as i32) >> 20;  // arithmetic shift propagates sign

        let func3 = (memory_instruction >> 12) & 0x7;

        match func3 {
            0x0 => Ok(Instruction::Lb { rd, rs1, imm }),
            0x1 => Ok(Instruction::Lh { rd, rs1, imm }),
            0x2 => Ok(Instruction::Lw { rd, rs1, imm }),
            0x4 => Ok(Instruction::Lbu { rd, rs1, imm }),
            0x5 => Ok(Instruction::Lhu { rd, rs1, imm }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_s_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rs1 = ((memory_instruction >> 15) & 0x1F) as usize;
        let rs2 = ((memory_instruction >> 20) & 0x1F) as usize;
        let imm_11_5 = (memory_instruction as i32) >> 25; // Arithmetic shift propagates sign to 31:6
        let imm_4_0  = ((memory_instruction >> 7) & 0x1F) as i32;  // bits 11:7
        let imm = (imm_11_5 << 5) | imm_4_0; // bit 11 is the sign bit, and bits 31:12 are correct

        let func3 = (memory_instruction >> 12) & 0x7;

        match func3 {
            0x0 => Ok(Instruction::Sb { rs1, rs2, imm }),
            0x1 => Ok(Instruction::Sh { rs1, rs2, imm }),
            0x2 => Ok(Instruction::Sw { rs1, rs2, imm }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_b_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rs1 = ((memory_instruction >> 15) & 0x1F) as usize;
        let rs2 = ((memory_instruction >> 20) & 0x1F) as usize;

        let imm_12   = ((memory_instruction >> 31) & 0x1) as i32;
        let imm_10_5 = ((memory_instruction >> 25) & 0x3F) as i32;
        let imm_4_1  = ((memory_instruction >> 8)  & 0xF) as i32;
        let imm_11   = ((memory_instruction >> 7)  & 0x1) as i32;
        let imm = (imm_12 << 12) | (imm_11 << 11) | (imm_10_5 << 5) | (imm_4_1 << 1);
        let imm = (imm << 19) >> 19;  // sign extend from bit 12 (31-19=12)

        let func3 = (memory_instruction >> 12) & 0x7;

        match func3 {
            0x0 => Ok(Instruction::Beq { rs1, rs2, imm }),
            0x1 => Ok(Instruction::Bne { rs1, rs2, imm }),
            0x4 => Ok(Instruction::Blt { rs1, rs2, imm }),
            0x5 => Ok(Instruction::Bge { rs1, rs2, imm }),
            0x6 => Ok(Instruction::Bltu { rs1, rs2, imm }),
            0x7 => Ok(Instruction::Bgeu { rs1, rs2, imm }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_u_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rd = ((memory_instruction >> 7) & 0x1F) as usize;
        let imm = (memory_instruction & 0xFFFFF000) as i32;

        let opcode = memory_instruction & 0x7F;

        match opcode {
            0x37 => Ok(Instruction::Lui { rd, imm }),
            0x17 => Ok(Instruction::Auipc { rd, imm }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_j_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rd = ((memory_instruction >> 7) & 0x1F) as usize;

        let imm_20 = (memory_instruction >> 31) & 0x1;
        let imm_10_1 = (memory_instruction >> 21) & 0x3FF;
        let imm_11 = (memory_instruction >> 20) & 0x1;
        let imm_19_12 = (memory_instruction >> 12) & 0xFF;

        let imm = (imm_20 << 20) | (imm_19_12 << 12) | (imm_11 << 11) | (imm_10_1 << 1);
        let imm = ((imm as i32) << 11) >> 11;  // sign extend from bit 20 (31-11=20)

        Ok(Instruction::Jal { rd, imm })
    }

    fn decode_jalr_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rd = ((memory_instruction >> 7) & 0x1F) as usize;
        let rs1 = ((memory_instruction >> 15) & 0x1F) as usize;
        let imm = (memory_instruction as i32) >> 20;  // arithmetic shift propagates sign

        let func3 = (memory_instruction >> 12) & 0x7;
        if func3 != 0x0 {
            return Err(StepError::IllegalInstruction);
        }

        Ok(Instruction::Jalr { rd, rs1, imm })
    }

    fn decode_system_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let imm = ((memory_instruction >> 20) & 0xFFF) as i32;

        match imm {
            0x0 => Ok(Instruction::Ecall),
            0x1 => Ok(Instruction::Ebreak),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn execute(&mut self, instruction: Instruction) -> Result<(), StepError> {
        let mut next_pc = self.pc.wrapping_add(4);

        match instruction {
            Instruction::Add { rd, rs1, rs2 } => {
                // wrapping_add allows us not to panic on overflows and maintain the semantic of risc-v
                // returns (a + b) mod 2^N
                let result = self.read_register(rs1).wrapping_add(self.read_register(rs2));
                self.write_register(rd, result);
            },
            Instruction::Sub { rd, rs1, rs2 } => {
                let result = self.read_register(rs1).wrapping_sub(self.read_register(rs2));
                self.write_register(rd, result);
            },
            Instruction::Or { rd, rs1, rs2 } => {
                self.write_register(rd, self.read_register(rs1) | self.read_register(rs2));
            },
            Instruction::And { rd, rs1, rs2 } => {
                self.write_register(rd, self.read_register(rs1) & self.read_register(rs2));
            },
            Instruction::Xor { rd, rs1, rs2 } => {
                self.write_register(rd, self.read_register(rs1) ^ self.read_register(rs2));
            },
            Instruction::Sll { rd, rs1, rs2 } => {
                // Shift logical left on the value in register rs1 by the shift amount held in the lower 5 bits of register rs2
                let shamt = self.read_register(rs2) & 0x1F;
                self.write_register(rd, self.read_register(rs1) << shamt);
            },
            Instruction::Srl { rd, rs1, rs2 } => {
                // Shift logical right
                let shamt = self.read_register(rs2) & 0x1F;
                self.write_register(rd, self.read_register(rs1) >> shamt);
            },
            Instruction::Sra { rd, rs1, rs2 } => {
                // Shift right arithmetic
                let shamt = self.read_register(rs2) & 0x1F;
                let result = (self.read_register(rs1) as i32) >> shamt; // i32 >> is arithmetic
                self.write_register(rd, result as u32);
            },
            Instruction::Slt { rd, rs1, rs2 } => {
                let result = if (self.read_register(rs1) as i32) < (self.read_register(rs2) as i32) { 1 } else { 0 };
                self.write_register(rd, result);
            },
            Instruction::Sltu { rd, rs1, rs2 } => {
                let result = if self.read_register(rs1) < self.read_register(rs2) { 1 } else { 0 };
                self.write_register(rd, result);
            },
            Instruction::Addi { rd, rs1, imm } => {
                // casting i32 to u32 preserves the bit pattern
                let result = self.read_register(rs1).wrapping_add(imm as u32);
                self.write_register(rd, result);
            },
            Instruction::Xori { rd, rs1, imm } => {
                let result = self.read_register(rs1) ^ imm as u32;
                self.write_register(rd, result);
            },
            Instruction::Ori { rd, rs1, imm } => {
                let result = self.read_register(rs1) | imm as u32;
                self.write_register(rd, result);
            },
            Instruction::Andi { rd, rs1, imm } => {
                let result = self.read_register(rs1) & imm as u32;
                self.write_register(rd, result);
            },
            Instruction::Slli { rd, rs1, shamt } => {
                // shamt is already only the bits[0:4], masked in the decode
                let result = self.read_register(rs1) << shamt;
                self.write_register(rd, result);
            },
            Instruction::Srli { rd, rs1, shamt } => {
                // u32 >> is logical shift, fills with zeros
                let result = self.read_register(rs1) >> shamt;
                self.write_register(rd, result);
            },
            Instruction::Srai { rd, rs1, shamt } => {
                // i32 >> is arithmetic shift, fills with sign bit
                let result = (self.read_register(rs1) as i32) >> shamt;
                self.write_register(rd, result as u32);
            },
            Instruction::Slti { rd, rs1, imm } => {
                let result = if (self.read_register(rs1) as i32) < imm { 1 } else { 0 };
                self.write_register(rd, result);
            },
            Instruction::Sltiu { rd, rs1, imm } => {
                let result = if self.read_register(rs1) < imm as u32 { 1 } else { 0 };
                self.write_register(rd, result);
            },
            Instruction::Lb { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:7] (sign extended)
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.memory.read_byte(address)?;
                self.write_register(rd, value as i8 as u32);
            },
            Instruction::Lh { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:15] (sign extended)
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.memory.read_half(address)?;
                self.write_register(rd, value as i16 as u32);
            },
            Instruction::Lw { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:31]
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.memory.read_word(address)?;
                self.write_register(rd, value);
            },
            Instruction::Lbu { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:7] (zero extended)
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.memory.read_byte(address)?;
                self.write_register(rd, value as u32);
            },
            Instruction::Lhu { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:15] (zero extended)
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.memory.read_half(address)?;
                self.write_register(rd, value as u32);
            },
            Instruction::Sb { rs1, rs2, imm } => {
                // M[rs1+imm][0:7] = rs2[0:7]
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                self.memory.write_byte(address, self.read_register(rs2) as u8)?;
            },
            Instruction::Sh { rs1, rs2, imm } => {
                // M[rs1+imm][0:15] = rs2[0:15]
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                self.memory.write_half(address, self.read_register(rs2) as u16)?;
            },
            Instruction::Sw { rs1, rs2, imm } => {
                // M[rs1+imm][0:31] = rs2[0:31]
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                self.memory.write_word(address, self.read_register(rs2))?;
            },
            Instruction::Beq { rs1, rs2, imm } => {
                // if(rs1 == rs2) PC += imm
                if self.read_register(rs1) == self.read_register(rs2) {
                    next_pc = self.pc.wrapping_add(imm as u32);
                }
            },
            Instruction::Bne { rs1, rs2, imm } => {
                // if(rs1 != rs2) PC += imm
                if self.read_register(rs1) != self.read_register(rs2) {
                    next_pc = self.pc.wrapping_add(imm as u32);
                }
            },
            Instruction::Blt { rs1, rs2, imm } => {
                // if(rs1 < rs2) PC += imm
                if (self.read_register(rs1) as i32) < (self.read_register(rs2) as i32) {
                    next_pc = self.pc.wrapping_add(imm as u32);
                }
            },
            Instruction::Bge { rs1, rs2, imm } => {
                // if(rs1 >= rs2) PC += imm
                if (self.read_register(rs1) as i32) >= (self.read_register(rs2) as i32) {
                    next_pc = self.pc.wrapping_add(imm as u32);
                }
            },
            Instruction::Bltu { rs1, rs2, imm } => {
                // if(rs1 < rs2) PC += imm (zero extended / unsigned comparison)
                if self.read_register(rs1) < self.read_register(rs2) {
                    next_pc = self.pc.wrapping_add(imm as u32);
                }
            },
            Instruction::Bgeu { rs1, rs2, imm } => {
                // if(rs1 >= rs2) PC += imm (zero extended / unsigned comparison)
                if self.read_register(rs1) >= self.read_register(rs2) {
                    next_pc = self.pc.wrapping_add(imm as u32);
                }
            },
            Instruction::Jal { rd, imm } => {
                // rd = PC+4; PC += imm
                self.write_register(rd, self.pc.wrapping_add(4));
                next_pc = self.pc.wrapping_add(imm as u32);
            },
            Instruction::Jalr { rd, rs1, imm } => {
                // rd = PC+4; PC = rs1 + imm
                self.write_register(rd, self.pc.wrapping_add(4));
                // The & !1 masks out bit 0, ensuring the target is always 2-byte aligned
                next_pc = self.read_register(rs1).wrapping_add(imm as u32) & !1;
            },
            Instruction::Lui { rd, imm } => {
                // rd = upper imm (upper mask already applied by the decoder)
                self.write_register(rd, imm as u32);
            },
            Instruction::Auipc { rd, imm } => {
                // rd = PC + upper imm (upper mask already applied by the decoder)
                self.write_register(rd, self.pc.wrapping_add(imm as u32));
            },
            // TODO pending instructions: ecall, ebreak
            _ => return Err(StepError::IllegalInstruction),
        }

        self.pc = next_pc;
        Ok(())
    }

    fn read_register(&self, index: usize) -> u32 {
        if index == 0 {
            return 0;
        }
        self.registers[index]
    }

    fn write_register(&mut self, index: usize, value: u32) {
        if index == 0 {
            return;
        }
        self.registers[index] = value;
    }

    pub fn show_state(&self) {
        println!("PC: {}", self.pc);
        println!("Registers: {:?}", self.registers);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_add() {
        let processor = Processor::new(0, 0, 0, 0);
        // 0000000 (f7) | 00011 (rs2) | 00010 (rs1) | 000 (f3) | 00001 (rd) | 0110011 (op)
        let instruction = processor.decode(0x003100B3).unwrap();
        assert_eq!(instruction, Instruction::Add { rd: 1, rs1: 2, rs2: 3 });
    }

    #[test]
    fn test_decode_addi() {
        let processor = Processor::new(0, 0, 0, 0);
        // addi x1, x2, -1
        // imm[11:0] = -1 (0xFFF) | rs1=2 | f3=0 | rd=1 | op=0010011
        let instruction = processor.decode(0xFFF10093).unwrap();
        assert_eq!(instruction, Instruction::Addi { rd: 1, rs1: 2, imm: -1 });

        // addi x1, x2, 1
        let instruction = processor.decode(0x00110093).unwrap();
        assert_eq!(instruction, Instruction::Addi { rd: 1, rs1: 2, imm: 1 });
    }

    #[test]
    fn test_decode_sw() {
        let processor = Processor::new(0, 0, 0, 0);
        // sw x3, -4(x2)
        // imm[11:5] = -1 (0xfe0 >> 5 = 0x7f) | rs2=3 | rs1=2 | f3=2 | imm[4:0] = -4 & 0x1f (0x1c) | op=0100011
        // inst = 0xFE312E23
        let instruction = processor.decode(0xFE312E23).unwrap();
        assert_eq!(instruction, Instruction::Sw { rs1: 2, rs2: 3, imm: -4 });
    }

    #[test]
    fn test_decode_beq() {
        let processor = Processor::new(0, 0, 0, 0);
        // beq x1, x2, -4
        // imm = -4 (0xfffffffc)
        // imm[12]=1, imm[11]=1, imm[10:5]=0x3f, imm[4:1]=0xe
        // inst[31]=1, inst[7]=1, inst[30:25]=0x3f, inst[11:8]=0xe, rs2=2, rs1=1, f3=0, op=1100011
        // inst = 0xFE208EE3
        let instruction = processor.decode(0xFE208EE3).unwrap();
        assert_eq!(instruction, Instruction::Beq { rs1: 1, rs2: 2, imm: -4 });
    }

    #[test]
    fn test_decode_lui() {
        let processor = Processor::new(0, 0, 0, 0);
        // lui x5, 0x12345
        // imm[31:12]=0x12345, rd=5, op=0110111
        let instruction = processor.decode(0x123452B7).unwrap();
        assert_eq!(instruction, Instruction::Lui { rd: 5, imm: 0x12345000 });
    }

    #[test]
    fn test_decode_jal() {
        let processor = Processor::new(0, 0, 0, 0);
        // jal x1, -4
        // imm = -4 (0xfffffffc)
        // imm[20]=1, imm[19:12]=0xff, imm[11]=1, imm[10:1]=0x3fe
        // inst[31]=1, inst[30:21]=0x3fe, inst[20]=1, inst[19:12]=0xff, rd=1, op=1101111
        // Binary: 1 1111111110 1 11111111 00001 1101111
        // Groups: 1111 1111 1101 1111 1111 0000 1110 1111 => 0xFFDFF0EF
        let instruction = processor.decode(0xFFDFF0EF).unwrap();
        assert_eq!(instruction, Instruction::Jal { rd: 1, imm: -4 });
    }

    #[test]
    fn test_decode_jalr() {
        let processor = Processor::new(0, 0, 0, 0);
        // jalr x1, 4(x2)
        // imm=4 | rs1=2 | f3=0 | rd=1 | op=1100111
        let instruction = processor.decode(0x004100E7).unwrap();
        assert_eq!(instruction, Instruction::Jalr { rd: 1, rs1: 2, imm: 4 });
    }

    #[test]
    fn test_decode_shifts() {
        let processor = Processor::new(0, 0, 0, 0);

        // slli x1, x2, 5
        // imm[11:5]=0 | shamt=5 | rs1=2 | f3=1 | rd=1 | op=0010011
        // 0000000 00101 00010 001 00001 0010011 => 0x00511093
        let instruction = processor.decode(0x00511093).unwrap();
        assert_eq!(instruction, Instruction::Slli { rd: 1, rs1: 2, shamt: 5 });

        // srli x1, x2, 5
        // imm[11:5]=0 | shamt=5 | rs1=2 | f3=5 | rd=1 | op=0010011
        // 0000000 00101 00010 101 00001 0010011 => 0x00515093
        let instruction = processor.decode(0x00515093).unwrap();
        assert_eq!(instruction, Instruction::Srli { rd: 1, rs1: 2, shamt: 5 });

        // srai x1, x2, 5
        // imm[11:5]=0x20 | shamt=5 | rs1=2 | f3=5 | rd=1 | op=0010011
        // 0100000 00101 00010 101 00001 0010011 => 0x40515093
        let instruction = processor.decode(0x40515093).unwrap();
        assert_eq!(instruction, Instruction::Srai { rd: 1, rs1: 2, shamt: 5 });
    }

    #[test]
    fn test_decode_shift_max_shamt() {
        let processor = Processor::new(0, 0, 0, 0);
        // slli x1, x2, 31  — maximum meaningful shift for 32-bit registers
        // funct7=0000000 | shamt=11111 | rs1=00010 | funct3=001 | rd=00001 | op=0010011
        // 0x01F11093
        let instruction = processor.decode(0x01F11093).unwrap();
        assert_eq!(instruction, Instruction::Slli { rd: 1, rs1: 2, shamt: 31 });
    }

    #[test]
    fn test_decode_shift_invalid_func7() {
        let processor = Processor::new(0, 0, 0, 0);
        // srli with funct7=0x10 (invalid — only 0x00 and 0x20 are valid for funct3=0x5)
        // funct7=0010000 | shamt=00100 | rs1=00010 | funct3=101 | rd=00001 | op=0010011
        // 0x20415093
        let result = processor.decode(0x20415093);
        assert_eq!(result, Err(StepError::IllegalInstruction));
    }

    #[test]
    fn test_execute_add() {
        let mut processor = Processor::new(0, 0, 0, 0);
        processor.registers[1] = 10;
        processor.registers[2] = -20i32 as u32;
        let instruction = Instruction::Add { rd: 3, rs1: 1, rs2: 2 };
        processor.execute(instruction).unwrap();
        assert_eq!(processor.registers[3], -10i32 as u32);
    }

    #[test]
    fn test_execute_and() {
        let mut processor = Processor::new(0, 0, 0, 0);
        processor.registers[1] = 0b1100;
        processor.registers[2] = 0b1010;
        let instruction = Instruction::And { rd: 3, rs1: 1, rs2: 2 };
        processor.execute(instruction).unwrap();
        assert_eq!(processor.registers[3], 0b1000);
    }

    #[test]
    fn test_execute_x0() {
        let mut processor = Processor::new(0, 0, 0, 0);
        processor.registers[1] = 10;
        processor.registers[2] = 20;
        // Instruction that tries to write to x0
        let instruction = Instruction::Add { rd: 0, rs1: 1, rs2: 2 };
        processor.execute(instruction).unwrap();
        assert_eq!(processor.registers[0], 0);
    }

    #[test]
    fn test_step_pc_increment() {
        let mut processor = Processor::new(0x400000, 0, 0, 0);
        // add x3, x1, x2 (0x002081B3)
        processor.memory.text = vec![0xB3, 0x81, 0x20, 0x00];
        processor.pc = 0x400000;

        processor.step().unwrap();
        assert_eq!(processor.pc, 0x400000 + 4);
    }

    #[test]
    fn test_execute_slt_negative() {
        let mut processor = Processor::new(0, 0, 0, 0);
        // x1 = 2, x2 = 1 → x1 > x2 signed → rd = 0
        processor.registers[1] = 2;
        processor.registers[2] = 1;
        processor.execute(Instruction::Slt { rd: 3, rs1: 1, rs2: 2 }).unwrap();
        assert_eq!(processor.registers[3], 0);
    }

    #[test]
    fn test_execute_slt_signed_vs_unsigned() {
        let mut processor = Processor::new(0, 0, 0, 0);
        // x1 = -1 (0xFFFFFFFF), x2 = 1
        // signed: -1 < 1 → rd = 1  (this is the key difference with sltu)
        processor.registers[1] = 0xFFFFFFFF;
        processor.registers[2] = 1;
        processor.execute(Instruction::Slt { rd: 3, rs1: 1, rs2: 2 }).unwrap();
        assert_eq!(processor.registers[3], 1);
    }

    #[test]
    fn test_execute_slt_equal() {
        let mut processor = Processor::new(0, 0, 0, 0);
        // x1 == x2 → rd = 0 (strictly less than)
        processor.registers[1] = 5;
        processor.registers[2] = 5;
        processor.execute(Instruction::Slt { rd: 3, rs1: 1, rs2: 2 }).unwrap();
        assert_eq!(processor.registers[3], 0);
    }

    #[test]
    fn test_execute_sltu_signed_vs_unsigned() {
        let mut processor = Processor::new(0, 0, 0, 0);
        // x1 = 0xFFFFFFFF, x2 = 1
        // unsigned: 0xFFFFFFFF > 1 → rd = 0  (opposite of slt!)
        processor.registers[1] = 0xFFFFFFFF;
        processor.registers[2] = 1;
        processor.execute(Instruction::Sltu { rd: 3, rs1: 1, rs2: 2 }).unwrap();
        assert_eq!(processor.registers[3], 0);
    }

    #[test]
    fn test_execute_sltu_positive() {
        let mut processor = Processor::new(0, 0, 0, 0);
        // x1 = 1, x2 = 0xFFFFFFFF
        // unsigned: 1 < 0xFFFFFFFF → rd = 1
        processor.registers[1] = 1;
        processor.registers[2] = 0xFFFFFFFF;
        processor.execute(Instruction::Sltu { rd: 3, rs1: 1, rs2: 2 }).unwrap();
        assert_eq!(processor.registers[3], 1);
    }

    fn processor_with_data(data: Vec<u8>) -> Processor {
        let mut p = Processor::new(0x0, 0x10000000, 0x7FFFFFFF, 1024);
        p.memory.data = data;
        p
    }

    #[test]
    fn test_lb_sign_extends_negative() {
        let mut p = processor_with_data(vec![0xFF]);
        p.write_register(1, 0x10000000);  // rs1 = data_base
        p.execute(Instruction::Lb { rd: 2, rs1: 1, imm: 0 }).unwrap();
        // 0xFF as i8 = -1, sign extended to u32 = 0xFFFFFFFF
        assert_eq!(p.read_register(2), 0xFFFFFFFF);
    }

    #[test]
    fn test_lbu_zero_extends() {
        let mut p = processor_with_data(vec![0xFF]);
        p.write_register(1, 0x10000000);
        p.execute(Instruction::Lbu { rd: 2, rs1: 1, imm: 0 }).unwrap();
        // 0xFF zero extended = 0x000000FF
        assert_eq!(p.read_register(2), 0x000000FF);
    }

    #[test]
    fn test_load_with_negative_offset() {
        let mut p = processor_with_data(vec![0x42, 0x00]);
        // point rs1 past the first byte, use imm=-1 to reach it
        p.write_register(1, 0x10000001);
        p.execute(Instruction::Lb { rd: 2, rs1: 1, imm: -1 }).unwrap();
        assert_eq!(p.read_register(2), 0x42);
    }

    #[test]
    fn test_load_out_of_bounds_returns_fault() {
        let mut p = processor_with_data(vec![0x00]);
        p.write_register(1, 0x20000000); // unmapped address
        let result = p.execute(Instruction::Lw { rd: 2, rs1: 1, imm: 0 });
        assert!(matches!(result, Err(StepError::MemoryFault(MemoryFault::OutOfBounds { address: 0x20000000 }))));
    }

    #[test]
    fn test_store_with_negative_offset() {
        let mut p = processor_with_data(vec![0x00]);
        p.write_register(1, 0x10000001); // point rs1 past the first byte
        p.write_register(2, 0x42);
        p.execute(Instruction::Sb { rs1: 1, rs2: 2, imm: -1 }).unwrap();
        assert_eq!(p.memory.data[0], 0x42);
    }

    #[test]
    fn test_store_out_of_bounds_returns_fault() {
        let mut p = processor_with_data(vec![0x00]);
        p.write_register(1, 0x20000000); // unmapped address
        let result = p.execute(Instruction::Sb { rs1: 1, rs2: 2, imm: 0 });
        assert!(matches!(result, Err(StepError::MemoryFault(MemoryFault::OutOfBounds { address: 0x20000000 }))));
    }

    #[test]
    fn test_blt_signed_taken() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.write_register(1, 0xFFFFFFFF); // -1 signed
        p.write_register(2, 1);
        p.pc = 0;
        p.execute(Instruction::Blt { rs1: 1, rs2: 2, imm: 8 }).unwrap();
        assert_eq!(p.pc, 8); // branch taken, -1 < 1
    }

    #[test]
    fn test_bltu_signed_not_taken() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.write_register(1, 0xFFFFFFFF); // largest unsigned
        p.write_register(2, 1);
        p.pc = 0;
        p.execute(Instruction::Bltu { rs1: 1, rs2: 2, imm: 8 }).unwrap();
        assert_eq!(p.pc, 4); // branch NOT taken, 0xFFFFFFFF > 1 unsigned
    }

    #[test]
    fn test_jal_saves_return_address_and_jumps() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.pc = 0x100;
        p.execute(Instruction::Jal { rd: 1, imm: 16 }).unwrap();
        assert_eq!(p.read_register(1), 0x104); // return address = PC+4
        assert_eq!(p.pc, 0x110);               // PC = old PC + imm
    }

    #[test]
    fn test_jal_negative_offset() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.pc = 0x100;
        p.execute(Instruction::Jal { rd: 1, imm: -4 }).unwrap();
        assert_eq!(p.read_register(1), 0x104);
        assert_eq!(p.pc, 0xFC);
    }

    #[test]
    fn test_jalr_saves_return_address_and_jumps() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.pc = 0x100;
        p.write_register(2, 0x200);
        p.execute(Instruction::Jalr { rd: 1, rs1: 2, imm: 4 }).unwrap();
        assert_eq!(p.read_register(1), 0x104); // return address = PC+4
        assert_eq!(p.pc, 0x204);              // PC = rs1 + imm
    }

    #[test]
    fn test_jalr_clears_lsb() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.pc = 0x100;
        p.write_register(2, 0x200);
        p.execute(Instruction::Jalr { rd: 1, rs1: 2, imm: 1 }).unwrap(); // rs1 + imm = 0x201
        assert_eq!(p.pc, 0x200); // LSB cleared → 0x200
    }

    #[test]
    fn test_lui_loads_upper_immediate() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.execute(Instruction::Lui { rd: 1, imm: 0x12345000 }).unwrap();
        assert_eq!(p.read_register(1), 0x12345000);
    }

    #[test]
    fn test_lui_lower_bits_are_zero() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.execute(Instruction::Lui { rd: 1, imm: 0x12345000 }).unwrap();
        // lower 12 bits must always be zero
        assert_eq!(p.read_register(1) & 0xFFF, 0);
    }

    #[test]
    fn test_lui_ignores_pc() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.pc = 0x100;
        p.execute(Instruction::Lui { rd: 1, imm: 0x12345000 }).unwrap();
        // LUI does not involve PC at all
        assert_eq!(p.read_register(1), 0x12345000);
    }

    #[test]
    fn test_auipc_adds_pc() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.pc = 0x100;
        p.execute(Instruction::Auipc { rd: 1, imm: 0x12345000 }).unwrap();
        assert_eq!(p.read_register(1), 0x12345100); // PC + imm
    }

    #[test]
    fn test_auipc_at_pc_zero() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.pc = 0x0;
        p.execute(Instruction::Auipc { rd: 1, imm: 0x12345000 }).unwrap();
        // when PC=0, result is just imm
        assert_eq!(p.read_register(1), 0x12345000);
    }
}
