//! `qi doctor` —— 一体化工程诊断命令（聚合器，不新造分析器）。
//!
//! 把三项**已有**能力编排成一份联动诊断报告：
//!   1. 静态段：ARC 环检测收集版（`inkwell_gen::静态分析`）+ 工程概况计数 —— 不运行程序。
//!   2. CPU 段：QI_PROF=1 编译插桩 → 运行 → 捕获 profiler 报告 → top-N 热点。
//!   3. 内存段：QI_RC_REPORT=1 运行 → 捕获退出活跃对象计数 → 结合静态环给关联提示。
//!
//! doctor 本身只做「设开关 + 编译 + 跑 + 收集 + 合并呈现」，底层三者各自仍可独立使用。

use super::commands::{Cli, CliError};
use std::path::PathBuf;

/// 单条 CPU 热点行（从 profiler 报告解析）。
struct 热点行 {
    名: String,
    调用次数: String,
    总耗时ms: f64,
    占比: f64,
}

/// 「明显热点」的绝对耗时门槛（毫秒）：整体运行时间低于此值的程序，
/// 各函数占比只是微秒级噪声，不值得报热点。
const 热点门槛毫秒: f64 = 50.0;

/// 程序是否跑得够久、值得报 CPU 热点（以最大 total_ms —— 通常是 入口帧 —— 为整体耗时）。
fn 有明显热点(热点: &[热点行]) -> bool {
    热点
        .iter()
        .map(|h| h.总耗时ms)
        .fold(0.0f64, f64::max)
        >= 热点门槛毫秒
}

/// 内存活跃计数（从 `[qi-rc]` 行解析）。
#[derive(Clone, Copy)]
struct 内存计数 {
    对象: i64,
    字符串: i64,
    闭包: i64,
}

impl Cli {
    /// `qi doctor 程序.qi [--测CPU] [--查漏] [--超时 秒] [--json] [-- 参数...]`
    pub(super) async fn doctor_file(
        &self,
        file: PathBuf,
        args: Vec<String>,
        mut 测cpu: bool,
        mut 查漏: bool,
        超时: Option<u64>,
        json: bool,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        // 默认（两个动态开关都没给）：做全面体检（CPU + 查漏都开）。
        if !测cpu && !查漏 {
            测cpu = true;
            查漏 = true;
        }

        // ━━ 静态段：解析模块集 → 环检测收集版 + 工程概况 ━━
        let compiler = crate::QiCompiler::with_config(config.clone());
        let programs = compiler.collect_programs(&file)?;
        let 静态 = crate::codegen::inkwell_gen::静态分析(&programs)
            .map_err(|e| CliError::Compilation(crate::CompilerError::Codegen(e)))?;

        // ━━ 动态段：编译（按需插桩）+ 运行，捕获 profiler / RC 报告 ━━
        let mut 热点: Vec<热点行> = Vec::new();
        let mut 内存: Option<内存计数> = None;
        let mut 动态错误: Option<String> = None;

        if 测cpu || 查漏 {
            // 插桩门控 QI_PROF 在 后端::new() 读取 —— 编译前置好进程环境变量。
            // （与 test_files 设 QI_TEST_* 同一手法；doctor 单线程编排，安全。）
            let 旧prof = std::env::var("QI_PROF").ok();
            if 测cpu {
                std::env::set_var("QI_PROF", "1");
            } else {
                std::env::remove_var("QI_PROF");
            }
            // 静态段已单独报过环 —— 编译期 QI_LINT 警告在此静默，免得报告里重出一遍。
            let 旧lint = std::env::var("QI_LINT").ok();
            std::env::set_var("QI_LINT", "0");

            let dyn_compiler = crate::QiCompiler::with_config(config.clone());
            let 编译 = dyn_compiler.compile(file.clone());

            // 复原环境，避免污染同进程后续（doctor 通常即退出，但保持干净）。
            match 旧prof {
                Some(v) => std::env::set_var("QI_PROF", v),
                None => std::env::remove_var("QI_PROF"),
            }
            match 旧lint {
                Some(v) => std::env::set_var("QI_LINT", v),
                None => std::env::remove_var("QI_LINT"),
            }

            match 编译 {
                Ok(编译结果) => {
                    // 子进程环境：CPU → QI_PROF=1；查漏 → QI_RC_REPORT=1。
                    let mut 子环境: Vec<(&str, &str)> = Vec::new();
                    if 测cpu {
                        子环境.push(("QI_PROF", "1"));
                    }
                    if 查漏 {
                        子环境.push(("QI_RC_REPORT", "1"));
                    }

                    let 跑 = 运行并捕获(&编译结果.executable_path, &args, &子环境, 超时);

                    // 清理编译中间产物 + 可执行文件
                    for obj in &编译结果.object_paths {
                        let _ = std::fs::remove_file(obj);
                    }
                    for ir in &编译结果.ir_paths {
                        let _ = std::fs::remove_file(ir);
                    }
                    let _ = std::fs::remove_file(&编译结果.executable_path);

                    match 跑 {
                        Ok(诊断输出) => {
                            if 测cpu {
                                热点 = 解析剖析报告(&诊断输出);
                            }
                            if 查漏 {
                                内存 = 解析rc报告(&诊断输出);
                            }
                        }
                        Err(e) => 动态错误 = Some(e),
                    }
                }
                Err(e) => 动态错误 = Some(format!("编译失败: {}", e)),
            }
        }

        // ━━ 呈现 ━━
        if json {
            打印json(&file, &静态, &热点, 内存, 测cpu, 查漏, &动态错误);
        } else {
            打印文本(
                &file, &静态, &热点, 内存, 测cpu, 查漏, 超时, &动态错误,
            );
        }
        Ok(())
    }
}

