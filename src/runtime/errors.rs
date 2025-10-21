//! Runtime error handling for Qi language
//! Qi语言运行时错误处理

use std::fmt;
use crate::lexer::Span;
use crate::utils::diagnostics::{DiagnosticManager, DiagnosticLevel};
use super::RuntimeError;

/// Enhanced runtime error types
/// 增强的运行时错误类型
#[derive(Debug, Clone)]
pub enum EnhancedRuntimeError {
    /// Division by zero
    /// 除零错误
    DivisionByZero {
        message: String,
        span: Option<Span>,
        suggestion: String,
    },

    /// Stack overflow
    /// 栈溢出
    StackOverflow {
        message: String,
        span: Option<Span>,
        suggestion: String,
    },

    /// Out of memory
    /// 内存不足
    OutOfMemory {
        message: String,
        span: Option<Span>,
        suggestion: String,
    },

    /// Null pointer dereference
    /// 空指针解引用
    NullPointerDereference {
        message: String,
        span: Option<Span>,
        suggestion: String,
    },

    /// Integer overflow
    /// 整数溢出
    IntegerOverflow {
        message: String,
        span: Option<Span>,
        suggestion: String,
        operation: String,
    },

    /// Type conversion error
    /// 类型转换错误
    TypeConversion {
        message: String,
        span: Option<Span>,
        suggestion: String,
        from_type: String,
        to_type: String,
    },

    /// Array index out of bounds
    /// 数组索引越界
    ArrayIndexOutOfBounds {
        message: String,
        span: Option<Span>,
        suggestion: String,
        array_size: usize,
        index: isize,
    },

    /// Assertion failure
    /// 断言失败
    AssertionFailure {
        message: String,
        span: Option<Span>,
        suggestion: String,
        condition: String,
    },

    /// File not found
    /// 文件未找到
    FileNotFound {
        message: String,
        span: Option<Span>,
        suggestion: String,
        file_path: String,
    },

    /// Network error
    /// 网络错误
    NetworkError {
        message: String,
        span: Option<Span>,
        suggestion: String,
        url: Option<String>,
    },

    /// Math domain error
    /// 数学域错误
    MathDomainError {
        message: String,
        span: Option<Span>,
        suggestion: String,
        operation: String,
        value: String,
    },

    /// Function return error
    /// 函数返回错误
    FunctionReturnError {
        message: String,
        span: Option<Span>,
        suggestion: String,
        function_name: String,
        expected_type: String,
        actual_type: String,
    },

    /// Missing return value
    /// 缺少返回值
    MissingReturnValue {
        message: String,
        span: Option<Span>,
        suggestion: String,
        function_name: String,
        expected_type: String,
    },

    /// Invalid return value
    /// 无效返回值
    InvalidReturnValue {
        message: String,
        span: Option<Span>,
        suggestion: String,
        function_name: String,
        return_value: String,
    },

    /// Return type mismatch
    /// 返回类型不匹配
    ReturnTypeMismatch {
        message: String,
        span: Option<Span>,
        suggestion: String,
        function_name: String,
        declared_type: String,
        actual_type: String,
    },

    /// Custom error
    /// 自定义错误
    Custom {
        code: String,
        message: String,
        english_message: String,
        span: Option<Span>,
        suggestion: String,
    },
}

impl EnhancedRuntimeError {
    /// Get error code
    /// 获取错误代码
    pub fn code(&self) -> &str {
        match self {
            Self::DivisionByZero { .. } => "R001",
            Self::StackOverflow { .. } => "R002",
            Self::OutOfMemory { .. } => "R003",
            Self::NullPointerDereference { .. } => "R004",
            Self::IntegerOverflow { .. } => "R005",
            Self::TypeConversion { .. } => "R006",
            Self::ArrayIndexOutOfBounds { .. } => "R007",
            Self::AssertionFailure { .. } => "R008",
            Self::FileNotFound { .. } => "R009",
            Self::NetworkError { .. } => "R010",
            Self::MathDomainError { .. } => "R011",
            Self::FunctionReturnError { .. } => "R012",
            Self::MissingReturnValue { .. } => "R013",
            Self::InvalidReturnValue { .. } => "R014",
            Self::ReturnTypeMismatch { .. } => "R015",
            Self::Custom { code, .. } => code,
        }
    }

