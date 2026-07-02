//! 异常处理 —— `尝试 / 捕获 / 最终 / 抛出` 降级（setjmp/longjmp ABI）。
//!
//! runtime ABI（qi-runtime stdlib/exception_ffi.rs）：
//!   qi_exc_alloc_frame() -> ptr    分配 jmp_buf 并 push 到 thread-local 栈
//!   qi_exc_pop()                   弹栈（正常退出 / 进入 catch 时都要弹）
//!   qi_exc_throw(ptr msg) -> !     记录消息 + longjmp 到栈顶 frame
//!   qi_exc_message() -> ptr        取最近一次异常消息（rc 字符串）
//!   qi_exc_clear()                 清空消息
//!
//! setjmp 必须由**生成代码直接调用**（declare i32 @setjmp(ptr) returns_twice）：
//! 若经 Rust 薄 wrapper（qi_exc_setjmp）调用，wrapper 返回后其栈帧死亡，
//! longjmp 回跳时 wrapper 的尾声从被覆写的栈里重载 fp/lr → 段错误。
//! 直接调用则 setjmp 捕获的是当前 Qi 函数的活帧，longjmp 合法回跳。
//!
//! 布局：
//!   entry:  buf = alloc_frame(); rc = setjmp(buf); br rc==0, try, catch
//!   try:    <try body>; pop(); br finally
//!   catch:  pop(); 错误 = message(); <catch body>; clear(); br finally
//!   finally:<finally body>; （后续语句继续在此块）
//!
//! 注意：catch 先 pop 再跑 body —— catch 体内再 `抛出` 会传播到外层 frame。
//! 已知限制：try 体内 `返回` 不会弹栈（frame 泄漏 + 后续 throw 行为未定义），
//! 与老后端一致，暂不处理。

use super::后端;
use crate::parser::ast::{ThrowStatement, TryStatement};
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

