 //! Qi Language Compiler
//!
//! A compiler for the Qi programming language with 100% Chinese keywords.
//! Compiles Qi source code to executable binaries for multiple platforms.

#![allow(missing_docs)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![warn(clippy::all)]

pub mod cli;
pub mod codegen;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod semantic;
pub mod targets;
pub mod utils;

// Force export of async runtime FFI functions to ensure they're included in the static library
pub use runtime::async_runtime::ffi::{qi_runtime_create_task, qi_runtime_await, qi_runtime_spawn_task};

// Dummy function to ensure async runtime functions are not optimized out
#[doc(hidden)]
#[no_mangle]
pub extern "C" fn _qi_force_link_async_runtime() {
    // These functions need to be referenced to prevent optimization
    unsafe {
        std::ptr::read_volatile(&qi_runtime_create_task as *const _);
        std::ptr::read_volatile(&qi_runtime_await as *const _);
        std::ptr::read_volatile(&qi_runtime_spawn_task as *const _);
    }
}

use std::path::PathBuf;

/// Compiler configuration and settings
pub mod config;

/// Main compiler interface
#[allow(dead_code)]
pub struct QiCompiler {
    config: config::CompilerConfig,
}

impl QiCompiler {
    /// Create a new compiler instance with default configuration
    pub fn new() -> Self {
        Self {
            config: config::CompilerConfig::default(),
        }
    }

    /// Create a new compiler instance with custom configuration
    pub fn with_config(config: config::CompilerConfig) -> Self {
        Self { config }
    }

    /// Compile a Qi source file to an executable
    pub fn compile(&self, source_file: PathBuf) -> Result<CompilationResult, CompilerError> {
        let start_time = std::time::Instant::now();
        let warnings = Vec::new();

        // Read source file
        let source_code = std::fs::read_to_string(&source_file)
            .map_err(CompilerError::Io)?;

        // Phase 1: Lexical analysis and parsing using manual parser
        let mut lexer = crate::lexer::Lexer::new(source_code);
        let tokens = lexer.tokenize()
            .map_err(|e| CompilerError::Lexical(format!("{}", e)))?;

        let parser = crate::parser::Parser::new();
        let ast = parser.parse(tokens)
            .map_err(|e| CompilerError::Parse(format!("解析错误: {}", e)))?;

        // Phase 3: Generate LLVM IR from AST
        let mut codegen = crate::codegen::CodeGenerator::new(self.config.target_platform.clone());
        let ir_content = codegen.generate(&crate::parser::ast::AstNode::程序(ast))
            .map_err(|e| CompilerError::Codegen(format!("Code generation failed: {:?}", e)))?;

        // Write LLVM IR to file
        let ir_path = source_file.with_extension("ll");
        std::fs::write(&ir_path, ir_content)
            .map_err(CompilerError::Io)?;

        let duration = start_time.elapsed().as_millis() as u64;
        Ok(CompilationResult {
            executable_path: ir_path,
            duration_ms: duration,
            warnings,
        })
    }
}

/// Result of a compilation operation
#[derive(Debug, Clone)]
pub struct CompilationResult {
    /// Path to the generated executable
    pub executable_path: PathBuf,
    /// Compilation duration in milliseconds
    pub duration_ms: u64,
    /// Warnings generated during compilation
    pub warnings: Vec<String>,
}


/// Compilation error types
#[derive(Debug, thiserror::Error)]
pub enum CompilerError {
    /// Lexical analysis error
    #[error("词法错误: {0}")]
    Lexical(String),
    /// Parsing error
    #[error("解析错误: {0}")]
    Parse(String),
    /// Semantic analysis error
    #[error("语义错误: {0}")]
    Semantic(String),
    /// Code generation error
    #[error("代码生成错误: {0}")]
    Codegen(String),
    /// I/O error
    #[error("输入/输出错误: {0}")]
    Io(#[from] std::io::Error),
}