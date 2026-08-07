//! 中文关键字表（诊断/工具用）。
//!
//! ⚠ **这张表不是语法事实源。** 真正的保留字是 `parser/grammar.lalrpop` 里的
//! 字面量终结符 —— 解析走 `Parser::parse_source`（LALRPOP 内建词法器），
//! 本表的分类结果只影响手写 lexer 的 TokenKind（报错提示、工具链）。
//! 2026-08 精简时两表曾漂移出十几个词（本表有而语法没有、反之亦然），
//! 现在由 `tests/关键字表一致性.rs` 强制：本表词集 ⊆ 语法字面量集。
//! 往语法里加保留字前先想清楚 —— 每个保留字都从用户手里偷走一个标识符。

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

        // 控制流
        keywords.insert("如果".to_string(), TokenKind::如果);
        keywords.insert("否则".to_string(), TokenKind::否则);
        keywords.insert("当".to_string(), TokenKind::当);
        keywords.insert("对于".to_string(), TokenKind::对于);
        keywords.insert("在".to_string(), TokenKind::在);
        keywords.insert("返回".to_string(), TokenKind::返回);
        keywords.insert("跳出".to_string(), TokenKind::跳出);
        keywords.insert("继续".to_string(), TokenKind::继续);
        keywords.insert("匹配".to_string(), TokenKind::匹配);

        // 声明
        keywords.insert("函数".to_string(), TokenKind::函数);
        keywords.insert("变量".to_string(), TokenKind::变量);
        keywords.insert("常量".to_string(), TokenKind::常量);
        keywords.insert("类型".to_string(), TokenKind::类型);
        keywords.insert("结构体".to_string(), TokenKind::结构体);
        keywords.insert("枚举".to_string(), TokenKind::枚举);
        keywords.insert("闭包".to_string(), TokenKind::闭包);
        keywords.insert("新建".to_string(), TokenKind::新建);
        keywords.insert("自己".to_string(), TokenKind::自己);

        // 模块
        keywords.insert("包".to_string(), TokenKind::包);
        keywords.insert("模块".to_string(), TokenKind::模块);
        keywords.insert("导入".to_string(), TokenKind::导入);
        keywords.insert("导出".to_string(), TokenKind::导出);
        keywords.insert("作为".to_string(), TokenKind::作为);
        keywords.insert("公开".to_string(), TokenKind::公开);

        // 逻辑运算（词形只保留 与/且/或/非 —— 比较与四则一律用符号）
        keywords.insert("与".to_string(), TokenKind::与);
        keywords.insert("且".to_string(), TokenKind::与); // `且` 为 `与` 的等价逻辑与关键字
        keywords.insert("或".to_string(), TokenKind::或);

        // 字面量
        keywords.insert("真".to_string(), TokenKind::布尔字面量(true));
        keywords.insert("假".to_string(), TokenKind::布尔字面量(false));

        // 基础类型
        keywords.insert("整数".to_string(), TokenKind::类型关键词(BasicType::整数));
        keywords.insert(
            "字符串".to_string(),
            TokenKind::类型关键词(BasicType::字符串),
        );
        keywords.insert("布尔".to_string(), TokenKind::类型关键词(BasicType::布尔));
        keywords.insert(
            "浮点数".to_string(),
            TokenKind::类型关键词(BasicType::浮点数),
        );
        keywords.insert("字符".to_string(), TokenKind::类型关键词(BasicType::字符));
        keywords.insert("字节".to_string(), TokenKind::类型关键词(BasicType::字节));
        keywords.insert("空".to_string(), TokenKind::类型关键词(BasicType::空));
        keywords.insert("指针".to_string(), TokenKind::类型关键词(BasicType::指针));

        // 容器 / 泛型类型
        keywords.insert("列表".to_string(), TokenKind::类型关键词(BasicType::列表));
        keywords.insert("数组".to_string(), TokenKind::数组);

        // 并发
        keywords.insert("启动".to_string(), TokenKind::启动);
        keywords.insert("通道".to_string(), TokenKind::通道);
        keywords.insert("选择".to_string(), TokenKind::选择);
        keywords.insert("情况".to_string(), TokenKind::情况);
        keywords.insert("未来".to_string(), TokenKind::未来);
        keywords.insert("异步".to_string(), TokenKind::异步);
        keywords.insert("超时".to_string(), TokenKind::超时);

        // 错误处理
        keywords.insert("尝试".to_string(), TokenKind::尝试);
        keywords.insert("捕获".to_string(), TokenKind::捕获);
        keywords.insert("抛出".to_string(), TokenKind::抛出);
        keywords.insert("最终".to_string(), TokenKind::最终);

        // 其他
        keywords.insert("弱".to_string(), TokenKind::弱);

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

    /// 全部关键字（给一致性测试与工具用）
    pub fn all_keywords(&self) -> Vec<String> {
        self.keywords.keys().cloned().collect()
    }
}

impl Default for KeywordTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Global keyword lookup table
pub static KEYWORDS: once_cell::sync::Lazy<KeywordTable> =
    once_cell::sync::Lazy::new(KeywordTable::new);
