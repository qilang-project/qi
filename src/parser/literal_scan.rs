//! 字面量扫描：找双引号串和插值洞的结尾。
//!
//! 预处理链上好几个 pass（去注释、全角冒号、模板前缀、否则如果、插值改写）都要
//! 「跳过字符串字面量」。它们原来各自写一个状态机，统一的做法是碰到 `"` 就一直读到
//! 下一个没被转义的 `"`。这在插值洞里有字符串字面量时是错的：
//!
//! ```text
//! "键是 ${取(表, "甲")}"
//!         ^^^^^^^^^^^^ 洞里是代码，"甲" 是洞里的字面量
//! ```
//!
//! 按老办法，串在 `"键是 ${取(表, "` 就结束了，`甲` 被当成串外的代码 —— 全角冒号
//! 那一 pass 会去改写它（这类「预处理改写了串里的内容」正是磁带漂移那次的根因）。
//! 这里给出一个认得洞的扫描器，所有 pass 共用；qifmt（qi-tools）也用它算缩进，
//! 不另写第三份。
//!
//! 只看 ASCII 分隔符（`"` `` ` `` `'` `{` `}` `\` `$`），所以同一份代码既能喂 `&[char]`
//! 也能喂 UTF-8 字节：多字节字符的每个字节都 ≥ 0x80，不会被误认成分隔符。

/// 双引号串的结尾：`start` 指向开引号 `"`，返回闭引号**之后**的下标；
/// 没有闭引号时返回 `s.len()`（与原来各 pass「读到文件尾」的行为一致）。
///
/// 遇到 `${` 就按代码扫到配对的 `}`，洞里的字符串/字符字面量整段跳过。
pub fn quoted_string_end<T: Copy + Into<u32>>(s: &[T], start: usize) -> usize {
    quoted_string_close(s, start).map_or(s.len(), |k| k + 1)
}

/// 同 `quoted_string_end`，但返回**闭引号本身**的下标；没有闭引号返回 `None`。
/// 要区分「闭合了」和「读到了文件尾」时用这个 —— 光看最后一个字符是不是 `"`
/// 会被 `"abc\"`（结尾是转义引号）骗过。
pub fn quoted_string_close<T: Copy + Into<u32>>(s: &[T], start: usize) -> Option<usize> {
    let n = s.len();
    let at = |i: usize| -> u32 { s[i].into() };
    let mut i = start + 1;
    while i < n {
        let c = at(i);
        if c == '\\' as u32 {
            i += 2;
            continue;
        }
        if c == '"' as u32 {
            return Some(i);
        }
        if c == '$' as u32 && i + 1 < n && at(i + 1) == '{' as u32 {
            i = interpolation_hole_end(s, i + 2);
            continue;
        }
        i += 1;
    }
    None
}

/// 插值洞的结尾：`start` 指向 `${` 之后的第一个字符，返回配对 `}` **之后**的下标；
/// 没配上时返回 `s.len()`。
///
/// 洞里是代码：`{` `}` 计深度（结构体字面量、块），字符串 / 反引号串 / 字符字面量
/// 整段跳过 —— 里面的括号和引号都不算数。代码里本不该出现的反斜杠按「转义对」
/// 跳两个：`${f(\"a\")}` 这种写错的洞至少不会让扫描越过外层串跑到文件别处去，
/// 报错由插值改写那一步给出。
pub fn interpolation_hole_end<T: Copy + Into<u32>>(s: &[T], start: usize) -> usize {
    interpolation_hole_close(s, start).map_or(s.len(), |k| k + 1)
}

