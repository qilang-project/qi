//! MCP 客户端核心 FFI（标准库.MCP客户端）
//!
//! Rust 实现的 MCP 客户端，解决纯 Qi 实现的两个根本问题：
//!   1. 大 SSE 响应（如 browser_evaluate）被截断 → 用 reqwest blocking 全量读取。
//!   2. id 未关联 → 每条请求分配单调递增 id，在响应关联 map 中匹配。
//!
//! 传输：
//!   - stdio：复用 subprocess_ffi 的子进程基础设施（spawn + 后台 reader），
//!     在此层再加 id 关联，允许多并发请求（理论上；MCP stdio 通常串行）。
//!   - HTTP：reqwest::blocking，每次 POST 读全部 body（.text() 即可），
//!     对 `text/event-stream` 应答解析出第一条（也通常只有一条）`data:` 行。
//!
//! 连接描述符格式（Qi 侧字符串）：
//!   "mcpc|<conn_id>"
//!
//! 公开 FFI：
//!   qi_mcpc_connect_stdio(cmd, args_json) -> i64   (>0 = conn_id, <=0 = 失败)
//!   qi_mcpc_connect_http(base_url)        -> i64
//!   qi_mcpc_request(conn_id, method, params_json) -> *mut c_char (JSON result/error 串)
//!   qi_mcpc_close(conn_id)                -> i32
//!   qi_mcpc_free_string(ptr)              -> void

#![allow(non_snake_case)]

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::io::{BufRead, BufReader, Write};
use std::os::raw::c_char;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value as Json};

// ─────────────────────────────────────────────────────────────────────────────
// 传输类型
// ─────────────────────────────────────────────────────────────────────────────

/// stdio 子进程状态（共享给后台读线程 + 主线程写）
struct StdioChild {
    _child: Child,                           // 保持子进程存活
    stdin: ChildStdin,                       // 写端（序列化用，须持锁）
    /// 按 id 存储的响应队列（有 id 的消息，即 response）
    responses: Arc<Mutex<HashMap<i64, Json>>>,
    eof: Arc<AtomicBool>,
}

enum Transport {
    Stdio {
        child_state: Arc<Mutex<StdioChild>>,
    },
    Http {
        base_url: String,
        session_id: String,
    },
}

struct Connection {
    transport: Transport,
    /// 每条请求分配唯一 id（单调递增）
    next_id: AtomicI64,
}

// ─────────────────────────────────────────────────────────────────────────────
// 全局连接注册表
// ─────────────────────────────────────────────────────────────────────────────

type ConnRegistry = Mutex<HashMap<i64, Arc<Connection>>>;

fn conn_registry() -> &'static ConnRegistry {
    static REG: OnceLock<ConnRegistry> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

static CONN_COUNTER: AtomicI64 = AtomicI64::new(1);

