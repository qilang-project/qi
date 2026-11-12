//! 返回路径分析：声明了非 `空` 返回类型的函数/方法，是否有路径走到函数体结尾
//! 却没有 `返回`。
//!
//! codegen 在函数落底时补一条「返回零值」（inkwell_gen/声明.rs），所以漏写
//! `返回` 的函数**编译通过、静默返回 0 / 空串 / 空指针**——
//! `函数 绝对值(x: 整数): 整数 { 如果 x < 0 { 返回 -x; } }` 对 5 返回 0。
//!
//! 判定是「语句序列是否必然离开」（终结）：
//! - `返回` / `抛出` 终结；
//! - `如果` 只有在 then 与 否则 都终结时终结（`否则如果` 链递归）；
//! - `匹配` / `选择` 所有分支都终结时终结（不要求穷尽 —— 宽容，与文档一致）；
//! - `当 真 {}`（及 AST 里的无条件循环）里没有指向它的 `跳出` → 永不落出，视为终结；
//! - `尝试`：try 体与全部 捕获 都终结，或 最终 块终结；
//! - 调用名为 `退出` 的函数（进程.退出 等）视为终结。
//!
//! 宁可漏报不可误报：拿不准的结构（闭包、异步块里的跳出等）一律往「终结」
//! 那边靠，只在能证明有路径落底时才报。

use super::单元检查器;
use crate::lexer::Span;
use crate::parser::ast::*;
use crate::semantic::type_checker::TypeError;
use crate::semantic::诊断渲染::类型显示;

impl 单元检查器 {
    /// 声明了非 `空` 返回类型、却有路径走到结尾没 `返回` → 报错。
    /// codegen 会在落底处补「返回零值」，不拦就是静默返回 0。
    pub(super) fn check_missing_return(
        &mut self,
        kind: &str,
        name: &str,
        return_type: &Option<TypeNode>,
        body: &[AstNode],
        span: Span,
    ) {
        if name == "入口" || !requires_return(return_type) {
            return;
        }
        if !falls_off_end(body) {
            return;
        }
        let shown = return_type.as_ref().map(类型显示).unwrap_or_default();
        self.报(TypeError::General {
            message: format!(
                "{} `{}` 声明返回 {}，但有路径走到结尾没有 `返回`",
                kind, name, shown
            ),
            span,
        });
    }
}

/// 函数体是否**有路径落到结尾**（= 需要报错的情形）。
fn falls_off_end(body: &[AstNode]) -> bool {
    !block_terminates(body)
}

/// 返回类型是否要求显式 `返回`：未声明 / `空` / `未来<空>` 不要求。
fn requires_return(return_type: &Option<TypeNode>) -> bool {
    match return_type {
        None => false,
        Some(t) => !is_void(t),
    }
}

fn is_void(t: &TypeNode) -> bool {
    match t {
        TypeNode::基础类型(BasicType::空) => true,
        TypeNode::自定义类型(name) => name == "空",
        TypeNode::未来类型(inner) => is_void(inner),
        _ => false,
    }
}

fn block_terminates(stmts: &[AstNode]) -> bool {
    stmts.iter().any(stmt_terminates)
}

fn stmt_terminates(node: &AstNode) -> bool {
    match node {
        AstNode::返回语句(_) | AstNode::抛出语句(_) => true,
        AstNode::块语句(b) => block_terminates(&b.statements),
        AstNode::如果语句(s) => {
            block_terminates(&s.then_branch)
                && s.else_branch.as_deref().is_some_and(stmt_terminates)
        }
        AstNode::循环语句(s) => !contains_break(&s.body),
        AstNode::当语句(s) => is_always_true(&s.condition) && !contains_break(&s.body),
        AstNode::尝试语句(s) => {
            let finally_exits = s.finally_body.as_deref().is_some_and(block_terminates);
            let try_exits = block_terminates(&s.try_body)
                && s.catch_clauses.iter().all(|c| block_terminates(&c.body));
            finally_exits || try_exits
        }
        AstNode::表达式语句(e) => stmt_terminates(&e.expression),
        AstNode::匹配表达式(m) => {
            !m.arms.is_empty() && m.arms.iter().all(|arm| block_terminates(&arm.body))
        }
        AstNode::选择表达式(s) => {
            let mut arms = s
                .cases
                .iter()
                .chain(s.default_case.iter())
                .chain(s.timeout_case.iter())
                .peekable();
            arms.peek().is_some() && arms.all(|c| block_terminates(&c.body))
        }
        AstNode::函数调用表达式(call) => is_exit_name(&call.callee),
        AstNode::方法调用表达式(call) => is_exit_name(&call.method_name),
        _ => false,
    }
}

/// 进程.退出(码) 之类：调用后不回来。只按名字认 —— 认错（用户自己的
/// `退出` 方法其实会返回）只会少报，不会误报。
fn is_exit_name(name: &str) -> bool {
    matches!(name, "退出" | "exit")
}

fn is_always_true(cond: &AstNode) -> bool {
    match cond {
        AstNode::字面量表达式(lit) => match &lit.value {
            LiteralValue::布尔(b) => *b,
            LiteralValue::整数(n) => *n != 0,
            _ => false,
        },
        _ => false,
    }
}

/// 循环体里有没有指向**这一层**循环的 `跳出`。嵌套循环里的跳出属于内层；
/// 闭包 / 异步块是另一个函数体，跳不出来。如果、匹配、选择、尝试、块
/// 里的跳出都指向这一层（codegen 只有循环会压 循环栈）。
fn contains_break(stmts: &[AstNode]) -> bool {
    stmts.iter().any(break_in)
}

fn break_in(node: &AstNode) -> bool {
    match node {
        AstNode::跳出语句(_) => true,
        AstNode::块语句(b) => contains_break(&b.statements),
        AstNode::如果语句(s) => {
            contains_break(&s.then_branch) || s.else_branch.as_deref().is_some_and(break_in)
        }
        AstNode::尝试语句(s) => {
            contains_break(&s.try_body)
                || s.catch_clauses.iter().any(|c| contains_break(&c.body))
                || s.finally_body.as_deref().is_some_and(contains_break)
        }
        AstNode::表达式语句(e) => break_in(&e.expression),
        AstNode::匹配表达式(m) => m.arms.iter().any(|arm| contains_break(&arm.body)),
        AstNode::选择表达式(s) => s
            .cases
            .iter()
            .chain(s.default_case.iter())
            .chain(s.timeout_case.iter())
            .any(|c| contains_break(&c.body)),
        // 内层循环自己的跳出不算；闭包/异步块/嵌套函数是另一个函数体
        _ => false,
    }
}
