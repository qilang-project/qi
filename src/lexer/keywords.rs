//! Chinese keyword lookup for Qi language

use crate::lexer::tokens::TokenKind;
use crate::parser::ast::BasicType;
use std::collections::HashMap;

/// Chinese keyword lookup table
pub struct KeywordTable {
    keywords: HashMap<String, TokenKind>,
}

impl KeywordTable {
    pub fn new() -> Self {
        let mut keywords = HashMap::new();

        // Populate Chinese keywords
        keywords.insert("如果".to_string(), TokenKind::如果);
        keywords.insert("否则".to_string(), TokenKind::否则);
        keywords.insert("循环".to_string(), TokenKind::循环);
        keywords.insert("当".to_string(), TokenKind::当);
        keywords.insert("对于".to_string(), TokenKind::对于);
        keywords.insert("函数".to_string(), TokenKind::函数);
        keywords.insert("返回".to_string(), TokenKind::返回);
        keywords.insert("变量".to_string(), TokenKind::变量);
        keywords.insert("常量".to_string(), TokenKind::常量);
        keywords.insert("整数".to_string(), TokenKind::类型关键词(BasicType::整数));
        keywords.insert("字符串".to_string(), TokenKind::类型关键词(BasicType::字符串));
        keywords.insert("布尔".to_string(), TokenKind::类型关键词(BasicType::布尔));
        keywords.insert("浮点数".to_string(), TokenKind::类型关键词(BasicType::浮点数));

        // Additional keywords for grammar
        keywords.insert("导入".to_string(), TokenKind::导入);
        keywords.insert("导出".to_string(), TokenKind::导出);
        keywords.insert("作为".to_string(), TokenKind::作为);
        keywords.insert("在".to_string(), TokenKind::在);
        keywords.insert("字符".to_string(), TokenKind::类型关键词(BasicType::字符));
        keywords.insert("空".to_string(), TokenKind::类型关键词(BasicType::空));
        keywords.insert("与".to_string(), TokenKind::与);
        keywords.insert("或".to_string(), TokenKind::或);
        keywords.insert("参数".to_string(), TokenKind::参数);
        keywords.insert("包".to_string(), TokenKind::包);
        keywords.insert("模块".to_string(), TokenKind::模块);
        keywords.insert("公开".to_string(), TokenKind::公开);
        keywords.insert("私有".to_string(), TokenKind::私有);

        // Boolean literals
        keywords.insert("真".to_string(), TokenKind::布尔字面量(true));
        keywords.insert("假".to_string(), TokenKind::布尔字面量(false));

        // Type keywords - 基础类型
        keywords.insert("长整数".to_string(), TokenKind::类型关键词(BasicType::长整数));
        keywords.insert("短整数".to_string(), TokenKind::类型关键词(BasicType::短整数));
        keywords.insert("字节".to_string(), TokenKind::类型关键词(BasicType::字节));

        // Type keywords - 容器类型
        keywords.insert("字典".to_string(), TokenKind::类型关键词(BasicType::字典));
        keywords.insert("列表".to_string(), TokenKind::类型关键词(BasicType::列表));
        keywords.insert("集合".to_string(), TokenKind::类型关键词(BasicType::集合));

        // Type keywords - 指针和引用类型
        keywords.insert("指针".to_string(), TokenKind::类型关键词(BasicType::指针));
        keywords.insert("引用".to_string(), TokenKind::类型关键词(BasicType::引用));
        keywords.insert("可变引用".to_string(), TokenKind::类型关键词(BasicType::可变引用));

        // Type keywords - 复合类型
        keywords.insert("结构体".to_string(), TokenKind::结构体);
        keywords.insert("枚举".to_string(), TokenKind::枚举);
        keywords.insert("数组".to_string(), TokenKind::数组);
        keywords.insert("方法".to_string(), TokenKind::方法);
        keywords.insert("自己".to_string(), TokenKind::自己);

        // Minimal English keywords for debugging/testing only
        keywords.insert("let".to_string(), TokenKind::变量);
        keywords.insert("print".to_string(), TokenKind::标识符);
        keywords.insert("true".to_string(), TokenKind::布尔字面量(true));
        keywords.insert("false".to_string(), TokenKind::布尔字面量(false));

        Self { keywords }
    }

    /// Check if a string is a keyword and return the corresponding token kind
    pub fn lookup(&self, text: &str) -> Option<TokenKind> {
        self.keywords.get(text).cloned()
    }

    /// Check if a string is a keyword
    pub fn is_keyword(&self, text: &str) -> bool {
        self.keywords.contains_key(text)
    }
}

impl Default for KeywordTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Global keyword lookup table
pub static KEYWORDS: once_cell::sync::Lazy<KeywordTable> = once_cell::sync::Lazy::new(KeywordTable::new);