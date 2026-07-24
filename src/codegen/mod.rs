//! LLVM IR generation for Qi language
//!
//! inkwell 类型化 IR 后端是唯一后端（旧文本 IR builder 已淘汰）。

/// inkwell 后端（类型化 IR）—— 唯一后端。
pub mod inkwell_gen;
pub mod module_registry;
pub mod stdlib_abi;

use crate::config::CompilationTarget;

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
