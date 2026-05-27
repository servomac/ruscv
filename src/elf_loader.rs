use goblin::elf::Elf;

#[derive(Debug)]
pub struct ElfSegment {
    pub vaddr: u32,
    /// Physical/load address — may differ from vaddr for data segments in
    /// ROM→RAM layouts. Startup code reads from paddr and copies to vaddr.
    pub paddr: u32,
    pub data: Vec<u8>, // zero-padded to p_memsz
    pub filesz: usize, // bytes present in the file (< data.len() for .bss)
}

#[derive(Debug)]
pub struct ElfImage {
    pub segments: Vec<ElfSegment>,
    pub entry_point: u32,
    /// Address of the `tohost` symbol, used to detect test pass/fail
    pub tohost_addr: Option<u32>,
}

#[derive(Debug)]
pub enum ElfError {
    Parse(String),
    NotElf32,
}

impl std::fmt::Display for ElfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElfError::Parse(s) => write!(f, "ELF parse error: {}", s),
            ElfError::NotElf32 => write!(f, "Not a 32-bit ELF binary"),
        }
    }
}

pub fn load(bytes: &[u8]) -> Result<ElfImage, ElfError> {
    let elf = Elf::parse(bytes).map_err(|e| ElfError::Parse(e.to_string()))?;

    if elf.is_64 {
        return Err(ElfError::NotElf32);
    }

    let segments: Vec<ElfSegment> = elf
        .program_headers
        .iter()
        .filter(|ph| ph.p_type == goblin::elf::program_header::PT_LOAD)
        .map(|ph| {
            let file_start = ph.p_offset as usize;
            let filesz = ph.p_filesz as usize;
            let mut data = bytes[file_start..file_start + filesz].to_vec();
            // zero-fill to p_memsz (e.g. .bss)
            if ph.p_memsz > ph.p_filesz {
                data.resize(ph.p_memsz as usize, 0);
            }
            ElfSegment {
                vaddr: ph.p_vaddr as u32,
                paddr: ph.p_paddr as u32,
                data,
                filesz,
            }
        })
        .collect();

    let entry_point = elf.entry as u32;

    let tohost_addr = elf
        .syms
        .iter()
        .find(|sym| {
            elf.strtab
                .get_at(sym.st_name)
                .map_or(false, |name| name == "tohost")
        })
        .map(|sym| sym.st_value as u32);

    Ok(ElfImage {
        segments,
        entry_point,
        tohost_addr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_bytes() {
        let result = load(b"not an elf");
        assert!(matches!(result, Err(ElfError::Parse(_))));
    }
}
