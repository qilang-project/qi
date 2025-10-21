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
            println!("成功编译 {} 个文件", count);
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
        }

        if language {
            println!("支持的语言特性:");
            println!("  - 100% 中文关键字");
            println!("  - Unicode 标识符支持");
            println!("  - 变量声明 (变量, 常量)");
            println!("  - 控制流 (如果, 否则, 当, 对于)");
            println!("  - 函数定义 (函数, 返回)");
            println!("  - 基础数据类型 (整数, 字符串, 布尔, 浮点数)");
        }

        if targets {
            println!("支持的目标平台:");
            println!("  - Linux x86_64");
            println!("  - Windows x86_64");
            println!("  - macOS x86_64");
            println!("  - WebAssembly");
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