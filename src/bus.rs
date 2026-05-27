#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MemoryFault {
    OutOfBounds { address: u32 },
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AccessSize {
    Byte,
    Half,
    Word,
}

pub trait Device: Send + Sync {
    fn read(&self, addr: u32, size: AccessSize) -> Result<u32, MemoryFault>;
    fn write(&mut self, addr: u32, val: u32, size: AccessSize) -> Result<(), MemoryFault>;
}

pub struct Bus {
    regions: Vec<(u32, u32, Box<dyn Device>)>,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
        }
    }

    pub fn add_device(&mut self, base: u32, size: u32, device: Box<dyn Device>) {
        let end = base.wrapping_add(size);
        self.regions.push((base, end, device));
    }

    pub fn read(&self, addr: u32, size: AccessSize) -> Result<u32, MemoryFault> {
        for (base, end, device) in &self.regions {
            if addr >= *base && addr < *end {
                return device.read(addr - *base, size);
            }
        }
        Err(MemoryFault::OutOfBounds { address: addr })
    }

    pub fn write(&mut self, addr: u32, val: u32, size: AccessSize) -> Result<(), MemoryFault> {
        for (base, end, device) in &mut self.regions {
            if addr >= *base && addr < *end {
                return device.write(addr - *base, val, size);
            }
        }
        Err(MemoryFault::OutOfBounds { address: addr })
    }

    pub fn read_word(&self, addr: u32) -> Result<u32, MemoryFault> {
        self.read(addr, AccessSize::Word)
    }

    fn remove_region(&mut self, base_addr: u32) {
        self.regions.retain(|(base, _, _)| *base != base_addr);
    }

    pub fn replace_device(&mut self, base: u32, size: u32, device: Box<dyn Device>) {
        self.remove_region(base);
        self.add_device(base, size, device);
    }
}

fn read_bytes(data: &[u8], addr: u32, size: AccessSize) -> Result<u32, MemoryFault> {
    let addr = addr as usize;
    match size {
        AccessSize::Byte => data
            .get(addr)
            .map(|&b| b as u32)
            .ok_or(MemoryFault::OutOfBounds {
                address: addr as u32,
            }),
        AccessSize::Half => {
            let bytes = data.get(addr..addr + 2).ok_or(MemoryFault::OutOfBounds {
                address: addr as u32,
            })?;
            Ok(u32::from_le_bytes([bytes[0], bytes[1], 0, 0]))
        }
        AccessSize::Word => {
            let bytes = data.get(addr..addr + 4).ok_or(MemoryFault::OutOfBounds {
                address: addr as u32,
            })?;
            Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
        }
    }
}

// RAM Device Implementation
pub struct Ram {
    pub data: Vec<u8>,
}

impl Ram {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
        }
    }
}

impl Device for Ram {
    fn read(&self, addr: u32, size: AccessSize) -> Result<u32, MemoryFault> {
        read_bytes(&self.data, addr, size)
    }

    fn write(&mut self, addr: u32, val: u32, size: AccessSize) -> Result<(), MemoryFault> {
        let addr = addr as usize;
        let bytes = val.to_le_bytes();
        let n = match size {
            AccessSize::Byte => 1,
            AccessSize::Half => 2,
            AccessSize::Word => 4,
        };
        let slot = self
            .data
            .get_mut(addr..addr + n)
            .ok_or(MemoryFault::OutOfBounds {
                address: addr as u32,
            })?;
        slot.copy_from_slice(&bytes[..n]);
        Ok(())
    }
}

// Placeholder for MMIO devices
pub struct MmioDevice;

impl Device for MmioDevice {
    fn read(&self, _addr: u32, _size: AccessSize) -> Result<u32, MemoryFault> {
        Ok(0)
    }

    fn write(&mut self, _addr: u32, _val: u32, _size: AccessSize) -> Result<(), MemoryFault> {
        Ok(())
    }
}

