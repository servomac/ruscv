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
}
