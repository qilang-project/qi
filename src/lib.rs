 //! Qi Language Compiler
//!
//! A compiler for the Qi programming language with 100% Chinese keywords.
//! Compiles Qi source code to executable binaries for multiple platforms.

#![allow(missing_docs)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![warn(clippy::all)]

pub mod cli;
pub mod codegen;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod semantic;
pub mod targets;
pub mod utils;

// Force export of async runtime FFI functions to ensure they're included in the static library
pub use runtime::async_runtime::ffi::{qi_runtime_create_task, qi_runtime_await, qi_runtime_spawn_task};

// Dummy function to ensure async runtime functions are not optimized out
#[doc(hidden)]
#[no_mangle]
pub extern "C" fn _qi_force_link_async_runtime() {
    // These functions need to be referenced to prevent optimization
    unsafe {
        std::ptr::read_volatile(&qi_runtime_create_task as *const _);
        std::ptr::read_volatile(&qi_runtime_await as *const _);
        std::ptr::read_volatile(&qi_runtime_spawn_task as *const _);
    }
}

use std::path::PathBuf;

/// Compiler configuration and settings
pub mod config;

/// Main compiler interface
#[allow(dead_code)]
pub struct QiCompiler {
    config: config::CompilerConfig,
}

impl QiCompiler {
    /// Create a new compiler instance with default configuration
    pub fn new() -> Self {
        Self {
            config: config::CompilerConfig::default(),
        }
    }

    /// Create a new compiler instance with custom configuration
    pub fn with_config(config: config::CompilerConfig) -> Self {
        Self { config }
    }

    /// Compile a Qi source file to an executable
    pub fn compile(&self, source_file: PathBuf) -> Result<CompilationResult, CompilerError> {
        let start_time = std::time::Instant::now();
        let warnings: Vec<String> = Vec::new();

        // Multi-file compilation with import resolution
        let result = self.compile_project(source_file)?;

        let duration = start_time.elapsed().as_millis() as u64;
        Ok(CompilationResult {
            executable_path: result.executable_path,
            ir_paths: result.ir_paths,
            object_paths: result.object_paths,
            duration_ms: duration,
            warnings: result.warnings,
        })
    }

    /// Compile a project with multiple files and import resolution
    fn compile_project(&self, entry_file: PathBuf) -> Result<CompilationResult, CompilerError> {
        let mut module_registry = crate::semantic::module::ModuleRegistry::new();
        let mut compiled_modules = std::collections::HashMap::new();
        let warnings = Vec::new();

        // 1. Parse and register all modules
        self.parse_and_collect_modules(
            &entry_file,
            &mut module_registry,
            &mut compiled_modules
        )?;

    let mut object_files: Vec<PathBuf> = Vec::new();
    let mut ir_files: Vec<PathBuf> = Vec::new();

        // 2. Compile each module independently
        for (module_path, ast) in &compiled_modules {
            // Get module info to collect imported functions
            let module_name = module_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            let current_module = module_registry.get_module(&module_name);

            // Collect external function signatures from imported modules
            let mut external_functions = std::collections::HashMap::new();
            if let Some(module) = current_module {
                for import in &module.imports {
                    // Get the imported module
                    let import_module_name = import.module_path.last().unwrap_or(&String::new()).clone();
                    if let Some(imported_module) = module_registry.get_module(&import_module_name) {
                        // Add all exported functions from imported module as external
                        for (func_name, symbol) in &imported_module.exports {
                            if symbol.kind == crate::semantic::module::SymbolKind::Function {
                                if let Some(sig) = &symbol.function_signature {
                                    // Mangle the function name same way as builder does
                                    let mangled_name = self.mangle_function_name(func_name);
                                    let param_types: Vec<String> = sig.parameters.iter()
                                        .map(|(_, ty)| ty.clone())
                                        .collect();
                                    external_functions.insert(mangled_name, (param_types, sig.return_type.clone()));
                                }
                            }
                        }
                    }
                }
            }

            // Generate LLVM IR for this module
            let mut codegen = crate::codegen::CodeGenerator::new(self.config.target_platform.clone());
            
            // Set external functions for this module
            codegen.set_external_functions(external_functions);

            let ir_content = codegen.generate(&ast)
                .map_err(|e| CompilerError::Codegen(format!("代码生成失败 {}: {:?}", module_path.display(), e)))?;

            // Write LLVM IR to file
            let ir_path = module_path.with_extension("ll");
            std::fs::write(&ir_path, ir_content)
                .map_err(CompilerError::Io)?;
            ir_files.push(ir_path.clone());

            // Compile IR to object file (.o)
            let obj_path = self.compile_ir_to_object(&ir_path)?;
            object_files.push(obj_path);
        }

        // 3. Link all object files
        let executable_path = if cfg!(windows) {
            entry_file.with_extension("exe") // e.g., "main.exe"
        } else {
            entry_file.with_extension("")   // e.g., "main"
        };
        self.link_objects(&object_files, &executable_path)?;

        Ok(CompilationResult {
            executable_path,
            ir_paths: ir_files,
            object_paths: object_files,
            duration_ms: 0, // Will be set by caller
            warnings,
        })
    }

