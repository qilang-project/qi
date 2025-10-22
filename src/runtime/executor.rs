//! Program Executor
//!
//! This module provides the interface for executing compiled Qi programs
//! with the Rust runtime environment.

use std::ffi::{c_char, c_int, CStr};
use std::sync::{Mutex, Once};

use crate::runtime::{RuntimeEnvironment, RuntimeConfig, RuntimeResult};

static RUNTIME_INIT: Once = Once::new();
static mut RUNTIME: Option<Mutex<RuntimeEnvironment>> = None;

/// Initialize the Qi runtime
///
/// This function must be called before executing any Qi program.
/// It's safe to call multiple times - initialization only happens once.
#[no_mangle]
pub extern "C" fn qi_runtime_initialize() -> c_int {
    let mut result = 0;
    
    RUNTIME_INIT.call_once(|| {
        let config = RuntimeConfig::default();
        match RuntimeEnvironment::new(config) {
            Ok(mut runtime) => {
                if let Err(e) = runtime.initialize() {
                    eprintln!("Runtime 初始化失败: {}", e);
                    result = -1;
                    return;
                }
                unsafe {
                    RUNTIME = Some(Mutex::new(runtime));
                }
            }
            Err(e) => {
                eprintln!("Runtime 创建失败: {}", e);
                result = -1;
            }
        }
    });

    result
}

/// Shutdown the Qi runtime
#[no_mangle]
pub extern "C" fn qi_runtime_shutdown() -> c_int {
    unsafe {
        if let Some(runtime_mutex) = RUNTIME.take() {
            if let Ok(mut runtime) = runtime_mutex.lock() {
                match runtime.terminate() {
                    Ok(_) => 0,
                    Err(e) => {
                        eprintln!("Runtime 关闭失败: {}", e);
                        -1
                    }
                }
            } else {
                eprintln!("无法获取 runtime 锁");
                -1
            }
        } else {
            0 // Already shutdown or never initialized
        }
    }
}

/// Execute a Qi program
#[no_mangle]
pub extern "C" fn qi_runtime_execute(program_data: *const u8, data_len: usize) -> c_int {
    if program_data.is_null() {
        eprintln!("程序数据指针为空");
        return -1;
    }

    unsafe {
        let data_slice = std::slice::from_raw_parts(program_data, data_len);
        
        if let Some(runtime_mutex) = &RUNTIME {
            if let Ok(mut runtime) = runtime_mutex.lock() {
                match runtime.execute_program(data_slice) {
                    Ok(exit_code) => exit_code,
                    Err(e) => {
                        eprintln!("程序执行失败: {}", e);
                        runtime.increment_errors();
                        -1
                    }
                }
            } else {
                eprintln!("无法获取 runtime 锁");
                -1
            }
        } else {
            eprintln!("Runtime 未初始化");
            -1
        }
    }
}

/// Print a string (UTF-8)
#[no_mangle]
pub extern "C" fn qi_runtime_print(s: *const c_char) -> c_int {
    if s.is_null() {
        return -1;
    }

    unsafe {
        if let Ok(rust_str) = CStr::from_ptr(s).to_str() {
            print!("{}", rust_str);
            
            if let Some(runtime_mutex) = &RUNTIME {
                if let Ok(mut runtime) = runtime_mutex.lock() {
                    runtime.increment_io_operations();
                }
            }
            0
        } else {
            eprintln!("无效的 UTF-8 字符串");
            -1
        }
    }
}

/// Print a string with newline (UTF-8)
#[no_mangle]
pub extern "C" fn qi_runtime_println(s: *const c_char) -> c_int {
    if s.is_null() {
        return -1;
    }

    unsafe {
        if let Ok(rust_str) = CStr::from_ptr(s).to_str() {
            println!("{}", rust_str);
            
            if let Some(runtime_mutex) = &RUNTIME {
                if let Ok(mut runtime) = runtime_mutex.lock() {
                    runtime.increment_io_operations();
                }
            }
            0
        } else {
            eprintln!("无效的 UTF-8 字符串");
            -1
        }
    }
}

/// Print an integer
#[no_mangle]
pub extern "C" fn qi_runtime_print_int(value: i64) -> c_int {
    print!("{}", value);
    
    unsafe {
        if let Some(runtime_mutex) = &RUNTIME {
            if let Ok(mut runtime) = runtime_mutex.lock() {
                runtime.increment_io_operations();
            }
        }
    }
    0
}

