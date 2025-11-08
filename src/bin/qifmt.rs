//! qifmt - Qi 语言代码格式化工具
//! Code formatter for Qi programming language

use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(name = "qifmt")]
#[command(about = "Qi 语言代码格式化工具", long_about = None)]
#[command(version)]
struct Cli {
    /// 要格式化的文件或目录
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,

    /// 只检查格式，不修改文件
    #[arg(short, long)]
    check: bool,

    /// 显示格式化差异
    #[arg(short, long)]
    diff: bool,

    /// 递归处理目录
    #[arg(short, long)]
    recursive: bool,

    /// 配置文件路径
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// 静默模式
    #[arg(short, long)]
    quiet: bool,

    /// 详细输出
    #[arg(short, long)]
    verbose: bool,

    /// 输出格式
    #[arg(long, value_enum, default_value = "text")]
    format: OutputFormat,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    /// 普通文本输出
    Text,
    /// JSON 格式输出
    Json,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    // 如果没有指定路径，显示帮助信息
    if cli.paths.is_empty() {
        eprintln!("错误: 请指定要格式化的文件或目录");
        eprintln!("\n使用示例:");
        eprintln!("  qifmt 文件.qi");
        eprintln!("  qifmt -r 目录/");
        eprintln!("  qifmt --check 文件.qi");
        eprintln!("\n使用 --help 查看更多选项");
        process::exit(1);
    }

    let config = load_config(&cli);

    let mut total_files = 0;
    let mut formatted_files = 0;
    let mut error_files = 0;

    for path in &cli.paths {
        if path.is_dir() {
            if cli.recursive {
                match format_directory(path, &cli, &config) {
                    Ok((total, formatted, errors)) => {
                        total_files += total;
                        formatted_files += formatted;
                        error_files += errors;
                    }
                    Err(e) => {
                        eprintln!("错误: 处理目录 {:?} 失败: {}", path, e);
                        error_files += 1;
                    }
                }
            } else {
                eprintln!("警告: {:?} 是目录，使用 -r 选项递归处理", path);
            }
        } else if path.is_file() {
            total_files += 1;
            match format_file(path, &cli, &config) {
                Ok(true) => formatted_files += 1,
                Ok(false) => {}, // 已经格式化
                Err(e) => {
                    eprintln!("错误: 格式化 {:?} 失败: {}", path, e);
                    error_files += 1;
                }
            }
        } else {
            eprintln!("警告: {:?} 不存在", path);
        }
    }

    // 输出统计
    if !cli.quiet {
        println!();
        println!("格式化完成:");
        println!("  总文件数: {}", total_files);
        println!("  已格式化: {}", formatted_files);
        if error_files > 0 {
            println!("  失败: {}", error_files);
        }

        if cli.check && formatted_files > 0 {
            println!("\n{} 个文件需要格式化", formatted_files);
            process::exit(1);
        }
    }

    if error_files > 0 {
        process::exit(1);
    }
}

/// 加载配置
fn load_config(_cli: &Cli) -> qi_compiler::formatter::FormatConfig {
    // TODO: 实现从文件加载配置
    qi_compiler::formatter::FormatConfig::default()
}

/// 格式化单个文件
fn format_file(
    path: &PathBuf,
    cli: &Cli,
    config: &qi_compiler::formatter::FormatConfig,
) -> Result<bool, String> {
    if !cli.quiet && cli.verbose {
        println!("处理: {:?}", path);
    }

    // 读取文件
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("读取文件失败: {}", e))?;

    // 使用格式化器
    let formatter = qi_compiler::formatter::Formatter::with_config(config.clone());
    let formatted = formatter.format_file(&source)
        .map_err(|e| format!("格式化失败: {}", e))?;

    // 检查是否需要格式化
    let needs_formatting = source != formatted;

    if cli.check {
        if needs_formatting {
            if !cli.quiet {
                println!("需要格式化: {:?}", path);
            }
            return Ok(true);
        }
        return Ok(false);
    }

    if cli.diff && needs_formatting {
        print_diff(&source, &formatted, path);
    }

    if needs_formatting && !cli.check {
        // 写回文件
        std::fs::write(path, formatted)
            .map_err(|e| format!("写入文件失败: {}", e))?;

        if !cli.quiet {
            println!("已格式化: {:?}", path);
        }
        return Ok(true);
    }

    Ok(false)
}

/// 递归格式化目录
fn format_directory(
    dir: &PathBuf,
    cli: &Cli,
    config: &qi_compiler::formatter::FormatConfig,
) -> Result<(usize, usize, usize), String> {
    let mut total = 0;
    let mut formatted = 0;
    let mut errors = 0;

    let walker = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("qi"));

    for entry in walker {
        let path = entry.path().to_path_buf();
        total += 1;

        match format_file(&path, cli, config) {
            Ok(true) => formatted += 1,
            Ok(false) => {},
            Err(e) => {
                eprintln!("错误: {:?}: {}", path, e);
                errors += 1;
            }
        }
    }

    Ok((total, formatted, errors))
}

/// 打印差异
fn print_diff(original: &str, formatted: &str, path: &PathBuf) {
    println!("--- {:?} (原始)", path);
    println!("+++ {:?} (格式化)", path);

    let original_lines: Vec<&str> = original.lines().collect();
    let formatted_lines: Vec<&str> = formatted.lines().collect();

    for (i, (orig, fmt)) in original_lines.iter().zip(formatted_lines.iter()).enumerate() {
        if orig != fmt {
            println!("@@ 行 {} @@", i + 1);
            println!("- {}", orig);
            println!("+ {}", fmt);
        }
    }
}
