//! 通道 + 协程启动（阶段10 退化实现，够让 harness/工具.qi 编译并功能正确）。
//!
//! - `通道<整数>(cap)` → `qi_runtime_create_channel(cap)`，返回 ptr（当整数句柄）。
//! - `ch <- v`         → `qi_runtime_channel_send(ch, v_i64)`。
//! - `<- ch`           → 存 slot，`qi_runtime_channel_receive(ch, &slot)`，load slot(i64)。
//! - `启动 表达式`    → **真并发**：把表达式包成 nullary 闭包（fat obj），经共享
//!   trampoline 丢到 `qi_runtime_spawn_goroutine_with_args` 的 tokio 线程池执行。
//!   fire-and-forget，同步靠通道 / 等待组。见 生成协程启动。

use super::后端;
use super::类型::Qi类型;
use crate::parser::ast::{
    AstNode, ChannelCreateExpression, ChannelReceiveExpression, ChannelSendExpression,
    ExpressionStatement, GoroutineSpawnExpression, SelectCaseKind, SelectExpression,
};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::AddressSpace;
use inkwell::IntPredicate;

impl<'ctx> 后端<'ctx> {
    /// 同步 / 定时器内建（无模块限定的并发原语，如 创建等待组()、等待组完成(wg)）。
    /// 返回 None 表示不是这类内建。句柄一律按 整数(i64) 暴露给 Qi；FFI 侧句柄参数用 ptr。
    pub(super) fn 生成同步内建(
        &mut self,
        callee: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        // (FFI 名, 参数是否为句柄ptr的布尔序列, 额外整数参数个数, 返回是否句柄ptr)
        // 简化：用 (FFI, 参数种类列表, 返回种类)。种类: 'h'=句柄ptr, 'i'=i32整数, 'l'=i64整数, 'r'=返回ptr句柄, 'v'=i32/i64返回
        let (ffi, 参数种类, 返回句柄): (&str, &[char], bool) = match callee {
            "创建等待组" | "新建等待组" => ("qi_runtime_waitgroup_create", &[], true),
            "等待组增加" | "等待组添加" | "添加等待" => {
                ("qi_runtime_waitgroup_add", &['h', 'i'], false)
            }
            "等待组完成" | "完成" => ("qi_runtime_waitgroup_done", &['h'], false),
            "等待组等待" => ("qi_runtime_waitgroup_wait", &['h'], false),
            "创建互斥锁" | "新建互斥锁" => ("qi_runtime_mutex_create", &[], true),
            "互斥锁加锁" | "互斥锁锁定" | "加锁" => {
                ("qi_runtime_mutex_lock", &['h'], false)
            }
            "互斥锁解锁" | "解锁" => ("qi_runtime_mutex_unlock", &['h'], false),
            "尝试加锁" => ("qi_runtime_mutex_trylock", &['h'], false),
            "获取时间" => ("qi_runtime_get_time_ms", &[], false),
            "设置超时" => ("qi_runtime_set_timeout", &['l'], false),
            "创建定时器" => ("qi_runtime_timer_create", &['l'], true),
            "定时器过期" => ("qi_runtime_timer_expired", &['h'], false),
            "停止定时器" => ("qi_runtime_timer_stop", &['h'], false),
            _ => return Ok(None),
        };

        let i32t = self.ctx.i32_type();
        let i64t = self.ctx.i64_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        // 声明原型（幂等）
        let func = match self.module.get_function(ffi) {
            Some(f) => f,
            None => {
                let mut ps: Vec<inkwell::types::BasicMetadataTypeEnum> = Vec::new();
                for k in 参数种类 {
                    ps.push(match k {
                        'h' => ptrt.into(),
                        'i' => i32t.into(),
                        _ => i64t.into(), // 'l'
                    });
                }
                let ft = if 返回句柄 {
                    ptrt.fn_type(&ps, false)
                } else if matches!(
                    ffi,
                    "qi_runtime_get_time_ms"
                        | "qi_runtime_set_timeout"
                        | "qi_runtime_timer_expired"
                        | "qi_runtime_timer_stop"
                ) {
                    // get_time/set_timeout/timer 返回 i64
                    i64t.fn_type(&ps, false)
                } else {
                    // waitgroup/mutex 返回 i32
                    i32t.fn_type(&ps, false)
                };
                self.module.add_function(ffi, ft, None)
            }
        };

        // 实参
        let mut args: Vec<BasicMetadataValueEnum> = Vec::new();
        for (i, k) in 参数种类.iter().enumerate() {
            let (v, _t) = self
                .生成表达式(&arguments[i])?
                .ok_or_else(|| "同步内建实参无值".to_string())?;
            match k {
                'h' => {
                    // 句柄 i64 → ptr
                    let p = if v.is_pointer_value() {
                        v.into_pointer_value()
                    } else {
                        self.builder
                            .build_int_to_ptr(v.into_int_value(), ptrt, "h2p")
                            .map_err(|e| e.to_string())?
                    };
                    args.push(p.into());
                }
                'i' => {
                    let iv = v.into_int_value();
                    let iv = if iv.get_type().get_bit_width() != 32 {
                        self.builder
                            .build_int_truncate(iv, i32t, "t32")
                            .map_err(|e| e.to_string())?
                    } else {
                        iv
                    };
                    args.push(iv.into());
                }
                _ => args.push(v.into()),
            }
        }

        let cs = self
            .builder
            .build_call(func, &args, "syncbi")
            .map_err(|e| e.to_string())?;
        match cs.try_as_basic_value().basic() {
            Some(v) => {
                if 返回句柄 {
                    // ptr 句柄 → i64 暴露
                    let iv = self
                        .builder
                        .build_ptr_to_int(v.into_pointer_value(), i64t, "p2h")
                        .map_err(|e| e.to_string())?;
                    Ok(Some(Some((iv.into(), Qi类型::整数))))
                } else {
                    // i32/i64 返回 → 统一扩到 i64
                    let iv = v.into_int_value();
                    let iv = if iv.get_type().get_bit_width() < 64 {
                        self.builder
                            .build_int_z_extend(iv, i64t, "z64")
                            .map_err(|e| e.to_string())?
                    } else {
                        iv
                    };
                    Ok(Some(Some((iv.into(), Qi类型::整数))))
                }
            }
            None => Ok(Some(None)),
        }
    }

