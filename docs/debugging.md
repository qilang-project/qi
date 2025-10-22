# Qi Runtime Debugging Guide

This guide provides comprehensive documentation for the debugging capabilities of the Qi runtime environment.

## Overview

The Qi runtime includes a comprehensive debugging system that provides:

- **Stack Trace Collection**: Captures and displays call stack information with symbol resolution
- **Variable Inspection**: Real-time monitoring and inspection of runtime variables
- **Performance Profiling**: Function timing and memory usage analysis
- **Interactive Debug Commands**: Command-line interface for debugging operations
- **Chinese Language Support**: Full localization for debugging messages and commands

## Quick Start

### Basic Debugging

```bash
# Start debugging a Qi program
qi debug start program.qi

# Show current stack trace
qi debug stack

# Show memory usage information
qi debug memory

# Show debugging statistics
qi debug stats

# Clear debugging data
qi debug clear
```

### Interactive Debugging

```bash
# Start interactive debugging session with profiling
qi debug start --profile --variables --memory program.qi

# In the interactive debug shell:
debug> help                    # Show available commands
debug> list                    # List registered variables
debug> register x 42           # Register variable x with value 42
debug> inspect x               # Inspect variable x
debug> trace                   # Show stack trace
debug> stats                   # Show debugging statistics
debug> profile start test      # Start profiling session
debug> profile stop test       # Stop profiling and show results
debug> exit                    # Exit debugging mode
```

## Debug Commands Reference

### CLI Commands

#### `qi debug start <file>`
Start a debugging session for the specified Qi program.

**Options:**
- `--config <file>`: Debug configuration file path
- `--profile`: Enable performance profiling
- `--memory`: Enable memory tracking
- `--variables`: Enable variable tracking

**Example:**
```bash
qi debug start program.qi --profile --variables
```

#### `qi debug stack [--pid <id>]`
Display the current call stack trace.

**Options:**
- `--pid <id>`: Show stack trace for specific process ID (default: current process)

**Example:**
```bash
qi debug stack
qi debug stack --pid 12345
```

#### `qi debug memory [--detailed]`
Show memory usage information.

**Options:**
- `--detailed`: Show detailed memory information

**Example:**
```bash
qi debug memory
qi debug memory --detailed
```

#### `qi debug stats [--all]`
Display debugging system statistics.

**Options:**
- `--all`: Show all detailed statistics

**Example:**
```bash
qi debug stats
qi debug stats --all
```

#### `qi debug clear <type>`
Clear debugging data.

**Types:**
- `all`: Clear all debugging data
- `cache`: Clear symbol cache
- `history`: Clear command history
- `profiles`: Clear profiling data

**Example:**
```bash
qi debug clear all
qi debug clear cache
```

### Interactive Debug Commands

#### `help [command]`
Show help information for debugging commands.

**Examples:**
```bash
debug> help                    # Show all available commands
debug> help trace             # Show help for trace command
```

#### `trace` / `stack`
Display the current call stack trace.

**Example:**
```bash
debug> trace
debug> stack
```

#### `inspect <variable>`
Display detailed information about a registered variable.

**Example:**
```bash
debug> inspect my_variable
```

#### `list`
List all registered debugging variables.

**Example:**
```bash
debug> list
```

#### `register <variable> <value>`
Register a variable for debugging tracking.

**Supported value types:**
- Integers: `42`, `-10`
- Floats: `3.14`, `-0.5`
- Booleans: `true`, `false`
- Strings: `"hello"`, `"中文"`

**Examples:**
```bash
debug> register counter 42
debug> register message "Hello, Qi!"
debug> register enabled true
debug> register pi 3.14159
```

#### `unregister <variable>`
Remove a variable from debugging tracking.

**Example:**
```bash
debug> unregister old_variable
```

#### `clear`
Clear all debugging data.

**Example:**
```bash
debug> clear
```

#### `stats`
Show debugging system statistics.

**Example:**
```bash
debug> stats
```

#### `enable <feature>`
Enable a debugging feature.

**Features:**
- `stack_traces`: Stack trace collection
- `variable_inspection`: Variable inspection
- `profiling`: Performance profiling

**Example:**
```bash
debug> enable stack_traces
debug> enable profiling
```

#### `profile <action> [name]`
Control performance profiling.

