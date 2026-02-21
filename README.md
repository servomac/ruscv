# ruscv

A RISC-V Assembler and Emulator implementation in Rust.

`ruscv` is a project aimed at providing a modular and extensible platform for assembling RISC-V assembly code and emulating its execution on an RV32I-compatible virtual processor.

## Features

- **Modular Pipeline**: Separate stages for lexing, parsing, symbol resolution, assembly, and execution.
- **RV32I Support**: Implements decoding and execution for the base integer instruction set, including:
  - Arithmetic and Logical operations (R-type and I-type).
  - Memory operations (Loads and Stores).
  - Control Flow (Branches, `JAL`, `JALR`).
  - Upper Immediate instructions (`LUI`, `AUIPC`).
- **Assembler**: Supports basic assembly syntax, labels, and directives (`.text`, `.data`, `.word`, `.asciz`, `.align`).
- **Comprehensive Error Handling**: The assembler identifies and reports multiple errors across the source file instead of failing at the first encountered issue.
- **Unit Tested**: Extensively verified with a suite of unit tests for instruction encoding, decoding, and execution state transitions.

## Pending Features

- **System Instructions**: Implement `ECALL` and `EBREAK` for system calls and breakpoints.
- **Memory Faults**: Implement proper memory fault handling for out-of-bounds, unaligned, and non-executable access.

## Project Structure

- `src/lexer.rs`: Tokenizes assembly source into a stream of tokens.
- `src/parser.rs`: Parses tokens into abstract statements (Instructions or Directives).
- `src/symbols.rs`: Handles label definitions and address resolution.
- `src/assembler.rs`: Converts instructions and data into binary segments for the processor.
- `src/processor.rs`: The heart of the emulator, handling instruction fetch, decode, and execution (simulation of registers and memory).
- `src/config.rs`: Central configuration for memory base addresses and architectural constants.

## Usage

Currently, the project is in early development. The main entry point demonstrates a simple assembly and execution flow:

```rust
fn main() {
    let tokens = lexer::tokenize("add x20, x19, x18");
    let mut parser = Parser::new(tokens);
    let statements = parser.parse().unwrap();

    let mut symbol_table = SymbolTable::new(config::TEXT_BASE, config::DATA_BASE);
    symbol_table.build(&statements).expect("Symbol table build failed");

    let mut assembler = Assembler::new(config::TEXT_BASE, config::DATA_BASE);
    if let Ok(()) = assembler.assemble(&statements, &symbol_table) {
        let mut p = Processor::new(config::TEXT_BASE, config::DATA_BASE, config::STACK_BASE, config::STACK_SIZE);
        p.load(&assembler.text_bin, &assembler.data_bin);
        p.show_state();
    }
}
```

## Running Tests

To run the comprehensive test suite:

```bash
cargo test
```

## Contributing

This is an educational project exploring RISC-V architecture and Rust systems programming. Feel free to explore the code and run the existing tests to understand the implementation.
