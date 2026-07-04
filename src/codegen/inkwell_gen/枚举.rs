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
use super::类型检查::{枚举信息, 枚举变体信息};
use crate::parser::ast::{AstNode, Program};
use inkwell::values::BasicValueEnum;
use inkwell::{AddressSpace, IntPredicate};

/// 装箱枚举类型 idx 的 ARC 释放函数符号名。
pub(super) fn 枚举释放名(idx: u32) -> String {
    format!("qi.release.e{}", idx)
}

impl<'ctx> 后端<'ctx> {
    /// 第一趟：登记枚举名字（变体名 + tag 占位，载荷类型第二趟解析）。
    /// 必须先于 解析结构体字段 / 解析枚举变体 跑完 —— 载荷/字段里跨引用的枚举才能解析。
    pub(super) fn 登记枚举名字(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let AstNode::枚举声明(ed) = stmt {
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
            let (mut v, vt) = self
                .生成表达式(arg)?
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
