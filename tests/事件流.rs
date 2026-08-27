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

fn 编译器() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    ["release", "debug"]
        .iter()
        .map(|c| manifest.join("../target").join(c).join("qi"))
        .find(|p| p.exists())
        .expect("找不到 qi 二进制（先 cargo build --release）")
}

fn 运行时就位() -> bool {
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
fn 起SSE(分片: Vec<&'static [u8]>, 每块间隔: Duration) -> String {
    let 监听 = TcpListener::bind("127.0.0.1:0").unwrap();
    let 端口 = 监听.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut 连, _)) = 监听.accept() {
            let mut 丢 = [0u8; 8192];
            let _ = 连.read(&mut 丢);
            let _ = 连.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            );
            let _ = 连.flush();
            for 片 in 分片 {
                std::thread::sleep(每块间隔);
                if 连.write_all(片).is_err() {
                    return;
                }
                let _ = 连.flush();
            }
        }
    });
    format!("http://127.0.0.1:{}/", 端口)
}

/// 把 qi 源码写进独占临时目录再跑（`qi run` 的产物路径按源文件名定，
/// 并行跑同名文件会互相覆写）。返回 stdout。
fn 跑qi(源码: &str) -> String {
    let 临时 = tempfile::tempdir().expect("建不了临时目录");
    let 文件 = 临时.path().join("事件流用例.qi");
    std::fs::write(&文件, 源码).unwrap();
    let 出 = Command::new(编译器())
        .arg("run")
        .arg(&文件)
        .env_remove("QI_STDLIB_FFI")
        .output()
        .expect("起不来 qi");
    assert!(
        出.status.success(),
        "qi 跑失败:\n{}\n{}",
        String::from_utf8_lossy(&出.stdout),
        String::from_utf8_lossy(&出.stderr)
    );
    String::from_utf8_lossy(&出.stdout).into_owned()
}

fn 读全部事件的源码(端点: &str) -> String {
    format!(
        r#"导入 标准库.事件流;

函数 入口() {{
    变量 流: 整数 = 事件流.打开("GET", "{端点}", "", "");
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
fn 基本分帧与多data行拼接() {
    if !运行时就位() {
        eprintln!("跳过：未找到 qi-runtime 归档");
        return;
    }
    let 端点 = 起SSE(
        vec![
            b"data: hello\n\n",
            b"event: ping\ndata: first\ndata: second\nid: 42\n\n",
        ],
        Duration::from_millis(30),
    );
    let 出 = 跑qi(&读全部事件的源码(&端点));
    assert!(出.contains("EV|||hello"), "少了第一条:\n{}", 出);
    // 多个 data 行按换行拼接（规范如此），不是后盖前
    assert!(出.contains("EV|ping|42|first\nsecond"), "多 data 行没拼对:\n{}", 出);
    assert!(出.contains("END"), "没读到结束:\n{}", 出);
}

/// 心跳注释块不该变成一条 data 为空的事件 —— 否则调用方分不清
/// 「服务端在保活」和「真来了一条空事件」。
#[test]
fn 注释心跳不产生事件() {
    if !运行时就位() {
        return;
    }
    let 端点 = 起SSE(
        vec![b": keep-alive\n\n", b"data: real\n\n", b": ping\n\n"],
        Duration::from_millis(20),
    );
    let 出 = 跑qi(&读全部事件的源码(&端点));
    let 事件数 = 出.lines().filter(|l| l.starts_with("EV|")).count();
    assert_eq!(事件数, 1, "心跳被当成事件了:\n{}", 出);
    assert!(出.contains("EV|||real"));
}

/// 流末尾允许省略空行（规范允许）。不补这一下就会静默丢掉最后一条 ——
/// OpenAI 的 `data: [DONE]` 正好是这个位置。
#[test]
fn 末条无尾随空行也不丢() {
    if !运行时就位() {
        return;
    }
    let 端点 = 起SSE(
        vec![b"data: one\n\n", b"data: [DONE]"],
        Duration::from_millis(20),
    );
    let 出 = 跑qi(&读全部事件的源码(&端点));
    assert!(出.contains("EV|||one"), "{}", 出);
    assert!(出.contains("EV|||[DONE]"), "末条被丢了:\n{}", 出);
}

/// 一条事件被网络切成两半，且切点落在**汉字中间**。
/// 这是整条链路（Rust 侧 UTF-8 边界缓冲 + qi 侧跨块拼接）的会合点。
#[test]
fn 事件跨块且切在汉字中间() {
    if !运行时就位() {
        return;
    }
    // "中" = E4 B8 AD，故意在第二个字节后切开
    let 端点 = 起SSE(
        vec![
            b"data: {\"delta\":\"\xe4\xb8",
            b"\xad\xe6\x96\x87\"}\n\n",
        ],
        Duration::from_millis(40),
    );
    let 出 = 跑qi(&读全部事件的源码(&端点));
    assert!(
        出.contains(r#"EV|||{"delta":"中文"}"#),
        "汉字跨块被弄坏了:\n{}",
        出
    );
    assert!(!出.contains('\u{fffd}'), "出现替换字符:\n{}", 出);
}

/// 一个网络块里同时躺着两条事件 —— 第二条必须当场就能取出来，
/// 而不是等到下一次网络活动。
#[test]
fn 一块里的两条事件都能立刻取出() {
    if !运行时就位() {
        return;
    }
    let 端点 = 起SSE(vec![b"data: a\n\ndata: b\n\n"], Duration::from_millis(10));
    let 出 = 跑qi(&读全部事件的源码(&端点));
    let 事件: Vec<&str> = 出.lines().filter(|l| l.starts_with("EV|")).collect();
    assert_eq!(事件, vec!["EV|||a", "EV|||b"], "{}", 出);
    // 中间不该夹着超时 —— 夹了就说明第二条是等网络等出来的
    let 首个超时 = 出.lines().position(|l| l == "TIMEOUT");
    let 第二条 = 出.lines().position(|l| l == "EV|||b").unwrap();
    if let Some(t) = 首个超时 {
        assert!(t > 第二条, "取第二条之前等了网络:\n{}", 出);
    }
}

/// \r\n 行尾（规范允许，真实服务端有用的）。只认 \n 的话
/// "data: [DONE]\r" 跟 "data: [DONE]" 比不相等，判终止的地方会漏。
#[test]
fn CRLF行尾也认() {
    if !运行时就位() {
        return;
    }
    let 端点 = 起SSE(
        vec![b"data: crlf\r\n\r\n", b"event: e\r\ndata: v\r\n\r\n"],
        Duration::from_millis(20),
    );
    let 出 = 跑qi(&读全部事件的源码(&端点));
    assert!(出.contains("EV|||crlf"), "{}", 出);
    assert!(出.contains("EV|e||v"), "{}", 出);
    assert!(!出.contains('\r'), "行尾的 \\r 没去干净:\n{:?}", 出);
}
