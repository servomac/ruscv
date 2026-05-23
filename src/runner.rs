use std::io::Write;
use crate::elf_loader;
use crate::processor::{Processor, StepError};

pub const DEFAULT_MAX_STEPS: u64 = 100_000_000;

#[derive(Debug)]
pub enum TestResult {
    Pass,
    Fail { exit_code: u32 },
    Fault(StepError),
    MaxSteps,
}

/// Load and run an ELF32 binary, returning when tohost is written or a halt
/// condition is reached. Returns Err if the ELF cannot be parsed.
pub fn run_elf(elf_bytes: &[u8], max_steps: u64) -> Result<TestResult, String> {
    let image = elf_loader::load(elf_bytes).map_err(|e| e.to_string())?;
    let tohost_addr = image.tohost_addr;
    let mut processor = Processor::from_elf(&image);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for _ in 0..max_steps {
        match processor.step() {
            Ok(()) => {}
            Err(e) => {
                flush_uart(&mut processor, &mut out);
                return Ok(TestResult::Fault(e));
            }
        }

        flush_uart(&mut processor, &mut out);

        if let Some(addr) = tohost_addr {
            match processor.read_memory_word(addr) {
                Ok(1) => return Ok(TestResult::Pass),
                Ok(v) if v != 0 => return Ok(TestResult::Fail { exit_code: v >> 1 }),
                _ => {}
            }
        }
    }

    flush_uart(&mut processor, &mut out);
    Ok(TestResult::MaxSteps)
}

fn flush_uart(processor: &mut Processor, out: &mut impl Write) {
    let bytes = processor.drain_uart();
    if !bytes.is_empty() {
        let _ = out.write_all(&bytes);
        let _ = out.flush();
    }
}
