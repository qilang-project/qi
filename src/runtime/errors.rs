//! Error Handling Module
//!
//! Simple error handling for the Qi runtime.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::runtime::{RuntimeResult, RuntimeError};

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorSeverity {
    /// Informational only
    Info = 0,
    /// Warning
    Warning = 1,
    /// Error
    Error = 2,
    /// Critical
    Critical = 3,
    /// Fatal
    Fatal = 4,
}

impl ErrorSeverity {
    /// Get Chinese name
    pub fn chinese_name(&self) -> &'static str {
        match self {
            ErrorSeverity::Info => "信息",
            ErrorSeverity::Warning => "警告",
            ErrorSeverity::Error => "错误",
            ErrorSeverity::Critical => "严重",
            ErrorSeverity::Fatal => "致命",
        }
    }

    /// Get English name
    pub fn english_name(&self) -> &'static str {
        match self {
            ErrorSeverity::Info => "INFO",
            ErrorSeverity::Warning => "WARNING",
            ErrorSeverity::Error => "ERROR",
            ErrorSeverity::Critical => "CRITICAL",
            ErrorSeverity::Fatal => "FATAL",
        }
    }
}

/// Error statistics tracking
#[derive(Debug, Clone, Default)]
pub struct ErrorStatistics {
    /// Total errors encountered
    pub total_errors: u64,
    /// Errors by severity
    pub errors_by_severity: HashMap<ErrorSeverity, u64>,
    /// First error timestamp
    pub first_error: Option<u64>,
    /// Last error timestamp
    pub last_error: Option<u64>,
}

/// Stack frame for error context
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Function name
    pub function: String,
    /// File name
    pub file: String,
    /// Line number
    pub line: u32,
    /// Column number
    pub column: u32,
}

/// Error handling configuration
#[derive(Debug, Clone)]
pub struct ErrorConfig {
    /// Enable automatic error recovery
    pub auto_recovery: bool,
    /// Maximum error history size
    pub max_history_size: usize,
    /// Enable error context tracking
    pub enable_context_tracking: bool,
    /// Error logging level
    pub log_level: ErrorSeverity,
}

impl Default for ErrorConfig {
    fn default() -> Self {
        Self {
            auto_recovery: true,
            max_history_size: 1000,
            enable_context_tracking: true,
            log_level: ErrorSeverity::Warning,
        }
    }
}

/// Extended error statistics
#[derive(Debug, Clone, Default)]
pub struct ErrorStats {
    /// Total errors encountered
    pub total_errors: u64,
    /// Errors by severity
    pub errors_by_severity: HashMap<ErrorSeverity, u64>,
    /// Errors by type
    pub errors_by_type: HashMap<String, u64>,
    /// Recovered errors
    pub recovered_errors: u64,
    /// Timestamp of last error
    pub last_error_timestamp: Option<u64>,
}

/// Error context with extended information
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Error identifier
    pub id: String,
    /// Error message
    pub message: String,
    /// Error severity
    pub severity: ErrorSeverity,
    /// Stack frames
    pub stack_frames: Vec<StackFrame>,
    /// Timestamp
    pub timestamp: u64,
    /// Context metadata
    pub metadata: HashMap<String, String>,
}

/// Error handler for processing and recovering from errors
#[derive(Debug)]
pub struct ErrorHandler {
    /// Error history
    error_history: Vec<ErrorContext>,
}

impl ErrorHandler {
    /// Create new error handler
    pub fn new() -> Self {
        Self {
            error_history: Vec::new(),
        }
    }

    /// Add error to history
    pub fn add_to_history(&mut self, context: ErrorContext) {
        self.error_history.push(context);

        // Limit history size
        if self.error_history.len() > 1000 {
            self.error_history.remove(0);
        }
    }
}

impl Default for ErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Error interface
#[derive(Debug)]
pub struct ErrorInterface {
    /// Error handler
    handler: std::sync::Arc<std::sync::Mutex<ErrorHandler>>,
    /// Error statistics
    stats: std::sync::Arc<std::sync::Mutex<ErrorStats>>,
    /// Configuration
    config: std::sync::Arc<std::sync::Mutex<ErrorConfig>>,
}

