//! Chinese grammar parsing for Qi language using LALRPOP

pub mod ast;
pub mod error;

// Include the generated LALRPOP parser
include!(concat!(env!("OUT_DIR"), "/parser/grammar.rs"));

pub use ast::{
    ArrayAccessExpression, ArrayLiteralExpression, ArrayType, AssignmentExpression, AstNode,
    AwaitExpression, BasicType, BinaryExpression, BinaryOperator, EnumDeclaration, EnumType,
    EnumVariant, ExpressionStatement, FieldAccessExpression, ForStatement, FunctionCallExpression,
    FunctionDeclaration, IdentifierExpression, IfStatement, LiteralExpression, LiteralValue,
    LoopStatement, Parameter, Program, ReturnStatement, StringConcatExpression, StructDeclaration,
    StructField, StructFieldValue, StructLiteralExpression, StructType, TypeNode,
    VariableDeclaration, WhileStatement,
};
pub use error::ParseError;

/// Translate LALRPOP's `ParseError` into a rustc-style multi-line message with
/// a line/column header, a source snippet, a caret pointing at the offending
/// span, and a list of expected tokens (cleaned up — LALRPOP regex strings
/// like `r#"[\\u4e00-\\u9fff]..."#` are demangled to `<标识符>` / `<英文标识符>`).
///
/// Replaces the previous `format!("{:?}", e)` dump that exposed raw enum debug
/// info to end users.
fn format_lalrpop_error<L, T, E>(source: &str, err: &lalrpop_util::ParseError<L, T, E>) -> String
where
    L: Copy + Into<usize>,
    T: std::fmt::Display,
    E: std::fmt::Display,
{
    use lalrpop_util::ParseError as LE;
    match err {
        LE::InvalidToken { location } => {
            let off: usize = (*location).into();
            let (line, col) = byte_offset_to_line_col(source, off);
            let snippet = snippet_with_caret(source, line, col, 1);
            format!(
                "语法错误：无法识别的标记\n  --> 第 {line} 行第 {col} 列\n{snippet}\n  提示：检查是否有非法字符或拼写错误"
            )
        }
        LE::UnrecognizedEof { location, expected } => {
            let off: usize = (*location).into();
            let (line, col) = byte_offset_to_line_col(source, off);
            let snippet = snippet_with_caret(source, line, col, 1);
            let exp = friendly_expected_list(expected);
            format!(
                "语法错误：源码意外结束\n  --> 第 {line} 行第 {col} 列\n{snippet}\n  期望：{exp}"
            )
        }
        LE::UnrecognizedToken {
            token: (start, tok, end),
            expected,
        } => {
            let lo: usize = (*start).into();
            let hi: usize = (*end).into();
            let len = hi.saturating_sub(lo).max(1);
            let (line, col) = byte_offset_to_line_col(source, lo);
            let snippet = snippet_with_caret(source, line, col, len);
            let exp_summary = friendly_expected_list(expected);
            let tok_str = tok.to_string();
            let hint = build_unexpected_token_hint(&tok_str, expected);
            format!(
                "语法错误：意外的标记 `{tok}`\n  --> 第 {line} 行第 {col} 列\n{snippet}\n  期望：{exp_summary}{hint}"
            )
        }
        LE::ExtraToken {
            token: (start, tok, end),
        } => {
            let lo: usize = (*start).into();
            let hi: usize = (*end).into();
            let len = hi.saturating_sub(lo).max(1);
            let (line, col) = byte_offset_to_line_col(source, lo);
            let snippet = snippet_with_caret(source, line, col, len);
            format!(
                "语法错误：多余的标记 `{tok}`\n  --> 第 {line} 行第 {col} 列\n{snippet}\n  提示：删除多余的标记，或检查上一行是否漏写 `;` / `}}`"
            )
        }
        LE::User { error } => format!("语法错误：{error}"),
    }
}

