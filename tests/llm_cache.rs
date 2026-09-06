//! 提示缓存（prompt cache）的两道防线。
//!
//! ## 一、默认路径的请求体一个字节都不许变
//!
//! 磁带键 = 请求体序列化文本的哈希，serde_json 开了 preserve_order，
//! **键序即插入序**。所以「加一个可选特性」这件事在这里的风险不是特性本身
//! 写错，而是它无条件地往请求体里多塞了点什么 —— 那会让所有历史录制
//! 静默回放未命中，而失败长得像「模型今天答得不一样」。
//!
//! 这里的证据是：同一份语料，qi 实现和 Rust 实现各跑一遍，把假服务器收到的
//! **请求体原文逐字节比**。Rust 那条路这次没动过，两边相同即证明 qi 侧默认
//! 路径的请求体没变。（tests/llm_qi.rs 的磁带互放是另一半证据。）
//!
//! ## 二、开关打开之后 cache_control 打在对的位置
//!
//! Anthropic 的缓存前缀顺序是 tools → system → messages，一个断点缓存
//! 它自己以及它前面的一切。所以：有 system 时静态断点落在 system 上
//! （一并盖住 tools），没有 system 时退到最后一个 tool；增量断点永远落在
//! messages 最后一条的最后一块 —— 这一条才是 agent loop 的收益来源。
//!
//! ## 三、响应侧四家的缓存字段名各不相同
//!
//! anthropic: usage.cache_read_input_tokens / cache_creation_input_tokens
//!            （注意 input_tokens **不含**这两者）
//! openai:    usage.prompt_tokens_details.cached_tokens（已含在 prompt_tokens 里）
//! deepseek:  usage.prompt_cache_hit_tokens（顶层，也已含在 prompt_tokens 里）
//! gemini:    usageMetadata.cachedContentTokenCount

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

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

type Bodies = Arc<Mutex<Vec<String>>>;

/// 假 LLM 服务：按路径分辨三家，**响应里带缓存字段**，同时把收到的请求体原文
/// 按到达顺序记下来供断言。
fn start_fake_llm() -> (String, Bodies) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let bodies: Bodies = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&bodies);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            let sink = Arc::clone(&sink);
            std::thread::spawn(move || {
                let mut buf = vec![0u8; 65536];
                let n = match conn.read(&mut buf) {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let first_line = req.lines().next().unwrap_or("").to_string();
                let body_in = req
                    .split_once("\r\n\r\n")
                    .map(|(_, b)| b.to_string())
                    .unwrap_or_default();
                sink.lock().unwrap().push(body_in.clone());

                let has_tool_result = body_in.contains("\"role\":\"tool\"")
                    || body_in.contains("tool_result")
                    || body_in.contains("functionResponse");
                let wants_tools = body_in.contains("\"tools\"");

                let body = if first_line.contains("generateContent") {
                    r#"{"candidates":[{"content":{"parts":[{"text":"gemini答"}]}}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8,"cachedContentTokenCount":4}}"#.to_string()
                } else if first_line.contains("/messages") {
                    if wants_tools && !has_tool_result {
                        r#"{"content":[{"type":"tool_use","id":"toolu_1","name":"qi_tool_e69fa5e5a4a9e6b094","input":{"城市":"北京"}}],"usage":{"input_tokens":6,"output_tokens":4,"cache_read_input_tokens":120,"cache_creation_input_tokens":30}}"#.to_string()
                    } else {
                        r#"{"content":[{"type":"text","text":"claude答"}],"usage":{"input_tokens":6,"output_tokens":4,"cache_read_input_tokens":120,"cache_creation_input_tokens":30}}"#.to_string()
                    }
                } else if body_in.contains("deepseek") {
                    // DeepSeek 走同一条 OpenAI 兼容分支，但缓存数在**顶层** usage 上，
                    // 而且 prompt_tokens 是**含**命中部分的总数（跟 Anthropic 相反）
                    r#"{"choices":[{"message":{"role":"assistant","content":"deepseek答"}}],"usage":{"prompt_tokens":100,"completion_tokens":7,"total_tokens":107,"prompt_cache_hit_tokens":64,"prompt_cache_miss_tokens":36}}"#.to_string()
                } else if wants_tools && !has_tool_result {
                    r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"qi_tool_e69fa5e5a4a9e6b094","arguments":"{\"城市\":\"北京\"}"}}]}}],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18,"prompt_tokens_details":{"cached_tokens":8}}}"#.to_string()
                } else {
                    r#"{"choices":[{"message":{"role":"assistant","content":"openai答"}}],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18,"prompt_tokens_details":{"cached_tokens":8}}}"#.to_string()
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
    (format!("http://127.0.0.1:{}", port), bodies)
}

