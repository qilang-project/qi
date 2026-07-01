//! 数组字面量 + 数组索引访问 + `.长度` 属性。
//!
//! 表示：连续堆分配，**头部一个 i64 长度槽**，之后 n 个元素槽（每槽 8 字节）：
//!   `[len@0, elem0@1, elem1@2, ...]`
//! 索引访问对元素偏移 +1（跳过长度头）；`.长度` 读头。数组值本身是 ptr。
//! 仅支持同构标量元素（整数/浮点/布尔/指针）；嵌套/结构体元素按指针存。

use super::类型::{Qi类型, 元素类型};
use super::类型检查::推断表达式类型;
use super::后端;
use crate::parser::ast::{ArrayAccessExpression, ArrayLiteralExpression};
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

impl<'ctx> 后端<'ctx> {
    /// `[a, b, c]` → 堆分配（长度头 + 元素）+ 逐元素 store，返回数组指针。
    pub(super) fn 生成数组字面量(
        &mut self,
        arr: &ArrayLiteralExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        // 元素类型：由首元素推断
        let 元素 = arr
            .elements
            .first()
            .map(|e| 元素类型::从标量(推断表达式类型(e, &self.符号)))
            .unwrap_or(元素类型::整数);

        let n = arr.elements.len() as u64;
        let i64t = self.ctx.i64_type();
        // (n+1) 槽：0 号存长度，其余存元素，每槽 8 字节
        let size = i64t.const_int((n + 1) * 8, false);
        let alloc = self
            .module
            .get_function("qi_runtime_alloc")
            .ok_or_else(|| "运行时函数未声明: qi_runtime_alloc".to_string())?;
        let base = self
            .builder
            .build_call(alloc, &[size.into()], "arrmem")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "alloc 未返回".to_string())?
            .into_pointer_value();

        // 头部存长度
        let head = self.槽指针(base, i64t.into(), 0)?;
        self.builder
            .build_store(head, i64t.const_int(n, false))
            .map_err(|e| e.to_string())?;

        let 元素llvm = self.元素llvm类型(元素);
        for (i, e) in arr.elements.iter().enumerate() {
            let (mut v, vt) = self
                .生成表达式(e)?
                .ok_or_else(|| "数组元素无值".to_string())?;
            if 元素 == 元素类型::浮点数 && !vt.是浮点() {
                v = self
                    .builder
                    .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
                    .map_err(|e| e.to_string())?
                    .into();
            }
            // 元素偏移 +1（跳过长度头）
            let slot = self.槽指针(base, 元素llvm, (i as u64) + 1)?;
            self.builder.build_store(slot, v).map_err(|e| e.to_string())?;
        }
        Ok((base.into(), Qi类型::数组(元素)))
    }

    /// `arr[idx]` → GEP(idx+1) + load，返回元素值。
    pub(super) fn 生成数组访问(
        &mut self,
        acc: &ArrayAccessExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let (av, at) = self
            .生成表达式(&acc.array)?
            .ok_or_else(|| "数组表达式无值".to_string())?;
        let 元素 = at.数组元素().unwrap_or(元素类型::整数);
        let base = av.into_pointer_value();
        let (iv, _) = self
            .生成表达式(&acc.index)?
            .ok_or_else(|| "数组下标无值".to_string())?;

        // 元素偏移 = idx + 1（跳过长度头）
        let one = self.ctx.i64_type().const_int(1, false);
        let 偏移 = self
            .builder
            .build_int_add(iv.into_int_value(), one, "idx1")
            .map_err(|e| e.to_string())?;
        let 元素llvm = self.元素llvm类型(元素);
        let slot = self.槽指针动态(base, 元素llvm, 偏移)?;
        let v = self
            .builder
            .build_load(元素llvm, slot, "arrget")
            .map_err(|e| e.to_string())?;
        Ok((v, 元素.标量()))
    }

    /// `arr.长度` → 读长度头（i64）。
    pub(super) fn 生成数组长度(
        &mut self,
        base: PointerValue<'ctx>,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let i64t = self.ctx.i64_type();
        let head = self.槽指针(base, i64t.into(), 0)?;
        let v = self
            .builder
            .build_load(i64t, head, "arrlen")
            .map_err(|e| e.to_string())?;
        Ok((v, Qi类型::整数))
    }

    /// 元素的 LLVM 类型。
    fn 元素llvm类型(&self, e: 元素类型) -> inkwell::types::BasicTypeEnum<'ctx> {
        match e {
            元素类型::浮点数 => self.ctx.f64_type().into(),
            元素类型::布尔 => self.ctx.bool_type().into(),
            元素类型::指针 => self.ctx.ptr_type(inkwell::AddressSpace::default()).into(),
            元素类型::整数 => self.ctx.i64_type().into(),
        }
    }

    /// 常量槽号的指针（inbounds GEP）。
    fn 槽指针(
        &self,
        base: PointerValue<'ctx>,
        elem_ty: inkwell::types::BasicTypeEnum<'ctx>,
        idx: u64,
    ) -> Result<PointerValue<'ctx>, String> {
        let i = self.ctx.i64_type().const_int(idx, false);
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_ty, base, &[i], "arrslot")
                .map_err(|e| e.to_string())
        }
    }

    /// 动态槽号的指针。
    fn 槽指针动态(
        &self,
        base: PointerValue<'ctx>,
        elem_ty: inkwell::types::BasicTypeEnum<'ctx>,
        idx: IntValue<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_ty, base, &[idx], "arrslot")
                .map_err(|e| e.to_string())
        }
    }
}
