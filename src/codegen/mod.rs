//! LLVM IR generation for Qi language

pub mod builder;
pub mod llvm;
pub mod optimization;

use builder::IrBuilder;
use optimization::OptimizationManager;
use crate::config::{CompilationTarget, OptimizationLevel};

/// Code generator
#[allow(dead_code)]
pub struct CodeGenerator {
    target: CompilationTarget,
    ir_builder: IrBuilder,
    optimization_manager: OptimizationManager,
}

impl CodeGenerator {
    /// Create a new code generator for the given target
    pub fn new(target: CompilationTarget) -> Self {
        Self {
            target,
            ir_builder: IrBuilder::new(),
            optimization_manager: OptimizationManager::new(OptimizationLevel::Basic),
        }
    }

    /// Create a new code generator with specified optimization level
    pub fn new_with_optimization(target: CompilationTarget, opt_level: OptimizationLevel) -> Self {
        Self {
            target,
            ir_builder: IrBuilder::new(),
            optimization_manager: OptimizationManager::new(opt_level),
        }
    }

    /// Set external function declarations for this module
    pub fn set_external_functions(&mut self, external_funcs: std::collections::HashMap<String, (Vec<String>, String)>) {
        self.ir_builder.set_external_functions(external_funcs);
    }

    /// Set defined functions in this module
    pub fn set_defined_functions(&mut self, defined_funcs: std::collections::HashSet<String>) {
        self.ir_builder.set_defined_functions(defined_funcs);
    }

    /// Generate LLVM IR from AST
    pub fn generate(&mut self, ast: &crate::parser::ast::AstNode) -> Result<String, CodegenError> {
        let ir = self.ir_builder.build(ast)
            .map_err(|e| CodegenError::Llvm(e))?;

        // Run optimizations
        let optimized_ir = self.optimization_manager.run_optimizations(&ir)
            .map_err(|e| CodegenError::Optimization(e.to_string()))?;

        Ok(optimized_ir)
    }

    /// Generate object file from LLVM IR
    pub fn compile_to_object(&self, _ir: &str) -> Result<Vec<u8>, CodegenError> {
        // TODO: Implement object file generation
        todo!("Implement object file generation")
    }

    /// Set optimization level
    pub fn set_optimization_level(&mut self, level: OptimizationLevel) {
        self.optimization_manager.set_optimization_level(level);
    }

    /// Get current optimization level
    pub fn get_optimization_level(&self) -> OptimizationLevel {
        self.optimization_manager.get_optimization_level()
    }

    /// Generate LLVM IR without optimizations
    pub fn generate_without_optimization(&mut self, ast: &crate::parser::ast::AstNode) -> Result<String, CodegenError> {
        let ir = self.ir_builder.build(ast)
            .map_err(|e| CodegenError::Llvm(e))?;
        Ok(ir)
    }
}

/// Code generation errors
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    /// LLVM error
    #[error("LLVM 错误: {0}")]
    Llvm(String),

    /// Target not supported
    #[error("不支持的目标平台: {0}")]
    UnsupportedTarget(CompilationTarget),

    /// Optimization error
    #[error("优化错误: {0}")]
    Optimization(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    #[test]
    fn test_simple_code_generation() {
        let source = "变量 x = 42;".to_string();
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");

        let parser = Parser::new();
        let program = parser.parse(tokens).expect("Should parse successfully");

        let mut codegen = CodeGenerator::new(crate::config::CompilationTarget::Linux);
        let ir = codegen.generate(&program.statements[0]).expect("Should generate IR");

        // Check that generated IR contains expected elements
        assert!(ir.contains("x = alloca"));
        assert!(ir.contains("store i64 42"));
        println!("Generated IR:\n{}", ir);
    }

    #[test]
    fn test_function_code_generation() {
        let source = "函数 test() { 返回 42; }".to_string();
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");

        let parser = Parser::new();
        let program = parser.parse(tokens).expect("Should parse successfully");

        let mut codegen = CodeGenerator::new(crate::config::CompilationTarget::Linux);
        let ir = codegen.generate(&program.statements[0]).expect("Should generate IR");

        // Check that generated IR contains expected elements
        println!("Generated Function IR:\n{}", ir);
        assert!(ir.contains("define"));
        assert!(ir.contains("@test"));
        assert!(ir.contains("ret i64 42"));
        // Regression: ensure closing brace is present (allow optional trailing newline)
        let ir_trimmed = ir.trim_end();
        assert!(ir_trimmed.ends_with('}'), "Generated IR should end with a closing brace '}}' for function end");
    }
}