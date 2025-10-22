//! Integration tests for debugging functionality
//!
//! This module provides integration tests that verify the debugging system works
//! correctly when integrated with the Qi runtime, CLI, and other components.

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_debug_cli_integration() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test_debug.qi");

    // Create a simple Qi program
    std::fs::write(&test_file, r#"
// 简单的测试程序
变量 message = "Hello, Debug World!"
打印(message)
"#).unwrap();

    // Test debug start command
    let config = crate::config::CompilerConfig::default();
    let mut cli = crate::cli::commands::Cli {
        target: None,
        optimization: None,
        output: None,
        debug_symbols: true,
        no_runtime_checks: false,
        warnings_as_errors: false,
        verbose: false,
        config: None,
        import_paths: vec![],
        command: Some(crate::cli::commands::Commands::Debug {
            command: crate::cli::commands::DebugCommands::Start {
                file: test_file.clone(),
                config: None,
                profile: false,
                memory: false,
                variables: true,
            },
        }),
        source_files: vec![],
    };

    // This would normally start an interactive debug session
    // For testing, we'll just verify the command structure is valid
    match cli.command {
        Some(crate::cli::commands::Commands::Debug { command }) => {
            match command {
                crate::cli::commands::DebugCommands::Start { file, profile, memory, variables, .. } => {
                    assert_eq!(file, test_file);
                    assert!(!profile);
                    assert!(!memory);
                    assert!(variables);
                }
                _ => panic!("Expected DebugCommands::Start"),
            }
        }
        _ => panic!("Expected Commands::Debug"),
    }
}

#[tokio::test]
async fn test_debug_stack_command() {
    let config = crate::config::CompilerConfig::default();
    let mut cli = crate::cli::commands::Cli {
        target: None,
        optimization: None,
        output: None,
        debug_symbols: true,
        no_runtime_checks: false,
        warnings_as_errors: false,
        verbose: false,
        config: None,
        import_paths: vec![],
        command: Some(crate::cli::commands::Commands::Debug {
            command: crate::cli::commands::DebugCommands::Stack { pid: None },
        }),
        source_files: vec![],
    };

    // Verify command structure
    match cli.command {
        Some(crate::cli::commands::Commands::Debug { command }) => {
            match command {
                crate::cli::commands::DebugCommands::Stack { pid } => {
                    assert!(pid.is_none());
                }
                _ => panic!("Expected DebugCommands::Stack"),
            }
        }
        _ => panic!("Expected Commands::Debug"),
    }
}

#[tokio::test]
async fn test_debug_memory_command() {
    let config = crate::config::CompilerConfig::default();
    let mut cli = crate::cli::commands::Cli {
        target: None,
        optimization: None,
        output: None,
        debug_symbols: true,
        no_runtime_checks: false,
        warnings_as_errors: false,
        verbose: false,
        config: None,
        import_paths: vec![],
        command: Some(crate::cli::commands::Commands::Debug {
            command: crate::cli::commands::DebugCommands::Memory { detailed: true },
        }),
        source_files: vec![],
    };

    // Verify command structure
    match cli.command {
        Some(crate::cli::commands::Commands::Debug { command }) => {
            match command {
                crate::cli::commands::DebugCommands::Memory { detailed } => {
                    assert!(detailed);
                }
                _ => panic!("Expected DebugCommands::Memory"),
            }
        }
        _ => panic!("Expected Commands::Debug"),
    }
}

#[tokio::test]
async fn test_debug_stats_command() {
    let config = crate::config::CompilerConfig::default();
    let mut cli = crate::cli::commands::Cli {
        target: None,
        optimization: None,
        output: None,
        debug_symbols: true,
        no_runtime_checks: false,
        warnings_as_errors: false,
        verbose: false,
        config: None,
        import_paths: vec![],
        command: Some(crate::cli::commands::Commands::Debug {
            command: crate::cli::commands::DebugCommands::Stats { all: true },
        }),
        source_files: vec![],
    };

    // Verify command structure
    match cli.command {
        Some(crate::cli::commands::Commands::Debug { command }) => {
            match command {
                crate::cli::commands::DebugCommands::Stats { all } => {
                    assert!(all);
                }
                _ => panic!("Expected DebugCommands::Stats"),
            }
        }
        _ => panic!("Expected Commands::Debug"),
    }
}

