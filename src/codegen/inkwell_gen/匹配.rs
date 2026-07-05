//! 匹配（match）表达式降级 —— 基础实现：按分支顺序生成 if-else 链。
//!
//! 支持的分支模式：
//!   - 字面量：整数 / 浮点 / 布尔 / 字符 / 字符串（字符串用 qi_runtime_string_compare == 0）
//!   - 或模式：`1 | 2 | 3`（各子模式条件 OR）
//!   - 守卫：`模式 如果 条件 =>`（模式命中后再测守卫，不中落入下一分支）
//!   - 通配符 `_` 与 变量绑定（兜底分支；变量绑定把目标值绑进分支体作用域）
//!
//! 不支持的模式（结构体 / 枚举变体 / 元组 / 数组 / 范围）一律**编译报错** ——
//! 宁报错不静默不执行。不做穷尽性检查：全分支不中时整个 匹配 无副作用落空。
//!
//! 语句语义：匹配整体不产生值（返回 Ok(None)），当作值用会在调用处报“无值”。

use super::后端;
use super::类型::Qi类型;
use crate::parser::ast::{LiteralValue, MatchExpression, MatchPattern};
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::{FloatPredicate, IntPredicate};
use std::collections::HashSet;

impl<'ctx> 后端<'ctx> {
    /// 匹配表达式 → if-else 链。见模块头注释。
    pub(super) fn 生成匹配(
        &mut self,
        m: &MatchExpression,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let func = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or_else(|| "匹配：builder 无插入点".to_string())?;

        let (v, vt) = self
            .生成表达式(&m.value)?
            .ok_or_else(|| "匹配 的目标表达式无值".to_string())?;

        // 枚举 scrutinee → 专用降级（tag 比较 + 载荷解构 + 穷尽性检查）
        if let Some(idx) = vt.枚举索引() {
            let 装箱 = matches!(vt, Qi类型::装箱枚举(_));
            return self.生成枚举匹配(idx, 装箱, v, vt, m, func);
        }

        // 非枚举 scrutinee 用变体分支 → 报人话错误
        for arm in &m.arms {
            if matches!(arm.pattern, MatchPattern::枚举变体模式 { .. }) {
                return Err("匹配：变体解构分支（如 `圆(x) =>`）只能用于枚举类型的目标".to_string());
            }
        }

        let end_bb = self.ctx.append_basic_block(func, "match.end");

        // end 块可达性追踪：
        //   - 落空可达：模式链一路不中落到 end（存在未守卫的兜底分支即堵死）
        //   - 有分支汇入：某分支体没有终结（返回/抛出/跳出）自然汇入 end
        // 两者皆无 ⇒ end 不可达 ⇒ 发 unreachable，函数尾不再强制补一条返回。
        let mut 落空可达 = true;
        let mut 有分支汇入 = false;

        for (i, arm) in m.arms.iter().enumerate() {
            let body_bb = self
                .ctx
                .append_basic_block(func, &format!("match.body{}", i));
            let next_bb = self
                .ctx
                .append_basic_block(func, &format!("match.next{}", i));

            // 兜底分支（_ / 变量绑定）：模式恒真；其余生成比较条件
            let 兜底 = matches!(
                arm.pattern,
                MatchPattern::通配符 | MatchPattern::变量绑定(_)
            );
            if 兜底 {
                self.builder
                    .build_unconditional_branch(body_bb)
                    .map_err(|e| e.to_string())?;
            } else {
                let cond = self.匹配模式条件(&arm.pattern, v, vt)?;
                self.builder
                    .build_conditional_branch(cond, body_bb, next_bb)
                    .map_err(|e| e.to_string())?;
            }

            self.builder.position_at_end(body_bb);

            // 变量绑定：目标值拷入分支局部槽（entry 块 alloca），体内可读。
            // 分支结束后恢复同名旧绑定（避免污染 匹配 之后的作用域）。
            let 旧绑定 = if let MatchPattern::变量绑定(name) = &arm.pattern {
                let llvmt = self
                    .llvm基础类型(vt)
                    .ok_or_else(|| "匹配 绑定变量类型无效".to_string())?;
                let slot = self.入口块alloca(llvmt, name)?;
                self.builder
                    .build_store(slot, v)
                    .map_err(|e| e.to_string())?;
                let old = self.变量表.insert(name.clone(), (slot, vt));
                self.符号.声明变量(name, vt);
                Some((name.clone(), old))
            } else {
                None
            };

            // 守卫：模式命中后再测；不中 → 下一分支
            if let Some(g) = &arm.guard {
                let gv = self.生成条件(g)?;
                let guard_ok = self
                    .ctx
                    .append_basic_block(func, &format!("match.guard{}", i));
                self.builder
                    .build_conditional_branch(gv, guard_ok, next_bb)
                    .map_err(|e| e.to_string())?;
                self.builder.position_at_end(guard_ok);
            }

            let r = self.生成块(&arm.body, func);
            if let Some((name, old)) = 旧绑定 {
                match old {
                    Some(o) => {
                        self.变量表.insert(name, o);
                    }
                    None => {
                        self.变量表.remove(&name);
                    }
                }
            }
            r?;
            if !self.当前块已终结() {
                有分支汇入 = true;
            }
            self.跳转若未终结(end_bb)?;

            if 兜底 && arm.guard.is_none() {
                // 未守卫兜底：模式链到此必中，落空路径被堵死。
                // （其后的分支已不可达，仍照常生成，保守计入 有分支汇入。）
                落空可达 = false;
            }
            self.builder.position_at_end(next_bb);
        }

        // 所有分支不中（或兜底分支之后的不可达链）→ 落到 end
        self.builder
            .build_unconditional_branch(end_bb)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(end_bb);

        if !落空可达 && !有分支汇入 {
            // 全分支终结 + 无落空路径：end 不可达。发 unreachable，让上层的
            // 当前块已终结() 生效 —— 函数尾不再要求多余的 `返回 不可达`。
            self.builder
                .build_unreachable()
                .map_err(|e| e.to_string())?;
            return Ok(None);
        }

        // ARC：目标值若是结构上新建的 OWNED RC 临时（如函数调用返回的串）→ 释放。
        // 变量 / 字面量（BORROWED / immortal）不动。
        self.弧消费后释放2(v, vt, &m.value);
        Ok(None)
    }