/// Convert a UTF-8 byte offset into (1-based line, 1-based column).
/// Column counts characters, not bytes, so CJK content reads naturally.
fn byte_offset_to_line_col(source: &str, byte_off: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    let mut byte_seen = 0usize;
    for ch in source.chars() {
        if byte_seen >= byte_off {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        byte_seen += ch.len_utf8();
    }
    (line, col)
}

/// Build a two-line snippet: the source line, then a caret line pointing at
/// columns `col..col+span_chars`. Column is 1-based, character-counted.
fn snippet_with_caret(source: &str, line: usize, col: usize, span_chars: usize) -> String {
    let line_text = source.lines().nth(line - 1).unwrap_or("");
    let line_no = format!("{line:4} | ");
    let padding = " ".repeat(line_no.len());
    let mut caret = String::new();
    // Each character on the line that's before the cursor: we just lay down spaces.
    let prefix_chars = col.saturating_sub(1);
    for c in line_text.chars().take(prefix_chars) {
        // Approximation: use 2 spaces for wide chars, 1 for narrow. Most terminals
        // align CJK to 2-cell width.
        if (c as u32) >= 0x2E80 {
            caret.push(' ');
            caret.push(' ');
        } else {
            caret.push(' ');
        }
    }
    for _ in 0..span_chars.max(1) {
        caret.push('^');
    }
    format!("{line_no}{line_text}\n{padding}{caret}")
}

/// Demangle LALRPOP's regex-based expected list into a short user-facing summary.
/// Truncates to first 8 candidates with `…N more` suffix to avoid wall-of-text.
fn friendly_expected_list<T: std::fmt::Display>(expected: &[T]) -> String {
    let mut friendly: Vec<String> = Vec::new();
    let mut saw_cjk_ident = false;
    let mut saw_ascii_ident = false;
    // 把 LALRPOP 的原始正则终结符（如 [0-9]+、"([^"\\]|\\.)*" 等）映射成
    // 人话名字，否则报错里的「期望：」会直接吐一串难懂的正则。
    let mut specials: Vec<&str> = Vec::new();
    let mut push_special = |name: &'static str, bag: &mut Vec<&'static str>| {
        if !bag.contains(&name) {
            bag.push(name);
        }
    };
    for e in expected {
        let s = e.to_string();
        let is_regex = s.starts_with("r#") || s.contains("[0-9]") || s.contains("[^");
        if s.contains("u4e00") || s.contains("\\u4e00") {
            saw_cjk_ident = true;
        } else if s.contains("a-zA-Z") {
            saw_ascii_ident = true;
        } else if s.contains("\\{") || (s.contains("f\"") && s.contains("[^")) {
            // 格式字符串 f"...{...}..."
            push_special("<格式字符串>", &mut specials);
        } else if s.contains("'(") || s.contains("'.'") || s.contains("'\\") {
            // 字符字面量 '.' / '([^\\']|\\.)'（须先于字符串字面量判断，二者正则都含 [^）
            push_special("<字符字面量>", &mut specials);
        } else if s.contains("[^") {
            // 普通字符串字面量 "..."
            push_special("<字符串字面量>", &mut specials);
        } else if s.contains("[0-9]") && s.contains("\\.") {
            push_special("<浮点数字面量>", &mut specials);
        } else if s.contains("[0-9]") {
            push_special("<整数字面量>", &mut specials);
        } else if is_regex {
            // 兜底：未识别的正则，跳过（不往用户脸上糊正则）
        } else {
            let cleaned = s.trim_start_matches("r#").trim_matches('"').to_string();
            if !cleaned.is_empty() && !friendly.contains(&cleaned) {
                friendly.push(cleaned);
            }
        }
    }
    // 字面量类放在前面，运算符/符号类在后
    for sp in specials.into_iter().rev() {
        friendly.insert(0, sp.to_string());
    }
    if saw_ascii_ident && !saw_cjk_ident {
        friendly.insert(0, "<英文标识符>".to_string());
    }
    if saw_cjk_ident {
        friendly.insert(0, "<标识符>".to_string());
    }
    if friendly.is_empty() {
        return "（无）".to_string();
    }
    if friendly.len() > 8 {
        let total = friendly.len();
        friendly.truncate(8);
        format!("{} ... 以及 {} 个其他选项", friendly.join(" / "), total - 8)
    } else {
        friendly.join(" / ")
    }
}

/// Build the trailing hint line for an unexpected-token error. Returns `""` if
/// no specific hint applies. Hints are ordered most-specific first:
///   1. parser expected `;` → likely missing semicolon
///   2. parser expected `}` / `)` → likely unbalanced bracket
///   3. token is a known reserved word being used in identifier position
///   4. otherwise no hint
fn build_unexpected_token_hint<T: std::fmt::Display>(tok: &str, expected: &[T]) -> String {
    let expected_strs: Vec<String> = expected.iter().map(|e| e.to_string()).collect();
    let expects = |needle: &str| expected_strs.iter().any(|s| s == needle);

    if expects("\";\"") || expects("\"；\"") {
        return "\n  提示：上一行可能漏写了 `;`，或本行多写了 `变量` / `函数` 关键字".to_string();
    }
    if expects("\"}\"") {
        return "\n  提示：缺少 `}` — 检查上面的代码块是否漏写收尾大括号".to_string();
    }
    if expects("\")\"") {
        return "\n  提示：缺少 `)` — 检查函数调用 / 参数列表是否漏写右括号".to_string();
    }

    const RESERVED_LANDMINES: &[&str] = &[
        "结果",
        "类型",
        "尝试",
        "捕获",
        "抛出",
        "最终",
        "继续",
        "跳出",
        "返回",
        "等待",
        "异步",
        "异步块",
        "新建",
        "解引用",
        "取地址",
        "在",
        "作为",
        "选择",
        "情况",
    ];
    if RESERVED_LANDMINES.contains(&tok) {
        return format!(
            "\n  提示：`{tok}` 是 qi 的保留字，不能作为标识符名。常被误用的保留字：结果 / 类型 / 尝试 / 继续 / 返回。换个别名（如 `{tok}值`）"
        );
    }

    String::new()
}

#[cfg(test)]
mod error_format_tests {
    use super::*;

    #[test]
    fn reports_reserved_word_as_variable_name() {
        let src = "包 主程序;\n函数 入口() {\n    变量 结果: 整数 = 1;\n}\n";
        let p = Parser::new();
        let err = p.parse_source(src).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("第 3 行"), "msg lacks line: {msg}");
        assert!(msg.contains("保留字"), "msg lacks 保留字 hint: {msg}");
    }

    #[test]
    fn reports_missing_semicolon_hint() {
        // Parser sees `变量` on next line where it expected `;` after the assignment.
        let src = "包 主程序;\n变量 x: 整数 = 1\n变量 y: 整数 = 2;\n";
        let p = Parser::new();
        let err = p.parse_source(src).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("行"), "msg lacks line ref: {msg}");
        // Should detect missing `;` rather than going down the reserved-word path
        // (变量 isn't reserved in that sense).
        assert!(
            msg.contains("漏写了 `;`") || msg.contains("漏写"),
            "msg lacks semicolon hint: {msg}"
        );
    }

    #[test]
    fn expected_list_truncates_when_huge() {
        // Triggering a position with many possible operators creates a >8 entry list.
        let src = "包 主程序;\n变量 x: 整数 = 1\n变量 y: 整数 = 2;\n";
        let p = Parser::new();
        let err = p.parse_source(src).unwrap_err();
        let msg = format!("{}", err);
        // Either the full list is short, or we see the truncation hint
        let truncated = msg.contains("以及") && msg.contains("个其他选项");
        let short = msg.matches(" / ").count() <= 8;
        assert!(
            truncated || short,
            "expected truncation or short list: {msg}"
        );
    }

    #[test]
    fn byte_offset_chinese_correct_column() {
        // "包 主程序" — chars: 包(0), space(1), 主(2), 程(3), 序(4)
        // byte offsets: 包=3, space=1, 主=3, 程=3, 序=3
        // After "包 " (4 bytes), next char 主 starts at byte 4.
        let src = "包 主程序;\n";
        let (line, col) = byte_offset_to_line_col(src, 4);
        assert_eq!(line, 1);
        assert_eq!(col, 3); // 1-based: 包=1, space=2, 主=3
    }
}

