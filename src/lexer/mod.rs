//! 词法层留下的公共类型与关键字表。
//!
//! ## 这里曾经有一个词法分析器，它不干活
//!
//! 原来 `mod.rs` 有 1333 行的 `Lexer`：扫描源码产出 token 流。但 `Parser::parse`
//! 拿到那串 token 之后做的第一件事是**把源码从 token 重组回字符串**（token 之间
//! 的空隙用空格垫回原字节偏移），再交给 `parse_source` —— 而 `parse_source` 用的
//! 是 LALRPOP 自带的词法器，从头重新词法一遍。
//!
//! 也就是说整条链路是 `源码 → token → 源码 → token`，中间那一趟是恒等变换。
//! 而它还顺手带进来三个问题：
//!
//! - **列号在含中文的行上是错的**。它按显示宽度算列（汉字算 2），编辑器和
//!   LALRPOP 都按字符算，于是一行里每多一个汉字就偏一列。
//! - **未闭合块注释被静默吃掉**，整个文件变成空程序，不报错。
//!   （原代码自己的注释写着「this would be an error in a real implementation」。）
//! - **它私自吞掉 `。！？《》`**，于是 `qi run` 接受的程序 `qi check` 会拒绝 ——
//!   同一门语言两套方言。全仓 2189 个 .qi 实测零依赖这个行为。
//!
//! 那套带修复建议的富诊断（`report_invalid_character_error` 等）**从来没在用户
//! 面前出现过** —— 生产路径只取 `LexicalError` 的 Display 一行字，富诊断只有
//! 测试读过。
//!
//! ## 为什么模块名还叫 lexer
//!
//! 下面这三样跟词法分析毫无关系，只是历史遗留在这个目录下：
//!
//! - `Span` —— 整个 AST 的位置类型，值全部来自 LALRPOP 的 `@L/@R`
//! - `TokenKind` —— 现在只是「关键字分类标签」，给 keywords 表和 parser/error 用
//! - `KEYWORDS` —— qi-lsp 的关键字单一事实源（`tests/关键字表一致性.rs` 约束它
//!   必须 ⊆ grammar.lalrpop 的字面量集）
//!
//! qi-lsp 有 11 处 `qi_compiler::lexer::…`。保留这个模块路径，那边零改动。

pub mod keywords;
pub mod tokens;

pub use tokens::{Span, Token, TokenKind};