fn run_qi(fixture: &str, endpoint: &str, use_ffi: bool) -> (String, String) {
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

/// 开关关着 → qi 侧发出的请求体跟 Rust 侧**逐字节相同**。
/// 这条红了就意味着磁带全部失效，别的都不用看了。
#[test]
fn request_body_is_byte_identical_to_rust_when_cache_is_off() {
    if !runtime_ready() {
        eprintln!("跳过：未找到 qi-runtime 归档");
        return;
    }
    let (endpoint, qi_bodies) = start_fake_llm();
    let (qi_out, _) = run_qi("缓存请求体.qi", &endpoint, false);
    let qi_bodies = qi_bodies.lock().unwrap().clone();

    let (endpoint2, ffi_bodies) = start_fake_llm();
    let (ffi_out, _) = run_qi("缓存请求体.qi", &endpoint2, true);
    let ffi_bodies = ffi_bodies.lock().unwrap().clone();

    assert_eq!(qi_out, ffi_out, "输出就已经不一致了");
    assert!(!qi_bodies.is_empty(), "假服务器一个请求都没收到");
    assert_eq!(
        qi_bodies.len(),
        ffi_bodies.len(),
        "请求条数不同：qi {:?}\nrust {:?}",
        qi_bodies,
        ffi_bodies
    );
    for (i, (a, b)) in qi_bodies.iter().zip(ffi_bodies.iter()).enumerate() {
        assert_eq!(
            a, b,
            "第 {} 个请求体不一致 —— 磁带键会跟着变，所有历史录制静默失配\nqi  : {}\nrust: {}",
            i, a, b
        );
    }
    // 顺带钉死「默认不带缓存标记」这件事本身
    for b in &qi_bodies {
        assert!(
            !b.contains("cache_control"),
            "开关关着却出现了 cache_control：{}",
            b
        );
    }
}

/// 开关打开 → cache_control 出现在该出现的位置。
#[test]
fn cache_breakpoints_land_in_the_right_places_when_enabled() {
    if !runtime_ready() {
        return;
    }
    let (endpoint, bodies) = start_fake_llm();
    let (out, err) = run_qi("缓存开关.qi", &endpoint, false);
    assert!(out.contains("答2="), "程序没跑通:\n{}\n{}", out, err);
    let bodies = bodies.lock().unwrap().clone();

    // 语料顺序：anthropic 带 system 两轮、anthropic 无 system 带工具两轮、openai 一轮
    let anthropic_with_system: Vec<&String> = bodies
        .iter()
        .filter(|b| b.contains("你是一个严谨的助手"))
        .collect();
    assert_eq!(anthropic_with_system.len(), 2, "{:?}", bodies);

    for b in &anthropic_with_system {
        let v: serde_json::Value = serde_json::from_str(b).unwrap();
        // 静态断点：system 升成 block 数组，最后一块带 cache_control
        let sys =
            v.get("system").expect("没有 system").as_array().expect(
                "开了提示缓存之后 system 必须是 block 数组 —— 字符串形态挂不了 cache_control",
            );
        assert_eq!(
            sys.last().unwrap()["cache_control"]["type"],
            "ephemeral",
            "system 上没有静态断点：{}",
            b
        );
        // 增量断点：messages 最后一条的最后一块
        let msgs = v["messages"].as_array().unwrap();
        let last = msgs.last().unwrap();
        let blocks = last["content"].as_array().unwrap_or_else(|| {
            panic!("最后一条消息的 content 必须是块数组才挂得住 cache_control：{b}")
        });
        assert_eq!(
            blocks.last().unwrap()["cache_control"]["type"],
            "ephemeral",
            "历史末条上没有增量断点：{}",
            b
        );
        // 中间的消息不该被标 —— 断点只有 4 个额度，标多了直接 400
        for m in &msgs[..msgs.len() - 1] {
            assert!(
                !m.to_string().contains("cache_control"),
                "非末条消息被标了：{}",
                b
            );
        }
    }

    // 无 system 带工具那两轮：静态断点退到最后一个 tool 上
    let anthropic_tools: Vec<&String> = bodies
        .iter()
        .filter(|b| b.contains("input_schema") && !b.contains("你是一个严谨的助手"))
        .collect();
    assert!(!anthropic_tools.is_empty(), "{:?}", bodies);
    for b in &anthropic_tools {
        let v: serde_json::Value = serde_json::from_str(b).unwrap();
        assert!(v.get("system").is_none(), "这一轮本来就不该有 system");
        let tools = v["tools"].as_array().unwrap();
        assert_eq!(
            tools.last().unwrap()["cache_control"]["type"],
            "ephemeral",
            "最后一个 tool 上没有断点：{}",
            b
        );
    }

    // 开关只管 anthropic：openai 的请求体不许多出任何东西
    for b in bodies.iter().filter(|b| b.contains("\"gpt-test\"")) {
        assert!(!b.contains("cache_control"), "openai 请求体被污染了：{}", b);
    }
}

/// 四家的缓存字段解析。数字是假服务器给的固定值，不是真实命中率。
#[test]
fn all_four_providers_cache_fields_are_parsed() {
    if !runtime_ready() {
        return;
    }
    let (endpoint, _) = start_fake_llm();
    let (out, err) = run_qi("缓存字段.qi", &endpoint, false);
    assert!(out.contains("deepseek 缓存="), "没跑通:\n{}\n{}", out, err);

    for expected in [
        // anthropic：input_tokens 不含缓存，所以命中率分母是 p+cr+cw=156
        r#"anthropic 用量={"prompt":6,"completion":4,"total":10}"#,
        r#"anthropic 缓存={"read":120,"write":30,"read_total":120,"write_total":30}"#,
        "anthropic 命中率=76",
        // gemini：cachedContentTokenCount，没有 cache creation
        r#"gemini 缓存={"read":4,"write":0,"read_total":4,"write_total":0}"#,
        "gemini 命中率=80",
        // openai：prompt_tokens_details.cached_tokens（嵌套对象）
        r#"openai 缓存={"read":8,"write":0,"read_total":8,"write_total":0}"#,
        "openai 命中率=72",
        // deepseek：顶层 prompt_cache_hit_tokens，prompt_tokens 已含命中部分
        r#"deepseek 缓存={"read":64,"write":0,"read_total":64,"write_total":0}"#,
        "deepseek 命中率=64",
        // 预算口径没被偷偷改：累计用量仍然只是各家 usage 的 total
        "anthropic 已用预算=10",
        "deepseek 已用预算=107",
    ] {
        assert!(out.contains(expected), "缺少 `{}`：\n{}", expected, out);
    }
}
