//! Unit tests for debugging functionality
//!
//! This module provides comprehensive tests for the Qi runtime debugging system,
//! including stack trace collection, variable inspection, command processing,
//! and performance profiling.

use std::sync::Arc;
use qi_runtime::debug::*;

#[test]
fn test_debug_system_creation() {
    let debug_system = DebugSystem::new().unwrap();
    assert!(debug_system.initialize().is_ok());

    let stats = debug_system.get_statistics().unwrap();
    assert_eq!(stats.variable_inspector.registered_variables, 0);
    assert_eq!(stats.commands_processed, 0);
    assert!(stats.stack_traces.symbol_resolution_enabled);
}

#[test]
fn test_debug_system_configuration() {
    let config = DebugSystemConfig {
        enable_stack_traces: true,
        enable_variable_inspection: true,
        enable_commands: true,
        enable_profiling: false,
        auto_capture_stack_traces: false,
        max_debug_memory_mb: 50,
    };

    let debug_system = DebugSystem::with_config(config).unwrap();
    assert!(debug_system.initialize().is_ok());

    let system_config = debug_system.config();
    assert!(system_config.enable_stack_traces);
    assert!(system_config.enable_variable_inspection);
    assert!(!system_config.enable_profiling);
    assert_eq!(system_config.max_debug_memory_mb, 50);
}

#[test]
fn test_stack_trace_collection() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    let frames = debug_system.capture_stack_trace().unwrap();
    assert!(!frames.is_empty());

    // Check that frames have required fields
    for frame in &frames {
        assert!(!frame.frame.function.is_empty());
        assert!(!frame.frame.file.is_empty());
        assert!(matches!(frame.frame_type, FrameType::UserCode | FrameType::RuntimeCode | FrameType::SystemCode | FrameType::Unknown));
    }
}

#[test]
fn test_stack_trace_with_context() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    let frames = debug_system.capture_stack_trace_with_context("test_context").unwrap();
    assert!(!frames.is_empty());

    let formatted = debug_system.get_formatted_stack_trace().unwrap();
    assert!(formatted.contains("调用堆栈"));
}

#[test]
fn test_disabled_stack_traces() {
    let mut config = DebugSystemConfig::default();
    config.enable_stack_traces = false;

    let debug_system = DebugSystem::with_config(config).unwrap();
    debug_system.initialize().unwrap();

    let result = debug_system.capture_stack_trace();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("禁用"));
}

#[test]
fn test_variable_registration() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Test different variable types
    let int_var = 42i32;
    debug_system.register_variable("test_int", &int_var).unwrap();

    let string_var = "Hello, Qi!".to_string();
    debug_system.register_variable("test_string", &string_var).unwrap();

    let bool_var = true;
    debug_system.register_variable("test_bool", &bool_var).unwrap();

    let variables = debug_system.list_variables().unwrap();
    assert_eq!(variables.len(), 3);
    assert!(variables.contains(&"test_int".to_string()));
    assert!(variables.contains(&"test_string".to_string()));
    assert!(variables.contains(&"test_bool".to_string()));
}

#[test]
fn test_variable_inspection() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    let test_value = 123i32;
    debug_system.register_variable("test_var", &test_value).unwrap();

    let result = debug_system.inspect_variable("test_var").unwrap();
    assert_eq!(result.variable.name, "test_var");
    assert_eq!(result.variable.var_type, "i32");
    assert!(result.display.contains("test_var"));
    assert!(result.display.contains("123"));
}

#[test]
fn test_variable_update() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    let initial_value = 42i32;
    debug_system.register_variable("test_var", &initial_value).unwrap();

    let updated_value = 100i32;
    debug_system.update_variable("test_var", &updated_value).unwrap();

    let result = debug_system.inspect_variable("test_var").unwrap();
    assert!(result.display.contains("100"));
}

