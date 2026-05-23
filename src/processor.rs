use std::sync::{Arc, Mutex};
use crate::bus::{Bus, Ram, Rom, MmioDevice, Clint, ClintState, Uart, AccessSize, MemoryFault};
use crate::elf_loader::ElfImage;

// CSR addresses as specified by the RISC-V M-mode privileged ISA.
const MSTATUS:  u32 = 0x300;
const MIE_CSR:  u32 = 0x304;
const MTVEC:    u32 = 0x305;
const MSCRATCH: u32 = 0x340;
const MEPC:     u32 = 0x341;
const MCAUSE:   u32 = 0x342;
const MIP:      u32 = 0x344;

struct CsrFile {
    mstatus:  u32,
    mtvec:    u32,
    mscratch: u32,
    mepc:     u32,
    mcause:   u32,
    // mie: interrupt enable mask (bit 7 = MTIE: machine timer interrupt enable)
    mie:      u32,
    // mip: interrupt pending; MTIP (bit 7) is set/cleared by CLINT hardware, not CSR writes
    mip:      u32,
}

impl CsrFile {
    fn new() -> Self {
        Self { mstatus: 0, mtvec: 0, mscratch: 0, mepc: 0, mcause: 0, mie: 0, mip: 0 }
    }

    fn read(&self, addr: u32) -> u32 {
        match addr {
            MSTATUS  => self.mstatus,
            MIE_CSR  => self.mie,
            MTVEC    => self.mtvec,
            MSCRATCH => self.mscratch,
            MEPC     => self.mepc,
            MCAUSE   => self.mcause,
            MIP      => self.mip,
            _        => 0,
        }
    }

    // Apply a CSR instruction write. func3 encodes the operation:
    //   1/5 = CSRRW/CSRRWI: replace
    //   2/6 = CSRRS/CSRRSI: set bits
    //   3/7 = CSRRC/CSRRCI: clear bits
    // For RS/RC, write_val == 0 is a no-op (pure read, no side effects).
    fn write(&mut self, addr: u32, func3: u32, write_val: u32) {
        let old = self.read(addr);
        let (new_val, do_write) = match func3 {
            1 | 5 => (write_val,        true),
            2 | 6 => (old | write_val,  write_val != 0),
            3 | 7 => (old & !write_val, write_val != 0),
            _     => (write_val,        false),
        };
        if !do_write { return; }
        match addr {
            MSTATUS  => self.mstatus  = new_val,
            MIE_CSR  => self.mie      = new_val,
            MTVEC    => self.mtvec    = new_val,
            MSCRATCH => self.mscratch = new_val,
            MEPC     => self.mepc     = new_val,
            MCAUSE   => self.mcause   = new_val,
            // mip is hardware-driven; software writes are silently ignored.
            _ => {},
        }
    }
}

pub struct Processor {
    pc: u32,
    registers: [u32; crate::config::NUM_REGISTERS],
    bus: Bus,
    // Store bases for convenience/test compatibility
    text_base: u32,
    data_base: u32,
    stack_base: u32,
    stack_size: usize,
    // All M-mode CSR registers
    csrs: CsrFile,
    // Shared with the Clint bus device so the processor can increment mtime and read
    // mtimecmp without going through the bus on every step.
    clint_state: Arc<Mutex<ClintState>>,
    // UART output buffer shared with the Uart bus device
    uart_output: Arc<Mutex<Vec<u8>>>,
}

#[derive(Debug, PartialEq)]
pub enum StepError {
    IllegalInstruction { pc: u32, word: u32 },
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
    // Zicsr: CSR access; func3 distinguishes RW/RS/RC and register vs immediate variants
    Csr { rd: usize, csr_addr: u32, write_val: u32, func3: u32 },
    Mret,
    // Fence / Fence.I: no-op in a simple in-order emulator
    Fence,
}

fn register_platform_devices(
    bus: &mut Bus,
    clint_state: Arc<Mutex<ClintState>>,
    uart_output: Arc<Mutex<Vec<u8>>>,
) {
    bus.add_device(crate::config::CLINT_BASE, crate::config::CLINT_SIZE, Box::new(Clint::new(clint_state)));
    bus.add_device(crate::config::PLIC_BASE, crate::config::PLIC_SIZE, Box::new(MmioDevice::new("PLIC")));
    bus.add_device(crate::config::UART_BASE, crate::config::UART_SIZE, Box::new(Uart::new(uart_output)));
}

