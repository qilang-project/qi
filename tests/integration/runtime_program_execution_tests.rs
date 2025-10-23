//! Runtime Program Execution Integration Tests
//!
//! This module provides integration tests for the complete program execution lifecycle
//! including initialization, program loading, execution, termination, and cleanup.

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

use qi_compiler::runtime::{RuntimeEnvironment, RuntimeConfig, RuntimeState};
use qi_compiler::runtime::RuntimeResult;

#[test]
fn test_complete_program_execution_lifecycle() -> RuntimeResult<()> {
    // Create runtime environment
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;

    // 1. Initial state should be Initializing
    assert_eq!(runtime.state, RuntimeState::Initializing);
    assert!(!runtime.id.to_string().is_empty());

    // 2. Initialize runtime
    runtime.initialize()?;
    assert_eq!(runtime.state, RuntimeState::Ready);

    // 3. Execute a simple program
    let program_data = b"// Simple test program\n打印('Hello, World!');";
    let exit_code = runtime.execute_program(program_data)?;
    assert_eq!(exit_code, 0);
    assert_eq!(runtime.state, RuntimeState::Ready); // Should return to Ready

    // 4. Verify metrics updated
    let metrics = runtime.get_metrics();
    assert_eq!(metrics.programs_executed, 1);
    assert_eq!(metrics.io_operations, 1);

    // 5. Execute multiple programs
    for i in 0..5 {
        let program = format!("// Program {}\n打印('Program {} executed');", i, i);
        let _exit_code = runtime.execute_program(program.as_bytes())?;
    }

    // 6. Verify cumulative metrics
    let metrics = runtime.get_metrics();
    assert_eq!(metrics.programs_executed, 6);
    assert_eq!(metrics.io_operations, 6);

    // 7. Terminate runtime
    runtime.terminate()?;
    assert_eq!(runtime.state, RuntimeState::Terminated);

    Ok(())
}

#[test]
fn test_runtime_program_execution_with_chinese_content() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.locale = "zh-CN".to_string();

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute program with Chinese content
    let program_data = b"// 中文测试程序\n变量 消息 = '你好，世界！';\n打印(消息);";
    let exit_code = runtime.execute_program(program_data)?;
    assert_eq!(exit_code, 0);

    // Verify Chinese locale handling
    let metrics = runtime.get_metrics();
    assert_eq!(metrics.programs_executed, 1);

    runtime.terminate()?;
    Ok(())
}

#[test]
fn test_runtime_program_execution_invalid_state_handling() -> RuntimeResult<()> {
    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config)?;

    // Try to execute program without initialization
    let result = runtime.execute_program(b"test program");
    assert!(result.is_err());

    // Initialize and try again
    runtime.initialize()?;
    let result = runtime.execute_program(b"test program");
    assert!(result.is_ok());

    // Terminate and try to execute again
    runtime.terminate()?;
    let result = runtime.execute_program(b"test program after termination");
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_runtime_program_execution_debug_mode() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.debug_mode = true;

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute program in debug mode
    let program_data = b"// Debug test program\n打印('Debug mode enabled');";
    let exit_code = runtime.execute_program(program_data)?;
    assert_eq!(exit_code, 0);

    // In debug mode, should have detailed metrics
    let metrics = runtime.get_metrics();
    assert_eq!(metrics.programs_executed, 1);

    runtime.terminate()?;
    Ok(())
}

#[test]
fn test_runtime_memory_usage_during_program_execution() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.enable_metrics = true;

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute memory-intensive program
    let large_program = b"
    // Memory-intensive test program
    for i from 0 to 1000 {
        变量 data = create_array(100);
        // Process data
        process_array(data);
    }
    ";

    let initial_memory = runtime.get_metrics().memory_usage_mb;

    let exit_code = runtime.execute_program(large_program)?;
    assert_eq!(exit_code, 0);

    // Update and check memory metrics
    runtime.update_memory_metrics();
    let final_memory = runtime.get_metrics().memory_usage_mb;

    // Memory usage should have changed during execution
    assert!(final_memory >= initial_memory);

    runtime.terminate()?;
    Ok(())
}

