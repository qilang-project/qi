//! CLI command implementations

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Qi Language Compiler CLI | Qi 编程语言编译器
#[derive(Parser)]
#[command(name = "qi")]
#[command(about = "奇语言编译器 | Qi Language Compiler")]
#[command(version = concat!("v", env!("CARGO_PKG_VERSION")))]
#[command(disable_help_flag = true)]
#[command(disable_version_flag = true)]
#[command(
    override_usage = "qi [选项] [源文件]... [命令]\n       qi [OPTIONS] [SOURCE_FILES]... [COMMAND]"
)]
#[command(help_template = "\
奇语言编译器 · Qi Language Compiler {version}

用法 | Usage:
       {usage}

命令 | Commands:
{subcommands}

参数 | Arguments:
{positionals}

选项 | Options:
{options}

{after-help}\
")]
#[command(after_help = "示例 | Examples:
  qi run 程序.qi                    编译并运行 | Compile and run
  qi 运行 程序.qi                   中文命令等价 | Chinese commands work too
  qi compile 程序.qi -o 程序        编译出可执行文件 | Build an executable
  qi check 程序.qi                  只查语法 | Syntax check only
  qi test                           跑测试(*_测.qi) | Run tests
  qi get github.com/user/repo@v1.0  拉取远程依赖进缓存并登记 | Fetch remote dependency
  qi get                            拉取 qi.toml 全部远程依赖 | Fetch all remote deps
  qi 包 安装                        装齐 qi.toml [依赖] 的注册中心包 | Install registry deps
  qi 包 添加 海龟 0.1.0             登记依赖并安装 | Add a dependency and install it
  qi 包 发布                        打包当前目录发到注册中心 | Publish the current package
  qi 包 搜索 海龟                   搜注册中心 | Search the registry
  qi --target linux --release-runtime compile 程序.qi -o 程序
                                    交叉编译 Linux | Cross-compile for Linux
  qi compile 程序.qi --库路径 /opt/homebrew/lib
                                    给 外部 \"...\" 加库搜索目录 | Extra -L dir for 外部 blocks

更多信息 | More: https://qilang.org")]
pub struct Cli {
    /// 目标平台 | Target platform (Linux, Windows, macOS, Wasm)
    #[arg(short, long, value_enum)]
    pub target: Option<crate::config::CompilationTarget>,

    /// 交叉编译目标架构 | Target arch (x86_64 | aarch64 | loongarch64/龙芯). 默认 x86_64
    #[arg(long)]
    pub arch: Option<String>,

    /// 交叉链接用 release 运行时归档 | Use release runtime archive for cross-link (smaller binary)
    #[arg(long)]
    pub release_runtime: bool,

    /// 优化级别 | Optimization level (none, basic, standard, maximum)
    #[arg(short = 'O', long, value_enum)]
    pub optimization: Option<crate::config::OptimizationLevel>,

    /// 输出文件路径 | Output file path
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// 包含调试符号 | Include debug symbols
    #[arg(short, long)]
    pub debug_symbols: bool,

    /// 禁用运行时检查 | Disable runtime checks
    #[arg(long)]
    pub no_runtime_checks: bool,

    /// 不生成 DWARF 调试信息（产物更小，但 lldb/gdb 无法按源码断点）
    /// | Omit DWARF debug info (smaller binary, no source-level debugging)
    #[arg(long = "无调试信息", alias = "no-debug-info")]
    pub 无调试信息: bool,

    /// 将警告视为错误 | Treat warnings as errors
    #[arg(long)]
    pub warnings_as_errors: bool,

    /// 详细输出 | Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// 配置文件路径 | Config file path
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// 导入路径 | Import paths
    #[arg(long, value_delimiter = ':')]
    pub import_paths: Vec<PathBuf>,

    /// 链接库搜索目录，可重复；亦可用 QI_LIBRARY_PATH | Library search dir (-L), repeatable
    #[arg(
        long = "库路径",
        alias = "library-path",
        global = true,
        value_name = "目录"
    )]
    pub library_paths: Vec<PathBuf>,

    /// 子命令 | Command
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// 源文件路径 | Source file paths
    pub source_files: Vec<PathBuf>,

    /// 显示帮助信息 | Show help information
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,

    /// 显示版本信息 | Show version information
    #[arg(short = 'V', long, action = clap::ArgAction::Version)]
    version: Option<bool>,
}

/// CLI 子命令 | CLI Commands
#[derive(Subcommand)]
pub enum Commands {
    /// 编译 Qi 源文件为可执行文件 | Compile Qi source files
    #[command(visible_aliases = &["编译"])]
    #[command(help_template = "\
{name} - {about}

用法 | Usage: {usage}

选项 | Options:
{options}
")]
    Compile {
        /// 源文件路径 | Source file paths
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// 输出文件路径 | Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 反向 FFI：产出 C 库供外部语言调用（静态|动态 / static|dynamic）| Emit a C library
        #[arg(long = "库", alias = "lib")]
        library: Option<String>,

        /// C 头文件输出路径（默认与库同基名 .h）| C header output path
        #[arg(long = "头", alias = "header")]
        header: Option<PathBuf>,

        /// 显示帮助信息 | Show help information
        #[arg(short, long, action = clap::ArgAction::Help)]
        help: Option<bool>,
    },

    /// 只检查语法，不产出可执行 | Check syntax only
    #[command(visible_aliases = &["检查"])]
    Check {
        /// 源文件路径 | Source file paths
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },

    /// 格式化源代码 | Format source code
    #[command(visible_aliases = &["格式化"])]
    Format {
        /// 源文件路径 | Source file paths
        files: Vec<PathBuf>,

        /// 就地修改文件 | Modify files in place
        #[arg(short, long)]
        inplace: bool,
    },

    /// 编译并运行 | Compile and run
    #[command(visible_aliases = &["运行"])]
    #[command(help_template = "\
{name} - {about}

用法 | Usage: {usage}

参数 | Arguments:
{positionals}

选项 | Options:
{options}
")]
    Run {
        /// 源文件路径 | Source file path
        #[arg(required = true)]
        file: PathBuf,

        /// 运行参数 | Runtime arguments
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// 显示帮助信息 | Show help information
        #[arg(short, long, action = clap::ArgAction::Help)]
        help: Option<bool>,
    },

    /// 以调试模式编译运行 | Compile and run with debug info
    #[command(visible_aliases = &["调试"])]
    Debug {
        /// 源文件路径 | Source file path
        #[arg(required = true)]
        file: PathBuf,

        /// 运行参数 | Runtime arguments
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// 启用详细调试信息 | Enable verbose debug info
        #[arg(short, long)]
        verbose: bool,

        /// 启用内存监控 | Enable memory monitoring
        #[arg(long)]
        memory: bool,

        /// 启用性能分析 | Enable performance profiling
        #[arg(long)]
        profile: bool,

        /// 启用堆栈跟踪 | Enable stack tracing
        #[arg(long)]
        stack_trace: bool,
    },

