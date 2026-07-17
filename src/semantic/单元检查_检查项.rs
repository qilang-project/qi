//! 单元检查器·新增检查项（2026-07 波2 召回提升）。
//!
//! 与 单元检查.rs 同属一个检查器（子模块，直接访问其私有状态）。
//! 每一项都遵守父模块的宽容默认与字面量来源门槛，一项一验（四道门全绿）。
//!
//! 本文件承载：
//! - 枚举变体载荷类型检查（元数匹配后按位家族比对，泛型载荷沉默）
//! - 常量重赋值检查（重入声明是新声明，不误伤）
//! - 纯字面量数组的混族元素检查
//! - 模板串洞内检查（只报未定义变量/函数，家族比对不开）
//! - 重载 >1 时的实参家族比对（全部候选同族才开火）

use super::*;

impl 单元检查器 {
    /// 枚举变体载荷的按位类型检查（裸构造与 `枚举.变体(...)` 限定构造共用）。
    ///
    /// 开火条件（全部满足才比对，任一不满足 → 沉默）：
    /// - 实参个数与声明元数一致（元数错已另行报过，不重复）；
    /// - 该位实参是字面量来源且类型已知；
    /// - 该位载荷声明类型不含枚举的泛型类型参数；
    /// - 两侧家族均非「沉默」且不同族。
    pub(super) fn 检查变体载荷类型(
        &mut self,
        枚举名: &str,
        变体名: &str,
        实参型: &[推断],
        实参节点: &[AstNode],
        span: crate::lexer::Span,
    ) {
        let Some(载荷) = self
            .变体载荷
            .get(&(枚举名.to_string(), 变体名.to_string()))
            .cloned()
        else {
            return;
        };
        if 载荷.len() != 实参节点.len() {
            return; // 元数不匹配已在调用方报过
        }
        let 枚举参数: HashSet<String> = self
            .枚举类型参数
            .get(枚举名)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        for (i, 声明) in 载荷.iter().enumerate() {
            if !实参节点
                .get(i)
                .map(|n| self.是字面量来源(n))
                .unwrap_or(false)
            {
                continue;
            }
            let Some(Some(实际)) = 实参型.get(i) else {
                continue;
            };
            // 载荷类型含枚举类型参数（如 盒装<T> 的 T）→ 沉默
            let 旧 = std::mem::replace(&mut self.当前类型参数, 枚举参数.clone());
            let 已知声明 = self.解析类型(声明);
            self.当前类型参数 = 旧;
            let Some(声明) = 已知声明 else {
                continue;
            };
            if !家族相容(&声明, 实际) {
                self.报(TypeError::TypeMismatch {
                    expected: format!(
                        "枚举变体 '{}.{}' 第 {} 个载荷声明为 {}",
                        枚举名,
                        变体名,
                        i + 1,
                        类型显示(&声明)
                    ),
                    actual: format!("实参却是 {}", 类型显示(实际)),
                    span,
                });
            }
        }
    }

    /// 纯字面量数组的混族元素检查：`[1, "a"]` 报；任一元素不是**直接字面量**
    /// （变量/调用/组合表达式…）→ 整体沉默。整数/浮点/布尔同为数值族不报。
    pub(super) fn 检查纯字面量数组(
        &mut self,
        al: &ArrayLiteralExpression,
        各元素型: &[推断],
    ) {
        if al.elements.len() < 2 {
            return;
        }
        // 门槛最严：所有元素都是字面量表达式本体（不含字面量变量/组合式）
        if !al
            .elements
            .iter()
            .all(|e| matches!(e, AstNode::字面量表达式(_)))
        {
            return;
        }
        let mut 首元素: Option<(家族, TypeNode)> = None;
        for (i, t) in 各元素型.iter().enumerate() {
            let Some(t) = t else { return }; // 字面量类型必已知，保险起见
            let f = 类型家族(t);
            if f == 家族::沉默 {
                return;
            }
            match &首元素 {
                None => 首元素 = Some((f, t.clone())),
                Some((f0, t0)) if *f0 != f => {
                    self.报(TypeError::TypeMismatch {
                        expected: format!("数组元素第 1 个是 {}", 类型显示(t0)),
                        actual: format!("第 {} 个却是 {}", i + 1, 类型显示(t)),
                        span: al.span,
                    });
                    return; // 报一次即止，避免长数组刷屏
                }
                Some(_) => {}
            }
        }
    }

