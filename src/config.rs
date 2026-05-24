pub const NUM_REGISTERS: usize = 32;

// DRAM starts at 0x8000_0000 in QEMU virt machine
pub const DRAM_BASE: u32 = 0x8000_0000;

// Text at the beginning of DRAM
pub const TEXT_BASE: u32 = DRAM_BASE;

// Data after 1MB of text space
pub const DATA_BASE: u32 = DRAM_BASE + 0x0010_0000;

// Flat DRAM region: text, data, heap and stack all live here.
// Stack pointer initialises to DRAM_BASE + DRAM_SIZE and grows downward.
pub const DRAM_SIZE: u32 = 0x0400_0000; // 64 MB

// QEMU Virt Memory Map
pub const CLINT_BASE: u32 = 0x0200_0000;
pub const CLINT_SIZE: u32 = 0x0001_0000;

pub const PLIC_BASE: u32 = 0x0C00_0000;
pub const PLIC_SIZE: u32 = 0x0040_0000;

pub const UART_BASE: u32 = 0x1000_0000;
pub const UART_SIZE: u32 = 0x0000_0100;