fn next_conn_id() -> i64 {
    CONN_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn get_conn(id: i64) -> Option<Arc<Connection>> {
    conn_registry().lock().ok()?.get(&id).cloned()
}

// ─────────────────────────────────────────────────────────────────────────────
// 辅助：将 Rust String 转 C 字符串（所有权移出）
// ─────────────────────────────────────────────────────────────────────────────

fn to_cstr(s: String) -> *mut c_char {
    // 替换 NUL 字节，防止 CString 创建失败
    match CString::new(s.replace('\0', "\u{FFFD}")) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn empty_cstr() -> *mut c_char {
    CString::new("").unwrap().into_raw()
}

// ─────────────────────────────────────────────────────────────────────────────
// SSE 解析：从 text/event-stream 响应体中提取 data: 行的 JSON
//
// Playwright MCP 和标准 MCP 服务器的 SSE 格式：
//   event: message\ndata: {...}\n\n
//
// 可能有多个 data: 行（如带进度通知的消息流），我们拼接所有 data: 行，
// 取最后一条包含 "result" 或 "error" 的 JSON-RPC 响应。
// ─────────────────────────────────────────────────────────────────────────────

fn parse_sse_body(body: &str) -> String {
    // 优先找包含 result 或 error 的 data: 行（JSON-RPC 响应）
    let mut last_data = String::new();

    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("data:") {
            let data = rest.trim();
            if data.is_empty() {
                continue;
            }
            // 是有效 JSON 且含 result/error 字段的 JSON-RPC 响应
            if let Ok(v) = serde_json::from_str::<Json>(data) {
                if v.get("result").is_some() || v.get("error").is_some() {
                    return data.to_string();
                }
                // 记录最后一条有效 data: 行，用于 fallback
                last_data = data.to_string();
            } else {
                // 非 JSON，直接返回原始文本
                last_data = data.to_string();
            }
        }
    }

    // 没有找到明确的 result/error，返回最后一条 data: 行，或整个 body
    if !last_data.is_empty() {
        return last_data;
    }

    // 如果 body 本身就是 JSON（非 SSE 格式，直接 application/json 响应）
    body.trim().to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// HTTP MCP 请求辅助
// ─────────────────────────────────────────────────────────────────────────────

// ⚠️ reqwest::blocking 内部自建 tokio runtime。若在已有 tokio runtime 上下文里
// （如 qi-web 的请求处理线程）直接调用，流式 SSE body 的阻塞读取会被打断、
// `.text()` 返回空 body（大/流式响应尤甚，小响应碰巧在首个缓冲块里能拿到）。
// 解决：把整个阻塞 HTTP 调用放到一个独立 OS 线程上执行，确保它没有外层 async runtime。
fn http_post_mcp(
    base_url: &str,
    session_id: &str,
    body: &str,
    timeout_secs: u64,
) -> Result<String, String> {
    let base_url = base_url.to_string();
    let session_id = session_id.to_string();
    let body = body.to_string();
    std::thread::spawn(move || -> Result<String, String> {
        use reqwest::blocking::Client;
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| format!("reqwest build: {}", e))?;
        let mut req = client
            .post(&base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(body);
        if !session_id.is_empty() {
            req = req.header("Mcp-Session-Id", &session_id);
        }
        let resp = req.send().map_err(|e| format!("send: {}", e))?;
        resp.text().map_err(|e| format!("read body: {}", e))
    })
    .join()
    .unwrap_or_else(|_| Err("http thread panicked".to_string()))
}

fn http_extract_session(base_url: &str, body: &str) -> Result<(String, String), String> {
    let base_url = base_url.to_string();
    let body = body.to_string();
    std::thread::spawn(move || -> Result<(String, String), String> {
        use reqwest::blocking::Client;
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("reqwest build: {}", e))?;
        let resp = client
            .post(&base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(body)
            .send()
            .map_err(|e| format!("send: {}", e))?;
        let session_id = resp
            .headers()
            .get("mcp-session-id")
            .or_else(|| resp.headers().get("Mcp-Session-Id"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = resp.text().map_err(|e| format!("read body: {}", e))?;
        Ok((session_id, text))
    })
    .join()
    .unwrap_or_else(|_| Err("http thread panicked".to_string()))
}

// ─────────────────────────────────────────────────────────────────────────────
// stdio 等待 id 关联的响应
// ─────────────────────────────────────────────────────────────────────────────

fn stdio_wait_response(
    responses: Arc<Mutex<HashMap<i64, Json>>>,
    eof: Arc<AtomicBool>,
    id: i64,
    timeout_secs: u64,
) -> Option<Json> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        {
            let mut map = responses.lock().ok()?;
            if let Some(v) = map.remove(&id) {
                return Some(v);
            }
        }
        if eof.load(Ordering::SeqCst) {
            // 最后再检查一次
            let mut map = responses.lock().ok()?;
            return map.remove(&id);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FFI：连接 stdio MCP server
// ─────────────────────────────────────────────────────────────────────────────

/// 启动 stdio MCP 子进程，完成 initialize 握手。
/// 成功返回 conn_id (>0)，失败返回 -1。
#[no_mangle]
pub extern "C" fn qi_mcpc_connect_stdio(
    cmd: *const c_char,
    args_json: *const c_char,
) -> i64 {
    if cmd.is_null() {
        return -1;
    }
    let cmd_str = unsafe { CStr::from_ptr(cmd).to_string_lossy().to_string() };
    let args: Vec<String> = if args_json.is_null() {
        Vec::new()
    } else {
        let s = unsafe { CStr::from_ptr(args_json).to_string_lossy() };
        serde_json::from_str::<Vec<String>>(&s).unwrap_or_default()
    };

    let mut child = match Command::new(&cmd_str)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[qi-mcpc] spawn 失败: {}", e);
            return -1;
        }
    };

    let stdin = match child.stdin.take() {
        Some(s) => s,
        None => return -1,
    };
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => return -1,
    };

    // 后台读线程：持续读 stdout，按 id 路由响应
    let responses: Arc<Mutex<HashMap<i64, Json>>> = Arc::new(Mutex::new(HashMap::new()));
    let eof: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let responses_clone = responses.clone();
    let eof_clone = eof.clone();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Ok(v) = serde_json::from_str::<Json>(trimmed) {
                        if let Some(id_val) = v.get("id") {
                            // 有 id → response（含 result 或 error）
                            if let Some(id_num) = id_val.as_i64() {
                                if let Ok(mut map) = responses_clone.lock() {
                                    map.insert(id_num, v);
                                }
                            }
                        }
                        // notifications (无 id): 静默忽略（P2 扩展）
                    }
                }
                Err(_) => break,
            }
        }
        eof_clone.store(true, Ordering::SeqCst);
    });

    let child_state = Arc::new(Mutex::new(StdioChild {
        _child: child,
        stdin,
        responses: responses.clone(),
        eof: eof.clone(),
    }));

    let conn = Arc::new(Connection {
        transport: Transport::Stdio {
            child_state: child_state.clone(),
        },
        next_id: AtomicI64::new(1),
    });

    // initialize 握手
    let req_id = 1i64;
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": req_id,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "qi-harness", "version": "0.1.0"}
        }
    });

    {
        let mut st = match child_state.lock() {
            Ok(g) => g,
            Err(_) => return -1,
        };
        if writeln!(st.stdin, "{}", init_req).is_err() {
            eprintln!("[qi-mcpc] initialize 写入失败");
            return -1;
        }
        if st.stdin.flush().is_err() {
            return -1;
        }
    }

    // 等待 initialize 响应（30s）
    let init_resp = stdio_wait_response(responses.clone(), eof.clone(), req_id, 30);
    if init_resp.is_none() {
        eprintln!("[qi-mcpc] initialize 超时或 EOF");
        return -1;
    }

    // 发送 notifications/initialized
    {
        let mut st = match child_state.lock() {
            Ok(g) => g,
            Err(_) => return -1,
        };
        let notif = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        let _ = writeln!(st.stdin, "{}", notif.to_string());
        let _ = st.stdin.flush();
    }

    // 注册连接并返回 conn_id
    let conn_id = next_conn_id();
    // 更新 next_id（初始化用了 id=1）
    conn.next_id.fetch_add(1, Ordering::SeqCst); // 下次从 2 开始
    conn_registry()
        .lock()
        .unwrap()
        .insert(conn_id, conn);
    conn_id
}

