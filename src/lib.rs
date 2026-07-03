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
pub mod error;
pub mod lexer;
pub mod package;
pub mod parser;
pub mod runtime;
pub mod semantic;
pub mod targets;
pub mod utils;

// Force export of async runtime FFI functions to ensure they're included in the static library
pub use runtime::async_runtime::ffi::{
    qi_runtime_await, qi_runtime_create_task, qi_runtime_spawn_task,
};

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

// Declare external sync runtime functions to force linking
extern "C" {
    fn qi_runtime_mutex_create() -> *mut std::ffi::c_void;
    fn qi_runtime_mutex_lock(mutex: *mut std::ffi::c_void) -> i32;
    fn qi_runtime_mutex_unlock(mutex: *mut std::ffi::c_void) -> i32;
    fn qi_runtime_mutex_trylock(mutex: *mut std::ffi::c_void) -> i32;
    fn qi_runtime_waitgroup_create() -> *mut std::ffi::c_void;
    fn qi_runtime_waitgroup_add(wg: *mut std::ffi::c_void, delta: i32) -> i32;
    fn qi_runtime_waitgroup_wait(wg: *mut std::ffi::c_void) -> i32;
    fn qi_runtime_waitgroup_done(wg: *mut std::ffi::c_void) -> i32;
}