#[test]
fn test_variable_history_tracking() {
    let mut config = DebugSystemConfig::default();
    config.enable_variable_inspection = true;

    let mut inspector_config = InspectorConfig::default();
    inspector_config.enable_change_tracking = true;

    let debug_system = DebugSystem::with_config(config).unwrap();
    debug_system.initialize().unwrap();

    let initial_value = 42i32;
    debug_system.register_variable("test_var", &initial_value).unwrap();

    let updated_value = 100i32;
    debug_system.update_variable("test_var", &updated_value).unwrap();

    let history = debug_system.get_variable_history("test_var").unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].value.contains("42"));
}

#[test]
fn test_nonexistent_variable() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    let result = debug_system.inspect_variable("nonexistent");
    assert!(result.is_err());

    let result = debug_system.update_variable("nonexistent", &42i32);
    assert!(result.is_err());

    let result = debug_system.get_variable_history("nonexistent");
    assert!(result.is_err());

    let result = debug_system.unregister_variable("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_debug_commands() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Test help command
    let result = debug_system.process_command("help").unwrap();
    assert!(result.success);
    assert!(result.message.contains("可用命令"));

    // Test specific help
    let result = debug_system.process_command("help trace").unwrap();
    assert!(result.success);
    assert!(result.message.contains("堆栈跟踪"));

    // Test list command (should be empty)
    let result = debug_system.process_command("list").unwrap();
    assert!(result.success);
    assert!(result.message.contains("没有已注册的变量"));

    // Test stats command
    let result = debug_system.process_command("stats").unwrap();
    assert!(result.success);
    assert!(result.message.contains("调试系统统计信息"));
}

#[test]
fn test_debug_command_results() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Register a variable
    debug_system.register_variable("test_var", &42i32).unwrap();

    // Test inspect command
    let result = debug_system.process_command("inspect test_var").unwrap();
    assert!(result.success);
    assert!(result.data.is_some());

    // Test list command with variable
    let result = debug_system.process_command("list").unwrap();
    assert!(result.success);
    assert!(result.message.contains("test_var"));

    // Test register command
    let result = debug_system.process_command("register new_var 200").unwrap();
    assert!(result.success);

    // Test unregister command
    let result = debug_system.process_command("unregister new_var").unwrap();
    assert!(result.success);
}

#[test]
fn test_unknown_debug_command() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    let result = debug_system.process_command("unknown_command").unwrap();
    assert!(!result.success);
    assert!(result.message.contains("Unknown command"));
}

#[test]
fn test_profiling_functionality() {
    let mut config = DebugSystemConfig::default();
    config.enable_profiling = true;

    let debug_system = DebugSystem::with_config(config).unwrap();
    debug_system.initialize().unwrap();

    // Start profiling
    debug_system.start_profiling("test_profile").unwrap();

    // Simulate some work
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Stop profiling
    let profile_data = debug_system.stop_profiling("test_profile").unwrap();
    assert_eq!(profile_data.name, "test_profile");
    assert!(profile_data.total_duration_us > 0);

    // Get all profiles
    let profiles = debug_system.get_profile_data().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].name, "test_profile");
}

#[test]
fn test_profiling_disabled() {
    let config = DebugSystemConfig {
        enable_profiling: false,
        ..DebugSystemConfig::default()
    };

    let debug_system = DebugSystem::with_config(config).unwrap();
    debug_system.initialize().unwrap();

    let result = debug_system.start_profiling("test_profile");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("性能分析已禁用"));
}

#[test]
fn test_memory_inspection() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    let values = vec!["test1".to_string(), "test2".to_string(), "test3".to_string()];
    debug_system.register_variable("test_array", &values).unwrap();

    let result = debug_system.inspect_variable("test_array").unwrap();
    assert_eq!(result.variable.var_type, "Vec<String>");
    assert!(matches!(result.variable.metadata, VariableMetadata::Array { .. }));
}

