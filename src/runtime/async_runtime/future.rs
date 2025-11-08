//! Future type implementation for Qi async runtime
//!
//! Provides Future<T> support for async operations

use std::sync::{Arc, Mutex};
use std::os::raw::c_char;
#[cfg(test)]
use std::ffi::CStr;

/// Future state enumeration
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub enum FutureState {
    Pending,     // 等待中
    Completed,   // 已完成
    Failed,      // 已失败
}

/// Value types that Future can hold
/// Future 可以持有的值类型
#[derive(Debug, Clone)]
pub enum FutureValue {
    Integer(i64),           // 整数
    Float(f64),             // 浮点数
    Boolean(bool),          // 布尔值
    String(String),         // 字符串
    Pointer(*mut u8),       // 指针（用于结构体等）
    None,                   // 无值
}

/// Future structure - heap allocated
/// 未来结构 - 堆分配
#[repr(C)]
pub struct Future {
    state: Arc<Mutex<FutureState>>,
    value: Arc<Mutex<Option<FutureValue>>>,
    error: Arc<Mutex<Option<String>>>,
}

impl Future {
    /// Create a ready future with integer value
    /// 创建就绪的整数未来
    pub fn ready_i64(value: i64) -> Self {
        Future {
            state: Arc::new(Mutex::new(FutureState::Completed)),
            value: Arc::new(Mutex::new(Some(FutureValue::Integer(value)))),
            error: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a ready future with float value
    /// 创建就绪的浮点数未来
    pub fn ready_f64(value: f64) -> Self {
        Future {
            state: Arc::new(Mutex::new(FutureState::Completed)),
            value: Arc::new(Mutex::new(Some(FutureValue::Float(value)))),
            error: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a ready future with boolean value
    /// 创建就绪的布尔未来
    pub fn ready_bool(value: bool) -> Self {
        Future {
            state: Arc::new(Mutex::new(FutureState::Completed)),
            value: Arc::new(Mutex::new(Some(FutureValue::Boolean(value)))),
            error: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a ready future with string value
    /// 创建就绪的字符串未来
    pub fn ready_string(value: String) -> Self {
        Future {
            state: Arc::new(Mutex::new(FutureState::Completed)),
            value: Arc::new(Mutex::new(Some(FutureValue::String(value)))),
            error: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a ready future with pointer value
    /// 创建就绪的指针未来
    pub fn ready_ptr(ptr: *mut u8) -> Self {
        Future {
            state: Arc::new(Mutex::new(FutureState::Completed)),
            value: Arc::new(Mutex::new(Some(FutureValue::Pointer(ptr)))),
            error: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a failed future
    /// 创建失败的未来
    pub fn failed(error: String) -> Self {
        Future {
            state: Arc::new(Mutex::new(FutureState::Failed)),
            value: Arc::new(Mutex::new(Some(FutureValue::None))),
            error: Arc::new(Mutex::new(Some(error))),
        }
    }

    /// Check if future is completed
    /// 检查是否已完成
    pub fn is_completed(&self) -> bool {
        let state = self.state.lock().unwrap();
        *state == FutureState::Completed
    }

    /// Await the future and get the value
    /// 等待未来并获取值
    pub fn await_value(&self) -> Result<FutureValue, String> {
        loop {
            let state = self.state.lock().unwrap();
            match *state {
                FutureState::Completed => {
                    drop(state);
                    let value = self.value.lock().unwrap();
                    return Ok(value.clone().unwrap_or(FutureValue::None));
                }
                FutureState::Failed => {
                    drop(state);
                    let error = self.error.lock().unwrap();
                    return Err(error.clone().unwrap_or_else(|| "Unknown error".to_string()));
                }
                FutureState::Pending => {
                    drop(state);
                    // Yield to other tasks
                    std::thread::yield_now();
                }
            }
        }
    }
}

// ===== FFI Functions for LLVM IR =====

/// Create a ready future with an i64 value
/// FFI: qi_future_ready_i64(value: i64) -> *mut Future
#[no_mangle]
pub extern "C" fn qi_future_ready_i64(value: i64) -> *mut Future {
    let future = Box::new(Future::ready_i64(value));
    Box::into_raw(future)
}

/// Create a ready future with a f64 value
/// FFI: qi_future_ready_f64(value: f64) -> *mut Future
#[no_mangle]
pub extern "C" fn qi_future_ready_f64(value: f64) -> *mut Future {
    let future = Box::new(Future::ready_f64(value));
    Box::into_raw(future)
}

/// Create a ready future with a boolean value
/// FFI: qi_future_ready_bool(value: i32) -> *mut Future
/// Note: Use i32 for FFI compatibility (0 = false, non-zero = true)
#[no_mangle]
pub extern "C" fn qi_future_ready_bool(value: i32) -> *mut Future {
    let future = Box::new(Future::ready_bool(value != 0));
    Box::into_raw(future)
}

/// Create a ready future with a string value
/// FFI: qi_future_ready_string(str_ptr: *const u8, str_len: usize) -> *mut Future
#[no_mangle]
pub extern "C" fn qi_future_ready_string(str_ptr: *const u8, str_len: usize) -> *mut Future {
    let string_value = if str_ptr.is_null() {
        String::new()
    } else {
        unsafe {
            let slice = std::slice::from_raw_parts(str_ptr, str_len);
            String::from_utf8_lossy(slice).to_string()
        }
    };

    let future = Box::new(Future::ready_string(string_value));
    Box::into_raw(future)
}

/// Create a ready future with a pointer value (for structs, etc.)
/// FFI: qi_future_ready_ptr(ptr: *mut u8) -> *mut Future
#[no_mangle]
pub extern "C" fn qi_future_ready_ptr(ptr: *mut u8) -> *mut Future {
    let future = Box::new(Future::ready_ptr(ptr));
    Box::into_raw(future)
}

/// Create a failed future with an error message
/// FFI: qi_future_failed(error_ptr: *const u8, error_len: usize) -> *mut Future
#[no_mangle]
pub extern "C" fn qi_future_failed(error_ptr: *const u8, error_len: usize) -> *mut Future {
    let error_msg = if error_ptr.is_null() {
        "Unknown error".to_string()
    } else {
        unsafe {
            let slice = std::slice::from_raw_parts(error_ptr, error_len);
            String::from_utf8_lossy(slice).to_string()
        }
    };

    let future = Box::new(Future::failed(error_msg));
    Box::into_raw(future)
}

/// Await a future and get its i64 value (blocking)
/// FFI: qi_future_await_i64(future: *mut Future) -> i64
/// Returns: value on success, -1 on failure
#[no_mangle]
pub extern "C" fn qi_future_await_i64(future: *mut Future) -> i64 {
    if future.is_null() {
        return -1;
    }

    unsafe {
        let future_ref = &*future;
        match future_ref.await_value() {
            Ok(FutureValue::Integer(value)) => value,
            _ => -1,
        }
    }
}

/// Await a future and get its f64 value (blocking)
/// FFI: qi_future_await_f64(future: *mut Future) -> f64
/// Returns: value on success, 0.0 on failure
#[no_mangle]
pub extern "C" fn qi_future_await_f64(future: *mut Future) -> f64 {
    if future.is_null() {
        return 0.0;
    }

    unsafe {
        let future_ref = &*future;
        match future_ref.await_value() {
            Ok(FutureValue::Float(value)) => value,
            _ => 0.0,
        }
    }
}

/// Await a future and get its boolean value (blocking)
/// FFI: qi_future_await_bool(future: *mut Future) -> i32
/// Returns: 1 for true, 0 for false/failure
#[no_mangle]
pub extern "C" fn qi_future_await_bool(future: *mut Future) -> i32 {
    if future.is_null() {
        return 0;
    }

    unsafe {
        let future_ref = &*future;
        match future_ref.await_value() {
            Ok(FutureValue::Boolean(value)) => if value { 1 } else { 0 },
            _ => 0,
        }
    }
}

/// Await a future and get its string value (blocking)
/// FFI: qi_future_await_string(future: *mut Future) -> *const c_char
/// Returns: null-terminated C string, caller must free with qi_string_free
#[no_mangle]
pub extern "C" fn qi_future_await_string(future: *mut Future) -> *const c_char {
    if future.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let future_ref = &*future;
        match future_ref.await_value() {
            Ok(FutureValue::String(s)) => {
                // Allocate C string that caller must free
                let c_string = std::ffi::CString::new(s).unwrap_or_default();
                c_string.into_raw()
            }
            _ => std::ptr::null(),
        }
    }
}

/// Await a future and get its pointer value (blocking)
/// FFI: qi_future_await_ptr(future: *mut Future) -> *mut u8
/// Returns: pointer value on success, null on failure
#[no_mangle]
pub extern "C" fn qi_future_await_ptr(future: *mut Future) -> *mut u8 {
    if future.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let future_ref = &*future;
        match future_ref.await_value() {
            Ok(FutureValue::Pointer(ptr)) => ptr,
            _ => std::ptr::null_mut(),
        }
    }
}

/// Free a C string returned by qi_future_await_string
/// FFI: qi_string_free(str_ptr: *mut c_char)
#[no_mangle]
pub extern "C" fn qi_string_free(str_ptr: *mut c_char) {
    if !str_ptr.is_null() {
        unsafe {
            let _ = std::ffi::CString::from_raw(str_ptr);
        }
    }
}

/// Check if a future is completed
/// FFI: qi_future_is_completed(future: *mut Future) -> i32
/// Returns: 1 if completed, 0 otherwise
#[no_mangle]
pub extern "C" fn qi_future_is_completed(future: *mut Future) -> i32 {
    if future.is_null() {
        return 0;
    }

    unsafe {
        let future_ref = &*future;
        if future_ref.is_completed() { 1 } else { 0 }
    }
}

/// Free a future
/// FFI: qi_future_free(future: *mut Future)
#[no_mangle]
pub extern "C" fn qi_future_free(future: *mut Future) {
    if !future.is_null() {
        unsafe {
            let _ = Box::from_raw(future);
        }
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_future_ready_i64() {
        let future = Future::ready_i64(42);
        assert!(future.is_completed());
        match future.await_value().unwrap() {
            FutureValue::Integer(v) => assert_eq!(v, 42),
            _ => panic!("Expected integer value"),
        }
    }

    #[test]
    fn test_future_ready_f64() {
        let future = Future::ready_f64(3.14);
        assert!(future.is_completed());
        match future.await_value().unwrap() {
            FutureValue::Float(v) => assert!((v - 3.14).abs() < 0.0001),
            _ => panic!("Expected float value"),
        }
    }

    #[test]
    fn test_future_ready_bool() {
        let future = Future::ready_bool(true);
        assert!(future.is_completed());
        match future.await_value().unwrap() {
            FutureValue::Boolean(v) => assert_eq!(v, true),
            _ => panic!("Expected boolean value"),
        }
    }

    #[test]
    fn test_future_ready_string() {
        let future = Future::ready_string("Hello".to_string());
        assert!(future.is_completed());
        match future.await_value().unwrap() {
            FutureValue::String(s) => assert_eq!(s, "Hello"),
            _ => panic!("Expected string value"),
        }
    }

    #[test]
    fn test_future_failed() {
        let future = Future::failed("Test error".to_string());
        assert!(!future.is_completed());
        assert!(future.await_value().is_err());
    }

    #[test]
    fn test_ffi_future_ready_i64() {
        let future_ptr = qi_future_ready_i64(100);
        assert!(!future_ptr.is_null());

        let value = qi_future_await_i64(future_ptr);
        assert_eq!(value, 100);

        qi_future_free(future_ptr);
    }

    #[test]
    fn test_ffi_future_ready_f64() {
        let future_ptr = qi_future_ready_f64(2.718);
        assert!(!future_ptr.is_null());

        let value = qi_future_await_f64(future_ptr);
        assert!((value - 2.718).abs() < 0.0001);

        qi_future_free(future_ptr);
    }

    #[test]
    fn test_ffi_future_ready_bool() {
        let future_ptr = qi_future_ready_bool(1);
        assert!(!future_ptr.is_null());

        let value = qi_future_await_bool(future_ptr);
        assert_eq!(value, 1);

        qi_future_free(future_ptr);
    }

    #[test]
    fn test_ffi_future_ready_string() {
        let test_str = "测试字符串";
        let future_ptr = qi_future_ready_string(test_str.as_ptr(), test_str.len());
        assert!(!future_ptr.is_null());

        let result_ptr = qi_future_await_string(future_ptr);
        assert!(!result_ptr.is_null());

        unsafe {
            let c_str = CStr::from_ptr(result_ptr);
            let rust_str = c_str.to_string_lossy();
            assert_eq!(rust_str, test_str);
            qi_string_free(result_ptr as *mut c_char);
        }

        qi_future_free(future_ptr);
    }

    #[test]
    fn test_ffi_is_completed() {
        let future_ptr = qi_future_ready_i64(42);
        let is_completed = qi_future_is_completed(future_ptr);
        assert_eq!(is_completed, 1);
        qi_future_free(future_ptr);
    }
}
