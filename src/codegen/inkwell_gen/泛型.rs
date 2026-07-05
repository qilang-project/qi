//! 用户泛型（单态化）—— 泛型函数 / 泛型枚举 / 泛型结构体。
//!
//! 策略：**绑定式单态化**，不做 AST 替换。
//!   - 模板原样进符号表注册表（AST / TypeNode 含 T 占位）；
//!   - 调用点 / 注解处定型：显式类型实参优先，否则从实参逐 T 位置推断
//!     （全部一致才通过，冲突报人话）；
//!   - 实例 = (模板名, 类型实参) → 稳定实例名 `最大$整数`，幂等缓存；
//!   - 函数实例体排进 待合成泛型实例 队列（参照 待合成闭包 模式），
//!     末尾统一生成 —— 体生成期间压 T→实参 绑定，`解析类型` 查栈顶还原具体类型。
//!
//! ARC：实例是普通具体类型（结构体(idx)/装箱枚举(idx)），自动落进现有 RC 纪律；
//! 迟到实例的释放函数由 mod.rs 末尾再跑一遍 弧生成释放函数/弧生成枚举释放函数 补定义。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::{函数签名, 推断表达式类型, 泛型函数模板};
use crate::parser::ast::{AstNode, FunctionCallExpression, FunctionDeclaration};
use inkwell::values::BasicValueEnum;
use std::collections::HashMap;

/// 待合成的泛型函数实例：模板 AST + 实例名 + 类型参数绑定 + 声明包。
pub(super) struct 待合成泛型实例 {
    pub 实例名: String,
    pub 模板: FunctionDeclaration,
    pub 绑定: HashMap<String, Qi类型>,
    pub 包: Option<String>,
}

impl<'ctx> 后端<'ctx> {
    // ───────────────────────── 泛型函数 ─────────────────────────

    /// 泛型函数调用：定型 → 单态实例化（幂等）→ 以实例名走普通函数调用。
    /// callee 不是泛型模板时返回 Ok(None)（调用点继续走普通解析）。
    pub(super) fn 生成泛型函数调用(
        &mut self,
        call: &FunctionCallExpression,
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        let Some(模板) = self.符号.泛型函数模板.get(&call.callee).cloned() else {
            if !call.type_arguments.is_empty() {
                return Err(format!(
                    "{} 不是泛型函数，不能带显式类型实参 <...>",
                    call.callee
                ));
            }
            return Ok(None);
        };

        let 参数名表: Vec<String> = 模板.声明.type_params.clone();

        // 1) 定型：显式类型实参优先，否则从实参推断
        let 实参类型: Vec<Qi类型> = if !call.type_arguments.is_empty() {
            if call.type_arguments.len() != 参数名表.len() {
                return Err(format!(
                    "泛型函数 {} 声明了 {} 个类型参数（<{}>），显式类型实参给了 {} 个",
                    call.callee,
                    参数名表.len(),
                    参数名表.join(", "),
                    call.type_arguments.len()
                ));
            }
            let mut v = Vec::new();
            for tn in &call.type_arguments {
                let t = self.符号.解析类型(tn);
                if t == Qi类型::未知 {
                    return Err(format!(
                        "泛型函数 {} 的显式类型实参无法解析（未定义的类型？）",
                        call.callee
                    ));
                }
                v.push(t);
            }
            v
        } else {
            self.推断类型实参(&call.callee, &模板, &call.arguments)?
        };

        // 2) 单态实例化（幂等）
        let 实例名 = self.实例化泛型函数(&call.callee, &模板, &实参类型)?;

        // 3) 以实例名走普通函数调用（原型/签名已登记）
        let 实例调用 = FunctionCallExpression {
            module_qualifier: call.module_qualifier.clone(),
            callee: 实例名,
            type_arguments: vec![],
            arguments: call.arguments.clone(),
            span: call.span,
        };
        self.生成表达式(&AstNode::函数调用表达式(实例调用))
            .map(Some)
    }

