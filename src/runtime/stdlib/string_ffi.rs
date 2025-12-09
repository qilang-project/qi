//! String FFI Module
//!
//! This module provides C FFI functions for string manipulation
//! with full Unicode and Chinese language support.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Find the position of a substring in a string
/// Returns -1 if not found, otherwise returns the byte position
#[no_mangle]
pub extern "C" fn qi_string_find(text_ptr: *const c_char, search_ptr: *const c_char) -> i64 {
    if text_ptr.is_null() || search_ptr.is_null() {
        return -1;
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };

        let search = match CStr::from_ptr(search_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };

        match text.find(search) {
            Some(pos) => pos as i64,
            None => -1,
        }
    }
}

/// Find the position of a substring starting from a given position
/// Returns -1 if not found, otherwise returns the byte position
#[no_mangle]
pub extern "C" fn qi_string_find_from(
    text_ptr: *const c_char,
    search_ptr: *const c_char,
    start: i64,
) -> i64 {
    if text_ptr.is_null() || search_ptr.is_null() || start < 0 {
        return -1;
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };

        let search = match CStr::from_ptr(search_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };

        let start = start as usize;
        if start >= text.len() {
            return -1;
        }

        match text[start..].find(search) {
            Some(pos) => (start + pos) as i64,
            None => -1,
        }
    }
}

