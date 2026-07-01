//! 结构体降级 —— 声明登记、字面量堆分配、字段读写。
//!
//! 内存表示：结构体实例一律**堆分配**（`qi_runtime_alloc`），按指针传递，
//! 因此可跨函数返回（构造函数、方法返回自身、链式调用都依赖这点）。
//! LLVM 侧用具名 struct 类型 + typed GEP 定位字段，读写用 load/store，
//! 类型完全由类型检查器提供 —— 不再有旧后端的字节偏移 + i64 猜测。

use super::类型::Qi类型;
use super::类型检查::结构体信息;
use super::后端;
use crate::parser::ast::{AstNode, Program, StructLiteralExpression};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::AddressSpace;

impl<'ctx> 后端<'ctx> {
    /// 登记所有结构体：填符号表布局 + 建 LLVM 具名 struct 类型。
    pub(super) fn 登记结构体(&mut self, program: &Program) -> Result<(), String> {
        // 先收集布局到符号表（含字段的 Qi 类型，自定义类型可能相互引用，
        // 但字段目前只用指针语义即可，前向引用不成问题）。
        for stmt in &program.statements {
            if let AstNode::结构体声明(sd) = stmt {
                let 字段名: Vec<String> = sd.fields.iter().map(|f| f.name.clone()).collect();
                let 字段类型: Vec<Qi类型> = sd
                    .fields
                    .iter()
                    .map(|f| self.符号.解析类型(&f.type_annotation))
                    .collect();
                self.符号.登记结构体(结构体信息 {
                    名字: sd.name.clone(),
                    字段名,
                    字段类型,
                });
            }
        }

        // 再据符号表布局建 LLVM struct 类型，顺序与索引一致。
        self.结构体llvm.clear();
        for i in 0..self.符号.结构体.len() {
            let 字段类型 = self.符号.结构体[i].字段类型.clone();
            let 名字 = self.符号.结构体[i].名字.clone();
            let llvm字段: Vec<BasicTypeEnum> = 字段类型
                .iter()
                .map(|t| {
                    self.llvm基础类型(*t)
                        .unwrap_or_else(|| self.ctx.i64_type().into())
                })
                .collect();
            let st = self.ctx.opaque_struct_type(&format!("struct.{}", 名字));
            st.set_body(&llvm字段, false);
            self.结构体llvm.push(st);
        }
        Ok(())
    }

    /// 结构体字面量 → 堆分配指针 + 逐字段 store。
    pub(super) fn 生成结构体字面量(
        &mut self,
        lit: &StructLiteralExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let idx = self
            .符号
            .结构体索引(&lit.struct_name)
            .ok_or_else(|| format!("未定义的结构体: {}", lit.struct_name))?;
        let st = self.结构体llvm[idx as usize];

        // 堆分配：size = LLVM 结构体大小（用 target-independent 常量占位不可靠，
        // 直接用 store 之前的 GEP 定位；分配大小取字段数*8 的保守上界，够放所有字段，
        // 与旧后端一致，指针可 GC 追踪）。这里用精确布局大小更稳：字段数*8。
        let 字段数 = self.符号.结构体信息(idx).map(|s| s.字段名.len()).unwrap_or(0);
        let size = self.ctx.i64_type().const_int((字段数 as u64) * 8, false);
        let alloc = self
            .module
            .get_function("qi_runtime_alloc")
            .ok_or_else(|| "运行时函数未声明: qi_runtime_alloc".to_string())?;
        let cs = self
            .builder
            .build_call(alloc, &[size.into()], "structmem")
            .map_err(|e| e.to_string())?;
        let base = cs
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "qi_runtime_alloc 未返回指针".to_string())?
            .into_pointer_value();