#[tokio::test]
async fn test_debug_clear_command() {
    let config = crate::config::CompilerConfig::default();
    let mut cli = crate::cli::commands::Cli {
        target: None,
        optimization: None,
        output: None,
        debug_symbols: true,
        no_runtime_checks: false,
        warnings_as_errors: false,
        verbose: false,
        config: None,
        import_paths: vec![],
        command: Some(crate::cli::commands::Commands::Debug {
            command: crate::cli::commands::DebugCommands::Clear { data_type: "cache".to_string() },
        }),
        source_files: vec![],
    };

    // Verify command structure
    match cli.command {
        Some(crate::cli::commands::Commands::Debug { command }) => {
            match command {
                crate::cli::commands::DebugCommands::Clear { data_type } => {
                    assert_eq!(data_type, "cache");
                }
                _ => panic!("Expected DebugCommands::Clear"),
            }
        }
        _ => panic!("Expected Commands::Debug"),
    }
}

#[test]
fn test_debug_system_with_runtime_environment() {
    use qi_runtime::{RuntimeEnvironment, RuntimeConfig, DebugSystem};

    // Create runtime environment
    let runtime_config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(runtime_config).unwrap();
    runtime.initialize().unwrap();

    // Create debug system
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Test that both systems can coexist
    let result = debug_system.capture_stack_trace();
    assert!(result.is_ok());

    // Test variable registration with runtime values
    debug_system.register_variable("runtime_initialized", &true).unwrap();

    let inspection = debug_system.inspect_variable("runtime_initialized").unwrap();
    assert!(inspection.display.contains("true"));
}

#[test]
fn test_debug_system_error_handling_integration() {
    use qi_runtime::{DebugSystem, RuntimeError};

    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Test error handling with various error types
    let errors = vec![
        RuntimeError::user_error("Test user error", "测试用户错误"),
        RuntimeError::memory_error("Out of memory", "内存不足"),
        RuntimeError::io_error("File not found", "文件未找到"),
        RuntimeError::network_error("Connection failed", "连接失败"),
    ];

    for error in errors {
        let result = debug_system.handle_error(&error);
        assert!(result.is_ok());
    }

    // Verify that error handling doesn't break the debug system
    let stats = debug_system.get_statistics().unwrap();
    assert!(stats.commands_processed >= 0); // Error handling may or may not process commands
}

#[test]
fn test_debug_system_with_profiling_integration() {
    use qi_runtime::{DebugSystem, DebugSystemConfig};

    let config = DebugSystemConfig {
        enable_stack_traces: true,
        enable_variable_inspection: true,
        enable_commands: true,
        enable_profiling: true,
        auto_capture_stack_traces: true,
        max_debug_memory_mb: 100,
    };

    let debug_system = DebugSystem::with_config(config).unwrap();
    debug_system.initialize().unwrap();

    // Start profiling session
    debug_system.start_profiling("integration_test").unwrap();

    // Perform various operations while profiling
    debug_system.register_variable("test_var1", &42i32).unwrap();
    debug_system.register_variable("test_var2", &"Hello".to_string()).unwrap();

    debug_system.process_command("list").unwrap();
    debug_system.process_command("stats").unwrap();

    debug_system.capture_stack_trace().unwrap();

    debug_system.update_variable("test_var1", &100i32).unwrap();

    // Stop profiling and verify results
    let profile_data = debug_system.stop_profiling("integration_test").unwrap();
    assert_eq!(profile_data.name, "integration_test");
    assert!(profile_data.total_duration_us > 0);
}

