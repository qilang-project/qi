//! 关键字表防漂移：手写 lexer 的 KeywordTable 词集必须 ⊆ grammar.lalrpop 字面量集。
//!
//! 2026-08 精简保留字时发现两表漂移了十几个词：keywords.rs 里有而语法没有的
//! （参数/重试/协程/等待组…）不偷标识符但误导工具链；语法有而 keywords.rs
//! 没有的（默认/结果/直到…）让报错提示失真。真正的事实源是 grammar.lalrpop
//! 的字面量终结符 —— 本测试挡住"往 keywords.rs 加词却忘了语法"的那一半；
//! 反方向（语法加词忘了表）只影响诊断质量，不强制。

use std::collections::HashSet;

/// 从 grammar.lalrpop 抽出全部中文字面量终结符。
/// 按行切分再按 `"` 配对 —— 全局配对会被注释里的落单引号带偏（跳出 曾因此漏抽）。
fn grammar_literals() -> HashSet<String> {
    let src = include_str!("../src/parser/grammar.lalrpop");
    let mut out = HashSet::new();
    for line in src.lines() {
        // 行内注释里的引号不算
        let code = line.split("//").next().unwrap_or("");
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
fn keywords_rs_是语法字面量的子集() {
    let grammar = grammar_literals();
    assert!(
        grammar.len() > 30,
        "grammar.lalrpop 里只抽到 {} 个中文字面量，抽取逻辑坏了？",
        grammar.len()
    );
    let table = qi_compiler::lexer::keywords::KEYWORDS.all_keywords();
    let stale: Vec<_> = table
        .iter()
        .filter(|k| !grammar.contains(k.as_str()))
        .collect();
    assert!(
        stale.is_empty(),
        "keywords.rs 里这些词在 grammar.lalrpop 里没有对应字面量（幽灵关键字，删掉或先加语法）：{stale:?}"
    );
}

#[test]
fn 英文调试关键字已清除() {
    for w in ["let", "print", "true", "false"] {
        assert!(
            !qi_compiler::lexer::keywords::KEYWORDS.is_keyword(w),
            "英文调试关键字 {w} 又回来了 —— 它们从来没真正工作过（let 直接语法错误、true/false codegen 报未声明变量），别加回来"
        );
    }
}