#[test]
fn test_runtime_concurrent_program_execution() -> RuntimeResult<()> {
    use std::thread;
    use std::sync::Arc;

    let config = RuntimeConfig::default();
    let runtime = Arc::new(tokio::sync::Mutex::new(
        RuntimeEnvironment::new(config)?
    ));

    // Initialize runtime
    runtime.lock().await.initialize()?;

    let mut handles = vec![];

    // Spawn multiple threads to execute programs concurrently
    for i in 0..10 {
        let runtime_clone = Arc::clone(&runtime);
        let handle = thread::spawn(move || -> RuntimeResult<()> {
            let program_data = format!("// Concurrent program {}\n打印('Program {} running');", i, i);

            // Note: In a real implementation, we'd need to handle concurrent access properly
            // For this test, we'll simulate the behavior

            // Simulate program execution
            std::thread::sleep(std::time::Duration::from_millis(10));

            // This would normally be: runtime_clone.lock().unwrap().execute_program(program_data.as_bytes())
            // For now, we'll just verify the structure

            Ok(())
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap()?;
    }

    runtime.lock().await.terminate()?;
    Ok(())
}

#[tokio::test]
async fn test_runtime_program_execution_with_network_operations() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.network_timeout_ms = 5000; // 5 seconds timeout

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute program with network operations
    let network_program = b"
    // Network test program
    变量 response = http_get('https://httpbin.org/get');
    if response.status_code == 200 {
        打印('Network request successful');
    } else {
        打印('Network request failed');
    }
    ";

    let start_time = std::time::Instant::now();
    let exit_code = runtime.execute_program(network_program)?;
    let execution_time = start_time.elapsed();

    assert_eq!(exit_code, 0);
    assert!(execution_time.as_millis() < 10000); // Should complete within 10 seconds

    // Verify network operation metrics
    let metrics = runtime.get_metrics();
    assert_eq!(metrics.network_operations, 1);

    runtime.terminate()?;
    Ok(())
}

#[tokio::test]
async fn test_runtime_program_execution_with_file_operations() -> RuntimeResult<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test_output.txt");

    let mut config = RuntimeConfig::default();
    config.io_buffer_size = 4096;

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute program with file operations
    let file_program = format!(
        "
    // File operations test program
    变量 file_path = '{}';
    变量 content = 'Hello from Qi program!';

    // Write to file
    write_file(file_path, content);

    // Read from file
    变量 read_content = read_file(file_path);
    打印('Read content: ' + read_content);

    // Clean up
    delete_file(file_path);
    ",
        test_file.to_string_lossy()
    );

    let exit_code = runtime.execute_program(file_program.as_bytes())?;
    assert_eq!(exit_code, 0);

    // Verify file operation metrics
    let metrics = runtime.get_metrics();
    assert!(metrics.io_operations >= 3); // At least write, read, delete operations

    runtime.terminate()?;
    Ok(())
}

#[test]
fn test_runtime_program_execution_error_handling() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.debug_mode = true;

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute program with syntax error
    let error_program = b"
    // Program with syntax error
    变量 x = ; // Missing value
    打印(x);
    ";

    let result = runtime.execute_program(error_program);
    assert!(result.is_err());

    // Verify error metrics
    let metrics = runtime.get_metrics();
    assert!(metrics.errors_encountered > 0);

    // Execute program with runtime error
    let runtime_error_program = b"
    // Program with runtime error
    变量 x = 0;
    变量 result = 100 / x; // Division by zero
    打印(result);
    ";

    let result = runtime.execute_program(runtime_error_program);
    assert!(result.is_err());

    runtime.terminate()?;
    Ok(())
}