    /// Get error message
    /// 获取错误消息
    pub fn message(&self) -> &str {
        match self {
            Self::DivisionByZero { message, .. } => message,
            Self::StackOverflow { message, .. } => message,
            Self::OutOfMemory { message, .. } => message,
            Self::NullPointerDereference { message, .. } => message,
            Self::IntegerOverflow { message, .. } => message,
            Self::TypeConversion { message, .. } => message,
            Self::ArrayIndexOutOfBounds { message, .. } => message,
            Self::AssertionFailure { message, .. } => message,
            Self::FileNotFound { message, .. } => message,
            Self::NetworkError { message, .. } => message,
            Self::MathDomainError { message, .. } => message,
            Self::FunctionReturnError { message, .. } => message,
            Self::MissingReturnValue { message, .. } => message,
            Self::InvalidReturnValue { message, .. } => message,
            Self::ReturnTypeMismatch { message, .. } => message,
            Self::Custom { message, .. } => message,
        }
    }

    /// Get suggestion
    /// 获取建议
    pub fn suggestion(&self) -> &str {
        match self {
            Self::DivisionByZero { suggestion, .. } => suggestion,
            Self::StackOverflow { suggestion, .. } => suggestion,
            Self::OutOfMemory { suggestion, .. } => suggestion,
            Self::NullPointerDereference { suggestion, .. } => suggestion,
            Self::IntegerOverflow { suggestion, .. } => suggestion,
            Self::TypeConversion { suggestion, .. } => suggestion,
            Self::ArrayIndexOutOfBounds { suggestion, .. } => suggestion,
            Self::AssertionFailure { suggestion, .. } => suggestion,
            Self::FileNotFound { suggestion, .. } => suggestion,
            Self::NetworkError { suggestion, .. } => suggestion,
            Self::MathDomainError { suggestion, .. } => suggestion,
            Self::FunctionReturnError { suggestion, .. } => suggestion,
            Self::MissingReturnValue { suggestion, .. } => suggestion,
            Self::InvalidReturnValue { suggestion, .. } => suggestion,
            Self::ReturnTypeMismatch { suggestion, .. } => suggestion,
            Self::Custom { suggestion, .. } => suggestion,
        }
    }
}

impl fmt::Display for EnhancedRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for EnhancedRuntimeError {}

/// Enhanced error handling interface
/// 增强的错误处理接口
pub struct ErrorInterface {
    initialized: bool,
    diagnostics: DiagnosticManager,
    errors: Vec<EnhancedRuntimeError>,
    last_error: Option<String>,
    max_errors: usize,
}

impl ErrorInterface {
    /// Create a new error interface
    /// 创建新的错误接口
    pub fn new() -> Self {
        Self {
            initialized: false,
            diagnostics: DiagnosticManager::new(),
            errors: Vec::new(),
            last_error: None,
            max_errors: 100,
        }
    }

    /// Get a reference to the diagnostics manager
    /// 获取诊断管理器的引用
    pub fn diagnostics(&self) -> &DiagnosticManager {
        &self.diagnostics
    }

    /// Get a reference to the errors
    /// 获取错误的引用
    pub fn errors(&self) -> &[EnhancedRuntimeError] {
        &self.errors
    }

    /// Set maximum number of errors to collect
    /// 设置要收集的最大错误数
    pub fn set_max_errors(&mut self, max: usize) {
        self.max_errors = max;
    }

    pub fn initialize(&mut self) -> Result<(), RuntimeError> {
        if self.initialized {
            return Ok(());
        }

        // TODO: Initialize error handling
        self.initialized = true;
        self.last_error = None;
        Ok(())
    }

    pub fn set_error(&mut self, message: &str) -> Result<(), RuntimeError> {
        if !self.initialized {
            return Err(RuntimeError::Panic("错误处理未初始化".to_string()));
        }

        self.last_error = Some(message.to_string());
        Ok(())
    }

    pub fn get_last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn clear_error(&mut self) -> Result<(), RuntimeError> {
        if !self.initialized {
            return Err(RuntimeError::Panic("错误处理未初始化".to_string()));
        }

        self.last_error = None;
        Ok(())
    }