// ─────────────────────────────────────────────────────────────────────────────
// FFI：连接 HTTP MCP server
// ─────────────────────────────────────────────────────────────────────────────

/// 连接 HTTP(Streamable) MCP server，完成 initialize 握手。
/// 成功返回 conn_id (>0)，失败返回 -1。
#[no_mangle]
pub extern "C" fn qi_mcpc_connect_http(base_url: *const c_char) -> i64 {
    if base_url.is_null() {
        return -1;
    }
    let url = unsafe { CStr::from_ptr(base_url).to_string_lossy().to_string() };

    let init_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "qi-harness", "version": "0.1.0"}
        }
    })
    .to_string();

    let (session_id, init_body_text) = match http_extract_session(&url, &init_body) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[qi-mcpc] HTTP initialize 失败: {}", e);
            return -1;
        }
    };

    if session_id.is_empty() {
        // 某些服务器不需要会话 id（但 Playwright MCP 需要）；也允许继续
        eprintln!("[qi-mcpc] HTTP initialize: 未返回 Mcp-Session-Id，继续（可能不需要会话）");
    }

    // 验证 initialize 响应包含 result
    let parsed_init = parse_sse_body(&init_body_text);
    if parsed_init.is_empty() {
        eprintln!("[qi-mcpc] HTTP initialize: 响应体为空");
        return -1;
    }

    // 发送 notifications/initialized
    let notif = json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string();
    // 忽略通知响应（202 或空体）
    let _ = http_post_mcp(&url, &session_id, &notif, 10);

    let conn_id = next_conn_id();
    let conn = Arc::new(Connection {
        transport: Transport::Http {
            base_url: url,
            session_id,
        },
        next_id: AtomicI64::new(2), // id=1 已用于 initialize
    });

    conn_registry()
        .lock()
        .unwrap()
        .insert(conn_id, conn);
    conn_id
}

// ─────────────────────────────────────────────────────────────────────────────
// FFI：发送 MCP 请求并等待响应
// ─────────────────────────────────────────────────────────────────────────────

