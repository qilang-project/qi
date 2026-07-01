//! 通道 + 协程启动（阶段10 退化实现，够让 harness/工具.qi 编译并功能正确）。
//!
//! - `通道<整数>(cap)` → `qi_runtime_create_channel(cap)`，返回 ptr（当整数句柄）。
//! - `ch <- v`         → `qi_runtime_channel_send(ch, v_i64)`。
//! - `<- ch`           → 存 slot，`qi_runtime_channel_receive(ch, &slot)`，load slot(i64)。
//! - `启动 f(args)`    → **同步退化**：直接 `f(args)`（串行执行）。
//!   harness 的通道是缓冲的（cap=个数+1），worker 先 send 再由主循环 receive，
//!   串行下 send-then-receive 不死锁，结果正确（只是不并发）。真 goroutine 后置。

use super::类型::Qi类型;
use super::后端;
use crate::parser::ast::{
    AstNode, ChannelCreateExpression, ChannelReceiveExpression, ChannelSendExpression,
    GoroutineSpawnExpression,
};
use inkwell::values::BasicValueEnum;

impl<'ctx> 后端<'ctx> {
    /// `通道<T>(cap)` → 句柄指针（当整数）。cap 缺省 0（无缓冲）。
    pub(super) fn 生成通道创建(
        &mut self,
        c: &ChannelCreateExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let cap = match &c.capacity {
            Some(e) => {
                let (v, _t) = self
                    .生成表达式(e)?
                    .ok_or_else(|| "通道容量无值".to_string())?;
                v.into_int_value()
            }
            None => self.ctx.i64_type().const_zero(),
        };
        let f = self
            .module
            .get_function("qi_runtime_create_channel")
            .ok_or_else(|| "运行时函数未声明: qi_runtime_create_channel".to_string())?;
        let cs = self
            .builder
            .build_call(f, &[cap.into()], "chan")
            .map_err(|e| e.to_string())?;
        let ptr = cs
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "create_channel 未返回".to_string())?;
        // 句柄按整数（ptr 在 64 位与 i64 等价）暴露给 Qi
        Ok((ptr, Qi类型::整数))
    }

    /// `ch <- v` → qi_runtime_channel_send(ch_ptr, v_i64)。返回状态整数。
    pub(super) fn 生成通道发送(
        &mut self,
        s: &ChannelSendExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let ch = self.求通道指针(&s.channel)?;
        let (v, _t) = self
            .生成表达式(&s.value)?
            .ok_or_else(|| "通道发送值无值".to_string())?;
        let f = self
            .module
            .get_function("qi_runtime_channel_send")
            .ok_or_else(|| "运行时函数未声明: qi_runtime_channel_send".to_string())?;
        let cs = self
            .builder
            .build_call(f, &[ch.into(), v.into()], "chsend")
            .map_err(|e| e.to_string())?;
        let r = cs
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "channel_send 未返回".to_string())?;
        // i32 状态 → 扩到 i64
        let r64 = self
            .builder
            .build_int_z_extend(r.into_int_value(), self.ctx.i64_type(), "st")
            .map_err(|e| e.to_string())?;
        Ok((r64.into(), Qi类型::整数))
    }

    /// `<- ch` → slot=alloca i64; receive(ch, &slot); load slot。返回 i64。
    pub(super) fn 生成通道接收(
        &mut self,
        r: &ChannelReceiveExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let ch = self.求通道指针(&r.channel)?;
        let i64t = self.ctx.i64_type();
        let slot = self
            .builder
            .build_alloca(i64t, "recvslot")
            .map_err(|e| e.to_string())?;
        let f = self
            .module
            .get_function("qi_runtime_channel_receive")
            .ok_or_else(|| "运行时函数未声明: qi_runtime_channel_receive".to_string())?;
        self.builder
            .build_call(f, &[ch.into(), slot.into()], "chrecv")
            .map_err(|e| e.to_string())?;
        let v = self
            .builder
            .build_load(i64t, slot, "recvval")
            .map_err(|e| e.to_string())?;
        Ok((v, Qi类型::整数))
    }

    /// `启动 f(args)` —— 同步退化：直接执行表达式（串行）。返回空。
    pub(super) fn 生成协程启动(
        &mut self,
        g: &GoroutineSpawnExpression,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        // 退化：直接调用被启动的表达式（通常是函数调用）
        self.生成表达式(&g.expression)?;
        Ok(None)
    }

    /// 求通道句柄的 ptr 值（Qi 侧句柄是整数，转回 ptr 传给运行时）。
    fn 求通道指针(
        &mut self,
        node: &AstNode,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let (v, _t) = self
            .生成表达式(node)?
            .ok_or_else(|| "通道表达式无值".to_string())?;
        if v.is_pointer_value() {
            Ok(v.into_pointer_value())
        } else {
            // 句柄是 i64 → int_to_ptr
            let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
            self.builder
                .build_int_to_ptr(v.into_int_value(), ptrt, "ch2ptr")
                .map_err(|e| e.to_string())
        }
    }
}
