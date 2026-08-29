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

/// 起一个假 LLM 服务：按路径分辨三家，回固定响应。常驻直到进程退出。
///
/// 固定响应是**故意**的：这些测试要证明的是「两条实现发出的请求一样、
/// 对响应的处理一样」，不是模型答得对不对。响应一变，比对就失去意义。
fn start_fake_llm() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            std::thread::spawn(move || {
                let mut buf = vec![0u8; 65536];
                let n = match conn.read(&mut buf) {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let first_line = req.lines().next().unwrap_or("");
                // OpenAI 的多候选要按请求里的 n 回相应条数，否则 对话多候选
                // 测不出「候选个数」这一维
                let n_choices = req
                    .split("\"n\":")
                    .nth(1)
                    .and_then(|rest| {
                        rest.trim_start()
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse::<usize>()
                            .ok()
                    })
                    .unwrap_or(1)
                    .max(1);
                let multi_body = {
                    let items: Vec<String> = (0..n_choices)
                        .map(|i| {
                            format!(
                                r#"{{"message":{{"role":"assistant","content":"候选{}"}}}}"#,
                                i
                            )
                        })
                        .collect();
                    format!(
                        r#"{{"choices":[{}],"usage":{{"prompt_tokens":11,"completion_tokens":{},"total_tokens":{}}}}}"#,
                        items.join(","),
                        7 * n_choices,
                        11 + 7 * n_choices
                    )
                };
                // 流式：按 SSE 逐帧写出（chunked）。这里必须真的分块 + 有心跳 +
                // 有「只有 role 没有内容」的首帧，那三种正是分帧最容易写错的地方。
                if req.contains("\"stream\":true") {
                    let sse_header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
                    let _ = conn.write_all(sse_header.as_bytes());
                    let _ = conn.flush();
                    for frame in [
                        "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n\n",
                        ": keep-alive\n\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"好，\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"世界\"}}]}\n\n",
                        "data: [DONE]\n\n",
                    ] {
                        let b = frame.as_bytes();
                        let _ = conn.write_all(format!("{:X}\r\n", b.len()).as_bytes());
                        let _ = conn.write_all(b);
                        let _ = conn.write_all(b"\r\n");
                        let _ = conn.flush();
                        std::thread::sleep(std::time::Duration::from_millis(15));
                    }
                    let _ = conn.write_all(b"0\r\n\r\n");
                    let _ = conn.flush();
                    return;
                }
                // 工具轮：历史里已经有工具结果就答最终文本，否则要求调工具。
                // 三家的「工具结果」形状完全不同，这正是最容易拼错的地方：
                // openai 是 role:"tool"，anthropic 是 user 消息里的 tool_result 块，
                // gemini 是 functionResponse 部件。哪一家拼错了，模型就看不到结果、
                // 于是把同一个工具再调一遍 —— 看着像「模型不听话」而不是形状错。
                let wants_tools = req.contains("\"tools\"");
                let has_tool_result = req.contains("\"role\":\"tool\"")
                    || req.contains("tool_result")
                    || req.contains("functionResponse");
                let tool_body = if first_line.contains("generateContent") {
                    if has_tool_result {
                        r#"{"candidates":[{"content":{"parts":[{"text":"gemini最终答"}]}}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8}}"#
                    } else {
                        r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"qi_tool_e69fa5e5a4a9e6b094","args":{"城市":"北京"}}}]}}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8}}"#
                    }
                } else if first_line.contains("/messages") {
                    if has_tool_result {
                        r#"{"content":[{"type":"text","text":"claude最终答"}],"usage":{"input_tokens":6,"output_tokens":4}}"#
                    } else {
                        r#"{"content":[{"type":"tool_use","id":"toolu_1","name":"qi_tool_e69fa5e5a4a9e6b094","input":{"城市":"北京"}}],"usage":{"input_tokens":6,"output_tokens":4}}"#
                    }
                } else if has_tool_result {
                    r#"{"choices":[{"message":{"role":"assistant","content":"openai最终答"}}],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}}"#
                } else {
                    r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"qi_tool_e69fa5e5a4a9e6b094","arguments":"{\"城市\":\"北京\"}"}}]}}],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}}"#
                };
                let body = if wants_tools || has_tool_result {
                    tool_body
                } else if first_line.contains("generateContent") {
                    r#"{"candidates":[{"content":{"parts":[{"text":"gemini答"}]}}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8}}"#
                } else if first_line.contains("/messages") {
                    r#"{"content":[{"type":"text","text":"claude答"}],"usage":{"input_tokens":6,"output_tokens":4}}"#
                } else if first_line.contains("/embeddings") {
                    r#"{"data":[{"embedding":[0.25,0.5,0.75,1.0]}],"usage":{"prompt_tokens":3,"total_tokens":3}}"#
                } else {
                    multi_body.as_str()
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = conn.write_all(resp.as_bytes());
                let _ = conn.flush();
            });
        }
    });
    format!("http://127.0.0.1:{}", port)
}

