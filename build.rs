//! Build script for Qi compiler —— 只做一件事：跑 LALRPOP 生成语法分析器。
//!
//! 以前这里还编 src/runtime 的 syscalls.c、并把 libqi_gui.a 链进编译器自己。
//! 两件都随 src/runtime 一起废了：运行时符号由 qi-runtime 归档提供给**生成的程序**，
//! 编译器本身不需要任何运行时或 GUI 符号。

fn main() {
    println!("cargo:rerun-if-changed=src/parser/");

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