**Actions:**
- `start [name]`: Start profiling session (default name: "default")
- `stop [name]`: Stop profiling session and show results
- `list`: List all profiling sessions

**Examples:**
```bash
debug> profile start my_test
debug> profile stop my_test
debug> profile list
```

#### `memory`
Display memory usage information.

**Example:**
```bash
debug> memory
```

#### `system`
Show system and runtime information.

**Example:**
```bash
debug> system
```

#### `exit` / `quit`
Exit debugging mode.

**Example:**
```bash
debug> exit
debug> quit
```

## Variable Inspection

### Supported Variable Types

The debugging system supports inspection of the following variable types:

#### Primitive Types
- **Integers**: `i32`, `i64`, etc.
- **Floats**: `f32`, `f64`
- **Booleans**: `bool`
- **Strings**: `String`

#### Composite Types
- **Arrays**: `Vec<T>`
- **Structs**: Custom structures
- **Maps**: `HashMap<K, V>`

### Variable Information Display

When inspecting a variable, the following information is shown:

- **Name**: Variable identifier
- **Type**: Data type (e.g., `i32`, `String`, `Vec<String>`)
- **Value**: Current value as string
- **Memory Address**: Memory location (if available)
- **Size**: Memory size in bytes
- **Metadata**: Type-specific information

### Variable Change Tracking

When change tracking is enabled, the debugging system maintains a history of variable changes:

```bash
debug> register counter 0
debug> update counter 10
debug> update counter 25
debug> inspect counter
```

The inspection will show the current value along with change history.

## Stack Trace Analysis

### Stack Trace Format

Stack traces are displayed in the following format:

```
调用堆栈:
  0. [用户代码] main_function (main.qi:42:10) @ 0x7ff123456789
     模块: main
  1. [运行时] qi_runtime_execute (runtime.rs:123:5) @ 0x7ff1234567ab
     模块: qi_runtime
  2. [系统] __libc_start_main (libc.so:0x12345) @ 0x7ff1234567cd
```

### Frame Types

- **用户代码 (UserCode)**: Qi program code
- **运行时 (RuntimeCode)**: Qi runtime system code
- **系统 (SystemCode)**: System/library code
- **未知 (Unknown)**: Unknown frame type

### Symbol Resolution

The debugging system attempts to resolve:
- Function names
- Source file locations
- Memory addresses
- Module information

## Performance Profiling

### Profiling Sessions

Start and stop profiling sessions to analyze performance:

```bash
debug> profile start my_function_test
# ... run code to profile ...
debug> profile stop my_function_test
```

### Profiling Results

Profiling results include:

- **Total Duration**: Execution time in microseconds
- **Function Calls**: Number of function calls made
- **Unique Functions**: Count of different functions called
- **Maximum Call Depth**: Deepest level of function nesting
- **Average Call Duration**: Mean execution time per function call
- **Memory Usage**: Memory allocation and deallocation statistics

### Memory Profiling

When memory profiling is enabled, the system tracks:
- Total memory allocated
- Memory deallocated
- Peak memory usage
- Allocation patterns by function

## Configuration

### Debug System Configuration

The debugging system can be configured with the following options:

```rust
DebugSystemConfig {
    enable_stack_traces: true,        // Enable stack trace collection
    enable_variable_inspection: true,  // Enable variable inspection
    enable_commands: true,             // Enable debug commands
    enable_profiling: false,           // Enable performance profiling
    auto_capture_stack_traces: true,  // Auto-capture stack traces on errors
    max_debug_memory_mb: 100,          // Maximum debug memory usage
}
```

### Stack Trace Configuration

```rust
StackTraceConfig {
    max_frames: 32,                   // Maximum frames to collect
    enable_symbol_resolution: true,    // Enable symbol resolution
    enable_source_mapping: true,       // Enable source file mapping
    filter_internal_frames: true,      // Filter runtime frames
    include_parameters: false,         // Include function parameters
}
```

### Variable Inspector Configuration

```rust
InspectorConfig {
    max_depth: 5,                     // Maximum inspection depth
    max_string_length: 100,           // Maximum string length to display
    max_array_elements: 10,           // Maximum array elements to show
    include_memory_addresses: true,    // Include memory addresses
    include_type_info: true,           // Include type information
    pretty_print: true,                // Pretty print output
    enable_change_tracking: false,     // Enable variable change tracking
}
```

## Error Handling

