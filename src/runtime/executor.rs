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
}