/// 把语料里的 __端点__ 换成真端点，写进独占临时目录后跑。
/// 返回 (stdout, stderr)。
fn run_qi(fixture: &str, endpoint: &str, env: &[(&str, &str)], use_ffi: bool) -> (String, String) {
    let src_text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/大模型语料")
            .join(fixture),
    )
    .unwrap()
    .replace("__端点__", endpoint);
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join(fixture);
    std::fs::write(&file, src_text).unwrap();

    let mut cmd = Command::new(qi_binary());
    cmd.arg("run").arg(&file);
    for (k, v) in env {
        cmd.env(k, v);
    }
    if use_ffi {
        cmd.env("QI_STDLIB_FFI", "大模型");
    } else {
        cmd.env_remove("QI_STDLIB_FFI");
    }
    let out = cmd.output().expect("起不来 qi");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn compare(fixture: &str) {
    if !runtime_ready() {
        eprintln!("跳过：未找到 qi-runtime 归档");
        return;
    }
    let endpoint = start_fake_llm();
    let (qi_out, qi_err) = run_qi(fixture, &endpoint, &[], false);
    let (ffi_out, ffi_err) = run_qi(fixture, &endpoint, &[], true);
    assert!(!qi_out.trim().is_empty(), "{} 没有输出", fixture);
    assert_eq!(qi_out, ffi_out, "{} 的 stdout 不一致", fixture);
    // stderr 也要比：嵌入失败只走 stderr，只比 stdout 的话丢掉整条诊断也照样绿
    assert_eq!(qi_out.is_empty(), ffi_out.is_empty());
    assert_eq!(qi_err, ffi_err, "{} 的 stderr 不一致", fixture);
}

#[test]
fn session_state_matches_ffi() {
    compare("会话状态.qi");
}

#[test]
fn three_providers_match_ffi() {
    compare("三家对话.qi");
}

/// 多候选：openai 走请求体里的 n，anthropic/gemini 没有 n 语义 → **串行 n 次**。
/// 串行那条最容易写错的是历史：n 次请求都不能写历史，最后只写一次、且只写
/// 第一个候选，否则后续对话的上下文里多出 n-1 组重复问答。
#[test]
fn multi_choice_matches_ffi() {
    compare("多候选.qi");
}

/// 图像：user content 是**块数组**，三家形状都不同（openai 原样 /
/// anthropic image+source / gemini file_data）。语料里带一次「看过图之后追问」——
/// 那一轮的历史里躺着数组 content，成形代码要能继续处理。
#[test]
fn image_chat_matches_ffi() {
    compare("图像.qi");
}

/// 流式：SSE 分帧交给 标准库.事件流，增量拼接和历史落账在 qi。
/// 语料覆盖多轮流式、流式之后接非流式（历史要接得上）、半途关流、坏句柄。
#[test]
fn streaming_matches_ffi() {
    compare("流式.qi");
}