    /// 先查语法再运行 | Check syntax, then run
    #[command(visible_aliases = &["检查运行"])]
    CheckRun {
        /// 源文件路径 | Source file path
        #[arg(required = true)]
        file: PathBuf,

        /// 运行参数 | Runtime arguments
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// 仅检查不运行 | Check only, don't run
        #[arg(short, long)]
        check_only: bool,
    },

    /// 显示编译器信息 | Show compiler info
    #[command(visible_aliases = &["信息"])]
    Info {
        /// 显示版本信息 | Show version information
        #[arg(short, long)]
        version: bool,

        /// 显示支持的语言特性 | Show supported language features
        #[arg(short, long)]
        language: bool,

        /// 显示支持的目标平台 | Show supported target platforms
        #[arg(short, long)]
        targets: bool,
    },

    /// 拉取远程包依赖到本地缓存 | Fetch remote package dependencies
    #[command(visible_aliases = &["拉取"])]
    #[command(help_template = "\
{name} - {about}

用法 | Usage: {usage}

参数 | Arguments:
{positionals}

选项 | Options:
{options}
")]
    Get {
        /// 远程包地址，如 github.com/user/repo@v1.0（省略则拉取 qi.toml 全部远程依赖）| Remote package spec
        spec: Option<String>,

        /// 写入 qi.toml 的依赖别名（默认取被拉包的 包.名称 或仓库名）| Dependency alias
        #[arg(long = "名", alias = "alias")]
        name: Option<String>,

        /// 显示帮助信息 | Show help information
        #[arg(short, long, action = clap::ArgAction::Help)]
        help: Option<bool>,
    },

    /// 包管理：安装 / 添加 / 发布 / 搜索注册中心包 | Package manager
    #[command(name = "包", visible_aliases = &["pkg"])]
    #[command(subcommand_required = true, arg_required_else_help = true)]
    #[command(help_template = "\
{name} - {about}

用法 | Usage: {usage}

子命令 | Commands:
{subcommands}

选项 | Options:
{options}

示例 | Examples:
  qi 包 安装                     装齐 qi.toml [依赖] | Install declared deps
  qi 包 添加 海龟 0.1.0          登记并安装 | Add and install
  qi 包 发布                     发布当前包 | Publish current package
  qi 包 搜索 绘图                搜注册中心 | Search registry

环境变量 | Environment:
  QI_REGISTRY        注册中心地址（默认 https://pkg.qilang.org）| Registry base URL
  QI_REGISTRY_TOKEN  发布用的 Bearer token | Publish token
")]
    Pkg {
        #[command(subcommand)]
        command: PkgCommands,
    },

    /// 一体化工程诊断：环检测 + CPU 热点 + 内存泄漏 | All-in-one project diagnosis
    #[command(visible_aliases = &["诊断"])]
    #[command(help_template = "\
{name} - {about}

用法 | Usage: {usage}

参数 | Arguments:
{positionals}

选项 | Options:
{options}
")]
    Doctor {
        /// 源文件路径 | Source file path
        #[arg(required = true)]
        file: PathBuf,

        /// 运行参数（透传给被诊断程序）| Runtime args passed to the program
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// 采集 CPU 热点（QI_PROF 插桩）| Profile CPU hotspots
        #[arg(long = "测CPU", alias = "cpu")]
        cpu: bool,

        /// 检查内存泄漏（QI_RC_REPORT 活跃对象计数）| Check memory leaks
        #[arg(long = "查漏", alias = "leak")]
        leak: bool,

        /// 长驻服务超时采样秒数（到点发信号收报告）| Timeout seconds for long-running services
        #[arg(long = "超时", alias = "timeout")]
        timeout: Option<u64>,

        /// 输出机器可读 JSON | Emit machine-readable JSON
        #[arg(long)]
        json: bool,

        /// 显示帮助信息 | Show help information
        #[arg(short, long, action = clap::ArgAction::Help)]
        help: Option<bool>,
    },

    /// 发现并运行测试（*_测.qi）| Discover and run tests
    #[command(visible_aliases = &["测试"])]
    Test {
        /// 测试目录或文件（默认当前目录递归找 *_测.qi）| Test dir or file
        #[arg(default_value = ".")]
        path: PathBuf,

        /// 只跑名字含该子串的用例（≈ go test -run）| Run only tests whose name contains this
        #[arg(short, long)]
        filter: Option<String>,

        /// 详细：连通过的断言也打印 | Verbose: also print passing assertions
        #[arg(short, long)]
        verbose: bool,
    },
}

