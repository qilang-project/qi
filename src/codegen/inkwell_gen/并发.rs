//! 通道 + 协程启动（阶段10 退化实现，够让 harness/工具.qi 编译并功能正确）。
//!
//! - `通道<整数>(cap)` → `qi_runtime_create_channel(cap)`，返回 ptr（当整数句柄）。
//! - `ch <- v`         → `qi_runtime_channel_send(ch, v_i64)`。
//! - `<- ch`           → 存 slot，`qi_runtime_channel_receive(ch, &slot)`，load slot(i64)。
//! - `启动 表达式`    → **真并发**：把表达式包成 nullary 闭包（fat obj），经共享
//!   trampoline 丢到 `qi_runtime_spawn_goroutine_with_args` 的 tokio 线程池执行。
//!   fire-and-forget，同步靠通道 / 等待组。见 生成协程启动。

use super::类型::Qi类型;
use super::后端;
use crate::parser::ast::{
    AstNode, ChannelCreateExpression, ChannelReceiveExpression, ChannelSendExpression,
    ExpressionStatement, GoroutineSpawnExpression,
};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, PointerValue};
use inkwell::AddressSpace;

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
            "互斥锁加锁" | "互斥锁锁定" | "加锁" => ("qi_runtime_mutex_lock", &['h'], false),
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
        match cs.try_as_basic_value().left() {
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
        let (v, t) = self
            .生成表达式(&s.value)?
            .ok_or_else(|| "通道发送值无值".to_string())?;
        // ARC：发送即转移 —— BORROWED 字符串 retain 后发；OWNED 直接发。
        // （接收端类型退化为整数句柄，永不 release —— 宁泄漏。）
        if self.弧开() && t == super::类型::Qi类型::字符串 && v.is_pointer_value() {
            self.弧存入槽(v, &s.value);
        }
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

    /// `<- ch` → runtime ABI：`receive(ch, &slot)` 把 slot 填成**指向 boxed i64 的指针**
    /// （send 侧 `Box::into_raw(Box::new(value))`）。故需两步 load：
    ///   1) slot(ptr) 里 load 出 boxed 指针 `p`；
    ///   2) 从 `p` load 出真正的 i64 值。
    /// 只 load 一层会得到 boxed 指针本身（垃圾整数）——这是历史 bug 的根因。
    pub(super) fn 生成通道接收(
        &mut self,
        r: &ChannelReceiveExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let ch = self.求通道指针(&r.channel)?;
        let i64t = self.ctx.i64_type();
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
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
        let v = self
            .builder
            .build_load(i64t, boxed, "recvval")
            .map_err(|e| e.to_string())?;
        Ok((v, Qi类型::整数))
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
            .build_call(
                spawn,
                &[tramp.into(), 数组.into(), one.into()],
                "go_spawn",
            )
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
            .left()
            .ok_or_else(|| "get_fn 未返回".to_string())?
            .into_pointer_value();
        // 调 fn(env=fat)（nullary 闭包：仅 env 参，void 返回）
        let 闭包fn类型 = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        self.builder
            .build_indirect_call(闭包fn类型, fptr, &[obj.into()], "go_call")
            .map_err(|e| e.to_string())?;
        self.builder.build_return(None).map_err(|e| e.to_string())?;

        if let Some(bb) = 保存 {
            self.builder.position_at_end(bb);
        }
        Ok(tramp.as_global_value().as_pointer_value())
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
