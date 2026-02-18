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

    pub fn show_state(&self) {
        println!("PC: {}", self.pc);
        println!("Registers: {:?}", self.registers);
    }

}