/// `qi 包` 下面的四个动作。中文是主名，英文做 alias —— 与 qi 一贯的
/// 「中文命令是第一等公民、英文等价可用」保持一致。
#[derive(Subcommand)]
pub enum PkgCommands {
    /// 装齐 qi.toml [依赖] 里的注册中心包 | Install declared registry dependencies
    #[command(name = "安装", visible_aliases = &["install"])]
    Install {
        /// 详细输出（打印 sha256、lock 命中情况）| Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// 把包写进 qi.toml [依赖] 并立即安装 | Add a dependency and install it
    #[command(name = "添加", visible_aliases = &["add"])]
    Add {
        /// 包名称（可为中文）| Package name
        name: String,

        /// 精确版本，如 0.1.0（v1 不支持范围）| Exact version
        version: String,

        /// 详细输出 | Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// 打包当前目录并发布到注册中心 | Package the current dir and publish
    #[command(name = "发布", visible_aliases = &["publish"])]
    Publish {
        /// 只打包并报 sha256，不上传（发布前自查）| Pack only, do not upload
        #[arg(long = "只打包", alias = "dry-run")]
        dry_run: bool,

        /// 详细输出 | Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// 按名称/描述搜索注册中心的包 | Search the registry by name/description
    #[command(name = "搜索", visible_aliases = &["search"])]
    Search {
        /// 关键词 | Keyword
        keyword: String,
    },

    /// 列出本项目已装的注册中心包 | List installed registry packages
    #[command(name = "列出", visible_aliases = &["list", "ls"])]
    List,
}

impl Cli {
    /// 执行 CLI 命令
    pub async fn execute(&mut self, config: crate::config::CompilerConfig) -> Result<(), CliError> {
        let command = std::mem::take(&mut self.command);

        match command {
            Some(Commands::Compile {
                files,
                output,
                library,
                header,
                help: _,
            }) => {
                let mut config = config;
                if let Some(ref lib) = library {
                    match crate::config::LibraryKind::parse(lib) {
                        Some(k) => config.library_kind = Some(k),
                        None => {
                            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                                format!(
                                    "未知的 --库 类型 `{}` —— 请用 静态/static 或 动态/dynamic。",
                                    lib
                                ),
                            )))
                        }
                    }
                    config.header_output = header;
                    // 库模式：输出路径直接交给编译器（写到最终库路径），compile_files 不再改名。
                    config.output_file = output.clone();
                }
                self.compile_files(files, output, config).await
            }
            Some(Commands::Run {
                file,
                args,
                help: _,
            }) => self.run_file(file, args, config).await,
            Some(Commands::Debug {
                file,
                args,
                verbose,
                memory,
                profile,
                stack_trace,
            }) => {
                self.debug_file(file, args, verbose, memory, profile, stack_trace, config)
                    .await
            }
            Some(Commands::CheckRun {
                file,
                args,
                check_only,
            }) => self.check_run_file(file, args, check_only, config).await,
            Some(Commands::Check { files }) => self.check_files(files, config).await,
            Some(Commands::Format { files, inplace }) => {
                self.format_files(files, inplace, config).await
            }
            Some(Commands::Info {
                version,
                language,
                targets,
            }) => self.show_info(version, language, targets).await,
            Some(Commands::Get {
                spec,
                name,
                help: _,
            }) => crate::cli::get::run(spec, name, config.verbose),
            Some(Commands::Pkg { command }) => match command {
                PkgCommands::Install { verbose } => {
                    crate::cli::pkg::安装(verbose || config.verbose)
                }
                PkgCommands::Add {
                    name,
                    version,
                    verbose,
                } => crate::cli::pkg::添加(name, version, verbose || config.verbose),
                PkgCommands::Publish { dry_run, verbose } => {
                    crate::cli::pkg::发布(dry_run, verbose || config.verbose)
                }
                PkgCommands::Search { keyword } => crate::cli::pkg::搜索(keyword),
                PkgCommands::List => crate::cli::pkg::列出(),
            },
            Some(Commands::Doctor {
                file,
                args,
                cpu,
                leak,
                timeout,
                json,
                help: _,
            }) => {
                self.doctor_file(file, args, cpu, leak, timeout, json, config)
                    .await
            }
            Some(Commands::Test {
                path,
                filter,
                verbose,
            }) => self.test_files(path, filter, verbose, config).await,
            None => {
                // Default compilation behavior when no subcommand is provided
                if self.source_files.is_empty() {
                    return Err(CliError::NoInputFiles);
                }
                self.compile_files(self.source_files.clone(), self.output.clone(), config)
                    .await
            }
        }
    }

    async fn compile_files(
        &self,
        files: Vec<PathBuf>,
        output: Option<PathBuf>,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        if files.is_empty() {
            return Err(CliError::NoInputFiles);
        }

        if config.verbose {
            println!("编译配置:");
            println!("  目标平台: {}", config.target_platform);
            println!("  优化级别: {}", config.optimization_level);
            println!(
                "  调试符号: {}",
                if config.debug_symbols { "是" } else { "否" }
            );
            println!(
                "  运行时检查: {}",
                if config.runtime_checks { "是" } else { "否" }
            );
            println!();
        }

        let compiler = crate::QiCompiler::with_config(config.clone());

        for file in &files {
            if config.verbose {
                println!("正在编译: {:?}", file);
            }

            let result = compiler.compile(file.clone())?;

            if config.verbose {
                println!("  编译完成，耗时: {}ms", result.duration_ms);
            }

            // Handle warnings
            for warning in &result.warnings {
                eprintln!("警告: {}", warning);
            }

            // WebAssembly：编译器只负责产出 wasm32 目标文件，**不链接**。
            // 链接需要 wasm-ld + wasi sysroot + wasm 版最小运行时归档，
            // 这三样目前由 wasm演示/构建.sh 串起来（见该目录 README 段）。
            if matches!(
                config.target_platform,
                crate::config::CompilationTarget::Wasm
            ) {
                let obj = &result.executable_path;
                let obj = if let Some(output_path) = &output {
                    if files.len() > 1 {
                        return Err(CliError::Compilation(crate::CompilerError::Codegen(
                            "无法将多个输入文件编译到单个输出文件".to_string(),
                        )));
                    }
                    std::fs::rename(obj, output_path)?;
                    output_path.clone()
                } else {
                    obj.clone()
                };
                println!("生成 WebAssembly 目标文件: {:?}", obj);
                continue;
            }

            // 库模式：编译器已直接写到最终库路径（+ .h），不走可执行改名逻辑。
            if config.library_kind.is_some() {
                println!("生成库文件: {:?}", result.executable_path);
                let header = result.executable_path.with_extension("h");
                let header = config.header_output.clone().unwrap_or(header);
                println!("生成 C 头文件: {:?}", header);
                // 清理中间 .o
                for obj in &result.object_paths {
                    let _ = std::fs::remove_file(obj);
                }
                continue;
            }

            // QiCompiler::compile already emits and links the executable.
            // Do not feed the executable back into clang as LLVM IR.
            let final_executable = result.executable_path;

            // Move or rename output file if custom output is specified
            if let Some(output_path) = &output {
                if files.len() == 1 {
                    // Single file: rename the output
                    std::fs::rename(&final_executable, output_path)?;
                    if config.verbose {
                        println!("  输出文件: {:?}", output_path);
                    }
                } else {
                    // Multiple files: can't use single output path
                    return Err(CliError::Compilation(crate::CompilerError::Codegen(
                        "无法将多个输入文件编译到单个输出文件".to_string(),
                    )));
                }
            } else {
                if config.verbose {
                    println!("  生成可执行文件: {:?}", final_executable);
                }
            }
        }

        if !config.verbose && config.library_kind.is_none() {
            let count = files.len();
            // wasm 产出的是目标文件（还要 wasm-ld 一步），别叫「可执行文件」
            if matches!(
                config.target_platform,
                crate::config::CompilationTarget::Wasm
            ) {
                println!("成功编译 {} 个 WebAssembly 目标文件", count);
                return Ok(());
            }
            let target = match config.target_platform {
                crate::config::CompilationTarget::Linux => " (Linux)",
                crate::config::CompilationTarget::Windows => " (Windows)",
                crate::config::CompilationTarget::MacOS => " (macOS)",
                crate::config::CompilationTarget::Wasm => " (WebAssembly)",
            };
            println!("成功编译 {} 个可执行文件{}", count, target);
        }

        Ok(())
    }