/// 编译产物运行并捕获 stdout+stderr（合并成一段文本供解析）。
/// `超时`：Some(秒) 时到点发 SIGTERM（Unix）—— profiler 看门狗据此打报告并退出；
/// None 时阻塞到程序自然退出（适合会退出的普通程序）。
fn 运行并捕获(
    exe: &std::path::Path,
    args: &[String],
    env: &[(&str, &str)],
    超时: Option<u64>,
) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e| format!("无法启动程序: {}", e))?;

    // 先把管道取出，交给读线程 —— 防止长驻程序写满管道阻塞。
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_handle = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(ref mut p) = out_pipe {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let err_handle = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(ref mut p) = err_pipe {
            let _ = p.read_to_string(&mut s);
        }
        s
    });

    if let Some(秒) = 超时 {
        let 截止 = std::time::Instant::now() + std::time::Duration::from_secs(秒);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if std::time::Instant::now() >= 截止 {
                        // 发 SIGTERM：profiler 看门狗收到后打报告 + exit(130)。
                        终止子进程(&mut child);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => return Err(format!("等待程序失败: {}", e)),
            }
        }
    }

    let _ = child.wait();
    let mut 合并 = out_handle.join().unwrap_or_default();
    合并.push('\n');
    合并.push_str(&err_handle.join().unwrap_or_default());
    Ok(合并)
}

#[cfg(unix)]
fn 终止子进程(child: &mut std::process::Child) {
    // SIGTERM 让 profiler 看门狗优雅收尾（打报告）；给它一点时间后再兜底 kill。
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    let _ = child.kill();
}

#[cfg(not(unix))]
fn 终止子进程(child: &mut std::process::Child) {
    let _ = child.kill();
}

/// 从合并输出里抠 profiler 报告表，解析成热点行（已按总耗时降序，profiler 侧已排好）。
fn 解析剖析报告(输出: &str) -> Vec<热点行> {
    let mut 行: Vec<热点行> = Vec::new();
    let mut 在表内 = false;
    for line in 输出.lines() {
        if line.contains("=== Qi Profiler") {
            在表内 = true;
            continue;
        }
        if !在表内 {
            continue;
        }
        if line.starts_with("=== (") {
            break;
        }
        // 跳过表头行（含「占比%」列名）
        if line.contains("占比%") {
            continue;
        }
        // 数据行：末尾 4 列 = 调用次数 总耗时ms 每次µs 占比%；其余为函数名。
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.len() < 5 {
            continue;
        }
        let n = toks.len();
        let 占比字 = toks[n - 1].trim_end_matches('%');
        let 占比: f64 = 占比字.parse().unwrap_or(0.0);
        let 总耗时ms: f64 = toks[n - 3].parse().unwrap_or(0.0);
        let 调用次数 = toks[n - 4].to_string();
        let 名 = toks[..n - 4].join(" ");
        行.push(热点行 {
            名,
            调用次数,
            总耗时ms,
            占比,
        });
    }
    行
}

/// 从合并输出里找 `[qi-rc] 活跃对象=.. 活跃字符串=.. 活跃闭包=..`。
fn 解析rc报告(输出: &str) -> Option<内存计数> {
    for line in 输出.lines() {
        if !line.contains("[qi-rc]") {
            continue;
        }
        let 取 = |键: &str| -> i64 {
            line.split_whitespace()
                .find_map(|t| t.strip_prefix(键))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        };
        return Some(内存计数 {
            对象: 取("活跃对象="),
            字符串: 取("活跃字符串="),
            闭包: 取("活跃闭包="),
        });
    }
    None
}