    /// `通道<T>(cap)` → 句柄指针（Qi类型::通道(元素)，收发两端据元素类型装/还原）。
    /// cap 缺省 0（无缓冲）。
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
        let 元素 = super::类型::元素类型::从标量(self.符号.解析类型(&c.element_type));
        // R3：协程模式用协程原生通道（单线程执行器内、非阻塞、i64 直传）。
        let ffi = if self.协程开() {
            "qi_coro_chan_new"
        } else {
            "qi_runtime_create_channel"
        };
        let f = match self.module.get_function(ffi) {
            Some(f) => f,
            None => {
                let ptrt = self.ctx.ptr_type(AddressSpace::default());
                self.module.add_function(
                    ffi,
                    ptrt.fn_type(&[self.ctx.i64_type().into()], false),
                    None,
                )
            }
        };
        let cs = self
            .builder
            .build_call(f, &[cap.into()], "chan")
            .map_err(|e| e.to_string())?;
        let ptr = cs
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "create_channel 未返回".to_string())?;
        Ok((ptr, Qi类型::通道(元素)))
    }

    /// `ch <- v` → qi_runtime_channel_send(ch_ptr, v_i64)。返回状态整数。
    /// 通道按值传输 i64：浮点 bitcast、字符串/结构体指针 ptr→int、布尔 zext ——
    /// 接收端按通道元素类型还原（生成通道接收）。
    pub(super) fn 生成通道发送(
        &mut self,
        s: &ChannelSendExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let ch = self.求通道指针(&s.channel)?;
        let (v, t) = self
            .生成表达式(&s.value)?
            .ok_or_else(|| "通道发送值无值".to_string())?;
        // ARC：发送即转移 —— BORROWED RC 值（字符串/结构体/数组）retain 后发；
        // OWNED 直接发。接收端把值当 OWNED 接收（表达式拥有RC 认 通道接收），
        // 净额平衡：发 +1（或转移），收端槽位/丢弃时 -1。
        if self.弧开() && v.is_pointer_value() {
            self.弧存入槽2(v, &s.value, t);
        }
        // 值 → i64 位模式（f64 bitcast / ptr ptrtoint / i1 zext）
        let i64t = self.ctx.i64_type();
        let v64: BasicValueEnum = if v.is_float_value() {
            self.builder
                .build_bit_cast(v.into_float_value(), i64t, "f2bits")
                .map_err(|e| e.to_string())?
        } else if v.is_pointer_value() {
            self.builder
                .build_ptr_to_int(v.into_pointer_value(), i64t, "p2i")
                .map_err(|e| e.to_string())?
                .into()
        } else {
            let iv = v.into_int_value();
            if iv.get_type().get_bit_width() < 64 {
                self.builder
                    .build_int_z_extend(iv, i64t, "zext")
                    .map_err(|e| e.to_string())?
                    .into()
            } else {
                v
            }
        };
        // R3：协程模式走协程原生 try_send（无界恒成功；软上限满返 1）。签名同为 (ptr,i64)->i32。
        let f = if self.协程开() {
            let ptrt = self.ctx.ptr_type(AddressSpace::default());
            match self.module.get_function("qi_coro_chan_try_send") {
                Some(f) => f,
                None => self.module.add_function(
                    "qi_coro_chan_try_send",
                    self.ctx
                        .i32_type()
                        .fn_type(&[ptrt.into(), i64t.into()], false),
                    None,
                ),
            }
        } else {
            self.module
                .get_function("qi_runtime_channel_send")
                .ok_or_else(|| "运行时函数未声明: qi_runtime_channel_send".to_string())?
        };
        let cs = self
            .builder
            .build_call(f, &[ch.into(), v64.into()], "chsend")
            .map_err(|e| e.to_string())?;
        let r = cs
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "channel_send 未返回".to_string())?;

        // R5 park-wake 背压：协程体内、有界通道满（try_send 返 1）→ 把当前协程连同值
        // park 进 send_waiters，挂起；接收方腾空位时补值并唤醒本协程（视作已发）。
        if self.协程开() && self.协程当前.is_some() {
            let ptrt = self.ctx.ptr_type(AddressSpace::default());
            let func = self
                .builder
                .get_insert_block()
                .and_then(|b| b.get_parent())
                .ok_or_else(|| "通道发送：无当前函数".to_string())?;
            let 满 = self
                .builder
                .build_int_compare(
                    IntPredicate::NE,
                    r.into_int_value(),
                    self.ctx.i32_type().const_zero(),
                    "chsend.full",
                )
                .map_err(|e| e.to_string())?;
            let park_bb = self.ctx.append_basic_block(func, "chsend.park");
            let done_bb = self.ctx.append_basic_block(func, "chsend.done");
            self.builder
                .build_conditional_branch(满, park_bb, done_bb)
                .map_err(|e| e.to_string())?;
            self.builder.position_at_end(park_bb);
            let park = self.取或声明函数(
                "qi_coro_chan_park_send",
                self.ctx
                    .void_type()
                    .fn_type(&[ptrt.into(), i64t.into()], false),
            );
            self.builder
                .build_call(park, &[ch.into(), v64.into()], "")
                .map_err(|e| e.to_string())?;
            self.生成挂起点(false)?; // resume 后值已被接收方补进缓冲 → 视作已发送
            self.builder
                .build_unconditional_branch(done_bb)
                .map_err(|e| e.to_string())?;
            self.builder.position_at_end(done_bb);
            return Ok((self.ctx.i64_type().const_zero().into(), Qi类型::整数));
        }

        // i32 状态 → 扩到 i64
        let r64 = self
            .builder
            .build_int_z_extend(r.into_int_value(), self.ctx.i64_type(), "st")
            .map_err(|e| e.to_string())?;
        Ok((r64.into(), Qi类型::整数))
    }

    /// `<- ch` → runtime ABI：`receive(ch, &slot)` 把 slot 填成**指向 boxed i64 的指针**
    /// （send 侧 `Box::into_raw(Box::new(value))`）。故需两步 load：
    ///   1) slot(ptr) 里 load 出 boxed 指针 `p`；
    ///   2) 从 `p` load 出真正的 i64 值。
    /// 只 load 一层会得到 boxed 指针本身（垃圾整数）——这是历史 bug 的根因。
    pub(super) fn 生成通道接收(
        &mut self,
        r: &ChannelReceiveExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        // 通道表达式先生成，其 Qi 类型携带元素类型（通道<字符串> 等按位模式还原）
        let (chv, cht) = self
            .生成表达式(&r.channel)?
            .ok_or_else(|| "通道表达式无值".to_string())?;
        let ch = self.通道值转指针(chv)?;
        let i64t = self.ctx.i64_type();
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());

        // ── R3：协程模式走协程原生通道 —— 值按 i64 单层直取（不 box）。空通道时：
        //    · 协程体内：让出+挂起（协作式，不占线程），resume 再试 → 唤醒语义；
        //    · 同步顶层：单步驱动执行器（qi_coro_step_once）让发送方协程产出后再试。
        let v: inkwell::values::IntValue<'ctx> = if self.协程开() {
            let func = self
                .builder
                .get_insert_block()
                .and_then(|b| b.get_parent())
                .ok_or_else(|| "通道接收：无当前函数".to_string())?;
            let slot = self.入口块alloca(i64t.into(), "corecvslot")?;
            let try_recv = self.取或声明函数(
                "qi_coro_chan_try_recv",
                self.ctx
                    .i32_type()
                    .fn_type(&[ptrt.into(), ptrt.into()], false),
            );
            let 在协程体内 = self.协程当前.is_some();

            let check_bb = self.ctx.append_basic_block(func, "corecv.check");
            let wait_bb = self.ctx.append_basic_block(func, "corecv.wait");
            let got_bb = self.ctx.append_basic_block(func, "corecv.got");
            self.builder
                .build_unconditional_branch(check_bb)
                .map_err(|e| e.to_string())?;

            // check：try_recv；0=取到→got，1=空→wait
            self.builder.position_at_end(check_bb);
            let rc = self
                .builder
                .build_call(try_recv, &[ch.into(), slot.into()], "corecv.try")
                .map_err(|e| e.to_string())?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| "qi_coro_chan_try_recv 无返回".to_string())?
                .into_int_value();
            let 取到 = self
                .builder
                .build_int_compare(
                    IntPredicate::EQ,
                    rc,
                    self.ctx.i32_type().const_zero(),
                    "corecv.ok",
                )
                .map_err(|e| e.to_string())?;
            self.builder
                .build_conditional_branch(取到, got_bb, wait_bb)
                .map_err(|e| e.to_string())?;

            // wait：协程体内→让出+挂起；顶层→驱动执行器一步。都回 check 重试。
            self.builder.position_at_end(wait_bb);
            if 在协程体内 {
                // R5 park-wake：把当前协程 park 进通道 recv_waiters（不空转），发送方
                // 到来时唤醒。挂起 → 控制权回 executor；resume 后回 check 重试 try_recv。
                let park = self.取或声明函数(
                    "qi_coro_chan_park_recv",
                    self.ctx.void_type().fn_type(&[ptrt.into()], false),
                );
                self.builder
                    .build_call(park, &[ch.into()], "")
                    .map_err(|e| e.to_string())?;
                self.生成挂起点(false)?;
                self.builder
                    .build_unconditional_branch(check_bb)
                    .map_err(|e| e.to_string())?;
            } else {
                let step = self.取或声明函数("qi_coro_step_once", i64t.fn_type(&[], false));
                self.builder
                    .build_call(step, &[], "")
                    .map_err(|e| e.to_string())?;
                self.builder
                    .build_unconditional_branch(check_bb)
                    .map_err(|e| e.to_string())?;
            }

            // got：单层 load i64
            self.builder.position_at_end(got_bb);
            self.builder
                .build_load(i64t, slot, "corecv.val")
                .map_err(|e| e.to_string())?
                .into_int_value()
        } else {
            // slot 存 *mut i64（boxed 值的指针）。entry 块 alloca：循环里反复
            // 接收不吃栈；FFI 每次调用前后完整写/读该槽，复用安全。
            let slot = self.入口块alloca(ptrt.into(), "recvslot")?;
            let f = self
                .module
                .get_function("qi_runtime_channel_receive")
                .ok_or_else(|| "运行时函数未声明: qi_runtime_channel_receive".to_string())?;
            self.builder
                .build_call(f, &[ch.into(), slot.into()], "chrecv")
                .map_err(|e| e.to_string())?;
            // 1) load boxed 指针
            let boxed = self
                .builder
                .build_load(ptrt, slot, "recvbox")
                .map_err(|e| e.to_string())?
                .into_pointer_value();
            // 2) deref 得真正 i64 值
            self.builder
                .build_load(i64t, boxed, "recvval")
                .map_err(|e| e.to_string())?
                .into_int_value()
        };
        // 3) 按通道元素类型还原（send 侧按位模式装入 i64）
        use super::类型::元素类型;
        let Some(元素) = cht.通道元素() else {
            // 旧式整数句柄通道：保持 i64 语义
            return Ok((v.into(), Qi类型::整数));
        };
        let (rv, rt): (BasicValueEnum<'ctx>, Qi类型) = match 元素 {
            元素类型::整数 => (v.into(), Qi类型::整数),
            元素类型::浮点数 => (
                self.builder
                    .build_bit_cast(v, self.ctx.f64_type(), "bits2f")
                    .map_err(|e| e.to_string())?,
                Qi类型::浮点数,
            ),
            元素类型::布尔 => (
                self.builder
                    .build_int_truncate(v, self.ctx.bool_type(), "i2b")
                    .map_err(|e| e.to_string())?
                    .into(),
                Qi类型::布尔,
            ),
            元素类型::指针 => (
                self.builder
                    .build_int_to_ptr(v, ptrt, "i2p")
                    .map_err(|e| e.to_string())?
                    .into(),
                Qi类型::字符串,
            ),
            元素类型::结构体(i) => (
                self.builder
                    .build_int_to_ptr(v, ptrt, "i2p")
                    .map_err(|e| e.to_string())?
                    .into(),
                Qi类型::结构体(i),
            ),
        };
        Ok((rv, rt))
    }

    /// `启动 表达式` —— 真并发：把表达式包成 nullary 闭包（fat obj），
    /// 经共享 trampoline 丢到 runtime 线程池执行。fire-and-forget，返回空。
    ///
    /// 步骤：
    /// 1. 把 `表达式` 当作 nullary 闭包体（表达式语句，求值即弃）→ fat obj 指针 `g`
    ///    （复用 闭包.rs：自由变量扫描 + 捕获 + 待合成）。
    /// 2. 取共享 trampoline `qi_go_tramp(args: *const i64)`：读 args[0] → inttoptr
    ///    成 fat obj → 从 fat[0] 取闭包 fn_ptr → 调 fn(fat)（无用户参）。
    /// 3. 栈上放 `[1 x i64] = [ ptrtoint(g) ]`，调
    ///    `qi_runtime_spawn_goroutine_with_args(qi_go_tramp, 数组指针, 1)`。
    pub(super) fn 生成协程启动(
        &mut self,
        g: &GoroutineSpawnExpression,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        // R3 统一：`启动 <协程函数调用>` → 入同一 M:N 协程执行器（而非 tokio 池）。
        // 调用点代码本身就会 ramp+qi_coro_spawn（把协程排进 PENDING），这里生成表达式
        // 并丢弃返回的 coro future 即完成「fire-and-forget 调度到执行器」。父协程之后
        // 让出（等待/通道挂起）时执行器轮转即会驱动它。非协程调用仍走下面的 tokio 路径。
        if let AstNode::函数调用表达式(call) = &*g.expression {
            if self.调用是协程(call) {
                self.生成表达式(&g.expression)?;
                return Ok(None);
            }
        }

        let i64t = self.ctx.i64_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        // 1. 表达式 → nullary 闭包体（一条表达式语句：求值并丢弃）
        let body = vec![AstNode::表达式语句(ExpressionStatement {
            expression: g.expression.clone(),
            span: g.span.clone(),
        })];
        let 闭包obj = self.合成nullary闭包(body)?;

        // 2. 共享 trampoline
        let tramp = self.取协程trampoline()?;

        // 3. 栈上 i64 = ptrtoint(闭包obj)；其指针即 1 元素 args 数组（*const i64）。
        //    runtime 侧会把这 1 个 i64 拷贝走，故用栈 alloca 安全（spawn 时读取即拷贝）。
        //    entry 块 alloca：循环里反复 启动 不吃栈；spawn 调用即拷贝，复用安全。
        let 数组 = self.入口块alloca(i64t.into(), "go_args")?;
        let obj_i = self
            .builder
            .build_ptr_to_int(闭包obj, i64t, "obj2i")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(数组, obj_i)
            .map_err(|e| e.to_string())?;

        // 4. spawn（fire-and-forget）
        let spawn = self
            .module
            .get_function("qi_runtime_spawn_goroutine_with_args")
            .ok_or_else(|| "运行时函数未声明: qi_runtime_spawn_goroutine_with_args".to_string())?;
        let one = i64t.const_int(1, false);
        self.builder
            .build_call(spawn, &[tramp.into(), 数组.into(), one.into()], "go_spawn")
            .map_err(|e| e.to_string())?;
        Ok(None)
    }

    /// 共享协程 trampoline `fn qi_go_tramp(args: *const i64)`：
    /// 读 args[0] → inttoptr 成 fat obj → fat[0] 取闭包 fn_ptr → 调 fn(fat)。幂等。
    fn 取协程trampoline(&mut self) -> Result<PointerValue<'ctx>, String> {
        const 名: &str = "qi_go_tramp";
        if let Some(f) = self.module.get_function(名) {
            return Ok(f.as_global_value().as_pointer_value());
        }
        let i64t = self.ctx.i64_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let fn_type = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        let tramp = self.module.add_function(名, fn_type, None);

        let 保存 = self.builder.get_insert_block();
        let entry = self.ctx.append_basic_block(tramp, "entry");
        self.builder.position_at_end(entry);

        // args: *const i64（形参0）
        let args_ptr = tramp.get_nth_param(0).unwrap().into_pointer_value();
        // 读 args[0]
        let obj_i = self
            .builder
            .build_load(i64t, args_ptr, "go_obj_i")
            .map_err(|e| e.to_string())?
            .into_int_value();
        // inttoptr → fat obj
        let obj = self
            .builder
            .build_int_to_ptr(obj_i, ptrt, "go_obj")
            .map_err(|e| e.to_string())?;
        // fat[0] 取闭包 fn_ptr（与 发射fat调用 一致）
        let getfn = self
            .module
            .get_function("qi_closure_get_fn")
            .ok_or_else(|| "运行时函数未声明: qi_closure_get_fn".to_string())?;
        let fptr = self
            .builder
            .build_call(getfn, &[obj.into()], "go_getfn")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "get_fn 未返回".to_string())?
            .into_pointer_value();
        // 调 fn(env=fat)（nullary 闭包：仅 env 参，void 返回）
        let 闭包fn类型 = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        self.builder
            .build_indirect_call(闭包fn类型, fptr, &[obj.into()], "go_call")
            .map_err(|e| e.to_string())?;
        // ARC（E1）：spawn 是发送即转移 —— 闭包 obj 的 +1（创建时 rc=1）随
        // spawn 移交 goroutine；体跑完在此释放（归零调 env dtor 级联释放捕获）。
        // 跨线程 refcount 是 AtomicI64，安全。QI_ARC=0 不释放（与旧行为一致）。
        if self.弧开() {
            if let Some(rel) = self.module.get_function("qi_closure_release") {
                self.builder
                    .build_call(rel, &[obj.into()], "")
                    .map_err(|e| e.to_string())?;
            }
        }
        self.builder.build_return(None).map_err(|e| e.to_string())?;

        if let Some(bb) = 保存 {
            self.builder.position_at_end(bb);
        }
        Ok(tramp.as_global_value().as_pointer_value())
    }

    /// `选择 { 情况 … : …  默认: …  情况 超时(ms): … }` —— 通道多路选择。
    ///
    /// 最小可用降级：非阻塞轮询各通道 case
    /// （`qi_runtime_channel_try_receive` / `qi_runtime_channel_try_send`）：
    ///   - 有 `默认` 分支：单轮轮询，全部未就绪立刻走 默认（Go 语义）。
    ///   - 有 `超时(ms)` 分支：轮询 + `qi_runtime_select_backoff`（~1ms）退避，
    ///     到截止时间走 超时 体。
    ///   - 两者皆无：轮询 + 退避直到某 case 就绪（阻塞语义）。
    /// 通道表达式 / 发送值 / 超时毫秒只在进入 选择 时求值一次，轮询不重复副作用。
    /// 接收 case 的 `变量 := <-ch` 把收到的 i64 绑进 case 体（与 `<-ch` 表达式
    /// 一致的两层 load：slot 里是 boxed 指针，再 deref 出值）。
    pub(super) fn 生成选择(
        &mut self,
        s: &SelectExpression,
        func: FunctionValue<'ctx>,
    ) -> Result<(), String> {
        let i32t = self.ctx.i32_type();
        let i64t = self.ctx.i64_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        // FFI 原型（幂等声明）
        let try_recv = self.取或声明函数(
            "qi_runtime_channel_try_receive",
            i32t.fn_type(&[ptrt.into(), ptrt.into()], false),
        );
        let try_send = self.取或声明函数(
            "qi_runtime_channel_try_send",
            i32t.fn_type(&[ptrt.into(), i64t.into()], false),
        );
        let backoff = self.取或声明函数(
            "qi_runtime_select_backoff",
            self.ctx.void_type().fn_type(&[], false),
        );

        // 预求值：各 case 的通道指针 / 发送值（避免每轮轮询重复副作用）。
        // (是否发送, 通道 ptr, 发送值 i64)
        let mut 准备: Vec<(
            bool,
            PointerValue<'ctx>,
            Option<inkwell::values::IntValue<'ctx>>,
        )> = Vec::new();
        for case in &s.cases {
            match &case.kind {
                SelectCaseKind::通道接收 { channel, .. } => {
                    let ch = self.求通道指针(channel)?;
                    准备.push((false, ch, None));
                }
                SelectCaseKind::通道发送 { channel, value } => {
                    let ch = self.求通道指针(channel)?;
                    let (v, _t) = self
                        .生成表达式(value)?
                        .ok_or_else(|| "选择 发送值无值".to_string())?;
                    let iv = if v.is_pointer_value() {
                        self.builder
                            .build_ptr_to_int(v.into_pointer_value(), i64t, "sel.p2i")
                            .map_err(|e| e.to_string())?
                    } else {
                        v.into_int_value()
                    };
                    准备.push((true, ch, Some(iv)));
                }
                // 语法层已把 默认/超时 分离进 default_case / timeout_case
                _ => return Err("选择：默认/超时 分支位置异常".to_string()),
            }
        }

        // 超时：截止时刻 = 进入时刻 + 毫秒（只求一次）
        let 超时 = match &s.timeout_case {
            Some(tc) => {
                let SelectCaseKind::超时 { 毫秒 } = &tc.kind else {
                    return Err("选择：超时分支结构异常".to_string());
                };
                let get_ms =
                    self.取或声明函数("qi_runtime_get_time_ms", i64t.fn_type(&[], false));
                let (msv, _) = self
                    .生成表达式(毫秒)?
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

        // 接收 slot（entry 块 alloca，轮询循环里复用不吃栈）
        let slot = self.入口块alloca(ptrt.into(), "sel.slot")?;

        let poll_bb = self.ctx.append_basic_block(func, "select.poll");
        let end_bb = self.ctx.append_basic_block(func, "select.end");
        self.builder
            .build_unconditional_branch(poll_bb)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(poll_bb);

        // 轮询链：逐 case 非阻塞试收/试发，就绪即跳该 case 体
        let mut 体块 = Vec::new();
        for (i, (是发送, ch, val)) in 准备.iter().enumerate() {
            let body_bb = self
                .ctx
                .append_basic_block(func, &format!("select.body{}", i));
            let next_bb = self
                .ctx
                .append_basic_block(func, &format!("select.try{}", i));
            let rc = if *是发送 {
                self.builder
                    .build_call(
                        try_send,
                        &[(*ch).into(), val.unwrap().into()],
                        "sel.trysend",
                    )
                    .map_err(|e| e.to_string())?
            } else {
                self.builder
                    .build_call(try_recv, &[(*ch).into(), slot.into()], "sel.tryrecv")
                    .map_err(|e| e.to_string())?
            }
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
            体块.push(body_bb);
        }

        // 全部未就绪
        if let Some(dc) = &s.default_case {
            // 默认：单轮判定，立刻走 默认 体（不退避不循环）
            self.生成块(&dc.body, func)?;
            self.跳转若未终结(end_bb)?;
        } else if let Some((get_ms, deadline, tc)) = &超时 {
            let now = self
                .builder
                .build_call(*get_ms, &[], "sel.now")
                .map_err(|e| e.to_string())?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| "get_time_ms 未返回".to_string())?
                .into_int_value();
            let 到期 = self
                .builder
                .build_int_compare(IntPredicate::SGE, now, *deadline, "sel.expired")
                .map_err(|e| e.to_string())?;
            let to_bb = self.ctx.append_basic_block(func, "select.timeout");
            let wait_bb = self.ctx.append_basic_block(func, "select.wait");
            self.builder
                .build_conditional_branch(到期, to_bb, wait_bb)
                .map_err(|e| e.to_string())?;
            self.builder.position_at_end(wait_bb);
            self.builder
                .build_call(backoff, &[], "")
                .map_err(|e| e.to_string())?;
            self.builder
                .build_unconditional_branch(poll_bb)
                .map_err(|e| e.to_string())?;
            self.builder.position_at_end(to_bb);
            self.生成块(&tc.body, func)?;
            self.跳转若未终结(end_bb)?;
        } else {
            // 无 默认/超时：退避后再轮询（阻塞语义）
            self.builder
                .build_call(backoff, &[], "")
                .map_err(|e| e.to_string())?;
            self.builder
                .build_unconditional_branch(poll_bb)
                .map_err(|e| e.to_string())?;
        }

        // 各 case 体
        for (case, body_bb) in s.cases.iter().zip(体块) {
            self.builder.position_at_end(body_bb);
            if let SelectCaseKind::通道接收 {
                variable: Some(name),
                ..
            } = &case.kind
            {
                // slot → boxed 指针 → i64 值（与 生成通道接收 一致的两层 load）
                let boxed = self
                    .builder
                    .build_load(ptrt, slot, "sel.box")
                    .map_err(|e| e.to_string())?
                    .into_pointer_value();
                let v = self
                    .builder
                    .build_load(i64t, boxed, "sel.val")
                    .map_err(|e| e.to_string())?;
                let var = self.入口块alloca(i64t.into(), name)?;
                self.builder
                    .build_store(var, v)
                    .map_err(|e| e.to_string())?;
                self.变量表.insert(name.clone(), (var, Qi类型::整数));
                self.符号.声明变量(name, Qi类型::整数);
            }
            self.生成块(&case.body, func)?;
            self.跳转若未终结(end_bb)?;
        }

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    /// 幂等取/声明模块级函数原型。
    fn 取或声明函数(
        &mut self,
        name: &str,
        ft: inkwell::types::FunctionType<'ctx>,
    ) -> FunctionValue<'ctx> {
        match self.module.get_function(name) {
            Some(f) => f,
            None => self.module.add_function(name, ft, None),
        }
    }

    /// 求通道句柄的 ptr 值（通道值是 ptr；旧式整数句柄转回 ptr 传给运行时）。
    fn 求通道指针(
        &mut self,
        node: &AstNode,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let (v, _t) = self
            .生成表达式(node)?
            .ok_or_else(|| "通道表达式无值".to_string())?;
        self.通道值转指针(v)
    }

    /// 通道值 → ptr（ptr 原样；i64 句柄 int_to_ptr）。
    fn 通道值转指针(
        &mut self,
        v: BasicValueEnum<'ctx>,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        if v.is_pointer_value() {
            Ok(v.into_pointer_value())
        } else {
            let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
            self.builder
                .build_int_to_ptr(v.into_int_value(), ptrt, "ch2ptr")
                .map_err(|e| e.to_string())
        }
    }
}
