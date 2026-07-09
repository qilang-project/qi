//! 闭包自引用环静态检测（赋值点分析）—— 第 2 座山：把 ARC 引用环的最隐蔽口子焊死。
//!
//! 纯引用计数（ARC）无环收集器：`按钮.回调 = 闭包 { 按钮.响应() }` 这类
//! 「结构体持有闭包字段、闭包又强捕获该结构体」会让双方计数永不归零 → 永久泄漏。
//! 结构体/枚举/数组的静态环由 环检测.rs 处理；闭包环是它诚实标注的唯一缺口，
//! 本模块补上。
//!
//! ## 方案：精确赋值点分析（非保守近似）
//!
//! 只认一种**语法上 100% 确定成环**的形态：
//! ```text
//!   OBJ.字段 = 闭包 { …引用 OBJ… }
//! ```
//! 即赋值目标是 `标识符.字段`，右值是闭包字面量，且闭包体自由引用了同一个
//! 根变量 OBJ（`自身.回调 = 闭包 { 自身.做事() }` 是其经典特例，OBJ = 自身）。
//! 这条链是真环：OBJ 的字段强持有闭包，闭包又强捕获 OBJ。
//!
//! **为什么选精确赋值点分析而非「有函数值字段 + 存在捕获同类型闭包」的保守近似？**
//! 保守版会对「结构体有回调字段」这种极常见形态大面积误报，淹没真信号；
//! 赋值点分析直击 `self.callback = 闭包{self…}` 经典场景，误报率≈0
//! （闭包字面地写出了它捕获的那个持有者），代价是漏掉「闭包先存局部再赋值」
//! 这类间接形态（宁可少报精确的真环，也不误报——见文件末尾局限）。
//!
//! ## 编译期强制（焊死）
//!
//! 检出的**强**自引用环（未用 `弱` 打破）→ **编译错误**，要求改用
//! `闭包 [弱 OBJ] { … }` 弱捕获。已写 `弱 OBJ` 的：环已断，放行。
//! 这不受 QI_LINT 门控——它是确定性正确性错误、修复方式单一明确。

use crate::parser::ast::{AstNode, ClosureExpression, Program};
use std::collections::HashSet;

/// 一条闭包自引用环发现。
pub(super) struct 闭包环发现 {
    /// 被赋值的对象根变量名（也是闭包捕获的那个），如 `按钮` / `自身`。
    pub 对象: String,
    /// 持有闭包的字段名，如 `回调`。
    pub 字段: String,
    /// 所在函数 / 方法名（诊断定位用）。
    pub 位置: String,
    /// 是否已用 `弱` 打破（true = 弱捕获，环已断，不算真环 / 不报错）。
    pub 已弱: bool,
}

/// 扫全部模块，收集闭包自引用环发现（强 + 弱都收，调用方按 `已弱` 区分）。
pub(super) fn 收集闭包环(programs: &[Program]) -> Vec<闭包环发现> {
    let mut 发现 = Vec::new();
    for p in programs {
        for 语句 in &p.statements {
            扫顶层声明(语句, &mut 发现);
        }
    }
    发现
}

/// 只返回**强**环（未弱化）的规范化描述串，供 doctor 静态段与编译期错误共用口径。
/// 形如 `按钮.回调 ↺ 闭包捕获 按钮`。
pub(super) fn 收集闭包强环链(programs: &[Program]) -> Vec<String> {
    let mut 链: Vec<String> = 收集闭包环(programs)
        .into_iter()
        .filter(|f| !f.已弱)
        .map(|f| format!("{}.{} ↺ 闭包捕获 {}", f.对象, f.字段, f.对象))
        .collect();
    链.sort();
    链.dedup();
    链
}

// ───────────── 顶层声明分发 ─────────────

fn 扫顶层声明(node: &AstNode, 发现: &mut Vec<闭包环发现>) {
    match node {
        AstNode::函数声明(f) => 扫体(&f.body, &f.name, 发现),
        AstNode::方法声明(m) => {
            扫体(&m.body, &方法位置(&m.receiver_type, &m.method_name), 发现)
        }
        AstNode::结构体声明(s) => {
            for m in &s.methods {
                扫体(&m.body, &方法位置(&s.name, &m.method_name), 发现);
            }
        }
        AstNode::实现块(b) => {
            for m in &b.methods {
                扫体(&m.body, &方法位置(&m.receiver_type, &m.method_name), 发现);
            }
        }
        AstNode::导出函数(ef) => 扫顶层声明(&ef.decl, 发现),
        _ => {}
    }
}

fn 方法位置(类型: &str, 方法: &str) -> String {
    format!("{}.{}", 类型, 方法)
}

fn 扫体(body: &[AstNode], 位置: &str, 发现: &mut Vec<闭包环发现>) {
    for s in body {
        扫节点(s, 位置, 发现);
    }
}

// ───────────── 递归找赋值点 ─────────────

/// 递归遍历任意节点：命中 `OBJ.字段 = 闭包{…}` 就判环，其余下钻子节点。
fn 扫节点(node: &AstNode, 位置: &str, 发现: &mut Vec<闭包环发现>) {
    if let AstNode::赋值表达式(a) = node {
        if let (AstNode::字段访问表达式(fa), AstNode::闭包表达式(c)) =
            (a.target.as_ref(), a.value.as_ref())
        {
            if let AstNode::标识符表达式(根) = fa.object.as_ref() {
                if 闭包引用名(c, &根.name) {
                    发现.push(闭包环发现 {
                        对象: 根.name.clone(),
                        字段: fa.field.clone(),
                        位置: 位置.to_string(),
                        已弱: c.weak_captures.iter().any(|n| n == &根.name),
                    });
                }
            }
        }
    }
    for 子 in 子节点(node) {
        扫节点(子, 位置, 发现);
    }
}