// ─────────────────────────── 文本报告 ───────────────────────────

const 分隔: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

#[allow(clippy::too_many_arguments)]
fn 打印文本(
    file: &std::path::Path,
    静态: &crate::codegen::inkwell_gen::静态诊断,
    热点: &[热点行],
    内存: Option<内存计数>,
    测cpu: bool,
    查漏: bool,
    超时: Option<u64>,
    动态错误: &Option<String>,
) {
    println!("{}", 分隔);
    println!("  Qi Doctor · 工程诊断报告");
    println!("  文件: {}", file.display());
    println!("{}", 分隔);

    // ── 静态拓扑 ──
    println!("\n━━ 静态拓扑 ━━");
    println!(
        "  工程概况: 结构体 {} · 枚举 {} · 函数 {} · 外部函数 {} · 导出函数 {}",
        静态.结构体数, 静态.枚举数, 静态.函数数, 静态.外部函数数, 静态.导出函数数
    );
    if 静态.环列表.is_empty() {
        println!("  内存拓扑: ✓ 无循环引用");
    } else {
        println!(
            "  内存拓扑: ⚠ 发现 {} 个潜在引用环（纯 ARC 无环收集器 → 永久泄漏风险）",
            静态.环列表.len()
        );
        for (i, 环) in 静态.环列表.iter().enumerate() {
            println!("    {}. {}", i + 1, 环);
        }
        println!("    提示: 把环上一条边改用整数 id 间接引用（暂无 weak 语法）以打破环。");
    }

    // ── CPU 热点 ──
    if 测cpu {
        println!("\n━━ CPU 热点 ━━");
        if let Some(e) = 动态错误 {
            println!("  ✗ 未能采集: {}", e);
        } else if 热点.is_empty() {
            println!("  ✓ 无明显热点（程序过快或未产生可归因的函数调用）");
        } else if !有明显热点(热点) {
            let 整体 = 热点.iter().map(|h| h.总耗时ms).fold(0.0f64, f64::max);
            println!(
                "  ✓ 无明显热点（整体耗时 {:.1}ms < {:.0}ms 门槛，占比皆为微秒级噪声）",
                整体, 热点门槛毫秒
            );
        } else {
            println!(
                "  {:<24} {:>10} {:>12} {:>8}",
                "函数", "调用次数", "总耗时(ms)", "占比%"
            );
            for h in 热点.iter().take(8) {
                println!(
                    "  {:<24} {:>10} {:>12.3} {:>7.1}%",
                    h.名, h.调用次数, h.总耗时ms, h.占比
                );
            }
            println!("  (wall-inclusive：占比以最大值为 100% 基准，父含子)");
        }
    }

    // ── 内存 ──
    if 查漏 {
        println!("\n━━ 内存 ━━");
        match 内存 {
            None => {
                if let Some(e) = 动态错误 {
                    println!("  ✗ 未能采集: {}", e);
                } else {
                    println!("  ? 未捕获到 RC 报告（程序可能未退出或无 RC 分配）");
                }
            }
            Some(m) => {
                let 泄漏 = m.对象 > 0 || m.字符串 > 0 || m.闭包 > 0;
                if 泄漏 {
                    println!(
                        "  ⚠ 退出时活跃: 对象 {} · 字符串 {} · 闭包 {}（>0 提示可能泄漏）",
                        m.对象, m.字符串, m.闭包
                    );
                    if !静态.环列表.is_empty() && m.对象 > 0 {
                        println!(
                            "    联动: 活跃对象未归零，很可能来自上面的 {} 个引用环 —— 优先排查环上类型。",
                            静态.环列表.len()
                        );
                    }
                } else {
                    println!("  ✓ 退出时活跃对象 0 · 字符串 0 · 闭包 0（无明显泄漏）");
                }
            }
        }
    }

    // ── 诊断结论 ──
    println!("\n━━ 诊断结论 ━━");
    println!("  {}", 生成结论(静态, 热点, 内存, 测cpu, 查漏));
    if let Some(秒) = 超时 {
        println!("  (动态段用了 {} 秒超时采样)", 秒);
    }
    println!("{}", 分隔);
}

/// 结论用的「顶级热点」：跳过 入口/main（wall-inclusive 恒 100% 的包装帧），
/// 取占比最高的真实用户函数；若只有入口帧则退回它本身。
fn 顶级热点(热点: &[热点行]) -> Option<&热点行> {
    热点
        .iter()
        .find(|h| h.名 != "入口" && h.名 != "main")
        .or_else(|| 热点.first())
}