/// Extract substring from start position with given length (in bytes)
/// Returns a new string allocated with malloc
#[no_mangle]
pub extern "C" fn qi_string_substring(
    text_ptr: *const c_char,
    start: i64,
    length: i64,
) -> *mut c_char {
    if text_ptr.is_null() || start < 0 || length < 0 {
        return std::ptr::null_mut();
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let start = start as usize;
        let length = length as usize;

        if start >= text.len() {
            // Return empty string
            return match CString::new("") {
                Ok(s) => s.into_raw(),
                Err(_) => std::ptr::null_mut(),
            };
        }

        let end = std::cmp::min(start + length, text.len());
        let substring = &text[start..end];

        match CString::new(substring) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// Extract substring from start position to end
/// Returns a new string allocated with malloc
#[no_mangle]
pub extern "C" fn qi_string_substring_from(
    text_ptr: *const c_char,
    start: i64,
) -> *mut c_char {
    if text_ptr.is_null() || start < 0 {
        return std::ptr::null_mut();
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let start = start as usize;

        if start >= text.len() {
            // Return empty string
            return match CString::new("") {
                Ok(s) => s.into_raw(),
                Err(_) => std::ptr::null_mut(),
            };
        }

        let substring = &text[start..];

        match CString::new(substring) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// Get the byte length of a string
#[no_mangle]
pub extern "C" fn qi_string_byte_length(text_ptr: *const c_char) -> i64 {
    if text_ptr.is_null() {
        return 0;
    }

    unsafe {
        match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s.len() as i64,
            Err(_) => 0,
        }
    }
}

/// Get the character count of a UTF-8 string
#[no_mangle]
pub extern "C" fn qi_string_char_count(text_ptr: *const c_char) -> i64 {
    if text_ptr.is_null() {
        return 0;
    }

    unsafe {
        match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s.chars().count() as i64,
            Err(_) => 0,
        }
    }
}

/// Replace all occurrences of a substring with another
/// Returns a new string allocated with malloc
#[no_mangle]
pub extern "C" fn qi_string_replace(
    text_ptr: *const c_char,
    search_ptr: *const c_char,
    replace_ptr: *const c_char,
) -> *mut c_char {
    if text_ptr.is_null() || search_ptr.is_null() || replace_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let search = match CStr::from_ptr(search_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let replace = match CStr::from_ptr(replace_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let result = text.replace(search, replace);

        match CString::new(result) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// Trim whitespace from both ends of a string
/// Returns a new string allocated with malloc
#[no_mangle]
pub extern "C" fn qi_string_trim(text_ptr: *const c_char) -> *mut c_char {
    if text_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let trimmed = text.trim();

        match CString::new(trimmed) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// Convert string to uppercase
/// Returns a new string allocated with malloc
#[no_mangle]
pub extern "C" fn qi_string_to_upper(text_ptr: *const c_char) -> *mut c_char {
    if text_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let upper = text.to_uppercase();

        match CString::new(upper) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// Convert string to lowercase
/// Returns a new string allocated with malloc
#[no_mangle]
pub extern "C" fn qi_string_to_lower(text_ptr: *const c_char) -> *mut c_char {
    if text_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let lower = text.to_lowercase();

        match CString::new(lower) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// Check if a string contains a substring
/// Returns 1 if contains, 0 if not
#[no_mangle]
pub extern "C" fn qi_string_contains(
    text_ptr: *const c_char,
    search_ptr: *const c_char,
) -> i64 {
    if text_ptr.is_null() || search_ptr.is_null() {
        return 0;
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        };

        let search = match CStr::from_ptr(search_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        };

        if text.contains(search) {
            1
        } else {
            0
        }
    }
}

/// Check if a string starts with a prefix
/// Returns 1 if starts with, 0 if not
#[no_mangle]
pub extern "C" fn qi_string_starts_with(
    text_ptr: *const c_char,
    prefix_ptr: *const c_char,
) -> i64 {
    if text_ptr.is_null() || prefix_ptr.is_null() {
        return 0;
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        };

        let prefix = match CStr::from_ptr(prefix_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        };

        if text.starts_with(prefix) {
            1
        } else {
            0
        }
    }
}

/// Check if a string ends with a suffix
/// Returns 1 if ends with, 0 if not
#[no_mangle]
pub extern "C" fn qi_string_ends_with(
    text_ptr: *const c_char,
    suffix_ptr: *const c_char,
) -> i64 {
    if text_ptr.is_null() || suffix_ptr.is_null() {
        return 0;
    }

    unsafe {
        let text = match CStr::from_ptr(text_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        };

        let suffix = match CStr::from_ptr(suffix_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        };

        if text.ends_with(suffix) {
            1
        } else {
            0
        }
    }
}

/// Split a string by a delimiter
/// Returns a handle to a string list (整数列表句柄)
/// Note: This is simplified - in a full implementation, would return a list handle
#[no_mangle]
pub extern "C" fn qi_string_split(
    _text_ptr: *const c_char,
    _delimiter_ptr: *const c_char,
) -> i64 {
    // TODO: Implement proper list integration
    // For now, return -1 to indicate not implemented
    -1
}

/// Free a string allocated by string functions
/// Note: Uses qi_string_free from future.rs (already defined)

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_string_find() {
        let text = CString::new("Hello, 世界!").unwrap();
        let search = CString::new("世界").unwrap();

        let pos = qi_string_find(text.as_ptr(), search.as_ptr());
        assert!(pos >= 0);

        let not_found = CString::new("不存在").unwrap();
        let pos = qi_string_find(text.as_ptr(), not_found.as_ptr());
        assert_eq!(pos, -1);
    }

    #[test]
    fn test_string_find_from() {
        let text = CString::new("abcabc").unwrap();
        let search = CString::new("bc").unwrap();

        let pos1 = qi_string_find_from(text.as_ptr(), search.as_ptr(), 0);
        assert_eq!(pos1, 1);

        let pos2 = qi_string_find_from(text.as_ptr(), search.as_ptr(), 2);
        assert_eq!(pos2, 4);
    }

    #[test]
    fn test_string_substring() {
        let text = CString::new("Hello, World!").unwrap();

        let sub = qi_string_substring(text.as_ptr(), 0, 5);
        assert!(!sub.is_null());
        unsafe {
            let result = CStr::from_ptr(sub).to_str().unwrap();
            assert_eq!(result, "Hello");
            qi_string_free(sub);
        }
    }

    #[test]
    fn test_string_lengths() {
        let text = CString::new("你好世界").unwrap();

        let byte_len = qi_string_byte_length(text.as_ptr());
        assert_eq!(byte_len, 12); // 4 characters * 3 bytes each

        let char_count = qi_string_char_count(text.as_ptr());
        assert_eq!(char_count, 4);
    }

    #[test]
    fn test_string_replace() {
        let text = CString::new("Hello World").unwrap();
        let search = CString::new("World").unwrap();
        let replace = CString::new("Rust").unwrap();

        let result = qi_string_replace(text.as_ptr(), search.as_ptr(), replace.as_ptr());
        assert!(!result.is_null());
        unsafe {
            let result_str = CStr::from_ptr(result).to_str().unwrap();
            assert_eq!(result_str, "Hello Rust");
            qi_string_free(result);
        }
    }

    #[test]
    fn test_string_trim() {
        let text = CString::new("  Hello  ").unwrap();

        let result = qi_string_trim(text.as_ptr());
        assert!(!result.is_null());
        unsafe {
            let result_str = CStr::from_ptr(result).to_str().unwrap();
            assert_eq!(result_str, "Hello");
            qi_string_free(result);
        }
    }

    #[test]
    fn test_string_case() {
        let text = CString::new("Hello World").unwrap();

        let upper = qi_string_to_upper(text.as_ptr());
        assert!(!upper.is_null());
        unsafe {
            let upper_str = CStr::from_ptr(upper).to_str().unwrap();
            assert_eq!(upper_str, "HELLO WORLD");
            qi_string_free(upper);
        }

        let lower = qi_string_to_lower(text.as_ptr());
        assert!(!lower.is_null());
        unsafe {
            let lower_str = CStr::from_ptr(lower).to_str().unwrap();
            assert_eq!(lower_str, "hello world");
            qi_string_free(lower);
        }
    }

    #[test]
    fn test_string_checks() {
        let text = CString::new("Hello World").unwrap();
        let hello = CString::new("Hello").unwrap();
        let world = CString::new("World").unwrap();
        let test = CString::new("test").unwrap();

        assert_eq!(qi_string_contains(text.as_ptr(), world.as_ptr()), 1);
        assert_eq!(qi_string_contains(text.as_ptr(), test.as_ptr()), 0);

        assert_eq!(qi_string_starts_with(text.as_ptr(), hello.as_ptr()), 1);
        assert_eq!(qi_string_starts_with(text.as_ptr(), world.as_ptr()), 0);

        assert_eq!(qi_string_ends_with(text.as_ptr(), world.as_ptr()), 1);
        assert_eq!(qi_string_ends_with(text.as_ptr(), hello.as_ptr()), 0);
    }

    #[test]
    fn test_chinese_strings() {
        let text = CString::new("你好，世界！").unwrap();
        let search = CString::new("世界").unwrap();

        // Test find with Chinese
        let pos = qi_string_find(text.as_ptr(), search.as_ptr());
        assert!(pos >= 0);

        // Test substring with Chinese
        let sub = qi_string_substring_from(text.as_ptr(), pos);
        assert!(!sub.is_null());
        unsafe {
            let result = CStr::from_ptr(sub).to_str().unwrap();
            assert!(result.starts_with("世界"));
            qi_string_free(sub);
        }

        // Test character count
        let count = qi_string_char_count(text.as_ptr());
        assert_eq!(count, 6); // 你好，世界！
    }
}
