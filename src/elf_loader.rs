use crate::config;
use goblin::elf::Elf;
use goblin::elf::header::{EM_RISCV, ET_EXEC};

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
    NotRiscv,
    NotExecutable,
    BadSegment(String),
    BadEntryPoint(u32),
}

impl std::fmt::Display for ElfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElfError::Parse(s) => write!(f, "ELF parse error: {}", s),
            ElfError::NotElf32 => write!(f, "Not a 32-bit ELF binary"),
            ElfError::NotRiscv => write!(f, "Not a RISC-V binary"),
            ElfError::NotExecutable => write!(f, "Not an executable ELF (e_type != ET_EXEC)"),
            ElfError::BadSegment(s) => write!(f, "Bad ELF segment: {}", s),
            ElfError::BadEntryPoint(addr) => {
                write!(
                    f,
                    "Entry point 0x{:08x} is not inside any loaded segment",
                    addr
                )
            }
        }
    }
}

pub fn load(bytes: &[u8]) -> Result<ElfImage, ElfError> {
    let elf = Elf::parse(bytes).map_err(|e| ElfError::Parse(e.to_string()))?;

    if elf.is_64 {
        return Err(ElfError::NotElf32);
    }
    if elf.header.e_machine != EM_RISCV {
        return Err(ElfError::NotRiscv);
    }
    if elf.header.e_type != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }

    let mut segments = Vec::new();
    for ph in elf
        .program_headers
        .iter()
        .filter(|ph| ph.p_type == goblin::elf::program_header::PT_LOAD)
    {
        // A crafted header can claim any sizes/offsets: validate everything
        // before slicing or allocating.
        if ph.p_filesz > ph.p_memsz {
            return Err(ElfError::BadSegment(format!(
                "p_filesz {} exceeds p_memsz {}",
                ph.p_filesz, ph.p_memsz
            )));
        }
        if ph.p_memsz > config::DRAM_SIZE as u64 {
            return Err(ElfError::BadSegment(format!(
                "p_memsz {} exceeds DRAM size ({} bytes)",
                ph.p_memsz,
                config::DRAM_SIZE
            )));
        }
        let file_start = ph.p_offset as usize;
        let filesz = ph.p_filesz as usize;
        let file_bytes = file_start
            .checked_add(filesz)
            .and_then(|end| bytes.get(file_start..end))
            .ok_or_else(|| {
                ElfError::BadSegment(format!(
                    "segment data (offset {}, {} bytes) extends past end of file ({} bytes)",
                    file_start,
                    filesz,
                    bytes.len()
                ))
            })?;
        let mut data = file_bytes.to_vec();
        // zero-fill to p_memsz (e.g. .bss)
        if ph.p_memsz > ph.p_filesz {
            data.resize(ph.p_memsz as usize, 0);
        }
        segments.push(ElfSegment {
            vaddr: ph.p_vaddr as u32,
            paddr: ph.p_paddr as u32,
            data,
            filesz,
        });
    }

    let entry_point = elf.entry as u32;
    let entry_in_segment = segments.iter().any(|seg| {
        let start = seg.vaddr as u64;
        let end = start + seg.data.len() as u64;
        (start..end).contains(&(entry_point as u64))
    });
    if !entry_in_segment {
        return Err(ElfError::BadEntryPoint(entry_point));
    }

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
    use crate::processor::Processor;

    // Hand-built minimal ELF32 (little-endian): 52-byte header, one program
    // header at offset 52, segment payload at offset 84. Lets each test craft
    // a specific malformed field.
    struct ElfBuilder {
        e_machine: u16,
        e_type: u16,
        entry: u32,
        vaddr: u32,
        filesz: u32,
        memsz: u32,
        payload: Vec<u8>,
    }

    impl ElfBuilder {
        fn riscv_exec() -> Self {
            Self {
                e_machine: 243, // EM_RISCV
                e_type: 2,      // ET_EXEC
                entry: config::DRAM_BASE,
                vaddr: config::DRAM_BASE,
                filesz: 4,
                memsz: 4,
                payload: vec![0x13, 0x00, 0x00, 0x00], // nop (addi x0, x0, 0)
            }
        }

        fn build(&self) -> Vec<u8> {
            let mut b = Vec::new();
            // e_ident: magic, ELFCLASS32, ELFDATA2LSB (LE), EV_CURRENT
            b.extend_from_slice(&[0x7f, b'E', b'L', b'F', 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            b.extend_from_slice(&self.e_type.to_le_bytes());
            b.extend_from_slice(&self.e_machine.to_le_bytes());
            b.extend_from_slice(&1u32.to_le_bytes()); // e_version
            b.extend_from_slice(&self.entry.to_le_bytes());
            b.extend_from_slice(&52u32.to_le_bytes()); // e_phoff
            b.extend_from_slice(&0u32.to_le_bytes()); // e_shoff
            b.extend_from_slice(&0u32.to_le_bytes()); // e_flags
            b.extend_from_slice(&52u16.to_le_bytes()); // e_ehsize
            b.extend_from_slice(&32u16.to_le_bytes()); // e_phentsize
            b.extend_from_slice(&1u16.to_le_bytes()); // e_phnum
            b.extend_from_slice(&40u16.to_le_bytes()); // e_shentsize
            b.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
            b.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx
            // Program header
            b.extend_from_slice(&1u32.to_le_bytes()); // p_type = PT_LOAD
            b.extend_from_slice(&84u32.to_le_bytes()); // p_offset
            b.extend_from_slice(&self.vaddr.to_le_bytes());
            b.extend_from_slice(&self.vaddr.to_le_bytes()); // p_paddr = p_vaddr
            b.extend_from_slice(&self.filesz.to_le_bytes());
            b.extend_from_slice(&self.memsz.to_le_bytes());
            b.extend_from_slice(&7u32.to_le_bytes()); // p_flags
            b.extend_from_slice(&4u32.to_le_bytes()); // p_align
            b.extend_from_slice(&self.payload);
            b
        }
    }

    #[test]
    fn rejects_invalid_bytes() {
        let result = load(b"not an elf");
        assert!(matches!(result, Err(ElfError::Parse(_))));
    }

    #[test]
    fn rejects_empty_file() {
        assert!(load(&[]).is_err());
    }

    #[test]
    fn accepts_minimal_valid_riscv_exec() {
        let image = load(&ElfBuilder::riscv_exec().build()).unwrap();
        assert_eq!(image.entry_point, config::DRAM_BASE);
        assert_eq!(image.segments.len(), 1);
        let processor = Processor::from_elf(&image).unwrap();
        assert_eq!(processor.read_memory_word(config::DRAM_BASE), Ok(0x13));
    }

    #[test]
    fn rejects_truncated_file() {
        let bytes = ElfBuilder::riscv_exec().build();
        // Cut into the payload: header parses but segment data is missing.
        assert!(matches!(
            load(&bytes[..bytes.len() - 2]),
            Err(ElfError::BadSegment(_))
        ));
        // Cut into the header itself.
        assert!(load(&bytes[..30]).is_err());
    }

    #[test]
    fn rejects_64bit_elf() {
        let mut bytes = ElfBuilder::riscv_exec().build();
        bytes[4] = 2; // EI_CLASS = ELFCLASS64
        assert!(load(&bytes).is_err());
    }

    #[test]
    fn rejects_foreign_machine() {
        let mut builder = ElfBuilder::riscv_exec();
        builder.e_machine = 3; // EM_386
        assert!(matches!(load(&builder.build()), Err(ElfError::NotRiscv)));
    }

    #[test]
    fn rejects_non_executable() {
        let mut builder = ElfBuilder::riscv_exec();
        builder.e_type = 1; // ET_REL
        assert!(matches!(
            load(&builder.build()),
            Err(ElfError::NotExecutable)
        ));
    }

    #[test]
    fn rejects_oversized_memsz() {
        let mut builder = ElfBuilder::riscv_exec();
        builder.memsz = u32::MAX; // would allocate 4 GB
        assert!(matches!(
            load(&builder.build()),
            Err(ElfError::BadSegment(_))
        ));
    }

    #[test]
    fn rejects_filesz_larger_than_memsz() {
        let mut builder = ElfBuilder::riscv_exec();
        builder.memsz = 2;
        assert!(matches!(
            load(&builder.build()),
            Err(ElfError::BadSegment(_))
        ));
    }

    #[test]
    fn rejects_entry_point_outside_segments() {
        let mut builder = ElfBuilder::riscv_exec();
        builder.entry = config::DRAM_BASE + 0x1000;
        assert!(matches!(
            load(&builder.build()),
            Err(ElfError::BadEntryPoint(_))
        ));
    }

    #[test]
    fn from_elf_rejects_segment_below_dram() {
        let mut builder = ElfBuilder::riscv_exec();
        builder.vaddr = 0x1000;
        builder.entry = 0x1000;
        let image = load(&builder.build()).unwrap();
        assert!(Processor::from_elf(&image).is_err());
    }

    #[test]
    fn from_elf_rejects_segment_overhanging_dram_end() {
        let mut builder = ElfBuilder::riscv_exec();
        builder.vaddr = config::DRAM_BASE + config::DRAM_SIZE - 2;
        builder.entry = builder.vaddr;
        let image = load(&builder.build()).unwrap();
        assert!(Processor::from_elf(&image).is_err());
    }
}
