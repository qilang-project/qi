//! Chinese grammar parsing for Qi language using LALRPOP

pub mod ast;
pub mod error;
mod html;
#[path = "位置.rs"]
pub mod 位置;

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
    let push_special = |name: &'static str, bag: &mut Vec<&'static str>| {
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
        "到",
        "作为",
        "选择",
        "情况",
        "枚举",
        "弱",
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

    /// 反引号原始字符串里的 `//` 不是注释。
    ///
    /// 回归自一次真实事故：qi-web 把打包好的客户端 JS 塞进原始字符串常量，
    /// 里面有 `"wss://"`，被 strip_comments 当成行注释，从 `//` 一直吃到行尾，
    /// 连收尾的反引号一起吃掉 —— 该常量后面的整个文件都被卷进字符串里，
    /// 报错却是几千字符之外的「未定义的函数: 样式源」，极难定位。
    #[test]
    fn raw_string_keeps_double_slash() {
        let src = "包 主程序;\n函数 入口() {\n    变量 s: 字符串 = `a//b`;\n}\n";
        let p = Parser::new();
        assert!(p.parse_source(src).is_ok(), "原始字符串里的 // 不该当注释");
    }

    /// 原始字符串后面还有别的定义时，早闭合的破坏最明显
    #[test]
    fn raw_string_with_url_does_not_swallow_rest_of_file() {
        let src = concat!(
            "包 主程序;\n",
            "函数 脚本() : 字符串 {\n",
            "    返回 `var ws = new WebSocket(\"wss://\" + host);`;\n",
            "}\n",
            "函数 样式() : 字符串 {\n",
            "    返回 `:root{--a:1}`;\n",
            "}\n",
            "函数 入口() {}\n"
        );
        let p = Parser::new();
        assert!(p.parse_source(src).is_ok(), "后面的函数不该被卷进字符串");
    }

    /// 块注释起始 `/*` 同理
    #[test]
    fn raw_string_keeps_block_comment_marker() {
        let src = "包 主程序;\n函数 入口() {\n    变量 s: 字符串 = `/* not a comment */`;\n}\n";
        let p = Parser::new();
        assert!(p.parse_source(src).is_ok(), "原始字符串里的 /* 不该开注释");
    }

    /// 全角冒号归一化也不能动原始字符串 —— 里面是 JS/CSS/文案，改了就坏
    #[test]
    fn raw_string_keeps_fullwidth_colon() {
        let src = "包 主程序;\n函数 入口() {\n    变量 s: 字符串 = `提示：请稍候`;\n}\n";
        let p = Parser::new();
        let ast = p.parse_source(src).expect("应当解析通过");
        let dumped = format!("{ast:?}");
        assert!(
            dumped.contains('：'),
            "原始字符串里的全角冒号被改写了: {dumped}"
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
        let (source, has_html) =
            html::rewrite_html_templates(source).map_err(ParseError::General)?;
        let source = source.as_str();
        // Preprocess: strip BOM and comments to support fixtures that include them.
        //
        // 【保字节偏移】所有预处理都按「等字节数替换」实现：注释内容/BOM 用等量
        // 空格顶位、换行原样保留 —— 这样 LALRPOP @L/@R 产出的 AST span 就是
        // **原始源码**的 UTF-8 字节偏移，语义检查可直接换算行列。
        // （唯一例外：normalize_else_if 对连写 `否则如果` 插 1 个空格，见彼处注释。）
        fn strip_comments(input: &str) -> String {
            // UTF-8 BOM（3 字节）用 3 个空格顶位，不改变后续字节偏移
            let (bom_pad, s) = if input.starts_with('\u{feff}') {
                ("   ", &input["\u{feff}".len()..])
            } else {
                ("", input)
            };

            let bytes = s.as_bytes();
            let mut out = String::with_capacity(input.len());
            out.push_str(bom_pad);
            let mut i = 0;
            let n = bytes.len();

            let mut in_line_comment = false;
            // 块注释可嵌套（与 Rust 语义一致）：用深度计数配对 /* 与 */
            let mut block_comment_depth = 0usize;
            let mut in_string = false;
            let mut in_char = false;
            // 反引号原始字符串：内部不做转义，也**不认注释**。内嵌 JS/CSS 里
            // 全是 "wss://" 和 "//" 这种，不单独跟踪的话会被当成行注释吃掉，
            // 连带把收尾的反引号一起吃了，报错点却落在几千字符之外。
            let mut in_raw = false;
            let mut escape = false;

            while i < n {
                if in_line_comment {
                    // End of line ends the comment
                    if bytes[i] == b'\n' {
                        in_line_comment = false;
                        out.push('\n');
                    } else {
                        out.push(' '); // 注释字节以空格顶位，保偏移
                    }
                    i += 1;
                    continue;
                }

                if block_comment_depth > 0 {
                    if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        block_comment_depth += 1;
                        out.push_str("  ");
                        i += 2;
                    } else if i + 1 < n && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        block_comment_depth -= 1;
                        out.push_str("  ");
                        i += 2;
                    } else {
                        // 保留换行，让后续诊断的行号不漂移；其余字节空格顶位
                        if bytes[i] == b'\n' {
                            out.push('\n');
                        } else {
                            out.push(' ');
                        }
                        i += 1;
                    }
                    continue;
                }

                if in_raw {
                    let ch = s[i..].chars().next().unwrap();
                    let len = ch.len_utf8();
                    out.push(ch);
                    if ch == '`' {
                        in_raw = false;
                    }
                    i += len;
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
                    out.push_str("  "); // `//` 两字节空格顶位
                    i += 2;
                    continue;
                }
                if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    block_comment_depth = 1;
                    out.push_str("  "); // `/*` 两字节空格顶位
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
                if ch == '`' {
                    in_raw = true;
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
        // 【保字节偏移】`：` 是 3 字节，替换为 `:` + 2 空格（同 3 字节）；
        // 连写 `：：`（6 字节）替换为 `::` + 4 空格，保住 `::` 的相邻性。
        fn normalize_colons(s: &str) -> String {
            let mut out = String::with_capacity(s.len());
            let mut in_string = false;
            // 反引号原始字符串同样原样放行：内嵌的中文文案里出现「：」不该被改写
            let mut in_raw = false;
            let mut escape = false;
            let mut it = s.chars().peekable();
            while let Some(ch) = it.next() {
                if in_raw {
                    if ch == '`' {
                        in_raw = false;
                    }
                    out.push(ch);
                } else if in_string {
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
                } else if ch == '`' {
                    in_raw = true;
                    out.push(ch);
                } else if ch == '：' {
                    if it.peek() == Some(&'：') {
                        it.next();
                        out.push_str("::    ");
                    } else {
                        out.push_str(":  ");
                    }
                } else {
                    out.push(ch);
                }
            }
            out
        }
        let cleaned = normalize_colons(&cleaned);

        // 中文插值前缀归一化：字符串外的 模板"/模版" → f"。
        // 自定义 lexer 路径（run/compile）在 token text 里已做过同样归一化；
        // parse_source 直喂 LALRPOP（check/测试走这里），必须补齐，否则
        // qi check 对 模板"..." 误报语法错误。带字符串/字符/反引号状态机，
        // 绝不动字面量内部的「模板"」（如 "我的模板" 的闭引号）。
        fn normalize_template_prefix(s: &str) -> String {
            let chars: Vec<char> = s.chars().collect();
            let mut out = String::with_capacity(s.len());
            let mut i = 0;
            let (mut in_str, mut in_char, mut in_raw, mut escape) = (false, false, false, false);
            while i < chars.len() {
                let c = chars[i];
                if in_raw {
                    out.push(c);
                    if c == '`' {
                        in_raw = false;
                    }
                } else if in_str || in_char {
                    out.push(c);
                    if escape {
                        escape = false;
                    } else if c == '\\' {
                        escape = true;
                    } else if in_str && c == '"' {
                        in_str = false;
                    } else if in_char && c == '\'' {
                        in_char = false;
                    }
                } else {
                    match c {
                        '"' => {
                            in_str = true;
                            out.push(c);
                        }
                        '\'' => {
                            in_char = true;
                            out.push(c);
                        }
                        '`' => {
                            in_raw = true;
                            out.push(c);
                        }
                        '模' if i + 2 < chars.len()
                            && (chars[i + 1] == '板' || chars[i + 1] == '版')
                            && chars[i + 2] == '"' =>
                        {
                            // 【保字节偏移】`模板`（6 字节）→ 5 空格 + `f`（6 字节），
                            // `f` 紧贴引号满足 f"..." 词法；串内内容偏移与原文一致。
                            out.push_str("     f");
                            out.push('"');
                            in_str = true;
                            i += 2; // 吃掉 板/版 和引号
                        }
                        _ => out.push(c),
                    }
                }
                i += 1;
            }
            out
        }
        let cleaned = normalize_template_prefix(&cleaned);

        // `否则如果`（else if 连写）归一化：字符串外的连写 `否则如果` → `否则 如果`。
        // LALRPOP 内建词法器最大匹配：连写的 `否则如果` 会被中文标识符正则整吞成
        // 一个 4 字标识符（长度 4 > 关键字 `否则` 长度 2），于是 else-if 链解析失败。
        // 插一个空格后 LALRPOP 稳定切成 `否则`+`如果` 两个关键字 token，与用户
        // 手写空格分开的 `否则 如果` 归一，语法层统一按连续两 token 处理。
        // 自定义 lexer 路径（run/compile）把连写读成一个标识符 token，parse() 重组源码
        // 后仍是连写，同样在这里被接住 —— 双解析路径共用此归一化。
        fn normalize_else_if(s: &str) -> String {
            let chars: Vec<char> = s.chars().collect();
            let mut out = String::with_capacity(s.len() + 8);
            let mut i = 0;
            let (mut in_str, mut in_char, mut in_raw, mut escape) = (false, false, false, false);
            while i < chars.len() {
                let c = chars[i];
                if in_raw {
                    out.push(c);
                    if c == '`' {
                        in_raw = false;
                    }
                    i += 1;
                } else if in_str || in_char {
                    out.push(c);
                    if escape {
                        escape = false;
                    } else if c == '\\' {
                        escape = true;
                    } else if in_str && c == '"' {
                        in_str = false;
                    } else if in_char && c == '\'' {
                        in_char = false;
                    }
                    i += 1;
                } else if c == '"' {
                    in_str = true;
                    out.push(c);
                    i += 1;
                } else if c == '\'' {
                    in_char = true;
                    out.push(c);
                    i += 1;
                } else if c == '`' {
                    in_raw = true;
                    out.push(c);
                    i += 1;
                } else if c == '否'
                    && i + 3 < chars.len()
                    && chars[i + 1] == '则'
                    && chars[i + 2] == '如'
                    && chars[i + 3] == '果'
                {
                    out.push_str("否则 如果");
                    i += 4;
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            out
        }
        let cleaned = normalize_else_if(&cleaned);

        // 把单个双引号串的内容（不含两端引号、转义仍是原始形态）改写成 f-string 体。
        // 返回 Ok(Some(体)) 表示含插值、已改写；Ok(None) 表示无插值、保持原样；
        // Err(消息) 表示空洞/未闭合/嵌套等错误。line 仅用于错误消息定位。
        fn 改写插值体(content: &[char], line: usize) -> Result<Option<String>, String> {
            let n = content.len();
            let mut out = String::with_capacity(n + 8);
            let mut found = false;
            let mut i = 0;
            while i < n {
                let c = content[i];
                if c == '\\' {
                    // 转义序列。`\$` → 字面 `$`（其后的 `{` 走下方字面括号分支变 `\{`，
                    // 合起来 `\${` → `$\{` → 解析回字面 `${`）；其余 `\x` 原样保留交给
                    // f-string 词法/parse_format_string（`\n`/`\t`/`\"`/`\\`/`\{`…）。
                    if i + 1 < n {
                        let nx = content[i + 1];
                        if nx == '$' {
                            out.push('$');
                        } else {
                            out.push('\\');
                            out.push(nx);
                        }
                        i += 2;
                        continue;
                    }
                    out.push('\\');
                    i += 1;
                    continue;
                }
                if c == '$' && i + 1 < n && content[i + 1] == '{' {
                    found = true;
                    // 扫描配对花括号，定位洞尾。洞内字符串字面量整段跳过（其中的
                    // {}/引号不参与平衡）；洞内再出现 `${` → 嵌套，v1 明确报错。
                    let hole_start = i + 2;
                    let mut j = hole_start;
                    let mut depth = 1i32;
                    while j < n {
                        let hc = content[j];
                        if hc == '\\' {
                            j += 2;
                            continue;
                        }
                        if hc == '"' {
                            j += 1;
                            while j < n && content[j] != '"' {
                                if content[j] == '\\' {
                                    j += 1;
                                }
                                j += 1;
                            }
                            if j < n {
                                j += 1; // 跳过内层闭引号
                            }
                            continue;
                        }
                        if hc == '$' && j + 1 < n && content[j + 1] == '{' {
                            return Err(format!(
                                "字符串插值暂不支持嵌套：第 {line} 行的 ${{…}} 里又出现了 ${{…}}，请把内层表达式先存到变量再插值"
                            ));
                        }
                        if hc == '{' {
                            depth += 1;
                        } else if hc == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        j += 1;
                    }
                    if depth != 0 {
                        return Err(format!(
                            "字符串插值未闭合：第 {line} 行的 ${{ 缺少配对的 }}"
                        ));
                    }
                    let hole: String = content[hole_start..j].iter().collect();
                    if hole.trim().is_empty() {
                        return Err(format!(
                            "字符串插值不能为空：第 {line} 行的 ${{}} 里必须写一个表达式"
                        ));
                    }
                    // 输出成 f-string 的洞：{原文}（洞原文原样交给 parse_format_string，
                    // 它对非平凡洞会用 ExprParser 真解析）。
                    out.push('{');
                    out.push_str(&hole);
                    out.push('}');
                    i = j + 1;
                    continue;
                }
                // 串里的字面花括号（如内嵌 JSON）：补反斜杠让 f-string 当字面文本，
                // 不被误判成洞。全/半角一并处理。
                match c {
                    '{' => out.push_str("\\{"),
                    '}' => out.push_str("\\}"),
                    '｛' => out.push_str("\\｛"),
                    '｝' => out.push_str("\\｝"),
                    _ => out.push(c),
                }
                i += 1;
            }
            if found {
                Ok(Some(out))
            } else {
                Ok(None)
            }
        }

        // 归一化字符串插值：扫描源码，普通双引号串含 `${` 时改写成 `f"..."`。
        // 带字符串/字符/反引号状态机，绝不动原始串与字符字面量内部。
        fn normalize_string_interpolation(s: &str) -> Result<String, String> {
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len();
            let mut out = String::with_capacity(s.len() + 16);
            let mut i = 0;
            let mut line = 1usize;
            while i < n {
                let c = chars[i];
                match c {
                    '\n' => {
                        line += 1;
                        out.push(c);
                        i += 1;
                    }
                    '`' => {
                        // 原始字符串：整段原样复制，绝不插值
                        out.push(c);
                        i += 1;
                        while i < n && chars[i] != '`' {
                            if chars[i] == '\n' {
                                line += 1;
                            }
                            out.push(chars[i]);
                            i += 1;
                        }
                        if i < n {
                            out.push(chars[i]); // 闭合反引号
                            i += 1;
                        }
                    }
                    '\'' => {
                        // 字符字面量：'x' / '\n' / '"'。内部含 `"` 不能误当字符串开始。
                        out.push(c);
                        i += 1;
                        if i < n && chars[i] == '\\' {
                            out.push(chars[i]);
                            i += 1;
                            if i < n {
                                out.push(chars[i]);
                                i += 1;
                            }
                        } else if i < n {
                            out.push(chars[i]);
                            i += 1;
                        }
                        if i < n && chars[i] == '\'' {
                            out.push(chars[i]);
                            i += 1;
                        }
                    }
                    '"' => {
                        // 已带 f 前缀（含 模板→f 归一化后的）= 格式串，跳过插值改写，
                        // 整串原样复制交给已有 f-string 机制。
                        let 是格式串 = out.chars().last() == Some('f');
                        i += 1; // 跳过开引号
                        let content_start = i;
                        while i < n && chars[i] != '"' {
                            if chars[i] == '\\' && i + 1 < n {
                                i += 2;
                            } else {
                                if chars[i] == '\n' {
                                    line += 1;
                                }
                                i += 1;
                            }
                        }
                        let content = &chars[content_start..i.min(n)];
                        let 有闭引号 = i < n;
                        if 有闭引号 {
                            i += 1; // 跳过闭引号
                        }
                        if 是格式串 {
                            out.push('"');
                            out.extend(content.iter());
                            if 有闭引号 {
                                out.push('"');
                            }
                            continue;
                        }
                        match 改写插值体(content, line)? {
                            Some(体) => {
                                out.push('f');
                                out.push('"');
                                out.push_str(&体);
                                if 有闭引号 {
                                    out.push('"');
                                }
                            }
                            None => {
                                out.push('"');
                                out.extend(content.iter());
                                if 有闭引号 {
                                    out.push('"');
                                }
                            }
                        }
                    }
                    _ => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
            Ok(out)
        }

        // 普通双引号字符串插值：`"你好 ${名}，n+1=${n + 1}"` → 归一化为已有的
        // 格式串形态 `f"你好 {名}，n+1={n + 1}"`，完全复用 f"..." 的脱糖链
        // （parse_format_string 造洞 + codegen 生成格式字符串 拿 ExprParser 真解析洞
        // + 类型检查/所有权/ARC 全部按 格式字符串表达式 走）——最终脱糖成字符串拼接。
        //
        // 设计要点：
        // - 仅**普通双引号**串参与；反引号原始串（内嵌 JS/qiMarkdown 的 `${}` 是 JS 语义）
        //   与字符字面量整段原样复制，绝不插值。
        // - 已带 `f`/`模板` 前缀的格式串（前缀归一化后都是 `f"`）跳过，避免二次改写。
        // - 只有真正含未转义 `${` 的串才改写；不含的原样字节复制 —— 存量零改动。
        // - `\${` 输出字面 `${`（escape）；串里其它字面 `{}`（如 JSON）在改写时补 `\{`
        //   `\}` 交给 f-string 当字面文本，语义不变。
        // - 空洞 `${}`、未闭合 `${`、嵌套 `${…${…}…}` → 清晰中文编译错误。
        // 放在归一化链末尾：前面的冒号/模板/else-if 各 pass 都按原始 `"..."` 串工作
        // （它们久经测试），改写成 `f"..."` 后只剩 LALRPOP 接手（其洞内含引号由
        // f-string 词法 `\{[^}]*\}` 一段吞掉，安全）。
        let cleaned = normalize_string_interpolation(&cleaned).map_err(ParseError::General)?;

        // Use LALRPOP-generated parser with cleaned string input
        use crate::parser::__parse__Program::ProgramParser;
        let mut program = ProgramParser::new()
            .parse(&cleaned)
            .map_err(|e| ParseError::General(format_lalrpop_error(&cleaned, &e)))?;
        if has_html {
            program.imports.push(ImportStatement {
                module_path: vec!["Web".to_string(), "HTML块".to_string()],
                items: Some(vec![
                    "HTML块".to_string(),
                    "__qi_html模板".to_string(),
                    "__qi_html循环块".to_string(),
                    "__qi_html渲染块组".to_string(),
                    "创建片段".to_string(),
                    "块加子块".to_string(),
                    "渲染块".to_string(),
                ]),
                alias: None,
                is_public: false,
                span: Default::default(),
            });
        }
        Ok(program)
    }

    /// Parse tokens into an AST (legacy method - tokenizes first)
    pub fn parse(&self, tokens: Vec<crate::lexer::Token>) -> Result<Program, ParseError> {
        // Reconstruct source from tokens preserving original structure.
        // 【保字节偏移】token.span 是原始源码的 UTF-8 字节偏移（lexer 按 len_utf8
        // 递增），重组时把每个 token 文本垫回它原来的字节偏移处（间隙 —— 原本的
        // 空白/注释 —— 用空格顶位），这样 @L/@R 产出的 span 与原文件字节偏移一致。
        // 注意不再 trim 开头空白，否则整体偏移左移。
        let mut source = String::new();

        for token in &tokens {
            let cur = source.len();
            if token.span.start > cur {
                // 垫空格到 token 的原始字节偏移
                source.push_str(&" ".repeat(token.span.start - cur));
            }
            // 若 cur > start（个别 token 文本经词法归一化变长），直接顺排，
            // 后续 token 仍会按各自偏移重新对齐。
            source.push_str(&token.text);
        }

        self.parse_source(&source)
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
