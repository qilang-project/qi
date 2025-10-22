//! Runtime Environment Unit Tests
//!
//! This module provides comprehensive unit tests for the RuntimeEnvironment
//! including lifecycle management, state transitions, and configuration validation.

use qi_runtime::runtime::{RuntimeEnvironment, RuntimeConfig, RuntimeState};
use qi_runtime::runtime::RuntimeResult;

#[test]
fn test_runtime_config_creation() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();

    // Verify default configuration values
    assert_eq!(config.max_memory_mb, 1024);
    assert_eq!(config.gc_threshold_percent, 0.8);
    assert_eq!(config.io_buffer_size, 8192);
    assert_eq!(config.network_timeout_ms, 30000);
    assert!(!config.debug_mode);
    assert_eq!(config.locale, "zh-CN");
    assert!(config.enable_metrics);

    Ok(())
}

#[test]
fn test_runtime_config_customization() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.max_memory_mb = 2048;
    config.debug_mode = true;
    config.locale = "en-US".to_string();

    assert_eq!(config.max_memory_mb, 2048);
    assert!(config.debug_mode);
    assert_eq!(config.locale, "en-US");

    Ok(())
}

#[test]
fn test_runtime_environment_creation() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let runtime = RuntimeEnvironment::new(config)?;

    // Verify initial state
    assert_eq!(runtime.state, RuntimeState::Initializing);
    assert!(!runtime.id.to_string().is_empty());

    Ok(())
}

#[test]
fn test_runtime_environment_initialization() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;

    // Should be in Initializing state after creation
    assert_eq!(runtime.state, RuntimeState::Initializing);

    // Initialize the runtime
    runtime.initialize()?;

    // Should be in Ready state after initialization
    assert_eq!(runtime.state, RuntimeState::Ready);

    Ok(())
}

#[test]
fn test_runtime_environment_state_transitions() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;

    // Initial state
    assert_eq!(runtime.state, RuntimeState::Initializing);

    // Initialize
    runtime.initialize()?;
    assert_eq!(runtime.state, RuntimeState::Ready);

    // Execute program (simulate)
    let _result = runtime.execute_program(b"test program")?;
    assert_eq!(runtime.state, RuntimeState::Ready); // Should return to Ready after execution

    // Terminate
    runtime.terminate()?;
    assert_eq!(runtime.state, RuntimeState::Terminated);

    Ok(())
}

#[test]
fn test_runtime_program_execution_invalid_state() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;

    // Try to execute program without initialization
    let result = runtime.execute_program(b"test program");
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_runtime_metrics_initialization() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let runtime = RuntimeEnvironment::new(config)?;

    let metrics = runtime.get_metrics();

    // Verify initial metrics
    assert_eq!(metrics.programs_executed, 0);
    assert_eq!(metrics.io_operations, 0);
    assert_eq!(metrics.network_operations, 0);
    assert_eq!(metrics.gc_collections, 0);
    assert_eq!(metrics.errors_encountered, 0);

    Ok(())
}

#[test]
fn test_runtime_debug_mode() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.debug_mode = true;

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Should execute successfully in debug mode
    let result = runtime.execute_program(b"debug test program");
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn test_runtime_metrics_updates() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute a program to update metrics
    let _result = runtime.execute_program(b"test program")?;

    let metrics = runtime.get_metrics();

    // Should have updated metrics
    assert_eq!(metrics.programs_executed, 1);
    assert_eq!(metrics.io_operations, 1);

    Ok(())
}

#[test]
fn test_runtime_chinese_locale() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.locale = "zh-CN".to_string();

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Should work with Chinese locale
    let result = runtime.execute_program(b"中文测试程序");
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn test_runtime_multiple_program_executions() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute multiple programs
    for i in 0..5 {
        let program_data = format!("program {}", i);
        let _result = runtime.execute_program(program_data.as_bytes())?;
    }

    let metrics = runtime.get_metrics();
    assert_eq!(metrics.programs_executed, 5);
    assert_eq!(metrics.io_operations, 5);

    Ok(())
}

#[test]
fn test_runtime_cleanup_on_termination() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute some operations
    let _result = runtime.execute_program(b"test program")?;

    // Terminate should cleanup successfully
    let result = runtime.terminate();
    assert!(result.is_ok());
    assert_eq!(runtime.state, RuntimeState::Terminated);

    Ok(())
}

#[test]
fn test_runtime_error_handling() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Simulate an error scenario
    runtime.increment_errors();

    let metrics = runtime.get_metrics();
    assert_eq!(metrics.errors_encountered, 1);

    Ok(())
}

#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;
    use qi_runtime::runtime::{RuntimeEnvironment, RuntimeConfig};
    use qi_runtime::runtime::RuntimeResult;

    proptest! {
        #[test]
        fn test_runtime_creation_with_various_configs(
            max_memory in 512usize..4096,
            gc_threshold in 0.1f64..0.9,
            buffer_size in 1024usize..16384,
            timeout in 1000u64..60000
        ) -> RuntimeResult<()> {
            let mut config = RuntimeConfig::default();
            config.max_memory_mb = max_memory;
            config.gc_threshold_percent = gc_threshold;
            config.io_buffer_size = buffer_size;
            config.network_timeout_ms = timeout;

            let runtime = RuntimeEnvironment::new(config)?;
            assert_eq!(runtime.state, qi_runtime::runtime::RuntimeState::Initializing);

            Ok(())
        }
    }

    proptest! {
        #[test]
        fn test_multiple_program_executions(
            program_count in 1u32..10
        ) -> RuntimeResult<()> {
            let config = RuntimeConfig::default();
            let mut runtime = RuntimeEnvironment::new(config)?;
            runtime.initialize()?;

            for i in 0..program_count {
                let program_data = format!("test_program_{}", i);
                let _result = runtime.execute_program(program_data.as_bytes())?;
            }

            let metrics = runtime.get_metrics();
            assert_eq!(metrics.programs_executed, program_count as u64);

            Ok(())
        }
    }
}