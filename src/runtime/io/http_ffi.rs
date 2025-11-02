//! HTTP 模块 FFI 接口
//!
//! 为 Qi 语言提供 C 接口的 HTTP 客户端操作

use super::http::{HttpClient, HttpRequest, HttpMethod};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::time::Duration;
use std::sync::OnceLock;
use std::sync::Mutex;
use std::collections::HashMap;

// 全局 HTTP 客户端
static HTTP客户端: OnceLock<Mutex<HttpClient>> = OnceLock::new();

// HTTP 请求池（用于异步请求管理）
static HTTP请求池: OnceLock<Mutex<HashMap<i64, HttpRequest>>> = OnceLock::new();
static 请求句柄计数器: OnceLock<Mutex<i64>> = OnceLock::new();

#[allow(non_snake_case)]
fn 获取HTTP客户端() -> &'static Mutex<HttpClient> {
    HTTP客户端.get_or_init(|| Mutex::new(HttpClient::new()))
}

fn 获取请求池() -> &'static Mutex<HashMap<i64, HttpRequest>> {
    HTTP请求池.get_or_init(|| Mutex::new(HashMap::new()))
}

fn 获取请求句柄计数器() -> &'static Mutex<i64> {
    请求句柄计数器.get_or_init(|| Mutex::new(0))
}

/// 初始化 HTTP 模块
#[no_mangle]
pub extern "C" fn qi_http_init() -> i64 {
    let _客户端 = 获取HTTP客户端();
    1  // 成功
}