/// 工具调用：带工具的请求成形（三家形状全不同）、tool_calls 读取、
/// 工具结果回填历史、续传。语料跑完整一轮 调用→执行→回填→续传。
///
/// 最容易错的是**工具结果的形状**：openai 是 role:"tool"，anthropic 要翻成
/// user 消息里的 tool_result 块，gemini 要翻成 functionResponse 部件。
/// 拼错了模型看不到结果，会把同一个工具再调一遍 —— 表现像「模型不听话」。
/// 第一版 qi 实现就漏了 anthropic / gemini 这两条，被这条语料当场比出来。
#[test]
fn tool_calling_matches_ffi() {
    compare("工具调用.qi");
}

/// 取不到工具调用时（越界 / 消息里压根没有 tool_calls / 坏 JSON），
/// qi 版返回**空串**，Rust 版返回**空指针**。这是故意不一致，且是修 bug：
///
/// 空指针进到 qi 的字符串拼接里，整条 `打印行(...)` 会**静默消失** ——
/// 没有输出、没有报错、没有崩溃。qi-harness 的 对话.qi 就在用这两个索引
/// 访问器（工具调用ID索引 / 工具调用名称索引），一旦模型少返一个调用，
/// 上层某行日志就凭空不见了，完全无从察觉。
///
/// 所以这条不进 compare 语料，单独钉住 qi 的行为。
#[test]
fn tool_accessors_return_empty_string_not_null() {
    if !runtime_ready() {
        return;
    }
    let endpoint = start_fake_llm();
    let (out, _) = run_qi("工具越界.qi", &endpoint, &[], false);
    for expected in [
        "越界ID=[]",
        "越界参数=[]",
        "无调用ID=[]",
        "无调用参数=[]",
        "坏JSON_ID=[]",
        "数量=0",
        "收尾",
    ] {
        assert!(out.contains(expected), "缺少 `{}`：\n{}", expected, out);
    }

    // 对照：走 FFI 时那几行确实是**整行消失**（不是内容不同）
    let (ffi_out, _) = run_qi("工具越界.qi", &endpoint, &[], true);
    assert!(
        !ffi_out.contains("越界ID="),
        "Rust 版居然打出来了？那这条测试的前提变了，重新确认：\n{}",
        ffi_out
    );
    assert!(ffi_out.contains("收尾"), "程序应当照常跑完：\n{}", ffi_out);
}

/// 半途关流**不录磁带** —— 否则磁带里存下的是截断的回答，之后每次回放都
/// 拿到半句话，而且完全看不出是磁带的问题。
/// 语料里最后那次「开了不读就关」正是这种，回放时必须报未命中而不是给半句。
#[test]
fn stream_tape_both_ways_and_partial_not_recorded() {
    if !runtime_ready() {
        return;
    }
    for (record_via_ffi, label) in [(true, "rust录qi放"), (false, "qi录rust放")] {
        let endpoint = start_fake_llm();
        let tmp = tempfile::tempdir().unwrap();
        let tape = tmp.path().join("tape.json");
        let tape_path = tape.to_string_lossy().to_string();

        run_qi(
            "流式.qi",
            &endpoint,
            &[("QI_LLM_RECORD", "1"), ("QI_LLM_TAPE", &tape_path)],
            record_via_ffi,
        );
        let content = std::fs::read_to_string(&tape).expect("没录出磁带");
        let map: serde_json::Value = serde_json::from_str(&content).unwrap();
        let n_stream_tapes = map
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with("stream:"))
            .count();
        // 三次流式调用，但「半途关流」那次不录 → 只应有 2 条
        assert_eq!(
            n_stream_tapes, 2,
            "{}: 流式磁带条数不对（半途关流不该被录）",
            label
        );

        // 端点换成连不上的：键没命中而偷偷联网会当场失败，不会蒙混过关
        let (out, 错) = run_qi(
            "流式.qi",
            "http://127.0.0.1:1",
            &[("QI_LLM_REPLAY", "1"), ("QI_LLM_TAPE", &tape_path)],
            !record_via_ffi,
        );
        assert!(
            out.contains("你好，世界"),
            "{}: 回放不出内容:\n{}\n{}",
            label,
            out,
            错
        );
        // 恰好一条未命中 = 半途那次；多了就是键不匹配
        let n_miss = 错.matches("未命中").count() + out.matches("未命中").count();
        assert_eq!(
            n_miss, 1,
            "{}: 未命中次数应恰为 1（半途那次）。多了说明键不匹配。\n{}\n{}",
            label, out, 错
        );
    }
}