impl Processor {
    pub fn new(text_base: u32, data_base: u32, stack_base: u32, stack_size: usize) -> Self {
        let mut registers = [0; crate::config::NUM_REGISTERS];
        registers[2] = stack_base;

        let mut bus = Bus::new();
        let clint_state = Arc::new(Mutex::new(ClintState { mtime: 0, mtimecmp: u64::MAX }));
        let uart_output = Arc::new(Mutex::new(Vec::new()));
        register_platform_devices(&mut bus, Arc::clone(&clint_state), Arc::clone(&uart_output));

        // Initial regions for backward compatibility with existing tests
        // and current assembler/loader expectations.
        // These will eventually be merged into a single DRAM device.
        bus.add_device(text_base, crate::config::DEFAULT_SEGMENT_SIZE, Box::new(Rom::new(Vec::new()))); // Placeholder text
        bus.add_device(data_base, crate::config::DEFAULT_SEGMENT_SIZE, Box::new(Ram::new(crate::config::DEFAULT_SEGMENT_SIZE as usize))); // Placeholder data

        // Stack grows downward, but we map it from (stack_base - stack_size) to stack_base
        let stack_start = stack_base.wrapping_sub(stack_size as u32);
        bus.add_device(stack_start, stack_size as u32, Box::new(Ram::new(stack_size)));

        Processor {
            pc: text_base,
            registers,
            bus,
            text_base,
            data_base,
            stack_base,
            stack_size,
            csrs: CsrFile::new(),
            clint_state,
            uart_output,
        }
    }

    pub fn from_elf(image: &ElfImage) -> Self {
        let stack_base = crate::config::STACK_BASE;
        let stack_size = crate::config::STACK_SIZE;

        let mut registers = [0; crate::config::NUM_REGISTERS];
        registers[2] = stack_base; // sp

        let mut bus = Bus::new();
        let clint_state = Arc::new(Mutex::new(ClintState { mtime: 0, mtimecmp: u64::MAX }));
        let uart_output = Arc::new(Mutex::new(Vec::new()));
        register_platform_devices(&mut bus, Arc::clone(&clint_state), Arc::clone(&uart_output));

        // Map every ELF PT_LOAD segment at its VMA (runtime address).
        // If paddr != vaddr (ROM→RAM layout), also map the file bytes at paddr
        // so startup copy code (e.g. FreeRTOS start.S) can read from there.
        let mut sorted_segs: Vec<_> = image.segments.iter().collect();
        sorted_segs.sort_by_key(|s| s.vaddr);

        for seg in &sorted_segs {
            let mut ram = Ram::new(seg.data.len());
            ram.data.copy_from_slice(&seg.data);
            bus.add_device(seg.vaddr, seg.data.len() as u32, Box::new(ram));

            if seg.paddr != seg.vaddr && seg.filesz > 0 {
                let mut lma_ram = Ram::new(seg.filesz);
                lma_ram.data.copy_from_slice(&seg.data[..seg.filesz]);
                bus.add_device(seg.paddr, seg.filesz as u32, Box::new(lma_ram));
            }
        }

        // Fill small gaps between consecutive segments with zeroed RAM. Linker
        // alignment can leave a few unmapped bytes between sections that startup
        // code still accesses (e.g. BSS clear loop starting before the BSS segment).
        for pair in sorted_segs.windows(2) {
            let gap_start = pair[0].vaddr + pair[0].data.len() as u32;
            let gap_end   = pair[1].vaddr;
            if gap_start < gap_end && (gap_end - gap_start) < 0x10000 {
                let sz = (gap_end - gap_start) as usize;
                bus.add_device(gap_start, sz as u32, Box::new(Ram::new(sz)));
            }
        }

        let stack_start = stack_base.wrapping_sub(stack_size as u32);
        bus.add_device(stack_start, stack_size as u32, Box::new(Ram::new(stack_size)));

        Processor {
            pc: image.entry_point,
            registers,
            bus,
            text_base: image.entry_point,
            data_base: 0,
            stack_base,
            stack_size,
            csrs: CsrFile::new(),
            clint_state,
            uart_output,
        }
    }

