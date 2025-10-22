//! Runtime Environment Management
//!
//! This module provides the core runtime environment that manages program lifecycle,
//! memory, I/O operations, and system resources for Qi program execution.

use std::time::{Duration, Instant};
use std::collections::HashMap;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use crate::runtime::{RuntimeResult, RuntimeError};
use crate::runtime::memory::MemoryManager;
use crate::runtime::io::{FileSystemInterface, NetworkManager};
use crate::runtime::stdlib::{StringModule, MathModule, SystemModule, ConversionModule, DebugModule};
use crate::runtime::error::ErrorHandler;

/// Runtime environment states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeState {
    /// Runtime components being initialized
    Initializing,
    /// Runtime ready to execute programs
    Ready,
    /// Currently executing a program
    Running,
    /// Cleaning up resources
    ShuttingDown,
    /// Runtime completely shut down
    Terminated,
}

/// Runtime configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Maximum memory usage in megabytes
    pub max_memory_mb: usize,
    /// Garbage collection trigger threshold (0.0-1.0)
    pub gc_threshold_percent: f64,
    /// Default I/O buffer size in bytes
    pub io_buffer_size: usize,
    /// Default network timeout in milliseconds
    pub network_timeout_ms: u64,
    /// Enable debug mode
    pub debug_mode: bool,
    /// Default locale for error messages
    pub locale: String,
    /// Enable performance monitoring
    pub enable_metrics: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 1024,
            gc_threshold_percent: 0.8,
            io_buffer_size: 8192,
            network_timeout_ms: 30000,
            debug_mode: false,
            locale: "zh-CN".to_string(),
            enable_metrics: true,
        }
    }
}

/// Runtime performance metrics
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeMetrics {
    /// Runtime initialization timestamp
    #[serde(skip)]
    pub startup_time: Instant,
    /// Current memory usage in megabytes
    pub memory_usage_mb: f64,
    /// Peak memory usage in megabytes
    pub peak_memory_mb: f64,
    /// Number of programs executed
    pub programs_executed: u64,
    /// Total execution time for all programs
    pub total_execution_time: Duration,
    /// Number of I/O operations performed
    pub io_operations: u64,
    /// Number of network operations performed
    pub network_operations: u64,
    /// Number of garbage collections performed
    pub gc_collections: u64,
    /// Number of errors encountered
    pub errors_encountered: u64,
}

/// Runtime performance metrics for deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetricsSerializable {
    /// Current memory usage in megabytes
    pub memory_usage_mb: f64,
    /// Peak memory usage in megabytes
    pub peak_memory_mb: f64,
    /// Number of programs executed
    pub programs_executed: u64,
    /// Total execution time for all programs
    pub total_execution_time: Duration,
    /// Number of I/O operations performed
    pub io_operations: u64,
    /// Number of network operations performed
    pub network_operations: u64,
    /// Number of garbage collections performed
    pub gc_collections: u64,
    /// Number of errors encountered
    pub errors_encountered: u64,
}

impl From<RuntimeMetrics> for RuntimeMetricsSerializable {
    fn from(metrics: RuntimeMetrics) -> Self {
        Self {
            memory_usage_mb: metrics.memory_usage_mb,
            peak_memory_mb: metrics.peak_memory_mb,
            programs_executed: metrics.programs_executed,
            total_execution_time: metrics.total_execution_time,
            io_operations: metrics.io_operations,
            network_operations: metrics.network_operations,
            gc_collections: metrics.gc_collections,
            errors_encountered: metrics.errors_encountered,
        }
    }
}

impl Default for RuntimeMetrics {
    fn default() -> Self {
        Self {
            startup_time: std::time::Instant::now(),
            memory_usage_mb: 0.0,
            peak_memory_mb: 0.0,
            programs_executed: 0,
            total_execution_time: Duration::ZERO,
            io_operations: 0,
            network_operations: 0,
            gc_collections: 0,
            errors_encountered: 0,
        }
    }
}

/// Core runtime environment that manages program lifecycle
#[derive(Debug)]
pub struct RuntimeEnvironment {
    /// Unique runtime identifier
    pub id: Uuid,
    /// Current runtime state
    pub state: RuntimeState,
    /// Memory management subsystem
    pub memory_manager: MemoryManager,
    /// File system interface
    pub file_system: FileSystemInterface,
    /// Network manager
    pub network_manager: NetworkManager,
    /// Standard library modules
    pub string_module: StringModule,
    pub math_module: MathModule,
    pub system_module: SystemModule,
    pub conversion_module: ConversionModule,
    pub debug_module: DebugModule,
    /// Error handling system
    pub error_handler: ErrorHandler,
    /// Runtime configuration
    pub config: RuntimeConfig,
    /// Performance and usage metrics
    pub metrics: RuntimeMetrics,
    /// Runtime startup timestamp
    pub startup_time: Instant,
}

impl RuntimeEnvironment {
    /// Create a new runtime environment with the given configuration
    pub fn new(config: RuntimeConfig) -> RuntimeResult<Self> {
        let id = Uuid::new_v4();
        let startup_time = Instant::now();

        // Initialize subsystems
        let memory_manager = MemoryManager::new(config.max_memory_mb, config.gc_threshold_percent)?;
        let file_system = FileSystemInterface::new(config.io_buffer_size)?;
              let network_manager = NetworkManager::new();
        let string_module = StringModule::new();
        let math_module = MathModule::new();
        let system_module = SystemModule::new();
        let conversion_module = ConversionModule::new();
        let debug_module = DebugModule::new();
        let error_handler = ErrorHandler::new();

        Ok(Self {
            id,
            state: RuntimeState::Initializing,
            memory_manager,
            file_system,
            network_manager,
            string_module,
            math_module,
            system_module,
            conversion_module,
            debug_module,
            error_handler,
            config,
            metrics: RuntimeMetrics::default(),
            startup_time,
        })
    }

