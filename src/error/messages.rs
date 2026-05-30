//! 中文错误消息定义
//! Chinese error message definitions

/// 编译器错误代码和消息
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorMessage {
    pub code: &'static str,
    pub message: &'static str,
    pub hint: Option<&'static str>,
}

impl ErrorMessage {
    pub fn format(&self, args: &[&str]) -> String {
        let mut msg = self.message.to_string();
        for (i, arg) in args.iter().enumerate() {
            msg = msg.replace(&format!("{{{}}}", i), arg);
        }
        msg
    }

    pub fn with_hint(&self) -> String {
        if let Some(hint) = self.hint {
            format!("{}\n提示: {}", self.message, hint)
        } else {
            self.message.to_string()
        }
    }
}

// 词法分析错误 - E1xxx
pub const E1001_UNEXPECTED_CHAR: ErrorMessage = ErrorMessage {
    code: "E1001",
    message: "意外的字符 '{0}'",
    hint: Some("请检查是否使用了不支持的符号或特殊字符"),
};

pub const E1002_UNTERMINATED_STRING: ErrorMessage = ErrorMessage {
    code: "E1002",
    message: "未结束的字符串字面量",
    hint: Some("字符串必须以引号结束"),
};

pub const E1003_INVALID_NUMBER: ErrorMessage = ErrorMessage {
    code: "E1003",
    message: "无效的数字格式 '{0}'",
    hint: Some("数字格式示例: 42, 3.14, 0xFF, 0b1010"),
};

// 语法分析错误 - E2xxx
pub const E2001_EXPECTED_TOKEN: ErrorMessage = ErrorMessage {
    code: "E2001",
    message: "期望 '{0}'，但发现 '{1}'",
    hint: Some("请检查语法是否正确"),
};

pub const E2002_UNEXPECTED_TOKEN: ErrorMessage = ErrorMessage {
    code: "E2002",
    message: "意外的标记 '{0}'",
    hint: None,
};

pub const E2003_MISSING_SEMICOLON: ErrorMessage = ErrorMessage {
    code: "E2003",
    message: "缺少分号 ';' 或 '；'",
    hint: Some("中文分号 '；' 和英文分号 ';' 都支持"),
};

pub const E2004_UNCLOSED_DELIMITER: ErrorMessage = ErrorMessage {
    code: "E2004",
    message: "未闭合的 '{0}'",
    hint: Some("支持中英文标点: {} 【】 () （） [] ［］"),
};

// 类型检查错误 - E3xxx
pub const E3001_TYPE_MISMATCH: ErrorMessage = ErrorMessage {
    code: "E3001",
    message: "类型不匹配: 期望 '{0}'，但得到 '{1}'",
    hint: None,
};

pub const E3002_UNDEFINED_VARIABLE: ErrorMessage = ErrorMessage {
    code: "E3002",
    message: "未定义的变量 '{0}'",
    hint: Some("请确保变量已声明"),
};

pub const E3003_UNDEFINED_FUNCTION: ErrorMessage = ErrorMessage {
    code: "E3003",
    message: "未定义的函数 '{0}'",
    hint: Some("请确保函数已声明或已导入"),
};

pub const E3004_DUPLICATE_DEFINITION: ErrorMessage = ErrorMessage {
    code: "E3004",
    message: "重复定义 '{0}'",
    hint: Some("名称在当前作用域中已存在"),
};

pub const E3005_INVALID_OPERATION: ErrorMessage = ErrorMessage {
    code: "E3005",
    message: "无效的操作: 不能对类型 '{0}' 使用运算符 '{1}'",
    hint: None,
};

// 模块和导入错误 - E4xxx
pub const E4001_MODULE_NOT_FOUND: ErrorMessage = ErrorMessage {
    code: "E4001",
    message: "模块 '{0}' 未找到",
    hint: Some("请检查模块路径和包声明是否正确"),
};

pub const E4002_CIRCULAR_DEPENDENCY: ErrorMessage = ErrorMessage {
    code: "E4002",
    message: "检测到循环依赖: {0}",
    hint: Some("模块之间不能形成循环引用"),
};

pub const E4003_IMPORT_NOT_FOUND: ErrorMessage = ErrorMessage {
    code: "E4003",
    message: "无法导入 '{0}': 在模块 '{1}' 中未找到",
    hint: None,
};

pub const E4004_PRIVATE_ACCESS: ErrorMessage = ErrorMessage {
    code: "E4004",
    message: "无法访问私有成员 '{0}'",
    hint: Some("只能访问标记为 '公开' 的成员"),
};

// 内存和指针错误 - E5xxx
pub const E5001_INVALID_DEREFERENCE: ErrorMessage = ErrorMessage {
    code: "E5001",
    message: "无效的解引用操作: '{0}' 不是指针类型",
    hint: Some("只能对指针类型使用 '解引用' 或 '*' 运算符"),
};

pub const E5002_INVALID_ADDRESS_OF: ErrorMessage = ErrorMessage {
    code: "E5002",
    message: "无法获取 '{0}' 的地址",
    hint: Some("只能对变量使用 '取地址' 或 '&' 运算符"),
};

