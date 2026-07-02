//! 异步 —— 同步 future 模型（eager future），与老后端 ABI 平齐。
//!
//! 语义：不是真协程。`未来<T>` = 一个 future 句柄指针；返回 `未来<T>` 的函数把
//! 算出的 T 值包成 ready future（`qi_future_ready_T`）；`等待 fut` 用
//! `qi_future_await_T(fut)` 取回 T 值。`未来::就绪(v)`=ready，`未来::失败(s)`=failed。
//!
//! runtime FFI（qi-runtime 归档已确认存在）：
//!   qi_future_ready_i64(i64)->ptr / _f64(double)->ptr / _bool(i32)->ptr
//!   qi_future_ready_ptr(ptr)->ptr / _string(ptr,len)->ptr
//!   qi_future_await_i64(ptr)->i64 / _f64->double / _bool->i32 / _ptr->ptr / _string->ptr(c_char)
//!   qi_future_failed(ptr,len)->ptr

use super::类型::{Qi类型, 元素类型};
use super::后端;
use crate::parser::ast::AstNode;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;

impl<'ctx> 后端<'ctx> {
    /// 声明 future runtime 原型（幂等，在 声明运行时 里调）。
    pub(super) fn 声明future运行时(&self) {
        let i32t = self.ctx.i32_type();
        let i64t = self.ctx.i64_type();
        let f64t = self.ctx.f64_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        self.module
            .add_function("qi_future_ready_i64", ptrt.fn_type(&[i64t.into()], false), None);
        self.module
            .add_function("qi_future_ready_f64", ptrt.fn_type(&[f64t.into()], false), None);
        self.module
            .add_function("qi_future_ready_bool", ptrt.fn_type(&[i32t.into()], false), None);
        self.module
            .add_function("qi_future_ready_ptr", ptrt.fn_type(&[ptrt.into()], false), None);
        self.module.add_function(
            "qi_future_ready_string",
            ptrt.fn_type(&[ptrt.into(), i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_future_failed",
            ptrt.fn_type(&[ptrt.into(), i64t.into()], false),
            None,
        );

        self.module
            .add_function("qi_future_await_i64", i64t.fn_type(&[ptrt.into()], false), None);
        self.module
            .add_function("qi_future_await_f64", f64t.fn_type(&[ptrt.into()], false), None);
        self.module
            .add_function("qi_future_await_bool", i32t.fn_type(&[ptrt.into()], false), None);
        self.module
            .add_function("qi_future_await_ptr", ptrt.fn_type(&[ptrt.into()], false), None);
        self.module
            .add_function("qi_future_await_string", ptrt.fn_type(&[ptrt.into()], false), None);
    }

    /// 把一个 T 值包成 ready future 指针。返回 (future_ptr, 未来(元素))。
    pub(super) fn 包装ready(
        &mut self,
        v: BasicValueEnum<'ctx>,
        vt: Qi类型,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let 元素 = 元素类型::从标量(vt);
        let (ffi, arg): (&str, Vec<inkwell::values::BasicMetadataValueEnum>) = match 元素 {
            元素类型::浮点数 => {
                let fv = if vt.是浮点() {
                    v
                } else {
                    self.builder
                        .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "s2f")
                        .map_err(|e| e.to_string())?
                        .into()
                };
                ("qi_future_ready_f64", vec![fv.into()])
            }
            元素类型::布尔 => {
                let b = self
                    .builder
                    .build_int_z_extend(v.into_int_value(), self.ctx.i32_type(), "b2i32")
                    .map_err(|e| e.to_string())?;
                ("qi_future_ready_bool", vec![b.into()])
            }
            元素类型::指针 => {
                // 字符串走 ready_string(ptr,len)；其它指针走 ready_ptr
                if vt == Qi类型::字符串 {
                    let len = self.计算字符串长度(v)?;
                    ("qi_future_ready_string", vec![v.into(), len.into()])
                } else {
                    let p = if v.is_pointer_value() {
                        v
                    } else {
                        self.builder
                            .build_int_to_ptr(
                                v.into_int_value(),
                                self.ctx.ptr_type(AddressSpace::default()),
                                "i2p",
                            )
                            .map_err(|e| e.to_string())?
                            .into()
                    };
                    ("qi_future_ready_ptr", vec![p.into()])
                }
            }
            元素类型::整数 => ("qi_future_ready_i64", vec![v.into()]),
        };
        let f = self
            .module
            .get_function(ffi)
            .ok_or_else(|| format!("future 运行时未声明: {}", ffi))?;
        let fut = self
            .builder
            .build_call(f, &arg, "ready")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "ready 未返回".to_string())?;
        Ok(fut)
    }

