//! qi 写的标准库模块 vs 原 Rust FFI 实现：同一份源码跑两遍，输出必须逐字节一致。
//!
//! 为什么要这样测，而不是逐个函数写断言：
//!
//! 标准库是一个一个模块搬过去的，每搬一个，都有一份现成的、在生产里跑了很久的
//! Rust 实现摆在旁边当参考答案。放着不用而去手写期望值，等于把「qi 版对不对」
//! 换成「我对日历规则记得对不对」—— 后者才是真正容易错的地方（ISO 周数、
//! 1970 前的负时间戳向下取整、400 年闰年例外，每一条都能悄悄写反）。
//!
//! 分发是**按函数**判定的（见 codegen 的 尝试标准库调用），所以语料里只放
//! qi 那边真正实现了的函数；没实现的照常走 FFI，两遍跑的是同一份代码，
//! 比了也没意义。
//!
//! 故意不一致的函数不进语料，单独在 时间_加月加年_改的是错行为 里钉住。

use std::path::{Path, PathBuf};
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

/// 每次调用给源码一份独占的临时拷贝。
///
/// 不这么做会踩并发：`qi run` 的产物路径是按源文件名定的，两个测试用同一份
/// 语料并行跑，就会同时往同一个可执行文件上写 —— 一个正在链接，另一个已经
/// 开始执行那个写了一半的文件。表现是随机一条测试报「跑 X 失败」而 stderr
/// 是空的，单独重跑又必过。cargo test 默认多线程，所以这条只在跑全量时炸。
fn 独占拷贝(源: &Path, 标记: &str) -> (tempfile::TempDir, PathBuf) {
    let 临时 = tempfile::tempdir().expect("建不了临时目录");
    let 目标 = 临时.path().join(format!(
        "{}_{}.qi",
        源.file_stem().unwrap().to_string_lossy(),
        标记
    ));
    std::fs::copy(源, &目标).expect("拷不过去");
    (临时, 目标)
}

/// 跑一份源码，返回 stdout。走 FFI 时置 QI_STDLIB_FFI。
fn 跑(源: &Path, 强制ffi: Option<&str>) -> String {
    let (_临时, 源) = 独占拷贝(源, if 强制ffi.is_some() { "ffi" } else { "qi" });
    let 源 = &源;
    let mut cmd = Command::new(编译器());
    cmd.arg("run").arg(源);
    match 强制ffi {
        Some(模块) => {
            cmd.env("QI_STDLIB_FFI", 模块);
        }
        // 显式清掉：外面设了这个变量会让「qi 版」那一趟其实也跑 FFI，
        // 两边一致于是测试绿 —— 一个什么都没验证的绿。
        None => {
            cmd.env_remove("QI_STDLIB_FFI");
        }
    }
    let 出 = cmd.output().expect("起不来 qi");
    assert!(
        出.status.success(),
        "跑 {} 失败（QI_STDLIB_FFI={:?}）:\n{}\n{}",
        源.display(),
        强制ffi,
        String::from_utf8_lossy(&出.stdout),
        String::from_utf8_lossy(&出.stderr),
    );
    String::from_utf8_lossy(&出.stdout).into_owned()
}

fn 对照(模块: &str) {
    if !运行时就位() {
        eprintln!("跳过：未找到 qi-runtime 归档（先在 qi-runtime/ 跑 cargo build --release）");
        return;
    }
    let 源 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/标准库qi语料")
        .join(format!("{}.qi", 模块));
    let qi版 = 跑(&源, None);
    let rust版 = 跑(&源, Some(模块));

    assert!(!qi版.trim().is_empty(), "{} 语料没有任何输出", 模块);
    if qi版 != rust版 {
        let 差: Vec<String> = qi版
            .lines()
            .zip(rust版.lines())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(20)
            .map(|(i, (a, b))| format!("  第{}行  qi: {}   rust: {}", i + 1, a, b))
            .collect();
        panic!(
            "标准库.{} 的 qi 实现与 Rust FFI 输出不一致（前 20 处）：\n{}",
            模块,
            差.join("\n")
        );
    }
}

#[test]
fn 时间_qi实现与ffi逐字节一致() {
    对照("时间");
}

/// 加月 / 加年 是**故意**跟 Rust 版不一样的：那边是 `+ 月数*30天` 和
/// `+ 年数*365天`，压根不是日历运算。这个测试钉住新行为，免得哪天有人
/// 「修」回去对齐 FFI。
#[test]
fn 时间_加月加年_改的是错行为() {
    if !运行时就位() {
        eprintln!("跳过：未找到 qi-runtime 归档");
        return;
    }
    let 源 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/标准库qi语料/时间_加月加年.qi");
    let 出 = 跑(&源, None);
    for 期望 in [
        "1月31日 +1月 -> 2026-2-28", // 截到月末，不是溢出到 3 月 2 日
        "3月31日 +1月 -> 2026-4-30",
        "12月15日 +1月 -> 2027-1-15", // 跨年不掉一天
        "1月15日 -1月 -> 2025-12-15",
        "1月15日 -13月 -> 2024-12-15",
        "2月29日 +1年 -> 2025-2-28", // 闰日落到平年的 2 月 28
        "8月26日 -3年 -> 2023-8-26", // 跨闰年往回不掉一天
    ] {
        assert!(出.contains(期望), "缺少 `{}`，实际输出：\n{}", 期望, 出);
    }
}

/// QI_STDLIB_FFI 是逃生口，必须真的能把整个模块切回 FFI。
/// 它坏了的话上面那条对照测试会变成「自己跟自己比」，永远绿。
#[test]
fn 逃生口真的切回ffi() {
    if !运行时就位() {
        eprintln!("跳过：未找到 qi-runtime 归档");
        return;
    }
    let 源 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/标准库qi语料/时间_加月加年.qi");
    assert_ne!(
        跑(&源, None),
        跑(&源, Some("时间")),
        "QI_STDLIB_FFI=时间 没有切回 FFI —— 两条实现在 加月 上行为不同，输出不该一样"
    );
}
