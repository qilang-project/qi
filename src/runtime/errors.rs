//! Error Handling Module
//!
//! This module provides comprehensive error handling for the Qi runtime with both
//! legacy support and modern interface implementations.

use std::sync::Arc;
use std::sync::Mutex;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::runtime::{RuntimeResult, RuntimeError};

/// Legacy error types (deprecated)
#[derive(Debug, Clone)]
#[deprecated(note = "Use runtime::error::Error instead")]
pub enum LegacyError {
    /// Memory allocation error
    MemoryAllocation(String),
    /// String operation error
    StringOperation(String),
    /// Runtime panic
    Panic(String),
    /// I/O error
    IoError(String),
    /// Network error
    NetworkError(String),
    /// System error
    SystemError(String),
    /// Validation error
    ValidationError(String),
    /// Security error
    SecurityError(String),
    /// Initialization error
    InitializationError(String),
    /// Execution error
    ExecutionError(String),
}

impl LegacyError {
    /// Get error message
    pub fn message(&self) -> &str {
        match self {
            LegacyError::MemoryAllocation(msg) => msg,
            LegacyError::StringOperation(msg) => msg,
            LegacyError::Panic(msg) => msg,
            LegacyError::IoError(msg) => msg,
            LegacyError::NetworkError(msg) => msg,
            LegacyError::SystemError(msg) => msg,
            LegacyError::ValidationError(msg) => msg,
            LegacyError::SecurityError(msg) => msg,
            LegacyError::InitializationError(msg) => msg,
            LegacyError::ExecutionError(msg) => msg,
        }
    }

    /// Get Chinese error message
    pub fn chinese_message(&self) -> &str {
        match self {
            LegacyError::MemoryAllocation(_) => "内存分配错误",
            LegacyError::StringOperation(_) => "字符串操作错误",
            LegacyError::Panic(_) => "运行时错误",
            LegacyError::IoError(_) => "输入输出错误",
            LegacyError::NetworkError(_) => "网络错误",
            LegacyError::SystemError(_) => "系统错误",
            LegacyError::ValidationError(_) => "验证错误",
            LegacyError::SecurityError(_) => "安全错误",
            LegacyError::InitializationError(_) => "初始化错误",
            LegacyError::ExecutionError(_) => "执行错误",
        }
    }

    /// Convert to RuntimeError
    pub fn to_runtime_error(&self) -> RuntimeError {
        let chinese_msg = self.chinese_message().to_string();
        match self {
            LegacyError::MemoryAllocation(msg) => RuntimeError::memory_error(msg, &chinese_msg),
            LegacyError::StringOperation(msg) => RuntimeError::internal_error(msg, &chinese_msg),
            LegacyError::Panic(msg) => RuntimeError::internal_error(msg, &chinese_msg),
            LegacyError::IoError(msg) => RuntimeError::io_error(msg, &chinese_msg),
            LegacyError::NetworkError(msg) => RuntimeError::network_error(msg, &chinese_msg),
            LegacyError::SystemError(msg) => RuntimeError::system_error(msg, &chinese_msg),
            LegacyError::ValidationError(msg) => RuntimeError::validation_error(msg, &chinese_msg),
            LegacyError::SecurityError(msg) => RuntimeError::security_error(msg, &chinese_msg),
            LegacyError::InitializationError(msg) => RuntimeError::initialization_failed(msg, &chinese_msg),
            LegacyError::ExecutionError(msg) => RuntimeError::program_execution_error(msg, &chinese_msg),
        }
    }
}

impl std::fmt::Display for LegacyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.chinese_message(), self.message())
    }
}

impl std::error::Error for LegacyError {}

/// Legacy error handler (deprecated)
#[derive(Debug)]
#[deprecated(note = "Use runtime::error::ErrorHandler instead")]
pub struct LegacyErrorHandler {
    error_count: usize,
    max_errors: usize,
}

impl LegacyErrorHandler {
    /// Create new legacy error handler
    pub fn new() -> Self {
        Self {
            error_count: 0,
            max_errors: 100,
        }
    }

    /// Create legacy error handler with max errors
    pub fn with_max_errors(max_errors: usize) -> Self {
        Self {
            error_count: 0,
            max_errors,
        }
    }

    /// Handle an error
    pub fn handle_error(&mut self, error: LegacyError) -> RuntimeResult<()> {
        self.error_count += 1;

        if self.error_count >= self.max_errors {
            return Err(error.to_runtime_error());
        }

        // Log the error
        eprintln!("错误 #{}: {}", self.error_count, error);

        Ok(())
    }

    /// Get error count
    pub fn error_count(&self) -> usize {
        self.error_count
    }

