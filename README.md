# ruscv

A RISC-V Assembler and Emulator implementation in Rust.

`ruscv` is a project aimed at providing a modular and extensible platform for assembling RISC-V assembly code and emulating its execution on an RV32I-compatible virtual processor.

## Features

- **Interactive TUI**: Real-time visualization of the processor state, memory, and logs.
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

- `src/tui.rs`: The interactive Terminal User Interface.
- `src/processor.rs`: The heart of the emulator, handling instruction fetch, decode, and execution.
- `src/assembler.rs`: Converts instructions and data into binary segments.
- `src/symbols.rs`: Handles label definitions and address resolution.
- `src/parser.rs`: Parses tokens into abstract statements.
- `src/lexer.rs`: Tokenizes assembly source into a stream of tokens.
- `src/config.rs`: Central configuration for memory base addresses and architectural constants.

## Usage

To start the interactive emulator, simply run:

```bash
cargo run
```

### Controls

| Key | Action |
| --- | --- |
| **F5** | Assemble and Run to completion / Halted |
| **F10** | Assemble and Step one instruction |
| **F9** | Cycle Number Format (Hex, Binary, Decimal) |
| **Tab** | Cycle Focus (Editor, Registers, Memory, Logs) |
| **Arrows** | Edit code or Scroll focused pane |
| **Esc** | Quit application |

## Running Tests

To run the comprehensive test suite:

```bash
cargo test
```

## Contributing

This is an educational project exploring RISC-V architecture and Rust systems programming. Feel free to explore the code and run the existing tests to understand the implementation.

