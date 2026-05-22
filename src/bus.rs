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
            .ok_or(MemoryFault::OutOfBounds { address: addr as u32 }),
        AccessSize::Half => {
            let bytes = data
                .get(addr..addr + 2)
                .ok_or(MemoryFault::OutOfBounds { address: addr as u32 })?;
            Ok(u32::from_le_bytes([bytes[0], bytes[1], 0, 0]))
        }
        AccessSize::Word => {
            let bytes = data
                .get(addr..addr + 4)
                .ok_or(MemoryFault::OutOfBounds { address: addr as u32 })?;
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
        let slot = self.data
            .get_mut(addr..addr + n)
            .ok_or(MemoryFault::OutOfBounds { address: addr as u32 })?;
        slot.copy_from_slice(&bytes[..n]);
        Ok(())
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
        read_bytes(&self.data, addr, size)
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