        // 逐字段初始化（按字面量出现顺序，用字段名定索引）
        for fv in &lit.fields {
            let (fidx, ftype) = self
                .符号
                .结构体信息(idx)
                .and_then(|s| s.查字段(&fv.name))
                .ok_or_else(|| format!("结构体 {} 无字段 {}", lit.struct_name, fv.name))?;
            let (mut v, vt) = self
                .生成表达式(&fv.value)?
                .ok_or_else(|| format!("字段 {} 初值无值", fv.name))?;
            if ftype.是浮点() && !vt.是浮点() {
                v = self.整数转浮点值(v)?;
            }
            let fptr = self.字段指针(base, st, fidx, &fv.name)?;
            self.builder.build_store(fptr, v).map_err(|e| e.to_string())?;
        }

        Ok((base.into(), Qi类型::结构体(idx)))
    }

    /// 字段访问 x.字段 → GEP + load，返回 (值, 字段类型)。
    pub(super) fn 生成字段访问(
        &mut self,
        object: &AstNode,
        field: &str,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let (base, idx) = self.求结构体指针(object)?;
        let st = self.结构体llvm[idx as usize];
        let (fidx, ftype) = self
            .符号
            .结构体信息(idx)
            .and_then(|s| s.查字段(field))
            .ok_or_else(|| format!("结构体无字段 {}", field))?;
        let fptr = self.字段指针(base, st, fidx, field)?;
        let llvmt = self
            .llvm基础类型(ftype)
            .ok_or_else(|| format!("字段 {} 类型无效", field))?;
        let v = self
            .builder
            .build_load(llvmt, fptr, field)
            .map_err(|e| e.to_string())?;
        Ok((v, ftype))
    }

    /// 字段赋值 x.字段 = 值 → GEP + store。
    pub(super) fn 生成字段赋值(
        &mut self,
        object: &AstNode,
        field: &str,
        value: &AstNode,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let (base, idx) = self.求结构体指针(object)?;
        let st = self.结构体llvm[idx as usize];
        let (fidx, ftype) = self
            .符号
            .结构体信息(idx)
            .and_then(|s| s.查字段(field))
            .ok_or_else(|| format!("结构体无字段 {}", field))?;
        let (mut v, vt) = self
            .生成表达式(value)?
            .ok_or_else(|| "字段赋值右值无值".to_string())?;
        if ftype.是浮点() && !vt.是浮点() {
            v = self.整数转浮点值(v)?;
        }
        let fptr = self.字段指针(base, st, fidx, field)?;
        self.builder.build_store(fptr, v).map_err(|e| e.to_string())?;
        Ok((v, ftype))
    }

    /// 求某表达式的结构体指针 + 结构体索引。表达式类型必须是结构体。
    pub(super) fn 求结构体指针(
        &mut self,
        object: &AstNode,
    ) -> Result<(PointerValue<'ctx>, u32), String> {
        let (v, t) = self
            .生成表达式(object)?
            .ok_or_else(|| "结构体表达式无值".to_string())?;
        let idx = t
            .结构体索引()
            .ok_or_else(|| "该表达式不是结构体，无法访问字段/方法".to_string())?;
        Ok((v.into_pointer_value(), idx))
    }

    /// 计算字段的 typed GEP 指针。
    fn 字段指针(
        &self,
        base: PointerValue<'ctx>,
        st: inkwell::types::StructType<'ctx>,
        fidx: u32,
        name: &str,
    ) -> Result<PointerValue<'ctx>, String> {
        self.builder
            .build_struct_gep(st, base, fidx, name)
            .map_err(|_| format!("字段 {} GEP 失败", name))
    }

    /// 整数值 sitofp → double（结构体字段/字面量隐式提升用）。
    fn 整数转浮点值(
        &self,
        v: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        Ok(self
            .builder
            .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
            .map_err(|e| e.to_string())?
            .into())
    }

    /// 便捷：不透明指针类型。
    #[allow(dead_code)]
    fn ptr类型(&self) -> inkwell::types::PointerType<'ctx> {
        self.ctx.ptr_type(AddressSpace::default())
    }
}