// ───────────── 闭包体是否自由引用某名字 ─────────────

/// 闭包体是否自由引用变量 `name`（受闭包自身参数 + 顶层局部声明屏蔽）。
fn 闭包引用名(c: &ClosureExpression, name: &str) -> bool {
    let mut 屏蔽: HashSet<String> = c.parameters.iter().map(|p| p.name.clone()).collect();
    for s in &c.body {
        if let AstNode::变量声明(vd) = s {
            屏蔽.insert(vd.name.clone());
        }
    }
    if 屏蔽.contains(name) {
        return false;
    }
    let mut 命中 = false;
    for s in &c.body {
        查引用(s, name, &屏蔽, &mut 命中);
        if 命中 {
            break;
        }
    }
    命中
}

/// 递归查 `name` 是否作为标识符 / 字段接收者 / 方法接收者 / fat-call callee 出现。
/// 遇嵌套闭包：把其参数并入屏蔽后继续下钻（外层名穿透嵌套闭包仍算引用）。
fn 查引用(node: &AstNode, name: &str, 屏蔽: &HashSet<String>, 命中: &mut bool) {
    if *命中 {
        return;
    }
    match node {
        AstNode::标识符表达式(id) => {
            if id.name == name && !屏蔽.contains(&id.name) {
                *命中 = true;
            }
        }
        // fat 调用：callee 是被捕获的函数值变量（`回调()` 里 回调 == name）
        AstNode::函数调用表达式(call) => {
            if call.callee == name && !屏蔽.contains(&call.callee) {
                *命中 = true;
                return;
            }
            for a in &call.arguments {
                查引用(a, name, 屏蔽, 命中);
            }
        }
        AstNode::闭包表达式(c) => {
            let mut 内屏蔽 = 屏蔽.clone();
            for p in &c.parameters {
                内屏蔽.insert(p.name.clone());
            }
            for s in &c.body {
                查引用(s, name, &内屏蔽, 命中);
            }
        }
        _ => {
            for 子 in 子节点(node) {
                查引用(子, name, 屏蔽, 命中);
            }
        }
    }
}

// ───────────── 通用子节点枚举 ─────────────

/// 返回一个节点的所有子表达式 / 子语句（用于两趟递归）。
/// 覆盖常见语句/表达式；漏掉的冷门形态只会漏报（宁可少报，不误报/不崩）。
fn 子节点(node: &AstNode) -> Vec<&AstNode> {
    let mut v: Vec<&AstNode> = Vec::new();
    match node {
        AstNode::变量声明(vd) => {
            if let Some(init) = &vd.initializer {
                v.push(init);
            }
        }
        AstNode::如果语句(i) => {
            v.push(&i.condition);
            v.extend(i.then_branch.iter());
            if let Some(e) = &i.else_branch {
                v.push(e);
            }
        }
        AstNode::循环语句(l) => v.extend(l.body.iter()),
        AstNode::当语句(w) => {
            v.push(&w.condition);
            v.extend(w.body.iter());
        }
        AstNode::对于语句(f) => {
            v.push(&f.range);
            v.extend(f.body.iter());
        }
        AstNode::返回语句(r) => {
            if let Some(x) = &r.value {
                v.push(x);
            }
        }
        AstNode::表达式语句(e) => v.push(&e.expression),
        AstNode::块语句(b) => v.extend(b.statements.iter()),
        AstNode::尝试语句(t) => {
            v.extend(t.try_body.iter());
            for c in &t.catch_clauses {
                v.extend(c.body.iter());
            }
            if let Some(fb) = &t.finally_body {
                v.extend(fb.iter());
            }
        }
        AstNode::抛出语句(t) => v.push(&t.expression),
        AstNode::二元操作表达式(b) => {
            v.push(&b.left);
            v.push(&b.right);
        }
        AstNode::一元操作表达式(u) => v.push(&u.operand),
        AstNode::类型转换表达式(t) => v.push(&t.expression),
        AstNode::函数调用表达式(c) => v.extend(c.arguments.iter()),
        AstNode::等待表达式(a) => v.push(&a.expression),
        AstNode::协程启动表达式(g) => v.push(&g.expression),
        AstNode::赋值表达式(a) => {
            v.push(&a.target);
            v.push(&a.value);
        }
        AstNode::数组访问表达式(a) => {
            v.push(&a.array);
            v.push(&a.index);
        }
        AstNode::数组字面量表达式(a) => v.extend(a.elements.iter()),
        AstNode::区间表达式(r) => {
            v.push(&r.start);
            v.push(&r.end);
        }
        AstNode::字符串连接表达式(s) => {
            v.push(&s.left);
            v.push(&s.right);
        }
        AstNode::结构体实例化表达式(s) => {
            for f in &s.fields {
                v.push(&f.value);
            }
        }
        AstNode::字段访问表达式(f) => v.push(&f.object),
        AstNode::方法调用表达式(m) => {
            v.push(&m.object);
            v.extend(m.arguments.iter());
        }
        AstNode::通道发送表达式(c) => {
            v.push(&c.channel);
            v.push(&c.value);
        }
        AstNode::通道接收表达式(c) => v.push(&c.channel),
        AstNode::取地址表达式(a) => v.push(&a.expression),
        AstNode::解引用表达式(d) => v.push(&d.expression),
        AstNode::闭包表达式(c) => v.extend(c.body.iter()),
        AstNode::匹配表达式(m) => {
            v.push(&m.value);
            for arm in &m.arms {
                if let Some(g) = &arm.guard {
                    v.push(g);
                }
                v.extend(arm.body.iter());
            }
        }
        _ => {}
    }
    v
}