    /// 从调用实参推断类型实参：形参注解是裸类型参数（`a: T`）的位置，
    /// 取实参的推断类型；同一 T 的所有位置必须一致，冲突/推不出报人话。
    fn 推断类型实参(
        &mut self,
        名: &str,
        模板: &泛型函数模板,
        实参: &[AstNode],
    ) -> Result<Vec<Qi类型>, String> {
        let mut 已定: HashMap<String, Qi类型> = HashMap::new();
        for (i, p) in 模板.声明.parameters.iter().enumerate() {
            let Some(crate::parser::ast::TypeNode::自定义类型(tp)) = &p.type_annotation else {
                continue;
            };
            if !模板.声明.type_params.contains(tp) {
                continue;
            }
            let Some(arg) = 实参.get(i) else { continue };
            let at = 推断表达式类型(arg, &self.符号);
            if at == Qi类型::未知 {
                continue; // 推不出的位置先跳过（其他位置可能能定）
            }
            match 已定.get(tp) {
                None => {
                    已定.insert(tp.clone(), at);
                }
                Some(prev) if *prev == at => {}
                Some(prev) => {
                    return Err(format!(
                        "泛型函数 {} 的类型参数 {} 推断冲突：形参 {} 处是 {}，而形参 {} 处是 {}。所有 {} 位置的实参类型必须一致，或用显式类型实参消歧：{}::<类型>(...)",
                        名,
                        tp,
                        模板.声明.parameters[i].name,
                        self.符号.类型名(at),
                        模板
                            .声明
                            .parameters
                            .iter()
                            .find(|q| matches!(&q.type_annotation, Some(crate::parser::ast::TypeNode::自定义类型(x)) if x == tp))
                            .map(|q| q.name.clone())
                            .unwrap_or_default(),
                        self.符号.类型名(*prev),
                        tp,
                        名
                    ));
                }
            }
        }
        let mut 结果 = Vec::new();
        for tp in &模板.声明.type_params {
            match 已定.get(tp) {
                Some(t) => 结果.push(*t),
                None => {
                    return Err(format!(
                        "泛型函数 {} 的类型参数 {} 无法从实参推断。请用显式类型实参：{}::<整数>(...)，或给实参加类型注解",
                        名, tp, 名
                    ));
                }
            }
        }
        Ok(结果)
    }

    /// 单态化一个泛型函数实例：登记签名 + 声明 LLVM 原型 + 排进待合成队列。
    /// 幂等（按实例名缓存）。返回实例名（如 最大$整数）。
    fn 实例化泛型函数(
        &mut self,
        模板名: &str,
        模板: &泛型函数模板,
        实参类型: &[Qi类型],
    ) -> Result<String, String> {
        let 实例名 = self.符号.泛型实例名(模板名, 实参类型);
        self.符号.泛型深度检查(&实例名)?;
        if self.已实例化泛型函数.contains(&实例名) {
            return Ok(实例名);
        }
        self.已实例化泛型函数.insert(实例名.clone());

        let 绑定: HashMap<String, Qi类型> = 模板
            .声明
            .type_params
            .iter()
            .cloned()
            .zip(实参类型.iter().copied())
            .collect();

        // 压绑定解析签名（参数/返回注解里的 T / 盒装<T> 都在此定型）
        self.符号.压类型参数(绑定.clone());
        let 参数类型: Vec<Qi类型> = 模板
            .声明
            .parameters
            .iter()
            .map(|p| {
                let t = p
                    .type_annotation
                    .as_ref()
                    .map(|t| self.符号.解析类型(t))
                    .unwrap_or(Qi类型::整数);
                if p.is_variadic {
                    Qi类型::数组(super::类型::元素类型::从标量(t))
                } else {
                    t
                }
            })
            .collect();
        let 返回类型 = 模板
            .声明
            .return_type
            .as_ref()
            .map(|t| self.符号.解析类型(t))
            .unwrap_or(Qi类型::空);
        self.符号.弹类型参数();

        // LLVM 原型（符号带模板声明包，跨包调用走 函数按包 的全局唯一定位）
        let 参数llvm: Vec<inkwell::types::BasicMetadataTypeEnum> = 参数类型
            .iter()
            .map(|t| {
                self.llvm基础类型(*t)
                    .map(|b| b.into())
                    .unwrap_or_else(|| self.ctx.i64_type().into())
            })
            .collect();
        let fn_type = match self.llvm基础类型(返回类型) {
            Some(rt) => {
                use inkwell::types::BasicType;
                rt.fn_type(&参数llvm, false)
            }
            None => self.ctx.void_type().fn_type(&参数llvm, false),
        };
        let mangled = super::包内符号名(模板.包.as_deref(), &实例名);
        self.module.add_function(&mangled, fn_type, None);

        // 签名登记：包内 + 扁平（与 声明函数原型 同款）
        let sig = 函数签名 {
            参数: 参数类型,
            返回: 返回类型,
        };
        if let Some(pkg) = 模板.包.clone() {
            self.符号
                .函数按包
                .insert((pkg, 实例名.clone()), sig.clone());
        }
        self.符号.函数.entry(实例名.clone()).or_insert(sig);
        // 默认参数值 / 变参标志随实例名复制
        let 默认: Vec<Option<AstNode>> = 模板
            .声明
            .parameters
            .iter()
            .map(|p| p.default_value.as_ref().map(|b| (**b).clone()))
            .collect();
        if 默认.iter().any(|d| d.is_some()) {
            self.符号.函数默认值.entry(实例名.clone()).or_insert(默认);
        }
        if 模板
            .声明
            .parameters
            .last()
            .map(|p| p.is_variadic)
            .unwrap_or(false)
        {
            self.符号.函数变参.insert(实例名.clone());
        }

        // 体排队末尾统一合成（避免污染当前 builder 插入点）
        self.待合成泛型实例.push(待合成泛型实例 {
            实例名: 实例名.clone(),
            模板: 模板.声明.clone(),
            绑定,
            包: 模板.包.clone(),
        });
        Ok(实例名)
    }

