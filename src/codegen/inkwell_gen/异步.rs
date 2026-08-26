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

use super::后端;
use super::类型::{元素类型, Qi类型};
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

        self.module.add_function(
            "qi_future_ready_i64",
            ptrt.fn_type(&[i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_future_ready_f64",
            ptrt.fn_type(&[f64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_future_ready_bool",
            ptrt.fn_type(&[i32t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_future_ready_ptr",
            ptrt.fn_type(&[ptrt.into()], false),
            None,
        );
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

        self.module.add_function(
            "qi_future_await_i64",
            i64t.fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_future_await_f64",
            f64t.fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_future_await_bool",
            i32t.fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_future_await_ptr",
            ptrt.fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_future_await_string",
            ptrt.fn_type(&[ptrt.into()], false),
            None,
        );
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
            元素类型::指针 | 元素类型::结构体(_) => {
                // 字符串走 ready_string(ptr,len)；其它指针（含结构体）走 ready_ptr
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
            .basic()
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
                // ARC：结构体/数组/闭包经 ready_ptr 存指针不拷贝 —— 发送即转移：
                // BORROWED 先 retain（payload 随 await take / future free 释放）
                if self.弧开()
                    && matches!(vt, Qi类型::结构体(_) | Qi类型::数组(_) | Qi类型::函数值(_))
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
                    .basic()
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
        // QI_CORO：协程体内 `等待 让出()` / `等待 异步睡眠(ms)` = 真挂起点（交回 executor）。
        // 非挂起原语的 等待（如 等待 某 future）在协程内仍走下方 eager 路径。
        if self.协程当前.is_some() {
            if let Some(r) = self.尝试协程挂起(expr)? {
                return Ok(r);
            }
        }
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
        // R4：协程体内 `等待 <future>` → 协作式轮询（poll 未就绪则 让出+挂起，resume 再 poll）。
        // 就绪后再走下方取值（此刻已就绪，await FFI 立即返回不阻塞、协程 future 也不再
        // re-entrant 驱动）。① 协程内 await 另一协程不崩；② await 异步 IO 让出 → 真 IO 上车。
        if self.协程当前.is_some() {
            self.协程等待轮询(fut)?;
        }
        // 结构体 future 单独走「延迟解码」路径：payload 可能是
        // - 结构体指针（普通 eager future）→ 原样返回；
        // - 原始回复字符串（异步询问::<T> 的在飞 future）→ 调 __异步询问收$T
        //   （JSON 解码 + 建 T）。运行时按 RC header magic 区分（qi_rc_is_string）。
        if let 元素类型::结构体(i) = 内部 {
            return self.生成结构体等待(fut, i);
        }
        let (ffi, ret) = match 内部 {
            元素类型::浮点数 => ("qi_future_await_f64", Qi类型::浮点数),
            元素类型::布尔 => ("qi_future_await_bool", Qi类型::布尔),
            元素类型::指针 => ("qi_future_await_ptr", Qi类型::字符串),
            元素类型::结构体(_) => unreachable!("结构体 future 已在上方分流"),
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
            .basic()
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

    /// R4：协程体内 `等待 <future>` 的协作式轮询循环（仅 协程当前.is_some() 时调）。
    /// ```text
    ///   br poll
    /// poll:
    ///   st = qi_coro_await_poll(fut)     ; 1=就绪 0=未就绪
    ///   br st!=0, ready, wait
    /// wait:
    ///   qi_coro_yield_ready()
    ///   <生成挂起点(false)>              ; 挂起 → 控制权回 executor（驱动被等协程/等 IO）
    ///   br poll
    /// ready:                             ; builder 停在此，后续取值
    /// ```
    fn 协程等待轮询(&mut self, fut: BasicValueEnum<'ctx>) -> Result<(), String> {
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let i32t = self.ctx.i32_type();
        let func = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or_else(|| "协程等待轮询：无当前函数".to_string())?;
        let futp = if fut.is_pointer_value() {
            fut
        } else {
            self.builder
                .build_int_to_ptr(fut.into_int_value(), ptrt, "f2p")
                .map_err(|e| e.to_string())?
                .into()
        };
        let poll = match self.module.get_function("qi_coro_await_poll") {
            Some(f) => f,
            None => self.module.add_function(
                "qi_coro_await_poll",
                i32t.fn_type(&[ptrt.into()], false),
                None,
            ),
        };
        let yr = match self.module.get_function("qi_coro_yield_ready") {
            Some(f) => f,
            None => self.module.add_function(
                "qi_coro_yield_ready",
                self.ctx.void_type().fn_type(&[], false),
                None,
            ),
        };
        let poll_bb = self.ctx.append_basic_block(func, "coawait.poll");
        let wait_bb = self.ctx.append_basic_block(func, "coawait.wait");
        let ready_bb = self.ctx.append_basic_block(func, "coawait.ready");
        self.builder
            .build_unconditional_branch(poll_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(poll_bb);
        let st = self
            .builder
            .build_call(poll, &[futp.into()], "coawait.st")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "qi_coro_await_poll 无返回".to_string())?
            .into_int_value();
        let 就绪 = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::NE,
                st,
                i32t.const_zero(),
                "coawait.rdy",
            )
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(就绪, ready_bb, wait_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(wait_bb);
        self.builder
            .build_call(yr, &[], "")
            .map_err(|e| e.to_string())?;
        self.生成挂起点(false)?; // 挂起：resume 后落到其 0-case 块
        self.builder
            .build_unconditional_branch(poll_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(ready_bb);
        Ok(())
    }

    /// `等待 未来<结构体 T>` 的延迟解码 IR：
    /// ```text
    ///   p  = qi_future_await_ptr(fut)          ; take payload
    ///   is = qi_rc_is_string(p)                ; STR magic 判别
    ///   br is != 0, 解码块, 汇合块
    /// 解码块:                                   ; 异步询问 的原始回复串
    ///   sv = __异步询问收$T(p)                  ; JSON 解码 + 建 T（owned 交出）
    ///   release(p)                              ; 回复串消费完毕（ARC 门控）
    ///   br 汇合块
    /// 汇合块:
    ///   phi [p, 直通], [sv, 解码块尾]           ; 两路都是 T 结构体指针（owned）
    /// ```
    /// 普通 eager 结构体 future（payload 是结构体指针）走直通路，行为不变。
    fn 生成结构体等待(
        &mut self,
        fut: BasicValueEnum<'ctx>,
        idx: u32,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let i64t = self.ctx.i64_type();

        let await_f = self
            .module
            .get_function("qi_future_await_ptr")
            .ok_or_else(|| "future 运行时未声明: qi_future_await_ptr".to_string())?;
        let p = self
            .builder
            .build_call(await_f, &[fut.into()], "await")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "await 未返回".to_string())?;

        // is = qi_rc_is_string(p)（原型幂等补声明）
        let is_str_f = match self.module.get_function("qi_rc_is_string") {
            Some(f) => f,
            None => self.module.add_function(
                "qi_rc_is_string",
                i64t.fn_type(&[ptrt.into()], false),
                None,
            ),
        };
        let is_str = self
            .builder
            .build_call(is_str_f, &[p.into()], "is_str")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "qi_rc_is_string 未返回".to_string())?
            .into_int_value();
        let cond = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::NE,
                is_str,
                i64t.const_zero(),
                "is_str_b",
            )
            .map_err(|e| e.to_string())?;

        let 当前函数 = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or_else(|| "等待：无当前函数".to_string())?;
        let 直通块 = self
            .builder
            .get_insert_block()
            .ok_or_else(|| "等待：无当前块".to_string())?;
        let 解码块 = self.ctx.append_basic_block(当前函数, "await_decode");
        let 汇合块 = self.ctx.append_basic_block(当前函数, "await_merge");
        self.builder
            .build_conditional_branch(cond, 解码块, 汇合块)
            .map_err(|e| e.to_string())?;

        // 解码块：直接 IR 调 __异步询问收$T(p)（param 借用字符串，返回 owned T）。
        // 不落 变量表 / 不建条件块内 alloca —— 避免 ARC 作用域清理的支配性问题。
        self.builder.position_at_end(解码块);
        let 收名 = self.登记异步询问收(idx)?;
        let (收f, _) = self
            .尝试解析用户函数(&收名, 1)
            .ok_or_else(|| format!("异步询问收未声明: {}", 收名))?;
        let sv = self
            .builder
            .build_call(收f, &[p.into()], "aw_decode")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "异步询问收：无返回值".to_string())?;
        // 回复串消费完毕（await_ptr 交出的 +1）—— ARC 门控释放
        self.弧release(p);
        let 解码尾块 = self
            .builder
            .get_insert_block()
            .ok_or_else(|| "等待：解码块丢失".to_string())?;
        self.builder
            .build_unconditional_branch(汇合块)
            .map_err(|e| e.to_string())?;

        // 汇合：两路都是 T 结构体指针（owned +1 交调用方）
        self.builder.position_at_end(汇合块);
        let phi = self
            .builder
            .build_phi(ptrt, "await_s")
            .map_err(|e| e.to_string())?;
        phi.add_incoming(&[(&p, 直通块), (&sv, 解码尾块)]);
        Ok((phi.as_basic_value(), Qi类型::结构体(idx)))
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
        // 必须是**字节**长度：qi_future_ready_string(ptr, len) 按字节 from_raw_parts。
        // 用 qi_string_byte_length（不是 qi_runtime_string_length —— 那个返回**字符**
        // 数，非 ASCII 会把 future payload 截断成半个多字节字符）。
        let f = match self.module.get_function("qi_string_byte_length") {
            Some(f) => f,
            None => self.module.add_function(
                "qi_string_byte_length",
                self.ctx.i64_type().fn_type(&[ptrt.into()], false),
                None,
            ),
        };
        let len = self
            .builder
            .build_call(f, &[p.into()], "sbytelen")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "string_length 未返回".to_string())?
            .into_int_value();
        Ok(len)
    }
}