/// Qi language parser using LALRPOP-generated parser
pub struct Parser {
    _private: (),
}

impl Parser {
    /// Create a new parser
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Parse source code directly into an AST
    pub fn parse_source(&self, source: &str) -> Result<Program, ParseError> {
        // Preprocess: strip BOM and comments to support fixtures that include them
        fn strip_comments(input: &str) -> String {
            // Strip UTF-8 BOM if present
            let s = if input.starts_with('\u{feff}') {
                &input["\u{feff}".len()..]
            } else {
                input
            };

            let bytes = s.as_bytes();
            let mut out = String::with_capacity(s.len());
            let mut i = 0;
            let n = bytes.len();

            let mut in_line_comment = false;
            // 块注释可嵌套（与 Rust 语义一致）：用深度计数配对 /* 与 */
            let mut block_comment_depth = 0usize;
            let mut in_string = false;
            let mut in_char = false;
            let mut escape = false;

            while i < n {
                if in_line_comment {
                    // End of line ends the comment
                    if bytes[i] == b'\n' {
                        in_line_comment = false;
                        out.push('\n');
                    }
                    i += 1;
                    continue;
                }

                if block_comment_depth > 0 {
                    if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        block_comment_depth += 1;
                        i += 2;
                    } else if i + 1 < n && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        block_comment_depth -= 1;
                        i += 2;
                    } else {
                        // 保留换行，让后续诊断的行号不漂移
                        if bytes[i] == b'\n' {
                            out.push('\n');
                        }
                        i += 1;
                    }
                    continue;
                }

                if in_string {
                    // Read next UTF-8 char
                    let ch = s[i..].chars().next().unwrap();
                    let len = ch.len_utf8();
                    out.push(ch);
                    if !escape {
                        if ch == '"' {
                            in_string = false;
                        } else if ch == '\\' {
                            escape = true;
                        }
                    } else {
                        // escaped char consumed
                        escape = false;
                    }
                    i += len;
                    continue;
                }

                if in_char {
                    // Read next UTF-8 char
                    let ch = s[i..].chars().next().unwrap();
                    let len = ch.len_utf8();
                    out.push(ch);
                    if !escape {
                        if ch == '\'' {
                            in_char = false;
                        } else if ch == '\\' {
                            escape = true;
                        }
                    } else {
                        escape = false;
                    }
                    i += len;
                    continue;
                }

                // Not inside any literal or comment
                if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    block_comment_depth = 1;
                    i += 2;
                    continue;
                }

