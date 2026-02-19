use crate::config;

// TODO: this is not a good way to represent memory, it should be a
// contiguous block of memory with different segments
struct Memory {
    text: Vec<u8>,
    data: Vec<u8>,
    stack: Vec<u8>,
    text_base: u32,
    data_base: u32,
    stack_base: u32,
}

pub struct Processor {
    pc: u32,
    registers: [u32; config::NUM_REGISTERS],
    memory: Memory,
}

#[derive(Debug)]
enum StepError {
    IllegalInstruction,
    MemoryFault,    // TODO which memory fault type?
    Ebreak,
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
        let bytes = self.memory.text.get(offset..offset + 4).ok_or(StepError::MemoryFault)?;

        // assemble 4 bytes into u32, assuming little endian
        let instruction = u32::from_le_bytes(bytes.try_into().unwrap());
        Ok(instruction)
    }

    fn decode(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let opcode = memory_instruction & 0x7F;

        // TODO other opcodes
        match opcode {
            0b0110011 => self.decode_r_type(memory_instruction),
            0b0010011 => self.decode_i_type(memory_instruction),
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
            (0x8, 0x00) => Ok(Instruction::And { rd, rs1, rs2 }),
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
        let imm = ((memory_instruction >> 20) & 0xFFF) as i32;

        let func3 = (memory_instruction >> 12) & 0x7;

        match func3 {
            0x0 => Ok(Instruction::Addi { rd, rs1, imm }),
            0x4 => Ok(Instruction::Xori { rd, rs1, imm }),
            0x6 => Ok(Instruction::Ori { rd, rs1, imm }),
            0x7 => Ok(Instruction::Andi { rd, rs1, imm }),
            0x1 => self.decode_i_shift(func3, rd, rs1, imm),
            0x5 => self.decode_i_shift(func3, rd, rs1, imm),
            0x2 => Ok(Instruction::Slti { rd, rs1, imm }),
            0x3 => Ok(Instruction::Sltiu { rd, rs1, imm }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn decode_i_shift(&self, func3: u32, rd: usize, rs1: usize, imm: i32) -> Result<Instruction, StepError> {
        let shamt = (imm & 0x1F) as u32;
        let func7 = (imm >> 5) & 0x7F; // imm[5:11]

        match (func3, func7) {
            (0x1, 0x0) => Ok(Instruction::Slli { rd, rs1, shamt }),
            (0x5, 0x0) => Ok(Instruction::Srli { rd, rs1, shamt }),
            (0x5, 0x20) => Ok(Instruction::Srai { rd, rs1, shamt }),
            _ => Err(StepError::IllegalInstruction),
        }
    }

    fn execute(&self, instruction: Instruction) -> Result<(), StepError> {
        // TODO
        Ok(())
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
}