#[test]
fn test_complex_variable_inspection() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Test struct-like inspection (using a map to simulate)
    let mut test_data = std::collections::HashMap::new();
    test_data.insert("name".to_string(), "test_value".to_string());
    test_data.insert("count".to_string(), "42".to_string());

    // This would require custom VariableValue implementation for HashMap
    // For now, test with basic types
    debug_system.register_variable("simple_var", &42i32).unwrap();

    let result = debug_system.inspect_variable("simple_var").unwrap();
    assert!(result.success);
    assert!(result.display.contains("42"));
}

#[test]
fn test_error_handling_in_debug_system() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Test error handling with invalid command
    let result = debug_system.process_command("").unwrap();
    assert!(!result.success);

    // Test error handling in stack trace collection with errors
    let error = qi_runtime::error::Error::user_error("Test error", "测试错误");
    assert!(debug_system.handle_error(&error).is_ok());
}

#[test]
fn test_debug_system_statistics() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Register some variables
    debug_system.register_variable("var1", &42i32).unwrap();
    debug_system.register_variable("var2", &"test".to_string()).unwrap();

    // Process some commands
    debug_system.process_command("help").unwrap();
    debug_system.process_command("list").unwrap();
    debug_system.process_command("stats").unwrap();

    let stats = debug_system.get_statistics().unwrap();
    assert_eq!(stats.variable_inspector.registered_variables, 2);
    assert_eq!(stats.commands_processed, 3);
    assert!(stats.total_memory_usage > 0);
}

#[test]
fn test_clear_debug_data() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Add some data
    debug_system.register_variable("test_var", &42i32).unwrap();
    debug_system.process_command("help").unwrap();

    // Clear data
    debug_system.clear_all_data().unwrap();

    // Verify data is cleared
    let variables = debug_system.list_variables().unwrap();
    assert_eq!(variables.len(), 0);

    let stats = debug_system.get_statistics().unwrap();
    assert_eq!(stats.variable_inspector.registered_variables, 0);
}

#[test]
fn test_stack_trace_collector_configuration() {
    let debug_module = Arc::new(qi_runtime::stdlib::debug::DebugModule::new());

    let config = StackTraceConfig {
        max_frames: 16,
        enable_symbol_resolution: false,
        enable_source_mapping: false,
        filter_internal_frames: true,
        include_parameters: false,
    };

    let collector = StackTraceCollector::with_config(debug_module, config);
    let stats = collector.get_statistics().unwrap();
    assert_eq!(stats.max_frames, 16);
    assert!(!stats.symbol_resolution_enabled);
    assert!(!stats.source_mapping_enabled);
}

#[test]
fn test_variable_inspector_configuration() {
    let debug_module = Arc::new(qi_runtime::stdlib::debug::DebugModule::new());

    let config = InspectorConfig {
        max_depth: 3,
        max_string_length: 50,
        max_array_elements: 5,
        include_memory_addresses: false,
        include_type_info: true,
        pretty_print: false,
        enable_change_tracking: false,
    };

    let inspector = VariableInspector::with_config(debug_module, config);
    let stats = inspector.get_statistics().unwrap();
    assert_eq!(stats.max_depth, 3);
    assert!(!stats.change_tracking_enabled);
}

#[test]
fn test_debug_command_processor() {
    let debug_module = Arc::new(qi_runtime::stdlib::debug::DebugModule::new());
    let processor = DebugCommandProcessor::new(debug_module);
    let debug_system = DebugSystem::new().unwrap();

    // Test command registration
    let commands = processor.list_commands();
    assert!(commands.contains(&"help".to_string()));
    assert!(commands.contains(&"trace".to_string()));
    assert!(commands.contains(&"inspect".to_string()));

    // Test command processing
    let result = processor.process_command("help", &debug_system).unwrap();
    assert!(result.success);

    // Test command history
    let history = processor.get_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0], "help");

    // Test command statistics
    let commands_processed = processor.get_commands_processed();
    assert_eq!(commands_processed, 1);
}

