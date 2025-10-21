//! CLI command implementations

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Qi Language Compiler CLI
#[derive(Parser)]
#[command(name = "qi")]
#[command(about = "Qi 编程语言编译器")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "Qi Language Team <team@qi-lang.org>")]
pub struct Cli {
    /// 目标平台 (Linux, Windows, macOS, Wasm)
    #[arg(short, long, value_enum)]
    pub target: Option<crate::config::CompilationTarget>,

    /// 优化级别 (none, basic, standard, maximum)
    #[arg(short = 'O', long, value_enum)]
    pub optimization: Option<crate::config::OptimizationLevel>,

    /// 输出文件路径
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// 包含调试符号
    #[arg(short, long)]
    pub debug_symbols: bool,

    /// 禁用运行时检查
    #[arg(long)]
    pub no_runtime_checks: bool,

    /// 将警告视为错误
    #[arg(long)]
    pub warnings_as_errors: bool,

    /// 详细输出
    #[arg(short, long)]
    pub verbose: bool,

    /// 配置文件路径
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// 导入路径
    #[arg(long, value_delimiter = ':')]
    pub import_paths: Vec<PathBuf>,

    /// 子命令
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// 源文件路径
    pub source_files: Vec<PathBuf>,
}

/// CLI 子命令
#[derive(Subcommand)]
pub enum Commands {
    /// 编译 Qi 源文件
    Compile {
        /// 源文件路径
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// 输出文件路径
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// 检查源文件语法（不生成可执行文件）
    Check {
        /// 源文件路径
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },

    /// 格式化源代码
    Format {
        /// 源文件路径
        files: Vec<PathBuf>,

        /// 就地修改文件
        #[arg(short, long)]
        inplace: bool,
    },

    /// 编译并运行 Qi 程序
    Run {
        /// 源文件路径
        #[arg(required = true)]
        file: PathBuf,

        /// 运行参数
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// 显示编译器信息
    Info {
        /// 显示版本信息
        #[arg(short, long)]
        version: bool,

        /// 显示支持的语言特性
        #[arg(short, long)]
        language: bool,

        /// 显示支持的目标平台
        #[arg(short, long)]
        targets: bool,
    },
}

impl Cli {
    /// 执行 CLI 命令
    pub async fn execute(&mut self, config: crate::config::CompilerConfig) -> Result<(), CliError> {
        let command = std::mem::take(&mut self.command);

        match command {
            Some(Commands::Compile { files, output }) => {
                self.compile_files(files, output, config).await
            }
            Some(Commands::Run { file, args }) => {
                self.run_file(file, args, config).await
            }
            Some(Commands::Check { files }) => {
                self.check_files(files, config).await
            }
            Some(Commands::Format { files, inplace }) => {
                self.format_files(files, inplace, config).await
            }
            Some(Commands::Info { version, language, targets }) => {
                self.show_info(version, language, targets).await
            }
            None => {
                // Default compilation behavior when no subcommand is provided
                if self.source_files.is_empty() {
                    return Err(CliError::NoInputFiles);
                }
                self.compile_files(self.source_files.clone(), self.output.clone(), config).await
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
            println!("  调试符号: {}", if config.debug_symbols { "是" } else { "否" });
            println!("  运行时检查: {}", if config.runtime_checks { "是" } else { "否" });
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

            // Move or rename output file if custom output is specified
            if let Some(output_path) = &output {
                if files.len() == 1 {
                    // Single file: rename the output
                    std::fs::rename(&result.executable_path, output_path)?;
                    if config.verbose {
                        println!("  输出文件: {:?}", output_path);
                    }
                } else {
                    // Multiple files: can't use single output path
                    return Err(CliError::Compilation(crate::CompilerError::Codegen(
                        "无法将多个输入文件编译到单个输出文件".to_string()
                    )));
                }
            } else {
                if config.verbose {
                    println!("  生成文件: {:?}", result.executable_path);
                }
            }
        }

        if !config.verbose {
            let count = files.len();
            let target = match config.target_platform {
                crate::config::CompilationTarget::Linux => " (Linux)",
                crate::config::CompilationTarget::Windows => " (Windows)",
                crate::config::CompilationTarget::MacOS => " (macOS)",
                crate::config::CompilationTarget::Wasm => " (WebAssembly)",
            };
            println!("成功编译 {} 个文件{}", count, target);
        }

        Ok(())
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
            println!("正在编译: {:?}", file);
        }

        let compile_result = compiler.compile(file.clone())?;

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
        match config.target_platform {
            crate::config::CompilationTarget::MacOS => {
                // For macOS, we need to compile LLVM IR to executable
                self.run_macos_executable(&compile_result.executable_path, &args, config).await?;
            }
            crate::config::CompilationTarget::Linux => {
                // For Linux, run the executable directly
                self.run_executable(&compile_result.executable_path, &args, config).await?;
            }
            crate::config::CompilationTarget::Windows => {
                // For Windows, run the executable directly
                self.run_executable(&compile_result.executable_path, &args, config).await?;
            }
            crate::config::CompilationTarget::Wasm => {
                // For WebAssembly, we need a different approach
                return Err(CliError::Compilation(crate::CompilerError::Codegen(
                    "WebAssembly 运行暂未实现".to_string()
                )));
            }
        }

        Ok(())
    }

    async fn run_macos_executable(
        &self,
        llvm_ir_path: &std::path::Path,
        args: &[String],
        config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        use std::process::Command;

        // Generate executable path in current directory
        let executable_name = llvm_ir_path.file_stem()
            .ok_or_else(|| CliError::Compilation(crate::CompilerError::Codegen(
                "无效的文件名".to_string()
            )))?
            .to_string_lossy()
            .to_string();

        let temp_executable = std::env::current_dir()?
            .join(format!("{}.exec", executable_name));

        if config.verbose {
            println!("正在编译 LLVM IR 到可执行文件...");
        }

        // Compile LLVM IR to object file
        let output = Command::new("clang")
            .arg("-c")
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
                format!("LLVM IR 编译失败: {}", error)
            )));
        }

        // Link to create executable
        let output = Command::new("clang")
            .arg(&temp_executable.with_extension("o"))
            .arg("-o")
            .arg(&temp_executable)
            .output()
            .map_err(|e| CliError::Io(e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::Compilation(crate::CompilerError::Codegen(
                format!("链接失败: {}", error)
            )));
        }

        if config.verbose {
            println!("正在运行可执行文件...");
        }

        // Run the executable
        let mut cmd = Command::new(&temp_executable);
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
                format!("程序运行失败，退出码: {:?}", output.status.code())
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
                format!("程序运行失败，退出码: {:?}", output.status.code())
            )));
        }

        Ok(())
    }

    async fn check_files(
        &self,
        files: Vec<PathBuf>,
        _config: crate::config::CompilerConfig,
    ) -> Result<(), CliError> {
        if files.is_empty() {
            return Err(CliError::NoInputFiles);
        }

        // TODO: Implement syntax checking
        println!("检查文件: {:?}", files);

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

    async fn show_info(&self, version: bool, language: bool, targets: bool) -> Result<(), CliError> {
        if version || (!language && !targets) {
            println!("Qi 编译器 v{}", env!("CARGO_PKG_VERSION"));
            println!("作者: Qi Language Team <team@qi-lang.org>");
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
            println!("  - Linux x86_64");
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
}