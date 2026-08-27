//! 标准库.大模型 的 对话 / 嵌入 改成 qi 写之后的两道防线。
//!
//! ## 一、行为对照
//!
//! 同一份 .qi 跑两遍（qi 实现 / QI_STDLIB_FFI=大模型 走回 Rust），
//! 输出逐字节比。语料覆盖三家 provider、系统提示、response_format、
//! 多轮历史、预算闸、无效句柄。
//!
//! ## 二、磁带键必须一模一样 —— 这条才是真正危险的地方
//!
//! 磁带键 = 请求体序列化文本的哈希，而 serde_json 开了 preserve_order，
//! **键序即插入序**。qi 侧的请求成形只要有一个字段换了位置，键就变了，
//! 后果是所有历史录制**静默回放未命中** —— AIOne 课程那批磁带全靠它命中，
//! 而失败长得像「模型今天答得不一样」，不像 bug。
//!
//! 所以这里用 Rust 实现录一盘、再用 qi 实现在 QI_LLM_REPLAY=1 下回放。
//! REPLAY 模式不许联网，键错一位就是硬错误。反方向也跑一遍。
//!
//! 这条已经抓到过一个真 bug：Anthropic 的 assistant 历史消息 content 是
//! **块数组**不是字符串，qi 版原样传了字符串 —— 第一轮看不出来（历史空），
//! 第二轮才炸。没有这道防线的话，它会一路活到线上。

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

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

/// 起一个假 LLM 服务：按路径分辨三家，回固定响应。常驻直到进程退出。
///
/// 固定响应是**故意**的：这些测试要证明的是「两条实现发出的请求一样、
/// 对响应的处理一样」，不是模型答得对不对。响应一变，比对就失去意义。
fn 起假服务() -> String {
    let 监听 = TcpListener::bind("127.0.0.1:0").unwrap();
    let 端口 = 监听.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for 连 in 监听.incoming() {
            let Ok(mut 连) = 连 else { continue };
            std::thread::spawn(move || {
                let mut 缓冲 = vec![0u8; 65536];
                let n = match 连.read(&mut 缓冲) {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let 请求 = String::from_utf8_lossy(&缓冲[..n]).into_owned();
                let 首行 = 请求.lines().next().unwrap_or("");
                // OpenAI 的多候选要按请求里的 n 回相应条数，否则 对话多候选
                // 测不出「候选个数」这一维
                let 候选数 = 请求
                    .split("\"n\":")
                    .nth(1)
                    .and_then(|尾| {
                        尾.trim_start()
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse::<usize>()
                            .ok()
                    })
                    .unwrap_or(1)
                    .max(1);
                let 多候选体 = {
                    let 各条: Vec<String> = (0..候选数)
                        .map(|i| {
                            format!(
                                r#"{{"message":{{"role":"assistant","content":"候选{}"}}}}"#,
                                i
                            )
                        })
                        .collect();
                    format!(
                        r#"{{"choices":[{}],"usage":{{"prompt_tokens":11,"completion_tokens":{},"total_tokens":{}}}}}"#,
                        各条.join(","),
                        7 * 候选数,
                        11 + 7 * 候选数
                    )
                };
                let 体 = if 首行.contains("generateContent") {
                    r#"{"candidates":[{"content":{"parts":[{"text":"gemini答"}]}}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8}}"#
                } else if 首行.contains("/messages") {
                    r#"{"content":[{"type":"text","text":"claude答"}],"usage":{"input_tokens":6,"output_tokens":4}}"#
                } else if 首行.contains("/embeddings") {
                    r#"{"data":[{"embedding":[0.25,0.5,0.75,1.0]}],"usage":{"prompt_tokens":3,"total_tokens":3}}"#
                } else {
                    多候选体.as_str()
                };
                let 响应 = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    体.len(),
                    体
                );
                let _ = 连.write_all(响应.as_bytes());
                let _ = 连.flush();
            });
        }
    });
    format!("http://127.0.0.1:{}", 端口)
}

/// 把语料里的 __端点__ 换成真端点，写进独占临时目录后跑。
/// 返回 (stdout, stderr)。
fn 跑(语料: &str, 端点: &str, 环境: &[(&str, &str)], 走ffi: bool) -> (String, String) {
    let 源文本 = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/大模型语料")
            .join(语料),
    )
    .unwrap()
    .replace("__端点__", 端点);
    let 临时 = tempfile::tempdir().unwrap();
    let 文件 = 临时.path().join(语料);
    std::fs::write(&文件, 源文本).unwrap();

    let mut cmd = Command::new(编译器());
    cmd.arg("run").arg(&文件);
    for (k, v) in 环境 {
        cmd.env(k, v);
    }
    if 走ffi {
        cmd.env("QI_STDLIB_FFI", "大模型");
    } else {
        cmd.env_remove("QI_STDLIB_FFI");
    }
    let 出 = cmd.output().expect("起不来 qi");
    (
        String::from_utf8_lossy(&出.stdout).into_owned(),
        String::from_utf8_lossy(&出.stderr).into_owned(),
    )
}