impl<'ctx> 后端<'ctx> {
    /// 幂等声明一个异常运行时函数。
    fn 取异常运行时(&mut self, name: &str) -> Result<FunctionValue<'ctx>, String> {
        if let Some(f) = self.module.get_function(name) {
            return Ok(f);
        }
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let voidt = self.ctx.void_type();
        let ft = match name {
            "qi_exc_alloc_frame" => ptrt.fn_type(&[], false),
            "qi_exc_pop" | "qi_exc_clear" => voidt.fn_type(&[], false),
            "qi_exc_throw" => voidt.fn_type(&[ptrt.into()], false),
            "qi_exc_message" => ptrt.fn_type(&[], false),
            _ => return Err(format!("未知异常运行时函数: {}", name)),
        };
        Ok(self.module.add_function(name, ft, None))
    }

    /// 幂等声明 libc `setjmp`（带 returns_twice —— LLVM 必须知道它会返回两次，
    /// 否则跨 setjmp 的寄存器缓存在 longjmp 回跳路径上是垃圾）。
    fn 取setjmp(&mut self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("setjmp") {
            return f;
        }
        let i32t = self.ctx.i32_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let f = self
            .module
            .add_function("setjmp", i32t.fn_type(&[ptrt.into()], false), None);
        let kind = inkwell::attributes::Attribute::get_named_enum_kind_id("returns_twice");
        f.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            self.ctx.create_enum_attribute(kind, 0),
        );
        f
    }

    /// `抛出 表达式;` → qi_exc_throw(msg)。非字符串值先转成字符串。
    pub(super) fn 生成抛出(&mut self, t: &ThrowStatement) -> Result<(), String> {
        let (v, vt) = self
            .生成表达式(&t.expression)?
            .ok_or_else(|| "抛出表达式无值".to_string())?;
        // 消息指针：字符串直接用；整数/浮点转成字符串
        let msg = if v.is_pointer_value() {
            v.into_pointer_value()
        } else if vt.是浮点() {
            let f = match self.module.get_function("qi_runtime_float_to_string") {
                Some(f) => f,
                None => self.module.add_function(
                    "qi_runtime_float_to_string",
                    self.ctx
                        .ptr_type(AddressSpace::default())
                        .fn_type(&[self.ctx.f64_type().into()], false),
                    None,
                ),
            };
            self.builder
                .build_call(f, &[v.into()], "f2s")
                .map_err(|e| e.to_string())?
                .try_as_basic_value()
                .left()
                .ok_or_else(|| "float_to_string 未返回".to_string())?
                .into_pointer_value()
        } else {
            let mut iv = v.into_int_value();
            if iv.get_type().get_bit_width() < 64 {
                iv = self
                    .builder
                    .build_int_z_extend(iv, self.ctx.i64_type(), "z64")
                    .map_err(|e| e.to_string())?;
            }
            let f = match self.module.get_function("qi_runtime_int_to_string") {
                Some(f) => f,
                None => self.module.add_function(
                    "qi_runtime_int_to_string",
                    self.ctx
                        .ptr_type(AddressSpace::default())
                        .fn_type(&[self.ctx.i64_type().into()], false),
                    None,
                ),
            };
            self.builder
                .build_call(f, &[iv.into()], "i2s")
                .map_err(|e| e.to_string())?
                .try_as_basic_value()
                .left()
                .ok_or_else(|| "int_to_string 未返回".to_string())?
                .into_pointer_value()
        };

        let throw_fn = self.取异常运行时("qi_exc_throw")?;
        self.builder
            .build_call(throw_fn, &[msg.into()], "")
            .map_err(|e| e.to_string())?;
        // qi_exc_throw 不返回（longjmp / panic）—— 终结当前块
        self.builder
            .build_unreachable()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// `尝试 { } 捕获 错误 { } 最终 { }` 降级。
    pub(super) fn 生成尝试(
        &mut self,
        t: &TryStatement,
        func: FunctionValue<'ctx>,
    ) -> Result<(), String> {
        let alloc_fn = self.取异常运行时("qi_exc_alloc_frame")?;
        let setjmp_fn = self.取setjmp();
        let pop_fn = self.取异常运行时("qi_exc_pop")?;

        // buf = alloc_frame(); rc = setjmp(buf)
        let buf = self
            .builder
            .build_call(alloc_fn, &[], "exc.buf")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "alloc_frame 未返回".to_string())?;
        let rc = self
            .builder
            .build_call(setjmp_fn, &[buf.into()], "exc.rc")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| "setjmp 未返回".to_string())?
            .into_int_value();
        let 正常 = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                rc,
                rc.get_type().const_zero(),
                "exc.ok",
            )
            .map_err(|e| e.to_string())?;

        let try_bb = self.ctx.append_basic_block(func, "try.body");
        let catch_bb = self.ctx.append_basic_block(func, "try.catch");
        let cont_bb = self.ctx.append_basic_block(func, "try.cont");

        self.builder
            .build_conditional_branch(正常, try_bb, catch_bb)
            .map_err(|e| e.to_string())?;

        // ── try 体：正常走完弹 frame ────────────────────────────────
        self.builder.position_at_end(try_bb);
        self.生成块(&t.try_body, func)?;
        if !self.当前块已终结() {
            self.builder
                .build_call(pop_fn, &[], "")
                .map_err(|e| e.to_string())?;
            self.builder
                .build_unconditional_branch(cont_bb)
                .map_err(|e| e.to_string())?;
        }

        // ── catch：先弹 frame（体内再抛传播到外层），绑定错误变量 ──
        self.builder.position_at_end(catch_bb);
        self.builder
            .build_call(pop_fn, &[], "")
            .map_err(|e| e.to_string())?;
        if let Some(clause) = t.catch_clauses.first() {
            self.符号.进入作用域();
            if let Some(var) = &clause.error_var {
                let msg_fn = self.取异常运行时("qi_exc_message")?;
                let msg = self
                    .builder
                    .build_call(msg_fn, &[], "exc.msg")
                    .map_err(|e| e.to_string())?
                    .try_as_basic_value()
                    .left()
                    .ok_or_else(|| "exc_message 未返回".to_string())?;
                // entry 块 alloca:尝试/捕获 嵌在循环里时不随迭代吃栈。
                // ARC：错误变量是字符串局部槽，走 entry null 初始化 —— 函数从
                // 未进 catch 也会在出口统一释放该槽，未初始化读到垃圾会崩。
                let slot = if self.弧开() {
                    self.弧字符串槽(func, var)?
                } else {
                    self.入口块alloca(self.ctx.ptr_type(AddressSpace::default()).into(), var)?
                };
                self.builder
                    .build_store(slot, msg)
                    .map_err(|e| e.to_string())?;
                self.变量表
                    .insert(var.clone(), (slot, super::类型::Qi类型::字符串));
                self.符号.声明变量(var, super::类型::Qi类型::字符串);
            }
            for s in &clause.body {
                self.生成语句(s, func)?;
                if self.当前块已终结() {
                    break;
                }
            }
            self.符号.退出作用域();
            if !self.当前块已终结() {
                let clear_fn = self.取异常运行时("qi_exc_clear")?;
                self.builder
                    .build_call(clear_fn, &[], "")
                    .map_err(|e| e.to_string())?;
            }
        }
        if !self.当前块已终结() {
            self.builder
                .build_unconditional_branch(cont_bb)
                .map_err(|e| e.to_string())?;
        }

        // ── 最终（有则生成），后续语句继续在 cont 块 ────────────────
        self.builder.position_at_end(cont_bb);
        if let Some(fin) = &t.finally_body {
            self.生成块(fin, func)?;
        }
        Ok(())
    }
}
