//! Qi Compiler CLI Application
//!
//! Command-line interface for the Qi programming language compiler.

use clap::Parser;
use qi_compiler::cli::commands::Cli;
use qi_compiler::config::CompilerConfig;
use std::process;

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::init();

    // Parse command line arguments
    let mut cli = Cli::parse();

    // Convert CLI arguments to compiler configuration
    let config = match CompilerConfig::from_cli(&cli) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("配置错误: {}", e);
            process::exit(1);
        }
    };

    // Execute the command
    if let Err(e) = cli.execute(config).await {
        eprintln!("{}", e);
        process::exit(1);
    }
}