/// Rust 录、qi 放。键错一位就在 REPLAY 下硬报未命中。
#[test]
fn tape_recorded_by_ffi_replays_in_qi() {
    if !runtime_ready() {
        return;
    }
    let endpoint = start_fake_llm();
    let tmp = tempfile::tempdir().unwrap();
    let tape = tmp.path().join("tape.json");
    let tape_path = tape.to_string_lossy().to_string();

    let (_, _) = run_qi(
        "三家对话.qi",
        &endpoint,
        &[("QI_LLM_RECORD", "1"), ("QI_LLM_TAPE", &tape_path)],
        true,
    );
    let recorded = std::fs::read_to_string(&tape).expect("没录出磁带");
    let n_keys = serde_json::from_str::<serde_json::Value>(&recorded)
        .unwrap()
        .as_object()
        .unwrap()
        .len();
    assert!(n_keys >= 6, "录到的磁带条数不对：{}", n_keys);

    // 回放时把端点换成一个**连不上**的地址：万一键没命中而代码悄悄去联网，
    // 这里会直接失败，而不是偷偷真调一次 API 让测试蒙混过关。
    let (out, 错) = run_qi(
        "三家对话.qi",
        "http://127.0.0.1:1",
        &[("QI_LLM_REPLAY", "1"), ("QI_LLM_TAPE", &tape_path)],
        false,
    );
    assert!(
        !out.contains("未命中") && !错.contains("未命中"),
        "qi 实现拼出的请求体跟 Rust 版不同 —— 磁带键变了，所有历史录制都会静默失配。\n{}\n{}",
        out,
        错
    );
    // openai 分支的假响应是「候选0…」（多候选共用同一个 fixture）
    assert!(
        out.contains("claude答") && out.contains("gemini答") && out.contains("候选0"),
        "回放出来的内容不对：\n{}",
        out
    );
}

/// 反方向：qi 录、Rust 放。两个方向都过才说明键真的相同，
/// 而不是「qi 自己跟自己一致」。
#[test]
fn tape_recorded_by_qi_replays_in_ffi() {
    if !runtime_ready() {
        return;
    }
    let endpoint = start_fake_llm();
    let tmp = tempfile::tempdir().unwrap();
    let tape = tmp.path().join("tape.json");
    let tape_path = tape.to_string_lossy().to_string();

    run_qi(
        "三家对话.qi",
        &endpoint,
        &[("QI_LLM_RECORD", "1"), ("QI_LLM_TAPE", &tape_path)],
        false,
    );
    assert!(tape.exists(), "qi 实现没录出磁带");

    let (out, 错) = run_qi(
        "三家对话.qi",
        "http://127.0.0.1:1",
        &[("QI_LLM_REPLAY", "1"), ("QI_LLM_TAPE", &tape_path)],
        true,
    );
    assert!(
        !out.contains("未命中") && !错.contains("未命中"),
        "Rust 实现放不了 qi 录的磁带：\n{}\n{}",
        out,
        错
    );
}

/// 逃生口得真能切回去 —— 它坏了的话上面几条对照会退化成自己跟自己比。
/// 用「Rust 版嵌入错误信息带句柄号、两边措辞必须都对得上」以外的办法验：
/// 直接看两条路径下 QI_STDLIB_FFI 是否真的改变了分发 —— 拿一个只有 qi 版
/// 才认识的行为不好找，所以退一步：确认设了这个变量之后程序仍然跑得通，
/// 且磁带互放（上面两条）成立，即两条实现确实都被执行过。
#[test]
fn escape_hatch_runs() {
    if !runtime_ready() {
        return;
    }
    let endpoint = start_fake_llm();
    let (out, _) = run_qi("会话状态.qi", &endpoint, &[], true);
    assert!(out.contains("答1="), "走 FFI 时程序没跑通:\n{}", out);
}