pub const E5003_NULL_POINTER: ErrorMessage = ErrorMessage {
    code: "E5003",
    message: "空指针解引用",
    hint: Some("在解引用前请确保指针不为空"),
};

// 并发错误 - E6xxx
pub const E6001_CHANNEL_CLOSED: ErrorMessage = ErrorMessage {
    code: "E6001",
    message: "通道已关闭",
    hint: Some("无法向已关闭的通道发送数据"),
};

pub const E6002_DEADLOCK: ErrorMessage = ErrorMessage {
    code: "E6002",
    message: "检测到潜在的死锁",
    hint: Some("请检查互斥锁的获取顺序"),
};

// 运行时错误 - E7xxx
pub const E7001_DIVISION_BY_ZERO: ErrorMessage = ErrorMessage {
    code: "E7001",
    message: "除以零错误",
    hint: Some("除数不能为零"),
};

pub const E7002_INDEX_OUT_OF_BOUNDS: ErrorMessage = ErrorMessage {
    code: "E7002",
    message: "索引越界: 索引 {0}，长度 {1}",
    hint: None,
};

pub const E7003_STACK_OVERFLOW: ErrorMessage = ErrorMessage {
    code: "E7003",
    message: "栈溢出",
    hint: Some("可能是递归过深或栈空间不足"),
};

// IO 错误 - E8xxx
pub const E8001_FILE_NOT_FOUND: ErrorMessage = ErrorMessage {
    code: "E8001",
    message: "文件未找到: '{0}'",
    hint: None,
};

pub const E8002_PERMISSION_DENIED: ErrorMessage = ErrorMessage {
    code: "E8002",
    message: "权限被拒绝: '{0}'",
    hint: Some("请检查文件或目录的访问权限"),
};

pub const E8003_NETWORK_ERROR: ErrorMessage = ErrorMessage {
    code: "E8003",
    message: "网络错误: {0}",
    hint: Some("请检查网络连接"),
};

/// 错误消息工具函数
pub mod utils {
    use super::*;

    /// 格式化带错误代码的完整错误信息
    pub fn format_error(err_msg: &ErrorMessage, args: &[&str]) -> String {
        format!("错误 [{}]: {}", err_msg.code, err_msg.format(args))
    }

    /// 格式化带错误代码和提示的完整错误信息
    pub fn format_error_with_hint(err_msg: &ErrorMessage, args: &[&str]) -> String {
        let msg = format_error(err_msg, args);
        if let Some(hint) = err_msg.hint {
            format!("{}\n提示: {}", msg, hint)
        } else {
            msg
        }
    }

    /// 根据错误代码获取错误消息
    pub fn get_error_by_code(code: &str) -> Option<&'static ErrorMessage> {
        match code {
            "E1001" => Some(&E1001_UNEXPECTED_CHAR),
            "E1002" => Some(&E1002_UNTERMINATED_STRING),
            "E1003" => Some(&E1003_INVALID_NUMBER),
            "E2001" => Some(&E2001_EXPECTED_TOKEN),
            "E2002" => Some(&E2002_UNEXPECTED_TOKEN),
            "E2003" => Some(&E2003_MISSING_SEMICOLON),
            "E2004" => Some(&E2004_UNCLOSED_DELIMITER),
            "E3001" => Some(&E3001_TYPE_MISMATCH),
            "E3002" => Some(&E3002_UNDEFINED_VARIABLE),
            "E3003" => Some(&E3003_UNDEFINED_FUNCTION),
            "E3004" => Some(&E3004_DUPLICATE_DEFINITION),
            "E3005" => Some(&E3005_INVALID_OPERATION),
            "E4001" => Some(&E4001_MODULE_NOT_FOUND),
            "E4002" => Some(&E4002_CIRCULAR_DEPENDENCY),
            "E4003" => Some(&E4003_IMPORT_NOT_FOUND),
            "E4004" => Some(&E4004_PRIVATE_ACCESS),
            "E5001" => Some(&E5001_INVALID_DEREFERENCE),
            "E5002" => Some(&E5002_INVALID_ADDRESS_OF),
            "E5003" => Some(&E5003_NULL_POINTER),
            "E6001" => Some(&E6001_CHANNEL_CLOSED),
            "E6002" => Some(&E6002_DEADLOCK),
            "E7001" => Some(&E7001_DIVISION_BY_ZERO),
            "E7002" => Some(&E7002_INDEX_OUT_OF_BOUNDS),
            "E7003" => Some(&E7003_STACK_OVERFLOW),
            "E8001" => Some(&E8001_FILE_NOT_FOUND),
            "E8002" => Some(&E8002_PERMISSION_DENIED),
            "E8003" => Some(&E8003_NETWORK_ERROR),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_message_format() {
        let msg = E3001_TYPE_MISMATCH.format(&["整数", "字符串"]);
        assert_eq!(msg, "类型不匹配: 期望 '整数'，但得到 '字符串'");
    }

    #[test]
    fn test_error_with_hint() {
        let msg = E2003_MISSING_SEMICOLON.with_hint();
        assert!(msg.contains("提示:"));
    }

    #[test]
    fn test_get_error_by_code() {
        let err = utils::get_error_by_code("E1001");
        assert!(err.is_some());
        assert_eq!(err.unwrap().code, "E1001");
    }
}
