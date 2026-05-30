//! MCP 客户端核心 FFI（标准库.MCP客户端）
//!
//! Rust 实现的 MCP 客户端，解决纯 Qi 实现的两个根本问题：
//!   1. 大 SSE 响应（如 browser_evaluate）被截断 → 裸 TcpStream 读到 EOF 全量读取。
//!   2. id 未关联 → 每条请求分配单调递增 id，在响应关联 map 中匹配。
//!
//! 传输：
//!   - stdio：复用 subprocess_ffi 的子进程基础设施（spawn + 后台 reader），
//!     在此层再加 id 关联，允许多并发请求（理论上；MCP stdio 通常串行）。
//!   - HTTP：裸 std::net::TcpStream 手写 HTTP/1.1 客户端，每次 POST 读全部 body，
//!     对 `text/event-stream` 应答解析出第一条（也通常只有一条）`data:` 行。
//!     （历史：曾用 reqwest::blocking，但在 qi-web 进程内发大 POST body 时，
//!      reqwest 内部 tokio runtime 与外层 hyper server runtime 交互，导致
//!      Playwright 返回 200 空 body / 会话失效。裸 TcpStream 无 runtime，彻底规避。）
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
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
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

/// HTTP(Streamable) MCP 连接状态。
///
/// ⚠️ 关键：Playwright MCP 把会话绑定在 TCP 连接上 —— 一旦某个请求带
/// `Connection: close` 并断开，整条会话立刻失效（后续请求 404 Session not found）。
/// 因此这里必须复用【同一条 keep-alive 连接】跑完整个会话，而不是每请求新建/关闭。
struct HttpConn {
    base_url: String,
    session_id: String,
    /// 持久 keep-alive 连接（懒建/断后重连）。须持锁串行化请求。
    stream: Option<TcpStream>,
}

enum Transport {
    Stdio {
        child_state: Arc<Mutex<StdioChild>>,
    },
    Http {
        http: Arc<Mutex<HttpConn>>,
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

// ⚠️ 两个历史/隐藏 bug，本实现一并解决：
//   (1) reqwest::blocking 内部自建 tokio runtime。在 qi-web 进程内（已有外层
//       hyper/tokio HTTP server）发大 POST body 时，行为异常（空 body/会话失效）。
//       → 改用裸 std::net::TcpStream 手写 HTTP/1.1，无任何 async runtime。
//   (2) Playwright MCP 把会话绑定在 TCP 连接上：任何带 `Connection: close` 的请求
//       一旦断开，整条会话立即失效（后续 404 Session not found）。这才是「大请求空
//       body」真正的根因 —— 大请求恰好更容易触发连接被关/复用断裂。
//       → 复用同一条 keep-alive 连接跑完整个会话；按 Content-Length / chunked 精确
//         读取每条响应，绝不主动关闭连接。
// 仅支持 http://（localhost MCP 无需 TLS）。

/// 解析 `http://host:port/path` → (host, port, path)。
fn parse_http_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("仅支持 http:// 的 MCP 端点: {}", url))?;
    // host[:port] 与 path 切分
    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rfind(':') {
        Some(idx) => {
            let h = &authority[..idx];
            let p: u16 = authority[idx + 1..]
                .parse()
                .map_err(|_| format!("非法端口: {}", authority))?;
            (h.to_string(), p)
        }
        None => (authority.to_string(), 80u16),
    };
    let path = if path.is_empty() { "/".to_string() } else { path.to_string() };
    Ok((host, port, path))
}

/// 在响应头原文里大小写不敏感地查找 header 值。
fn find_header_value(headers: &str, name: &str) -> Option<String> {
    let name_lc = name.to_ascii_lowercase();
    for line in headers.lines() {
        if let Some(idx) = line.find(':') {
            let (k, v) = line.split_at(idx);
            if k.trim().to_ascii_lowercase() == name_lc {
                return Some(v[1..].trim().to_string());
            }
        }
    }
    None
}