    /// 生成所有待合成泛型函数实例的函数体（体内可能再实例化新的泛型 → 循环到清空）。
    /// 与 合成待处理闭包 交替调用（见 mod.rs）。
    pub(super) fn 合成待处理泛型实例(&mut self) -> Result<(), String> {
        while let Some(inst) = self.待合成泛型实例.pop() {
            let 原包 = self.当前包.clone();
            self.设当前包(inst.包.clone());
            self.符号.压类型参数(inst.绑定.clone());
            let mut f = inst.模板.clone();
            f.name = inst.实例名.clone();
            f.type_params = Vec::new();
            let r = self.生成函数体(&f);
            self.符号.弹类型参数();
            self.设当前包(原包);
            r?;
        }
        Ok(())
    }

    // ───────────────────────── 泛型枚举构造 ─────────────────────────

    /// 识别 `盒装.满(x)` / `盒装.空盒` 形态的泛型枚举模板构造。
    /// 接收者是裸标识符（非变量/全局/已登记枚举名）且命中泛型枚举模板的变体名。
    /// 返回 (模板名, 变体名, 实参)。
    pub(super) fn 识别泛型枚举构造(
        &self,
        node: &AstNode,
    ) -> Option<(String, String, Vec<AstNode>)> {
        let (对象, 变体, 实参): (&AstNode, &str, Vec<AstNode>) = match node {
            AstNode::方法调用表达式(mc) => {
                (mc.object.as_ref(), &mc.method_name, mc.arguments.clone())
            }
            AstNode::字段访问表达式(fa) => (fa.object.as_ref(), &fa.field, Vec::new()),
            _ => return None,
        };
        let AstNode::标识符表达式(id) = 对象 else {
            return None;
        };
        if self.变量表.contains_key(&id.name)
            || self.全局变量表.contains_key(&id.name)
            || self.符号.是枚举名(&id.name)
        {
            return None;
        }
        let 模板 = self.符号.泛型枚举模板.get(&id.name)?;
        if !模板.变体.iter().any(|(n, _)| n == 变体) {
            return None;
        }
        Some((id.name.clone(), 变体.to_string(), 实参))
    }

