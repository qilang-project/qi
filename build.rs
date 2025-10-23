//! Build script for Qi compiler - LALRPOP grammar processing only
//! 
//! NOTE: The C runtime library (previously in /runtime) is no longer used.
//! All runtime functions are now provided by src/runtime/executor.rs via FFI.

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
}
