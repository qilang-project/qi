//! 保留字防漂移：`parser/reserved.rs` 的 RESERVED_WORDS 必须**恰好等于**
//! `grammar.lalrpop` 里的中文终结符集，`lexer/keywords.rs` 必须是它的子集。
//!
//! 为什么要双向：2026-08 只强制了单向（keywords.rs ⊆ 语法），于是「语法有、
//! 表里没有」的十来个词长期让报错提示失真。2026-09 放开十三个词时又发现
//! 全仓一共有**三份**手抄的保留字清单（keywords.rs、grammar 里两条 if 链、
//! parser/mod.rs 的 RESERVED_LANDMINES），其中两份已经过期，会对已经合法的
//! 名字继续报「是保留字」。现在只剩一份，本测试钉住它。

use std::collections::HashSet;

/// 从 grammar.lalrpop 抽出全部中文**终结符**字面量。
///
/// 三条排除，每条都对应一个抽错过的东西：
///   1. 行内注释里的引号（`// 见 "包"`）
///   2. `=>` 之后的动作代码（`=> "长度".to_string()`）
///   3. Rust 侧的字符串（`error: "匹配模式里的浮点数字面量无法解析"`）
fn grammar_terminals() -> HashSet<String> {
    let src = include_str!("../src/parser/grammar.lalrpop");
    let mut out = HashSet::new();
    for line in src.lines() {
        let mut code = line.split("//").next().unwrap_or("");
        if let Some(i) = code.find("=>") {
            code = &code[..i];
        }
        if code.contains("id ==") || code.contains("error:") {
            continue;
        }
        for (i, seg) in code.split('"').enumerate() {
            if i % 2 == 1
                && !seg.is_empty()
                && seg.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
            {
                out.insert(seg.to_string());
            }
        }
    }
    out
}

#[test]
fn 词表与语法终结符完全一致() {
    let grammar = grammar_terminals();
    assert!(
        grammar.len() > 30,
        "只从 grammar.lalrpop 抽到 {} 个中文终结符，抽取逻辑坏了？",
        grammar.len()
    );
    let table: HashSet<String> = qi_compiler::parser::reserved::RESERVED_WORDS
        .iter()
        .map(|s| s.to_string())
        .collect();

    let mut 表里多的: Vec<_> = table.difference(&grammar).cloned().collect();
    let mut 语法多的: Vec<_> = grammar.difference(&table).cloned().collect();
    表里多的.sort();
    语法多的.sort();
    assert!(
        表里多的.is_empty(),
        "reserved.rs 里这些词语法根本没有 —— 白占用户的名字：{:?}\n\
         语法里删掉一个终结符后，记得同步 reserved.rs。",
        表里多的
    );
    assert!(
        语法多的.is_empty(),
        "语法里这些终结符没进 reserved.rs —— 报错提示会说不出「是保留字」：{:?}",
        语法多的
    );
}

#[test]
fn keywords_rs_是保留字的子集() {
    let reserved: HashSet<String> = qi_compiler::parser::reserved::RESERVED_WORDS
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut stale: Vec<_> = qi_compiler::lexer::keywords::KEYWORDS
        .all_keywords()
        .into_iter()
        .filter(|k| !reserved.contains(k))
        .collect();
    stale.sort();
    assert!(
        stale.is_empty(),
        "keywords.rs 里这些词已经不是保留字了，删掉它们（否则手写 lexer 会把\
         合法名字标成关键字，LSP 高亮和格式化都跟着错）：{:?}",
        stale
    );
}
