#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MemoryFault {
    OutOfBounds { address: u32 },
    WriteToReadOnly { address: u32 },
    UnalignedAccess { address: u32 },
    ExecuteFromNonExecutable { address: u32 },
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
    pub regions: Vec<(u32, u32, Box<dyn Device>)>,
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
        let addr = addr as usize;
        match size {
            AccessSize::Byte => {
                self.data.get(addr).map(|&b| b as u32).ok_or(MemoryFault::OutOfBounds { address: addr as u32 })
            }
            AccessSize::Half => {
                if addr + 1 >= self.data.len() {
                    return Err(MemoryFault::OutOfBounds { address: addr as u32 });
                }
                let val = (self.data[addr] as u32) | (self.data[addr + 1] as u32) << 8;
                Ok(val)
            }
            AccessSize::Word => {
                if addr + 3 >= self.data.len() {
                    return Err(MemoryFault::OutOfBounds { address: addr as u32 });
                }
                let val = (self.data[addr] as u32)
                    | (self.data[addr + 1] as u32) << 8
                    | (self.data[addr + 2] as u32) << 16
                    | (self.data[addr + 3] as u32) << 24;
                Ok(val)
            }
        }
    }

    fn write(&mut self, addr: u32, val: u32, size: AccessSize) -> Result<(), MemoryFault> {
        let addr = addr as usize;
        match size {
            AccessSize::Byte => {
                if addr >= self.data.len() {
                    return Err(MemoryFault::OutOfBounds { address: addr as u32 });
                }
                self.data[addr] = val as u8;
                Ok(())
            }
            AccessSize::Half => {
                if addr + 1 >= self.data.len() {
                    return Err(MemoryFault::OutOfBounds { address: addr as u32 });
                }
                self.data[addr] = val as u8;
                self.data[addr + 1] = (val >> 8) as u8;
                Ok(())
            }
            AccessSize::Word => {
                if addr + 3 >= self.data.len() {
                    return Err(MemoryFault::OutOfBounds { address: addr as u32 });
                }
                self.data[addr] = val as u8;
                self.data[addr + 1] = (val >> 8) as u8;
                self.data[addr + 2] = (val >> 16) as u8;
                self.data[addr + 3] = (val >> 24) as u8;
                Ok(())
            }
        }
    }
}

// ROM Device Implementation
pub struct Rom {
    pub data: Vec<u8>,
}

impl Rom {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl Device for Rom {
    fn read(&self, addr: u32, size: AccessSize) -> Result<u32, MemoryFault> {
        let addr = addr as usize;
        match size {
            AccessSize::Byte => {
                self.data.get(addr).map(|&b| b as u32).ok_or(MemoryFault::OutOfBounds { address: addr as u32 })
            }
            AccessSize::Half => {
                if addr + 1 >= self.data.len() {
                    return Err(MemoryFault::OutOfBounds { address: addr as u32 });
                }
                let val = (self.data[addr] as u32) | (self.data[addr + 1] as u32) << 8;
                Ok(val)
            }
            AccessSize::Word => {
                if addr + 3 >= self.data.len() {
                    return Err(MemoryFault::OutOfBounds { address: addr as u32 });
                }
                let val = (self.data[addr] as u32)
                    | (self.data[addr + 1] as u32) << 8
                    | (self.data[addr + 2] as u32) << 16
                    | (self.data[addr + 3] as u32) << 24;
                Ok(val)
            }
        }
    }

    fn write(&mut self, addr: u32, _val: u32, _size: AccessSize) -> Result<(), MemoryFault> {
        Err(MemoryFault::WriteToReadOnly { address: addr })
    }
}

// Placeholder for MMIO devices
pub struct MmioDevice {
    name: String,
}

impl MmioDevice {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Device for MmioDevice {
    fn read(&self, _addr: u32, _size: AccessSize) -> Result<u32, MemoryFault> {
        // Just return 0 for now as a placeholder
        Ok(0)
    }

    fn write(&mut self, _addr: u32, _val: u32, _size: AccessSize) -> Result<(), MemoryFault> {
        Ok(())
    }
}