    /// 枚举 匹配降级：tag 比较 if-else 链 + 载荷解构（BORROWED 借用，retain/release 平衡）。
    ///
    /// - 无载荷枚举：`v` 是 i64 tag，直接与变体 tag 比较。
    /// - 装箱枚举：`v` 是指针，load 槽0 得 tag；命中变体后 load 槽1.. 载荷绑进变量。
    /// - 分支形式：`变体名 =>`（含无载荷 `点`）/ `变体名(绑定...) =>` / `_` / 整体绑定兜底。
    /// - 穷尽性：无 `_`/整体兜底且未覆盖全部变体（无守卫分支才算覆盖）→ 编译报错。
    #[allow(clippy::too_many_arguments)]
    fn 生成枚举匹配(
        &mut self,
        idx: u32,
        装箱: bool,
        v: BasicValueEnum<'ctx>,
        vt: Qi类型,
        m: &MatchExpression,
        func: inkwell::values::FunctionValue<'ctx>,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let i64t = self.ctx.i64_type();
        let 变体表 = self.符号.枚举[idx as usize].变体.clone();
        let 是变体 = |name: &str| 变体表.iter().any(|v| v.名字 == name);

        // ── 穷尽性预检 + 模式合法性校验（一次遍历）──
        let mut 覆盖: HashSet<String> = HashSet::new();
        let mut 有兜底 = false;
        for arm in &m.arms {
            let 无守卫 = arm.guard.is_none();
            match &arm.pattern {
                MatchPattern::通配符 => {
                    if 无守卫 {
                        有兜底 = true;
                    }
                }
                MatchPattern::变量绑定(name) => {
                    if 是变体(name) {
                        if 无守卫 {
                            覆盖.insert(name.clone());
                        }
                    } else if 无守卫 {
                        有兜底 = true; // 整体绑定当兜底
                    }
                }
                MatchPattern::枚举变体模式 {
                    variant_name,
                    bindings,
                    ..
                } => {
                    let vinfo =
                        变体表
                            .iter()
                            .find(|v| &v.名字 == variant_name)
                            .ok_or_else(|| {
                                format!(
                                    "匹配：枚举 {} 没有变体 {}",
                                    super::枚举::枚举显示名(
                                        &self.符号.枚举[idx as usize].名字
                                    ),
                                    variant_name
                                )
                            })?;
                    if bindings.len() != vinfo.载荷.len() {
                        return Err(format!(
                            "匹配：变体 {} 解构需要 {} 个绑定，实际写了 {} 个",
                            variant_name,
                            vinfo.载荷.len(),
                            bindings.len()
                        ));
                    }
                    if 无守卫 {
                        覆盖.insert(variant_name.clone());
                    }
                }
                MatchPattern::字面量(_) => {
                    return Err(
                        "匹配：枚举目标的分支要用变体名（如 `红 =>` / `圆(x) =>`），不能用字面量"
                            .to_string(),
                    );
                }
                _ => {
                    return Err("匹配：枚举目标暂不支持该模式（用变体名 / `_` 兜底）".to_string());
                }
            }
        }
        if !有兜底 {
            let 缺: Vec<String> = 变体表
                .iter()
                .map(|v| v.名字.clone())
                .filter(|n| !覆盖.contains(n))
                .collect();
            if !缺.is_empty() {
                return Err(format!(
                    "匹配：未覆盖枚举 {} 的全部变体，缺少 [{}]。补齐这些分支，或加 `_ =>` 兜底",
                    super::枚举::枚举显示名(&self.符号.枚举[idx as usize].名字),
                    缺.join("、")
                ));
            }
        }

        // ── tag 值 ──
        let tagval: IntValue<'ctx> = if 装箱 {
            let base = v.into_pointer_value();
            let tp = self.槽指针(base, i64t.into(), 0)?;
            self.builder
                .build_load(i64t, tp, "tag")
                .map_err(|e| e.to_string())?
                .into_int_value()
        } else {
            v.into_int_value()
        };

