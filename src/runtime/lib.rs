//! Qi Runtime Library
//!
//! This is the main entry point for the Qi runtime library that can be
//! compiled into a static library and linked with Qi programs.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::ffi::{c_char, c_int, c_long, c_double};
use std::sync::{Mutex, Once};

static RUNTIME_INIT: Once = Once::new();
static mut RUNTIME_INITIALIZED: bool = false;

/// Initialize the Qi runtime
#[no_mangle]
pub extern "C" fn qi_runtime_initialize() -> c_int {
    let mut result = 0;

    RUNTIME_INIT.call_once(|| {
        // Initialize any global state here
        unsafe {
            RUNTIME_INITIALIZED = true;
        }
        println!("Qi Runtime initialized");
    });

    result
}

/// Shutdown the Qi runtime
#[no_mangle]
pub extern "C" fn qi_runtime_shutdown() -> c_int {
    unsafe {
        if RUNTIME_INITIALIZED {
            RUNTIME_INITIALIZED = false;
            println!("Qi Runtime shutdown");
        }
    }
    0
}

// ============================================================================
// English Runtime Functions
// ============================================================================

/// Print an integer value
#[no_mangle]
pub extern "C" fn qi_runtime_print_int(value: c_long) -> c_int {
    println!("{}", value);
    0
}

/// Print a floating-point value
#[no_mangle]
pub extern "C" fn qi_runtime_print_float(value: c_double) -> c_int {
    println!("{}", value);
    0
}

// ============================================================================
// Chinese Function Aliases (HEX names)
// ============================================================================

/// 打印字符串 - Chinese alias for print (HEX: e68993e5b0b0)
#[no_mangle]
pub extern "C" fn e6_89_93_e5_b0_b0(s: *const c_char) -> c_int {
    if s.is_null() {
        return -1;
    }

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(s);
        match c_str.to_str() {
            Ok(text) => {
                print!("{}", text);
                0
            }
            Err(_) => -1,
        }
    }
}

/// 打印行 - Chinese alias for println (HEX: e6_89_93_e5_8d_b0_e8_a1_8c)
#[no_mangle]
pub extern "C" fn e6_89_93_e5_8d_b0_e8_a1_8c(s: *const c_char) -> c_int {
    if s.is_null() {
        return -1;
    }

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(s);
        match c_str.to_str() {
            Ok(text) => {
                println!("{}", text);
                0
            }
            Err(_) => -1,
        }
    }
}

/// 打印整数 - Chinese alias for print_int (HEX: e68993e5b0b0_e695b4e695b0)
#[no_mangle]
pub extern "C" fn e6_89_93_e5_b0_b0_e6_95_b4_e6_95_b4(value: c_long) -> c_int {
    println!("{}", value);
    0
}

/// 打印浮点数 - Chinese alias for print_float (HEX: e68993e5b0b0_e6b5aee782b9e695b0)
#[no_mangle]
pub extern "C" fn e6_89_93_e5_b0_b0_e6_b5_be_e7_82_b9_e6_95_b4(value: c_double) -> c_int {
    println!("{}", value);
    0
}

/// 求平方根 - Chinese alias for sqrt (HEX: e6b1b2e5b9b3e6a0b9)
#[no_mangle]
pub extern "C" fn e6_b1_b2_e5_b9_b3_e6_a0_b9(x: c_double) -> c_double {
    if x < 0.0 {
        std::f64::NAN
    } else {
        x.sqrt()
    }
}

/// 求绝对值 - Chinese alias for abs (HEX: e6b182e7bb9de580bc)
#[no_mangle]
pub extern "C" fn e6_b1_82_e7_bb_9d_e5_80_bc(x: c_long) -> c_long {
    x.abs()
}

/// 字符串长度 - Chinese alias for string_length (HEX: e5ad97e7aca6e995bf)
#[no_mangle]
pub extern "C" fn e5_ad_97_e7_ac_a6_e9_95_bf(s: *const c_char) -> i64 {
    if s.is_null() {
        return 0;
    }

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(s);
        match c_str.to_str() {
            Ok(text) => text.chars().count() as i64,
            Err(_) => 0,
        }
    }
}

/// 字符串连接 - Chinese alias for string_concat (HEX: e5ad97e7aca6e8bf9ee68ea5)
#[no_mangle]
pub extern "C" fn e5_ad_97_e7_ac_a6_e8_bf_9e_e6_8e_a5(s1: *const c_char, s2: *const c_char) -> *mut c_char {
    if s1.is_null() || s2.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let c_str1 = std::ffi::CStr::from_ptr(s1);
        let c_str2 = std::ffi::CStr::from_ptr(s2);

        match (c_str1.to_str(), c_str2.to_str()) {
            (Ok(text1), Ok(text2)) => {
                let result = format!("{}{}", text1, text2);
                let c_result = std::ffi::CString::new(result).unwrap();
                c_result.into_raw()
            }
            _ => std::ptr::null_mut(),
        }
    }
}

