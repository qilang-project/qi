//! Web framework runtime helpers
//!
//! Provides panic-safe helpers used by qi-web's `recover` middleware so a
//! crashing handler returns a 500 response instead of taking down the goroutine.

use std::ffi::{c_char, c_void, CStr, CString};
use std::io::Write;

/// Call a Qi handler `fn(*const Ctx) -> *const Response` with panic isolation.
/// Returns the handler's response pointer on success, or null on panic.
/// The qi-web recover middleware checks for null and synthesizes a 500.
/// Uses C-unwind so panics from the called Qi/Rust code can unwind here.
#[no_mangle]
pub extern "C-unwind" fn qi_web_call_handler_safe(
    handler_fn: *const c_void,
    ctx_ptr: *const c_void,
) -> *const c_void {
    if handler_fn.is_null() {
        return std::ptr::null();
    }
    let handler_addr = handler_fn as usize;
    let ctx_addr = ctx_ptr as usize;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        unsafe {
            let func = std::mem::transmute::<
                usize,
                extern "C-unwind" fn(*const c_void) -> *const c_void,
            >(handler_addr);
            func(ctx_addr as *const c_void)
        }
    }));

    match result {
        Ok(ptr) => ptr,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            eprintln!("[qi-web] handler panic recovered: {}", msg);
            std::ptr::null()
        }
    }
}

/// 调用 (app_ptr, raw_request_ptr) -> response_string_ptr 的处理函数，panic 兜底。
/// 返回 *mut c_char（C 字符串）；qi 侧把它当 字符串 接收。
/// panic 时返回一个固定的 "HTTP/1.1 500 ..." 字符串。
/// C-unwind ABI 让 panic 能从被调用方传到这里被 catch_unwind 抓到。
#[no_mangle]
pub extern "C-unwind" fn qi_web_safe_process_request(
    process_fn: *const c_void,
    app_ptr: *const c_void,
    raw_request_ptr: *const c_char,
) -> *const c_char {
    if process_fn.is_null() {
        return fallback_500();
    }
    let process_addr = process_fn as usize;
    let app_addr = app_ptr as usize;
    let raw_addr = raw_request_ptr as usize;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        unsafe {
            let func = std::mem::transmute::<
                usize,
                extern "C-unwind" fn(*const c_void, *const c_char) -> *const c_char,
            >(process_addr);
            func(app_addr as *const c_void, raw_addr as *const c_char)
        }
    }));

    match result {
        Ok(ptr) if !ptr.is_null() => ptr,
        Ok(_) => fallback_500(),
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            eprintln!("[qi-web] request panic recovered: {}", msg);
            fallback_500()
        }
    }
}

/// 测试用：故意 panic 让 recover 能演示
/// 用 "C-unwind" ABI 才能让 panic 越过 FFI 边界传递到上游的 catch_unwind
#[no_mangle]
pub extern "C-unwind" fn qi_web_panic_for_test() -> i64 {
    panic!("intentional panic for recover demo");
}

/// 一次性 HTTP/1.1 响应序列化：把状态行 + 头部 + Content-Length + body 一锅写完，
/// 一个 alloc，零中间字符串。返回字节切片句柄，调用方负责 free。
///
/// 替代 qi-web 端 `输出响应头部` + `缓冲::从字符串` + `缓冲::追加字符串` 那条 ~10
/// 次小分配的链条。对 hot path 的"快响应"尤其有效（bench_最小 那种）。
#[no_mangle]
pub extern "C" fn qi_runtime_serialize_http_response(
    status_code: i64,
    status_text_ptr: *const c_char,
    headers_ptr: *const c_char,
    body_ptr: *const c_char,
) -> i64 {
    fn cstr_or_empty<'a>(p: *const c_char) -> &'a [u8] {
        if p.is_null() {
            &[]
        } else {
            unsafe { CStr::from_ptr(p).to_bytes() }
        }
    }

    let status_text = cstr_or_empty(status_text_ptr);
    let headers = cstr_or_empty(headers_ptr);
    let body = cstr_or_empty(body_ptr);

    // 预估：状态行 ~32 + 头部 + "Content-Length: NNNN\r\n\r\n" + body
    let cap = 48 + headers.len() + 32 + body.len();
    let mut out: Vec<u8> = Vec::with_capacity(cap);

    out.extend_from_slice(b"HTTP/1.1 ");
    let _ = write!(out, "{}", status_code);
    out.extend_from_slice(b" ");
    out.extend_from_slice(status_text);
    out.extend_from_slice(b"\r\n");
    if !headers.is_empty() {
        out.extend_from_slice(headers);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"Content-Length: ");
    let _ = write!(out, "{}", body.len());
    out.extend_from_slice(b"\r\n\r\n");
    out.extend_from_slice(body);

    crate::runtime::stdlib::bytes_ffi::register_bytes(out)
}

// ============================================================================
// HTTP/1.1 请求解析 fast path —— 替代 qi-web 端 13 次 字符串::子串/查找 链条
// ============================================================================

/// HTTP request parsed into 5 fields. Lives as long as the qi caller holds
/// the opaque pointer; freed via qi_web_request_parts_free.
pub struct RequestParts {
    method: CString,
    path: CString,
    query: CString,
    headers: CString,
    body: CString,
}