        let end_bb = self.ctx.append_basic_block(func, "match.end");
        let mut 落空可达 = true;
        let mut 有分支汇入 = false;

        for (i, arm) in m.arms.iter().enumerate() {
            let body_bb = self
                .ctx
                .append_basic_block(func, &format!("ematch.body{}", i));
            let next_bb = self
                .ctx
                .append_basic_block(func, &format!("ematch.next{}", i));

            // 兜底：`_` 或 整体绑定（变量绑定且名字不是变体）
            let 整体绑定名: Option<String> = match &arm.pattern {
                MatchPattern::变量绑定(name) if !是变体(name) => Some(name.clone()),
                _ => None,
            };
            let 兜底 = matches!(arm.pattern, MatchPattern::通配符) || 整体绑定名.is_some();

            if 兜底 {
                self.builder
                    .build_unconditional_branch(body_bb)
                    .map_err(|e| e.to_string())?;
            } else {
                // 变体名 → tag 常量；命中 = tagval == 该 tag
                let variant_name = match &arm.pattern {
                    MatchPattern::变量绑定(name) => name.clone(), // 无载荷变体
                    MatchPattern::枚举变体模式 { variant_name, .. } => variant_name.clone(),
                    _ => unreachable!(),
                };
                let vtag = 变体表
                    .iter()
                    .find(|v| v.名字 == variant_name)
                    .map(|v| v.tag)
                    .ok_or_else(|| format!("匹配：变体 {} 未登记", variant_name))?;
                let cond = self
                    .builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        tagval,
                        i64t.const_int(vtag as u64, true),
                        "ematch.tageq",
                    )
                    .map_err(|e| e.to_string())?;
                self.builder
                    .build_conditional_branch(cond, body_bb, next_bb)
                    .map_err(|e| e.to_string())?;
            }

            self.builder.position_at_end(body_bb);