impl ErrorInterface {
    /// Create new error interface
    pub fn new() -> Self {
        let config = ErrorConfig::default();

        Self {
            handler: std::sync::Arc::new(std::sync::Mutex::new(ErrorHandler::new())),
            stats: std::sync::Arc::new(std::sync::Mutex::new(ErrorStats::default())),
            config: std::sync::Arc::new(std::sync::Mutex::new(config)),
        }
    }

    /// Create error interface with custom configuration
    pub fn with_config(config: ErrorConfig) -> Self {
        Self {
            handler: std::sync::Arc::new(std::sync::Mutex::new(ErrorHandler::new())),
            stats: std::sync::Arc::new(std::sync::Mutex::new(ErrorStats::default())),
            config: std::sync::Arc::new(std::sync::Mutex::new(config)),
        }
    }

    /// Handle an error
    pub fn handle_error(&self, error: &RuntimeError) -> RuntimeResult<ErrorContext> {
        let context = self.create_error_context(error)?;

        // Record error statistics
        self.record_error_stats(&context)?;

        // Add to error history
        {
            let mut handler = self.handler.lock().unwrap();
            handler.add_to_history(context.clone());
        }

        Ok(context)
    }

    /// Create error context from runtime error
    pub fn create_error_context(&self, error: &RuntimeError) -> RuntimeResult<ErrorContext> {
        let severity = self.determine_error_severity(error);

        let context = ErrorContext {
            id: self.generate_error_id(),
            message: error.to_string(),
            severity,
            stack_frames: vec![], // Would be populated with actual stack frames
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            metadata: HashMap::new(),
        };

        Ok(context)
    }

    /// Get error statistics
    pub fn get_error_stats(&self) -> RuntimeResult<ErrorStats> {
        let stats = self.stats.lock().unwrap();
        Ok(stats.clone())
    }

    /// Get configuration
    pub fn get_config(&self) -> RuntimeResult<ErrorConfig> {
        let config = self.config.lock().unwrap();
        Ok(config.clone())
    }

    /// Initialize the error interface
    pub fn initialize(&self) -> RuntimeResult<()> {
        // Reset statistics
        let mut stats = self.stats.lock().unwrap();
        *stats = ErrorStats::default();
        Ok(())
    }

    /// Private helper methods

    fn generate_error_id(&self) -> String {
        format!("err_{}", SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis())
    }

    fn determine_error_severity(&self, error: &RuntimeError) -> ErrorSeverity {
        // Determine severity based on error type and content
        let error_str = error.to_string();

        if error_str.contains("致命") || error_str.contains("crash") {
            ErrorSeverity::Fatal
        } else if error_str.contains("严重") || error_str.contains("critical") {
            ErrorSeverity::Critical
        } else if error_str.contains("错误") || error_str.contains("error") {
            ErrorSeverity::Error
        } else if error_str.contains("警告") || error_str.contains("warning") {
            ErrorSeverity::Warning
        } else {
            ErrorSeverity::Info
        }
    }

    fn record_error_stats(&self, context: &ErrorContext) -> RuntimeResult<()> {
        let mut stats = self.stats.lock().unwrap();

        stats.total_errors += 1;
        *stats.errors_by_severity.entry(context.severity.clone()).or_insert(0) += 1;
        *stats.errors_by_type.entry("runtime_error".to_string()).or_insert(0) += 1;
        stats.last_error_timestamp = Some(context.timestamp);

        Ok(())
    }
}

impl Default for ErrorInterface {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_severity() {
        let severity = ErrorSeverity::Error;
        assert_eq!(severity.chinese_name(), "错误");
        assert_eq!(severity.english_name(), "ERROR");
    }

    #[test]
    fn test_error_interface() {
        let interface = ErrorInterface::new();
        assert!(interface.initialize().is_ok());

        let stats = interface.get_error_stats();
        assert!(stats.is_ok());

        let config = interface.get_config();
        assert!(config.is_ok());
    }

    #[test]
    fn test_error_config_default() {
        let config = ErrorConfig::default();
        assert!(config.auto_recovery);
        assert_eq!(config.max_history_size, 1000);
        assert!(config.enable_context_tracking);
        assert_eq!(config.log_level, ErrorSeverity::Warning);
    }
}