    pub fn load(&mut self, text: &[u8], data: &[u8]) {
        self.bus.replace_device(self.text_base, text.len() as u32, Box::new(Rom::new(text.to_vec())));

        let mut ram = Ram::new(data.len());
        ram.data.copy_from_slice(data);
        self.bus.replace_device(self.data_base, data.len() as u32, Box::new(ram));

        self.pc = self.text_base;
    }

    pub fn reset(&mut self) {
        self.pc = self.text_base;
        self.registers = [0; crate::config::NUM_REGISTERS];
        self.registers[2] = self.stack_base;
    }

    pub fn step(&mut self) -> Result<(), StepError> {
        self.tick_timer();
        if self.check_and_deliver_interrupt() {
            return Ok(());
        }
        let word = self.fetch()?;
        let instruction = self.decode(word)
            .map_err(|_| StepError::IllegalInstruction { pc: self.pc, word })?;
        self.execute(instruction)?;
        Ok(())
    }

    // Advance mtime by one tick and recompute MTIP.
    // MTIP is level-triggered: it stays set as long as mtime >= mtimecmp.
    // The OS clears it by writing a new future value to mtimecmp.
    fn tick_timer(&mut self) {
        let mut clint = self.clint_state.lock().unwrap();
        clint.mtime = clint.mtime.wrapping_add(1);
        if clint.mtime >= clint.mtimecmp {
            self.csrs.mip |= 1 << 7;   // set   MTIP
        } else {
            self.csrs.mip &= !(1 << 7); // clear MTIP
        }
    }

    // Deliver a pending machine timer interrupt if all three gates are open:
    // mstatus.MIE (global enable), mie.MTIE (timer enable), mip.MTIP (pending).
    // Returns true if an interrupt was taken (step should skip the normal fetch/decode/execute).
    fn check_and_deliver_interrupt(&mut self) -> bool {
        let globally_enabled = (self.csrs.mstatus >> 3) & 1 == 1;
        let timer_enabled    = (self.csrs.mie     >> 7) & 1 == 1;
        let timer_pending    = (self.csrs.mip     >> 7) & 1 == 1;

        if !(globally_enabled && timer_enabled && timer_pending) {
            return false;
        }

        self.csrs.mepc   = self.pc;
        self.csrs.mcause = 0x8000_0007; // bit 31 = interrupt, cause 7 = machine timer
        let mie_bit = (self.csrs.mstatus >> 3) & 1;
        self.csrs.mstatus = (self.csrs.mstatus & !(1 << 7)) | (mie_bit << 7); // MPIE = MIE
        self.csrs.mstatus &= !(1 << 3);                                   // MIE  = 0
        self.pc = self.csrs.mtvec & !3; // direct mode: jump to BASE
        true
    }

