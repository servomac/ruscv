mod assembler;
mod bus;
mod config;
mod elf_loader;
mod lexer;
mod parser;
mod processor;
mod pseudo;
mod runner;
mod session;
mod symbols;
mod tui;

fn main() -> Result<(), std::io::Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.get(1).map(String::as_str) == Some("--elf") {
        let path = args.get(2).unwrap_or_else(|| {
            eprintln!("Usage: ruscv --elf <path>");
            std::process::exit(1);
        });
        let bytes = std::fs::read(path)?;
        let result = runner::run_elf(&bytes, runner::DEFAULT_MAX_STEPS).unwrap_or_else(|e| {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        });
        match result {
            runner::TestResult::Pass => {
                println!("PASS");
                std::process::exit(0);
            }
            runner::TestResult::Fail { exit_code } => {
                println!("FAIL (test case {})", exit_code);
                std::process::exit(1);
            }
            runner::TestResult::Fault(e) => {
                use crate::processor::StepError;
                match e {
                    StepError::IllegalInstruction { pc, word } => println!(
                        "FAULT: IllegalInstruction at pc=0x{:08x} word=0x{:08x}",
                        pc, word
                    ),
                    other => println!("FAULT: {:?}", other),
                }
                std::process::exit(1);
            }
            runner::TestResult::MaxSteps => {
                println!("TIMEOUT (exceeded {} steps)", runner::DEFAULT_MAX_STEPS);
                std::process::exit(1);
            }
        }
    }

    let initial_file = args.get(1).cloned();
    tui::run(initial_file)
}