/// 在已建立的 keep-alive 连接上读取【恰好一条】HTTP 响应。
/// 支持 Content-Length 与 chunked 两种 body 框架；不依赖 EOF / 连接关闭。
/// 返回 (响应头原文, body 字符串)。
fn read_one_http_response(stream: &mut TcpStream) -> Result<(String, String), String> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 8192];

    // 1) 读到头部结束（\r\n\r\n）
    let header_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        let n = stream
            .read(&mut chunk)
            .map_err(|e| format!("read header 失败: {}", e))?;
        if n == 0 {
            return Err("连接在读取响应头时关闭（会话可能已失效）".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let headers_text = String::from_utf8_lossy(&buf[..header_end - 4]).to_string();
    let content_length: Option<usize> = find_header_value(&headers_text, "Content-Length")
        .and_then(|v| v.trim().parse().ok());
    let is_chunked = find_header_value(&headers_text, "Transfer-Encoding")
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);

    // 已经读到的 body 部分
    let mut body_raw: Vec<u8> = buf[header_end..].to_vec();

    if is_chunked {
        // 读到终止块 "0\r\n\r\n"
        while !body_raw.windows(5).any(|w| w == b"0\r\n\r\n") {
            let n = stream
                .read(&mut chunk)
                .map_err(|e| format!("read chunked 失败: {}", e))?;
            if n == 0 {
                break;
            }
            body_raw.extend_from_slice(&chunk[..n]);
        }
        let decoded = decode_chunked(&body_raw);
        Ok((headers_text, decoded))
    } else if let Some(cl) = content_length {
        while body_raw.len() < cl {
            let n = stream
                .read(&mut chunk)
                .map_err(|e| format!("read body 失败: {}", e))?;
            if n == 0 {
                break;
            }
            body_raw.extend_from_slice(&chunk[..n]);
        }
        body_raw.truncate(cl);
        Ok((headers_text, String::from_utf8_lossy(&body_raw).to_string()))
    } else {
        // 无 Content-Length 且非 chunked（如 202 空体）：返回已读到的部分
        Ok((headers_text, String::from_utf8_lossy(&body_raw).to_string()))
    }
}

/// 解码 HTTP/1.1 chunked transfer-encoding body。
fn decode_chunked(raw: &[u8]) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(raw.len());
    let mut i = 0usize;
    while i < raw.len() {
        // 读块大小行
        let line_end = match raw[i..].windows(2).position(|w| w == b"\r\n") {
            Some(p) => i + p,
            None => break,
        };
        let size_str = String::from_utf8_lossy(&raw[i..line_end]);
        let size_hex = size_str.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16).unwrap_or(0);
        i = line_end + 2; // 跳过 \r\n
        if size == 0 {
            break;
        }
        let end = (i + size).min(raw.len());
        out.extend_from_slice(&raw[i..end]);
        i = end + 2; // 跳过块尾 \r\n
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 在【持久 keep-alive 连接】上发一条 POST，读回恰好一条响应。
/// 断线时自动重连一次（连接可能被 server 空闲回收）。
fn http_post_keepalive(
    conn: &mut HttpConn,
    body: &str,
    timeout_secs: u64,
) -> Result<(String, String), String> {
    let (host, port, path) = parse_http_url(&conn.base_url)?;

    // 组装请求（keep-alive：不发 Connection: close）
    let mut req = String::new();
    req.push_str(&format!("POST {} HTTP/1.1\r\n", path));
    req.push_str(&format!("Host: {}:{}\r\n", host, port));
    req.push_str("Content-Type: application/json\r\n");
    req.push_str("Accept: application/json, text/event-stream\r\n");
    if !conn.session_id.is_empty() {
        req.push_str(&format!("Mcp-Session-Id: {}\r\n", conn.session_id));
    }
    req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    req.push_str("\r\n");
    let mut wire = req.into_bytes();
    wire.extend_from_slice(body.as_bytes());

    // 最多尝试两次：首次用现有连接，失败则重连后重试一次
    let mut last_err = String::new();
    for attempt in 0..2 {
        if conn.stream.is_none() {
            match TcpStream::connect((host.as_str(), port)) {
                Ok(s) => {
                    let t = Some(Duration::from_secs(timeout_secs));
                    let _ = s.set_read_timeout(t);
                    let _ = s.set_write_timeout(t);
                    conn.stream = Some(s);
                }
                Err(e) => {
                    last_err = format!("connect {}:{} 失败: {}", host, port, e);
                    continue;
                }
            }
        }

        let stream = conn.stream.as_mut().unwrap();
        // 每次刷新超时
        let t = Some(Duration::from_secs(timeout_secs));
        let _ = stream.set_read_timeout(t);
        let _ = stream.set_write_timeout(t);

        let write_ok = stream
            .write_all(&wire)
            .and_then(|_| stream.flush())
            .is_ok();
        if !write_ok {
            // 连接坏了，丢弃后重连重试
            conn.stream = None;
            last_err = "写请求失败，重连重试".to_string();
            continue;
        }

        match read_one_http_response(stream) {
            Ok(r) => return Ok(r),
            Err(e) => {
                // 第一次失败：可能是 server 关闭了空闲连接 → 重连重试
                conn.stream = None;
                last_err = e;
                if attempt == 0 {
                    continue;
                }
            }
        }
    }
    Err(last_err)
}