    fn fetch(&self) -> Result<u32, StepError> {
        self.bus.read(self.pc, AccessSize::Word).map_err(|e| StepError::MemoryFault(e))
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
            0b1110011 => self.decode_system_type(memory_instruction), // ecall, ebreak, csr
            0b0001111 => Ok(Instruction::Fence), // fence, fence.i
            _ => Err(StepError::IllegalInstruction { pc: 0, word: 0 }),
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
            _ => Err(StepError::IllegalInstruction { pc: 0, word: 0 }),
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
            _ => Err(StepError::IllegalInstruction { pc: 0, word: 0 }),
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
            _ => Err(StepError::IllegalInstruction { pc: 0, word: 0 }),
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
            _ => Err(StepError::IllegalInstruction { pc: 0, word: 0 }),
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
            _ => Err(StepError::IllegalInstruction { pc: 0, word: 0 }),
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
            _ => Err(StepError::IllegalInstruction { pc: 0, word: 0 }),
        }
    }

    fn decode_u_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let rd = ((memory_instruction >> 7) & 0x1F) as usize;
        let imm = (memory_instruction & 0xFFFFF000) as i32;

        let opcode = memory_instruction & 0x7F;

        match opcode {
            0x37 => Ok(Instruction::Lui { rd, imm }),
            0x17 => Ok(Instruction::Auipc { rd, imm }),
            _ => Err(StepError::IllegalInstruction { pc: 0, word: 0 }),
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
            return Err(StepError::IllegalInstruction { pc: 0, word: 0 });
        }

        Ok(Instruction::Jalr { rd, rs1, imm })
    }

    fn decode_system_type(&self, memory_instruction: u32) -> Result<Instruction, StepError> {
        let func3 = (memory_instruction >> 12) & 0x7;

        if func3 != 0 {
            let rd       = ((memory_instruction >> 7)  & 0x1F) as usize;
            let csr_addr =  (memory_instruction >> 20) & 0xFFF;
            let rs1      = ((memory_instruction >> 15) & 0x1F) as usize;
            // func3 1-3: register source; 5-7: 5-bit unsigned immediate in rs1 field
            let write_val = if func3 >= 5 {
                rs1 as u32  // CSRRWI/CSRRSI/CSRRCI use uimm
            } else {
                self.read_register(rs1)
            };
            return Ok(Instruction::Csr { rd, csr_addr, write_val, func3 });
        }

        let imm = (memory_instruction >> 20) & 0xFFF;
        match imm {
            0x000 => Ok(Instruction::Ecall),
            0x001 => Ok(Instruction::Ebreak),
            0x302 => Ok(Instruction::Mret),
            // sret, wfi, and other privileged instructions — no-op
            _ => Ok(Instruction::Csr { rd: 0, csr_addr: 0, write_val: 0, func3: 0 }),
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
                let value = self.bus.read(address, AccessSize::Byte)?;
                self.write_register(rd, value as i8 as u32);
            },
            Instruction::Lh { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:15] (sign extended)
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.bus.read(address, AccessSize::Half)?;
                self.write_register(rd, value as i16 as u32);
            },
            Instruction::Lw { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:31]
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.bus.read(address, AccessSize::Word)?;
                self.write_register(rd, value);
            },
            Instruction::Lbu { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:7] (zero extended)
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.bus.read(address, AccessSize::Byte)?;
                self.write_register(rd, value as u32);
            },
            Instruction::Lhu { rd, rs1, imm } => {
                // rd = M[rs1+imm][0:15] (zero extended)
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                let value = self.bus.read(address, AccessSize::Half)?;
                self.write_register(rd, value as u32);
            },
            Instruction::Sb { rs1, rs2, imm } => {
                // M[rs1+imm][0:7] = rs2[0:7]
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                self.bus.write(address, self.read_register(rs2), AccessSize::Byte)?;
            },
            Instruction::Sh { rs1, rs2, imm } => {
                // M[rs1+imm][0:15] = rs2[0:15]
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                self.bus.write(address, self.read_register(rs2), AccessSize::Half)?;
            },
            Instruction::Sw { rs1, rs2, imm } => {
                // M[rs1+imm][0:31] = rs2[0:31]
                let address = self.read_register(rs1).wrapping_add(imm as u32);
                self.bus.write(address, self.read_register(rs2), AccessSize::Word)?;
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
                // Read rs1 before writing rd — handles the rd==rs1 case correctly
                let target = self.read_register(rs1).wrapping_add(imm as u32) & !1;
                self.write_register(rd, self.pc.wrapping_add(4));
                next_pc = target;
            },
            Instruction::Lui { rd, imm } => {
                // rd = upper imm (upper mask already applied by the decoder)
                self.write_register(rd, imm as u32);
            },
            Instruction::Auipc { rd, imm } => {
                // rd = PC + upper imm (upper mask already applied by the decoder)
                self.write_register(rd, self.pc.wrapping_add(imm as u32));
            },
            Instruction::Ebreak => return Err(StepError::Ebreak),
            Instruction::Csr { rd, csr_addr, write_val, func3 } => {
                let read_val = self.csrs.read(csr_addr);
                self.write_register(rd, read_val);
                self.csrs.write(csr_addr, func3, write_val);
            },
            Instruction::Fence => {
                // No-op: single-core in-order emulator has no reordering to fence.
            },
            Instruction::Ecall => {
                // Save PC of the ecall instruction so the handler can return past it
                // by incrementing mepc before mret.
                self.csrs.mepc   = self.pc;
                self.csrs.mcause = 11; // Environment call from M-mode
                // Save MIE into MPIE, then disable interrupts (MIE = 0).
                let mie = (self.csrs.mstatus >> 3) & 1;
                self.csrs.mstatus = (self.csrs.mstatus & !(1 << 7)) | (mie << 7); // MPIE = MIE
                self.csrs.mstatus &= !(1 << 3);                               // MIE  = 0
                // mtvec direct mode: mask off the two mode bits before jumping.
                next_pc = self.csrs.mtvec & !3;
            },
            Instruction::Mret => {
                // Restore MIE from MPIE, then set MPIE = 1 (spec default after mret).
                let mpie = (self.csrs.mstatus >> 7) & 1;
                self.csrs.mstatus = (self.csrs.mstatus & !(1 << 3)) | (mpie << 3); // MIE  = MPIE
                self.csrs.mstatus |= 1 << 7;                                   // MPIE = 1
                next_pc = self.csrs.mepc;
            },
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

    pub fn pc(&self) -> u32 {
        self.pc
    }

    pub fn registers(&self) -> &[u32; crate::config::NUM_REGISTERS] {
        &self.registers
    }

    pub fn read_memory_word(&self, address: u32) -> Result<u32, MemoryFault> {
        self.bus.read_word(address)
    }

    pub fn text_base(&self) -> u32 {
        self.text_base
    }

    pub fn data_base(&self) -> u32 {
        self.data_base
    }

    pub fn stack_base(&self) -> u32 {
        self.stack_base
    }

    pub fn stack_size(&self) -> usize {
        self.stack_size
    }

    /// Drain all bytes written to the UART since the last call.
    pub fn drain_uart(&mut self) -> Vec<u8> {
        std::mem::take(&mut *self.uart_output.lock().unwrap())
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
        assert!(matches!(result, Err(StepError::IllegalInstruction { .. })));
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
        processor.load(&[0xB3, 0x81, 0x20, 0x00], &[]);
        processor.pc = 0x400000;

        processor.step().unwrap();
        assert_eq!(processor.pc, 0x400000 + 4);
    }

    #[test]
    fn test_execute_slt_not_taken() {
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
        p.load(&[], &data);
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
        p.write_register(1, 0x10000001); // point rs1 one byte past data_base
        p.write_register(2, 0x42);
        p.execute(Instruction::Sb { rs1: 1, rs2: 2, imm: -1 }).unwrap();
        // Read back through execute to stay at the public API and exercise the load path
        p.execute(Instruction::Lb { rd: 3, rs1: 1, imm: -1 }).unwrap();
        assert_eq!(p.read_register(3), 0x42);
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
    fn test_bltu_not_taken_when_unsigned_larger() {
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
    fn test_lui_ignores_pc() {
        // With pc=0x100 and imm=0x12345000, AUIPC would produce 0x12345100.
        // LUI must produce 0x12345000, proving it does not add PC.
        let mut p = Processor::new(0, 0, 0, 0);
        p.pc = 0x100;
        p.execute(Instruction::Lui { rd: 1, imm: 0x12345000 }).unwrap();
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
    // ---- CLINT timer (Step 3) ----

    #[test]
    fn test_mtime_increments_each_step() {
        let mut p = Processor::new(0x1000, 0x2000, 0x7FFF_FFF0, 1024);
        // load a nop (addi x0, x0, 0) so step() doesn't fault
        p.load(&[0x13, 0x00, 0x00, 0x00], &[]);
        let before = p.clint_state.lock().unwrap().mtime;
        p.step().unwrap();
        let after = p.clint_state.lock().unwrap().mtime;
        assert_eq!(after, before + 1);
    }

    #[test]
    fn test_timer_interrupt_fires_when_enabled() {
        let mut p = Processor::new(0x1000, 0x2000, 0x7FFF_FFF0, 1024);
        p.load(&[0x13, 0x00, 0x00, 0x00], &[]); // nop at 0x1000
        p.csrs.mtvec   = 0x2000;
        p.csrs.mstatus = 1 << 3; // MIE = 1
        p.csrs.mie     = 1 << 7; // MTIE = 1
        // Fire immediately: mtimecmp = 0 means mtime (which starts at 0 and becomes 1) >= 0
        p.clint_state.lock().unwrap().mtimecmp = 0;
        p.step().unwrap();
        assert_eq!(p.pc,     0x2000);        // jumped to trap handler
        assert_eq!(p.csrs.mcause, 0x8000_0007);   // timer interrupt
        assert_eq!(p.csrs.mepc,   0x1000);        // saved PC of interrupted instruction
        assert_eq!((p.csrs.mstatus >> 3) & 1, 0); // MIE cleared
    }

    #[test]
    fn test_timer_interrupt_blocked_when_mie_clear() {
        let mut p = Processor::new(0x1000, 0x2000, 0x7FFF_FFF0, 1024);
        p.load(&[0x13, 0x00, 0x00, 0x00], &[]); // nop
        p.csrs.mtvec   = 0x2000;
        p.csrs.mstatus = 0;      // MIE = 0 — interrupts globally disabled
        p.csrs.mie     = 1 << 7; // MTIE = 1
        p.clint_state.lock().unwrap().mtimecmp = 0;
        p.step().unwrap();
        assert_eq!(p.pc, 0x1004); // no interrupt — executed the nop normally
    }

    #[test]
    fn test_mtip_clears_when_mtimecmp_advanced() {
        let mut p = Processor::new(0x1000, 0x2000, 0x7FFF_FFF0, 1024);
        // Two nops so the second step can fetch from 0x1004.
        p.load(&[0x13, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00], &[]);
        // No interrupt enable so we can observe mip without being redirected.
        p.clint_state.lock().unwrap().mtimecmp = 0; // fires immediately
        p.step().unwrap();
        assert_eq!((p.csrs.mip >> 7) & 1, 1); // MTIP set
        // Advance mtimecmp far into the future
        p.clint_state.lock().unwrap().mtimecmp = u64::MAX;
        p.step().unwrap();
        assert_eq!((p.csrs.mip >> 7) & 1, 0); // MTIP cleared
    }

    #[test]
    fn test_clint_mtimecmp_readable_via_bus() {
        let p = Processor::new(0x1000, 0x2000, 0x7FFF_FFF0, 1024);
        p.clint_state.lock().unwrap().mtimecmp = 0xDEAD_BEEF_1234_5678;
        let lo = p.bus.read(crate::config::CLINT_BASE + 0x4000, AccessSize::Word).unwrap();
        let hi = p.bus.read(crate::config::CLINT_BASE + 0x4004, AccessSize::Word).unwrap();
        assert_eq!(lo, 0x1234_5678);
        assert_eq!(hi, 0xDEAD_BEEF);
    }

    // ---- UART (Step 2) ----

    #[test]
    fn test_uart_write_via_bus() {
        let mut p = Processor::new(0x1000, 0x2000, 0x7FFF_FFF0, 1024);
        // sw 'A' to UART base (0x1000_0000)
        p.write_register(1, crate::config::UART_BASE);
        p.write_register(2, b'A' as u32);
        p.execute(Instruction::Sb { rs1: 1, rs2: 2, imm: 0 }).unwrap();
        assert_eq!(p.drain_uart(), b"A");
    }

    #[test]
    fn test_uart_lsr_readable_via_bus() {
        let p = Processor::new(0x1000, 0x2000, 0x7FFF_FFF0, 1024);
        // Read LSR (offset 5 from UART base)
        let lsr = p.bus.read(crate::config::UART_BASE + 5, AccessSize::Byte).unwrap();
        assert_eq!(lsr, 0x60); // THRE + TEMT: TX always ready
    }

    #[test]
    fn test_drain_uart_clears_buffer() {
        let mut p = Processor::new(0x1000, 0x2000, 0x7FFF_FFF0, 1024);
        p.write_register(1, crate::config::UART_BASE);
        p.write_register(2, b'X' as u32);
        p.execute(Instruction::Sb { rs1: 1, rs2: 2, imm: 0 }).unwrap();
        let _ = p.drain_uart();
        assert!(p.drain_uart().is_empty()); // second drain is empty
    }

    // ---- M-mode trap save/restore (Step 1) ----

    #[test]
    fn test_ecall_saves_mepc_and_jumps_to_mtvec() {
        let mut p = Processor::new(0x1000, 0, 0, 0);
        p.csrs.mtvec = 0x2000;
        p.pc    = 0x1004;
        p.execute(Instruction::Ecall).unwrap();
        assert_eq!(p.csrs.mepc,   0x1004); // saved PC of the ecall
        assert_eq!(p.csrs.mcause, 11);     // environment call from M-mode
        assert_eq!(p.pc,     0x2000); // jumped to mtvec
    }

    #[test]
    fn test_ecall_clears_mie_and_saves_mpie() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.csrs.mstatus = 1 << 3; // MIE = 1
        p.execute(Instruction::Ecall).unwrap();
        assert_eq!((p.csrs.mstatus >> 3) & 1, 0); // MIE cleared
        assert_eq!((p.csrs.mstatus >> 7) & 1, 1); // MPIE = old MIE
    }

    #[test]
    fn test_mret_restores_mepc_and_mie() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.csrs.mepc    = 0x1008; // return address (ecall PC + 4, set by handler)
        p.csrs.mstatus = 1 << 7; // MPIE = 1, MIE = 0
        p.execute(Instruction::Mret).unwrap();
        assert_eq!(p.pc, 0x1008);             // jumped to mepc
        assert_eq!((p.csrs.mstatus >> 3) & 1, 1); // MIE restored from MPIE
        assert_eq!((p.csrs.mstatus >> 7) & 1, 1); // MPIE set to 1 after mret
    }

    #[test]
    fn test_csr_mscratch_roundtrip() {
        let mut p = Processor::new(0, 0, 0, 0);
        p.write_register(1, 0xDEAD_BEEF);
        // csrw mscratch, x1 — CSRRW rd=x0, csr=0x340
        p.execute(Instruction::Csr { rd: 0, csr_addr: 0x340, write_val: 0xDEAD_BEEF, func3: 1 }).unwrap();
        assert_eq!(p.csrs.mscratch, 0xDEAD_BEEF);
        // csrr x2, mscratch — CSRRS rd=x2, rs1=x0 (write_val=0, no write)
        p.execute(Instruction::Csr { rd: 2, csr_addr: 0x340, write_val: 0, func3: 2 }).unwrap();
        assert_eq!(p.read_register(2), 0xDEAD_BEEF);
    }

    #[test]
    fn test_csr_mstatus_set_and_clear_bits() {
        let mut p = Processor::new(0, 0, 0, 0);
        // csrsi mstatus, 0x8 — CSRRSI: set bit 3 (MIE) using uimm=8
        p.execute(Instruction::Csr { rd: 0, csr_addr: 0x300, write_val: 0x8, func3: 6 }).unwrap();
        assert_eq!((p.csrs.mstatus >> 3) & 1, 1); // MIE now set
        // csrci mstatus, 0x8 — CSRRCI: clear bit 3 (MIE)
        p.execute(Instruction::Csr { rd: 0, csr_addr: 0x300, write_val: 0x8, func3: 7 }).unwrap();
        assert_eq!((p.csrs.mstatus >> 3) & 1, 0); // MIE now cleared
    }

    #[test]
    fn test_trap_return_full_round_trip() {
        // Simulate: ecall → handler increments mepc → mret returns past ecall
        let mut p = Processor::new(0, 0, 0, 0);
        p.csrs.mtvec = 0x2000;
        p.pc    = 0x1000;
        p.execute(Instruction::Ecall).unwrap();
        assert_eq!(p.pc, 0x2000);
        // Handler: advance mepc past the ecall (mepc += 4)
        p.csrs.mepc += 4;
        p.execute(Instruction::Mret).unwrap();
        assert_eq!(p.pc, 0x1004); // returned past the ecall
    }

    #[test]
    fn test_processor_initializes_sp() {
        let text_base = 0x1000;
        let data_base = 0x2000;
        let stack_base = 0x7FFF_FFF0;
        let stack_size = 1024;
        let mut p = Processor::new(text_base, data_base, stack_base, stack_size);
        assert_eq!(p.registers[2], stack_base);

        p.registers[2] = 0x1234;
        p.reset();
        assert_eq!(p.registers[2], stack_base);
    }
}