/// Print an integer with newline
#[no_mangle]
pub extern "C" fn qi_runtime_println_int(value: i64) -> c_int {
    println!("{}", value);
    
    unsafe {
        if let Some(runtime_mutex) = &RUNTIME {
            if let Ok(mut runtime) = runtime_mutex.lock() {
                runtime.increment_io_operations();
            }
        }
    }
    0
}

/// Print a float
#[no_mangle]
pub extern "C" fn qi_runtime_print_float(value: f64) -> c_int {
    print!("{}", value);
    
    unsafe {
        if let Some(runtime_mutex) = &RUNTIME {
            if let Ok(mut runtime) = runtime_mutex.lock() {
                runtime.increment_io_operations();
            }
        }
    }
    0
}

/// Print a float with newline
#[no_mangle]
pub extern "C" fn qi_runtime_println_float(value: f64) -> c_int {
    println!("{}", value);
    
    unsafe {
        if let Some(runtime_mutex) = &RUNTIME {
            if let Ok(mut runtime) = runtime_mutex.lock() {
                runtime.increment_io_operations();
            }
        }
    }
    0
}

/// Allocate memory
#[no_mangle]
pub extern "C" fn qi_runtime_alloc(size: usize) -> *mut u8 {
    unsafe {
        if let Some(runtime_mutex) = &RUNTIME {
            if let Ok(mut runtime) = runtime_mutex.lock() {
                match runtime.memory_manager.allocate(size, None) {
                    Ok(ptr) => {
                        runtime.update_memory_metrics();
                        ptr
                    }
                    Err(e) => {
                        eprintln!("内存分配失败: {}", e);
                        std::ptr::null_mut()
                    }
                }
            } else {
                std::ptr::null_mut()
            }
        } else {
            // Fallback to standard allocation if runtime not initialized
            let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
            std::alloc::alloc(layout)
        }
    }
}

/// Deallocate memory
#[no_mangle]
pub extern "C" fn qi_runtime_dealloc(ptr: *mut u8, size: usize) -> c_int {
    if ptr.is_null() {
        return -1;
    }

    unsafe {
        if let Some(runtime_mutex) = &RUNTIME {
            if let Ok(mut runtime) = runtime_mutex.lock() {
                match runtime.memory_manager.deallocate(ptr) {
                    Ok(_) => {
                        runtime.update_memory_metrics();
                        0
                    }
                    Err(e) => {
                        eprintln!("内存释放失败: {}", e);
                        -1
                    }
                }
            } else {
                -1
            }
        } else {
            // Fallback to standard deallocation
            let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
            std::alloc::dealloc(ptr, layout);
            0
        }
    }
}

/// Get runtime metrics as JSON string
#[no_mangle]
pub extern "C" fn qi_runtime_get_metrics() -> *const c_char {
    unsafe {
        if let Some(runtime_mutex) = &RUNTIME {
            if let Ok(runtime) = runtime_mutex.lock() {
                let metrics = runtime.get_metrics();
                if let Ok(json) = serde_json::to_string(metrics) {
                    let c_string = std::ffi::CString::new(json).unwrap();
                    return c_string.into_raw();
                }
            }
        }
        std::ptr::null()
    }
}

/// Free a string allocated by the runtime
#[no_mangle]
pub extern "C" fn qi_runtime_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = std::ffi::CString::from_raw(s);
        }
    }
}

// ============================================================================
// String Operations
// ============================================================================

/// Get string length (returns number of UTF-8 characters)
#[no_mangle]
pub extern "C" fn qi_runtime_string_length(s: *const c_char) -> i64 {
    if s.is_null() {
        return 0;
    }
    unsafe {
        if let Ok(rust_str) = CStr::from_ptr(s).to_str() {
            rust_str.chars().count() as i64
        } else {
            0
        }
    }
}

/// Concatenate two strings (caller must free the result)
#[no_mangle]
pub extern "C" fn qi_runtime_string_concat(s1: *const c_char, s2: *const c_char) -> *mut c_char {
    if s1.is_null() || s2.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        if let (Ok(str1), Ok(str2)) = (
            CStr::from_ptr(s1).to_str(),
            CStr::from_ptr(s2).to_str(),
        ) {
            let result = format!("{}{}", str1, str2);
            if let Ok(c_string) = std::ffi::CString::new(result) {
                return c_string.into_raw();
            }
        }
        std::ptr::null_mut()
    }
}

