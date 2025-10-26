# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Qi is a programming language compiler with 100% Chinese keywords. It compiles Qi source code to LLVM IR and then to native executables. The project is written in Rust and includes a custom async runtime with M:N coroutine scheduling.

## Common Development Commands

### Building and Testing
```bash
# Build the compiler (development build)
cargo build

# Build with optimizations
cargo build --release

# Run tests
cargo test

# Run a specific test
cargo test test_name

# Run examples (verification)
cargo run -- run examples/basic/hello_world.qi
cargo run -- run examples/basic/calculations.qi
```

### Compiler Usage
```bash
# Check syntax only
cargo run -- check source_file.qi

# Compile to LLVM IR
cargo run -- compile source_file.qi -o output.ll

# Compile and run
cargo run -- run source_file.qi

# Format source code
cargo fmt

# Run linter
cargo clippy
```

### Development Workflow
```bash
# Build compiler and test with Chinese examples
cargo build
cargo run -- run examples/basic/变量类型演示.qi

# Test float operations
cargo run -- run examples/basic/float_test.qi

# Verify compilation pipeline
cargo run -- compile examples/basic/hello_world.qi -o test.ll
clang -c test.ll -o test.o  # Verify LLVM IR syntax
```

## Architecture Overview

### Core Compilation Pipeline
1. **Lexer** (`src/lexer/`): Tokenizes Chinese keywords and UTF-8 source
2. **Parser** (`src/parser/`): LALRPOP-generated parser with Chinese grammar rules
3. **AST** (`src/parser/ast.rs`): Abstract syntax tree with Chinese node names
4. **Code Generation** (`src/codegen/`): Converts AST to LLVM IR
5. **Runtime** (`src/runtime/`): Provides execution environment and async support

### Key Components

#### Parser (`src/parser/grammar.lalrpop`)
- Uses LALRPOP for parsing Chinese grammar
- Supports all Chinese keywords: `如果/否则`, `当`, `函数`, `变量`, etc.
- Handles operator precedence with 8-level expression hierarchy
- Generates AST with Chinese node variants

#### Code Generation (`src/codegen/builder.rs`)
- `IrBuilder` constructs LLVM IR through `IrInstruction` enum
- Special handling for Chinese function name mangling
- Parameter vs variable distinction (parameters used directly, variables loaded)
- Type-aware binary operations (integer vs float detection via `is_float_operand`)

#### Async Runtime (`src/runtime/async_runtime/`)
- M:N coroutine scheduler with work-stealing
- C FFI layer in `c_runtime/syscalls.c`
- Task queues, executor, and memory pools
- Integrated with LLVM IR generation through runtime calls

### Language Features Support
- **Chinese Keywords**: 100% Chinese identifiers and syntax
- **Type System**: Basic types (整数, 浮点数, 字符串, 布尔)
- **Functions**: Regular and async functions with Chinese names
- **Control Flow**: 如果/否则, 当, 对于 loops
- **Structs/Enums**: Chinese field names and variant names

## Important Implementation Details

### Chinese Name Handling
- Function names are mangled using `mangle_function_name()` for LLVM compatibility
- Parameter names get `%` prefix in LLVM IR
- Chinese identifiers are encoded as hexadecimal in LLVM symbols

### Type System
- Type annotations use `get_llvm_type()` to map Chinese types to LLVM types
- Float vs integer detection checks both literal content and variable types
- Parameters tracked with `param_` prefix in `variable_types` HashMap

### LLVM IR Generation
- Parameters used directly (no load instructions needed)
- Variables require alloca + store + load pattern
- Binary operations use operator-specific LLVM instructions
- String concatenation handled specially for `+` operator

### Known Issues and Workarounds
- Complex multi-function examples with mixed types may have type inference issues
- Chinese examples with multiple functions sometimes generate mismatched parameter types
- Simple examples and single functions work correctly

## File Structure Notes

### Critical Files
- `src/parser/grammar.lalrpop`: Chinese grammar definition
- `src/codegen/builder.rs`: LLVM IR generation logic
- `src/lexer/keywords.rs`: Chinese keyword definitions
- `src/main.rs`: CLI entry point
- `build.rs`: LALRPOP processing and C runtime compilation

### Example Organization
- `examples/basic/`: Working examples with Chinese syntax
- Chinese examples demonstrate full language capabilities
- Simple examples (hello_world, calculations) are most reliable

## Development Notes

### Compiler Flags
- Use `--features llvm` to enable LLVM code generation (when available)
- `--release` builds optimize for performance
- Debug builds include verbose logging for compilation steps

### Testing Strategy
- Test both simple and complex Chinese examples
- Verify LLVM IR compilation with clang
- Run generated executables to ensure runtime works
- Focus on type system correctness for mixed-type operations

### Performance Considerations
- M:N async runtime provides lightweight concurrency
- LLVM optimization passes in release builds
- Memory pools for coroutine stack management
- 可执行的 文件必须 包 主程序;

函数 入口() { ！！！