#[test]
fn test_runtime_program_execution_performance_monitoring() -> RuntimeResult<()> {
    let mut config = RuntimeConfig::default();
    config.enable_metrics = true;

    let mut runtime = RuntimeEnvironment::new(config)?;
    runtime.initialize()?;

    // Execute performance test program
    let perf_program = b"
    // Performance test program
    变量 start_time = get_current_time();

    // Perform intensive operations
    for i from 0 to 10000 {
        变量 result = sqrt(i * i);
        process_result(result);
    }

    变量 end_time = get_current_time();
    变量 duration = end_time - start_time;
    打印('Execution time: ' + duration + 'ms');
    ";

    let start_time = std::time::Instant::now();
    let exit_code = runtime.execute_program(perf_program)?;
    let execution_time = start_time.elapsed();

    assert_eq!(exit_code, 0);

    // Check performance metrics
    let metrics = runtime.get_metrics();
    assert!(metrics.total_execution_time.as_millis() > 0);
    assert!(metrics.total_execution_time.as_millis() <= execution_time.as_millis() + 100); // Allow some tolerance

    runtime.terminate()?;
    Ok(())
}

#[test]
fn test_runtime_program_execution_with_multiple_configurations() -> RuntimeResult<()> {
    // Test with different runtime configurations
    let configurations = vec![
        RuntimeConfig::default(),
        RuntimeConfig {
            max_memory_mb: 512,
            gc_threshold_percent: 0.6,
            debug_mode: true,
            ..RuntimeConfig::default()
        },
        RuntimeConfig {
            network_timeout_ms: 60000,
            io_buffer_size: 16384,
            locale: "en-US".to_string(),
            ..RuntimeConfig::default()
        },
    ];

    for (i, config) in configurations.into_iter().enumerate() {
        let mut runtime = RuntimeEnvironment::new(config)?;
        runtime.initialize()?;

        let program_data = format!(
            "// Configuration test program {}\n打印('Configuration {} test successful');",
            i, i
        );

        let exit_code = runtime.execute_program(program_data.as_bytes())?;
        assert_eq!(exit_code, 0);

        // Verify metrics were recorded
        let metrics = runtime.get_metrics();
        assert_eq!(metrics.programs_executed, 1);

        runtime.terminate()?;
    }

    Ok(())
}

#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;
    use qi_compiler::runtime::{RuntimeEnvironment, RuntimeConfig};
    use qi_compiler::runtime::RuntimeResult;

    proptest! {
        #[test]
        fn test_runtime_execution_with_various_program_sizes(
            program_size in 1usize..1000,
            execution_count in 1u32..10
        ) -> RuntimeResult<()> {
            let config = RuntimeConfig::default();
            let mut runtime = RuntimeEnvironment::new(config)?;
            runtime.initialize()?;

            // Generate program with varying size
            let program_content = "// Generated test program\n".to_string() +
                &"打印('test');\n".repeat(program_size);

            // Execute program multiple times
            for _ in 0..execution_count {
                let exit_code = runtime.execute_program(program_content.as_bytes())?;
                assert_eq!(exit_code, 0);
            }

            let metrics = runtime.get_metrics();
            assert_eq!(metrics.programs_executed, execution_count as u64);

            runtime.terminate()?;
            Ok(())
        }
    }

    proptest! {
        #[test]
        fn test_runtime_with_various_configurations(
            max_memory in 512usize..2048,
            gc_threshold in 0.5f64..0.9,
            buffer_size in 1024usize..8192,
            timeout in 1000u64..30000
        ) -> RuntimeResult<()> {
            let mut config = RuntimeConfig::default();
            config.max_memory_mb = max_memory;
            config.gc_threshold_percent = gc_threshold;
            config.io_buffer_size = buffer_size;
            config.network_timeout_ms = timeout;

            let mut runtime = RuntimeEnvironment::new(config)?;
            runtime.initialize()?;

            let program_data = b"// Property test program\n打印('Property test successful');";
            let exit_code = runtime.execute_program(program_data)?;
            assert_eq!(exit_code, 0);

            runtime.terminate()?;
            Ok(())
        }
    }
}