/// 读取文件 - Chinese alias for file_read (HEX: e8afbbe58f96e69687e4bbb6)
#[no_mangle]
pub extern "C" fn e8_af_bb_e5_8f_96_e6_96_87_e4_bb_b6(path: *const c_char) -> *mut c_char {
    if path.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let c_path = std::ffi::CStr::from_ptr(path);
        match c_path.to_str() {
            Ok(path_str) => {
                match std::fs::read_to_string(path_str) {
                    Ok(content) => {
                        let c_content = std::ffi::CString::new(content).unwrap();
                        c_content.into_raw()
                    }
                    Err(_) => std::ptr::null_mut(),
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// 写入文件 - Chinese alias for file_write (HEX: e58599e585a5e69687e4bbb6)
#[no_mangle]
pub extern "C" fn e5_85_99_e5_85_a5_e6_96_87_e4_bb_b6(path: *const c_char, content: *const c_char) -> c_int {
    if path.is_null() || content.is_null() {
        return -1;
    }

    unsafe {
        let c_path = std::ffi::CStr::from_ptr(path);
        let c_content = std::ffi::CStr::from_ptr(content);

        match (c_path.to_str(), c_content.to_str()) {
            (Ok(path_str), Ok(content_str)) => {
                match std::fs::write(path_str, content_str) {
                    Ok(_) => 0,
                    Err(_) => -1,
                }
            }
            _ => -1,
        }
    }
}

// ============================================================================
// English Function Implementations (for LLVM IR calls)
// ============================================================================

/// Print a string with newline
#[no_mangle]
pub extern "C" fn qi_runtime_println(s: *const c_char) -> c_int {
    if s.is_null() {
        return -1;
    }

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(s);
        match c_str.to_str() {
            Ok(text) => {
                println!("{}", text);
                0
            }
            Err(_) => -1,
        }
    }
}

/// Print a string (no newline)
#[no_mangle]
pub extern "C" fn qi_runtime_print(s: *const c_char) -> c_int {
    if s.is_null() {
        return -1;
    }

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(s);
        match c_str.to_str() {
            Ok(text) => {
                print!("{}", text);
                0
            }
            Err(_) => -1,
        }
    }
}

/// Write string to file
#[no_mangle]
pub extern "C" fn qi_runtime_file_write_string(path: *const c_char, content: *const c_char) -> c_int {
    if path.is_null() || content.is_null() {
        return -1;
    }

    unsafe {
        let c_path = std::ffi::CStr::from_ptr(path);
        let c_content = std::ffi::CStr::from_ptr(content);

        match (c_path.to_str(), c_content.to_str()) {
            (Ok(path_str), Ok(content_str)) => {
                match std::fs::write(path_str, content_str) {
                    Ok(_) => 0,
                    Err(_) => -1,
                }
            }
            _ => -1,
        }
    }
}

/// Read string from file
#[no_mangle]
pub extern "C" fn qi_runtime_file_read_string(path: *const c_char) -> *mut c_char {
    if path.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let c_path = std::ffi::CStr::from_ptr(path);
        match c_path.to_str() {
            Ok(path_str) => {
                match std::fs::read_to_string(path_str) {
                    Ok(content) => {
                        let c_content = std::ffi::CString::new(content).unwrap();
                        c_content.into_raw()
                    }
                    Err(_) => std::ptr::null_mut(),
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// String concatenation
#[no_mangle]
pub extern "C" fn qi_string_concat(s1: *const c_char, s2: *const c_char) -> *mut c_char {
    if s1.is_null() || s2.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let c_str1 = std::ffi::CStr::from_ptr(s1);
        let c_str2 = std::ffi::CStr::from_ptr(s2);

        match (c_str1.to_str(), c_str2.to_str()) {
            (Ok(text1), Ok(text2)) => {
                let result = format!("{}{}", text1, text2);
                let c_result = std::ffi::CString::new(result).unwrap();
                c_result.into_raw()
            }
            _ => std::ptr::null_mut(),
        }
    }
}

/// Print integer
#[no_mangle]
pub extern "C" fn qi_runtime_println_int(value: c_long) -> c_int {
    println!("{}", value);
    0
}

/// Print float
#[no_mangle]
pub extern "C" fn qi_runtime_println_float(value: c_double) -> c_int {
    println!("{}", value);
    0
}

// ============================================================================
// Math Functions
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
// Type Conversion Functions
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
        if let Ok(rust_str) = std::ffi::CStr::from_ptr(s).to_str() {
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
        if let Ok(rust_str) = std::ffi::CStr::from_ptr(s).to_str() {
            rust_str.parse::<f64>().unwrap_or(0.0)
        } else {
            0.0
        }
    }
}

/// String length
#[no_mangle]
pub extern "C" fn qi_runtime_string_length(s: *const c_char) -> i64 {
    if s.is_null() {
        return 0;
    }

    unsafe {
        let c_str = std::ffi::CStr::from_ptr(s);
        match c_str.to_str() {
            Ok(text) => text.chars().count() as i64,
            Err(_) => 0,
        }
    }
}

/// String concatenation
#[no_mangle]
pub extern "C" fn qi_runtime_string_concat(s1: *const c_char, s2: *const c_char) -> *mut c_char {
    if s1.is_null() || s2.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let c_str1 = std::ffi::CStr::from_ptr(s1);
        let c_str2 = std::ffi::CStr::from_ptr(s2);

        match (c_str1.to_str(), c_str2.to_str()) {
            (Ok(text1), Ok(text2)) => {
                let result = format!("{}{}", text1, text2);
                let c_result = std::ffi::CString::new(result).unwrap();
                c_result.into_raw()
            }
            _ => std::ptr::null_mut(),
        }
    }
}