/// Get substring (caller must free the result)
#[no_mangle]
pub extern "C" fn qi_runtime_string_slice(s: *const c_char, start: i64, end: i64) -> *mut c_char {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        if let Ok(rust_str) = CStr::from_ptr(s).to_str() {
            let chars: Vec<char> = rust_str.chars().collect();
            let start_idx = start.max(0) as usize;
            let end_idx = end.min(chars.len() as i64) as usize;
            
            if start_idx < end_idx && end_idx <= chars.len() {
                let substring: String = chars[start_idx..end_idx].iter().collect();
                if let Ok(c_string) = std::ffi::CString::new(substring) {
                    return c_string.into_raw();
                }
            }
        }
        std::ptr::null_mut()
    }
}

/// Compare two strings (returns 0 if equal, <0 if s1<s2, >0 if s1>s2)
#[no_mangle]
pub extern "C" fn qi_runtime_string_compare(s1: *const c_char, s2: *const c_char) -> c_int {
    if s1.is_null() || s2.is_null() {
        return -1;
    }
    
    unsafe {
        if let (Ok(str1), Ok(str2)) = (
            CStr::from_ptr(s1).to_str(),
            CStr::from_ptr(s2).to_str(),
        ) {
            str1.cmp(str2) as c_int
        } else {
            -1
        }
    }
}

// ============================================================================
// Math Operations
// ============================================================================

/// Square root
#[no_mangle]
pub extern "C" fn qi_runtime_math_sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// Power function
#[no_mangle]
pub extern "C" fn qi_runtime_math_pow(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

/// Sine
#[no_mangle]
pub extern "C" fn qi_runtime_math_sin(x: f64) -> f64 {
    x.sin()
}

/// Cosine
#[no_mangle]
pub extern "C" fn qi_runtime_math_cos(x: f64) -> f64 {
    x.cos()
}

/// Tangent
#[no_mangle]
pub extern "C" fn qi_runtime_math_tan(x: f64) -> f64 {
    x.tan()
}

/// Absolute value (integer)
#[no_mangle]
pub extern "C" fn qi_runtime_math_abs_int(x: i64) -> i64 {
    x.abs()
}

/// Absolute value (float)
#[no_mangle]
pub extern "C" fn qi_runtime_math_abs_float(x: f64) -> f64 {
    x.abs()
}

/// Floor
#[no_mangle]
pub extern "C" fn qi_runtime_math_floor(x: f64) -> f64 {
    x.floor()
}

/// Ceiling
#[no_mangle]
pub extern "C" fn qi_runtime_math_ceil(x: f64) -> f64 {
    x.ceil()
}

/// Round
#[no_mangle]
pub extern "C" fn qi_runtime_math_round(x: f64) -> f64 {
    x.round()
}

// ============================================================================
// File I/O Operations
// ============================================================================

/// Open a file (returns file handle or negative on error)
/// Temporary implementation using standard file handles
#[no_mangle]
pub extern "C" fn qi_runtime_file_open(path: *const c_char, mode: *const c_char) -> i64 {
    if path.is_null() || mode.is_null() {
        return -1;
    }
    
    // Temporary: Return a dummy handle
    // TODO: Implement proper file handle management
    eprintln!("警告: qi_runtime_file_open 尚未完全实现");
    -1
}

/// Read from file (returns bytes read or negative on error)
/// Temporary implementation
#[no_mangle]
pub extern "C" fn qi_runtime_file_read(
    handle: i64,
    buffer: *mut u8,
    size: usize,
) -> i64 {
    eprintln!("警告: qi_runtime_file_read 尚未完全实现");
    -1
}

/// Write to file (returns bytes written or negative on error)
/// Temporary implementation
#[no_mangle]
pub extern "C" fn qi_runtime_file_write(
    handle: i64,
    data: *const u8,
    size: usize,
) -> i64 {
    eprintln!("警告: qi_runtime_file_write 尚未完全实现");
    -1
}

/// Close file
/// Temporary implementation
#[no_mangle]
pub extern "C" fn qi_runtime_file_close(handle: i64) -> c_int {
    eprintln!("警告: qi_runtime_file_close 尚未完全实现");
    0
}

/// Read entire file as string (caller must free the result)
#[no_mangle]
pub extern "C" fn qi_runtime_file_read_string(path: *const c_char) -> *mut c_char {
    if path.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        if let Ok(path_str) = CStr::from_ptr(path).to_str() {
            // Use standard library to read file
            match std::fs::read_to_string(path_str) {
                Ok(content) => {
                    if let Ok(c_string) = std::ffi::CString::new(content) {
                        return c_string.into_raw();
                    }
                }
                Err(e) => {
                    eprintln!("读取文件内容失败: {}", e);
                }
            }
        }
        std::ptr::null_mut()
    }
}

