//! Build script for Qi compiler - LALRPOP grammar processing and C runtime compilation

fn main() {
    // Tell Cargo that has_gui is a valid cfg flag
    println!("cargo:rustc-check-cfg=cfg(has_gui)");

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

    // Link macOS system frameworks required by reqwest
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=SystemConfiguration");
    }

    eprintln!("✓ C async syscalls library compiled successfully");
    eprintln!("✓ Concurrency functions implemented in Rust");

    // Link GUI library (qi-gui) from workspace target directory - Optional
    // Check if qi-gui library exists before linking
    let workspace_target = std::env::var("CARGO_WORKSPACE_DIR")
        .map(|dir| format!("{}/target", dir))
        .unwrap_or_else(|_| {
            // Fallback: try to find workspace root
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            format!("{}/../target", manifest_dir)
        });

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let gui_lib_dir = format!("{}/{}", workspace_target, profile);

    // Check for GUI library file
    #[cfg(target_os = "macos")]
    let gui_lib_path = format!("{}/libqi_gui.a", gui_lib_dir);
    #[cfg(target_os = "linux")]
    let gui_lib_path = format!("{}/libqi_gui.a", gui_lib_dir);
    #[cfg(target_os = "windows")]
    let gui_lib_path = format!("{}\\qi_gui.lib", gui_lib_dir);

    if std::path::Path::new(&gui_lib_path).exists() {
        println!("cargo:rerun-if-changed=../qi-gui/src/");
        println!("cargo:rustc-link-search=native={}", gui_lib_dir);
        println!("cargo:rustc-link-lib=static=qi_gui");

        // Set a cfg flag to indicate GUI is available
        println!("cargo:rustc-cfg=has_gui");

        // Link system frameworks required by GUI library
        #[cfg(target_os = "macos")]
        {
            println!("cargo:rustc-link-lib=framework=Cocoa");
            println!("cargo:rustc-link-lib=framework=QuartzCore");
            println!("cargo:rustc-link-lib=framework=Carbon");
            println!("cargo:rustc-link-lib=framework=CoreGraphics");
            println!("cargo:rustc-link-lib=framework=CoreVideo");
            println!("cargo:rustc-link-lib=framework=AppKit");
            // Audio frameworks required by rodio
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=CoreAudio");
        }

        #[cfg(target_os = "linux")]
        {
            println!("cargo:rustc-link-lib=gtk-3");
            println!("cargo:rustc-link-lib=gdk-3");
        }

        eprintln!("✓ GUI library found and configured");
    } else {
        eprintln!("⚠ GUI library not found (图形化功能将不可用)");
        eprintln!("  Expected at: {}", gui_lib_path);
        eprintln!("  To build with GUI: cargo build --workspace --release");
    }
}
