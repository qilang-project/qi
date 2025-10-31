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
use crate::semantic::module::ModuleRegistry;

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
        
        // 1.5. Process public imports (re-exports)
        self.process_public_imports(&mut module_registry, &compiled_modules)?;

    let mut object_files: Vec<PathBuf> = Vec::new();
    let mut ir_files: Vec<PathBuf> = Vec::new();

        // 2. Compile each module independently
        for (module_path, ast) in &compiled_modules {
            // Get module info by file path (normalize for consistent lookup)
            let path_key = module_path.canonicalize()
                .unwrap_or_else(|_| module_path.clone())
                .to_string_lossy()
                .to_string();
            let current_module = module_registry.get_module(&path_key);

            // Collect external function signatures from imported modules
            let mut external_functions = std::collections::HashMap::new();
            // Collect import aliases for namespace resolution
            let mut import_aliases = std::collections::HashMap::new();

            if let Some(module) = current_module {
                for import in &module.imports {
                    // Resolve import path to actual module
                    let import_path = self.resolve_import_path(module_path, &import.module_path)?;
                    let import_path_key = import_path.canonicalize()
                        .unwrap_or_else(|_| import_path.clone())
                        .to_string_lossy()
                        .to_string();
                    
                    if let Some(imported_module) = module_registry.get_module(&import_path_key) {
                        // Use package name for the alias
                        let import_module_name = imported_module.package_name.as_ref()
                            .unwrap_or(&imported_module.name);
                        
                        // Set up alias mapping
                        let alias_name = import.alias.as_ref().unwrap_or(import_module_name);
                        import_aliases.insert(alias_name.clone(), import_module_name.clone());

                        // Add all exported functions from imported module as external
                        for (func_name, symbol) in &imported_module.exports {
                            if symbol.kind == crate::semantic::module::SymbolKind::Function {
                                if let Some(sig) = &symbol.function_signature {
                                    // Mangle the function name same way as builder does
                                    let mangled_name = self.mangle_function_name(func_name);
                                    let param_types: Vec<String> = sig.parameters.iter()
                                        .map(|(_, ty)| ty.clone())
                                        .collect();
                                    
                                    // Register function with both flat name and module-qualified name
                                    // 1. Flat import: 最大值
                                    external_functions.insert(mangled_name.clone(), (param_types.clone(), sig.return_type.clone()));
                                    
                                    // 2. Module-qualified import: 数学_最大值
                                    let module_qualified_name = format!("{}_{}", import_module_name, func_name);
                                    let module_qualified_mangled = self.mangle_function_name(&module_qualified_name);
                                    external_functions.insert(module_qualified_mangled, (param_types, sig.return_type.clone()));
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

            // Set import aliases for namespace resolution
            codegen.set_import_aliases(import_aliases);

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
        // Find the runtime library path
        let lib_path = self.find_runtime_library()?;

        let mut command = std::process::Command::new("clang");
        command.arg("-o").arg(executable_path);

        // Add all object files
        for obj in object_files {
            command.arg(obj);
        }

        // Link runtime library
        command.arg(&lib_path);

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

    /// Find the runtime library using multiple search strategies
    fn find_runtime_library(&self) -> Result<PathBuf, CompilerError> {
        let lib_name = if cfg!(windows) {
            "qi_compiler.lib"
        } else {
            "libqi_compiler.a"
        };

        // Get the compiler executable location
        let compiler_exe_path = std::env::current_exe()?;
        let compiler_dir = compiler_exe_path.parent()
            .ok_or_else(|| CompilerError::Codegen("无法确定编译器目录".to_string()))?;

        // Search strategies in order of preference:
        let search_paths = vec![
            // 1. Same directory as compiler executable (for deployed releases)
            compiler_dir.join(lib_name),

            // 2. Current working directory (for local development)
            std::env::current_dir()?.join(lib_name),

            // 3. target/release/ relative to current directory (for release builds)
            std::env::current_dir()?.join("target").join("release").join(lib_name),

            // 4. target/debug/ relative to current directory (for debug builds)
            std::env::current_dir()?.join("target").join("debug").join(lib_name),

            // 5. target/release/ relative to project root (go up from compiler dir)
            compiler_dir.parent()
                .and_then(|p| p.parent())
                .map(|root| root.join("target").join("release").join(lib_name))
                .ok_or_else(|| CompilerError::Codegen("无法确定项目根目录".to_string()))?,

            // 6. target/debug/ relative to project root (go up from compiler dir)
            compiler_dir.parent()
                .and_then(|p| p.parent())
                .map(|root| root.join("target").join("debug").join(lib_name))
                .ok_or_else(|| CompilerError::Codegen("无法确定项目根目录".to_string()))?,
        ];

        // Try each path in order
        for path in &search_paths {
            if path.exists() {
                eprintln!("Found runtime library at: {:?}", path);
                return Ok(path.clone());
            }
        }

        // If none found, return error with list of attempted paths
        let paths_str: Vec<String> = search_paths.iter()
            .map(|p| p.display().to_string())
            .collect();
        Err(CompilerError::Codegen(
            format!("找不到运行时库 {}\n尝试的路径:\n{}", lib_name, paths_str.join("\n"))
        ))
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
    
    /// Process public imports to populate re-exports
    fn process_public_imports(
        &self, 
        module_registry: &mut ModuleRegistry,
        compiled_modules: &std::collections::HashMap<PathBuf, crate::parser::ast::AstNode>
    ) -> Result<(), CompilerError> {
        // Collect all modules that need re-export processing
        let module_paths: Vec<PathBuf> = compiled_modules.keys().cloned().collect();
        
        for module_path in module_paths {
            let path_key = module_path.canonicalize()
                .unwrap_or_else(|_| module_path.clone())
                .to_string_lossy()
                .to_string();
            
            // Get current module (we'll need to modify it)
            let imports_to_process: Vec<Vec<String>> = {
                let module = match module_registry.get_module(&path_key) {
                    Some(m) => m,
                    None => continue,
                };
                
                // Collect public imports
                module.imports.iter()
                    .filter(|imp| {
                        // Check if this is a public import by looking at AST
                        if let Some(ast_node) = compiled_modules.get(&module_path) {
                            if let crate::parser::ast::AstNode::程序(ast) = ast_node {
                                ast.imports.iter().any(|ast_imp| {
                                    ast_imp.is_public && ast_imp.module_path == imp.module_path
                                })
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    })
                    .map(|imp| imp.module_path.clone())
                    .collect()
            };
            
            // Process each public import
            for import_path_parts in imports_to_process {
                let import_path = self.resolve_import_path(&module_path, &import_path_parts)?;
                let import_path_key = import_path.canonicalize()
                    .unwrap_or_else(|_| import_path.clone())
                    .to_string_lossy()
                    .to_string();
                
                // Get exports from imported module
                let exports_to_add: Vec<_> = {
                    if let Some(imported_module) = module_registry.get_module(&import_path_key) {
                        imported_module.exports.clone().into_iter().collect()
                    } else {
                        Vec::new()
                    }
                };
                
                // Add exports to current module (we need mutable access)
                module_registry.add_reexports(&path_key, exports_to_add);
            }
        }
        
        Ok(())
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