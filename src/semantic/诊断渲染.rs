//! 类型检查诊断的人话渲染 —— `qi check` 面向用户的错误文案。
//!
//! 与 compile 路径的调试输出（`[类型检查] 报错: <debug>`，扫描脚本靠它聚合）
//! 严格分离：本模块只服务 `qi check` 的渲染，绝不改动 Debug 格式。

use crate::lexer::Span;
use crate::parser::ast::TypeNode;
use crate::semantic::type_checker::TypeError;

/// 类型的人话显示：`基础类型(整数)` 调试串 → 「整数」；容器/泛型递归展开。
pub fn 类型显示(t: &TypeNode) -> String {
    match t {
        TypeNode::基础类型(b) => format!("{:?}", b), // BasicType 变体名即中文
        TypeNode::自定义类型(n) => n.clone(),
        TypeNode::数组类型(a) => format!("数组<{}>", 类型显示(&a.element_type)),
        TypeNode::列表类型(l) => format!("列表<{}>", 类型显示(&l.element_type)),
        TypeNode::集合类型(s) => format!("集合<{}>", 类型显示(&s.element_type)),
        TypeNode::字典类型(d) => format!(
            "字典<{}, {}>",
            类型显示(&d.key_type),
            类型显示(&d.value_type)
        ),
        TypeNode::通道类型(c) => format!("通道<{}>", 类型显示(&c.element_type)),
        TypeNode::指针类型(p) => format!("指针<{}>", 类型显示(&p.target_type)),
        TypeNode::引用类型(r) => format!("引用<{}>", 类型显示(&r.target_type)),
        TypeNode::未来类型(inner) => format!("未来<{}>", 类型显示(inner)),
        TypeNode::选项类型(o) => format!("选项<{}>", 类型显示(&o.inner_type)),
        TypeNode::结果类型(r) => {
            format!("结果<{}, {}>", 类型显示(&r.ok_type), 类型显示(&r.err_type))
        }
        TypeNode::泛型类型(g) => format!(
            "{}<{}>",
            g.base_type,
            g.type_arguments
                .iter()
                .map(类型显示)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeNode::函数类型(_) => "函数".to_string(),
        other => format!("{:?}", other), // 兜底：未知节点仍可读
    }
}

impl TypeError {
    /// 统一取 span（各变体都带）。
    pub fn span(&self) -> Span {
        match self {
            TypeError::TypeMismatch { span, .. }
            | TypeError::UndefinedVariable { span, .. }
            | TypeError::InvalidOperation { span, .. }
            | TypeError::FunctionCallError { span, .. }
            | TypeError::General { span, .. } => *span,
        }
    }

    /// 覆写 span（模板串洞内错误：洞节点 span 是 (0,0)/洞内相对偏移，
    /// 统一改指外层模板串本体）。
    pub fn 设span(&mut self, s: Span) {
        match self {
            TypeError::TypeMismatch { span, .. }
            | TypeError::UndefinedVariable { span, .. }
            | TypeError::InvalidOperation { span, .. }
            | TypeError::FunctionCallError { span, .. }
            | TypeError::General { span, .. } => *span = s,
        }
    }

    /// 人话渲染（`qi check` 用）。
    ///
    /// TypeMismatch 约定：单元检查器在建错时把上下文写进 expected（如
    /// 「变量 `x` 声明为 整数」），把带「却是」的实际情况写进 actual（如
    /// 「初值却是 字符串」）——两段直接拼接成完整句子；不带该约定的旧式
    /// 错误退化为通用文案。
    pub fn 渲染人话(&self) -> String {
        match self {
            TypeError::TypeMismatch {
                expected, actual, ..
            } => {
                if actual.contains("却是") {
                    format!("{}，{}", expected, actual)
                } else {
                    format!("类型不匹配: 期望 {}, 实际 {}", expected, actual)
                }
            }
            TypeError::UndefinedVariable { name, .. } => {
                format!("未定义的变量 `{}`", name)
            }
            TypeError::InvalidOperation {
                operation,
                type_name,
                ..
            } => format!("无效的操作: {} 对于类型 {}", operation, type_name),
            TypeError::FunctionCallError { message, .. } | TypeError::General { message, .. } => {
                message.clone()
            }
        }
    }
}

/// 编译路径（`qi run` / `qi compile`）面向用户的一条诊断：
///
/// ```text
/// 错误：e1.qi:3:15 类型不匹配：函数 '平方' 第 1 个参数期望 整数，实参却是 字符串
/// ```
///
/// 以前是 `[类型检查] 报错: TypeMismatch /绝对/路径/e1.qi:3:15 …`，后面再跟一句
/// 「代码生成错误: 类型检查失败 …误报是检查器的 bug，开 issue」—— 初学者读到的是
/// 「编译器坏了」，而这其实是他自己代码里的一个普通错误。
pub fn render_diagnostic_line(
    severity: &str,
    error: &TypeError,
    location: Option<(&str, &str)>,
) -> String {
    let mut text: String = error.渲染人话().chars().take(500).collect();
    // 「…却是…」那种 TypeMismatch 的文案本身没点明是类型问题，补个标签；
    // 旧式文案自带「类型不匹配: 期望 …」，别重复。
    if matches!(error, TypeError::TypeMismatch { actual, .. } if actual.contains("却是")) {
        text = format!("类型不匹配：{}", text);
    }
    let span = error.span();
    // span (0,0) = 该错误类还没接真实位置 → 只给文件名
    match location {
        Some((path, source)) if !(span.start == 0 && span.end == 0) => {
            let (line, column) = crate::parser::位置::偏移转行列(source, span.start);
            format!(
                "{}：{}:{}:{} {}",
                severity,
                display_path(path),
                line,
                column,
                text
            )
        }
        Some((path, _)) => format!("{}：{} {}", severity, display_path(path), text),
        None => format!("{}：{}", severity, text),
    }
}

/// 在当前目录底下的文件显示相对路径（`e1.qi:3:15`），否则原样。
fn display_path(path: &str) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return path.to_string();
    };
    let absolute = std::path::Path::new(path);
    let absolute = absolute
        .canonicalize()
        .unwrap_or_else(|_| absolute.to_path_buf());
    let cwd = cwd.canonicalize().unwrap_or(cwd);
    // 在根目录下跑时剥掉前缀只会剩一个缺了开头 `/` 的怪路径
    if cwd.parent().is_none() {
        return path.to_string();
    }
    match absolute.strip_prefix(&cwd) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ => path.to_string(),
    }
}

/// 误报逃生口的说明，整次编译只说一遍。
pub const FALSE_POSITIVE_HINT: &str =
    "（如果你确定代码没错、是检查器误判，可以先用 QI_TYPECHECK=warn 绕过，并把代码报给我们）";
