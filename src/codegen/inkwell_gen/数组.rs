//! 数组字面量 + 数组索引访问。
//!
//! 表示：连续堆分配（`qi_runtime_alloc(n * 8)`，每槽 8 字节，与旧后端一致），
//! 按元素类型 typed GEP + load/store。数组值本身是 ptr（`Qi类型::数组(元素)`）。
//! 仅支持同构标量元素（整数/浮点/布尔/指针），够覆盖示例；嵌套/结构体元素按指针存。

use super::类型::{Qi类型, 元素类型};
use super::类型检查::推断表达式类型;
use super::后端;
use crate::parser::ast::{ArrayAccessExpression, ArrayLiteralExpression};
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'ctx> 后端<'ctx> {
    /// `[a, b, c]` → 堆分配 + 逐元素 store，返回数组指针。
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
        // 每槽 8 字节（与旧后端一致，避免精确 sizeof）
        let size = i64t.const_int(n * 8, false);
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

        let 元素llvm = self.元素llvm类型(元素);
        for (i, e) in arr.elements.iter().enumerate() {
            let (mut v, vt) = self
                .生成表达式(e)?
                .ok_or_else(|| "数组元素无值".to_string())?;
            // 元素是浮点而值是整数：提升
            if 元素 == 元素类型::浮点数 && !vt.是浮点() {
                v = self
                    .builder
                    .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
                    .map_err(|e| e.to_string())?
                    .into();
            }
            let slot = self.元素指针(base, 元素llvm, i as u64)?;
            self.builder.build_store(slot, v).map_err(|e| e.to_string())?;
        }
        Ok((base.into(), Qi类型::数组(元素)))
    }

    /// `arr[idx]` → GEP + load，返回元素值。
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

        let 元素llvm = self.元素llvm类型(元素);
        let slot = self.元素指针动态(base, 元素llvm, iv.into_int_value())?;
        let v = self
            .builder
            .build_load(元素llvm, slot, "arrget")
            .map_err(|e| e.to_string())?;
        Ok((v, 元素.标量()))
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

    /// 常量下标的元素指针（inbounds GEP）。
    fn 元素指针(
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

    /// 动态下标的元素指针。
    fn 元素指针动态(
        &self,
        base: PointerValue<'ctx>,
        elem_ty: inkwell::types::BasicTypeEnum<'ctx>,
        idx: inkwell::values::IntValue<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        unsafe {
            self.builder
                .build_in_bounds_gep(elem_ty, base, &[idx], "arrslot")
                .map_err(|e| e.to_string())
        }
    }
}