    /// 模板串（f"…{洞}…"）洞内检查——逐步开火版。
    ///
    /// parse_format_string 对洞只造「伪标识符」节点（洞原文整串当名字，span
    /// 是 (0,0)）——镜像 codegen `生成格式字符串` 的做法，用 LALRPOP 的
    /// `pub Expr` 入口把洞原文真解析成表达式再走正常推断：
    /// `{真}`/`{3.14}` 得到字面量、`{加一(n)}` 得到调用……解析失败 → 沉默
    /// （codegen 编译时会自己报）。
    ///
    /// 开火面（2026-07 波2，先窄后宽）：只保留洞内的**未定义变量/未定义函数**
    /// ——这两类与洞外同样有把握；家族比对等其余报错全部丢弃（302 语料模板串
    /// 使用极多，宽面开火易爆误报）。
    ///
    /// span：洞内节点的 span 是洞文本内的相对偏移（ExprParser 从 0 起算），
    /// 换算不回原文件 → 统一改指模板串本体（grammar 给 f.span 回填了真值）。
    pub(super) fn 检查模板串(&mut self, f: &FormatStringExpression) {
        let 起 = self.错误.len();
        for part in &f.parts {
            if let FormatStringPart::表达式 { expr, .. } = part {
                match expr.as_ref() {
                    AstNode::标识符表达式(id) => {
                        if let Ok(节) = crate::parser::ExprParser::new().parse(&id.name) {
                            self.推断表达式(&节);
                        }
                    }
                    other => {
                        self.推断表达式(other);
                    }
                }
            }
        }
        let 收 = self.错误.split_off(起);
        for mut e in 收 {
            let 留 = match &e {
                TypeError::UndefinedVariable { .. } => true,
                TypeError::FunctionCallError { message, .. } => message.contains("未定义的函数"),
                _ => false,
            };
            if 留 {
                e.设span(f.span);
                self.错误.push(e);
            }
        }
    }

    /// 元数命中 >1 个候选时的实参比对（谨慎开火版）。
    ///
    /// 某实参位开火需同时满足（任一不满足 → 该位沉默）：
    /// - 实参是字面量来源且类型已知、家族非沉默；
    /// - **每个**候选在该位都有形参（不是变参位、没有越界吃默认值的歧义）、
    ///   形参类型已知（不含该候选的泛型参数）、家族非沉默；
    /// - 所有候选在该位期望的家族一致，且与实参家族不同。
    pub(super) fn 检查多候选实参(
        &mut self,
        名: &str,
        匹配: &[&函数签名],
        实参型: &[推断],
        实参节点: &[AstNode],
        span: crate::lexer::Span,
    ) {
        let 族名 = |f: 家族| match f {
            家族::数值 => "数值",
            家族::字符串族 => "字符串",
            家族::容器 => "容器",
            家族::沉默 => "未知",
        };
        for (i, at) in 实参型.iter().enumerate() {
            if !实参节点
                .get(i)
                .map(|n| self.是字面量来源(n))
                .unwrap_or(false)
            {
                continue;
            }
            let Some(at) = at else { continue };
            let 实参族 = 类型家族(at);
            if 实参族 == 家族::沉默 {
                continue;
            }
            let mut 期望族: Option<家族> = None;
            let mut 有把握 = true;
            for sig in 匹配 {
                let Some(p) = sig.参数.get(i) else {
                    有把握 = false;
                    break;
                };
                if p.is_variadic {
                    有把握 = false;
                    break;
                }
                let 旧 = std::mem::replace(
                    &mut self.当前类型参数,
                    sig.类型参数.iter().cloned().collect(),
                );
                let 形参型 = p.type_annotation.as_ref().and_then(|t| self.解析类型(t));
                self.当前类型参数 = 旧;
                let Some(pt) = 形参型 else {
                    有把握 = false;
                    break;
                };
                let pf = 类型家族(&pt);
                if pf == 家族::沉默 {
                    有把握 = false;
                    break;
                }
                match 期望族 {
                    None => 期望族 = Some(pf),
                    Some(f0) if f0 == pf => {}
                    Some(_) => {
                        有把握 = false;
                        break;
                    }
                }
            }
            if let (true, Some(f0)) = (有把握, 期望族) {
                if f0 != 实参族 {
                    self.报(TypeError::TypeMismatch {
                        expected: format!(
                            "函数 '{}' 第 {} 个参数各重载均期望{}类型",
                            名,
                            i + 1,
                            族名(f0)
                        ),
                        actual: format!("实参却是 {}", 类型显示(at)),
                        span,
                    });
                }
            }
        }
    }
}
