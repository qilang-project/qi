//! 枚举降级 —— 声明登记、构造、ARC 释放函数。
//!
//! 两 tier 表示（与设计一致）：
//!   - **全无载荷枚举** = 纯 i64（变体序号 tag）。构造是常量，匹配是整数比较，零分配。
//!   - **任一变体带载荷** = 装箱堆指针：`qi_obj_alloc((1+最大载荷槽)*8)`，
//!     槽0=tag(i64)，槽1..=载荷（8 字节槽，f64 位模式 / i64 / RC 指针）。
//!     ARC 纪律完全复用结构体那套（构造 rc=1；存槽 retain/release；出口释放；
//!     qi.release.e<idx> 按 tag switch 后释放对应变体的 RC 载荷槽）。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::推断表达式类型;
use super::类型检查::{枚举信息, 枚举变体信息};
use crate::parser::ast::{AstNode, Program};
use inkwell::values::BasicValueEnum;
use inkwell::{AddressSpace, IntPredicate};

/// 装箱枚举类型 idx 的 ARC 释放函数符号名。
pub(super) fn 枚举释放名(idx: u32) -> String {
    format!("qi.release.e{}", idx)
}

/// 内建参数化枚举构造子 → (模板名, 变体名, 载荷个数, 能否从参数[0]反推 T)。
///   有(x)→选项.有(T 来自参数)   无→选项.无
///   成(x)→结果.成(T 来自参数)   败(msg)→结果.败(字符串，msg 非 T，不能反推)
/// 返回 None 表示不是构造子名。集中一处，未来用户泛型构造子登记进同款表即可。
pub(super) fn 构造子模板(名: &str) -> Option<(&'static str, &'static str, usize, bool)> {
    match 名 {
        "有" => Some(("选项", "有", 1, true)),
        "无" => Some(("选项", "无", 0, false)),
        "成" => Some(("结果", "成", 1, true)),
        "败" => Some(("结果", "败", 1, false)),
        _ => None,
    }
}

/// 名字是否为保留构造子（用户不能定义同名函数）。
pub(super) fn 是保留构造子(名: &str) -> bool {
    构造子模板(名).is_some()
}

/// 枚举名的人类可读渲染：单态实例名 `选项$整数` → `选项<整数>`；
/// 嵌套 `选项$选项$整数` → `选项<选项<整数>>`；普通枚举原样。
pub(super) fn 枚举显示名(名: &str) -> String {
    match 名.split_once('$') {
        Some((模板, 实参)) => format!("{}<{}>", 模板, 枚举显示名(实参)),
        None => 名.to_string(),
    }
}

