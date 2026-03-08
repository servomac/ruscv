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
