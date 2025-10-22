//! Error Interface Module
//!
//! This module provides a unified interface for error handling, error recovery,
//! and error statistics with comprehensive Chinese language support.

use std::sync::Arc;
use std::sync::Mutex;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime::{RuntimeResult, RuntimeError};
use super::{LegacyError, ErrorSeverity, ErrorStatistics, StackFrame};

/// Unified error interface that provides access to all error handling functionality
#[derive(Debug)]
pub struct ErrorInterface {
    /// Error handler
    handler: Arc<Mutex<ErrorHandler>>,
    /// Error statistics
    stats: Arc<Mutex<ErrorStats>>,
    /// Error context cache
    context_cache: Arc<Mutex<HashMap<String, ErrorContext>>>,
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
    /// Enable crash protection
    pub crash_protection: bool,
    /// Maximum recovery attempts
    pub max_recovery_attempts: u32,
    /// Enable error statistics
    pub enable_statistics: bool,
    /// Error reporting channels
    pub reporting_channels: Vec<String>,
}

impl Default for ErrorConfig {
    fn default() -> Self {
        Self {
            auto_recovery: true,
            max_history_size: 1000,
            enable_context_tracking: true,
            log_level: ErrorSeverity::Warning,
            crash_protection: true,
            max_recovery_attempts: 3,
            enable_statistics: true,
            reporting_channels: vec!["console".to_string(), "file".to_string()],
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
    /// Unrecoverable errors
    pub unrecoverable_errors: u64,
    /// Errors in last hour
    pub errors_last_hour: u64,
    /// Peak error rate
    pub peak_error_rate: f64,
    /// Average recovery time
    pub avg_recovery_time_ms: u64,
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
    /// Parent error ID (for error chains)
    pub parent_id: Option<String>,
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
    /// Expected success rate
    pub success_rate: f64,
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
    /// Custom action
    Custom(String),
}

/// Error handler for processing and recovering from errors
#[derive(Debug)]
pub struct ErrorHandler {
    /// Error history
    error_history: Vec<ErrorContext>,
    /// Recovery strategies
    recovery_strategies: HashMap<String, RecoveryStrategy>,
    /// Current recovery state
    recovery_state: RecoveryState,
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
    /// Cooldown period between attempts
    pub cooldown_ms: u64,
}

/// Current recovery state
#[derive(Debug, Clone, Default)]
pub struct RecoveryState {
    /// Currently recovering from error
    pub is_recovering: bool,
    /// Current error being recovered
    pub current_error: Option<String>,
    /// Recovery attempts made
    pub attempts: u32,
    /// Last recovery attempt timestamp
    pub last_attempt: Option<u64>,
}

impl ErrorInterface {
    /// Create new error interface
    pub fn new() -> Self {
        let config = ErrorConfig::default();

        Self {
            handler: Arc::new(Mutex::new(ErrorHandler::new())),
            stats: Arc::new(Mutex::new(ErrorStats::default())),
            context_cache: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(config)),
        }
    }

    /// Create error interface with custom configuration
    pub fn with_config(config: ErrorConfig) -> Self {
        Self {
            handler: Arc::new(Mutex::new(ErrorHandler::new())),
            stats: Arc::new(Mutex::new(ErrorStats::default())),
            context_cache: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(config)),
        }
    }

    /// Handle an error
    pub fn handle_error(&self, error: &RuntimeError) -> RuntimeResult<ErrorContext> {
        let context = self.create_error_context(error)?;

        // Record error statistics
        self.record_error_stats(&context)?;

        // Attempt recovery if enabled
        let config = self.config.lock().unwrap();
        if config.auto_recovery {
            drop(config);
            let recovery_result = self.attempt_recovery(&context)?;
            if recovery_result {
                self.record_recovery(&context)?;
            }
        }

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
            parent_id: None,
            recovery_attempts: 0,
        };

