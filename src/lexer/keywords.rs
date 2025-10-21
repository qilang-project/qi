//! Chinese keyword lookup for Qi language

use crate::lexer::tokens::TokenKind;
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
        keywords.insert("整数".to_string(), TokenKind::整数);
        keywords.insert("字符串".to_string(), TokenKind::字符串);
        keywords.insert("布尔".to_string(), TokenKind::布尔);
        keywords.insert("浮点数".to_string(), TokenKind::浮点数);

        // Additional keywords for grammar
        keywords.insert("导入".to_string(), TokenKind::导入);
        keywords.insert("作为".to_string(), TokenKind::作为);
        keywords.insert("在".to_string(), TokenKind::在);
        keywords.insert("字符".to_string(), TokenKind::字符);
        keywords.insert("空".to_string(), TokenKind::空);
        keywords.insert("与".to_string(), TokenKind::与);
        keywords.insert("或".to_string(), TokenKind::或);
        keywords.insert("参数".to_string(), TokenKind::参数);
        keywords.insert("打印".to_string(), TokenKind::打印);

        // Boolean literals
        keywords.insert("真".to_string(), TokenKind::布尔字面量(true));
        keywords.insert("假".to_string(), TokenKind::布尔字面量(false));

        // Type keywords
        keywords.insert("结构体".to_string(), TokenKind::结构体);
        keywords.insert("枚举".to_string(), TokenKind::枚举);
        keywords.insert("数组".to_string(), TokenKind::数组);

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