/// 一句话总评。
fn 生成结论(
    静态: &crate::codegen::inkwell_gen::静态诊断,
    热点: &[热点行],
    内存: Option<内存计数>,
    测cpu: bool,
    查漏: bool,
) -> String {
    let mut 片段: Vec<String> = Vec::new();

    if 静态.环列表.is_empty() {
        片段.push("内存拓扑健康（无引用环）".to_string());
    } else {
        片段.push(format!("发现 {} 个引用环需打破", 静态.环列表.len()));
    }

    if 测cpu {
        // 结论里的「热点」跳过 入口/main 包装 —— 它 wall-inclusive 恒 100%（含所有子调用），
        // 真正有意义的热点是它下面占比最高的那个用户函数。程序太快则无热点可言。
        if !热点.is_empty() && !有明显热点(热点) {
            片段.push("无明显热点".to_string());
        } else if let Some(top) = 顶级热点(热点) {
            片段.push(format!("热点集中在 {}（{:.1}%）", top.名, top.占比));
        }
    }

    if 查漏 {
        if let Some(m) = 内存 {
            if m.对象 > 0 || m.字符串 > 0 || m.闭包 > 0 {
                片段.push(format!("退出仍有 {} 个活跃对象", m.对象));
            } else {
                片段.push("退出无泄漏".to_string());
            }
        }
    }

    let 有问题 = !静态.环列表.is_empty()
        || 内存
            .map(|m| m.对象 > 0 || m.字符串 > 0 || m.闭包 > 0)
            .unwrap_or(false);
    let 前缀 = if 有问题 { "需要关注：" } else { "✓ 整体健康：" };

    if 片段.is_empty() {
        format!("{}未采集到诊断信号。", 前缀)
    } else {
        let mut 建议 = String::new();
        if !静态.环列表.is_empty() {
            建议 = "；建议优先用 id 间接引用打破环".to_string();
        } else if 测cpu && 有明显热点(热点) {
            if let Some(top) = 顶级热点(热点) {
                if top.占比 > 60.0 {
                    建议 = format!("；建议优化 {} 降低热点占比", top.名);
                }
            }
        }
        format!("{}{}{}。", 前缀, 片段.join("，"), 建议)
    }
}

// ─────────────────────────── JSON 报告 ───────────────────────────

#[allow(clippy::too_many_arguments)]
fn 打印json(
    file: &std::path::Path,
    静态: &crate::codegen::inkwell_gen::静态诊断,
    热点: &[热点行],
    内存: Option<内存计数>,
    测cpu: bool,
    查漏: bool,
    动态错误: &Option<String>,
) {
    // 手写 JSON（避免引入 serde 到本命令；字段值均为已知形状，做最小转义）。
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("  \"file\": \"{}\",\n", esc(&file.display().to_string())));

    // 静态
    out.push_str("  \"static\": {\n");
    out.push_str(&format!("    \"structs\": {},\n", 静态.结构体数));
    out.push_str(&format!("    \"enums\": {},\n", 静态.枚举数));
    out.push_str(&format!("    \"functions\": {},\n", 静态.函数数));
    out.push_str(&format!("    \"externs\": {},\n", 静态.外部函数数));
    out.push_str(&format!("    \"exports\": {},\n", 静态.导出函数数));
    let 环 = 静态
        .环列表
        .iter()
        .map(|c| format!("\"{}\"", esc(c)))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!("    \"cycles\": [{}]\n", 环));
    out.push_str("  },\n");

    // CPU
    if 测cpu {
        let rows = 热点
            .iter()
            .take(8)
            .map(|h| {
                format!(
                    "{{\"name\": \"{}\", \"calls\": \"{}\", \"total_ms\": {:.3}, \"pct\": {:.1}}}",
                    esc(&h.名), esc(&h.调用次数), h.总耗时ms, h.占比
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("  \"cpu\": [{}],\n", rows));
    }

    // 内存
    if 查漏 {
        match 内存 {
            Some(m) => out.push_str(&format!(
                "  \"memory\": {{\"objects\": {}, \"strings\": {}, \"closures\": {}}},\n",
                m.对象, m.字符串, m.闭包
            )),
            None => out.push_str("  \"memory\": null,\n"),
        }
    }

    match 动态错误 {
        Some(e) => out.push_str(&format!("  \"dynamic_error\": \"{}\",\n", esc(e))),
        None => out.push_str("  \"dynamic_error\": null,\n"),
    }

    out.push_str(&format!(
        "  \"summary\": \"{}\"\n",
        esc(&生成结论(静态, 热点, 内存, 测cpu, 查漏))
    ));
    out.push('}');
    println!("{}", out);
}
