//! Build script for Qi compiler - LALRPOP grammar processing and C runtime compilation

fn main() {
    println!("cargo:rerun-if-changed=src/parser/");
    println!("cargo:rerun-if-changed=src/runtime/async_runtime/c_runtime/");

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

    // Compile C syscall library for async runtime
    println!("cargo:rerun-if-changed=src/runtime/async_runtime/c_runtime/syscalls.c");

    cc::Build::new()
        .file("src/runtime/async_runtime/c_runtime/syscalls.c")
        .warnings(true)
        .extra_warnings(true)
        .opt_level(2)
        .compile("qi_async_syscalls");

    eprintln!("✓ C async syscalls library compiled successfully");
    eprintln!("✓ Concurrency functions implemented in Rust");
}