/// 同 `interpolation_hole_end`，但返回**配对 `}` 本身**的下标；没配上返回 `None`。
/// `${新建 点 {x}` 这种少一个 `}` 的洞，最后一个字符恰好是 `}` —— 只看它会误判成闭合。
pub fn interpolation_hole_close<T: Copy + Into<u32>>(s: &[T], start: usize) -> Option<usize> {
    let n = s.len();
    let at = |i: usize| -> u32 { s[i].into() };
    let mut depth = 1usize;
    let mut i = start;
    while i < n {
        let c = at(i);
        if c == '\\' as u32 {
            i += 2;
        } else if c == '"' as u32 {
            i = quoted_string_end(s, i);
        } else if c == '`' as u32 {
            i = raw_string_end(s, i);
        } else if c == '\'' as u32 {
            i = char_literal_end(s, i);
        } else if c == '{' as u32 {
            depth += 1;
            i += 1;
        } else if c == '}' as u32 {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    None
}

/// 反引号原始串的结尾：`start` 指向开反引号，返回闭反引号之后的下标。原始串不认转义。
pub fn raw_string_end<T: Copy + Into<u32>>(s: &[T], start: usize) -> usize {
    let n = s.len();
    let mut i = start + 1;
    while i < n && s[i].into() != '`' as u32 {
        i += 1;
    }
    (i + 1).min(n)
}

/// 字符字面量 `'x'` / `'\n'` / `'中'` 的结尾。
///
/// 喂字节时一个中文字符占 3 个元素，所以不按「开引号 + 1 或 2 个元素 + 闭引号」数，
/// 而是向后找最近的未转义 `'`，最多看 8 个元素（`'中'` 是 5 字节，`'\u{..}'` 不支持）。
/// 找不到就当这个 `'` 是普通字符，只前进一格 —— 宁可少跳，也别吞掉后面的代码。
pub fn char_literal_end<T: Copy + Into<u32>>(s: &[T], start: usize) -> usize {
    let n = s.len();
    let mut i = start + 1;
    let limit = (start + 9).min(n);
    while i < limit {
        let c = s[i].into();
        if c == '\\' as u32 {
            i += 2;
            continue;
        }
        if c == '\'' as u32 {
            return i + 1;
        }
        i += 1;
    }
    start + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    /// 同一段源码按字符扫、按字节扫，得到的串尾必须指向同一处。
    fn end_both(src: &str) -> String {
        let c = chars(src);
        let ce = quoted_string_end(&c, 0);
        let b = src.as_bytes();
        let be = quoted_string_end(b, 0);
        let by_char: String = c[..ce].iter().collect();
        let by_byte = &src[..be];
        assert_eq!(by_char, by_byte, "字符扫描与字节扫描不一致：{src}");
        by_char
    }

    #[test]
    fn plain_string() {
        assert_eq!(end_both(r#""甲乙" + x"#), r#""甲乙""#);
        assert_eq!(end_both(r#""带\"转义" + x"#), r#""带\"转义""#);
    }

    #[test]
    fn literal_inside_hole_does_not_end_the_string() {
        assert_eq!(
            end_both(r#""键是 ${取(表, "甲")}" + 尾"#),
            r#""键是 ${取(表, "甲")}""#
        );
        // 洞里的串带花括号和转义引号
        assert_eq!(end_both(r#""${f("}{\"")}" 尾"#), r#""${f("}{\"")}""#);
        // 洞里是反引号 JSON
        assert_eq!(
            end_both(r#""${比(`{"a":1}`)}" 尾"#),
            r#""${比(`{"a":1}`)}""#
        );
        // 洞里是字符字面量（含引号、含中文）
        assert_eq!(
            end_both(r#""${f('"')}${g('中')}" 尾"#),
            r#""${f('"')}${g('中')}""#
        );
    }

    #[test]
    fn braces_in_hole_code_nest() {
        assert_eq!(
            end_both(r#""${新建 点 { 横: 1 }.横}" 尾"#),
            r#""${新建 点 { 横: 1 }.横}""#
        );
    }

    #[test]
    fn unterminated_runs_to_end() {
        let src = r#""没有闭引号 ${x"#;
        assert_eq!(quoted_string_end(&chars(src), 0), chars(src).len());
        assert_eq!(quoted_string_end(src.as_bytes(), 0), src.len());
    }

    #[test]
    fn escaped_quote_in_hole_stays_inside_outer_string() {
        // 写错的洞（洞里用了 \"）不能让扫描跑出外层串
        assert_eq!(end_both(r#""${f(\"a\")}" 尾"#), r#""${f(\"a\")}""#);
    }

    #[test]
    fn close_variants_tell_eof_from_closed() {
        // 结尾是转义引号：没闭合
        assert_eq!(quoted_string_close(&chars(r#""abc\""#), 0), None);
        assert_eq!(quoted_string_close(&chars(r#""abc""#), 0), Some(4));
        // 洞少一个 }，最后一个字符却恰好是 }
        let h = chars("新建 点 {x}");
        assert_eq!(interpolation_hole_close(&h, 0), None);
        let h2 = chars("新建 点 {x}}尾");
        assert_eq!(interpolation_hole_close(&h2, 0), Some(8)); // 新0 建1 ␠2 点3 ␠4 {5 x6 }7 }8
    }

    #[test]
    fn dollar_without_brace_is_text() {
        assert_eq!(end_both(r#""价格 $5" 尾"#), r#""价格 $5""#);
        assert_eq!(end_both(r#""字面 \${x}" 尾"#), r#""字面 \${x}""#);
    }
}