    /// Reset error count
    pub fn reset(&mut self) {
        self.error_count = 0;
    }

    /// Check if error limit is reached
    pub fn is_limit_reached(&self) -> bool {
        self.error_count >= self.max_errors
    }

    /// Get max errors
    pub fn max_errors(&self) -> usize {
        self.max_errors
    }

    /// Set max errors
    pub fn set_max_errors(&mut self, max_errors: usize) {
        self.max_errors = max_errors;
    }
}

impl Default for LegacyErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Legacy error utilities (deprecated)
#[derive(Debug)]
#[deprecated(note = "Use runtime::error module utilities instead")]
pub struct LegacyErrorUtils;

impl LegacyErrorUtils {
    /// Create memory allocation error
    pub fn memory_allocation_error(message: &str) -> LegacyError {
        LegacyError::MemoryAllocation(message.to_string())
    }

    /// Create string operation error
    pub fn string_operation_error(message: &str) -> LegacyError {
        LegacyError::StringOperation(message.to_string())
    }

    /// Create panic error
    pub fn panic_error(message: &str) -> LegacyError {
        LegacyError::Panic(message.to_string())
    }

    /// Create I/O error
    pub fn io_error(message: &str) -> LegacyError {
        LegacyError::IoError(message.to_string())
    }

    /// Create network error
    pub fn network_error(message: &str) -> LegacyError {
        LegacyError::NetworkError(message.to_string())
    }

    /// Create system error
    pub fn system_error(message: &str) -> LegacyError {
        LegacyError::SystemError(message.to_string())
    }

    /// Create validation error
    pub fn validation_error(message: &str) -> LegacyError {
        LegacyError::ValidationError(message.to_string())
    }

    /// Create security error
    pub fn security_error(message: &str) -> LegacyError {
        LegacyError::SecurityError(message.to_string())
    }

    /// Create initialization error
    pub fn initialization_error(message: &str) -> LegacyError {
        LegacyError::InitializationError(message.to_string())
    }

    /// Create execution error
    pub fn execution_error(message: &str) -> LegacyError {
        LegacyError::ExecutionError(message.to_string())
    }

    /// Format error with context
    pub fn format_error_with_context(error: &LegacyError, context: &str) -> String {
        format!("{} (上下文: {})", error, context)
    }