### Automatic Error Capture

When `auto_capture_stack_traces` is enabled, the debugging system automatically captures stack traces when runtime errors occur.

### Error Context

Errors are displayed with:
- Chinese error messages
- Stack trace information
- Context metadata
- Recovery suggestions

### Error Recovery

The debugging system provides recovery strategies for different error types:
- **Retry**: Attempt the operation again
- **Use Default**: Use a default value
- **Skip**: Skip the current operation
- **Abort**: Stop execution

## Integration with Qi Programs

### Programmatic Debugging

Qi programs can interact with the debugging system:

```qi
// Enable debugging in program
调试启用()

// Register variables for debugging
调试注册变量("counter", 42)
调试注册变量("message", "Hello, World!")

// Take stack snapshot
调试堆栈快照("important_point")

// Profile function execution
调试开始性能分析("my_function")
// ... function code ...
调试结束性能分析("my_function")

// Disable debugging
调试禁用()
```

### Debug Keywords

Qi provides Chinese debugging keywords:
- `调试启用()`: Enable debugging
- `调试禁用()`: Disable debugging
- `调试注册变量()`: Register variable
- `调试堆栈快照()`: Take stack snapshot
- `调试开始性能分析()`: Start profiling
- `调试结束性能分析()`: Stop profiling

## Performance Considerations

### Debugging Overhead

Debugging features introduce varying levels of performance overhead:

- **Stack Traces**: Low overhead when enabled, higher when captured
- **Variable Inspection**: Minimal overhead for basic types
- **Performance Profiling**: Moderate overhead during profiling
- **Change Tracking**: Higher overhead when enabled

### Memory Usage

The debugging system manages memory usage through:
- Automatic cache cleanup
- Configurable memory limits
- Data size restrictions
- Periodic garbage collection

### Production Usage

For production environments:
- Disable profiling to minimize overhead
- Use selective variable registration
- Configure appropriate memory limits
- Monitor debug system statistics

## Troubleshooting

### Common Issues

#### Debugging Not Working
- Ensure debugging is enabled in configuration
- Check that debug symbols are included during compilation
- Verify debug module is properly initialized

#### Stack Traces Missing
- Check `enable_stack_traces` configuration
- Ensure symbol resolution is enabled
- Verify stack trace collection depth limits

#### Variable Inspection Fails
- Confirm variable is properly registered
- Check variable type support
- Verify inspection depth limits

#### Profiling Not Working
- Ensure profiling is enabled in configuration
- Check that profiling sessions are properly started/stopped
- Verify profiler initialization

### Debug Information

To get detailed debug information:

```bash
qi debug stats --all
```

This shows:
- Debug system configuration
- Memory usage statistics
- Command processing statistics
- Error handling information

### Getting Help

For debugging issues:
1. Check the debug system statistics
2. Review configuration settings
3. Consult the error messages in Chinese
4. Use the `help` command in interactive mode

## Examples

### Example 1: Basic Debugging Session

```bash
# Start debugging with variable tracking
qi debug start program.qi --variables

# Interactive debug session
debug> register input 42
debug> register result "calculated_value"
debug> list
debug> inspect input
debug> trace
debug> stats
debug> exit
```

### Example 2: Performance Profiling

```bash
# Start debugging with profiling
qi debug start program.qi --profile

# Interactive debug session
debug> profile start algorithm_test
debug> register data_size 1000
debug> profile stop algorithm_test
debug> memory
debug> stats
debug> exit
```

### Example 3: Error Investigation

```bash
# After program crashes with error
qi debug stack
qi debug memory --detailed
qi debug stats --all
qi debug clear all
```

## API Reference

### Core Types

- `DebugSystem`: Main debugging system
- `StackTraceCollector`: Stack trace collection
- `VariableInspector`: Variable inspection
- `Profiler`: Performance profiling
- `DebugCommandProcessor`: Command processing

### Key Functions

- `create_debug_system()`: Create debugging system
- `capture_stack_trace()`: Capture current stack trace
- `inspect_variable()`: Inspect registered variable
- `process_command()`: Process debug command

### Configuration Types

- `DebugSystemConfig`: System-wide configuration
- `StackTraceConfig`: Stack trace configuration
- `InspectorConfig`: Variable inspection configuration
- `ProfileConfig`: Profiling configuration

For detailed API documentation, see the inline documentation in the source code.