fn 对照(语料: &str) {
    if !运行时就位() {
        eprintln!("跳过：未找到 qi-runtime 归档");
        return;
    }
    let 端点 = 起假服务();
    let (qi出, qi错) = 跑(语料, &端点, &[], false);
    let (rust出, rust错) = 跑(语料, &端点, &[], true);
    assert!(!qi出.trim().is_empty(), "{} 没有输出", 语料);
    assert_eq!(qi出, rust出, "{} 的 stdout 不一致", 语料);
    // stderr 也要比：嵌入失败只走 stderr，只比 stdout 的话丢掉整条诊断也照样绿
    assert_eq!(qi出.is_empty(), rust出.is_empty());
    assert_eq!(qi错, rust错, "{} 的 stderr 不一致", 语料);
}

#[test]
fn 会话状态_qi与ffi逐字节一致() {
    对照("会话状态.qi");
}

#[test]
fn 三家provider_qi与ffi逐字节一致() {
    对照("三家对话.qi");
}

/// 多候选：openai 走请求体里的 n，anthropic/gemini 没有 n 语义 → **串行 n 次**。
/// 串行那条最容易写错的是历史：n 次请求都不能写历史，最后只写一次、且只写
/// 第一个候选，否则后续对话的上下文里多出 n-1 组重复问答。
#[test]
fn 多候选_qi与ffi逐字节一致() {
    对照("多候选.qi");
}

/// 图像：user content 是**块数组**，三家形状都不同（openai 原样 /
/// anthropic image+source / gemini file_data）。语料里带一次「看过图之后追问」——
/// 那一轮的历史里躺着数组 content，成形代码要能继续处理。
#[test]
fn 图像_qi与ffi逐字节一致() {
    对照("图像.qi");
}

/// Rust 录、qi 放。键错一位就在 REPLAY 下硬报未命中。
#[test]
fn 磁带_rust录qi放() {
    if !运行时就位() {
        return;
    }
    let 端点 = 起假服务();
    let 临时 = tempfile::tempdir().unwrap();
    let 磁带 = 临时.path().join("tape.json");
    let 磁带路径 = 磁带.to_string_lossy().to_string();

    let (_, _) = 跑(
        "三家对话.qi",
        &端点,
        &[("QI_LLM_RECORD", "1"), ("QI_LLM_TAPE", &磁带路径)],
        true,
    );
    let 录了 = std::fs::read_to_string(&磁带).expect("没录出磁带");
    let 键数 = serde_json::from_str::<serde_json::Value>(&录了)
        .unwrap()
        .as_object()
        .unwrap()
        .len();
    assert!(键数 >= 6, "录到的磁带条数不对：{}", 键数);

    // 回放时把端点换成一个**连不上**的地址：万一键没命中而代码悄悄去联网，
    // 这里会直接失败，而不是偷偷真调一次 API 让测试蒙混过关。
    let (出, 错) = 跑(
        "三家对话.qi",
        "http://127.0.0.1:1",
        &[("QI_LLM_REPLAY", "1"), ("QI_LLM_TAPE", &磁带路径)],
        false,
    );
    assert!(
        !出.contains("未命中") && !错.contains("未命中"),
        "qi 实现拼出的请求体跟 Rust 版不同 —— 磁带键变了，所有历史录制都会静默失配。\n{}\n{}",
        出,
        错
    );
    // openai 分支的假响应是「候选0…」（多候选共用同一个 fixture）
    assert!(
        出.contains("claude答") && 出.contains("gemini答") && 出.contains("候选0"),
        "回放出来的内容不对：\n{}",
        出
    );
}

/// 反方向：qi 录、Rust 放。两个方向都过才说明键真的相同，
/// 而不是「qi 自己跟自己一致」。
#[test]
fn 磁带_qi录rust放() {
    if !运行时就位() {
        return;
    }
    let 端点 = 起假服务();
    let 临时 = tempfile::tempdir().unwrap();
    let 磁带 = 临时.path().join("tape.json");
    let 磁带路径 = 磁带.to_string_lossy().to_string();

    跑(
        "三家对话.qi",
        &端点,
        &[("QI_LLM_RECORD", "1"), ("QI_LLM_TAPE", &磁带路径)],
        false,
    );
    assert!(磁带.exists(), "qi 实现没录出磁带");

    let (出, 错) = 跑(
        "三家对话.qi",
        "http://127.0.0.1:1",
        &[("QI_LLM_REPLAY", "1"), ("QI_LLM_TAPE", &磁带路径)],
        true,
    );
    assert!(
        !出.contains("未命中") && !错.contains("未命中"),
        "Rust 实现放不了 qi 录的磁带：\n{}\n{}",
        出,
        错
    );
}

/// 逃生口得真能切回去 —— 它坏了的话上面几条对照会退化成自己跟自己比。
/// 用「Rust 版嵌入错误信息带句柄号、两边措辞必须都对得上」以外的办法验：
/// 直接看两条路径下 QI_STDLIB_FFI 是否真的改变了分发 —— 拿一个只有 qi 版
/// 才认识的行为不好找，所以退一步：确认设了这个变量之后程序仍然跑得通，
/// 且磁带互放（上面两条）成立，即两条实现确实都被执行过。
#[test]
fn 逃生口能跑通() {
    if !运行时就位() {
        return;
    }
    let 端点 = 起假服务();
    let (出, _) = 跑("会话状态.qi", &端点, &[], true);
    assert!(出.contains("答1="), "走 FFI 时程序没跑通:\n{}", 出);
}
