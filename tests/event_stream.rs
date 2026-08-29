//! SSE 分帧（标准库.事件流，用 qi 写）跑在真 socket 上的端到端验证。
//!
//! 为什么要起真服务而不是喂一段固定字符串：这个模块的全部难点都在**分块边界**
//! 上 —— 事件被网络切在哪里完全随机，而分帧代码最容易错的就是「一条事件跨了
//! 两个块」「一个块里躺着两条事件」「最后一条没有尾随空行」。喂完整字符串
//! 等于把这些情况全绕过去，测了个寂寞。
//!
//! 服务端按脚本逐块 sendall + sleep，精确控制切在哪。

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn qi_binary() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    ["release", "debug"]
        .iter()
        .map(|c| manifest.join("../target").join(c).join("qi"))
        .find(|p| p.exists())
        .expect("找不到 qi 二进制（先 cargo build --release）")
}

fn runtime_ready() -> bool {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    ["release", "debug"].iter().any(|c| {
        manifest
            .join("../qi-runtime/target")
            .join(c)
            .join("libqi_runtime.a")
            .exists()
    })
}

/// 起一次性 SSE 服务，按 分片 逐块写出。返回端点。
fn start_sse(chunks: Vec<&'static [u8]>, gap: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut conn, _)) = listener.accept() {
            let mut scratch = [0u8; 8192];
            let _ = conn.read(&mut scratch);
            let _ = conn.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            );
            let _ = conn.flush();
            for 片 in chunks {
                std::thread::sleep(gap);
                if conn.write_all(片).is_err() {
                    return;
                }
                let _ = conn.flush();
            }
        }
    });
    format!("http://127.0.0.1:{}/", port)
}

