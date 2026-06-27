//! Compiler configuration and settings

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

impl fmt::Display for CompilationTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompilationTarget::Linux => write!(f, "Linux"),
            CompilationTarget::Windows => write!(f, "Windows"),
            CompilationTarget::MacOS => write!(f, "macOS"),
            CompilationTarget::Wasm => write!(f, "WebAssembly"),
        }
    }
}

impl fmt::Display for OptimizationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptimizationLevel::None => write!(f, "无优化"),
            OptimizationLevel::Basic => write!(f, "基础优化"),
            OptimizationLevel::Standard => write!(f, "标准优化"),
            OptimizationLevel::Maximum => write!(f, "最大优化"),
        }
    }
}

/// Compiler configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerConfig {
    /// Target platform for compilation
    pub target_platform: CompilationTarget,
    /// 交叉编译目标架构（x86_64 / aarch64）。None = 默认 x86_64。仅交叉到 Linux 时生效。
    #[serde(default)]
    pub target_arch: Option<String>,
    /// Optimization level
    pub optimization_level: OptimizationLevel,
    /// Include debug symbols
    pub debug_symbols: bool,
    /// Enable runtime checks
    pub runtime_checks: bool,
    /// Output file path
    pub output_file: Option<PathBuf>,
    /// Additional import paths
    pub import_paths: Vec<PathBuf>,
    /// Configuration file path
    pub config_file: Option<PathBuf>,
    /// Treat warnings as errors
    pub warnings_as_errors: bool,
    /// Verbose output
    pub verbose: bool,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            target_platform: CompilationTarget::default(),
            target_arch: None,
            optimization_level: OptimizationLevel::Basic,
            debug_symbols: false,
            runtime_checks: true,
            output_file: None,
            import_paths: Vec::new(),
            config_file: None,
            warnings_as_errors: false,
            verbose: false,
        }
    }
}

/// Compilation target platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum CompilationTarget {
    /// Linux x86_64
    Linux,
    /// Windows x86_64
    Windows,
    /// macOS x86_64
    MacOS,
    /// WebAssembly
    Wasm,
}

impl Default for CompilationTarget {
    fn default() -> Self {
        // Default to the current platform
        #[cfg(target_os = "linux")]
        return Self::Linux;
        #[cfg(target_os = "windows")]
        return Self::Windows;
        #[cfg(target_os = "macos")]
        return Self::MacOS;
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        return Self::Linux; // Fallback
    }
}

/// Optimization levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum OptimizationLevel {
    /// No optimization
    None,
    /// Basic optimization
    Basic,
    /// Standard optimization
    Standard,
    /// Maximum optimization
    Maximum,
}

impl CompilerConfig {
    /// Create configuration from CLI arguments
    pub fn from_cli(cli: &crate::cli::commands::Cli) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        if cli.arch.is_some() {
            config.target_arch = cli.arch.clone();
        }
        if let Some(target) = &cli.target {
            config.target_platform = *target;
        }

        if let Some(opt_level) = cli.optimization {
            config.optimization_level = opt_level;
        }

        config.debug_symbols = cli.debug_symbols;
        config.runtime_checks = !cli.no_runtime_checks;
        config.output_file = cli.output.clone();
        config.import_paths = cli.import_paths.clone();
        config.config_file = cli.config.clone();
        config.warnings_as_errors = cli.warnings_as_errors;
        config.verbose = cli.verbose;

        // Load config file if specified
        if let Some(config_file) = &config.config_file {
            if config_file.exists() {
                let file_config = Self::from_file(config_file)?;
                config.merge(file_config);
            }
        }

        Ok(config)
    }

    /// Load configuration from file
    pub fn from_file(path: &PathBuf) -> Result<Self, ConfigError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ConfigError::Io(path.clone(), e))?;

        serde_json::from_str(&content).map_err(|e| ConfigError::Parse(path.clone(), e))
    }

    /// Merge another configuration into this one
    pub fn merge(&mut self, other: Self) {
        // CLI arguments take precedence over config file
        if self.target_platform == CompilationTarget::default() {
            self.target_platform = other.target_platform;
        }
        if self.optimization_level == OptimizationLevel::Basic {
            self.optimization_level = other.optimization_level;
        }
        if !other.debug_symbols {
            self.debug_symbols = other.debug_symbols;
        }
        if self.runtime_checks {
            self.runtime_checks = other.runtime_checks;
        }
        if self.output_file.is_none() {
            self.output_file = other.output_file;
        }
        if self.import_paths.is_empty() {
            self.import_paths = other.import_paths;
        }
        if self.config_file.is_none() {
            self.config_file = other.config_file;
        }
        if !self.warnings_as_errors {
            self.warnings_as_errors = other.warnings_as_errors;
        }
        if !self.verbose {
            self.verbose = other.verbose;
        }
    }
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// I/O error
    #[error("Failed to read config file {0}: {1}")]
    Io(PathBuf, std::io::Error),
    /// Parse error
    #[error("Failed to parse config file {0}: {1}")]
    Parse(PathBuf, serde_json::Error),
}
