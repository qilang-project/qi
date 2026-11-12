//! `选择 { 情况 … : …  默认: …  情况 超时(ms): … }` —— 通道多路选择。
//!
//! 从 并发.rs 拆出来（那边收发/启动已经够长）。收到的值按通道元素类型还原、
//! 发送值按位模式装进 i64，两边都复用 并发.rs 的 value_to_channel_bits /
//! channel_bits_to_value，跟 `ch <- v` / `<- ch` 走同一套规则。

use super::后端;
use super::类型::{Qi类型, 元素类型};
use crate::parser::ast::{SelectCaseKind, SelectExpression};
use inkwell::basic_block::BasicBlock;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::AddressSpace;
use inkwell::IntPredicate;

/// 一个通道分支进入 选择 时预先求好的东西（轮询时不重复副作用）。
struct SelectArm<'ctx> {
    channel: PointerValue<'ctx>,
    /// 发送分支：已装成 i64 的值；接收分支：None。
    send_bits: Option<IntValue<'ctx>>,
    /// 发送分支里的 RC 值（字符串/结构体/数组）：进 选择 前已 +1，
    /// 没被选中时要还回去。
    send_rc: Option<(BasicValueEnum<'ctx>, Qi类型)>,
    /// 接收分支：通道元素类型（还原收到的位模式用）。
    element: Option<元素类型>,
}

impl<'ctx> 后端<'ctx> {
    /// 最小可用降级：非阻塞轮询各通道 case（协程通道 `qi_coro_chan_try_*`，
    /// eager 通道 `qi_runtime_channel_try_*`）：
    ///   - 有 `默认` 分支：单轮轮询，全部未就绪立刻走 默认（Go 语义）。
    ///   - 有 `超时(ms)` 分支：轮询 + `qi_runtime_select_backoff`（~1ms）退避，
    ///     到截止时间走 超时 体。
    ///   - 两者皆无：轮询 + 退避直到某 case 就绪（阻塞语义）。
    ///
    /// 通道表达式 / 发送值 / 超时毫秒只在进入 选择 时求值一次。
    /// 接收 case 的 `变量 := <-ch` 按通道元素类型绑定（字符串通道绑字符串；
    /// 以前一律绑成整数，打印出来是指针地址）。
    pub(super) fn 生成选择(
        &mut self,
        s: &SelectExpression,
        func: FunctionValue<'ctx>,
    ) -> Result<(), String> {
        let i32t = self.ctx.i32_type();
        let i64t = self.ctx.i64_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        // FFI 原型（幂等声明）。协程模式下通道由 qi_coro_chan_new 创建（QiCoroChan
        // 布局），必须用协程原生非阻塞收发；否则把 QiCoroChan* 当 eager 通道解引用 → 类型
        // 混淆。签名一致：try_recv=(ptr,ptr)->i32、try_send=(ptr,i64)->i32。
        let coro_channel = self.协程开();
        let try_recv = self.取或声明函数(
            if coro_channel {
                "qi_coro_chan_try_recv"
            } else {
                "qi_runtime_channel_try_receive"
            },
            i32t.fn_type(&[ptrt.into(), ptrt.into()], false),
        );
        let try_send = self.取或声明函数(
            if coro_channel {
                "qi_coro_chan_try_send"
            } else {
                "qi_runtime_channel_try_send"
            },
            i32t.fn_type(&[ptrt.into(), i64t.into()], false),
        );
        let backoff = self.取或声明函数(
            "qi_runtime_select_backoff",
            self.ctx.void_type().fn_type(&[], false),
        );
        // 协程模式顶层 select 需驱动执行器（否则发送方协程永不运行 → 全未就绪空转）。
        // 运行时在协程 resume 中被调时什么也不做（不嵌套 resume）。
        let step_once = if coro_channel {
            Some(self.取或声明函数("qi_coro_step_once", i64t.fn_type(&[], false)))
        } else {
            None
        };

        let mut arms: Vec<SelectArm<'ctx>> = Vec::new();
        for case in &s.cases {
            match &case.kind {
                SelectCaseKind::通道接收 { channel, .. } => {
                    let (chv, cht) = self
                        .生成表达式(channel)?
                        .ok_or_else(|| "选择 通道表达式无值".to_string())?;
                    let channel = self.通道值转指针(chv)?;
                    arms.push(SelectArm {
                        channel,
                        send_bits: None,
                        send_rc: None,
                        element: cht.通道元素(),
                    });
                }
                SelectCaseKind::通道发送 { channel, value } => {
                    let channel = self.求通道指针(channel)?;
                    let (v, t) = self
                        .生成表达式(value)?
                        .ok_or_else(|| "选择 发送值无值".to_string())?;
                    // ARC：同 `ch <- v` —— 借用的 RC 值先 +1，随值移交接收方。
                    let send_rc =
                        if self.弧开() && v.is_pointer_value() && super::所有权::是RC类型(t)
                        {
                            self.弧存入槽2(v, value, t);
                            Some((v, t))
                        } else {
                            None
                        };
                    let bits = self.value_to_channel_bits(v)?;
                    arms.push(SelectArm {
                        channel,
                        send_bits: Some(bits),
                        send_rc,
                        element: None,
                    });
                }
                // 语法层已把 默认/超时 分离进 default_case / timeout_case
                _ => return Err("选择：默认/超时 分支位置异常".to_string()),
            }
        }

        // 超时：截止时刻 = 进入时刻 + 毫秒（只求一次）
        let timeout = match &s.timeout_case {
            Some(tc) => {
                let SelectCaseKind::超时 { 毫秒: millis } = &tc.kind else {
                    return Err("选择：超时分支结构异常".to_string());
                };
                let get_ms =
                    self.取或声明函数("qi_runtime_get_time_ms", i64t.fn_type(&[], false));
                let (msv, _) = self
                    .生成表达式(millis)?
                    .ok_or_else(|| "选择 超时毫秒无值".to_string())?;
                let start = self
                    .builder
                    .build_call(get_ms, &[], "sel.start")
                    .map_err(|e| e.to_string())?
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "get_time_ms 未返回".to_string())?
                    .into_int_value();
                let deadline = self
                    .builder
                    .build_int_add(start, msv.into_int_value(), "sel.deadline")
                    .map_err(|e| e.to_string())?;
                Some((get_ms, deadline, tc))
            }
            None => None,
        };

