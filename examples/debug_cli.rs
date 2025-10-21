use qi_compiler::QiCompiler;
use std::path::PathBuf;

fn main() {
    let source_file = PathBuf::from("test_cli_works.qi");
    let compiler = QiCompiler::new();

    println!("Testing compilation of: {:?}", source_file);

    match compiler.compile(source_file) {
        Ok(result) => {
            println!("Compilation succeeded!");
            println!("Generated file: {:?}", result.executable_path);
            println!("Duration: {}ms", result.duration_ms);

            // Read and display the generated IR
            if let Ok(ir_content) = std::fs::read_to_string(&result.executable_path) {
                println!("Generated IR:\n{}", ir_content);
            }
        }
        Err(e) => {
            println!("Compilation failed: {}", e);
        }
    }
}