    pub fn panic(&self, message: &str) -> Result<(), RuntimeError> {
        if !self.initialized {
            return Err(RuntimeError::Panic("错误处理未初始化".to_string()));
        }

        // TODO: Implement panic handling
        Err(RuntimeError::Panic(message.to_string()))
    }

    pub fn assert(&self, condition: bool, message: &str) -> Result<(), RuntimeError> {
        if !condition {
            return self.panic(&format!("断言失败: {}", message));
        }
        Ok(())
    }

    pub fn unwrap<T>(&self, value: Option<T>, message: &str) -> Result<T, RuntimeError> {
        match value {
            Some(v) => Ok(v),
            None => {
                self.panic(&format!("unwrap 失败: {}", message))?;
                unreachable!()
            }
        }
    }

    pub fn expect<T>(&self, value: Option<T>, message: &str) -> Result<T, RuntimeError> {
        self.unwrap(value, message)
    }

    // ===== Enhanced Error Reporting Methods | 增强错误报告方法 =====

    /// Report an enhanced runtime error
    /// 报告增强的运行时错误
    pub fn report_error(&mut self, error: EnhancedRuntimeError) -> Result<(), RuntimeError> {
        if !self.initialized {
            return Err(RuntimeError::Panic("错误处理未初始化".to_string()));
        }

        if self.errors.len() >= self.max_errors {
            return Err(RuntimeError::Panic("错误数量超过限制".to_string()));
        }

        // Add to diagnostics
        self.diagnostics.add_diagnostic({
            use crate::utils::diagnostics::Diagnostic;

            Diagnostic {
                level: DiagnosticLevel::错误,
                code: error.code().to_string(),
                message: error.message().to_string(),
                english_message: error.code().to_string(), // Use code as english message for now
                file_path: None,
                span: None, // EnhancedRuntimeError doesn't expose span directly
                suggestion: Some(error.suggestion().to_string()),
                related_code: None,
            }
        });

        self.errors.push(error);
        Ok(())
    }