    /// Create macOS executable from LLVM IR
    async fn create_macos_executable(
        &self,
        llvm_ir_path: &std::path::Path,
        config: &crate::config::CompilerConfig,
    ) -> Result<std::path::PathBuf, CliError> {
        eprintln!(
            "DEBUG: create_macos_executable called for: {:?}",
            llvm_ir_path
        );
        use std::process::Command;

        // Generate executable path in current directory
        let executable_name = llvm_ir_path
            .file_stem()
            .ok_or_else(|| {
                CliError::Compilation(crate::CompilerError::Codegen("无效的文件名".to_string()))
            })?
            .to_string_lossy()
            .to_string();

        let temp_executable = std::env::current_dir()?.join(format!("{}.exec", executable_name));

        if config.verbose {
            println!("正在编译 LLVM IR 到可执行文件...");
            println!("  集成 Qi Runtime 支持...");
        }

        // Compile LLVM IR to object file
        if config.verbose {
            eprintln!("DEBUG: Compiling IR to object file");
        }
        let output = Command::new("clang")
            .arg("-c")
            .arg("-x")
            .arg("ir")
            .arg(llvm_ir_path)
            .arg("-o")
            .arg(&temp_executable.with_extension("o"))
            .output()
            .map_err(|e| CliError::Io(e))?;

        if config.verbose {
            eprintln!(
                "DEBUG: clang -c finished, success={}",
                output.status.success()
            );
        }

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("LLVM IR 编译失败: {}", error),
            )));
        }

        // Link with Qi compiler library (which contains runtime + async symbols)
        if config.verbose {
            eprintln!("DEBUG: Getting compiler library path");
        }
        let compiler_lib_path = self.get_compiler_library_path()?;
        if config.verbose {
            eprintln!("DEBUG: compiler_lib_path = {:?}", compiler_lib_path);
        }

        if config.verbose {
            println!(
                "  链接 Qi Compiler 库 (包含运行时和异步符号): {:?}",
                compiler_lib_path
            );
        }

        // Link the static library - let linker pull only needed symbols
        if config.verbose {
            eprintln!("DEBUG: Linking with clang");
        }
        let mut link_command = Command::new("clang");
        link_command
            .arg(&temp_executable.with_extension("o"))
            .arg(&compiler_lib_path);

        // Add macOS frameworks required by reqwest and GUI
        #[cfg(target_os = "macos")]
        {
            link_command
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

        link_command.arg("-o").arg(&temp_executable);

        eprintln!("DEBUG: Link command: {:?}", link_command);

        let 命令字符串 = format!("{:?}", link_command);
        let output = link_command.output().map_err(|e| CliError::Io(e))?;

        if config.verbose {
            eprintln!(
                "DEBUG: clang link finished, success={}",
                output.status.success()
            );
        }

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            eprintln!("链接命令: {}", 命令字符串);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("链接失败: {}", error),
            )));
        }

        // Clean up object file, but keep executable
        let _ = std::fs::remove_file(&temp_executable.with_extension("o"));

        Ok(temp_executable)
    }

    async fn run_file(
        &self,
        file: PathBuf,
        args: Vec<String>,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        if config.verbose {
            println!("运行配置:");
            println!("  目标平台: {}", config.target_platform);
            println!("  优化级别: {}", config.optimization_level);
            println!("  源文件: {:?}", file);
            println!("  运行参数: {:?}", args);
            println!();
        }

        // Step 1: Compile the file
        let compiler = crate::QiCompiler::with_config(config.clone());

        if config.verbose {
            eprintln!("DEBUG: About to compile");
        }

        if config.verbose {
            println!("正在编译: {:?}", file);
        }

        let compile_result = compiler.compile(file.clone())?;

        if config.verbose {
            eprintln!("DEBUG: Compilation done");
        }

        if config.verbose {
            println!("  编译完成，耗时: {}ms", compile_result.duration_ms);
        }

        // Handle warnings
        for warning in &compile_result.warnings {
            eprintln!("警告: {}", warning);
        }

        if config.verbose {
            println!("  生成文件: {:?}", compile_result.executable_path);
        }

        // Step 2: Determine how to run the executable based on target platform
        let verbose_after = config.verbose;
        match config.target_platform {
            crate::config::CompilationTarget::MacOS => {
                // For macOS, the executable is already compiled and linked
                self.run_executable(&compile_result.executable_path, &args, config)
                    .await?;
            }
            crate::config::CompilationTarget::Linux => {
                // For Linux, run the executable directly
                self.run_executable(&compile_result.executable_path, &args, config)
                    .await?;
            }
            crate::config::CompilationTarget::Windows => {
                // For Windows, run the executable directly
                self.run_executable(&compile_result.executable_path, &args, config)
                    .await?;
            }
            crate::config::CompilationTarget::Wasm => {
                // For WebAssembly, we need a different approach
                return Err(CliError::Compilation(crate::CompilerError::Codegen(
                    "WebAssembly 运行暂未实现".to_string(),
                )));
            }
        }

        // Step 3: Cleanup intermediate files and final executable
        if verbose_after {
            println!("清理临时文件...");
        }

        // Remove object files
        for obj in &compile_result.object_paths {
            let _ = std::fs::remove_file(obj);
        }

        // Remove IR files
        for ir in &compile_result.ir_paths {
            let _ = std::fs::remove_file(ir);
        }

        // Remove the final executable
        let _ = std::fs::remove_file(&compile_result.executable_path);

        Ok(())
    }

    async fn run_macos_executable(
        &self,
        llvm_ir_path: &std::path::Path,
        args: &[String],
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        use std::process::Command;

        if config.verbose {
            eprintln!("DEBUG: run_macos_executable called");
        }

        // Generate executable path in current directory
        let executable_name = llvm_ir_path
            .file_stem()
            .ok_or_else(|| {
                CliError::Compilation(crate::CompilerError::Codegen("无效的文件名".to_string()))
            })?
            .to_string_lossy()
            .to_string();

        let temp_executable = std::env::current_dir()?.join(format!("{}.exec", executable_name));

        if config.verbose {
            eprintln!("DEBUG: temp_executable = {:?}", temp_executable);
        }

        if config.verbose {
            println!("正在编译 LLVM IR 到可执行文件...");
            println!("  集成 Qi Runtime 支持...");
        }

        // Compile LLVM IR to object file
        if config.verbose {
            eprintln!("DEBUG: Compiling IR to object file");
        }
        let output = Command::new("clang")
            .arg("-c")
            .arg("-x")
            .arg("ir")
            .arg(llvm_ir_path)
            .arg("-o")
            .arg(&temp_executable.with_extension("o"))
            .output()
            .map_err(|e| CliError::Io(e))?;

        if config.verbose {
            eprintln!(
                "DEBUG: clang -c finished, success={}",
                output.status.success()
            );
        }

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("LLVM IR 编译失败: {}", error),
            )));
        }

        // Link with Qi compiler library (which contains runtime + async symbols)
        if config.verbose {
            eprintln!("DEBUG: Getting compiler library path");
        }
        let compiler_lib_path = self.get_compiler_library_path()?;
        if config.verbose {
            eprintln!("DEBUG: compiler_lib_path = {:?}", compiler_lib_path);
        }

        if config.verbose {
            println!(
                "  链接 Qi Compiler 库 (包含运行时和异步符号): {:?}",
                compiler_lib_path
            );
        }

        // Link the static library - let linker pull only needed symbols
        if config.verbose {
            eprintln!("DEBUG: Linking with clang");
        }
        let mut link_command = Command::new("clang");
        link_command
            .arg(&temp_executable.with_extension("o"))
            .arg(&compiler_lib_path);

        // Add macOS frameworks required by reqwest and GUI
        #[cfg(target_os = "macos")]
        {
            link_command
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

        link_command.arg("-o").arg(&temp_executable);

        eprintln!("DEBUG: Link command: {:?}", link_command);

        let 命令字符串 = format!("{:?}", link_command);
        let output = link_command.output().map_err(|e| CliError::Io(e))?;

        if config.verbose {
            eprintln!(
                "DEBUG: clang link finished, success={}",
                output.status.success()
            );
        }

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            eprintln!("链接命令: {}", 命令字符串);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("链接失败: {}", error),
            )));
        }

        if config.verbose {
            println!("正在运行可执行文件...");
        }

        // Run the executable
        if config.verbose {
            eprintln!("DEBUG: About to run executable: {:?}", temp_executable);
        }
        let mut cmd = Command::new(&temp_executable);
        for arg in args {
            cmd.arg(arg);
        }

        if config.verbose {
            eprintln!("DEBUG: Calling cmd.output()");
        }
        let output = cmd.output().map_err(|e| CliError::Io(e))?;

        if config.verbose {
            eprintln!(
                "DEBUG: executable finished, success={}",
                output.status.success()
            );
        }

        // Print stdout
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }

        // Print stderr
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }

        if !output.status.success() {
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("程序运行失败，退出码: {:?}", output.status.code()),
            )));
        }

        // Clean up temporary files
        let _ = std::fs::remove_file(&temp_executable.with_extension("o"));
        let _ = std::fs::remove_file(&temp_executable);

        Ok(())
    }

    async fn run_executable(
        &self,
        executable_path: &std::path::Path,
        args: &[String],
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        use std::process::Command;

        if config.verbose {
            println!("正在运行可执行文件...");
        }

        let mut cmd = Command::new(executable_path);
        for arg in args {
            cmd.arg(arg);
        }

        let output = cmd.output().map_err(|e| CliError::Io(e))?;

        // Print stdout
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }

        // Print stderr
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }

        if !output.status.success() {
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("程序运行失败，退出码: {:?}", output.status.code()),
            )));
        }

        Ok(())
    }

    /// 递归收集目录下所有 *_测.qi 测试文件（按路径排序，结果稳定）
    fn 收集测试文件(path: &std::path::Path, 结果: &mut Vec<PathBuf>) {
        if path.is_file() {
            结果.push(path.to_path_buf());
            return;
        }
        let mut 项: Vec<PathBuf> = match std::fs::read_dir(path) {
            Ok(读) => 读.filter_map(|e| e.ok().map(|e| e.path())).collect(),
            Err(_) => return,
        };
        项.sort();
        for p in 项 {
            if p.is_dir() {
                // 跳过包镜像 / 构建产物目录
                let 名 = p
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if 名 == "qi_packages" || 名 == "target" || 名.starts_with('.') {
                    continue;
                }
                Self::收集测试文件(&p, 结果);
            } else if p.extension().map(|e| e == "qi").unwrap_or(false)
                && p.file_stem()
                    .map(|s| s.to_string_lossy().ends_with("_测"))
                    .unwrap_or(false)
            {
                结果.push(p);
            }
        }
    }

    /// `qi test` —— 发现并运行测试文件（*_测.qi），逐个编译+运行，聚合结果，有失败则非零退出。
    /// 每个测试文件自带 入口() + 测试::摘要并退出()，失败时以退出码 1 结束；本命令据此判套件成败。
    async fn test_files(
        &self,
        path: PathBuf,
        filter: Option<String>,
        verbose: bool,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        // 过滤 / 详细 通过环境变量传给测试进程（测试框架读 QI_TEST_FILTER / QI_TEST_VERBOSE）
        if let Some(ref f) = filter {
            std::env::set_var("QI_TEST_FILTER", f);
        }
        if verbose {
            std::env::set_var("QI_TEST_VERBOSE", "1");
        }

        let mut 文件: Vec<PathBuf> = Vec::new();
        Self::收集测试文件(&path, &mut 文件);

        if 文件.is_empty() {
            eprintln!("没有找到测试文件（约定：*_测.qi）于 {:?}", path);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                "no test files".to_string(),
            )));
        }

        let mut 总: usize = 0;
        let mut 败: usize = 0;
        for f in &文件 {
            总 += 1;
            let 名 = f
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            println!("▶ {}", 名);

            let compiler = crate::QiCompiler::with_config(config.clone());
            let 编译 = match compiler.compile(f.clone()) {
                Ok(r) => r,
                Err(e) => {
                    println!("  ✗ 编译失败: {}", e);
                    败 += 1;
                    continue;
                }
            };

            let 跑 = self
                .run_executable(&编译.executable_path, &[], config.clone())
                .await;

            // 清理中间产物
            for obj in &编译.object_paths {
                let _ = std::fs::remove_file(obj);
            }
            for ir in &编译.ir_paths {
                let _ = std::fs::remove_file(ir);
            }
            let _ = std::fs::remove_file(&编译.executable_path);

            match 跑 {
                Ok(()) => println!("  ✓ 套件通过\n"),
                Err(_) => {
                    println!("  ✗ 套件失败\n");
                    败 += 1;
                }
            }
        }

        println!("════════════════════════════");
        println!("测试套件: {}/{} 通过", 总 - 败, 总);
        if 败 > 0 {
            std::process::exit(1);
        }
        Ok(())
    }

    async fn check_files(
        &self,
        files: Vec<PathBuf>,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        if files.is_empty() {
            return Err(CliError::NoInputFiles);
        }

        use crate::parser::Parser;
        let parser = Parser::new();
        let mut all_passed = true;

        for file in &files {
            if config.verbose {
                println!("正在检查文件: {:?}", file);
            }

            let source = std::fs::read_to_string(file).map_err(|e| CliError::Io(e))?;

            match parser.parse_source(&source) {
                Ok(_) => {
                    if config.verbose {
                        println!("  ✓ 语法正确");
                    }
                }
                Err(parse_error) => {
                    all_passed = false;
                    // {} prints the friendly format (line/col + caret + hint);
                    // {:?} dumps the inner Debug enum which is what end users see.
                    eprintln!("文件: {}", file.display());
                    eprintln!("{}", parse_error);
                }
            }
        }

        // 语义类型检查（宽容默认，warning 不挡退出码）。
        // 只对 包 主程序 的入口跑：包成员文件单独作入口时，同包兄弟文件不进
        // 编译单元，会产生「未定义」伪影（与 codegen 行为同源）——降级跳过。
        // 全语料校准：302 绿码 0 误报 / 36 红码全抓（qi/tests/类型检查{绿码,红码}）。
        // QI_TYPECHECK=off 可关；QI_TYPECHECK=strict 时警告升级为失败。
        let mut 类型警告数 = 0usize;
        if all_passed
            && std::env::var("QI_TYPECHECK")
                .map(|v| v == "off")
                .unwrap_or(false)
                == false
        {
            let strict = std::env::var("QI_TYPECHECK")
                .map(|v| v == "strict")
                .unwrap_or(false);
            let compiler = crate::QiCompiler::with_config(config.clone());
            for file in &files {
                // 导入解析失败等一律静默跳过——check 的语义警告是尽力而为。
                // 用「不跑内置检查」版本：strict 语义由本函数统一管理，
                // 避免 collect 内部先 Err 被吞掉导致 strict 漏报。
                let Ok(programs) = compiler.collect_programs_带检查(file, false) else {
                    continue;
                };
                let 是主程序入口 = programs
                    .first()
                    .and_then(|p| p.package_name.as_deref())
                    .map(|n| n == "主程序")
                    .unwrap_or(false);
                if !是主程序入口 {
                    continue;
                }
                // 按 Program 分组：span 是相对各自文件的字节偏移，只有 entry
                // （programs[0]，即 file 本身）的错误能用 file 的源码换算行列；
                // 被导入模块的错误（组 1+）不带行列（源码路径在此层不可得）。
                let 错误组 = crate::semantic::分析编译单元_分组(&programs);
                let 总数: usize = 错误组.iter().map(|g| g.len()).sum();
                if 总数 > 0 {
                    let 源码 = std::fs::read_to_string(file).unwrap_or_default();
                    for (组号, 组) in 错误组.iter().enumerate() {
                        for e in 组 {
                            let span = e.span();
                            let 文案: String = e.渲染人话().chars().take(500).collect();
                            // span (0,0) = 该错误类还没接真实位置 → 退化为不带行列
                            if 组号 == 0 && !(span.start == 0 && span.end == 0) {
                                let (行, 列) =
                                    crate::parser::位置::偏移转行列(&源码, span.start);
                                eprintln!("{}:{}:{} 类型警告: {}", file.display(), 行, 列, 文案);
                            } else {
                                eprintln!("{} 类型警告: {}", file.display(), 文案);
                            }
                        }
                    }
                    类型警告数 += 总数;
                }
            }
            if strict && 类型警告数 > 0 {
                return Err(CliError::Compilation(crate::CompilerError::Codegen(
                    format!("类型检查失败（strict 模式）：{} 条", 类型警告数),
                )));
            }
        }

        if all_passed {
            if !config.verbose {
                if 类型警告数 > 0 {
                    println!(
                        "语法检查通过；类型警告 {} 条（QI_TYPECHECK=strict 可升级为失败）",
                        类型警告数
                    );
                } else {
                    println!("所有文件语法检查通过");
                }
            }
        } else {
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                "语法检查失败".to_string(),
            )));
        }

        Ok(())
    }

    async fn format_files(
        &self,
        files: Vec<PathBuf>,
        _inplace: bool,
        _config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        // TODO: Implement code formatting
        println!("格式化文件: {:?}", files);

        Ok(())
    }

    async fn show_info(
        &self,
        version: bool,
        language: bool,
        targets: bool,
    ) -> Result<(), CliError> {
        if version || (!language && !targets) {
            println!("Qi 编译器 v{}", env!("CARGO_PKG_VERSION"));
            println!("作者: Qi Language Team <team@qilang.org>");
            println!();
        }

        if language {
            println!("支持的语言特性:");
            println!("  - 100% 中文关键字");
            println!("  - Unicode 标识符支持");
            println!("  - 变量声明 (变量, 常量)");
            println!("  - 控制流 (如果, 否则, 当, 对于)");
            println!("  - 函数定义 (函数, 返回)");
            println!("  - 基础数据类型 (整数, 字符串, 布尔, 浮点数)");
            println!("  - 数组操作");
            println!("  - 错误处理和调试支持");
            println!();
        }

        if targets {
            println!("支持的目标平台:");
            println!("  - Linux x86_64 / aarch64 / loongarch64（龙芯，信创）");
            println!("    • 完整的系统调用支持");
            println!("    • POSIX 兼容性");
            println!("    • 共享内存和信号量");
            println!("  - Windows x86_64");
            println!("    • Win32 API 支持");
            println!("    • COM 和注册表操作");
            println!("    • 控制台和进程管理");
            println!("  - macOS x86_64");
            println!("    • CoreFoundation 集成");
            println!("    • Mach 内核调用");
            println!("    • Grand Central Dispatch 支持");
            println!("  - WebAssembly");
            println!("    • 浏览器和 Node.js 支持");
            println!("    • DOM 操作和事件处理");
            println!("    • JavaScript 互操作");
            println!();

            println!("使用方法:");
            println!("  qi compile --target linux source.qi     # 编译为 Linux 可执行文件");
            println!("  qi compile --target windows source.qi   # 编译为 Windows 可执行文件");
            println!("  qi compile --target macos source.qi     # 编译为 macOS 可执行文件");
            println!("  qi compile --target wasm source.qi       # 编译为 WebAssembly 模块");
            println!("  qi run source.qi                       # 编译并运行 Qi 程序");
            println!("  qi run --target macos source.qi         # 编译并运行 macOS 程序");
            println!("  qi run source.qi arg1 arg2             # 编译并运行，传递参数");
            println!();
        }

        Ok(())
    }

    /// Ensure the Qi runtime library is built
    fn ensure_runtime_library_built(
        &self,
        config: &crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        use std::process::Command;

        let runtime_lib = self.get_runtime_library_path()?;

        // Check if runtime library exists
        if runtime_lib.exists() {
            if config.verbose {
                println!("  Runtime 库已存在: {:?}", runtime_lib);
            }
            return Ok(());
        }

        if config.verbose {
            println!("  构建 Qi Runtime 库...");
        }

        // Build the runtime library using cargo
        let project_root = std::env::current_dir()?;

        let output = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("--lib")
            .current_dir(&project_root)
            .output()
            .map_err(|e| CliError::Io(e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("Runtime 库构建失败: {}", error),
            )));
        }

        if config.verbose {
            println!("  Runtime 库构建完成");
        }

        Ok(())
    }

    /// 调试运行 Qi 程序
    async fn debug_file(
        &self,
        file: PathBuf,
        args: Vec<String>,
        verbose: bool,
        memory: bool,
        profile: bool,
        stack_trace: bool,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        println!("🐛 调试模式启动");
        println!("📁 源文件: {:?}", file);
        println!("⚙️  调试选项:");
        if verbose {
            println!("  • 详细输出: 开启");
        }
        if memory {
            println!("  • 内存监控: 开启");
        }
        if profile {
            println!("  • 性能分析: 开启");
        }
        if stack_trace {
            println!("  • 堆栈跟踪: 开启");
        }
        println!();

        // Step 1: Parse and analyze the source file for debugging info
        if verbose || config.verbose {
            println!("🔍 正在分析源代码...");
        }

        use crate::parser::Parser;
        let parser = Parser::new();
        let source = std::fs::read_to_string(&file).map_err(|e| CliError::Io(e))?;

        let program = match parser.parse_source(&source) {
            Ok(program) => {
                if verbose || config.verbose {
                    println!("  ✓ 语法解析成功");
                    println!("  📊 解析统计:");
                    println!("    - 语句数量: {}", program.statements.len());
                }
                program
            }
            Err(parse_error) => {
                eprintln!("{}", parse_error);
                return Err(CliError::Compilation(crate::CompilerError::Codegen(
                    "语法解析失败".to_string(),
                )));
            }
        };

        // Step 2: Compile with debug symbols
        if verbose || config.verbose {
            println!("🛠️  正在编译调试版本...");
        }

        let mut debug_config = config.clone();
        debug_config.debug_symbols = true;
        debug_config.optimization_level = crate::config::OptimizationLevel::None; // No optimization for debugging

        let compiler = crate::QiCompiler::with_config(debug_config);
        let compile_result = compiler.compile(file.clone())?;

        if verbose || config.verbose {
            println!("  ✓ 编译完成，耗时: {}ms", compile_result.duration_ms);
            println!("  🔧 调试符号: 已嵌入");
            println!("  ⚡ 优化级别: 无");
        }

        // Step 3: Setup debugging environment
        if verbose || config.verbose {
            println!("🎯 正在设置调试环境...");
        }

        // Setup environment variables for debugging
        let mut debug_env = std::env::vars().collect::<std::collections::HashMap<String, String>>();

        if memory {
            debug_env.insert("QI_DEBUG_MEMORY".to_string(), "1".to_string());
            println!("  💾 内存监控: 已启用");
        }

        if profile {
            debug_env.insert("QI_DEBUG_PROFILE".to_string(), "1".to_string());
            println!("  📈 性能分析: 已启用");
        }

        if stack_trace {
            debug_env.insert("QI_DEBUG_STACK".to_string(), "1".to_string());
            println!("  📚 堆栈跟踪: 已启用");
        }

        println!();
        println!("🚀 启动调试运行...");
        println!("📝 运行参数: {:?}", args);
        println!("{}", "─".repeat(50));

        // Step 4: Run with debugging
        match config.target_platform {
            crate::config::CompilationTarget::MacOS => {
                self.run_macos_executable_debug(
                    &compile_result.executable_path,
                    &args,
                    debug_env,
                    config,
                )
                .await?;
            }
            crate::config::CompilationTarget::Linux => {
                self.run_executable_debug(
                    &compile_result.executable_path,
                    &args,
                    debug_env,
                    config,
                )
                .await?;
            }
            crate::config::CompilationTarget::Windows => {
                self.run_executable_debug(
                    &compile_result.executable_path,
                    &args,
                    debug_env,
                    config,
                )
                .await?;
            }
            crate::config::CompilationTarget::Wasm => {
                return Err(CliError::Compilation(crate::CompilerError::Codegen(
                    "WebAssembly 调试运行暂未实现".to_string(),
                )));
            }
        }

        println!("{}", "─".repeat(50));
        println!("✅ 调试运行完成");

        Ok(())
    }

    /// 检查并运行 Qi 程序
    async fn check_run_file(
        &self,
        file: PathBuf,
        args: Vec<String>,
        check_only: bool,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        println!("🔍 检查并运行模式");
        println!("📁 源文件: {:?}", file);

        if check_only {
            println!("📋 模式: 仅检查");
        } else {
            println!("🏃 模式: 检查并运行");
        }
        println!();

        // Step 1: Parse and validate
        if config.verbose {
            println!("🔍 正在语法检查...");
        }

        use crate::parser::Parser;
        let parser = Parser::new();
        let source = std::fs::read_to_string(&file).map_err(|e| CliError::Io(e))?;

        let program = match parser.parse_source(&source) {
            Ok(program) => {
                println!("  ✓ 语法检查通过");
                if config.verbose {
                    println!("  📊 语句数量: {}", program.statements.len());
                }
                program
            }
            Err(parse_error) => {
                eprintln!("  ✗ 语法错误: {:?}", parse_error);
                return Err(CliError::Compilation(crate::CompilerError::Codegen(
                    format!("语法检查失败: {:?}", parse_error),
                )));
            }
        };

        if check_only {
            println!("✅ 检查完成，程序语法正确");
            return Ok(());
        }

        // Step 2: Compile and run
        if config.verbose {
            println!("🛠️  正在编译...");
        }

        let compiler = crate::QiCompiler::with_config(config.clone());
        let compile_result = compiler.compile(file.clone())?;

        if config.verbose {
            println!("  ✓ 编译完成，耗时: {}ms", compile_result.duration_ms);
        }

        // Handle warnings
        for warning in &compile_result.warnings {
            eprintln!("⚠️  警告: {}", warning);
        }

        println!();
        println!("🚀 启动程序...");
        println!("📝 运行参数: {:?}", args);
        println!("{}", "─".repeat(40));

        // Step 3: Run the program
        match config.target_platform {
            crate::config::CompilationTarget::MacOS => {
                self.run_macos_executable(&compile_result.executable_path, &args, config)
                    .await?;
            }
            crate::config::CompilationTarget::Linux => {
                self.run_executable(&compile_result.executable_path, &args, config)
                    .await?;
            }
            crate::config::CompilationTarget::Windows => {
                self.run_executable(&compile_result.executable_path, &args, config)
                    .await?;
            }
            crate::config::CompilationTarget::Wasm => {
                return Err(CliError::Compilation(crate::CompilerError::Codegen(
                    "WebAssembly 运行暂未实现".to_string(),
                )));
            }
        }

        println!("{}", "─".repeat(40));
        println!("✅ 程序运行完成");

        Ok(())
    }

    /// Run executable with debugging environment
    async fn run_executable_debug(
        &self,
        executable_path: &std::path::Path,
        args: &[String],
        debug_env: std::collections::HashMap<String, String>,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        use std::process::Command;

        let mut cmd = Command::new(executable_path);
        for arg in args {
            cmd.arg(arg);
        }

        // Add debugging environment variables
        for (key, value) in debug_env {
            cmd.env(key, value);
        }

        let output = cmd.output().map_err(|e| CliError::Io(e))?;

        // Print stdout
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }

        // Print stderr
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }

        if !output.status.success() {
            eprintln!("❌ 程序异常退出，退出码: {:?}", output.status.code());
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("程序运行失败，退出码: {:?}", output.status.code()),
            )));
        }

        Ok(())
    }

    /// Run macOS executable with debugging environment
    async fn run_macos_executable_debug(
        &self,
        llvm_ir_path: &std::path::Path,
        args: &[String],
        debug_env: std::collections::HashMap<String, String>,
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        use std::process::Command;

        // Generate executable path in current directory
        let executable_name = llvm_ir_path
            .file_stem()
            .ok_or_else(|| {
                CliError::Compilation(crate::CompilerError::Codegen("无效的文件名".to_string()))
            })?
            .to_string_lossy()
            .to_string();

        let temp_executable =
            std::env::current_dir()?.join(format!("{}_debug.exec", executable_name));

        if config.verbose {
            println!("🔧 正在编译调试版本可执行文件...");
        }

        // Compile LLVM IR to object file with debug info
        let output = Command::new("clang")
            .arg("-c")
            .arg("-g") // Add debug symbols
            .arg("-O0") // No optimization
            .arg("-x")
            .arg("ir")
            .arg(llvm_ir_path)
            .arg("-o")
            .arg(&temp_executable.with_extension("o"))
            .output()
            .map_err(|e| CliError::Io(e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("LLVM IR 编译失败: {}", error),
            )));
        }

        // Build runtime library if needed
        self.ensure_runtime_library_built(&config)?;

        // Link with Qi runtime to create executable
        let runtime_lib_path = self.get_runtime_library_path()?;

        let mut link_command = Command::new("clang");
        link_command
            .arg(&temp_executable.with_extension("o"))
            .arg(&runtime_lib_path)
            .arg("-o")
            .arg(&temp_executable);

        // Add macOS frameworks required by reqwest and GUI
        #[cfg(target_os = "macos")]
        {
            link_command
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

        let 命令字符串 = format!("{:?}", link_command);
        let output = link_command.output().map_err(|e| CliError::Io(e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            eprintln!("链接命令: {}", 命令字符串);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("链接失败: {}", error),
            )));
        }

        // Run with debugging environment
        let mut cmd = Command::new(&temp_executable);
        for arg in args {
            cmd.arg(arg);
        }

        // Add debugging environment variables
        for (key, value) in debug_env {
            cmd.env(key, value);
        }

        let output = cmd.output().map_err(|e| CliError::Io(e))?;

        // Print stdout
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }

        // Print stderr
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }

        if !output.status.success() {
            eprintln!("❌ 调试程序异常退出，退出码: {:?}", output.status.code());
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("程序运行失败，退出码: {:?}", output.status.code()),
            )));
        }

        // Clean up temporary files
        let _ = std::fs::remove_file(&temp_executable.with_extension("o"));
        let _ = std::fs::remove_file(&temp_executable);

        Ok(())
    }

    /// Get the path to the Qi runtime library
    fn get_runtime_library_path(&self) -> Result<std::path::PathBuf, CliError> {
        let project_root = std::env::current_dir()?;

        // Check if runtime library source exists
        let runtime_src = project_root.join("src/runtime/lib.rs");
        if !runtime_src.exists() {
            // Fallback: try to use the compiler library instead
            return self.get_compiler_library_path();
        }

        let output_dir = project_root.join("target/debug");

        // Create output directory if it doesn't exist
        std::fs::create_dir_all(&output_dir)?;

        // Platform-specific library name
        let output_path = if cfg!(windows) {
            output_dir.join("qi_runtime.lib")
        } else {
            output_dir.join("libqi_runtime.a")
        };

        // If library already exists, return it
        if output_path.exists() {
            return Ok(output_path);
        }

        // We don't have access to config here, so we'll assume verbose for now
        println!("  编译 runtime 源文件到: {:?}", output_path);

        // Use rustc to compile the runtime as a static library
        let rustc_output = std::process::Command::new("rustc")
            .arg("--crate-type=staticlib")
            .arg("-C")
            .arg("panic=abort")
            .arg("-C")
            .arg("link-arg=-lc")
            .arg("-o")
            .arg(&output_path)
            .arg(&runtime_src)
            .current_dir(&project_root)
            .output()
            .map_err(|e| CliError::Io(e))?;

        if !rustc_output.status.success() {
            eprintln!(
                "Rust runtime 编译失败: {}",
                String::from_utf8_lossy(&rustc_output.stderr)
            );
            eprintln!("输出: {}", String::from_utf8_lossy(&rustc_output.stdout));
        }

        if output_path.exists() {
            return Ok(output_path);
        }

        Err(CliError::Compilation(crate::CompilerError::Codegen(
            "无法编译 Qi Runtime 库文件".to_string(),
        )))
    }

    /// Get the path to the Qi compiler library (which contains async runtime symbols)
    fn get_compiler_library_path(&self) -> Result<std::path::PathBuf, CliError> {
        // Get the compiler executable path
        let compiler_exe_path = std::env::current_exe()?;
        let compiler_dir = compiler_exe_path.parent().ok_or_else(|| {
            CliError::Compilation(crate::CompilerError::Codegen(
                "无法确定编译器目录".to_string(),
            ))
        })?;

        // Platform-specific library name
        let lib_name = if cfg!(windows) {
            "qi_compiler.lib"
        } else {
            "libqi_compiler.a"
        };

        // First try: same directory as compiler executable (for deployed builds)
        let lib_path = compiler_dir.join(lib_name);
        if lib_path.exists() {
            return Ok(lib_path);
        }

        // Second try: target/debug relative to project root (for development builds)
        let project_root = compiler_dir
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| {
                CliError::Compilation(crate::CompilerError::Codegen(
                    "无法确定项目根目录".to_string(),
                ))
            })?;

        let lib_path = project_root.join("target").join("debug").join(lib_name);
        if lib_path.exists() {
            return Ok(lib_path);
        }

        // Third try: use current directory (fallback)
        let current_dir = std::env::current_dir()?;
        let lib_path = current_dir.join("target").join("debug").join(lib_name);
        if lib_path.exists() {
            return Ok(lib_path);
        }

        Err(CliError::Compilation(crate::CompilerError::Codegen(
            format!("无法找到 Qi Compiler 库文件: {:?}", lib_path),
        )))
    }
}

/// CLI 错误类型
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// 没有输入文件
    #[error("没有指定输入文件")]
    NoInputFiles,

    /// 编译错误
    #[error("{0}")]
    Compilation(#[from] crate::CompilerError),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(#[from] crate::config::ConfigError),

    /// I/O 错误
    #[error("I/O 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 包管理错误（qi get 等）
    #[error("{0}")]
    Package(String),
}