    /// Initialize the runtime environment
    pub fn initialize(&mut self) -> RuntimeResult<()> {
        self.state = RuntimeState::Initializing;

        // Initialize memory manager
        self.memory_manager.initialize()?;

        // Initialize file system interface
        self.file_system.initialize()?;

        // Initialize network manager
        self.network_manager.initialize()?;

        
        // Initialize error handler
        self.error_handler.initialize()?;

        self.state = RuntimeState::Ready;
        Ok(())
    }

    /// Execute a compiled Qi program
    pub fn execute_program(&mut self, program_data: &[u8]) -> RuntimeResult<i32> {
        if self.state != RuntimeState::Ready {
            return Err(RuntimeError::program_execution_error(
                format!(
                    "运行时状态不正确，当前状态: {:?}，期望状态: Ready",
                    self.state
                ),
                "程序执行错误".to_string()
            ));
        }

        self.state = RuntimeState::Running;
        let execution_start = Instant::now();

        // TODO: Implement actual program execution logic
        // For now, simulate successful execution
        let result = self.simulate_program_execution(program_data)?;

        let execution_time = execution_start.elapsed();
        self.metrics.total_execution_time += execution_time;
        self.metrics.programs_executed += 1;

        self.state = RuntimeState::Ready;
        Ok(result)
    }

    /// Terminate the runtime environment and cleanup resources
    pub fn terminate(&mut self) -> RuntimeResult<()> {
        self.state = RuntimeState::ShuttingDown;

        // Trigger garbage collection
        self.memory_manager.trigger_gc()?;

        // Cleanup network connections
        self.network_manager.cleanup()?;

        // Close file handles
        self.file_system.cleanup()?;

        self.state = RuntimeState::Terminated;
        Ok(())
    }

    /// Get current runtime metrics
    pub fn get_metrics(&self) -> &RuntimeMetrics {
        &self.metrics
    }

    /// Update memory usage metrics
    pub fn update_memory_metrics(&mut self) {
        self.metrics.memory_usage_mb = self.memory_manager.get_current_usage_mb();
        self.metrics.peak_memory_mb = self.metrics.peak_memory_mb.max(self.metrics.memory_usage_mb);
    }

    /// Increment I/O operations counter
    pub fn increment_io_operations(&mut self) {
        self.metrics.io_operations += 1;
    }

    /// Increment network operations counter
    pub fn increment_network_operations(&mut self) {
        self.metrics.network_operations += 1;
    }

    /// Increment garbage collection counter
    pub fn increment_gc_collections(&mut self) {
        self.metrics.gc_collections += 1;
    }

    /// Increment errors encountered counter
    pub fn increment_errors(&mut self) {
        self.metrics.errors_encountered += 1;
    }

    /// Simulate program execution (placeholder implementation)
    fn simulate_program_execution(&mut self, program_data: &[u8]) -> RuntimeResult<i32> {
        if self.config.debug_mode {
            println!("调试: 模拟执行程序，大小: {} 字节", program_data.len());
        }

        // Simulate some basic operations
        self.increment_io_operations();
        self.update_memory_metrics();

        // Return simulated exit code
        Ok(0)
    }
}

impl Drop for RuntimeEnvironment {
    fn drop(&mut self) {
        if self.state != RuntimeState::Terminated {
            let _ = self.terminate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert_eq!(config.max_memory_mb, 1024);
        assert_eq!(config.gc_threshold_percent, 0.8);
        assert_eq!(config.locale, "zh-CN");
    }

    #[test]
    fn test_runtime_state_transitions() {
        let config = RuntimeConfig::default();
        let mut runtime = RuntimeEnvironment::new(config).unwrap();

        assert_eq!(runtime.state, RuntimeState::Initializing);

        runtime.initialize().unwrap();
        assert_eq!(runtime.state, RuntimeState::Ready);

        let _result = runtime.execute_program(b"test program").unwrap();
        assert_eq!(runtime.state, RuntimeState::Ready);

        runtime.terminate().unwrap();
        assert_eq!(runtime.state, RuntimeState::Terminated);
    }

    #[test]
    fn test_runtime_metrics() {
        let config = RuntimeConfig::default();
        let mut runtime = RuntimeEnvironment::new(config).unwrap();

        runtime.initialize().unwrap();

        // Initial metrics
        assert_eq!(runtime.metrics.programs_executed, 0);
        assert_eq!(runtime.metrics.io_operations, 0);

        // Execute a program
        runtime.execute_program(b"test").unwrap();

        // Updated metrics
        assert_eq!(runtime.metrics.programs_executed, 1);
        assert_eq!(runtime.metrics.io_operations, 1);
    }

    #[test]
    fn test_runtime_debug_mode() {
        let mut config = RuntimeConfig::default();
        config.debug_mode = true;

        let mut runtime = RuntimeEnvironment::new(config).unwrap();
        runtime.initialize().unwrap();

        // Should not panic when executing in debug mode
        let result = runtime.execute_program(b"debug test");
        assert!(result.is_ok());
    }
}