        Ok(context)
    }

    /// Attempt to recover from error
    pub fn attempt_recovery(&self, context: &ErrorContext) -> RuntimeResult<bool> {
        let mut handler = self.handler.lock().unwrap();
        let config = self.config.lock().unwrap();

        if context.recovery_attempts >= config.max_recovery_attempts {
            return Ok(false);
        }

        // Try each recovery option
        for option in &context.recovery_options {
            let success = self.execute_recovery_action(&option.action)?;
            if success {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Get error statistics
    pub fn get_error_stats(&self) -> RuntimeResult<ErrorStats> {
        let stats = self.stats.lock().unwrap();
        Ok(stats.clone())
    }

    /// Get error history
    pub fn get_error_history(&self) -> RuntimeResult<Vec<ErrorContext>> {
        let handler = self.handler.lock().unwrap();
        Ok(handler.error_history.clone())
    }

    /// Clear error history
    pub fn clear_history(&self) -> RuntimeResult<()> {
        let mut handler = self.handler.lock().unwrap();
        handler.error_history.clear();
        Ok(())
    }

    /// Get configuration
    pub fn get_config(&self) -> RuntimeResult<ErrorConfig> {
        let config = self.config.lock().unwrap();
        Ok(config.clone())
    }

    /// Update configuration
    pub fn update_config(&self, config: ErrorConfig) -> RuntimeResult<()> {
        *self.config.lock().unwrap() = config;
        Ok(())
    }

    /// Set error logging level
    pub fn set_log_level(&self, level: ErrorSeverity) -> RuntimeResult<()> {
        self.config.lock().unwrap().log_level = level;
        Ok(())
    }

    /// Enable/disable automatic recovery
    pub fn set_auto_recovery(&self, enabled: bool) -> RuntimeResult<()> {
        self.config.lock().unwrap().auto_recovery = enabled;
        Ok(())
    }

    /// Check if crash protection is enabled
    pub fn is_crash_protection_enabled(&self) -> bool {
        self.config.lock().unwrap().crash_protection
    }

    /// Get current recovery state
    pub fn get_recovery_state(&self) -> RuntimeResult<RecoveryState> {
        let handler = self.handler.lock().unwrap();
        Ok(handler.recovery_state.clone())
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
            success_rate: 0.7,
        });

        // Add context-specific options
        let error_str = error.to_string();
        if error_str.contains("文件") || error_str.contains("file") {
            options.push(RecoveryOption {
                name: "使用备用文件".to_string(),
                description: "使用备用文件路径".to_string(),
                action: RecoveryAction::Fallback("backup_file".to_string()),
                success_rate: 0.5,
            });
        }

        if error_str.contains("网络") || error_str.contains("network") {
            options.push(RecoveryOption {
                name: "切换到离线模式".to_string(),
                description: "切换到离线模式继续运行".to_string(),
                action: RecoveryAction::Fallback("offline_mode".to_string()),
                success_rate: 0.8,
            });
        }

        Ok(options)
    }

    fn execute_recovery_action(&self, action: &RecoveryAction) -> RuntimeResult<bool> {
        match action {
            RecoveryAction::Retry => {
                // Simulate retry logic
                Ok(true) // In real implementation, would retry the operation
            }
            RecoveryAction::Reset => {
                // Simulate reset logic
                Ok(true)
            }
            RecoveryAction::Fallback(_) => {
                // Simulate fallback logic
                Ok(true)
            }
            RecoveryAction::Skip => {
                // Simulate skip logic
                Ok(true)
            }
            RecoveryAction::UserIntervention => {
                // Need user intervention
                Ok(false)
            }
            RecoveryAction::Custom(_) => {
                // Custom action - placeholder
                Ok(false)
            }
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

    fn record_recovery(&self, context: &ErrorContext) -> RuntimeResult<()> {
        let mut stats = self.stats.lock().unwrap();
        stats.recovered_errors += 1;
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
            recovery_state: RecoveryState::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_interface_creation() {
        let error_interface = ErrorInterface::new();
        let config = error_interface.get_config().unwrap();
        assert!(config.auto_recovery);
        assert!(config.enable_context_tracking);
    }

    #[test]
    fn test_error_context_creation() {
        let error_interface = ErrorInterface::new();
        let error = RuntimeError::internal_error("测试错误", "这是一个测试错误");

        let context = error_interface.create_error_context(&error).unwrap();
        assert!(!context.id.is_empty());
        assert_eq!(context.message, error.to_string());
        assert!(!context.recovery_options.is_empty());
    }

    #[test]
    fn test_error_statistics() {
        let error_interface = ErrorInterface::new();
        let error = RuntimeError::internal_error("测试错误", "这是一个测试错误");

        // Handle an error
        error_interface.handle_error(&error).unwrap();

        let stats = error_interface.get_error_stats().unwrap();
        assert_eq!(stats.total_errors, 1);
        assert!(stats.last_error_timestamp.is_some());
    }

    #[test]
    fn test_recovery_options() {
        let error_interface = ErrorInterface::new();
        let error = RuntimeError::internal_error("文件错误", "无法读取文件");

        let context = error_interface.create_error_context(&error).unwrap();

        // Should have at least retry option
        assert!(!context.recovery_options.is_empty());
        let has_retry = context.recovery_options.iter()
            .any(|opt| matches!(opt.action, RecoveryAction::Retry));
        assert!(has_retry);
    }

    #[test]
    fn test_configuration() {
        let error_interface = ErrorInterface::new();

        // Test setting auto recovery
        error_interface.set_auto_recovery(false).unwrap();
        let config = error_interface.get_config().unwrap();
        assert!(!config.auto_recovery);

        // Test setting log level
        error_interface.set_log_level(ErrorSeverity::Error).unwrap();
        let config = error_interface.get_config().unwrap();
        assert_eq!(config.log_level, ErrorSeverity::Error);
    }
}