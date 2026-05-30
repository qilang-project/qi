//! Platform-specific code generation

pub mod linux;
pub mod macos;
pub mod wasm;
pub mod windows;

use crate::config::CompilationTarget;

/// Platform-specific target interface
pub trait Target {
    /// Get the target triple
    fn target_triple(&self) -> &str;

    /// Get the target-specific CPU features
    fn cpu_features(&self) -> &[&str];

    /// Get the target-specific linker flags
    fn linker_flags(&self) -> &[&str];

    /// Generate platform-specific runtime code
    fn generate_runtime(&self) -> Result<String, TargetError>;
}

/// Create a target for the given compilation target
pub fn create_target(target: CompilationTarget) -> Box<dyn Target> {
    match target {
        CompilationTarget::Linux => Box::new(linux::LinuxTarget::new()),
        CompilationTarget::Windows => Box::new(windows::WindowsTarget::new()),
        CompilationTarget::MacOS => Box::new(macos::MacOSTarget::new()),
        CompilationTarget::Wasm => Box::new(wasm::WasmTarget::new()),
    }
}

/// Target-specific errors
#[derive(Debug, thiserror::Error)]
pub enum TargetError {
    /// Unsupported feature
    #[error("不支持的功能: {0}")]
    UnsupportedFeature(String),

    /// Linker error
    #[error("链接器错误: {0}")]
    Linker(String),

    /// Platform-specific error
    #[error("平台错误: {0}")]
    Platform(String),
}