    /// 泛型枚举模板构造降级：定型实例（期望优先，否则按载荷实参反推）→ 复用枚举构造。
    pub(super) fn 生成泛型枚举构造(
        &mut self,
        模板名: &str,
        变体名: &str,
        实参: &[AstNode],
        期望: Option<Qi类型>,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let 模板 = self
            .符号
            .泛型枚举模板
            .get(模板名)
            .cloned()
            .ok_or_else(|| format!("内部错误：{} 不是泛型枚举模板", 模板名))?;
        let (_, 载荷节点) = 模板
            .变体
            .iter()
            .find(|(n, _)| n == 变体名)
            .cloned()
            .ok_or_else(|| format!("泛型枚举 {} 无变体 {}", 模板名, 变体名))?;
        if 实参.len() != 载荷节点.len() {
            return Err(format!(
                "枚举变体 {}.{} 需要 {} 个载荷，实际传入 {} 个",
                模板名,
                变体名,
                载荷节点.len(),
                实参.len()
            ));
        }

        // ① 期望是本模板的实例 → 直接用
        let 前缀 = format!("{}$", 模板名);
        let idx = match 期望 {
            Some(Qi类型::装箱枚举(i)) | Some(Qi类型::枚举(i))
                if self
                    .符号
                    .枚举信息(i)
                    .map(|e| e.名字.starts_with(&前缀))
                    .unwrap_or(false) =>
            {
                i
            }
            _ => {
                // ② 从载荷实参反推：载荷注解是裸 T 的位置取实参推断类型
                let mut 已定: HashMap<String, Qi类型> = HashMap::new();
                for (k, tn) in 载荷节点.iter().enumerate() {
                    let crate::parser::ast::TypeNode::自定义类型(tp) = tn else {
                        continue;
                    };
                    if !模板.类型参数.contains(tp) {
                        continue;
                    }
                    let at = 推断表达式类型(&实参[k], &self.符号);
                    if at == Qi类型::未知 {
                        continue;
                    }
                    match 已定.get(tp) {
                        None => {
                            已定.insert(tp.clone(), at);
                        }
                        Some(prev) if *prev == at => {}
                        Some(prev) => {
                            return Err(format!(
                                "枚举 {}.{} 的类型参数 {} 推断冲突：{} 与 {} 不一致",
                                模板名,
                                变体名,
                                tp,
                                self.符号.类型名(*prev),
                                self.符号.类型名(at)
                            ));
                        }
                    }
                }
                let mut 实参类型 = Vec::new();
                for tp in &模板.类型参数 {
                    match 已定.get(tp) {
                        Some(t) => 实参类型.push(*t),
                        None => {
                            return Err(format!(
                                "{}.{} 的类型参数 {} 无法推断，请给变量 / 返回值加类型注解，例如 : {}<整数>",
                                模板名, 变体名, tp, 模板名
                            ));
                        }
                    }
                }
                self.符号.实例化参数枚举(模板名, &实参类型)?
            }
        };

        let 实例名 = self
            .符号
            .枚举信息(idx)
            .map(|e| e.名字.clone())
            .ok_or_else(|| "泛型枚举实例缺失".to_string())?;
        self.生成枚举构造(&实例名, 变体名, 实参)
    }

    // ───────────────────────── 定型辅助 ─────────────────────────

    /// 该初值表达式是否需要「先生成拿真实类型再定槽」（推断表达式类型 认不出，
    /// 必须携带期望走 生成带期望）：内建/泛型枚举构造子、泛型结构体字面量、
    /// 显式泛型调用或泛型模板函数调用。
    pub(super) fn 是泛型定型表达式(&self, node: &AstNode) -> bool {
        if self.识别泛型枚举构造(node).is_some() {
            return true;
        }
        match node {
            AstNode::函数调用表达式(call) => {
                !call.type_arguments.is_empty() || self.符号.泛型函数模板.contains_key(&call.callee)
            }
            AstNode::结构体实例化表达式(lit) => {
                !lit.type_arguments.is_empty()
                    || self.符号.泛型结构体模板.contains_key(&lit.struct_name)
            }
            _ => false,
        }
    }
}
