//! 错误处理模块
//! Error handling module with Chinese error messages

pub mod messages;

use std::fmt;

/// Qi 编译器错误类型
#[derive(Debug, Clone)]
pub struct QiError {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
    pub location: Option<ErrorLocation>,
}

/// 错误位置信息
#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for QiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "错误 [{}]: {}", self.code, self.message)?;

        if let Some(loc) = &self.location {
            write!(f, "\n位置: {}:{}:{}", loc.file, loc.line, loc.column)?;
        }

        if let Some(hint) = &self.hint {
            write!(f, "\n提示: {}", hint)?;
        }

        Ok(())
    }
}

impl std::error::Error for QiError {}

/// 错误构建器
pub struct QiErrorBuilder {
    error: QiError,
}

impl QiErrorBuilder {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: QiError {
                code: code.into(),
                message: message.into(),
                hint: None,
                location: None,
            },
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.error.hint = Some(hint.into());
        self
    }

    pub fn location(mut self, file: impl Into<String>, line: usize, column: usize) -> Self {
        self.error.location = Some(ErrorLocation {
            file: file.into(),
            line,
            column,
        });
        self
    }

    pub fn build(self) -> QiError {
        self.error
    }
}

/// 便捷宏：创建错误
#[macro_export]
macro_rules! qi_error {
    ($err_msg:expr) => {
        $crate::error::QiError {
            code: $err_msg.code.to_string(),
            message: $err_msg.message.to_string(),
            hint: $err_msg.hint.map(|h| h.to_string()),
            location: None,
        }
    };
    ($err_msg:expr, $($arg:expr),*) => {
        $crate::error::QiError {
            code: $err_msg.code.to_string(),
            message: $err_msg.format(&[$($arg),*]),
            hint: $err_msg.hint.map(|h| h.to_string()),
            location: None,
        }
    };
}

/// Result type for Qi operations
pub type QiResult<T> = Result<T, QiError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::messages::*;

    #[test]
    fn test_error_display() {
        let err = QiErrorBuilder::new("E1001", "测试错误")
            .hint("这是一个测试")
            .location("test.qi", 10, 5)
            .build();

        let display = format!("{}", err);
        assert!(display.contains("错误 [E1001]"));
        assert!(display.contains("测试错误"));
        assert!(display.contains("提示:"));
        assert!(display.contains("位置:"));
    }

    #[test]
    fn test_qi_error_macro() {
        let err = qi_error!(E3001_TYPE_MISMATCH, "整数", "字符串");
        assert_eq!(err.code, "E3001");
        assert!(err.message.contains("整数"));
        assert!(err.message.contains("字符串"));
    }
}