            // ── 绑定：整体绑定 / 载荷解构 ──
            // (name, 旧绑定) 用于分支结束后恢复；rc槽 用于 fall-through 平衡 release。
            let mut 旧绑定: Vec<(String, Option<(PointerValue<'ctx>, Qi类型)>)> = Vec::new();
            let mut rc槽: Vec<(PointerValue<'ctx>, Qi类型)> = Vec::new();

            if let Some(name) = &整体绑定名 {
                // 整体绑定：绑 scrutinee 本身
                let llvmt = self
                    .llvm基础类型(vt)
                    .ok_or_else(|| "匹配 整体绑定类型无效".to_string())?;
                let slot = self.入口块alloca(llvmt, name)?;
                if self.弧开() && super::所有权::是RC类型(vt) && v.is_pointer_value() {
                    self.弧retain任意(v, vt); // +1，出口/fall-through -1 平衡
                    rc槽.push((slot, vt));
                }
                self.builder
                    .build_store(slot, v)
                    .map_err(|e| e.to_string())?;
                let old = self.变量表.insert(name.clone(), (slot, vt));
                self.符号.声明变量(name, vt);
                旧绑定.push((name.clone(), old));
            } else if let MatchPattern::枚举变体模式 {
                variant_name,
                bindings,
                ..
            } = &arm.pattern
            {
                // 载荷解构（仅装箱枚举有载荷；无载荷变体 bindings 为空）
                let 载荷类型: Vec<Qi类型> = 变体表
                    .iter()
                    .find(|v| &v.名字 == variant_name)
                    .map(|v| v.载荷.clone())
                    .unwrap_or_default();
                if !载荷类型.is_empty() {
                    let base = v.into_pointer_value();
                    for (k, (bpat, ft)) in bindings.iter().zip(载荷类型.iter()).enumerate() {
                        // 绑定子模式：变量绑定(name) 取值；通配符 跳过
                        let name = match bpat {
                            MatchPattern::变量绑定(n) => n.clone(),
                            MatchPattern::通配符 => continue,
                            _ => return Err("匹配：变体载荷位置只支持变量名或 `_`".to_string()),
                        };
                        let llvmt = self
                            .llvm基础类型(*ft)
                            .ok_or_else(|| "匹配 载荷类型无效".to_string())?;
                        let slotp = self.槽指针(base, i64t.into(), (k + 1) as u64)?;
                        let val = self
                            .builder
                            .build_load(llvmt, slotp, &name)
                            .map_err(|e| e.to_string())?;
                        let local = self.入口块alloca(llvmt, &name)?;
                        // RC 载荷绑定：retain +1（出口/fall-through -1 平衡），借用不动 scrutinee
                        if self.弧开() && super::所有权::是RC类型(*ft) && val.is_pointer_value()
                        {
                            self.弧retain任意(val, *ft);
                            rc槽.push((local, *ft));
                        }
                        self.builder
                            .build_store(local, val)
                            .map_err(|e| e.to_string())?;
                        let old = self.变量表.insert(name.clone(), (local, *ft));
                        self.符号.声明变量(&name, *ft);
                        旧绑定.push((name.clone(), old));
                    }
                }
            }

            // 守卫：命中后再测；不中 → 下一分支。守卫失败路径必须平衡释放已 retain
            // 的 RC 载荷绑定（否则守卫落空时泄漏），故经 guard_fail 清理块再入 next。
            if let Some(g) = &arm.guard {
                let gv = self.生成条件(g)?;
                let guard_ok = self
                    .ctx
                    .append_basic_block(func, &format!("ematch.guard{}", i));
                if rc槽.is_empty() {
                    self.builder
                        .build_conditional_branch(gv, guard_ok, next_bb)
                        .map_err(|e| e.to_string())?;
                } else {
                    let guard_fail = self
                        .ctx
                        .append_basic_block(func, &format!("ematch.gfail{}", i));
                    self.builder
                        .build_conditional_branch(gv, guard_ok, guard_fail)
                        .map_err(|e| e.to_string())?;
                    self.builder.position_at_end(guard_fail);
                    let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
                    for (slot, t) in &rc槽 {
                        let cur = self
                            .builder
                            .build_load(ptrt, *slot, "arc.gfail")
                            .map_err(|e| e.to_string())?;
                        self.弧release任意(cur, *t);
                    }
                    self.builder
                        .build_unconditional_branch(next_bb)
                        .map_err(|e| e.to_string())?;
                }
                self.builder.position_at_end(guard_ok);
            }

            let r = self.生成块(&arm.body, func);
            // fall-through（体未终结）：平衡释放 RC 绑定（+1 归还）
            if r.is_ok() && !self.当前块已终结() {
                for (slot, t) in &rc槽 {
                    let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
                    let cur = self
                        .builder
                        .build_load(ptrt, *slot, "arc.bind")
                        .map_err(|e| e.to_string())?;
                    self.弧release任意(cur, *t);
                }
            }
            // 恢复旧绑定（无论体是否终结）：匹配 之后作用域不被污染
            for (name, old) in 旧绑定.into_iter().rev() {
                match old {
                    Some(o) => {
                        self.变量表.insert(name, o);
                    }
                    None => {
                        self.变量表.remove(&name);
                    }
                }
            }
            r?;
            if !self.当前块已终结() {
                有分支汇入 = true;
            }
            self.跳转若未终结(end_bb)?;

            if 兜底 && arm.guard.is_none() {
                落空可达 = false;
            }
            self.builder.position_at_end(next_bb);
        }

        // 所有分支不中（或兜底后不可达链）→ 落到 end
        self.builder
            .build_unconditional_branch(end_bb)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(end_bb);

        if !落空可达 && !有分支汇入 {
            self.builder
                .build_unreachable()
                .map_err(|e| e.to_string())?;
            return Ok(None);
        }

        // ARC：scrutinee 若是 OWNED 临时（如就地构造的装箱枚举）→ 释放
        self.弧消费后释放2(v, vt, &m.value);
        Ok(None)
    }

    /// 单个模式 → i1 命中条件（仅字面量 / 或模式；兜底模式恒真；其余报错）。
    fn 匹配模式条件(
        &mut self,
        p: &MatchPattern,
        v: BasicValueEnum<'ctx>,
        vt: Qi类型,
    ) -> Result<IntValue<'ctx>, String> {
        match p {
            MatchPattern::字面量(lit) => self.匹配字面量条件(lit, v, vt),
            MatchPattern::或模式(ps) => {
                let mut acc: Option<IntValue<'ctx>> = None;
                for sub in ps {
                    let c = self.匹配模式条件(sub, v, vt)?;
                    acc = Some(match acc {
                        None => c,
                        Some(a) => self
                            .builder
                            .build_or(a, c, "match.or")
                            .map_err(|e| e.to_string())?,
                    });
                }
                acc.ok_or_else(|| "匹配：空的或模式".to_string())
            }
            MatchPattern::通配符 | MatchPattern::变量绑定(_) => {
                Ok(self.ctx.bool_type().const_int(1, false))
            }
            _ => Err(
                "匹配 暂不支持该模式（当前支持：整数/浮点/布尔/字符串字面量、或模式、守卫、_ 与变量绑定兜底）"
                    .to_string(),
            ),
        }
    }

