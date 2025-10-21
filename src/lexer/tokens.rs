//! Token definitions for Qi language

/// Source code span
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Token kinds for Qi language
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    // Chinese Keywords (multi-character)
    如果,      // if
    否则,      // else
    循环,      // loop
    当,        // while
    对于,      // for
    函数,      // function
    返回,      // return
    变量,      // variable
    常量,      // constant
    整数,      // integer
    字符串,    // string
    布尔,      // boolean
    浮点数,    // float
    结构体,    // struct
    枚举,      // enum
    数组,      // array

    // Single-character tokens
    赋值,      // =
    加,        // +
    减,        // -
    乘,        // *
    除,        // /
    取余,      // %
    等于,      // ==
    不等于,    // !=
    大于,      // >
    小于,      // <
    大于等于,  // >=
    小于等于,  // <=
    分号,      // ;
    逗号,      // ,
    左括号,    // (
    右括号,    // )
    左大括号,  // {
    右大括号,  // }
    左方括号,  // [
    右方括号,  // ]
    冒号,      // :
    箭头,      // ->
    点,        // .

    // Additional keywords for grammar
    导入,      // import
    作为,      // as
    在,        // in
    字符,      // char
    空,        // null/void
    参数,      // parameter
    与,        // and
    或,        // or
    打印,      // print

    // Boolean literal constants
    真,                 // true
    假,                 // false

    // Identifiers and literals
    标识符,              // Variable/function names (stored in text field)
    字符串字面量,        // String literals (stored in text field)
    整数字面量(i64),     // Integer literals
    浮点数字面量,   // Float literals (stored in text field)
    布尔字面量(bool),    // Boolean literals
    字符字面量(char),    // Character literals

    // Additional keywords
    非,                 // not
    跳出,               // break
    继续,               // continue
    输入,               // input
    长度,               // length
    类型,               // type

    // Special
    文件结束,
    错误,               // Lexical error (stored in text field)
}

/// Token with source location information
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, text: String, span: Span, line: usize, column: usize) -> Self {
        Self {
            kind,
            text,
            span,
            line,
            column,
        }
    }
}

// Basic Display implementation for debugging
impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::整数字面量(n) => write!(f, "{}", n),
            TokenKind::布尔字面量(b) => write!(f, "{}", b),
            TokenKind::字符字面量(c) => write!(f, "'{}'", c),
            _ => write!(f, "{:?}", self),
        }
    }
}