impl<'ctx> 后端<'ctx> {
    /// 第一趟：登记枚举名字（变体名 + tag 占位，载荷类型第二趟解析）。
    /// 必须先于 解析结构体字段 / 解析枚举变体 跑完 —— 载荷/字段里跨引用的枚举才能解析。
    pub(super) fn 登记枚举名字(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let AstNode::枚举声明(ed) = stmt {
                // 泛型枚举（带 <T>）：模板入注册表（载荷 TypeNode 原样存，含 T 占位），
                // 不占具体枚举索引 —— 按 (模板, 实参) 单态实例化。
                if !ed.type_params.is_empty() {
                    if ed.type_params.len() > 2 {
                        return Err(format!(
                            "泛型枚举 {} 声明了 {} 个类型参数，最多支持 2 个（<T> 或 <T, E>）",
                            ed.name,
                            ed.type_params.len()
                        ));
                    }
                    // 特性约束（`<T: 特性>`）v1 只支持泛型函数，枚举模板明确报错
                    if let Some(tp) = ed.type_params.iter().find(|t| t.contains(':')) {
                        return Err(format!(
                            "泛型枚举 {} 的类型参数 <{}> 带特性约束 —— 枚举暂不支持约束（目前只有泛型函数支持 <T: 特性>）",
                            ed.name,
                            tp.replace(':', ": ")
                        ));
                    }
                    self.符号.泛型枚举模板.insert(
                        ed.name.clone(),
                        super::类型检查::泛型枚举模板 {
                            类型参数: ed.type_params.clone(),
                            变体: ed
                                .variants
                                .iter()
                                .map(|v| (v.name.clone(), v.payload.clone()))
                                .collect(),
                        },
                    );
                    continue;
                }
                let 变体: Vec<枚举变体信息> = ed
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| 枚举变体信息 {
                        名字: v.name.clone(),
                        tag: v.value.unwrap_or(i as i64),
                        载荷: vec![], // 第二趟解析
                    })
                    .collect();
                self.符号.登记枚举(枚举信息 {
                    名字: ed.name.clone(),
                    包: self.当前包.clone(),
                    变体,
                    装箱: false, // 第二趟据载荷定
                    最大载荷槽: 0,
                });
            }
        }
        Ok(())
    }

    /// 第二趟：解析各变体载荷类型，定装箱标志与最大载荷槽数。
    /// 只写本包登记的那条；同包重复定义变体不一致 → 编译报错。
    pub(super) fn 解析枚举变体(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let AstNode::枚举声明(ed) = stmt {
                if !ed.type_params.is_empty() {
                    continue; // 泛型模板：实例化时才解析载荷
                }
                let idx = self
                    .符号
                    .本包枚举索引(&ed.name)
                    .ok_or_else(|| format!("内部错误：枚举 {} 未在第一趟登记", ed.name))?;
                let 名字们: Vec<String> = ed.variants.iter().map(|v| v.name.clone()).collect();
                let 已有: Vec<String> = self.符号.枚举[idx as usize]
                    .变体
                    .iter()
                    .map(|v| v.名字.clone())
                    .collect();
                if 已有 != 名字们 {
                    return Err(format!(
                        "枚举 {} 在包 {} 中重复定义且变体不一致",
                        ed.name,
                        self.当前包.as_deref().unwrap_or("(无包)")
                    ));
                }
                let mut 装箱 = false;
                let mut 最大 = 0usize;
                let mut 变体: Vec<枚举变体信息> = Vec::new();
                for (i, v) in ed.variants.iter().enumerate() {
                    let 载荷: Vec<Qi类型> =
                        v.payload.iter().map(|t| self.符号.解析类型(t)).collect();
                    if !载荷.is_empty() {
                        装箱 = true;
                    }
                    if 载荷.len() > 最大 {
                        最大 = 载荷.len();
                    }
                    变体.push(枚举变体信息 {
                        名字: v.name.clone(),
                        tag: v.value.unwrap_or(i as i64),
                        载荷,
                    });
                }
                let info = &mut self.符号.枚举[idx as usize];
                info.变体 = 变体;
                info.装箱 = 装箱;
                info.最大载荷槽 = 最大;
            }
        }
        Ok(())
    }

    /// 枚举构造：无载荷 → i64 tag 常量（Qi类型::枚举）；
    /// 装箱 → 堆分配 + 存 tag + 逐载荷存槽（Qi类型::装箱枚举，rc=1 OWNED）。
    pub(super) fn 生成枚举构造(
        &mut self,
        枚举名: &str,
        变体名: &str,
        参数: &[AstNode],
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let idx = self
            .符号
            .枚举索引(枚举名)
            .ok_or_else(|| format!("未定义的枚举: {}", 枚举名))?;
        let (装箱, 最大, tag, 载荷类型): (bool, usize, i64, Vec<Qi类型>) = {
            let info = self
                .符号
                .枚举信息(idx)
                .ok_or_else(|| "枚举信息缺失".to_string())?;
            let v = info
                .查变体(变体名)
                .ok_or_else(|| format!("枚举 {} 无变体 {}", 枚举名, 变体名))?;
            (info.装箱, info.最大载荷槽, v.tag, v.载荷.clone())
        };
        if 参数.len() != 载荷类型.len() {
            return Err(format!(
                "枚举变体 {}.{} 需要 {} 个载荷，实际传入 {} 个",
                枚举名,
                变体名,
                载荷类型.len(),
                参数.len()
            ));
        }

        let i64t = self.ctx.i64_type();

        if !装箱 {
            // 无载荷枚举：纯 i64 常量 tag
            return Ok((i64t.const_int(tag as u64, true).into(), Qi类型::枚举(idx)));
        }

        // 装箱：alloc (1+最大)*8 字节 —— 槽0=tag，槽1..=载荷
        let size = i64t.const_int(((1 + 最大) as u64) * 8, false);
        let alloc名 = if self.弧开() {
            "qi_obj_alloc"
        } else {
            "qi_runtime_alloc"
        };
        let alloc = self
            .module
            .get_function(alloc名)
            .ok_or_else(|| format!("运行时函数未声明: {}", alloc名))?;
        let base = self
            .builder
            .build_call(alloc, &[size.into()], "enummem")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "枚举分配未返回指针".to_string())?
            .into_pointer_value();

        // 槽0 = tag
        let tagptr = self.槽指针(base, i64t.into(), 0)?;
        self.builder
            .build_store(tagptr, i64t.const_int(tag as u64, true))
            .map_err(|e| e.to_string())?;

        // 载荷槽（槽1..）
        for (k, (arg, ft)) in 参数.iter().zip(载荷类型.iter()).enumerate() {
            // 期望透传：载荷本身是构造子（如 选项<选项<整数>> 的 有(有(5)) / 有(无)）时定型
            let (mut v, vt) = self
                .生成带期望(arg, Some(*ft))?
                .ok_or_else(|| "枚举载荷实参无值".to_string())?;
            v = self.协调到类型(v, vt, *ft)?;
            // ARC：RC 载荷（字符串/结构体/数组/装箱枚举）存槽 —— BORROWED retain / OWNED 转移
            if self.弧开() && super::所有权::是RC类型(*ft) && v.is_pointer_value() {
                self.弧存入槽2(v, arg, *ft);
            }
            let slot = self.槽指针(base, i64t.into(), (k + 1) as u64)?;
            self.builder
                .build_store(slot, v)
                .map_err(|e| e.to_string())?;
        }

        Ok((base.into(), Qi类型::装箱枚举(idx)))
    }

    // ───────────────── 参数化枚举构造子（选项/结果）codegen ─────────────────

    /// 识别一个表达式节点是否为内建构造子调用（裸 有(x)/无/成(x)/败(msg)）。
    /// 返回 (构造子名, 实参)。裸 `无` 是标识符且不能被局部/全局变量遮蔽时才算构造子。
    pub(super) fn 识别构造子调用(&self, node: &AstNode) -> Option<(String, Vec<AstNode>)> {
        match node {
            AstNode::函数调用表达式(call)
                if call.module_qualifier.is_none() && 构造子模板(&call.callee).is_some() =>
            {
                Some((call.callee.clone(), call.arguments.clone()))
            }
            AstNode::标识符表达式(id)
                if id.name == "无"
                    && !self.变量表.contains_key(&id.name)
                    && !self.全局变量表.contains_key(&id.name) =>
            {
                Some(("无".to_string(), Vec::new()))
            }
            _ => None,
        }
    }

    /// 生成表达式；若目标处有期望类型（返回/变量注解/形参/载荷位），先透传给构造子定型。
    /// 非构造子表达式直接走 生成表达式（期望被忽略）。这是四条定型规则的统一注入点。
    /// 用户泛型枚举构造（盒装.满(x) / 盒装.空盒）同样在此接收期望。
    pub(super) fn 生成带期望(
        &mut self,
        node: &AstNode,
        期望: Option<Qi类型>,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        if let Some((名, 参数)) = self.识别构造子调用(node) {
            return self.生成构造子(&名, &参数, 期望).map(Some);
        }
        if let Some((模板, 变体, 参数)) = self.识别泛型枚举构造(node) {
            return self.生成泛型枚举构造(&模板, &变体, &参数, 期望).map(Some);
        }
        self.生成表达式(node)
    }

    /// 构造子降级：定型对应的枚举实例，再复用装箱枚举构造。
    ///
    /// 定型（按序）：
    ///   ① 期望是匹配模板的装箱枚举实例 → 直接用它（覆盖规则①②③：注解/返回/形参）；
    ///   ② 否则若能从参数[0]反推 T（有/成）→ 按需实例化（规则④）；
    ///   ③ 否则（裸 无 / 败 缺上下文）→ 人话报错。
    pub(super) fn 生成构造子(
        &mut self,
        名: &str,
        参数: &[AstNode],
        期望: Option<Qi类型>,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let (模板, 变体, 载荷个数, 可推断) =
            构造子模板(名).ok_or_else(|| format!("内部错误：{} 不是构造子", 名))?;
        if 参数.len() != 载荷个数 {
            return Err(format!(
                "构造子 {} 需要 {} 个参数，实际传入 {} 个",
                名,
                载荷个数,
                参数.len()
            ));
        }

        // ① 期望是本模板的装箱枚举实例 → 直接用
        let 前缀 = format!("{}$", 模板);
        let idx = match 期望 {
            Some(Qi类型::装箱枚举(i))
                if self
                    .符号
                    .枚举信息(i)
                    .map(|e| e.名字.starts_with(&前缀))
                    .unwrap_or(false) =>
            {
                i
            }
            _ => {
                // ②/③ 反推 T 或报错
                if 可推断 {
                    let 元素 = 推断表达式类型(&参数[0], &self.符号);
                    if 元素 == Qi类型::未知 {
                        return Err(format!(
                            "{} 的载荷类型无法推断，请给变量 / 返回值加类型注解，例如 : {}<整数>",
                            名, 模板
                        ));
                    }
                    self.符号.实例化参数枚举(模板, &[元素])?
                } else {
                    return Err(format!(
                        "{} 需要类型上下文，给变量加 : {}<...> 注解（或用于已声明返回类型的返回语句 / 已知形参类型的实参位）",
                        名, 模板
                    ));
                }
            }
        };

        let 实例名 = self
            .符号
            .枚举信息(idx)
            .map(|e| e.名字.clone())
            .ok_or_else(|| "参数化枚举实例缺失".to_string())?;
        self.生成枚举构造(&实例名, 变体, 参数)
    }

    /// 为所有装箱枚举 emit ARC 释放函数（qi.release.e<idx>）。先声明后定义（幂等）。
    /// 在 弧生成释放函数（结构体/数组）之后调用一次；QI_ARC 关时不产生任何 IR。
    pub(super) fn 弧生成枚举释放函数(&mut self) -> Result<(), String> {
        if !self.弧开() {
            return Ok(());
        }
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let sig = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        let n = self.符号.枚举.len();
        for i in 0..n {
            if self.符号.枚举[i].装箱 {
                let name = 枚举释放名(i as u32);
                if self.module.get_function(&name).is_none() {
                    self.module.add_function(&name, sig, None);
                }
            }
        }
        for i in 0..n {
            if self.符号.枚举[i].装箱 {
                self.弧定义枚举释放(i as u32)?;
            }
        }
        Ok(())
    }

    /// 定义 qi.release.e<idx>：
    ///   null → ret；qi_obj_dec(p) 旧值 != 1 → ret；
    ///   load tag = p[0]；switch tag → 各带 RC 载荷的变体块逐槽 release；
    ///   qi_obj_free(p)。无 RC 载荷的变体走 default（只 free 本体）。
    fn 弧定义枚举释放(&mut self, idx: u32) -> Result<(), String> {
        let func = self
            .module
            .get_function(&枚举释放名(idx))
            .ok_or_else(|| "枚举释放函数原型缺失".to_string())?;
        if func.get_first_basic_block().is_some() {
            return Ok(()); // 已定义
        }
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let i64t = self.ctx.i64_type();
        let p = func.get_nth_param(0).unwrap().into_pointer_value();

        let entry = self.ctx.append_basic_block(func, "entry");
        let dec_bb = self.ctx.append_basic_block(func, "dec");
        let switch_bb = self.ctx.append_basic_block(func, "switch");
        let free_bb = self.ctx.append_basic_block(func, "free");
        let ret_bb = self.ctx.append_basic_block(func, "ret");

        // entry：null 短路
        self.builder.position_at_end(entry);
        let isnull = self
            .builder
            .build_is_null(p, "isnull")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(isnull, ret_bb, dec_bb)
            .map_err(|e| e.to_string())?;

        // dec：旧 refcount == 1 才真释放
        self.builder.position_at_end(dec_bb);
        let dec = self
            .module
            .get_function("qi_obj_dec")
            .ok_or("qi_obj_dec 未声明")?;
        let old = self
            .builder
            .build_call(dec, &[p.into()], "old")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or("qi_obj_dec 未返回")?
            .into_int_value();
        let last = self
            .builder
            .build_int_compare(IntPredicate::EQ, old, i64t.const_int(1, false), "last")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(last, switch_bb, ret_bb)
            .map_err(|e| e.to_string())?;

        // switch：按 tag 分派到各变体的载荷释放块（仅含 RC 载荷的变体建块）
        self.builder.position_at_end(switch_bb);
        let tagptr = self.槽指针(p, i64t.into(), 0)?;
        let tag = self
            .builder
            .build_load(i64t, tagptr, "tag")
            .map_err(|e| e.to_string())?
            .into_int_value();

        // 收集需要 emit 释放块的变体（含 RC 载荷）
        let 变体表: Vec<枚举变体信息> = self.符号.枚举[idx as usize].变体.clone();
        let mut cases: Vec<(
            inkwell::values::IntValue<'ctx>,
            inkwell::basic_block::BasicBlock<'ctx>,
        )> = Vec::new();
        for v in &变体表 {
            let 有rc = v.载荷.iter().any(|t| super::所有权::是RC类型(*t));
            if !有rc {
                continue;
            }
            let blk = self.ctx.append_basic_block(func, &format!("v{}", v.tag));
            cases.push((i64t.const_int(v.tag as u64, true), blk));
        }
        self.builder
            .build_switch(tag, free_bb, &cases)
            .map_err(|e| e.to_string())?;

        // 各变体块：逐 RC 载荷槽 load + release，再汇入 free
        let mut ci = 0usize;
        for v in &变体表 {
            let 有rc = v.载荷.iter().any(|t| super::所有权::是RC类型(*t));
            if !有rc {
                continue;
            }
            let blk = cases[ci].1;
            ci += 1;
            self.builder.position_at_end(blk);
            for (k, ft) in v.载荷.iter().enumerate() {
                if !super::所有权::是RC类型(*ft) {
                    continue;
                }
                let slot = self.槽指针(p, i64t.into(), (k + 1) as u64)?;
                let ev = self
                    .builder
                    .build_load(ptrt, slot, "pl")
                    .map_err(|e| e.to_string())?;
                self.弧release任意(ev, *ft);
            }
            self.builder
                .build_unconditional_branch(free_bb)
                .map_err(|e| e.to_string())?;
        }

        // free：回收本体
        self.builder.position_at_end(free_bb);
        let free = self
            .module
            .get_function("qi_obj_free")
            .ok_or("qi_obj_free 未声明")?;
        self.builder
            .build_call(free, &[p.into()], "")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_unconditional_branch(ret_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(ret_bb);
        self.builder.build_return(None).map_err(|e| e.to_string())?;
        Ok(())
    }
}
