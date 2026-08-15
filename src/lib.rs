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
// rustc 不接受非 ASCII 模块名自动映射文件名（E0754），要显式 #[path]，
// 跟 codegen/inkwell_gen 下那批中文模块一个写法。
#[path = "链接.rs"]
pub mod 链接;

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

        // 库模式（反向 FFI）：产出 C 静态/动态库 + .h，供外部语言调用 Qi 函数。
        if self.config.library_kind.is_some() {
            return self.compile_library(source_file, start_time);
        }

        // inkwell 类型化 IR 后端 —— 唯一后端（旧文本后端已淘汰）。
        self.compile_inkwell(source_file, start_time)
    }

    /// 解析 entry + 所有被导入的用户模块，返回合并编译用的 programs 列表
    /// （entry 在最前，其余按路径排序保证确定性）。可执行 / 库模式 / doctor 静态分析共用。
    pub fn collect_programs(
        &self,
        source_file: &PathBuf,
    ) -> Result<Vec<crate::parser::ast::Program>, CompilerError> {
        self.collect_programs_带检查(source_file, true)
    }

    /// 同 collect_programs，但可关掉内置的 QI_TYPECHECK 度量/strict 段——
    /// `qi check` 走这里（它自己跑 分析编译单元 并管理 warning/strict 语义，
    /// 避免 strict 下 collect 先 Err 被吞导致漏报）。
    pub fn collect_programs_带检查(
        &self,
        source_file: &PathBuf,
        跑类型检查: bool,
    ) -> Result<Vec<crate::parser::ast::Program>, CompilerError> {
        let mut module_registry = crate::semantic::module::ModuleRegistry::new();
        let mut compiled_modules: std::collections::HashMap<PathBuf, crate::parser::ast::AstNode> =
            std::collections::HashMap::new();
        self.parse_and_collect_modules(source_file, &mut module_registry, &mut compiled_modules)?;

        let entry_key = source_file
            .canonicalize()
            .unwrap_or_else(|_| source_file.clone());
        let mut programs: Vec<crate::parser::ast::Program> = Vec::new();
        if let Some(crate::parser::ast::AstNode::程序(p)) = compiled_modules.get(&entry_key) {
            programs.push(p.clone());
        } else {
            let content = std::fs::read_to_string(source_file).map_err(CompilerError::Io)?;
            let mut p = crate::parser::Parser::new()
                .parse_source(&content)
                .map_err(|e| CompilerError::Codegen(format!("解析失败: {:?}", e)))?;
            p.source_path = Some(source_file.display().to_string());
            programs.push(p);
        }
        let mut 其余: Vec<&PathBuf> = compiled_modules
            .keys()
            .filter(|path| **path != entry_key)
            .collect();
        其余.sort();
        for path in 其余 {
            if let Some(crate::parser::ast::AstNode::程序(p)) = compiled_modules.get(path) {
                programs.push(p.clone());
            }
        }

        // 【语义类型检查·度量阶段】QI_TYPECHECK=1 时非致命地对**整个编译单元**
        // 跑一遍宽容语义分析（semantic::单元检查：两遍——先跨文件收集声明，再逐
        // Program 检查），逐条打印结构化错误，绝不中止 codegen。设计原则与沉默
        // 面见 qi/src/semantic/单元检查.rs 模块文档；红码合同见
        // qi/tests/类型检查红码/。无 QI_TYPECHECK 环境变量时零行为变化。
        // 取值语义：未设/off=不跑；strict=报错致命（中止编译）；其他值（如 1）=只打印。
        // 【硬错·永远拦】跟 QI_TYPECHECK 档位无关的那一类：**证明是错**的写法。
        //
        // 目前只有「已知结构体喂给标量形参」——`取查询(上下文值, "k")` 这种。
        // 它零误报（只认本单元声明过的结构体名），而放过去的代价是编译通过、
        // 运行时静默返回空值：真事是「保存后『已保存』永远不出现，cookie 却存对了」，
        // 没有任何报错，查了很久。这种错不该只给个警告让人带到线上。
        //
        // 别的检查仍是度量性质（默认不跑）—— 仓库里有一批历史写法过不了。
        if 跑类型检查 && std::env::var("QI_TYPECHECK").as_deref() != Ok("off") {
            let 硬 = crate::semantic::单元检查::硬错检查(&programs);
            if !硬.is_empty() {
                for e in &硬 {
                    let s: String = format!("{:?}", e).chars().take(2000).collect();
                    eprintln!("[类型检查] {}", s);
                }
                return Err(CompilerError::Codegen(format!(
                    "类型检查失败：{} 处把结构体传给了标量形参（这一类一定是错的，\n                       不受 QI_TYPECHECK 控制；确实要绕过请先改签名）",
                    硬.len()
                )));
            }
        }

        match std::env::var("QI_TYPECHECK") {
            Ok(v) if v != "off" && 跑类型检查 => {
                let 错误 = crate::semantic::分析编译单元(&programs);
                for e in &错误 {
                    // 超长兜底截断防洪水（按字符截，避免切到多字节 UTF-8 中间）
                    let s: String = format!("{:?}", e).chars().take(2000).collect();
                    eprintln!("[类型检查] 报错: {}", s);
                }
                if !错误.is_empty() {
                    eprintln!("[类型检查] 共 {} 条报错", 错误.len());
                    if v == "strict" {
                        return Err(CompilerError::Codegen(format!(
                            "类型检查失败（QI_TYPECHECK=strict）：{} 条报错",
                            错误.len()
                        )));
                    }
                }
            }
            _ => {}
        }

        Ok(programs)
    }

    /// 库模式编译：Qi → .o（含 C ABI 导出包装，不生成 @main）→ 打包成自包含的
    /// 静态库(.a) 或动态库(.dylib/.so/.dll) + 生成 C 头文件(.h)。
    fn compile_library(
        &self,
        source_file: PathBuf,
        start_time: std::time::Instant,
    ) -> Result<CompilationResult, CompilerError> {
        let programs = self.collect_programs(&source_file)?;

        let obj = source_file.with_extension("o");
        let exports = crate::codegen::inkwell_gen::compile_to_object_multi(
            &programs,
            &obj,
            self.config.target_platform,
            self.config.target_arch.as_deref(),
            self.config.optimization_level,
            true, // 库模式：无 @main，生成导出包装 + 构造器
            !self.config.无调试信息,
        )
        .map_err(CompilerError::Codegen)?;

        if exports.is_empty() {
            return Err(CompilerError::Codegen(
                "库模式下未发现任何 `导出 函数` —— 至少导出一个函数才能生成库。".to_string(),
            ));
        }

        // 输出库路径：优先 config.output_file，否则按平台约定命名（lib<基名>.<ext>）。
        let kind = self.config.library_kind.unwrap();
        let stem = source_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("qilib")
            .to_string();
        let lib_out = self.config.output_file.clone().unwrap_or_else(|| {
            let ext = match (kind, self.config.target_platform) {
                (config::LibraryKind::静态, _) => "a".to_string(),
                (config::LibraryKind::动态, config::CompilationTarget::MacOS) => {
                    "dylib".to_string()
                }
                (config::LibraryKind::动态, config::CompilationTarget::Windows) => {
                    "dll".to_string()
                }
                (config::LibraryKind::动态, _) => "so".to_string(),
            };
            source_file.with_file_name(format!("lib{}.{}", stem, ext))
        });

        match kind {
            // 静态库是 ar 打包，链接期才解析符号 —— 外部 C 库由**使用者**在最终链接时提供，
            // 这里无处可放，故只有动态库需要把 外部 "..." 的链接目标带上。
            config::LibraryKind::静态 => self.build_static_library(&obj, &lib_out)?,
            config::LibraryKind::动态 => {
                let 兜底目录 = source_file.parent().unwrap_or(std::path::Path::new("."));
                let extern_libs = Self::collect_extern_libs(&programs, 兜底目录)?;
                self.build_dynamic_library(&obj, &lib_out, &extern_libs)?
            }
        }

        // 生成 C 头文件（.h）。
        let header_out = self
            .config
            .header_output
            .clone()
            .unwrap_or_else(|| lib_out.with_extension("h"));
        let header = Self::render_c_header(&stem, &exports);
        std::fs::write(&header_out, header).map_err(CompilerError::Io)?;

        Ok(CompilationResult {
            executable_path: lib_out,
            ir_paths: Vec::new(),
            object_paths: vec![obj],
            duration_ms: start_time.elapsed().as_millis() as u64,
            warnings: Vec::new(),
        })
    }

    /// 静态库：把用户 .o 与 libqi_runtime.a 合并成一个自包含的 .a。
    /// 做法：在临时目录 `ar x` 解包运行时归档，连同用户 .o 一起 `ar rcs` 重新打包。
    /// 这样使用者只需链接这一个 .a，无需再并列运行时。
    fn build_static_library(&self, obj: &PathBuf, lib_out: &PathBuf) -> Result<(), CompilerError> {
        let runtime = self.find_host_runtime_library()?;
        let tmp = std::env::temp_dir().join(format!("qi_ar_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(CompilerError::Io)?;

        // 解包运行时归档到临时目录
        let out = std::process::Command::new("ar")
            .arg("x")
            .arg(&runtime)
            .current_dir(&tmp)
            .output()
            .map_err(CompilerError::Io)?;
        if !out.status.success() {
            return Err(CompilerError::Codegen(format!(
                "解包运行时归档失败: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }

        // 收集解包出的所有 .o
        let mut members: Vec<PathBuf> = std::fs::read_dir(&tmp)
            .map_err(CompilerError::Io)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("o"))
            .collect();
        members.sort();

        let _ = std::fs::remove_file(lib_out);
        let abs_out = std::fs::canonicalize(lib_out).unwrap_or_else(|_| {
            std::env::current_dir()
                .map(|d| d.join(lib_out))
                .unwrap_or_else(|_| lib_out.clone())
        });
        let abs_obj = std::fs::canonicalize(obj).unwrap_or_else(|_| obj.clone());

        let mut cmd = std::process::Command::new("ar");
        cmd.arg("rcs").arg(&abs_out).arg(&abs_obj);
        for m in &members {
            cmd.arg(m);
        }
        let out = cmd.output().map_err(CompilerError::Io)?;
        let _ = std::fs::remove_dir_all(&tmp);
        if !out.status.success() {
            return Err(CompilerError::Codegen(format!(
                "打包静态库失败: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }

    /// 动态库：clang -shared/-dynamiclib 用户.o + libqi_runtime.a（+ 平台系统库）→ .dylib/.so/.dll。
    fn build_dynamic_library(
        &self,
        obj: &PathBuf,
        lib_out: &PathBuf,
        extern_libs: &[crate::链接::链接项],
    ) -> Result<(), CompilerError> {
        let runtime = self.find_host_runtime_library()?;
        let mut cmd = std::process::Command::new("clang");
        // 同 link_objects：debug map 里的 .o 时间戳会破坏产物可复现性。
        cmd.env("ZERO_AR_DATE", "1");
        #[cfg(target_os = "macos")]
        {
            // 装载名用输出文件名，@rpath 让使用者可 -rpath 定位。
            let name = lib_out
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "libqilib.dylib".to_string());
            cmd.arg("-dynamiclib")
                .arg("-install_name")
                .arg(format!("@rpath/{}", name));
        }
        #[cfg(not(target_os = "macos"))]
        {
            cmd.arg("-shared").arg("-fPIC");
        }
        cmd.arg("-o").arg(lib_out).arg(obj);
        // 运行时归档整体拉入（--whole-archive 保证 no_mangle FFI 符号不被丢弃）。
        #[cfg(target_os = "macos")]
        {
            cmd.arg("-Wl,-force_load").arg(&runtime);
        }
        #[cfg(not(target_os = "macos"))]
        {
            cmd.arg("-Wl,--whole-archive")
                .arg(&runtime)
                .arg("-Wl,--no-whole-archive");
        }

        if cfg!(target_os = "macos") {
            for fw in [
                "AudioUnit",
                "AudioToolbox",
                "CoreAudio",
                "Security",
                "CoreFoundation",
                "SystemConfiguration",
                "Cocoa",
                "QuartzCore",
                "Carbon",
                "CoreGraphics",
                "CoreVideo",
                "AppKit",
            ] {
                cmd.arg("-framework").arg(fw);
            }
        } else {
            cmd.arg("-lpthread").arg("-lm");
            #[cfg(target_os = "linux")]
            {
                cmd.arg("-ldl");
            }
        }

        // 库自身用到的 外部 "..." 目标（-L 搜索路径 + -l/直链文件/framework）。
        let 搜索路径 = self.库搜索路径()?;
        crate::链接::追加链接参数(&mut cmd, &搜索路径, extern_libs, self.目标是mac())
            .map_err(CompilerError::Codegen)?;

        let out = cmd.output().map_err(CompilerError::Io)?;
        if !out.status.success() {
            return Err(CompilerError::Codegen(format!(
                "构建动态库失败: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }

    /// 生成 C 头文件内容（include guard + extern "C" + 每个导出函数原型）。
    fn render_c_header(stem: &str, exports: &[crate::codegen::inkwell_gen::CExportInfo]) -> String {
        // include guard：ASCII 字母数字保留，其余转下划线。全中文名（转换后无字母数字）
        // 会塌成一串下划线并互相撞名，故追加 stem 的稳定 hash 保证唯一。
        let ascii: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect();
        let has_alnum = stem.chars().any(|c| c.is_ascii_alphanumeric());
        let mut hash: u64 = 5381;
        for b in stem.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(b as u64);
        }
        let guard = if has_alnum {
            format!("QI_{}_H", ascii)
        } else {
            format!("QI_LIB_{:X}_H", hash)
        };
        let mut s = String::new();
        s.push_str("/* 由 qi 编译器自动生成的 C 头文件（反向 FFI）。\n");
        s.push_str("   运行时在库加载时自动初始化（constructor），C 侧无需手动 init。\n");
        s.push_str("   返回 char* 的函数所有权归调用方：用毕请 free()。 */\n");
        s.push_str(&format!("#ifndef {}\n#define {}\n\n", guard, guard));
        s.push_str("#include <stdint.h>\n\n");
        s.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");
        for e in exports {
            let args = if e.params.is_empty() {
                "void".to_string()
            } else {
                e.params
                    .iter()
                    .map(|(t, n)| format!("{} {}", t, n))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            s.push_str(&format!("/* Qi: {} */\n", e.qi_name));
            s.push_str(&format!("{} {}({});\n\n", e.ret, e.c_name, args));
        }
        s.push_str("#ifdef __cplusplus\n}\n#endif\n\n");
        s.push_str(&format!("#endif /* {} */\n", guard));
        s
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
        let programs = self.collect_programs(&source_file)?;

        let obj = source_file.with_extension("o");
        crate::codegen::inkwell_gen::compile_to_object_multi(
            &programs,
            &obj,
            self.config.target_platform,
            self.config.target_arch.as_deref(),
            self.config.optimization_level,
            false, // 可执行模式：生成 @main
            !self.config.无调试信息,
        )
        .map_err(CompilerError::Codegen)?;
        let exe = if cfg!(windows) {
            source_file.with_extension("exe")
        } else {
            source_file.with_extension("")
        };
        // 收集所有 外部 "库" 块声明的链接目标（-l / 直链文件 / framework）。
        let 兜底目录 = source_file.parent().unwrap_or(std::path::Path::new("."));
        let extern_libs = Self::collect_extern_libs(&programs, 兜底目录)?;
        // 复用跨平台链接：mac frameworks / linux / windows / zig 交叉，链 libqi_runtime.a。
        self.link_objects(&[obj.clone()], &exe, &extern_libs)?;
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

    /// 目标架构（x86_64 / aarch64 / loongarch64），默认 x86_64。
    /// 已在 config::from_cli 规范化，此处只兜默认。
    fn 目标架构(&self) -> String {
        self.config
            .target_arch
            .clone()
            .unwrap_or_else(|| "x86_64".to_string())
    }

    /// zig 交叉编译用的目标三元组。
    /// glibc 版本按架构选：x86_64/aarch64 用 2.34（覆盖绝大多数发行版）；
    /// loongarch64（龙芯）是较新架构，glibc 支持从 2.36 起 —— 2.34 没有 loong 移植，
    /// 必须用 2.36（信创发行版如统信 UOS/麒麟龙芯版 glibc ≥ 2.36）。
    fn zig目标三元组(&self) -> String {
        let arch = self.目标架构();
        let glibc = if arch == "loongarch64" {
            "2.36"
        } else {
            "2.34"
        };
        format!("{}-linux-gnu.{}", arch, glibc)
    }

    /// 对应的 Rust 目标三元组（cargo zigbuild 用，定位交叉构建的运行时归档）。
    fn rust目标三元组(&self) -> String {
        format!("{}-unknown-linux-gnu", self.目标架构())
    }

    /// 扫所有编译单元的 `外部 "库" { ... }` 块，解析成链接项（`-l名` / 直链文件 /
    /// macOS framework，三种写法见 crate::链接）。去重、去空串（空串 = 不额外链接）；
    /// 库名如已隐式链接（m/c/pthread/dl）重复 `-l` 无害，故不特判，交链接器处理。
    ///
    /// 直链写法的相对路径以**声明该外部块的那个 .qi 文件所在目录**为基准
    /// （Program.source_path），跟导入解析一样跟着源码走，而不是当前工作目录 ——
    /// 否则同一份源码换个目录编译就链不上。source_path 缺失时退回 `兜底目录`（入口文件所在目录）。
    fn collect_extern_libs(
        programs: &[crate::parser::ast::Program],
        兜底目录: &std::path::Path,
    ) -> Result<Vec<crate::链接::链接项>, CompilerError> {
        let mut libs: Vec<crate::链接::链接项> = Vec::new();
        for p in programs {
            let 源目录 = p
                .source_path
                .as_ref()
                .map(PathBuf::from)
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .filter(|d| !d.as_os_str().is_empty())
                .unwrap_or_else(|| 兜底目录.to_path_buf());
            for stmt in &p.statements {
                if let crate::parser::ast::AstNode::外部声明(blk) = stmt {
                    let 项 = crate::链接::解析外部库名(&blk.library, &源目录)
                        .map_err(CompilerError::Codegen)?;
                    if let Some(项) = 项 {
                        if !libs.iter().any(|l| *l == 项) {
                            libs.push(项);
                        }
                    }
                }
            }
        }
        Ok(libs)
    }

    /// 目标平台是 macOS 吗（framework 只在这时可用；mac 宿主交叉到 Linux 也算不是）。
    fn 目标是mac(&self) -> bool {
        self.config.target_platform == config::CompilationTarget::MacOS
    }

    /// `-L` 搜索路径：`--库路径`（CLI）在前，环境变量 `QI_LIBRARY_PATH` 在后。
    fn 库搜索路径(&self) -> Result<Vec<PathBuf>, CompilerError> {
        crate::链接::库搜索路径(&self.config.library_paths).map_err(CompilerError::Codegen)
    }

    /// Link object files into executable
    fn link_objects(
        &self,
        object_files: &[PathBuf],
        executable_path: &PathBuf,
        extern_libs: &[crate::链接::链接项],
    ) -> Result<(), CompilerError> {
        // -L 搜索路径（CLI --库路径 优先，QI_LIBRARY_PATH 追加），两条链接路径共用。
        let 搜索路径 = self.库搜索路径()?;
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
            // 外部块声明的链接目标（-L 搜索路径 + -l/直链文件；framework 在此目标上会报错）
            crate::链接::追加链接参数(&mut command, &搜索路径, extern_libs, false)
                .map_err(CompilerError::Codegen)?;

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
        // macOS 的调试信息不进可执行文件，只留一张 debug map（N_OSO 条目）指回
        // .o —— 而 ld64 会把 .o 的**修改时间**写进那条记录。同一份源码重编两次，
        // 时间戳不同 → 可执行文件字节不同 → LC_UUID 也跟着变，「编译 5 次产物
        // 一致」的确定性回归当场变红。ZERO_AR_DATE 让链接器把时间戳写 0
        // （lldb 见 0 就跳过 .o 新旧校验，断点照常）。
        command.env("ZERO_AR_DATE", "1");
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

        // 外部块声明的链接目标。放在系统库之后，确保符号可被解析。
        // -L 搜索路径也在这里发（ld 对 -L 的位置不敏感，但排在 -l 前面更符合直觉）。
        crate::链接::追加链接参数(&mut command, &搜索路径, extern_libs, self.目标是mac())
            .map_err(CompilerError::Codegen)?;

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

    /// 要链接的运行时归档比 qi-runtime 源码还旧时提醒一句。
    ///
    /// qi-runtime **不是 workspace 成员**（有自己的 target/），在仓库根跑
    /// `cargo build` 根本编不到它。于是「改了 runtime → 重新构建 → 测试」这条路很容易
    /// 断在中间：链进去的还是上一次的归档，不报错、只是行为对不上，极难查
    /// （踩过一次：WS 客户端的 bug 修完测了半天没反应）。
    ///
    /// 只在看得见源码树时提醒，发布包/普通用户不受影响。QI_NO_STALE_WARN=1 可关。
    fn 提醒归档过期(归档: &std::path::Path) {
        if std::env::var("QI_NO_STALE_WARN").is_ok() {
            return;
        }
        // 从当前目录往上找 qi-runtime/src
        let Ok(mut dir) = std::env::current_dir() else {
            return;
        };
        let 源码目录 = loop {
            let p = dir.join("qi-runtime/src");
            if p.is_dir() {
                break Some(p);
            }
            if !dir.pop() {
                break None;
            }
        };
        let Some(源码目录) = 源码目录 else {
            return;
        };
        let 取时间 = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let Some(t归档) = 取时间(归档) else {
            return;
        };
        // 递归看有没有 .rs 比归档新
        fn 有更新的源文件(目录: &std::path::Path, 基准: std::time::SystemTime) -> bool {
            let Ok(项) = std::fs::read_dir(目录) else {
                return false;
            };
            for 条目 in 项.flatten() {
                let p = 条目.path();
                if p.is_dir() {
                    if 有更新的源文件(&p, 基准) {
                        return true;
                    }
                } else if p.extension().is_some_and(|e| e == "rs") {
                    if let Ok(t) = std::fs::metadata(&p).and_then(|m| m.modified()) {
                        if t > 基准 {
                            return true;
                        }
                    }
                }
            }
            false
        }

        if 有更新的源文件(&源码目录, t归档) {
            eprintln!(
                "警告: qi-runtime 源码比要链接的归档新，链进去的是旧运行时。\n\
             \x20 归档: {}\n\
             \x20 qi-runtime 不在 workspace 里，仓库根的 cargo build 编不到它。\n\
             \x20 跑 qi/scripts/同步本地构建.sh（或 cd qi-runtime && cargo build --release）。",
                归档.display()
            );
        }
    }

    /// 找宿主平台的 qi-runtime 归档（无 LLVM）。inkwell 后端链接它，避免 libqi_compiler.a 拖 LLVM。
    fn find_host_runtime_library(&self) -> Result<PathBuf, CompilerError> {
        if let Ok(p) = std::env::var("QI_RUNTIME_LIB") {
            let path = PathBuf::from(p);
            if path.exists() {
                Self::提醒归档过期(&path);
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
                Self::提醒归档过期(&p);
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
                    Self::提醒归档过期(&p);
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
                    source_path: None,
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
        let mut program = parser.parse(tokens).map_err(|e| {
            CompilerError::Parse(format!("{}\n  （文件：{}）", e, file_path.display()))
        })?;
        // 解析器只见到字符串，文件名在这儿补 —— 私有函数按文件消歧要用它
        program.source_path = Some(file_path.display().to_string());

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

            // 包子模块导入（`导入 Web.元信息::{...}`，module_path.len() >= 2 且首段是包名）：
            // 额外加载该包的入口模块（Web.qi），让它的 `公开 导入` 把整个包的模块拉齐。
            // 否则只有 discover_and_parse_same_package_files_fixed 能带出同包文件，而它只
            // 收「无 import」的零散文件 —— 引用了其它模块结构体的文件（如 配置.qi 用 应用）
            // 会被带进来、但定义 应用 的 请求.qi（有 import）不会，导致 应用 未注册、
            // codegen 报「不是结构体」。包按整体编译单元加载才一致（与 `导入 Web::{...}` 同）。
            let first = import_stmt.module_path.first().map(|s| s.as_str());
            let is_relative = matches!(first, Some(".") | Some(".."));
            if !is_relative && import_stmt.module_path.len() >= 2 {
                if let Some(pkg) = first {
                    if let Ok(pkg_entry) = self.resolve_import_path(file_path, &[pkg.to_string()]) {
                        if pkg_entry != import_path {
                            self.parse_and_collect_modules_internal(
                                &pkg_entry,
                                module_registry,
                                compiled_modules,
                                visited,
                            )?;
                        }
                    }
                }
            }
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

            // QI_PACKAGES_PATH 支持 PATH 式多路径（Unix `:`、Windows `;`），按序
            // 先到先得。以前整串被当**一个目录名**：拼了 "a:b" 的调用方所有包
            // 全 miss，报「无法找到导入模块」——本地被祖先扫描兜住看不出来，
            // 独立 checkout 的 CI 才炸（qi-harness service persistence 踩的）。
            // split_paths 对不含分隔符的单路径原样返回，旧用法行为不变。
            if let Ok(package_root) = std::env::var("QI_PACKAGES_PATH") {
                for packages_root in std::env::split_paths(&package_root) {
                    if packages_root.as_os_str().is_empty() {
                        continue;
                    }
                    if let Some(package_path) =
                        self.resolve_package_path_from_root(&packages_root, module_path)
                    {
                        return Ok(package_path);
                    }
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
    /// 返回 `None` 表示与本步骤无关（无清单 / 别名不匹配），继续走后续解析链；
    /// 返回 `Some(Err(..))` 表示确定是声明过的依赖但无法解析（远程缓存缺失 →
    /// 提示先运行 qi get；本地路径写歪 → 指出是路径不存在还是包对不上）。
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
            // 本地路径依赖按声明的路径解析。声明失败时不立即判死：老工程里
            // 「包恰好躺在某级祖先目录下」曾是唯一能跑通的路子（那时这条声明
            // 根本没参与解析），留一层祖先扫描兜底免得升级即炸；兜底也不中，
            // 才报路径级别的具体原因，而不是笼统的「哪儿都没找到」。
            Ok(crate::package::DependencySource::LocalPath(declared)) => {
                return Some(
                    manifest
                        .resolve_local_dependency_module(alias, &declared, module_path)
                        .or_else(|reason| {
                            self.resolve_local_manifest_package_path(current_file, module_path)
                                .ok_or_else(|| {
                                    CompilerError::Io(std::io::Error::new(
                                        std::io::ErrorKind::NotFound,
                                        reason.message(alias, &manifest.manifest_path, module_path),
                                    ))
                                })
                        }),
                );
            }
            // 注册中心依赖装在 <项目根>/qi_packages/<别名>/（扁平安装）。
            // 这里必须显式处理：默认解析链第 5 步扫的是 QI_PACKAGES_PATH 与祖先
            // 目录里的**直接子目录**，qi_packages/<别名> 是孙子层，扫不到。
            Ok(crate::package::DependencySource::Registry { version }) => {
                let package_root = manifest.registry_package_dir(alias);
                if !package_root.is_dir() {
                    return Some(Err(CompilerError::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "注册中心依赖 `{}` = \"{}\" 尚未安装。\n  预期位置: {}\n  请先在项目目录运行: qi 包 安装",
                            alias,
                            version,
                            package_root.display()
                        ),
                    ))));
                }

                let dep_manifest = crate::package::ResolvedPackageManifest::load_dir(&package_root)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| crate::package::ResolvedPackageManifest {
                        manifest_path: package_root.join("qi.toml"),
                        root_dir: package_root.clone(),
                        manifest: Default::default(),
                    });

                // 装好的版本跟 qi.toml 写的对不上，说明改了 qi.toml 还没重装。
                // 不拦下来的话，编译用的是旧代码，报错会指向完全不相干的地方。
                if let Some(标记) = crate::package::install::读标记(&package_root) {
                    if 标记.版本 != version {
                        return Some(Err(CompilerError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!(
                                "注册中心依赖 `{}` 已装的是 {}，qi.toml 要的是 {}。\n  请运行: qi 包 安装",
                                alias, 标记.版本, version
                            ),
                        ))));
                    }
                }

                return Some(
                    dep_manifest
                        .resolve_module_path(alias, module_path)
                        .ok_or_else(|| {
                            CompilerError::Io(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!(
                                    "注册中心依赖 `{}` 已装于 {}，但找不到模块 {}（检查包内文件名或其 qi.toml 的 [源码] 配置）",
                                    alias,
                                    package_root.display(),
                                    module_path.join(".")
                                ),
                            ))
                        }),
                );
            }
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