    /// `未来::就绪(v)` / `未来::失败(s)` 静态方法。返回 None 表示不是这俩。
    pub(super) fn 生成未来静态方法(
        &mut self,
        method: &str,
        arguments: &[AstNode],
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let method = method.trim_start_matches(':');
        match method {
            "就绪" | "ready" => {
                let (v, vt) = self
                    .生成表达式(&arguments[0])?
                    .ok_or_else(|| "未来::就绪 实参无值".to_string())?;
                // ARC：结构体/数组经 ready_ptr 存指针不拷贝 —— 发送即转移：
                // BORROWED 先 retain（随 future 泄漏，宁泄漏不悬垂）
                if self.弧开()
                    && matches!(vt, Qi类型::结构体(_) | Qi类型::数组(_))
                    && !self.表达式拥有RC(&arguments[0], vt)
                {
                    self.弧retain任意(v, vt);
                }
                let fut = self.包装ready(v, vt)?;
                // ARC：ready_string 已把字节拷进 future —— OWNED 字符串源释放
                // （结构体/数组已转移进 future，不释放）
                self.弧消费后释放(v, vt, &arguments[0]);
                Ok(Some((fut, Qi类型::未来(元素类型::从标量(vt)))))
            }
            "失败" | "failed" => {
                let (v, vt) = self
                    .生成表达式(&arguments[0])?
                    .ok_or_else(|| "未来::失败 实参无值".to_string())?;
                let len = self.计算字符串长度(v)?;
                let f = self
                    .module
                    .get_function("qi_future_failed")
                    .ok_or_else(|| "future 运行时未声明: qi_future_failed".to_string())?;
                let fut = self
                    .builder
                    .build_call(f, &[v.into(), len.into()], "failed")
                    .map_err(|e| e.to_string())?
                    .try_as_basic_value()
                    .left()
                    .ok_or_else(|| "failed 未返回".to_string())?;
                // ARC：qi_future_failed 已拷贝错误消息 —— OWNED 字符串源释放
                self.弧消费后释放(v, vt, &arguments[0]);
                // 失败 future 的内部类型未知，按整数占位（await 时按目标类型定）
                Ok(Some((fut, Qi类型::未来(元素类型::整数))))
            }
            _ => Ok(None),
        }
    }

    /// `等待 表达式`：求 future 指针 → qi_future_await_T 取回 T 值。
    /// T 由 future 的内部类型定；未知时按整数。
    pub(super) fn 生成等待(
        &mut self,
        expr: &AstNode,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let (v, t) = self
            .生成表达式(expr)?
            .ok_or_else(|| "等待操作数无值".to_string())?;
        let 内部 = t.未来内部().unwrap_or(元素类型::整数);
        let fut = if v.is_pointer_value() {
            v
        } else {
            self.builder
                .build_int_to_ptr(
                    v.into_int_value(),
                    self.ctx.ptr_type(AddressSpace::default()),
                    "i2p",
                )
                .map_err(|e| e.to_string())?
                .into()
        };
        let (ffi, ret) = match 内部 {
            元素类型::浮点数 => ("qi_future_await_f64", Qi类型::浮点数),
            元素类型::布尔 => ("qi_future_await_bool", Qi类型::布尔),
            元素类型::指针 => ("qi_future_await_ptr", Qi类型::字符串),
            元素类型::整数 => ("qi_future_await_i64", Qi类型::整数),
        };
        let f = self
            .module
            .get_function(ffi)
            .ok_or_else(|| format!("future 运行时未声明: {}", ffi))?;
        let mut val = self
            .builder
            .build_call(f, &[fut.into()], "await")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "await 未返回".to_string())?;
        // await_bool 回来是 i32，截回 i1
        if ret == Qi类型::布尔 {
            val = self
                .builder
                .build_int_truncate(val.into_int_value(), self.ctx.bool_type(), "b2i1")
                .map_err(|e| e.to_string())?
                .into();
        }
        Ok((val, ret))
    }

    /// 计算一个字符串值（i8* 或整数句柄）的字节长度 —— 调 qi_runtime_string_length。
    fn 计算字符串长度(
        &mut self,
        v: BasicValueEnum<'ctx>,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let p = if v.is_pointer_value() {
            v.into_pointer_value()
        } else {
            self.builder
                .build_int_to_ptr(v.into_int_value(), ptrt, "i2p")
                .map_err(|e| e.to_string())?
        };
        let f = match self.module.get_function("qi_runtime_string_length") {
            Some(f) => f,
            None => self.module.add_function(
                "qi_runtime_string_length",
                self.ctx.i64_type().fn_type(&[ptrt.into()], false),
                None,
            ),
        };
        let len = self
            .builder
            .build_call(f, &[p.into()], "slen")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "string_length 未返回".to_string())?
            .into_int_value();
        Ok(len)
    }
}