    /// Get error severity
    pub fn get_error_severity(error: &LegacyError) -> ErrorSeverity {
        match error {
            LegacyError::MemoryAllocation(_) => ErrorSeverity::Critical,
            LegacyError::SecurityError(_) => ErrorSeverity::Critical,
            LegacyError::InitializationError(_) => ErrorSeverity::Fatal,
            LegacyError::ExecutionError(_) => ErrorSeverity::Error,
            LegacyError::IoError(_) => ErrorSeverity::Error,
            LegacyError::NetworkError(_) => ErrorSeverity::Warning,
            LegacyError::SystemError(_) => ErrorSeverity::Error,
            LegacyError::ValidationError(_) => ErrorSeverity::Warning,
            LegacyError::StringOperation(_) => ErrorSeverity::Info,
            LegacyError::Panic(_) => ErrorSeverity::Fatal,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_error_creation() {
        let memory_error = LegacyError::MemoryAllocation("Out of memory".to_string());
        assert_eq!(memory_error.message(), "Out of memory");
        assert_eq!(memory_error.chinese_message(), "内存分配错误");

        let string_error = LegacyErrorUtils::string_operation_error("Invalid format");
        assert_eq!(string_error.message(), "Invalid format");
        assert_eq!(string_error.chinese_message(), "字符串操作错误");
    }

    #[test]
    fn test_legacy_error_handler() {
        let mut handler = LegacyErrorHandler::new();
        assert_eq!(handler.error_count(), 0);
        assert!(!handler.is_limit_reached());

        let result = handler.handle_error(LegacyError::ValidationError("Test error".to_string()));
        assert!(result.is_ok());
        assert_eq!(handler.error_count(), 1);

        handler.reset();
        assert_eq!(handler.error_count(), 0);
    }

    #[test]
    fn test_legacy_error_handler_with_limit() {
        let mut handler = LegacyErrorHandler::with_max_errors(2);

        let result1 = handler.handle_error(LegacyError::ValidationError("Test 1".to_string()));
        assert!(result1.is_ok());

        let result2 = handler.handle_error(LegacyError::ValidationError("Test 2".to_string()));
        assert!(result2.is_err()); // Should fail after reaching limit
        assert!(handler.is_limit_reached());
    }

    #[test]
    fn test_error_severity() {
        let memory_error = LegacyError::MemoryAllocation("test".to_string());
        let severity = LegacyErrorUtils::get_error_severity(&memory_error);
        assert_eq!(severity, ErrorSeverity::Critical);
        assert_eq!(severity.chinese_name(), "严重");

        let string_error = LegacyError::StringOperation("test".to_string());
        let severity = LegacyErrorUtils::get_error_severity(&string_error);
        assert_eq!(severity, ErrorSeverity::Info);
        assert_eq!(severity.english_name(), "INFO");
    }

    #[test]
    fn test_error_formatting() {
        let error = LegacyError::IoError("File not found".to_string());
        let formatted = LegacyErrorUtils::format_error_with_context(&error, "during read operation");
        assert!(formatted.contains("File not found"));
        assert!(formatted.contains("during read operation"));
    }

    #[test]
    fn test_error_conversion() {
        let legacy_error = LegacyError::MemoryAllocation("Out of memory".to_string());
        let runtime_error = legacy_error.to_runtime_error();

        // The conversion should work without panicking
        // We can't easily test the exact type conversion due to the opaque Error type
        // but we can at least ensure it doesn't panic
        let _ = runtime_error;
    }

    #[test]
    fn test_error_display() {
        let error = LegacyError::ValidationError("Invalid input".to_string());
        let display_str = format!("{}", error);
        assert!(display_str.contains("验证错误"));
        assert!(display_str.contains("Invalid input"));
    }
}

/// Error severity levels (public)
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

/// Unified error interface that provides access to all error handling functionality
#[derive(Debug)]
pub struct ErrorInterface {
    /// Error handler
    handler: Arc<Mutex<ErrorHandler>>,
    /// Error statistics
    stats: Arc<Mutex<ErrorStats>>,
    /// Configuration
    config: Arc<Mutex<ErrorConfig>>,
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
    /// Recovery options
    pub recovery_options: Vec<RecoveryOption>,
    /// Context metadata
    pub metadata: HashMap<String, String>,
    /// Recovery attempts
    pub recovery_attempts: u32,
}

/// Error recovery options
#[derive(Debug, Clone)]
pub struct RecoveryOption {
    /// Option name
    pub name: String,
    /// Option description
    pub description: String,
    /// Recovery action
    pub action: RecoveryAction,
}

/// Recovery action types
#[derive(Debug, Clone)]
pub enum RecoveryAction {
    /// Retry the operation
    Retry,
    /// Reset to safe state
    Reset,
    /// Use fallback value
    Fallback(String),
    /// Skip operation
    Skip,
    /// Request user intervention
    UserIntervention,
}

/// Error handler for processing and recovering from errors
#[derive(Debug)]
pub struct ErrorHandler {
    /// Error history
    error_history: Vec<ErrorContext>,
    /// Recovery strategies
    recovery_strategies: HashMap<String, RecoveryStrategy>,
}

/// Recovery strategy definition
#[derive(Debug, Clone)]
pub struct RecoveryStrategy {
    /// Strategy name
    pub name: String,
    /// Error types this strategy handles
    pub handles: Vec<String>,
    /// Recovery action
    pub action: RecoveryAction,
    /// Maximum attempts
    pub max_attempts: u32,
}

impl ErrorInterface {
    /// Create new error interface
    pub fn new() -> Self {
        let config = ErrorConfig::default();

        Self {
            handler: Arc::new(Mutex::new(ErrorHandler::new())),
            stats: Arc::new(Mutex::new(ErrorStats::default())),
            config: Arc::new(Mutex::new(config)),
        }
    }

    /// Create error interface with custom configuration
    pub fn with_config(config: ErrorConfig) -> Self {
        Self {
            handler: Arc::new(Mutex::new(ErrorHandler::new())),
            stats: Arc::new(Mutex::new(ErrorStats::default())),
            config: Arc::new(Mutex::new(config)),
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
        let recovery_options = self.generate_recovery_options(error)?;

        let context = ErrorContext {
            id: self.generate_error_id(),
            message: error.to_string(),
            severity,
            stack_frames: vec![], // Would be populated with actual stack frames
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            recovery_options,
            metadata: HashMap::new(),
            recovery_attempts: 0,
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

    fn generate_recovery_options(&self, error: &RuntimeError) -> RuntimeResult<Vec<RecoveryOption>> {
        let mut options = Vec::new();

        // Add generic retry option
        options.push(RecoveryOption {
            name: "重试".to_string(),
            description: "重试失败的操作".to_string(),
            action: RecoveryAction::Retry,
        });

        Ok(options)
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

impl ErrorHandler {
    /// Create new error handler
    pub fn new() -> Self {
        Self {
            error_history: Vec::new(),
            recovery_strategies: HashMap::new(),
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