        // 接收 slot（entry 块 alloca，轮询循环里复用不吃栈）。协程通道 try_recv 写裸 i64
        // （单层）；eager try_receive 写 boxed 指针（双层）。故槽类型按模式分。
        let slot = if coro_channel {
            self.入口块alloca(i64t.into(), "sel.slot")?
        } else {
            self.入口块alloca(ptrt.into(), "sel.slot")?
        };

        let poll_bb = self.ctx.append_basic_block(func, "select.poll");
        let end_bb = self.ctx.append_basic_block(func, "select.end");
        self.builder
            .build_unconditional_branch(poll_bb)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(poll_bb);

        // 轮询链：逐 case 非阻塞试收/试发，就绪即跳该 case 体
        let mut body_blocks = Vec::new();
        for (i, arm) in arms.iter().enumerate() {
            let body_bb = self
                .ctx
                .append_basic_block(func, &format!("select.body{}", i));
            let next_bb = self
                .ctx
                .append_basic_block(func, &format!("select.try{}", i));
            let call = match arm.send_bits {
                Some(bits) => self
                    .builder
                    .build_call(try_send, &[arm.channel.into(), bits.into()], "sel.trysend")
                    .map_err(|e| e.to_string())?,
                None => self
                    .builder
                    .build_call(try_recv, &[arm.channel.into(), slot.into()], "sel.tryrecv")
                    .map_err(|e| e.to_string())?,
            };
            let rc = call
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| "try_receive/try_send 未返回".to_string())?
                .into_int_value();
            let ok = self
                .builder
                .build_int_compare(IntPredicate::EQ, rc, i32t.const_zero(), "sel.ok")
                .map_err(|e| e.to_string())?;
            self.builder
                .build_conditional_branch(ok, body_bb, next_bb)
                .map_err(|e| e.to_string())?;
            self.builder.position_at_end(next_bb);
            body_blocks.push(body_bb);
        }

        // 全部未就绪
        if let Some(dc) = &s.default_case {
            // 默认：单轮判定，立刻走 默认 体（不退避不循环）
            self.release_unsent_select_values(&arms, None);
            self.生成块(&dc.body, func)?;
            self.跳转若未终结(end_bb)?;
        } else if let Some((get_ms, deadline, tc)) = &timeout {
            let now = self
                .builder
                .build_call(*get_ms, &[], "sel.now")
                .map_err(|e| e.to_string())?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| "get_time_ms 未返回".to_string())?
                .into_int_value();
            let expired = self
                .builder
                .build_int_compare(IntPredicate::SGE, now, *deadline, "sel.expired")
                .map_err(|e| e.to_string())?;
            let timeout_bb = self.ctx.append_basic_block(func, "select.timeout");
            let wait_bb = self.ctx.append_basic_block(func, "select.wait");
            self.builder
                .build_conditional_branch(expired, timeout_bb, wait_bb)
                .map_err(|e| e.to_string())?;
            self.builder.position_at_end(wait_bb);
            self.select_wait_round(step_once, backoff, poll_bb)?;
            self.builder.position_at_end(timeout_bb);
            self.release_unsent_select_values(&arms, None);
            self.生成块(&tc.body, func)?;
            self.跳转若未终结(end_bb)?;
        } else {
            // 无 默认/超时：退避后再轮询（阻塞语义）。
            self.select_wait_round(step_once, backoff, poll_bb)?;
        }

        // 各 case 体
        for (i, (case, body_bb)) in s.cases.iter().zip(body_blocks).enumerate() {
            self.builder.position_at_end(body_bb);
            self.release_unsent_select_values(&arms, Some(i));
            let SelectCaseKind::通道接收 { variable, .. } = &case.kind else {
                self.生成块(&case.body, func)?;
                self.跳转若未终结(end_bb)?;
                continue;
            };
            // 协程通道：slot 直接是 i64（单层）；eager：slot 是 boxed 指针（双层）。
            let bits = if coro_channel {
                self.builder
                    .build_load(i64t, slot, "sel.val")
                    .map_err(|e| e.to_string())?
                    .into_int_value()
            } else {
                let boxed = self
                    .builder
                    .build_load(ptrt, slot, "sel.box")
                    .map_err(|e| e.to_string())?
                    .into_pointer_value();
                self.builder
                    .build_load(i64t, boxed, "sel.val")
                    .map_err(|e| e.to_string())?
                    .into_int_value()
            };
            let (value, value_type) = self.channel_bits_to_value(bits, arms[i].element)?;
            let Some(name) = variable else {
                // `情况 <- ch:` 不绑定：收到的 RC 值（发送方移交的 +1）没人要，还掉。
                self.弧release任意(value, value_type);
                self.生成块(&case.body, func)?;
                self.跳转若未终结(end_bb)?;
                continue;
            };
            // 绑定只在本分支可见：自己一层作用域。
            self.符号.进入作用域();
            self.作用域遮蔽栈.push(Vec::new());
            let result = self
                .bind_select_receive(func, name, value, value_type)
                .and_then(|_| self.生成块(&case.body, func));
            self.弹作用域帧();
            self.符号.退出作用域();
            result?;
            self.跳转若未终结(end_bb)?;
        }

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    /// 接收分支的 `名 := <- ch`：按元素类型建槽。RC 类型（字符串/结构体）用 entry 块
    /// null 初始化的 RC 槽 —— 收到的值是发送方移交的 +1（OWNED），直接存入；
    /// 循环里再次进入先释放上一轮的旧值；出口由 弧释放局部 统一释放。
    fn bind_select_receive(
        &mut self,
        func: FunctionValue<'ctx>,
        name: &str,
        value: BasicValueEnum<'ctx>,
        value_type: Qi类型,
    ) -> Result<(), String> {
        let rc_slot = self.弧开() && super::所有权::是RC类型(value_type);
        let slot = if rc_slot {
            let slot = self.弧字符串槽(func, name)?;
            self.弧释放槽旧值2(slot, value_type)?;
            slot
        } else {
            let llvm_type = self
                .llvm基础类型(value_type)
                .ok_or_else(|| format!("选择 绑定 {} 类型无效", name))?;
            self.入口块alloca(llvm_type, name)?
        };
        self.builder
            .build_store(slot, value)
            .map_err(|e| e.to_string())?;
        self.声明局部槽(name, slot, value_type);
        self.符号.声明变量(name, value_type);
        Ok(())
    }

    /// 进某个分支体前：没被选中的发送分支，进 选择 时为它 +1 的 RC 值还回去
    /// （值没发出去）。`chosen` 是被选中的分支下标；默认/超时 分支传 None。
    fn release_unsent_select_values(&mut self, arms: &[SelectArm<'ctx>], chosen: Option<usize>) {
        for (i, arm) in arms.iter().enumerate() {
            if Some(i) == chosen {
                continue;
            }
            if let Some((v, t)) = arm.send_rc {
                self.弧release任意(v, t);
            }
        }
    }

    /// 全部未就绪时等一轮：协程模式先驱动执行器一步，再退避，回轮询。
    fn select_wait_round(
        &mut self,
        step_once: Option<FunctionValue<'ctx>>,
        backoff: FunctionValue<'ctx>,
        poll_bb: BasicBlock<'ctx>,
    ) -> Result<(), String> {
        if let Some(step) = step_once {
            self.builder
                .build_call(step, &[], "")
                .map_err(|e| e.to_string())?;
        }
        self.builder
            .build_call(backoff, &[], "")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_unconditional_branch(poll_bb)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