/// HTTP GET 请求
/// 返回响应体字符串（需要调用 qi_http_free_string 释放）
#[no_mangle]
pub extern "C" fn qi_http_get(url: *const c_char) -> *mut c_char {
    if url.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let 地址 = CStr::from_ptr(url).to_string_lossy().to_string();
        let 请求 = HttpRequest::get(地址);

        let 客户端 = 获取HTTP客户端().lock().unwrap();
        match 客户端.execute(请求) {
            Ok(响应) => {
                match 响应.body_as_string() {
                    Ok(响应体) => CString::new(响应体).unwrap().into_raw(),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// HTTP POST 请求
/// 返回响应体字符串（需要调用 qi_http_free_string 释放）
#[no_mangle]
pub extern "C" fn qi_http_post(url: *const c_char, body: *const c_char) -> *mut c_char {
    if url.is_null() || body.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let 地址 = CStr::from_ptr(url).to_string_lossy().to_string();
        let 请求体 = CStr::from_ptr(body).to_string_lossy().to_string();

        let 请求 = HttpRequest::post(地址, 请求体.into_bytes());

        let 客户端 = 获取HTTP客户端().lock().unwrap();
        match 客户端.execute(请求) {
            Ok(响应) => {
                match 响应.body_as_string() {
                    Ok(响应体) => CString::new(响应体).unwrap().into_raw(),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// HTTP PUT 请求
#[no_mangle]
pub extern "C" fn qi_http_put(url: *const c_char, body: *const c_char) -> *mut c_char {
    if url.is_null() || body.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let 地址 = CStr::from_ptr(url).to_string_lossy().to_string();
        let 请求体 = CStr::from_ptr(body).to_string_lossy().to_string();

        let mut 请求 = HttpRequest::get(地址);
        请求.method = HttpMethod::Put;
        请求.body = Some(请求体.into_bytes());

        let 客户端 = 获取HTTP客户端().lock().unwrap();
        match 客户端.execute(请求) {
            Ok(响应) => {
                match 响应.body_as_string() {
                    Ok(响应体) => CString::new(响应体).unwrap().into_raw(),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// HTTP DELETE 请求
#[no_mangle]
pub extern "C" fn qi_http_delete(url: *const c_char) -> *mut c_char {
    if url.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let 地址 = CStr::from_ptr(url).to_string_lossy().to_string();

        let mut 请求 = HttpRequest::get(地址);
        请求.method = HttpMethod::Delete;

        let 客户端 = 获取HTTP客户端().lock().unwrap();
        match 客户端.execute(请求) {
            Ok(响应) => {
                match 响应.body_as_string() {
                    Ok(响应体) => CString::new(响应体).unwrap().into_raw(),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// 创建 HTTP 请求（返回请求句柄）
#[no_mangle]
pub extern "C" fn qi_http_request_create(method: *const c_char, url: *const c_char) -> i64 {
    if method.is_null() || url.is_null() {
        return -1;
    }

    unsafe {
        let 方法名 = CStr::from_ptr(method).to_string_lossy().to_string();
        let 地址 = CStr::from_ptr(url).to_string_lossy().to_string();

        let 方法 = match 方法名.to_uppercase().as_str() {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "DELETE" => HttpMethod::Delete,
            "HEAD" => HttpMethod::Head,
            "PATCH" => HttpMethod::Patch,
            "OPTIONS" => HttpMethod::Options,
            _ => HttpMethod::Get,
        };

        let mut 请求 = HttpRequest::get(地址);
        请求.method = 方法;

        let mut 句柄计数 = 获取请求句柄计数器().lock().unwrap();
        *句柄计数 += 1;
        let 句柄 = *句柄计数;

        let mut 请求池 = 获取请求池().lock().unwrap();
        请求池.insert(句柄, 请求);

        句柄
    }
}

/// 设置请求头
#[no_mangle]
pub extern "C" fn qi_http_request_set_header(
    handle: i64,
    name: *const c_char,
    value: *const c_char,
) -> i64 {
    if name.is_null() || value.is_null() {
        return 0;
    }

    unsafe {
        let 头名称 = CStr::from_ptr(name).to_string_lossy().to_string();
        let 头值 = CStr::from_ptr(value).to_string_lossy().to_string();

        let mut 请求池 = 获取请求池().lock().unwrap();
        if let Some(请求) = 请求池.get_mut(&handle) {
            请求.headers.insert(头名称, 头值);
            1
        } else {
            0
        }
    }
}

/// 设置请求体
#[no_mangle]
pub extern "C" fn qi_http_request_set_body(handle: i64, body: *const c_char) -> i64 {
    if body.is_null() {
        return 0;
    }

    unsafe {
        let 请求体 = CStr::from_ptr(body).to_string_lossy().to_string();

        let mut 请求池 = 获取请求池().lock().unwrap();
        if let Some(请求) = 请求池.get_mut(&handle) {
            请求.body = Some(请求体.into_bytes());
            1
        } else {
            0
        }
    }
}

/// 设置请求超时（毫秒）
#[no_mangle]
pub extern "C" fn qi_http_request_set_timeout(handle: i64, timeout_ms: i64) -> i64 {
    if timeout_ms <= 0 {
        return 0;
    }

    let mut 请求池 = 获取请求池().lock().unwrap();
    if let Some(请求) = 请求池.get_mut(&handle) {
        请求.timeout = Duration::from_millis(timeout_ms as u64);
        1
    } else {
        0
    }
}

/// 执行 HTTP 请求
/// 返回响应体字符串（需要调用 qi_http_free_string 释放）
#[no_mangle]
pub extern "C" fn qi_http_request_execute(handle: i64) -> *mut c_char {
    let mut 请求池 = 获取请求池().lock().unwrap();
    if let Some(请求) = 请求池.remove(&handle) {
        let 客户端 = 获取HTTP客户端().lock().unwrap();
        match 客户端.execute(请求) {
            Ok(响应) => {
                match 响应.body_as_string() {
                    Ok(响应体) => CString::new(响应体).unwrap().into_raw(),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// 获取 HTTP 状态码（简化版，返回 200 表示成功）
#[no_mangle]
pub extern "C" fn qi_http_get_status(url: *const c_char) -> i64 {
    if url.is_null() {
        return -1;
    }

    unsafe {
        let 地址 = CStr::from_ptr(url).to_string_lossy().to_string();
        let 请求 = HttpRequest::get(地址);

        let 客户端 = 获取HTTP客户端().lock().unwrap();
        match 客户端.execute(请求) {
            Ok(响应) => 响应.status_code as i64,
            Err(_) => -1,
        }
    }
}

/// 释放 HTTP 响应字符串
#[no_mangle]
pub extern "C" fn qi_http_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_http_init() {
        let result = qi_http_init();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_http_get() {
        qi_http_init();

        let url = CString::new("https://example.com").unwrap();
        let response = qi_http_get(url.as_ptr());

        // 在模拟实现中应该返回 "Hello, World!"
        if !response.is_null() {
            let response_str = unsafe { CStr::from_ptr(response).to_string_lossy() };
            assert_eq!(response_str, "Hello, World!");
            qi_http_free_string(response);
        }
    }

    #[test]
    fn test_http_request_builder() {
        qi_http_init();

        let method = CString::new("POST").unwrap();
        let url = CString::new("https://api.example.com").unwrap();
        let handle = qi_http_request_create(method.as_ptr(), url.as_ptr());
        assert!(handle > 0);

        let header_name = CString::new("Content-Type").unwrap();
        let header_value = CString::new("application/json").unwrap();
        let result = qi_http_request_set_header(handle, header_name.as_ptr(), header_value.as_ptr());
        assert_eq!(result, 1);

        let body = CString::new("{\"test\":\"data\"}").unwrap();
        let result = qi_http_request_set_body(handle, body.as_ptr());
        assert_eq!(result, 1);

        let result = qi_http_request_set_timeout(handle, 5000);
        assert_eq!(result, 1);
    }
}