// Dummy function to ensure sync runtime functions are not optimized out
#[doc(hidden)]
#[no_mangle]
pub extern "C" fn _qi_force_link_sync_runtime() {
    // These functions need to be referenced to prevent optimization
    unsafe {
        std::ptr::read_volatile(&qi_runtime_mutex_create as *const _);
        std::ptr::read_volatile(&qi_runtime_mutex_lock as *const _);
        std::ptr::read_volatile(&qi_runtime_mutex_unlock as *const _);
        std::ptr::read_volatile(&qi_runtime_mutex_trylock as *const _);
        std::ptr::read_volatile(&qi_runtime_waitgroup_create as *const _);
        std::ptr::read_volatile(&qi_runtime_waitgroup_add as *const _);
        std::ptr::read_volatile(&qi_runtime_waitgroup_wait as *const _);
        std::ptr::read_volatile(&qi_runtime_waitgroup_done as *const _);
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

        // Resolve relative path to absolute path
        let source_file = if source_file.is_relative() {
            std::env::current_dir()
                .map_err(CompilerError::Io)?
                .join(&source_file)
        } else {
            source_file
        };

        // inkwell 类型化 IR 后端 —— 唯一后端（旧文本后端已淘汰）。
        self.compile_inkwell(source_file, start_time)
    }

    /// inkwell 后端入口：解析 → inkwell 类型化 IR → .o → 复用跨平台链接。
    fn compile_inkwell(
        &self,
        source_file: PathBuf,
        start_time: std::time::Instant,
    ) -> Result<CompilationResult, CompilerError> {
        // 多文件：收集 entry + 所有被导入的**用户模块**（标准库导入不解析文件）。
        // 全部合并进同一次 inkwell 编译 → 单个 .o，跨模块函数/结构体/方法用统一 mangle
        // 天然可见、无需跨对象 extern 声明。
        let mut module_registry = crate::semantic::module::ModuleRegistry::new();
        let mut compiled_modules: std::collections::HashMap<PathBuf, crate::parser::ast::AstNode> =
            std::collections::HashMap::new();
        self.parse_and_collect_modules(&source_file, &mut module_registry, &mut compiled_modules)?;

        // entry 程序放最前（它有 入口()），其余用户模块随后合并。
        let entry_key = source_file
            .canonicalize()
            .unwrap_or_else(|_| source_file.clone());
        let mut programs: Vec<crate::parser::ast::Program> = Vec::new();
        if let Some(crate::parser::ast::AstNode::程序(p)) = compiled_modules.get(&entry_key) {
            programs.push(p.clone());
        } else {
            // 回退：直接解析 entry（canonicalize 失配时）
            let content = std::fs::read_to_string(&source_file).map_err(CompilerError::Io)?;
            let p = crate::parser::Parser::new()
                .parse_source(&content)
                .map_err(|e| CompilerError::Codegen(format!("解析失败: {:?}", e)))?;
            programs.push(p);
        }
        for (path, ast) in &compiled_modules {
            if *path == entry_key {
                continue;
            }
            if let crate::parser::ast::AstNode::程序(p) = ast {
                programs.push(p.clone());
            }
        }

        let obj = source_file.with_extension("o");
        crate::codegen::inkwell_gen::compile_to_object_multi(
            &programs,
            &obj,
            self.config.target_platform,
            self.config.target_arch.as_deref(),
            self.config.optimization_level,
        )
        .map_err(CompilerError::Codegen)?;
        let exe = if cfg!(windows) {
            source_file.with_extension("exe")
        } else {
            source_file.with_extension("")
        };
        // 复用跨平台链接：mac frameworks / linux / windows / zig 交叉，链 libqi_runtime.a。
        self.link_objects(&[obj.clone()], &exe)?;
        Ok(CompilationResult {
            executable_path: exe,
            ir_paths: Vec::new(),
            object_paths: vec![obj],
            duration_ms: start_time.elapsed().as_millis() as u64,
            warnings: Vec::new(),
        })
    }

    /// 交叉编译到 Linux？—— 宿主非 Linux 但目标是 Linux 时，走 zig cc 交叉路径。
    /// （宿主==目标 的本地构建保持原 clang 路径不变，零风险。）
    fn 交叉到linux(&self) -> bool {
        !cfg!(target_os = "linux")
            && self.config.target_platform == config::CompilationTarget::Linux
    }

    /// 目标架构（x86_64 / aarch64），默认 x86_64。
    fn 目标架构(&self) -> String {
        self.config
            .target_arch
            .clone()
            .unwrap_or_else(|| "x86_64".to_string())
    }

    /// zig 交叉编译用的目标三元组（glibc 2.34，匹配多数发行版）。
    fn zig目标三元组(&self) -> String {
        format!("{}-linux-gnu.2.34", self.目标架构())
    }

    /// 对应的 Rust 目标三元组（cargo zigbuild 用，定位交叉构建的运行时归档）。
    fn rust目标三元组(&self) -> String {
        format!("{}-unknown-linux-gnu", self.目标架构())
    }

    /// Link object files into executable
    fn link_objects(
        &self,
        object_files: &[PathBuf],
        executable_path: &PathBuf,
    ) -> Result<(), CompilerError> {
        // 交叉到 Linux：用 zig cc -target 链接，归档用交叉构建的 libqi_runtime.a。
        // rustls + bundled sqlite 已在归档里，无需 -lssl/-lcrypto/-lsqlite3；zig 自带 libc/pthread/m/dl。
        if self.交叉到linux() {
            let lib_path = self.find_linux_runtime_library()?;
            let mut command = std::process::Command::new("zig");
            command.arg("cc").arg("-target").arg(self.zig目标三元组());
            command.arg("-o").arg(executable_path);
            for obj in object_files {
                command.arg(obj);
            }
            command.arg(&lib_path);
            // Rust std 引用 _Unwind_* 解卷符号 → zig 自带的 libunwind 提供。
            command
                .arg("-lunwind")
                .arg("-lpthread")
                .arg("-lm")
                .arg("-ldl");

            let output = command.output().map_err(CompilerError::Io)?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(CompilerError::Codegen(format!(
                    "zig cc 交叉链接失败: {}",
                    stderr
                )));
            }
            return Ok(());
        }

        // 链接无 LLVM 的 qi-runtime 归档（libqi_runtime.a）。
        let lib_path = self.find_host_runtime_library()?;

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
                "-lkernel32", // Core Windows API functions
                "-luser32",   // User interface functions
                "-ladvapi32", // Advanced Windows API
                "-lntdll",    // NT native API
                "-luserenv",  // User environment functions (including GetUserProfileDirectoryW)
                "-lws2_32",   // Windows Sockets API
                "-lshell32",  // Shell functions (SHGetKnownFolderPath)
                "-lole32",    // COM functions (CoTaskMemFree)
            ]);
        } else {
            // On Unix-like systems, use pthread and math library
            command.arg("-lpthread");
            command.arg("-lm"); // Link math library (required for pow, sin, cos, etc.)

            // Linux：libqi_runtime.a 已含 rustls(纯 Rust TLS) + bundled sqlite，
            // 无需 -lssl/-lcrypto/-lsqlite3（与 zig 交叉路径一致）。
            #[cfg(target_os = "linux")]
            {
                command.arg("-ldl");
            }

            // On macOS, add frameworks required by reqwest and GUI
            // Force rebuild 2025-11-15
            #[cfg(target_os = "macos")]
            {
                command
                    .arg("-framework")
                    .arg("AudioUnit")
                    .arg("-framework")
                    .arg("AudioToolbox")
                    .arg("-framework")
                    .arg("CoreAudio")
                    .arg("-framework")
                    .arg("Security")
                    .arg("-framework")
                    .arg("CoreFoundation")
                    .arg("-framework")
                    .arg("SystemConfiguration")
                    .arg("-framework")
                    .arg("Cocoa")
                    .arg("-framework")
                    .arg("QuartzCore")
                    .arg("-framework")
                    .arg("Carbon")
                    .arg("-framework")
                    .arg("CoreGraphics")
                    .arg("-framework")
                    .arg("CoreVideo")
                    .arg("-framework")
                    .arg("AppKit");
            }
        }

        let output = command.output().map_err(CompilerError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let cmd_str = command
                .get_args()
                .map(|arg| format!("\"{}\"", arg.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(" ");
            return Err(CompilerError::Codegen(format!(
                "链接失败: {}\nCommand: clang {}\nStdout: {}\nStderr: {}",
                stderr, cmd_str, stdout, stderr
            )));
        }

        // On Unix-like systems, ensure the executable has execute permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(executable_path).map_err(CompilerError::Io)?;
            let mut permissions = metadata.permissions();
            // Set executable permission (0o755 = rwxr-xr-x)
            permissions.set_mode(0o755);
            std::fs::set_permissions(executable_path, permissions).map_err(CompilerError::Io)?;
        }

        Ok(())
    }

    /// 找交叉构建的 Linux 运行时归档 libqi_runtime.a。
    /// 先看 QI_RUNTIME_LIB_LINUX 环境变量；否则在 workspace 下
    /// qi-runtime/target/<triple>/{debug,release}/libqi_runtime.a 找。
    fn find_linux_runtime_library(&self) -> Result<PathBuf, CompilerError> {
        if let Ok(p) = std::env::var("QI_RUNTIME_LIB_LINUX") {
            let path = PathBuf::from(p);
            if path.exists() {
                return Ok(path);
            }
        }
        let exe = std::env::current_exe()?;
        // qilang/target/debug/qi → 上溯到 qilang
        let workspace = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .ok_or_else(|| CompilerError::Codegen("无法确定 workspace 根".to_string()))?;
        let triple = self.rust目标三元组();
        // --release-runtime 时优先 release（更小的部署二进制），否则优先 debug（开发更快）。
        let profiles: [&str; 2] = if self.config.release_runtime {
            ["release", "debug"]
        } else {
            ["debug", "release"]
        };
        for profile in profiles {
            let p = workspace
                .join("qi-runtime/target")
                .join(&triple)
                .join(profile)
                .join("libqi_runtime.a");
            if p.exists() {
                return Ok(p);
            }
        }
        Err(CompilerError::Codegen(format!(
            "找不到 Linux 运行时归档 libqi_runtime.a。先在 qi-runtime/ 跑：\n  \
             cargo zigbuild --target {} \n\
             或设 QI_RUNTIME_LIB_LINUX 指向它。",
            triple
        )))
    }

    /// 找宿主平台的 qi-runtime 归档（无 LLVM）。inkwell 后端链接它，避免 libqi_compiler.a 拖 LLVM。
    fn find_host_runtime_library(&self) -> Result<PathBuf, CompilerError> {
        if let Ok(p) = std::env::var("QI_RUNTIME_LIB") {
            let path = PathBuf::from(p);
            if path.exists() {
                return Ok(path);
            }
        }
        let exe = std::env::current_exe().map_err(CompilerError::Io)?;
        // 归档文件名平台相关：unix .a / windows msvc .lib
        let 归档名 = if cfg!(windows) {
            "qi_runtime.lib"
        } else {
            "libqi_runtime.a"
        };
        // 安装后/发布包布局：<prefix>/bin/qi → <prefix>/lib/qi/<归档>
        // （installer 装到 /usr/local;release tar/zip 解压即用同构）
        if let Some(prefix) = exe.parent().and_then(|p| p.parent()) {
            let p = prefix.join("lib/qi").join(归档名);
            if p.exists() {
                return Ok(p);
            }
        }
        // 开发布局：qilang/target/debug/qi → 上溯到 qilang
        if let Some(ws) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        {
            for profile in ["debug", "release"] {
                let p = ws.join("qi-runtime/target").join(profile).join(归档名);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
        Err(CompilerError::Codegen(
            "找不到 qi-runtime 归档。先在 qi-runtime/ 跑 cargo build，或设 QI_RUNTIME_LIB。"
                .to_string(),
        ))
    }

    /// Parse a file and recursively parse its imports
    fn parse_and_collect_modules(
        &self,
        file_path: &PathBuf,
        module_registry: &mut crate::semantic::module::ModuleRegistry,
        compiled_modules: &mut std::collections::HashMap<PathBuf, crate::parser::ast::AstNode>,
    ) -> Result<crate::parser::ast::AstNode, CompilerError> {
        self.parse_and_collect_modules_internal(
            file_path,
            module_registry,
            compiled_modules,
            &mut std::collections::HashSet::new(),
        )
    }

    /// Internal implementation with visited set to prevent infinite recursion
    fn parse_and_collect_modules_internal(
        &self,
        file_path: &PathBuf,
        module_registry: &mut crate::semantic::module::ModuleRegistry,
        compiled_modules: &mut std::collections::HashMap<PathBuf, crate::parser::ast::AstNode>,
        visited: &mut std::collections::HashSet<PathBuf>,
    ) -> Result<crate::parser::ast::AstNode, CompilerError> {
        // 规范化路径：同一个文件可能经由符号链接（如 qi_packages/Web -> qi-web）
        // 以两种不同路径被解析到。若按原始路径做 key，会把同一模块编译两次，
        // 链接时报数百个 duplicate symbol。canonicalize 后两条路径塌缩成同一 key。
        let canonical = std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.clone());
        let file_path = &canonical;

        // Prevent infinite recursion
        if visited.contains(file_path) {
            return Ok(compiled_modules.get(file_path).cloned().unwrap_or_else(|| {
                crate::parser::ast::AstNode::程序(crate::parser::ast::Program {
                    package_name: None,
                    imports: vec![],
                    statements: vec![],
                    source_span: Default::default(),
                })
            }));
        }

        // Check if already compiled
        if let Some(ast) = compiled_modules.get(file_path) {
            return Ok(ast.clone());
        }

        // Mark as visited to prevent cycles
        visited.insert(file_path.clone());

        // Read and parse the file
        let source_code = std::fs::read_to_string(file_path).map_err(CompilerError::Io)?;

        let mut lexer = crate::lexer::Lexer::new(source_code);
        let tokens = lexer
            .tokenize()
            .map_err(|e| CompilerError::Lexical(format!("{}", e)))?;

        let parser = crate::parser::Parser::new();
        let program = parser.parse(tokens).map_err(|e| {
            CompilerError::Parse(format!("{}\n  （文件：{}）", e, file_path.display()))
        })?;

        // Convert program to AST node and extract imports
        let ast = crate::parser::ast::AstNode::程序(program.clone());

        // Register current module
        let module_name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let module = crate::semantic::module::Module {
            name: module_name.clone(),
            path: file_path.clone(),
            package_name: program.package_name.clone(),
            exports: module_registry.extract_exports(&program),
            imports: program
                .imports
                .iter()
                .map(|imp| crate::semantic::module::Import {
                    module_path: imp.module_path.clone(),
                    items: imp.items.clone(),
                    alias: imp.alias.clone(),
                })
                .collect(),
        };

        module_registry.register_module(module);

        // FIXED: Auto-discovery with improved conflict handling
        if let Some(package_name) = &program.package_name {
            self.discover_and_parse_same_package_files_fixed(
                file_path,
                package_name,
                module_registry,
                compiled_modules,
                visited,
            )?;
        }

        // Process imports
        for import_stmt in &program.imports {
            // Skip standard library imports (they are built-in)
            let is_stdlib = import_stmt.module_path.get(0).map(|s| s.as_str()) == Some("标准库");
            if is_stdlib {
                continue;
            }

            let import_path = self.resolve_import_path(file_path, &import_stmt.module_path)?;

            // Recursively parse imported module
            self.parse_and_collect_modules_internal(
                &import_path,
                module_registry,
                compiled_modules,
                visited,
            )?;
        }

        // Store the compiled AST
        compiled_modules.insert(file_path.clone(), ast.clone());

        Ok(ast)
    }

    /// Resolve import path with support for third-party packages
    fn resolve_import_path(
        &self,
        current_file: &PathBuf,
        module_path: &[String],
    ) -> Result<PathBuf, CompilerError> {
        let parent_dir = current_file
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        // Check if this is an explicit relative path (starts with . or ..)
        let is_explicit_relative =
            !module_path.is_empty() && (module_path[0] == "." || module_path[0] == "..");

        // 1. Handle explicit relative paths with . and ..
        if is_explicit_relative {
            let mut import_path = parent_dir.to_path_buf();

            // Process each component, handling . and .. specially
            for (i, component) in module_path.iter().enumerate() {
                match component.as_str() {
                    "." => {
                        // Current directory - do nothing
                        continue;
                    }
                    ".." => {
                        // Parent directory - pop the last component
                        import_path.pop();
                    }
                    _ => {
                        // Regular component
                        // If this is the last component, it's the module name
                        if i == module_path.len() - 1 {
                            // Try module_name.qi first
                            let simple_path = import_path.join(format!("{}.qi", component));
                            if simple_path.exists() {
                                return Ok(simple_path);
                            }
                            // Try module_name/module_name.qi (package structure)
                            let package_path = import_path
                                .join(component)
                                .join(format!("{}.qi", component));
                            if package_path.exists() {
                                return Ok(package_path);
                            }
                            // If neither exists, return error
                            return Err(CompilerError::Io(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!(
                                    "无法找到相对路径导入的模块: {} (尝试了 {}.qi 和 {}/{}.qi)",
                                    module_path.join("/"),
                                    simple_path.display(),
                                    import_path.join(component).display(),
                                    component
                                ),
                            )));
                        } else {
                            // Intermediate directory component
                            import_path.push(component);
                        }
                    }
                }
            }
        }

        // 1.5. Try package-internal submodule paths such as Web.控制器 -> <package_root>/控制器.qi
        if !module_path.is_empty() {
            if let Ok(Some(package_manifest)) =
                crate::package::ResolvedPackageManifest::discover(current_file)
            {
                if let Some(package_name) = package_manifest.package_name() {
                    if module_path.first().map(|s| s.as_str()) == Some(package_name) {
                        if let Some(package_path) =
                            package_manifest.resolve_module_path(package_name, module_path)
                        {
                            return Ok(package_path);
                        }
                    }
                }
            }
        }

        // 1.6. Project qi.toml declared dependencies: import 首段 == 依赖别名。
        // 远程依赖（github/任意 git/详细表 git）从本地缓存解析，编译期绝不联网；
        // 缓存缺失时直接报错提示先运行 qi get。
        if let Some(result) = self.resolve_manifest_declared_dependency(current_file, module_path) {
            return result;
        }

        if module_path.len() > 1 {
            if let Some(package_path) =
                self.resolve_package_internal_module_path(current_file, module_path)
            {
                return Ok(package_path);
            }
        }

        // 2. Try relative path import (current directory first) - for non-explicit paths
        let mut import_path = parent_dir.to_path_buf();
        for component in module_path {
            import_path.push(component);
        }
        import_path.set_extension("qi");
        if import_path.exists() {
            return Ok(import_path);
        }

        // 3. Try pattern: module_name.qi in current directory
        if module_path.len() == 1 {
            let simple_path = parent_dir.join(format!("{}.qi", module_path[0]));
            if simple_path.exists() {
                return Ok(simple_path);
            }
        }

        // 4. Try package directory structure: module_name/module_name.qi
        if module_path.len() == 1 {
            let package_dir_path = parent_dir.join(&module_path[0]);
            let package_entry_path = package_dir_path.join(format!("{}.qi", module_path[0]));
            if package_entry_path.exists() {
                return Ok(package_entry_path);
            }
        }

        // 5. Try third-party package paths (QI_PACKAGES_PATH environment variable)
        if !module_path.is_empty() {
            if let Some(local_package_path) =
                self.resolve_local_manifest_package_path(current_file, module_path)
            {
                return Ok(local_package_path);
            }

            if let Ok(package_root) = std::env::var("QI_PACKAGES_PATH") {
                let packages_root = std::path::Path::new(&package_root);
                if let Some(package_path) =
                    self.resolve_package_path_from_root(packages_root, module_path)
                {
                    return Ok(package_path);
                }
            }

            // 6. Try default third-party package locations
            let mut default_package_paths = vec![
                // Current directory packages
                std::env::current_dir().unwrap().join("qi_packages"),
                // Home directory packages
                dirs::home_dir().unwrap().join(".qi").join("packages"),
                // System-wide packages
                std::path::Path::new("/usr/local/lib/qi/packages").to_path_buf(),
            ];

            // Also search ancestor directories of the source file for qi_packages
            // This allows third-party packages in a project root to be found from any subdirectory
            let mut ancestor = parent_dir.to_path_buf();
            loop {
                let candidate = ancestor.join("qi_packages");
                if candidate.is_dir() {
                    default_package_paths.push(candidate);
                }
                if !ancestor.pop() {
                    break;
                }
            }

            for packages_root in default_package_paths {
                if let Some(package_path) =
                    self.resolve_package_path_from_root(&packages_root, module_path)
                {
                    return Ok(package_path);
                }
            }
        }

        Err(CompilerError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "无法找到导入模块: {} (尝试了相对路径、包目录结构和第三方包路径)",
                module_path.join("/")
            ),
        )))
    }

    /// 解析项目 qi.toml 中声明的远程依赖（import 首段 == 依赖别名）。
    ///
    /// 返回 `None` 表示与本步骤无关（无清单/别名不匹配/本地路径依赖），继续走后续解析链；
    /// 返回 `Some(Err(..))` 表示确定是远程依赖但无法解析（缓存缺失 → 提示先运行 qi get）。
    fn resolve_manifest_declared_dependency(
        &self,
        current_file: &PathBuf,
        module_path: &[String],
    ) -> Option<Result<PathBuf, CompilerError>> {
        let alias = module_path.first()?;
        let manifest = crate::package::ResolvedPackageManifest::discover(current_file)
            .ok()
            .flatten()?;
        let dependency = manifest.manifest.dependencies.get(alias)?;

        let spec = match dependency.source() {
            Ok(crate::package::DependencySource::Remote(spec)) => spec,
            // 本地路径依赖保持原有解析机制（qi_packages / 名称匹配等）
            Ok(crate::package::DependencySource::LocalPath(_)) => return None,
            Err(message) => {
                return Some(Err(CompilerError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "qi.toml 中依赖 `{}` 配置有误: {} ({})",
                        alias,
                        message,
                        manifest.manifest_path.display()
                    ),
                ))));
            }
        };

        let package_root = spec.package_root();
        if !package_root.is_dir() {
            return Some(Err(CompilerError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "远程依赖 `{}`（{}）尚未拉取到本地缓存。\n  预期缓存位置: {}\n  请先在项目目录运行: qi get",
                    alias,
                    spec.coordinate(),
                    package_root.display()
                ),
            ))));
        }

        // 优先用包自己的 qi.toml（只认缓存目录本身，不向上查找）；
        // 没有清单时退化为默认布局（<别名>.qi / <仓库名>.qi / 子模块 .qi）。
        let dep_manifest = crate::package::ResolvedPackageManifest::load_dir(&package_root)
            .ok()
            .flatten()
            .unwrap_or_else(|| crate::package::ResolvedPackageManifest {
                manifest_path: package_root.join("qi.toml"),
                root_dir: package_root.clone(),
                manifest: Default::default(),
            });

        let resolved = dep_manifest
            .resolve_module_path(alias, module_path)
            .or_else(|| {
                if module_path.len() == 1 {
                    let candidate = package_root.join(format!("{}.qi", spec.repo));
                    candidate.exists().then_some(candidate)
                } else {
                    None
                }
            });

        match resolved {
            Some(path) => Some(Ok(path)),
            None => Some(Err(CompilerError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "远程依赖 `{}` 已缓存于 {}，但找不到模块 {}（检查包内文件名或 qi.toml 的 [源码] 配置）",
                    alias,
                    package_root.display(),
                    module_path.join(".")
                ),
            )))),
        }
    }

    fn resolve_local_manifest_package_path(
        &self,
        current_file: &PathBuf,
        module_path: &[String],
    ) -> Option<PathBuf> {
        let package_name = module_path.first()?;
        let mut ancestor = current_file.parent()?.to_path_buf();

        loop {
            if let Ok(entries) = std::fs::read_dir(&ancestor) {
                for entry in entries.flatten() {
                    let package_dir = entry.path();
                    if !package_dir.is_dir() {
                        continue;
                    }

                    let manifest =
                        match crate::package::ResolvedPackageManifest::discover(&package_dir) {
                            Ok(Some(manifest))
                                if manifest.root_dir
                                    == package_dir
                                        .canonicalize()
                                        .unwrap_or(package_dir.clone()) =>
                            {
                                manifest
                            }
                            _ => continue,
                        };

                    if manifest.package_name() == Some(package_name.as_str()) {
                        if let Some(package_path) =
                            manifest.resolve_module_path(package_name, module_path)
                        {
                            return Some(package_path);
                        }
                    }
                }
            }

            if !ancestor.pop() {
                break;
            }
        }

        None
    }

    fn resolve_package_internal_module_path(
        &self,
        current_file: &PathBuf,
        module_path: &[String],
    ) -> Option<PathBuf> {
        let package_name = module_path.first()?;
        let mut ancestor = current_file.parent()?.to_path_buf();

        loop {
            let entry_file = ancestor.join(format!("{}.qi", package_name));
            let matches_package_root = ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == package_name)
                .unwrap_or(false);

            if matches_package_root && entry_file.exists() {
                let package_dir = ancestor;
                let submodule_path =
                    self.resolve_submodule_path_in_package(&package_dir, &module_path[1..]);
                if let Some(path) = submodule_path {
                    return Some(path);
                }
                break;
            }

            if !ancestor.pop() {
                break;
            }
        }

        None
    }

    fn resolve_package_path_from_root(
        &self,
        packages_root: &std::path::Path,
        module_path: &[String],
    ) -> Option<PathBuf> {
        let package_name = module_path.first()?;
        let package_dir = packages_root.join(package_name);
        if !package_dir.is_dir() {
            return None;
        }

        if module_path.len() == 1 {
            let package_entry = package_dir.join(format!("{}.qi", package_name));
            if package_entry.exists() {
                return Some(package_entry);
            }
            return None;
        }

        self.resolve_submodule_path_in_package(&package_dir, &module_path[1..])
    }

    fn resolve_submodule_path_in_package(
        &self,
        package_dir: &std::path::Path,
        submodule_parts: &[String],
    ) -> Option<PathBuf> {
        if submodule_parts.is_empty() {
            return None;
        }

        let mut flat_path = package_dir.to_path_buf();
        for part in submodule_parts {
            flat_path.push(part);
        }
        flat_path.set_extension("qi");
        if flat_path.exists() {
            return Some(flat_path);
        }

        if submodule_parts.len() == 1 {
            let nested_name = &submodule_parts[0];
            let nested_entry = package_dir
                .join(nested_name)
                .join(format!("{}.qi", nested_name));
            if nested_entry.exists() {
                return Some(nested_entry);
            }
        }

        None
    }

    /// FIXED VERSION: Discover and parse same-package files without causing external function conflicts
    fn discover_and_parse_same_package_files_fixed(
        &self,
        entry_file: &PathBuf,
        package_name: &str,
        module_registry: &mut crate::semantic::module::ModuleRegistry,
        compiled_modules: &mut std::collections::HashMap<PathBuf, crate::parser::ast::AstNode>,
        visited: &mut std::collections::HashSet<PathBuf>,
    ) -> Result<(), CompilerError> {
        // Only auto-discover files that don't have imports to avoid conflicts
        // This is a conservative approach to prevent external function declaration issues
        let dir = entry_file
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        let entries = std::fs::read_dir(dir).map_err(CompilerError::Io)?;

        for entry in entries {
            let entry = entry.map_err(CompilerError::Io)?;
            let path = entry.path();

            // Skip directories and non-.qi files
            if path.is_dir() || path.extension().and_then(|s| s.to_str()) != Some("qi") {
                continue;
            }

            // Skip the entry file itself
            if path == *entry_file {
                continue;
            }

            // Only auto-include files that:
            // 1. Belong to the same package
            // 2. Have no main function (to avoid duplicate main symbols)
            // 3. Have no imports (to avoid external function conflicts)
            if let Ok(source_code) = std::fs::read_to_string(&path) {
                if let Ok(file_package_info) = self.extract_package_info(&source_code) {
                    if let Some(file_package_name) = file_package_info {
                        if file_package_name == package_name {
                            let has_main_function = source_code.contains("函数 入口");
                            let has_imports = source_code.contains("导入 ");

                            // Only auto-include pure utility files with no imports and no main function
                            if !has_main_function && !has_imports {
                                if !compiled_modules.contains_key(&path) {
                                    self.parse_and_collect_modules_internal(
                                        &path,
                                        module_registry,
                                        compiled_modules,
                                        visited,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Extract package name from source code without full parsing
    fn extract_package_info(&self, source_code: &str) -> Result<Option<String>, CompilerError> {
        // Simple extraction: look for "包 <name>;" pattern
        for line in source_code.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("包") {
                // Find the package declaration
                let rest = trimmed.strip_prefix("包").unwrap_or("").trim();
                if let Some((name_part, _)) = rest.split_once(';') {
                    let name = name_part.trim().to_string();
                    return Ok(Some(name));
                }
                // Try Chinese semicolon
                if let Some((name_part, _)) = rest.split_once('；') {
                    let name = name_part.trim().to_string();
                    return Ok(Some(name));
                }
            }
        }
        Ok(None) // No package declaration found
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