    /// Mangle function name (same logic as codegen::builder)
    fn mangle_function_name(&self, name: &str) -> String {
        // ASCII names remain unchanged
        if name.chars().all(|c| c.is_ascii()) {
            return name.to_string();
        }

        // Convert UTF-8 bytes to hex representation
        let utf8_bytes = name.as_bytes();
        let hex_string: String = utf8_bytes
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect();

        // Add prefix to prevent symbol conflicts
        format!("_Z_{}", hex_string)
    }

    /// Compile LLVM IR to object file
    fn compile_ir_to_object(&self, ir_path: &PathBuf) -> Result<PathBuf, CompilerError> {
        let obj_path = ir_path.with_extension("o");
        
        let output = std::process::Command::new("clang")
            .arg("-c")
            .arg(ir_path)
            .arg("-o")
            .arg(&obj_path)
            .output()
            .map_err(CompilerError::Io)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CompilerError::Codegen(
                format!("LLVM IR 编译为目标文件失败: {}", error)
            ));
        }

        Ok(obj_path)
    }

    /// Link object files into executable
    fn link_objects(
        &self,
        object_files: &[PathBuf],
        executable_path: &PathBuf,
    ) -> Result<(), CompilerError> {
        // Get the compiler library path for linking runtime
        let lib_name = if cfg!(windows) {
            "qi_compiler.lib"
        } else {
            "libqi_compiler.a"
        };

        // The library should be in target/debug directory relative to compiler executable
        let compiler_exe_path = std::env::current_exe()?;
        let compiler_dir = compiler_exe_path.parent()
            .ok_or_else(|| CompilerError::Codegen("无法确定编译器目录".to_string()))?;

        // The library should be in the same directory as the compiler executable
        let compiler_lib_path = compiler_dir.join(lib_name);

        // If not found there, try target/debug relative to compiler directory (development build)
        let compiler_lib_path = if compiler_lib_path.exists() {
            compiler_lib_path
        } else {
            // Go up from compiler/bin to project root, then to target/debug
            let project_root = compiler_dir.parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| CompilerError::Codegen("无法确定项目根目录".to_string()))?;
            project_root.join("target").join("debug").join(lib_name)
        };

        let mut command = std::process::Command::new("clang");
        command.arg("-o").arg(executable_path);

        // Add all object files
        for obj in object_files {
            command.arg(obj);
        }

        // Get the compiler library path for linking runtime
        let compiler_exe_path = std::env::current_exe()?;
        let compiler_dir = compiler_exe_path.parent()
            .ok_or_else(|| CompilerError::Codegen("无法确定编译器目录".to_string()))?;

        // The library should be in the same directory as the compiler executable
        let compiler_lib_path = compiler_dir.join(lib_name);

        // If not found there, try target/debug relative to compiler directory (development build)
        let compiler_lib_path = if compiler_lib_path.exists() {
            compiler_lib_path
        } else {
            // Go up from compiler/bin to project root, then to target/debug
            let project_root = compiler_dir.parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| CompilerError::Codegen("无法确定项目根目录".to_string()))?;
            project_root.join("target").join("debug").join(lib_name)
        };

        // Link runtime library
        if compiler_lib_path.exists() {
            command.arg(&compiler_lib_path);
        } else {
            eprintln!("Warning: Runtime library not found at: {:?}", compiler_lib_path);
        }

        // Add threading libraries (platform-specific)
        if cfg!(windows) {
            // On Windows, link with essential Windows API libraries
            command.args(&[
                "-lkernel32",     // Core Windows API functions
                "-luser32",       // User interface functions
                "-ladvapi32",     // Advanced Windows API
                "-lntdll",        // NT native API
                "-luserenv",      // User environment functions (including GetUserProfileDirectoryW)
                "-lws2_32",       // Windows Sockets API
            ]);
        } else {
            // On Unix-like systems, use pthread
            command.arg("-lpthread");
        }

        
        let output = command.output()
            .map_err(CompilerError::Io)?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CompilerError::Codegen(
                format!("链接失败: {}", error)
            ));
        }

        Ok(())
    }

    /// Parse a file and recursively parse its imports
    fn parse_and_collect_modules(
        &self,
        file_path: &PathBuf,
        module_registry: &mut crate::semantic::module::ModuleRegistry,
        compiled_modules: &mut std::collections::HashMap<PathBuf, crate::parser::ast::AstNode>,
    ) -> Result<crate::parser::ast::AstNode, CompilerError> {
        // Check if already compiled
        if let Some(ast) = compiled_modules.get(file_path) {
            return Ok(ast.clone());
        }

        // Read and parse the file
        let source_code = std::fs::read_to_string(file_path)
            .map_err(CompilerError::Io)?;

        let mut lexer = crate::lexer::Lexer::new(source_code);
        let tokens = lexer.tokenize()
            .map_err(|e| CompilerError::Lexical(format!("{}", e)))?;

        let parser = crate::parser::Parser::new();
        let program = parser.parse(tokens)
            .map_err(|e| CompilerError::Parse(format!("解析错误 {}: {}", file_path.display(), e)))?;

        // Convert program to AST node and extract imports
        let ast = crate::parser::ast::AstNode::程序(program.clone());

        // Register current module
        let module_name = file_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let module = crate::semantic::module::Module {
            name: module_name.clone(),
            path: file_path.clone(),
            package_name: program.package_name.clone(),
            exports: module_registry.extract_exports(&program),
            imports: program.imports.iter().map(|imp| crate::semantic::module::Import {
                module_path: imp.module_path.clone(),
                items: imp.items.clone(),
                alias: imp.alias.clone(),
            }).collect(),
        };

        module_registry.register_module(module);

        // Process imports
        for import_stmt in &program.imports {
            let import_path = self.resolve_import_path(file_path, &import_stmt.module_path)?;

            // Recursively parse imported module
            self.parse_and_collect_modules(
                &import_path,
                module_registry,
                compiled_modules
            )?;
        }

        // Store the compiled AST
        compiled_modules.insert(file_path.clone(), ast.clone());

        Ok(ast)
    }

    /// Resolve import path relative to current file
    fn resolve_import_path(
        &self,
        current_file: &PathBuf,
        module_path: &[String]
    ) -> Result<PathBuf, CompilerError> {
        let parent_dir = current_file.parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        // Simple path resolution - just join the module path with .qi extension
        let mut import_path = parent_dir.to_path_buf();
        for component in module_path {
            import_path.push(component);
        }
        import_path.set_extension("qi");

        // Check if file exists
        if import_path.exists() {
            return Ok(import_path);
        }

        // Try pattern: module_name.qi
        if module_path.len() == 1 {
            let simple_path = parent_dir.join(format!("{}.qi", module_path[0]));
            if simple_path.exists() {
                return Ok(simple_path);
            }
        }

        Err(CompilerError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("无法找到导入模块: {:?}", import_path)
        )))
    }
}

/// Result of a compilation operation
#[derive(Debug, Clone)]
pub struct CompilationResult {
    /// Path to the generated executable
    pub executable_path: PathBuf,
    /// Paths to generated LLVM IR files (.ll)
    pub ir_paths: Vec<PathBuf>,
    /// Paths to generated object files (.o)
    pub object_paths: Vec<PathBuf>,
    /// Compilation duration in milliseconds
    pub duration_ms: u64,
    /// Warnings generated during compilation
    pub warnings: Vec<String>,
}


/// Compilation error types
#[derive(Debug, thiserror::Error)]
pub enum CompilerError {
    /// Lexical analysis error
    #[error("词法错误: {0}")]
    Lexical(String),
    /// Parsing error
    #[error("解析错误: {0}")]
    Parse(String),
    /// Semantic analysis error
    #[error("语义错误: {0}")]
    Semantic(String),
    /// Code generation error
    #[error("代码生成错误: {0}")]
    Codegen(String),
    /// I/O error
    #[error("输入/输出错误: {0}")]
    Io(#[from] std::io::Error),
}