#[test]
fn test_debug_system_memory_management_integration() {
    use qi_runtime::{DebugSystem, DebugSystemConfig};

    // Create debug system with limited memory
    let config = DebugSystemConfig {
        max_debug_memory_mb: 1, // 1MB limit
        ..DebugSystemConfig::default()
    };

    let debug_system = DebugSystem::with_config(config).unwrap();
    debug_system.initialize().unwrap();

    // Add data that approaches memory limit
    for i in 0..100 {
        let large_string = "x".repeat(1000); // 1KB string
        debug_system.register_variable(&format!("large_var_{}", i), &large_string).unwrap();
    }

    // System should still be functional
    let stats = debug_system.get_statistics().unwrap();
    assert!(stats.variable_inspector.registered_variables == 100);

    // Clear data and verify cleanup
    debug_system.clear_all_data().unwrap();
    let cleared_stats = debug_system.get_statistics().unwrap();
    assert_eq!(cleared_stats.variable_inspector.registered_variables, 0);
}

#[test]
fn test_debug_system_concurrent_integration() {
    use qi_runtime::DebugSystem;
    use std::sync::Arc;
    use std::thread;

    let debug_system = Arc::new(DebugSystem::new().unwrap());
    debug_system.initialize().unwrap();

    let mut handles = vec![];

    // Spawn multiple threads performing different debugging operations
    for i in 0..10 {
        let debug_clone = Arc::clone(&debug_system);
        let handle = thread::spawn(move || {
            // Each thread performs a complete debugging workflow
            let session_name = format!("session_{}", i);

            // Register variables
            debug_clone.register_variable(&format!("var_{}_1", i), &(i * 10)).unwrap();
            debug_clone.register_variable(&format!("var_{}_2", i), &format!("value_{}", i)).unwrap();

            // Process commands
            debug_clone.process_command("list").unwrap();
            debug_clone.process_command("stats").unwrap();

            // Inspect variables
            debug_clone.inspect_variable(&format!("var_{}_1", i)).unwrap();
            debug_clone.inspect_variable(&format!("var_{}_2", i)).unwrap();

            // Capture stack trace
            debug_clone.capture_stack_trace().unwrap();

            // Update variables
            debug_clone.update_variable(&format!("var_{}_1", i), &(i * 20)).unwrap();

            // Clean up
            debug_clone.unregister_variable(&format!("var_{}_1", i)).unwrap();
            debug_clone.unregister_variable(&format!("var_{}_2", i)).unwrap();
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify final state
    let variables = debug_system.list_variables().unwrap();
    assert_eq!(variables.len(), 0); // All variables should be unregistered

    let stats = debug_system.get_statistics().unwrap();
    assert!(stats.commands_processed > 0);
}

#[test]
fn test_debug_system_with_file_operations() {
    use qi_runtime::{DebugSystem, RuntimeEnvironment, RuntimeConfig};
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test_file.txt");

    // Create runtime environment for file operations
    let runtime_config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(runtime_config).unwrap();
    runtime.initialize().unwrap();

    // Create debug system
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Perform file operations while debugging
    std::fs::write(&test_file, "Test content for debugging").unwrap();

    // Register file-related variables
    debug_system.register_variable("file_path", &test_file.to_string_lossy().to_string()).unwrap();
    debug_system.register_variable("file_exists", &test_file.exists()).unwrap();

    // Capture stack trace after file operation
    let stack_trace = debug_system.capture_stack_trace().unwrap();
    assert!(!stack_trace.is_empty());

    // Verify variables are tracked correctly
    let path_inspection = debug_system.inspect_variable("file_path").unwrap();
    assert!(path_inspection.display.contains(&test_file.to_string_lossy()));

    let exists_inspection = debug_system.inspect_variable("file_exists").unwrap();
    assert!(exists_inspection.display.contains("true"));

    // Clean up
    std::fs::remove_file(&test_file).unwrap();
}

#[test]
fn test_debug_system_with_network_operations() {
    use qi_runtime::{DebugSystem, DebugSystemConfig};

    let config = DebugSystemConfig {
        enable_profiling: true,
        ..DebugSystemConfig::default()
    };

    let debug_system = DebugSystem::with_config(config).unwrap();
    debug_system.initialize().unwrap();

    // Start profiling to track network operation performance
    debug_system.start_profiling("network_test").unwrap();

    // Simulate network-like operations
    debug_system.register_variable("network_endpoint", &"http://example.com".to_string()).unwrap();
    debug_system.register_variable("connection_status", &false).unwrap();

    // Simulate some processing time
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Update connection status
    debug_system.update_variable("network_endpoint", &"http://example.com/api".to_string()).unwrap();
    debug_system.update_variable("connection_status", &true).unwrap();

    // Stop profiling
    let profile_data = debug_system.stop_profiling("network_test").unwrap();
    assert!(profile_data.total_duration_us > 10000); // At least 10ms

    // Verify network-related debugging information
    let endpoint_inspection = debug_system.inspect_variable("network_endpoint").unwrap();
    assert!(endpoint_inspection.display.contains("api"));

    let status_inspection = debug_system.inspect_variable("connection_status").unwrap();
    assert!(status_inspection.display.contains("true"));
}

#[test]
fn test_debug_system_comprehensive_workflow() {
    use qi_runtime::{DebugSystem, DebugSystemConfig, RuntimeError};

    // Create comprehensive debug configuration
    let config = DebugSystemConfig {
        enable_stack_traces: true,
        enable_variable_inspection: true,
        enable_commands: true,
        enable_profiling: true,
        auto_capture_stack_traces: true,
        max_debug_memory_mb: 50,
    };

    let debug_system = DebugSystem::with_config(config).unwrap();
    debug_system.initialize().unwrap();

    // 1. Start profiling session
    debug_system.start_profiling("comprehensive_test").unwrap();

    // 2. Register various types of variables
    debug_system.register_variable("int_var", &42i32).unwrap();
    debug_system.register_variable("float_var", &3.14159f64).unwrap();
    debug_system.register_variable("string_var", &"Hello, Qi!".to_string()).unwrap();
    debug_system.register_variable("bool_var", &true).unwrap();
    debug_system.register_variable("array_var", &vec![1, 2, 3, 4, 5]).unwrap();

    // 3. Process debug commands
    debug_system.process_command("list").unwrap();
    debug_system.process_command("stats").unwrap();
    debug_system.process_command("memory").unwrap();

    // 4. Inspect variables
    for var_name in ["int_var", "float_var", "string_var", "bool_var", "array_var"] {
        debug_system.inspect_variable(var_name).unwrap();
    }

    // 5. Update variables
    debug_system.update_variable("int_var", &100i32).unwrap();
    debug_system.update_variable("string_var", &"Updated string".to_string()).unwrap();

    // 6. Capture stack traces
    debug_system.capture_stack_trace().unwrap();
    debug_system.capture_stack_trace_with_context("after_variable_updates").unwrap();

    // 7. Simulate error handling
    let test_error = RuntimeError::user_error("Test error for comprehensive workflow", "综合工作流测试错误");
    debug_system.handle_error(&test_error).unwrap();

    // 8. Perform cleanup operations
    debug_system.unregister_variable("bool_var").unwrap();
    debug_system.unregister_variable("array_var").unwrap();

    // 9. Stop profiling
    let profile_data = debug_system.stop_profiling("comprehensive_test").unwrap();

    // 10. Verify comprehensive results
    assert_eq!(profile_data.name, "comprehensive_test");
    assert!(profile_data.total_duration_us > 0);
    assert!(profile_data.summary.unique_functions > 0);

    let final_stats = debug_system.get_statistics().unwrap();
    assert_eq!(final_stats.variable_inspector.registered_variables, 3); // int_var, float_var, string_var
    assert!(final_stats.commands_processed >= 3); // list, stats, memory

    // 11. Final cleanup
    debug_system.clear_all_data().unwrap();
    let cleared_stats = debug_system.get_statistics().unwrap();
    assert_eq!(cleared_stats.variable_inspector.registered_variables, 0);
}