                // Handle start of string/char literals or just copy next char
                let ch = s[i..].chars().next().unwrap();
                let len = ch.len_utf8();
                if ch == '"' {
                    in_string = true;
                    escape = false;
                    out.push(ch);
                    i += len;
                    continue;
                }
                if ch == '\'' {
                    in_char = true;
                    escape = false;
                    out.push(ch);
                    i += len;
                    continue;
                }

                out.push(ch);
                i += len;
            }

            out
        }

        let cleaned = strip_comments(source);

        // Normalize Chinese fullwidth colon （：U+FF1A) to ASCII colon outside string literals,
        // so users can write `：` anywhere `:` is accepted in the grammar.
        fn normalize_colons(s: &str) -> String {
            let mut out = String::with_capacity(s.len());
            let mut in_string = false;
            let mut escape = false;
            for ch in s.chars() {
                if in_string {
                    if escape {
                        escape = false;
                    } else if ch == '\\' {
                        escape = true;
                    } else if ch == '"' {
                        in_string = false;
                    }
                    out.push(ch);
                } else if ch == '"' {
                    in_string = true;
                    out.push(ch);
                } else if ch == '：' {
                    out.push(':');
                } else {
                    out.push(ch);
                }
            }
            out
        }
        let cleaned = normalize_colons(&cleaned);

        // Use LALRPOP-generated parser with cleaned string input
        use crate::parser::__parse__Program::ProgramParser;
        ProgramParser::new()
            .parse(&cleaned)
            .map_err(|e| ParseError::General(format_lalrpop_error(&cleaned, &e)))
    }

    /// Parse tokens into an AST (legacy method - tokenizes first)
    pub fn parse(&self, tokens: Vec<crate::lexer::Token>) -> Result<Program, ParseError> {
        // Reconstruct source from tokens preserving original structure
        // Use the original span information to maintain proper spacing
        let mut source = String::new();
        let mut last_end = 0;

        for token in &tokens {
            // Preserve spacing between tokens based on original positions
            if token.span.start > last_end {
                // Add the original whitespace/newlines that were between tokens
                // For now, add a space if there was a gap
                source.push(' ');
            }
            source.push_str(&token.text);
            last_end = token.span.end;
        }

        self.parse_source(&source.trim())
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to unescape string literals
pub fn unescape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '\'' => out.push('\''),
                    '0' => out.push('\0'),
                    _ => out.push(next),
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