fn http_extract_session(base_url: &str, body: &str) -> Result<(String, String, Option<TcpStream>), String> {
    let mut conn = HttpConn {
        base_url: base_url.to_string(),
        session_id: String::new(),
        stream: None,
    };
    let (headers, body_text) = http_post_keepalive(&mut conn, body, 30)?;
    let session_id = find_header_value(&headers, "Mcp-Session-Id").unwrap_or_default();
    // 复用这条已建立的连接给后续请求（保持会话存活的关键）
    Ok((session_id, body_text, conn.stream.take()))
}

/// 在持久连接上发 DELETE 关闭会话（忽略失败）。
fn http_delete_keepalive(conn: &mut HttpConn) -> Result<(), String> {
    let (host, port, path) = parse_http_url(&conn.base_url)?;
    if conn.stream.is_none() {
        let s = TcpStream::connect((host.as_str(), port))
            .map_err(|e| format!("connect 失败: {}", e))?;
        let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = s.set_write_timeout(Some(Duration::from_secs(5)));
        conn.stream = Some(s);
    }
    let stream = conn.stream.as_mut().unwrap();
    let req = format!(
        "DELETE {} HTTP/1.1\r\nHost: {}:{}\r\nMcp-Session-Id: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        path, host, port, conn.session_id
    );
    let _ = stream.write_all(req.as_bytes());
    let _ = stream.flush();
    let _ = read_one_http_response(stream);
    conn.stream = None;
    Ok(())
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

    let (session_id, init_body_text, stream) = match http_extract_session(&url, &init_body) {
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

    // 复用 initialize 时建立的 keep-alive 连接，保持会话存活
    let http = Arc::new(Mutex::new(HttpConn {
        base_url: url,
        session_id,
        stream,
    }));

    // 发送 notifications/initialized（同一条连接；忽略 202/空体）
    {
        let notif = json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string();
        if let Ok(mut c) = http.lock() {
            let _ = http_post_keepalive(&mut c, &notif, 10);
        }
    }

    let conn_id = next_conn_id();
    let conn = Arc::new(Connection {
        transport: Transport::Http { http },
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

        Transport::Http { http } => {
            // 在【持久 keep-alive 连接】上发请求并精确读回一条响应。
            // 复用连接是保持 Playwright 会话存活的关键（Connection: close 会杀会话）。
            let mut c = match http.lock() {
                Ok(g) => g,
                Err(_) => return empty_cstr(),
            };
            match http_post_keepalive(&mut c, &request_str, 60) {
                Ok((_headers, body)) => {
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
        Transport::Http { http } => {
            if let Ok(mut c) = http.lock() {
                if !c.session_id.is_empty() {
                    // 发 DELETE 关闭会话（忽略失败）
                    let _ = http_delete_keepalive(&mut c);
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
    fn test_decode_chunked() {
        // 5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n
        let raw = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        assert_eq!(decode_chunked(raw), "hello world");
    }

    #[test]
    fn test_find_header_value_case_insensitive() {
        let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nMcp-Session-Id: abc123";
        assert_eq!(find_header_value(headers, "mcp-session-id").as_deref(), Some("abc123"));
        assert_eq!(find_header_value(headers, "Content-Length"), None);
    }

    #[test]
    fn test_parse_http_url() {
        assert_eq!(parse_http_url("http://localhost:43570/mcp").unwrap(),
            ("localhost".to_string(), 43570u16, "/mcp".to_string()));
        assert_eq!(parse_http_url("http://127.0.0.1:8/x/y").unwrap(),
            ("127.0.0.1".to_string(), 8u16, "/x/y".to_string()));
        assert!(parse_http_url("https://x/y").is_err());
    }

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
