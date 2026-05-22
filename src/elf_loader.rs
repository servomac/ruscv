use goblin::elf::Elf;

#[derive(Debug)]
pub struct ElfImage {
    /// (load_address, bytes) for each PT_LOAD segment
    pub segments: Vec<(u32, Vec<u8>)>,
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

    let segments: Vec<(u32, Vec<u8>)> = elf
        .program_headers
        .iter()
        .filter(|ph| ph.p_type == goblin::elf::program_header::PT_LOAD)
        .map(|ph| {
            let file_start = ph.p_offset as usize;
            let file_end = file_start + ph.p_filesz as usize;
            let mut data = bytes[file_start..file_end].to_vec();
            // zero-fill to p_memsz (e.g. .bss)
            if ph.p_memsz > ph.p_filesz {
                data.resize(ph.p_memsz as usize, 0);
            }
            (ph.p_paddr as u32, data)
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

    Ok(ElfImage { segments, entry_point, tohost_addr })
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