/// 从字节切片句柄解析 HTTP/1.1 请求，返回 *mut RequestParts。
/// 失败返回 null。调用方负责调 qi_web_request_parts_free 释放。
#[no_mangle]
pub extern "C" fn qi_web_parse_request_bytes(bytes_handle: i64) -> *mut RequestParts {
    let bytes = match crate::runtime::stdlib::bytes_ffi::clone_bytes(bytes_handle) {
        Some(b) => b,
        None => return std::ptr::null_mut(),
    };
    let parts = parse_http_request(&bytes);
    Box::into_raw(Box::new(parts))
}

/// 从 c_string 解析（兼容旧 qi-web 解析请求 签名）
#[no_mangle]
pub extern "C" fn qi_web_parse_request_cstr(s: *const c_char) -> *mut RequestParts {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { CStr::from_ptr(s).to_bytes() };
    Box::into_raw(Box::new(parse_http_request(bytes)))
}

fn parse_http_request(bytes: &[u8]) -> RequestParts {
    // 找第一个 \r\n（或 \n）— 请求行结束
    let line_end = find_subslice(bytes, b"\r\n").unwrap_or_else(|| {
        find_subslice(bytes, b"\n").unwrap_or(bytes.len())
    });
    let request_line = &bytes[..line_end];

    // request_line: METHOD SP PATH SP HTTP-VERSION
    let mut method = &b""[..];
    let mut full_path = &b""[..];
    if let Some(sp1) = request_line.iter().position(|&b| b == b' ') {
        method = &request_line[..sp1];
        let rest = &request_line[sp1 + 1..];
        if let Some(sp2) = rest.iter().position(|&b| b == b' ') {
            full_path = &rest[..sp2];
        } else {
            full_path = rest;
        }
    }

    // path?query
    let (path, query) = match full_path.iter().position(|&b| b == b'?') {
        Some(qmark) => (&full_path[..qmark], &full_path[qmark + 1..]),
        None => (full_path, &b""[..]),
    };

    // 跳过 \r\n（或 \n），找 \r\n\r\n（或 \n\n）
    let after_line_start = if bytes.get(line_end..line_end + 2) == Some(b"\r\n") {
        line_end + 2
    } else if bytes.get(line_end..line_end + 1) == Some(b"\n") {
        line_end + 1
    } else {
        line_end
    };
    let rest = &bytes[after_line_start..];
    let (headers, body) = match find_subslice(rest, b"\r\n\r\n") {
        Some(boundary) => (&rest[..boundary], &rest[boundary + 4..]),
        None => match find_subslice(rest, b"\n\n") {
            Some(boundary) => (&rest[..boundary], &rest[boundary + 2..]),
            None => (rest, &b""[..]),
        },
    };

    RequestParts {
        method: cstring_from_bytes(method),
        path: cstring_from_bytes(path),
        query: cstring_from_bytes(query),
        headers: cstring_from_bytes(headers),
        body: cstring_from_bytes(body),
    }
}

fn cstring_from_bytes(b: &[u8]) -> CString {
    // 内嵌 NUL 替换为空格（C 字符串约束）
    let cleaned: Vec<u8> = b.iter().map(|&x| if x == 0 { b' ' } else { x }).collect();
    CString::new(cleaned).unwrap_or_else(|_| CString::new("").unwrap())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

// 每个访问器返回 *新 alloc 的* CString — 不能借引用，否则 qi 调用方释放
// RequestParts 后 qi 持有的 字符串 会变成 dangling pointer。
fn dup_cstring(src: &CString) -> *const c_char {
    let bytes = src.as_bytes();
    match CString::new(bytes) {
        Ok(c) => c.into_raw() as *const c_char,
        Err(_) => CString::new("").unwrap().into_raw() as *const c_char,
    }
}

#[no_mangle]
pub extern "C" fn qi_web_request_method(p: *const RequestParts) -> *const c_char {
    if p.is_null() { return CString::new("").unwrap().into_raw(); }
    unsafe { dup_cstring(&(*p).method) }
}

#[no_mangle]
pub extern "C" fn qi_web_request_path(p: *const RequestParts) -> *const c_char {
    if p.is_null() { return CString::new("").unwrap().into_raw(); }
    unsafe { dup_cstring(&(*p).path) }
}

#[no_mangle]
pub extern "C" fn qi_web_request_query(p: *const RequestParts) -> *const c_char {
    if p.is_null() { return CString::new("").unwrap().into_raw(); }
    unsafe { dup_cstring(&(*p).query) }
}

#[no_mangle]
pub extern "C" fn qi_web_request_headers(p: *const RequestParts) -> *const c_char {
    if p.is_null() { return CString::new("").unwrap().into_raw(); }
    unsafe { dup_cstring(&(*p).headers) }
}

#[no_mangle]
pub extern "C" fn qi_web_request_body(p: *const RequestParts) -> *const c_char {
    if p.is_null() { return CString::new("").unwrap().into_raw(); }
    unsafe { dup_cstring(&(*p).body) }
}

/// Returns 0 (i64) — qi codegen assigns return values; void breaks at the
/// emission point, so we return a dummy i64 instead.
#[no_mangle]
pub extern "C" fn qi_web_request_parts_free(p: *mut RequestParts) -> i64 {
    if !p.is_null() {
        unsafe { drop(Box::from_raw(p)); }
    }
    0
}

fn fallback_500() -> *const c_char {
    let body = "Internal Server Error";
    let response = format!(
        "HTTP/1.1 500 Internal Server Error\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Connection: close\r\n\
         Content-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    // intentional leak — returned as a static-like C string for the qi runtime
    let cstr = CString::new(response).unwrap();
    cstr.into_raw() as *const c_char
}