// CLINT — Core Local Interruptor
//
// The CLINT is a standard RISC-V peripheral that provides two things:
//   • mtime    — a 64-bit free-running counter (read-only from software in practice,
//                though technically writable; the processor increments it each step)
//   • mtimecmp — a 64-bit compare register (writable by software); when
//                mtime >= mtimecmp the CLINT asserts the machine timer interrupt (MTIP)
//
// Standard memory map (single hart, offsets from CLINT base):
//   0x0000  MSIP      — machine software interrupt pending (unused here)
//   0x4000  mtimecmp low word
//   0x4004  mtimecmp high word
//   0xBFF8  mtime low word
//   0xBFFC  mtime high word
//
// The state is shared with the Processor via Arc<Mutex<>> so the processor can
// increment mtime and read mtimecmp without going through the bus.
pub struct ClintState {
    pub mtime: u64,
    pub mtimecmp: u64,
}

pub struct Clint {
    state: std::sync::Arc<std::sync::Mutex<ClintState>>,
}

impl Clint {
    pub fn new(state: std::sync::Arc<std::sync::Mutex<ClintState>>) -> Self {
        Self { state }
    }
}

impl Device for Clint {
    fn read(&self, addr: u32, _size: AccessSize) -> Result<u32, MemoryFault> {
        let s = self.state.lock().unwrap();
        match addr {
            0x4000 => Ok(s.mtimecmp as u32),
            0x4004 => Ok((s.mtimecmp >> 32) as u32),
            0xBFF8 => Ok(s.mtime as u32),
            0xBFFC => Ok((s.mtime >> 32) as u32),
            _ => Ok(0),
        }
    }

    fn write(&mut self, addr: u32, val: u32, _size: AccessSize) -> Result<(), MemoryFault> {
        let mut s = self.state.lock().unwrap();
        match addr {
            0x4000 => s.mtimecmp = (s.mtimecmp & 0xFFFF_FFFF_0000_0000) | val as u64,
            0x4004 => s.mtimecmp = (s.mtimecmp & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32),
            0xBFF8 => s.mtime = (s.mtime & 0xFFFF_FFFF_0000_0000) | val as u64,
            0xBFFC => s.mtime = (s.mtime & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32),
            _ => {}
        }
        Ok(())
    }
}

// NS16550-compatible UART — the National Semiconductor NS16550 (1987) defined the
// register layout that became the PC serial port standard and was copied into most
// RISC-V boards. QEMU's "virt" machine maps one at 0x1000_0000, so OS code written
// for QEMU works here without changes.
//
// Minimal register map (offsets from base):
//   0  THR — Transmit Holding Register: write a byte to send it
//   5  LSR — Line Status Register: bit 5 (THRE) = TX buffer empty, bit 6 (TEMT) = TX idle
//
// We implement TX only. The output is buffered in a Vec<u8> shared with the Processor
// via Arc<Mutex<...>>, so callers can drain it without touching stdout — important for
// keeping the TUI display clean.
pub struct Uart {
    output: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

impl Uart {
    pub fn new(output: std::sync::Arc<std::sync::Mutex<Vec<u8>>>) -> Self {
        Self { output }
    }
}

impl Device for Uart {
    fn read(&self, addr: u32, _size: AccessSize) -> Result<u32, MemoryFault> {
        match addr {
            // LSR bits 5+6 set: TX holding register empty, TX shift register empty.
            // Returning 0x60 means the transmitter is always ready, so OS polling
            // loops of the form `while (LSR & 0x20 == 0) {}` exit immediately.
            5 => Ok(0x60),
            _ => Ok(0),
        }
    }

    fn write(&mut self, addr: u32, val: u32, _size: AccessSize) -> Result<(), MemoryFault> {
        if addr == 0 {
            // THR: low byte is the character to transmit.
            self.output.lock().unwrap().push((val & 0xFF) as u8);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_uart_write_buffers_byte() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let mut uart = Uart::new(Arc::clone(&buf));
        uart.write(0, b'H' as u32, AccessSize::Byte).unwrap();
        uart.write(0, b'i' as u32, AccessSize::Byte).unwrap();
        assert_eq!(*buf.lock().unwrap(), b"Hi");
    }

    #[test]
    fn test_uart_lsr_reports_ready() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let uart = Uart::new(Arc::clone(&buf));
        assert_eq!(uart.read(5, AccessSize::Byte).unwrap(), 0x60);
    }

    #[test]
    fn test_uart_write_to_non_thr_offset_is_ignored() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let mut uart = Uart::new(Arc::clone(&buf));
        uart.write(1, 0xFF, AccessSize::Byte).unwrap(); // IER — not THR
        assert!(buf.lock().unwrap().is_empty());
    }
}
