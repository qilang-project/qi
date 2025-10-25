# Qi Compiler Tests

This directory contains the test suite for the Qi compiler, a 100% Chinese programming language compiler.

## Test Structure

All test files in this directory are active integration tests that run with `cargo test --test '*'`.

### Test Files

| Test File | Purpose | Test Count |
|----------|---------|------------|
| `integration_tests.rs` | Full compilation pipeline integration tests | 28 |
| `lexer_tests.rs` | Tokenization and lexical analysis tests | 20 |
| `parser_tests.rs` | Parsing and AST generation tests | 34 (1 ignored) |
| `semantic_tests.rs` | Semantic analysis and type checking tests | 21 |
| `codegen_tests.rs` | LLVM IR code generation tests | 23 |
| `control_flow_tests.rs` | Control flow statement tests | 17 |
| `diagnostics_tests.rs` | Error reporting and diagnostics tests | 12 |
| `module_tests.rs` | Package/module system tests | 8 |
| `async_runtime_tests.rs` | Async runtime functionality tests | 3 |

### Running Tests

```bash
# Run all tests (library + integration)
cargo test

# Run only integration tests
cargo test --test '*'

# Run specific test file
cargo test --test lexer_tests

# Run with output
cargo test --test integration_tests -- --nocapture
```

## Test Coverage

The test suite covers:
- **Chinese keyword tokenization** - Complete support for Chinese language tokens
- **Parsing** - LALRPOP grammar parsing with Chinese syntax
- **Semantic analysis** - Type checking and semantic validation
- **Code generation** - LLVM IR generation for various constructs
- **Integration** - End-to-end compilation pipeline
- **Error handling** - Chinese error messages and diagnostics
- **Runtime system** - Async runtime and memory management

## Total Tests

**166 active tests** across all test files, with 100% pass rate.

All tests are maintained to ensure the Qi compiler continues to work correctly across all its features.