    /// 字面量模式 → i1 相等条件。
    fn 匹配字面量条件(
        &mut self,
        lit: &LiteralValue,
        v: BasicValueEnum<'ctx>,
        vt: Qi类型,
    ) -> Result<IntValue<'ctx>, String> {
        match lit {
            LiteralValue::整数(n) => {
                if !v.is_int_value() {
                    return Err("匹配：整数模式但目标不是整数".to_string());
                }
                let iv = v.into_int_value();
                let c = iv.get_type().const_int(*n as u64, true);
                self.builder
                    .build_int_compare(IntPredicate::EQ, iv, c, "match.int")
                    .map_err(|e| e.to_string())
            }
            LiteralValue::字符(ch) => {
                if !v.is_int_value() {
                    return Err("匹配：字符模式但目标不是整数/字符".to_string());
                }
                let iv = v.into_int_value();
                let c = iv.get_type().const_int(*ch as u64, false);
                self.builder
                    .build_int_compare(IntPredicate::EQ, iv, c, "match.chr")
                    .map_err(|e| e.to_string())
            }
            LiteralValue::布尔(b) => {
                if !v.is_int_value() {
                    return Err("匹配：布尔模式但目标不是布尔".to_string());
                }
                let iv = v.into_int_value();
                let c = iv.get_type().const_int(*b as u64, false);
                self.builder
                    .build_int_compare(IntPredicate::EQ, iv, c, "match.bool")
                    .map_err(|e| e.to_string())
            }
            LiteralValue::浮点数(f) => {
                if !v.is_float_value() {
                    return Err("匹配：浮点模式但目标不是浮点数".to_string());
                }
                let fv = v.into_float_value();
                let c = self.ctx.f64_type().const_float(*f);
                self.builder
                    .build_float_compare(FloatPredicate::OEQ, fv, c, "match.flt")
                    .map_err(|e| e.to_string())
            }
            LiteralValue::字符串(s) => {
                if vt != Qi类型::字符串 || !v.is_pointer_value() {
                    return Err("匹配：字符串模式但目标不是字符串".to_string());
                }
                let lit_ptr = self.emit_immortal_string(s, "qi.match.str")?;
                let f = self
                    .module
                    .get_function("qi_runtime_string_compare")
                    .ok_or_else(|| "运行时函数未声明: qi_runtime_string_compare".to_string())?;
                let cs = self
                    .builder
                    .build_call(f, &[v.into(), lit_ptr.into()], "match.strcmp")
                    .map_err(|e| e.to_string())?;
                let r = cs
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "string_compare 未返回".to_string())?
                    .into_int_value();
                self.builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        r,
                        r.get_type().const_zero(),
                        "match.streq",
                    )
                    .map_err(|e| e.to_string())
            }
        }
    }
}
