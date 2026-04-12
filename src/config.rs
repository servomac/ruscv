pub const NUM_REGISTERS: usize = 32;

// Text at 0x0001_0000 — closer to real ELF, still clear of address 0
pub const TEXT_BASE: u32 = 0x0001_0000;

// Data immediately after a reasonable text region (1MB for code)
// 0x0001_0000 + 0x0010_0000 = 0x0011_0000
pub const DATA_BASE: u32 = 0x0011_0000;

// Stack top near the top of a "safe" 32-bit user space region
// 0x7FFF_FFF0 — aligned to 16 bytes, per RISC-V ABI requirement
pub const STACK_BASE: u32 = 0x7FFF_FFF0;
pub const STACK_SIZE: usize = 1024 * 1024 * 8; // 8MB — more realistic

// Default size for memory segments if not specified
pub const DEFAULT_SEGMENT_SIZE: u32 = 0x0010_0000;

// QEMU Virt Memory Map
pub const ROM_BASE: u32 = 0x0000_1000;
pub const ROM_SIZE: u32 = 0x0000_1000;

pub const CLINT_BASE: u32 = 0x0200_0000;
pub const CLINT_SIZE: u32 = 0x0001_0000;

pub const PLIC_BASE: u32 = 0x0C00_0000;
pub const PLIC_SIZE: u32 = 0x0040_0000;

pub const UART_BASE: u32 = 0x1000_0000;
pub const UART_SIZE: u32 = 0x0000_0100;

pub const DRAM_BASE: u32 = 0x8000_0000;