    /// Report division by zero error
    /// 报告除零错误
    pub fn division_by_zero(&mut self, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::DivisionByZero {
            message: "除零错误".to_string(),
            span,
            suggestion: "检查除数是否为零，如果可能，添加条件检查避免除零".to_string(),
        })
    }

    /// Report stack overflow error
    /// 报告栈溢出错误
    pub fn stack_overflow(&mut self, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::StackOverflow {
            message: "栈溢出".to_string(),
            span,
            suggestion: "检查是否存在无限递归，或者增加栈大小限制".to_string(),
        })
    }

    /// Report out of memory error
    /// 报告内存不足错误
    pub fn out_of_memory(&mut self, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::OutOfMemory {
            message: "内存不足".to_string(),
            span,
            suggestion: "减少内存使用或增加系统内存，检查是否有内存泄漏".to_string(),
        })
    }

    /// Report null pointer dereference error
    /// 报告空指针解引用错误
    pub fn null_pointer_dereference(&mut self, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::NullPointerDereference {
            message: "空指针解引用".to_string(),
            span,
            suggestion: "检查指针是否为空，在使用前进行空指针检查".to_string(),
        })
    }

    /// Report integer overflow error
    /// 报告整数溢出错误
    pub fn integer_overflow(&mut self, operation: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::IntegerOverflow {
            message: format!("整数溢出: {}", operation),
            span,
            suggestion: "使用更大的整数类型或检查数值范围，考虑使用溢出检查".to_string(),
            operation,
        })
    }

    /// Report type conversion error
    /// 报告类型转换错误
    pub fn type_conversion(&mut self, from_type: String, to_type: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::TypeConversion {
            message: format!("类型转换错误: 无法将 '{}' 转换为 '{}'", from_type, to_type),
            span,
            suggestion: "检查类型转换是否合理，使用适当的类型转换函数".to_string(),
            from_type,
            to_type,
        })
    }

    /// Report array index out of bounds error
    /// 报告数组索引越界错误
    pub fn array_index_out_of_bounds(&mut self, array_size: usize, index: isize, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::ArrayIndexOutOfBounds {
            message: format!("数组索引越界: 数组长度 {}, 访问索引 {}", array_size, index),
            span,
            suggestion: "检查数组索引是否在有效范围内，使用边界检查".to_string(),
            array_size,
            index,
        })
    }

    /// Report assertion failure error
    /// 报告断言失败错误
    pub fn assertion_failure(&mut self, condition: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::AssertionFailure {
            message: format!("断言失败: {}", condition),
            span,
            suggestion: "检查断言条件是否正确，确认程序逻辑是否符合预期".to_string(),
            condition,
        })
    }

    /// Report file not found error
    /// 报告文件未找到错误
    pub fn file_not_found(&mut self, file_path: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::FileNotFound {
            message: format!("文件未找到: '{}'", file_path),
            span,
            suggestion: "检查文件路径是否正确，确保文件存在且有读取权限".to_string(),
            file_path,
        })
    }

    /// Report network error
    /// 报告网络错误
    pub fn network_error(&mut self, details: String, url: Option<String>, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::NetworkError {
            message: format!("网络错误: {}", details),
            span,
            suggestion: "检查网络连接，确认URL是否正确，考虑添加重试机制".to_string(),
            url,
        })
    }

    /// Report math domain error
    /// 报告数学域错误
    pub fn math_domain_error(&mut self, operation: String, value: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::MathDomainError {
            message: format!("数学域错误: {}({})", operation, value),
            span,
            suggestion: "检查输入值是否在数学函数的定义域内，考虑添加输入验证".to_string(),
            operation,
            value,
        })
    }

    /// Report custom error
    /// 报告自定义错误
    pub fn custom_error(&mut self, code: String, message: String, suggestion: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::Custom {
            code,
            english_message: message.clone(),
            message,
            span,
            suggestion,
        })
    }

    // ===== Return Value Error Handling Methods | 返回值错误处理方法 =====

    /// Report function return error
    /// 报告函数返回错误
    pub fn function_return_error(&mut self, function_name: String, expected_type: String, actual_type: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::FunctionReturnError {
            message: format!("函数 '{}' 返回错误: 期望 '{}'，实际 '{}'", function_name, expected_type, actual_type),
            span,
            suggestion: format!("检查函数 '{}' 的返回值类型是否与声明的返回类型匹配", function_name),
            function_name,
            expected_type,
            actual_type,
        })
    }

    /// Report missing return value error
    /// 报告缺少返回值错误
    pub fn missing_return_value(&mut self, function_name: String, expected_type: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::MissingReturnValue {
            message: format!("函数 '{}' 缺少返回值，期望返回 '{}'", function_name, expected_type),
            span,
            suggestion: format!("在函数 '{}' 的所有返回路径中添加返回语句，或考虑更改返回类型为 '空'", function_name),
            function_name,
            expected_type,
        })
    }

    /// Report invalid return value error
    /// 报告无效返回值错误
    pub fn invalid_return_value(&mut self, function_name: String, return_value: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::InvalidReturnValue {
            message: format!("函数 '{}' 返回了无效的值: '{}'", function_name, return_value),
            span,
            suggestion: format!("检查函数 '{}' 的返回值是否有效，确保返回值符合函数的语义要求", function_name),
            function_name,
            return_value,
        })
    }

    /// Report return type mismatch error
    /// 报告返回类型不匹配错误
    pub fn return_type_mismatch(&mut self, function_name: String, declared_type: String, actual_type: String, span: Option<Span>) -> Result<(), RuntimeError> {
        self.report_error(EnhancedRuntimeError::ReturnTypeMismatch {
            message: format!("函数 '{}' 返回类型不匹配: 声明为 '{}'，实际返回 '{}'", function_name, declared_type, actual_type),
            span,
            suggestion: format!("修改函数 '{}' 的返回类型声明或确保返回正确的类型", function_name),
            function_name,
            declared_type,
            actual_type,
        })
    }

    /// Validate function return value
    /// 验证函数返回值
    pub fn validate_return_value(&mut self, function_name: &str, expected_type: &str, actual_value: &str, span: Option<Span>) -> Result<(), RuntimeError> {
        // This is a simplified validation - in a real implementation, you'd need
        // proper type checking and value validation
        if actual_value.is_empty() {
            self.missing_return_value(function_name.to_string(), expected_type.to_string(), span)?;
        } else if !self.is_valid_return_type(expected_type, actual_value) {
            self.return_type_mismatch(function_name.to_string(), expected_type.to_string(), self.infer_type(actual_value), span)?;
        }
        Ok(())
    }

    /// Check if a return value type is valid for the expected type
    /// 检查返回值类型是否对期望类型有效
    fn is_valid_return_type(&self, expected_type: &str, actual_value: &str) -> bool {
        // Simplified type validation - in a real implementation, this would be more sophisticated
        match expected_type {
            "整数" => actual_value.parse::<i64>().is_ok(),
            "浮点数" => actual_value.parse::<f64>().is_ok(),
            "布尔" => actual_value == "真" || actual_value == "假" || actual_value.parse::<bool>().is_ok(),
            "字符串" => actual_value.starts_with('"') && actual_value.ends_with('"'),
            "字符" => actual_value.len() == 3 && actual_value.starts_with('\'') && actual_value.ends_with('\''),
            "空" => actual_value == "空" || actual_value.is_empty(),
            _ => true, // Allow unknown types for now
        }
    }

    /// Infer the type of a value (simplified)
    /// 推断值的类型（简化版）
    fn infer_type(&self, value: &str) -> String {
        if value.parse::<i64>().is_ok() {
            "整数".to_string()
        } else if value.parse::<f64>().is_ok() {
            "浮点数".to_string()
        } else if value == "真" || value == "假" || value.parse::<bool>().is_ok() {
            "布尔".to_string()
        } else if value.starts_with('"') && value.ends_with('"') {
            "字符串".to_string()
        } else if value.len() == 3 && value.starts_with('\'') && value.ends_with('\'') {
            "字符".to_string()
        } else if value == "空" || value.is_empty() {
            "空".to_string()
        } else {
            "未知".to_string()
        }
    }

    /// Handle function return with validation
    /// 处理函数返回并验证
    pub fn handle_function_return(&mut self, function_name: &str, return_value: Option<String>, expected_type: Option<String>, span: Option<Span>) -> Result<Option<String>, RuntimeError> {
        match (return_value, expected_type) {
            (Some(value), Some(expected)) => {
                // Validate return value type
                self.validate_return_value(function_name, &expected, &value, span)?;
                Ok(Some(value))
            }
            (Some(value), None) => {
                // No expected type specified, just return the value
                Ok(Some(value))
            }
            (None, Some(expected)) => {
                // Expected a return value but got none
                self.missing_return_value(function_name.to_string(), expected, span)?;
                Ok(None)
            }
            (None, None) => {
                // No return value expected and none provided
                Ok(None)
            }
        }
    }

    /// Get error summary statistics
    /// 获取错误摘要统计
    pub fn get_error_summary(&self) -> (usize, usize) {
        (self.diagnostics.error_count(), self.diagnostics.warning_count())
    }

    /// Check if any critical errors occurred
    /// 检查是否发生了关键错误
    pub fn has_critical_errors(&self) -> bool {
        self.diagnostics.error_count() > 0
    }

    /// Format all diagnostics as Chinese messages
    /// 将所有诊断信息格式化为中文消息
    pub fn format_diagnostics(&self) -> String {
        self.diagnostics.format_chinese_messages()
    }

    /// Clear all errors
    /// 清除所有错误
    pub fn clear_errors(&mut self) -> Result<(), RuntimeError> {
        if !self.initialized {
            return Err(RuntimeError::Panic("错误处理未初始化".to_string()));
        }

        self.errors.clear();
        self.diagnostics.clear();
        self.last_error = None;
        Ok(())
    }

    /// Check if error limit has been reached
    /// 检查是否已达到错误限制
    pub fn is_at_limit(&self) -> bool {
        self.errors.len() >= self.max_errors
    }

    pub fn cleanup(&mut self) -> Result<(), RuntimeError> {
        if self.initialized {
            // Clear all errors and diagnostics
            let _ = self.clear_errors();
            self.initialized = false;
        }
        Ok(())
    }
}

impl Default for ErrorInterface {
    fn default() -> Self {
        Self::new()
    }
}