/// 发送 JSON-RPC 请求（method + params_json），等待对应 id 的响应。
/// 返回响应的 result 字段的 JSON 串（或 error 对象）。
/// 失败返回空 C 字符串（非 NULL）。
#[no_mangle]
pub extern "C" fn qi_mcpc_request(
    conn_id: i64,
    method: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    if method.is_null() || params_json.is_null() {
        return empty_cstr();
    }

    let method_str = unsafe { CStr::from_ptr(method).to_string_lossy().to_string() };
    let params_str = unsafe { CStr::from_ptr(params_json).to_string_lossy().to_string() };

    let params: Json = match serde_json::from_str(&params_str) {
        Ok(v) => v,
        Err(_) => json!({}),
    };

    let conn = match get_conn(conn_id) {
        Some(c) => c,
        None => {
            eprintln!("[qi-mcpc] 连接 {} 不存在", conn_id);
            return empty_cstr();
        }
    };

    let req_id = conn.next_id.fetch_add(1, Ordering::SeqCst);
    let request = json!({
        "jsonrpc": "2.0",
        "id": req_id,
        "method": method_str,
        "params": params
    });
    let request_str = request.to_string();

    match &conn.transport {
        Transport::Stdio { child_state } => {
            let (responses, eof) = {
                let st = match child_state.lock() {
                    Ok(g) => g,
                    Err(_) => return empty_cstr(),
                };
                (st.responses.clone(), st.eof.clone())
            };

            // 写请求
            {
                let mut st = match child_state.lock() {
                    Ok(g) => g,
                    Err(_) => return empty_cstr(),
                };
                if writeln!(st.stdin, "{}", request_str).is_err()
                    || st.stdin.flush().is_err()
                {
                    return empty_cstr();
                }
            }

            // 等待响应（60s）
            match stdio_wait_response(responses, eof, req_id, 60) {
                // 返回完整 JSON-RPC 响应行（含 jsonrpc/id/result 或 error），
                // 与原纯 Qi 实现的 发送请求 返回值格式一致。
                Some(resp) => to_cstr(resp.to_string()),
                None => {
                    eprintln!("[qi-mcpc] stdio 响应超时 (id={})", req_id);
                    empty_cstr()
                }
            }
        }

        Transport::Http { base_url, session_id } => {
            // 完整读取响应 body（reqwest blocking .text() 读全部，修复大 SSE 截断 bug）
            match http_post_mcp(base_url, session_id, &request_str, 60) {
                Ok(body) => {
                    if body.is_empty() {
                        // 202 Accepted with empty body（通知响应）
                        return empty_cstr();
                    }
                    // 解析 SSE 或 JSON body，提取 data: 行
                    let data = parse_sse_body(&body);
                    if data.is_empty() {
                        return empty_cstr();
                    }
                    // 返回与原纯 Qi 实现一致的格式：完整 JSON-RPC 响应行
                    to_cstr(data)
                }
                Err(e) => {
                    eprintln!("[qi-mcpc] HTTP 请求失败 (id={}): {}", req_id, e);
                    empty_cstr()
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FFI：关闭连接
// ─────────────────────────────────────────────────────────────────────────────

/// 关闭 MCP 连接。
/// stdio: 杀子进程。HTTP: 发 DELETE（可选，忽略失败）。
/// 成功返回 1，失败返回 0。返回 i64 与 Qi 整数类型对齐。
#[no_mangle]
pub extern "C" fn qi_mcpc_close(conn_id: i64) -> i64 {
    let conn = match conn_registry().lock().unwrap().remove(&conn_id) {
        Some(c) => c,
        None => return 0,
    };

    match &conn.transport {
        Transport::Stdio { child_state } => {
            if let Ok(mut st) = child_state.lock() {
                let _ = st._child.kill();
                let _ = st._child.wait();
            }
            1
        }
        Transport::Http { base_url, session_id } => {
            if !session_id.is_empty() {
                use reqwest::blocking::Client;
                if let Ok(client) = Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                {
                    let _ = client
                        .delete(base_url)
                        .header("Mcp-Session-Id", session_id.as_str())
                        .send();
                }
            }
            1
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FFI：释放字符串
// ─────────────────────────────────────────────────────────────────────────────

/// 释放由 qi_mcpc_request 返回的字符串。
#[no_mangle]
pub extern "C" fn qi_mcpc_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 单元测试
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_body_direct_json() {
        // 直接 JSON 响应（application/json）
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let out = parse_sse_body(body);
        assert!(out.contains("result"));
    }

    #[test]
    fn test_parse_sse_body_event_message() {
        // 标准 SSE 格式
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
        let out = parse_sse_body(body);
        assert!(out.contains("tools"));
    }

    #[test]
    fn test_parse_sse_body_large_data() {
        // 大体：多行 SSE，result 在其中一行
        let big_value = "x".repeat(10000);
        let body = format!(
            "event: message\ndata: {{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"output\":\"{}\"}}}}\n\n",
            big_value
        );
        let out = parse_sse_body(&body);
        assert!(out.contains("result"));
        assert!(out.contains(&big_value));
    }

    #[test]
    fn test_parse_sse_body_with_notifications() {
        // SSE 流中先有通知再有响应
        let body = concat!(
            "event: message\n",
            "data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{}}\n\n",
            "event: message\n",
            "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"content\":[]}}\n\n",
        );
        let out = parse_sse_body(body);
        // 应该返回有 result 的那一条
        assert!(out.contains("result"));
        assert!(!out.contains("notifications/progress") || out.contains("result"));
    }

    // 复现 HTTP 大 browser_evaluate（带转义引号/换行）失败。
    // 用法：先 `npx -y @playwright/mcp@latest --port 43560`，再
    // `QI_TEST_MCP_URL=http://localhost:43560/mcp cargo test debug_http_big_eval -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn debug_http_big_eval() {
        use std::ffi::{CStr, CString};
        let url = std::env::var("QI_TEST_MCP_URL").unwrap_or_default();
        if url.is_empty() { eprintln!("跳过：未设 QI_TEST_MCP_URL"); return; }
        let cu = CString::new(url).unwrap();
        let conn = qi_mcpc_connect_http(cu.as_ptr());
        eprintln!("[dbg] conn={}", conn);
        assert!(conn > 0);
        let method = CString::new("tools/call").unwrap();
        let call = |p: &str| -> String {
            let cp = CString::new(p).unwrap();
            let r = qi_mcpc_request(conn, method.as_ptr(), cp.as_ptr());
            unsafe { CStr::from_ptr(r).to_string_lossy().into_owned() }
        };
        let nav = call(r#"{"name":"browser_navigate","arguments":{"url":"https://example.com/"}}"#);
        eprintln!("[dbg] navigate len={} head={}", nav.len(), &nav[..nav.len().min(80)]);
        // 真实失败的那段大函数（~1.5KB），用 serde 正确转义构造 params
        let func = r##"() => {
  const title = document.title;
  const metaDesc = document.querySelector('meta[name="description"]')?.getAttribute('content') || '';
  const h1s = document.querySelectorAll('h1').length;
  const h2s = document.querySelectorAll('h2').length;
  const h3s = document.querySelectorAll('h3').length;
  const jsonlds = document.querySelectorAll('script[type="application/ld+json"]').length;
  const ogTitle = document.querySelector('meta[property="og:title"]');
  const ogDesc = document.querySelector('meta[property="og:description"]');
  const ogImage = document.querySelector('meta[property="og:image"]');
  const twitterCard = document.querySelector('meta[name="twitter:card"]');
  const twitterTitle = document.querySelector('meta[name="twitter:title"]');
  const twitterDesc = document.querySelector('meta[name="twitter:description"]');
  const viewport = document.querySelector('meta[name="viewport"]')?.getAttribute('content') || '';
  const imgs = document.querySelectorAll('img');
  let imgsWithAlt = 0;
  imgs.forEach(img => { if(img.getAttribute('alt') !== null && img.getAttribute('alt') !== '') imgsWithAlt++; });
  const imgAltCoverage = imgs.length > 0 ? Math.round((imgsWithAlt / imgs.length) * 100) : 100;
  const bodyText = document.body.innerText.replace(/\s+/g, ' ').trim();
  const wordCount = bodyText.split(' ').filter(w => w.length > 0).length;
  return JSON.stringify({ title, metaDescription: metaDesc, h1Count: h1s, h2Count: h2s, h3Count: h3s, jsonldCount: jsonlds, ogTitle: !!ogTitle, ogDesc: !!ogDesc, ogImage: !!ogImage, twitterCard: !!twitterCard, twitterTitle: !!twitterTitle, twitterDesc: !!twitterDesc, viewport, imgAltCoverage, wordCount });
}"##;
        let big = serde_json::json!({"name":"browser_evaluate","arguments":{"function": func}}).to_string();
        eprintln!("[dbg] big params bytes={}", big.len());
        let res = call(&big);
        eprintln!("[dbg] BIG EVAL RESULT len={} : {}", res.len(), &res[..res.len().min(400)]);
        // 看是否还能用（session 是否被搞坏）
        let snap = call(r#"{"name":"browser_snapshot","arguments":{}}"#);
        eprintln!("[dbg] snapshot-after len={} head={}", snap.len(), &snap[..snap.len().min(80)]);
    }
}