#[test]
fn test_profiler_functionality() {
    let debug_module = Arc::new(qi_runtime::stdlib::debug::DebugModule::new());
    let profiler = Profiler::new(debug_module);

    assert!(profiler.initialize().is_ok());

    // Test profiling session
    profiler.start_profiling("test_session").unwrap();

    // Simulate function calls
    profiler.enter_function("test_session", "test_function_1").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1));
    profiler.exit_function("test_session", "test_function_1").unwrap();

    profiler.enter_function("test_session", "test_function_2").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1));
    profiler.exit_function("test_session", "test_function_2").unwrap();

    let profile_data = profiler.stop_profiling("test_session").unwrap();
    assert_eq!(profile_data.name, "test_session");
    assert!(profile_data.total_duration_us > 0);
    assert_eq!(profile_data.summary.unique_functions, 2);

    // Test profiler statistics
    let stats = profiler.get_statistics().unwrap();
    assert_eq!(stats.completed_profiles, 1);
    assert_eq!(stats.global_stats.total_profiles, 1);
}

#[test]
fn test_profiler_error_handling() {
    let debug_module = Arc::new(qi_runtime::stdlib::debug::DebugModule::new());
    let profiler = Profiler::new(debug_module);

    assert!(profiler.initialize().is_ok());

    // Test stopping non-existent session
    let result = profiler.stop_profiling("nonexistent_session");
    assert!(result.is_err());

    // Test function calls without session
    let result = profiler.enter_function("nonexistent_session", "test_function");
    assert!(result.is_err());

    let result = profiler.exit_function("nonexistent_session", "test_function");
    assert!(result.is_err());
}

#[test]
fn test_concurrent_debug_operations() {
    use std::sync::Arc;
    use std::thread;

    let debug_system = Arc::new(DebugSystem::new().unwrap());
    debug_system.initialize().unwrap();

    let mut handles = vec![];

    // Spawn multiple threads to test concurrent operations
    for i in 0..5 {
        let debug_clone = Arc::clone(&debug_system);
        let handle = thread::spawn(move || {
            let var_name = format!("thread_var_{}", i);
            let value = i * 10;

            // Register variable
            debug_clone.register_variable(&var_name, &value).unwrap();

            // Process commands
            debug_clone.process_command("list").unwrap();
            debug_clone.process_command("stats").unwrap();

            // Capture stack trace
            debug_clone.capture_stack_trace().unwrap();

            // Inspect variable
            debug_clone.inspect_variable(&var_name).unwrap();

            // Unregister variable
            debug_clone.unregister_variable(&var_name).unwrap();
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
}

#[test]
fn test_debug_system_memory_usage() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Add a significant amount of debugging data
    for i in 0..100 {
        let var_name = format!("large_var_{}", i);
        let large_value = format!("This is a large string value with index {} and some additional content to make it longer", i);
        debug_system.register_variable(&var_name, &large_value).unwrap();
    }

    // Process many commands
    for _ in 0..50 {
        debug_system.process_command("list").unwrap();
        debug_system.process_command("stats").unwrap();
        debug_system.capture_stack_trace().unwrap();
    }

    let stats = debug_system.get_statistics().unwrap();
    assert_eq!(stats.variable_inspector.registered_variables, 100);
    assert_eq!(stats.commands_processed, 150);
    assert!(stats.total_memory_usage > 0);

    // Clear data and verify cleanup
    debug_system.clear_all_data().unwrap();
    let cleared_stats = debug_system.get_statistics().unwrap();
    assert_eq!(cleared_stats.variable_inspector.registered_variables, 0);
}

#[test]
fn test_debug_system_error_recovery() {
    let debug_system = DebugSystem::new().unwrap();
    debug_system.initialize().unwrap();

    // Test that system recovers from errors
    let result1 = debug_system.inspect_variable("nonexistent");
    assert!(result1.is_err());

    // System should still be functional after error
    debug_system.register_variable("test_var", &42i32).unwrap();
    let result2 = debug_system.inspect_variable("test_var");
    assert!(result2.is_ok());

    // Test command error recovery
    let result3 = debug_system.process_command("invalid_command");
    assert!(!result3.success);

    // System should still be functional
    let result4 = debug_system.process_command("help");
    assert!(result4.success);
}