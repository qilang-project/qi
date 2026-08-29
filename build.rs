//! Build script for Qi compiler —— 只做一件事：跑 LALRPOP 生成语法分析器。
//!
//! 以前这里还编 src/runtime 的 syscalls.c、并把 libqi_gui.a 链进编译器自己。
//! 两件都随 src/runtime 一起废了：运行时符号由 qi-runtime 归档提供给**生成的程序**，
//! 编译器本身不需要任何运行时或 GUI 符号。

fn main() {
    println!("cargo:rerun-if-changed=src/parser/");
    emit_qi_stdlib_table();

    // Process LALRPOP grammar
    // Note: This may report shift/reduce conflicts which are benign
    // See grammar.lalrpop for documentation of expected conflicts
    match lalrpop::process_root() {
        Ok(_) => eprintln!("✓ LALRPOP grammar processed successfully"),
        Err(e) => {
            eprintln!("✗ LALRPOP processing failed!");
            eprintln!("Error details: {:#?}", e);
            eprintln!("\nNote: Shift/reduce conflicts are expected in this grammar.");
            eprintln!("See comments in grammar.lalrpop for details.");
            panic!("LALRPOP failed to generate parser");
        }
    }

    // Link macOS system frameworks required by reqwest
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=SystemConfiguration");
    }
}

/// 把 `标准库/*.qi` 嵌进二进制。
///
/// 生成一张 `&[(模块名, 源码)]` 表，用 include_str! 引源文件 —— 这样每个 .qi
/// 都进 rerun-if-changed，改了就重编，不会拿着上一次的源码继续跑。
///
/// 为什么不装到磁盘上再运行时读：见 src/标准库qi.rs 的模块文档。
fn emit_qi_stdlib_table() {
    use std::fmt::Write as _;

    let root = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("标准库");
    println!("cargo:rerun-if-changed={}", root.display());
    // 覆盖目录只在运行时读，但它一变就该重跑（否则 unset 之后还用着旧的内置表）
    println!("cargo:rerun-if-env-changed=QI_STDLIB_QI");

    let mut entries: Vec<(String, std::path::PathBuf)> = Vec::new();
    if root.is_dir() {
        let mut items: Vec<_> = std::fs::read_dir(&root)
            .unwrap_or_else(|e| panic!("读不了 {}: {e}", root.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("qi"))
            .collect();
        // 排序：生成文件的内容必须只由目录内容决定，不能随 read_dir 的返回序变，
        // 否则同一份源码两次构建产出不同的二进制。
        items.sort();
        for p in items {
            let name = p.file_stem().unwrap().to_str().unwrap().to_string();
            println!("cargo:rerun-if-changed={}", p.display());
            entries.push((name, p));
        }
    }

    let mut code = String::from(
        "// 由 build.rs 生成，勿手改。源在 qi/标准库/*.qi。\npub static BUILTIN_SOURCES: &[(&str, &str)] = &[\n",
    );
    for (name, p) in &entries {
        writeln!(
            code,
            "    ({:?}, include_str!({:?})),",
            name,
            p.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");

    let out_path =
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("qi_stdlib_builtin.rs");
    std::fs::write(&out_path, code)
        .unwrap_or_else(|e| panic!("写不了 {}: {e}", out_path.display()));
    eprintln!("✓ 标准库 qi 实现 {} 个模块已嵌入", entries.len());
}