/// 把 qi 源码写进独占临时目录再跑（`qi run` 的产物路径按源文件名定，
/// 并行跑同名文件会互相覆写）。返回 stdout。
fn run_qi(program: &str) -> String {
    let tmp = tempfile::tempdir().expect("建不了临时目录");
    let file = tmp.path().join("事件流用例.qi");
    std::fs::write(&file, program).unwrap();
    let out = Command::new(qi_binary())
        .arg("run")
        .arg(&file)
        .env_remove("QI_STDLIB_FFI")
        .output()
        .expect("起不来 qi");
    assert!(
        out.status.success(),
        "qi 跑失败:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn sse_reader_program(endpoint: &str) -> String {
    format!(
        r#"导入 标准库.事件流;

函数 入口() {{
    变量 流: 整数 = 事件流.打开("GET", "{endpoint}", "", "");
    如果 (流 <= 0) {{ 打印行("开流失败"); 返回; }}
    变量 n: 整数 = 0;
    当 (n < 100) {{
        如果 (事件流.下一条(流, 3000) == 1) {{
            n = n + 1;
            打印行("EV|" + 事件流.事件名(流) + "|" + 事件流.事件id(流) + "|" + 事件流.数据(流));
            继续;
        }}
        如果 (事件流.已结束(流) == 1) {{ 打印行("END"); 跳出; }}
        如果 (事件流.是出错(流) == 1) {{ 打印行("ERR|" + 事件流.错误(流)); 跳出; }}
        打印行("TIMEOUT");
    }}
    事件流.关闭(流);
}}
"#
    )
}

#[test]
fn basic_framing_and_multi_data_lines() {
    if !runtime_ready() {
        eprintln!("跳过：未找到 qi-runtime 归档");
        return;
    }
    let endpoint = start_sse(
        vec![
            b"data: hello\n\n",
            b"event: ping\ndata: first\ndata: second\nid: 42\n\n",
        ],
        Duration::from_millis(30),
    );
    let out = run_qi(&sse_reader_program(&endpoint));
    assert!(out.contains("EV|||hello"), "少了第一条:\n{}", out);
    // 多个 data 行按换行拼接（规范如此），不是后盖前
    assert!(
        out.contains("EV|ping|42|first\nsecond"),
        "多 data 行没拼对:\n{}",
        out
    );
    assert!(out.contains("END"), "没读到结束:\n{}", out);
}

/// 心跳注释块不该变成一条 data 为空的事件 —— 否则调用方分不清
/// 「服务端在保活」和「真来了一条空事件」。
#[test]
fn comment_heartbeat_yields_no_event() {
    if !runtime_ready() {
        return;
    }
    let endpoint = start_sse(
        vec![b": keep-alive\n\n", b"data: real\n\n", b": ping\n\n"],
        Duration::from_millis(20),
    );
    let out = run_qi(&sse_reader_program(&endpoint));
    let n_events = out.lines().filter(|l| l.starts_with("EV|")).count();
    assert_eq!(n_events, 1, "心跳被当成事件了:\n{}", out);
    assert!(out.contains("EV|||real"));
}

/// 流末尾允许省略空行（规范允许）。不补这一下就会静默丢掉最后一条 ——
/// OpenAI 的 `data: [DONE]` 正好是这个位置。
#[test]
fn last_event_without_trailing_blank_line_kept() {
    if !runtime_ready() {
        return;
    }
    let endpoint = start_sse(
        vec![b"data: one\n\n", b"data: [DONE]"],
        Duration::from_millis(20),
    );
    let out = run_qi(&sse_reader_program(&endpoint));
    assert!(out.contains("EV|||one"), "{}", out);
    assert!(out.contains("EV|||[DONE]"), "末条被丢了:\n{}", out);
}

/// 一条事件被网络切成两半，且切点落在**汉字中间**。
/// 这是整条链路（Rust 侧 UTF-8 边界缓冲 + qi 侧跨块拼接）的会合点。
#[test]
fn event_split_mid_cjk_char() {
    if !runtime_ready() {
        return;
    }
    // "中" = E4 B8 AD，故意在第二个字节后切开
    let endpoint = start_sse(
        vec![b"data: {\"delta\":\"\xe4\xb8", b"\xad\xe6\x96\x87\"}\n\n"],
        Duration::from_millis(40),
    );
    let out = run_qi(&sse_reader_program(&endpoint));
    assert!(
        out.contains(r#"EV|||{"delta":"中文"}"#),
        "汉字跨块被弄坏了:\n{}",
        out
    );
    assert!(!out.contains('\u{fffd}'), "出现替换字符:\n{}", out);
}

/// 一个网络块里同时躺着两条事件 —— 第二条必须当场就能取出来，
/// 而不是等到下一次网络活动。
#[test]
fn two_events_in_one_chunk_both_available() {
    if !runtime_ready() {
        return;
    }
    let endpoint = start_sse(vec![b"data: a\n\ndata: b\n\n"], Duration::from_millis(10));
    let out = run_qi(&sse_reader_program(&endpoint));
    let events: Vec<&str> = out.lines().filter(|l| l.starts_with("EV|")).collect();
    assert_eq!(events, vec!["EV|||a", "EV|||b"], "{}", out);
    // 中间不该夹着超时 —— 夹了就说明第二条是等网络等出来的
    let first_timeout = out.lines().position(|l| l == "TIMEOUT");
    let second_at = out.lines().position(|l| l == "EV|||b").unwrap();
    if let Some(t) = first_timeout {
        assert!(t > second_at, "取第二条之前等了网络:\n{}", out);
    }
}

/// \r\n 行尾（规范允许，真实服务端有用的）。只认 \n 的话
/// "data: [DONE]\r" 跟 "data: [DONE]" 比不相等，判终止的地方会漏。
#[test]
fn crlf_line_endings_accepted() {
    if !runtime_ready() {
        return;
    }
    let endpoint = start_sse(
        vec![b"data: crlf\r\n\r\n", b"event: e\r\ndata: v\r\n\r\n"],
        Duration::from_millis(20),
    );
    let out = run_qi(&sse_reader_program(&endpoint));
    assert!(out.contains("EV|||crlf"), "{}", out);
    assert!(out.contains("EV|e||v"), "{}", out);
    assert!(!out.contains('\r'), "行尾的 \\r 没去干净:\n{:?}", out);
}
