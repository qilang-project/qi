//! Qi Basic Runtime Environment
//!
//! This module provides the foundational runtime environment for executing compiled Qi programs.
//! It includes memory management, I/O operations, standard library functions, and comprehensive
//! Chinese language support.
//!
//! # Features
//!
//! - **Memory Management**: Hybrid ownership + reference counting system
//! - **I/O Operations**: Synchronous file and network operations with Chinese keyword support
//! - **Standard Library**: Built-in functions for strings, math, and system operations
//! - **Error Handling**: Comprehensive Chinese error message system
//! - **Cross-Platform**: Support for Linux, Windows, and macOS
//!
//! # Usage
//!
//! ```rust
//! use qi_runtime::{RuntimeEnvironment, RuntimeConfig};
//!
//! let config = RuntimeConfig::default();
//! let mut runtime = RuntimeEnvironment::new(config)?;
//! runtime.initialize()?;
//! runtime.execute_program(program_data)?;
//! ```

pub mod environment;
pub mod memory;
pub mod io;
pub mod stdlib;
pub mod error;
pub mod executor;

// Legacy modules for backward compatibility
pub mod strings;
pub mod errors;

// Re-export core components for convenience
pub use environment::{RuntimeEnvironment, RuntimeState, RuntimeConfig};
pub use memory::{MemoryManager, AllocationStrategy};
pub use io::{FileSystemInterface, NetworkManager};
pub use stdlib::{StandardLibrary, StringModule, MathModule};
pub use error::{ErrorHandler, ChineseErrorMessages};

/// Runtime version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Runtime build timestamp
pub const BUILD_TIMESTAMP: &str = "2025-01-22";

/// Result type for runtime operations
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Core runtime error type - unified with error module
pub type RuntimeError = error::Error;

/// Runtime library interface
pub struct RuntimeLibrary {
    memory_interface: memory::MemoryInterface,
    string_interface: strings::StringInterface,
    error_interface: errors::ErrorInterface,
    io_interface: io::IoInterface,
}

impl RuntimeLibrary {
    /// Create a new runtime library interface
    pub fn new() -> Result<Self, RuntimeError> {
        Ok(Self {
            memory_interface: memory::MemoryInterface::new()?,
            string_interface: strings::StringInterface::new(),
            error_interface: errors::ErrorInterface::new(),
            io_interface: io::IoInterface::new()?,
        })
    }

    /// Initialize the runtime library
    pub fn initialize(&mut self) -> Result<(), RuntimeError> {
        self.memory_interface.initialize()?;
        self.string_interface.initialize()?;
        self.error_interface.initialize()?;
        self.io_interface.initialize()?;
        Ok(())
    }

    /// Get memory management interface
    pub fn memory(&self) -> &memory::MemoryInterface {
        &self.memory_interface
    }

    /// Get mutable memory management interface
    pub fn memory_mut(&mut self) -> &mut memory::MemoryInterface {
        &mut self.memory_interface
    }

    /// Get string operations interface
    pub fn strings(&self) -> &strings::StringInterface {
        &self.string_interface
    }

    /// Get error handling interface
    pub fn errors(&self) -> &errors::ErrorInterface {
        &self.error_interface
    }

    /// Get I/O operations interface
    pub fn io(&self) -> &io::IoInterface {
        &self.io_interface
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info() {
        assert!(!VERSION.is_empty());
        assert!(!BUILD_TIMESTAMP.is_empty());
    }

    #[test]
    fn test_runtime_error_display() {
        let error = RuntimeError::program_execution_error("测试错误消息", "测试错误消息");
        assert!(error.to_string().contains("测试错误消息"));
    }

    #[test]
    fn test_runtime_library_initialization() {
        let mut runtime = RuntimeLibrary::new().unwrap();
        assert!(runtime.initialize().is_ok());
    }

    #[test]
    fn test_memory_operations() {
        let mut runtime = RuntimeLibrary::new().unwrap();
        runtime.initialize().unwrap();

        let memory = runtime.memory();
        assert_eq!(memory.get_allocated_bytes(), 0);
    }

    #[test]
    fn test_string_operations() {
        let mut runtime = RuntimeLibrary::new().unwrap();
        runtime.initialize().unwrap();

        let strings = runtime.strings();

        // Test string length
        assert_eq!(strings.length("你好").unwrap(), 2);
        assert_eq!(strings.length("Hello").unwrap(), 5);

        // Test string concatenation
        assert_eq!(strings.concat(&[String::from("你好"), String::from("世界")]).unwrap(), "你好世界");

        // Test string comparison
        assert_eq!(strings.compare("你好", "你好").unwrap(), 0);
        let result = strings.compare("你好", "世界").unwrap();
        assert!(result != 0, "Comparison should not be equal");
    }

    #[test]
    fn test_io_operations() {
        let mut runtime = RuntimeLibrary::new().unwrap();
        runtime.initialize().unwrap();

        let io = runtime.io();

        // Test printing (should not panic)
        assert!(io.print("Hello").is_ok());
        assert!(io.println_int(42).is_ok());
        assert!(io.println_float(3.14).is_ok());
    }

    #[test]
    fn test_memory_allocation() {
        let mut runtime = RuntimeLibrary::new().unwrap();
        runtime.initialize().unwrap();

        let memory = runtime.memory_mut();

        // Test allocation (using unsafe for testing)
        let ptr = memory.allocate(1024);
        assert!(ptr.is_ok());

        if let Ok(allocated_ptr) = ptr {
            assert_eq!(memory.get_allocated_bytes(), 1024);

            // Test deallocation
            assert!(memory.deallocate(allocated_ptr, 1024).is_ok());
            assert_eq!(memory.get_allocated_bytes(), 0);
        }
    }
}