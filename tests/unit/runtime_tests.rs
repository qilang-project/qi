//! Unit Tests for Runtime Environment
//!
//! Tests for the core runtime environment functionality.

use qi_runtime::{RuntimeEnvironment, RuntimeConfig, RuntimeState};

#[test]
fn test_runtime_config_default() {
    let config = RuntimeConfig::default();
    assert_eq!(config.max_memory_mb, 1024);
    assert_eq!(config.gc_threshold_percent, 0.8);
    assert_eq!(config.locale, "zh-CN");
    assert!(!config.debug_mode);
    assert!(config.enable_metrics);
}

#[test]
fn test_runtime_creation() {
    let config = RuntimeConfig::default();
    let runtime = RuntimeEnvironment::new(config);
    assert!(runtime.is_ok());

    let runtime = runtime.unwrap();
    assert_eq!(runtime.state, RuntimeState::Initializing);
    assert!(!runtime.id.to_string().is_empty());
}

#[test]
fn test_runtime_initialization() {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config).unwrap();

    // Initialize runtime
    let result = runtime.initialize();
    assert!(result.is_ok());
    assert_eq!(runtime.state, RuntimeState::Ready);
}

#[test]
fn test_runtime_program_execution() {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config).unwrap();
    runtime.initialize().unwrap();

    // Execute test program
    let program_data = b"test program";
    let result = runtime.execute_program(program_data);
    assert!(result.is_ok());

    let exit_code = result.unwrap();
    assert_eq!(exit_code, 0); // Simulated success
}

#[test]
fn test_runtime_termination() {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config).unwrap();
    runtime.initialize().unwrap();

    // Terminate runtime
    let result = runtime.terminate();
    assert!(result.is_ok());
    assert_eq!(runtime.state, RuntimeState::Terminated);
}

#[test]
fn test_runtime_metrics() {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config).unwrap();
    runtime.initialize().unwrap();

    // Initial metrics
    let metrics = runtime.get_metrics();
    assert_eq!(metrics.programs_executed, 0);
    assert_eq!(metrics.io_operations, 0);
    assert_eq!(metrics.network_operations, 0);

    // Execute program and check updated metrics
    runtime.execute_program(b"test").unwrap();
    let metrics = runtime.get_metrics();
    assert_eq!(metrics.programs_executed, 1);
    assert_eq!(metrics.io_operations, 1);
}

#[test]
fn test_runtime_debug_mode() {
    let mut config = RuntimeConfig::default();
    config.debug_mode = true;

    let mut runtime = RuntimeEnvironment::new(config).unwrap();
    runtime.initialize().unwrap();

    // Should execute without issues in debug mode
    let result = runtime.execute_program(b"debug test");
    assert!(result.is_ok());
}

#[test]
fn test_runtime_error_handling() {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config).unwrap();

    // Try to execute program without initialization
    let result = runtime.execute_program(b"test");
    assert!(result.is_err());
}