/// Write string to file
#[no_mangle]
pub extern "C" fn qi_runtime_file_write_string(path: *const c_char, content: *const c_char) -> c_int {
    if path.is_null() || content.is_null() {
        return -1;
    }
    
    unsafe {
        if let (Ok(path_str), Ok(content_str)) = (
            CStr::from_ptr(path).to_str(),
            CStr::from_ptr(content).to_str(),
        ) {
            // Use standard library to write file
            match std::fs::write(path_str, content_str) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("写入文件内容失败: {}", e);
                    -1
                }
            }
        } else {
            -1
        }
    }
}

// ============================================================================
// Array Operations
// ============================================================================

/// Create array (returns pointer to array structure)
#[no_mangle]
pub extern "C" fn qi_runtime_array_create(size: i64, element_size: i64) -> *mut u8 {
    if size <= 0 || element_size <= 0 {
        return std::ptr::null_mut();
    }
    
    let total_size = (size * element_size) as usize;
    qi_runtime_alloc(total_size)
}

/// Get array length
#[no_mangle]
pub extern "C" fn qi_runtime_array_length(array: *const u8) -> i64 {
    // For now, we'll store the length in the first 8 bytes
    // This is a simplified implementation
    if array.is_null() {
        return 0;
    }
    
    unsafe {
        let length_ptr = array as *const i64;
        *length_ptr
    }
}

// ============================================================================
// Type Conversion
// ============================================================================

/// Convert integer to string (caller must free the result)
#[no_mangle]
pub extern "C" fn qi_runtime_int_to_string(value: i64) -> *mut c_char {
    let string = value.to_string();
    if let Ok(c_string) = std::ffi::CString::new(string) {
        c_string.into_raw()
    } else {
        std::ptr::null_mut()
    }
}

/// Convert float to string (caller must free the result)
#[no_mangle]
pub extern "C" fn qi_runtime_float_to_string(value: f64) -> *mut c_char {
    let string = value.to_string();
    if let Ok(c_string) = std::ffi::CString::new(string) {
        c_string.into_raw()
    } else {
        std::ptr::null_mut()
    }
}

/// Convert string to integer
#[no_mangle]
pub extern "C" fn qi_runtime_string_to_int(s: *const c_char) -> i64 {
    if s.is_null() {
        return 0;
    }
    
    unsafe {
        if let Ok(rust_str) = CStr::from_ptr(s).to_str() {
            rust_str.parse::<i64>().unwrap_or(0)
        } else {
            0
        }
    }
}

/// Convert string to float
#[no_mangle]
pub extern "C" fn qi_runtime_string_to_float(s: *const c_char) -> f64 {
    if s.is_null() {
        return 0.0;
    }
    
    unsafe {
        if let Ok(rust_str) = CStr::from_ptr(s).to_str() {
            rust_str.parse::<f64>().unwrap_or(0.0)
        } else {
            0.0
        }
    }
}

/// Convert integer to float
#[no_mangle]
pub extern "C" fn qi_runtime_int_to_float(value: i64) -> f64 {
    value as f64
}

/// Convert float to integer (truncate)
#[no_mangle]
pub extern "C" fn qi_runtime_float_to_int(value: f64) -> i64 {
    value as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_initialization() {
        let result = qi_runtime_initialize();
        assert_eq!(result, 0);

        let shutdown_result = qi_runtime_shutdown();
        assert_eq!(shutdown_result, 0);
    }

    #[test]
    fn test_runtime_print_functions() {
        qi_runtime_initialize();

        // These should not panic
        qi_runtime_print_int(42);
        qi_runtime_println_int(42);
        qi_runtime_print_float(3.14);
        qi_runtime_println_float(3.14);

        qi_runtime_shutdown();
    }
    
    #[test]
    fn test_string_operations() {
        use std::ffi::CString;
        
        let s1 = CString::new("Hello").unwrap();
        let s2 = CString::new("World").unwrap();
        
        let len = qi_runtime_string_length(s1.as_ptr());
        assert_eq!(len, 5);
        
        let result = qi_runtime_string_concat(s1.as_ptr(), s2.as_ptr());
        assert!(!result.is_null());
        
        unsafe {
            let result_str = CStr::from_ptr(result).to_str().unwrap();
            assert_eq!(result_str, "HelloWorld");
            qi_runtime_free_string(result);
        }
    }
    
    #[test]
    fn test_math_operations() {
        let result = qi_runtime_math_sqrt(16.0);
        assert_eq!(result, 4.0);
        
        let result = qi_runtime_math_pow(2.0, 3.0);
        assert_eq!(result, 8.0);
        
        let result = qi_runtime_math_abs_int(-42);
        assert_eq!(result, 42);
    }
}
