//! 协程 —— 真协程变换（Plan A：LLVM 21 `llvm.coro.*` stackless 状态机）。
//! Round 1：标量协程 + 最小 executor；Round 2：ARC 跨挂起点（RC 局部/参数/返回值）。
//!
//! ## 门控
//! `QI_CORO=1` 时启用。**不设时默认路径逐字节不变**（走 异步.rs 的 eager future
//! 老路，所有存量测试/示例零影响）。编译器在 `后端::new` 读一次 env 存 `后端.协程`。
//!
//! ## 变换范围
//! `QI_CORO=1` 下，「返回类型是 `未来<T>` **且**函数体内含 `等待`」的用户函数走 coro
//! 变换；不含 `等待` 的返回 `未来<T>` 函数保持 eager（语义等价、省事）。合成函数
//! （询问家族）本轮不动。判定见 `收集协程函数` / `含等待`。
//!
//! ## 变换形状（switched-resume ABI）
//! 用户函数
//! ```text
//! 函数 甲() : 未来<整数> { 打印行("A1"); 等待 让出(); 打印行("A2"); 返回 42; }
//! ```
//! 编译成一个带 `presplitcoroutine` 属性的 ramp 函数（LLVM CoroSplit 会自动把它
//! 拆成 `甲` / `甲.resume` / `甲.destroy` 三份，并把跨挂起的标量 alloca 搬进 frame）：
//! ```text
//! entry:
//!   %promise = alloca i64                       ; 存返回值（标量位模式）
//!   %id  = llvm.coro.id(8, %promise, null, null)
//!   %sz  = llvm.coro.size.i64()
//!   %mem = malloc(%sz)                          ; frame 堆分配（不做 elision）
//!   %hdl = llvm.coro.begin(%id, %mem)
//!   … 打印行("A1") …
//!   ; 等待 让出():
//!   call qi_coro_yield_ready()                  ; 告知 executor：下次立即就绪
//!   %s = llvm.coro.save(null)
//!   %c = llvm.coro.suspend(%s, false)
//!   switch %c, susp [0->resume1, 1->cleanup]    ; 挂起点：控制权回 executor
//! resume1:
//!   … 打印行("A2") …
//!   ; 返回 42:
//!   store i64 42, %promise
//!   %fs = llvm.coro.save(null)
//!   %fc = llvm.coro.suspend(%fs, true)          ; final suspend → done()=true
//!   switch %fc, susp [0->unreachable, 1->cleanup]
//! cleanup:
//!   %m = llvm.coro.free(%id, %hdl); free(%m); br susp
//! susp:
//!   llvm.coro.end(%hdl, false, none); ret ptr %hdl   ; 初始挂起返回**裸 handle**
//! ```
//! ramp 返回**裸 coroutine handle**；`qi_coro_spawn(handle)` 在**调用点**把它包成
//! coro future（`QiCoro*`，见 qi-runtime/async_runtime/coro.rs）——不在 `susp` 块里
//! 包，因为 CoroSplit 会把 `susp` 克隆进 resume/destroy，在那里包会重复 spawn。
//!
//! ## frame 布局
//! 完全交给 LLVM CoroSplit：入口块 alloca（含 %promise、跨挂起的局部槽）自动
//! 搬进堆 frame。整数/浮点直接跨挂起；返回值经 %promise（i64 位模式）交回。
//!
//! ## R2：ARC 跨挂起点（RC 局部/参数/返回值）
//! RC 释放从普通函数的 per-exit `弧释放局部` 模型改挂到 **coroutine cleanup 路径**
//! （见 填协程cleanup）：cleanup 块是 destroy 克隆的唯一路径 —— 正常完成（final
//! suspend 后 executor destroy）与提前 destroy（`取消未来`，停在中途挂起点）都走它，
//! frame 内每个 RC 槽在 coro.free 之前恰好释放一次。三条纪律：
//! 1. **槽**：RC 参数落地 retain +1、RC 局部按既有声明/赋值纪律持 +1；每个 RC 槽的
//!    alloca 后紧跟 `store null`（destroy 早于声明执行时 cleanup 读 null 安全跳过）；
//!    cleanup 逐槽 load + release（null 安全）。cleanup 引用槽也迫使 CoroSplit 把
//!    它们搬进 frame。
//! 2. **返回值**：`返回 rc值` 与普通返回同纪律（BORROWED retain / OWNED 转移），
//!    promise 持 +1；executor 在 destroy **之前**把 promise 读进 coro future 值槽，
//!    `等待` 时 take（+1 移交调用方，二次 take 得 null，杜绝双释放）。
//! 3. **等待 RC payload**：qi_future_await_ptr/string 按 magic 分发到
//!    qi_coro_take_ptr（take 语义）；`等待 未来<结构体>` 的延迟解码分支
//!    （qi_rc_is_string）对协程结构体指针天然走直通路，不受影响。
//!
//! ## executor 协议（与 qi-runtime/async_runtime/coro.rs 对接）
//! - runtime 无法直接发 `llvm.coro.*` intrinsic，故 codegen emit 四个薄封装
//!   `__qi_coro_resume/done/destroy/promise_i64`，`入口` 序言 `qi_coro_register_ops`
//!   注册其地址。
//! - `等待 让出()`   → `qi_coro_yield_ready()` + 普通挂起（下次立即就绪）。
//! - `等待 异步睡眠(ms)` → `qi_coro_sleep(ms)` + 普通挂起（睡到 now+ms 恢复）。
//! - `执行器运行全部()` → `qi_coro_run_all()`（轮转驱动所有排队 coroutine 到完成）。
//! - 同步上下文 `等待 <coro future>` → 老 `qi_future_await_T` 里按 magic 分发到
//!   `qi_coro_await_i64`（未完成先驱动 executor，再取 %promise 值）。
//!
//! ## 剩余限制（R2 后的能力边界，如实文档化）
//! - **已支持**：标量 + RC 对象（字符串/结构体/数组/闭包/装箱枚举**槽**）跨挂起；
//!   协程返回 字符串/结构体（经 promise +1 take）；提前 destroy 释放 frame 内 RC 槽。
//! - **表达式临时值跨挂起不覆盖**：`等待` 出现在复合表达式中间时（如
//!   `f(拼接串, 等待 让出())`），挂起前算出的 OWNED 临时不在任何槽里 ——
//!   提前 destroy 会漏。写法上把临时先落变量即可。
//! - **取消后勿再 等待 该未来**：`取消未来` 后值槽为 0/null，await 得空值。
//! - **同名遮蔽**：循环内重声明同名 RC 变量产生的旧 alloca 不再被追踪（与
//!   变量表 flat 语义一致），旧槽值可能泄漏（宁泄漏不双释放）。
//! - 协程体内 `尝试/抛出`（setjmp 跨 coroutine frame）未定义；QI_PROF 不注入。
//! - `未来<数组>` 返回值未验证（内部元素类型经 指针 退化为 字符串 语义，与 eager 同）。
//! - Round 3：调度器 / 真 IO / 大模型 pending future 集成；协程内 await 另一协程
//!   的真嵌套挂起（现走 eager 阻塞驱动，可能相互等待）。

use super::后端;
use super::类型::{Qi类型, 元素类型};
use crate::parser::ast::{AstNode, FunctionDeclaration, Program};
use inkwell::attributes::AttributeLoc;
use inkwell::basic_block::BasicBlock;
use inkwell::context::AsContextRef;
use inkwell::llvm_sys::core::{
    LLVMAddFunction, LLVMBuildCall2, LLVMConstNull, LLVMFunctionType, LLVMGetNamedFunction,
    LLVMTokenTypeInContext,
};
use inkwell::llvm_sys::prelude::{LLVMTypeRef, LLVMValueRef};
use inkwell::types::AsTypeRef;
use inkwell::values::{AsValueRef, BasicValueEnum, IntValue, PointerValue};
use inkwell::AddressSpace;
use std::ffi::CString;

/// 一个正在生成的 coroutine 的上下文（frame handle / promise / 公共块）。
/// 仅在 `生成协程函数体` 期间为 `Some`；`等待 让出/睡眠` 与 `返回` 据此发挂起点。
pub(super) struct 协程上下文<'ctx> {
    /// `llvm.coro.id` 返回的 token（裸 ref；供 coro.free）。
    id_token: LLVMValueRef,
    /// `llvm.coro.begin` 返回的 frame handle（裸 ref；ptr）。
    hdl: LLVMValueRef,
    /// promise alloca（i64；存标量返回值位模式）。
    promise: PointerValue<'ctx>,
    /// 公共 cleanup 块（coro.free + free + br suspend）。
    cleanup_bb: BasicBlock<'ctx>,
    /// 公共 suspend 块（coro.end + ret 裸 handle）。
    suspend_bb: BasicBlock<'ctx>,
    /// 挂起点计数（生成唯一 resume 块名）。
    计数: u32,
}

impl<'ctx> 后端<'ctx> {
    // ───────────────────────── 协程函数集合收集 ─────────────────────────

    /// 扫描全部顶层用户函数，把「返回 `未来<T>` 且含 `等待`」的填进 `协程函数集`
    /// （mangled 名）。QI_CORO 关时直接返回（集合恒空）。须在 登记函数 之后、
    /// 生成任何函数体之前调（调用点据此包装 spawn）。
    pub(super) fn 收集协程函数(&mut self, programs: &[Program]) {
        if !self.协程开() {
            return;
        }
        for p in programs {
            self.设当前包(p.package_name.clone());
            for stmt in &p.statements {
                if let AstNode::函数声明(f) = stmt {
                    if self.函数应协程化(f) {
                        let 元数 = f.parameters.len();
                        let mangled =
                            super::包内符号名(p.package_name.as_deref(), &f.name, 元数);
                        self.协程函数集.insert(mangled);
                    }
                }
            }
        }
    }

    /// 判定一个函数是否应走 coro 变换：返回 `未来<T>`（据已登记签名）且体含 `等待`。
    fn 函数应协程化(&self, f: &FunctionDeclaration) -> bool {
        if !f.type_params.is_empty() {
            return false; // 泛型模板不直接协程化（按实例化处理，Round 1 不涉及）
        }
        let 元数 = f.parameters.len();
        let 是未来 = self
            .符号
            .解析重载(&f.name, 元数)
            .map(|s| s.返回.未来内部().is_some())
            .unwrap_or(false);
        是未来 && 含等待(&f.body)
    }

    /// 调用点判定：`f`（LLVM 函数）是否 coroutine（其返回 handle 需包成 coro future）。
    pub(super) fn 是协程函数(&self, f: inkwell::values::FunctionValue<'ctx>) -> bool {
        if !self.协程开() {
            return false;
        }
        f.get_name()
            .to_str()
            .map(|n| self.协程函数集.contains(n))
            .unwrap_or(false)
    }

    // ───────────────────────── 协程函数体生成 ─────────────────────────

    /// 生成一个 coroutine ramp 函数体（switched-resume）。仅 QI_CORO=1 且
    /// `是协程函数` 时由 `生成函数体` 分派进来。
    pub(super) fn 生成协程函数体(&mut self, f: &FunctionDeclaration) -> Result<(), String> {
        let 元数 = f.parameters.len();
        let mangled = super::包内符号名(self.当前包.as_deref(), &f.name, 元数);
        let func = self
            .module
            .get_function(&mangled)
            .ok_or_else(|| format!("协程函数原型缺失: {}", f.name))?;
        let sig = self
            .符号
            .解析重载(&f.name, 元数)
            .cloned()
            .ok_or_else(|| format!("协程函数签名缺失: {}", f.name))?;

        self.变量表.clear();
        self.符号.进入作用域();
        self.当前返回类型 = sig.返回;
        self.try深度 = 0;

        // presplitcoroutine 属性 —— 触发默认 PM 管线的 CoroEarly/CoroSplit/CoroCleanup。
        let kind = inkwell::attributes::Attribute::get_named_enum_kind_id("presplitcoroutine");
        let attr = self.ctx.create_enum_attribute(kind, 0);
        func.add_attribute(AttributeLoc::Function, attr);

        let entry = self.ctx.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        // promise alloca（i64）—— 存标量返回值位模式；先清零（无显式返回时默认 0）。
        let i64t = self.ctx.i64_type();
        let promise = self
            .builder
            .build_alloca(i64t, "coro.promise")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(promise, i64t.const_zero())
            .map_err(|e| e.to_string())?;

        // coro 序言：id / size / malloc / begin
        let (hdl, id_token) = unsafe { self.协程序言(promise)? };

        // 公共块：cleanup + suspend（先建，供各挂起点/返回分支）。
        let cleanup_bb = self.ctx.append_basic_block(func, "coro.cleanup");
        let suspend_bb = self.ctx.append_basic_block(func, "coro.susp");

        // 填 suspend 块：coro.end + ret 裸 handle。
        self.builder.position_at_end(suspend_bb);
        unsafe { self.协程end(hdl)? };
        let hdl_ptr = unsafe { PointerValue::new(hdl) };
        self.builder
            .build_return(Some(&hdl_ptr))
            .map_err(|e| e.to_string())?;

        // cleanup 块**延后填充**（体生成完才知道全部 RC 槽）——见 填协程cleanup。
        // 它是正常完成（final suspend 后 destroy）与提前 destroy 共用的唯一销毁路径。

        self.协程当前 = Some(协程上下文 {
            id_token,
            hdl,
            promise,
            cleanup_bb,
            suspend_bb,
            计数: 0,
        });

        // 回到 entry：形参落地 + 生成体（挂起点/返回由 生成语句 分派回本模块）。
        self.builder.position_at_end(entry);
        for (i, p) in f.parameters.iter().enumerate() {
            let t = sig.参数.get(i).copied().unwrap_or(Qi类型::整数);
            let llvmt = self
                .llvm基础类型(t)
                .ok_or_else(|| format!("协程参数 {} 类型无效", p.name))?;
            let ptr = self
                .builder
                .build_alloca(llvmt, &p.name)
                .map_err(|e| e.to_string())?;
            let arg = func
                .get_nth_param(i as u32)
                .ok_or_else(|| format!("协程缺少第 {} 个形参", i))?;
            self.builder
                .build_store(ptr, arg)
                .map_err(|e| e.to_string())?;
            // R2：RC 参数与普通函数同纪律 —— 落地进槽 retain 一次；
            // 与 cleanup 路径「释放所有 RC 槽」平衡（正常完成与提前 destroy 共用）。
            self.弧retain任意(arg, t);
            self.变量表.insert(p.name.clone(), (ptr, t));
            self.符号.声明变量(&p.name, t);
        }

        for stmt in &f.body {
            self.生成语句(stmt, func)?;
            if self.当前块已终结() {
                break;
            }
        }

        // 体落底无显式返回：补默认协程返回（存 0 + final suspend）。
        if !self.当前块已终结() {
            self.生成协程返回(None)?;
        }

        // R2：体生成完毕，RC 槽全部已知 —— 现在填 cleanup 块（释放所有 RC 槽 +
        // coro.free）并给每个 RC 槽在 alloca 后补 null 初始化。
        self.填协程cleanup(cleanup_bb, suspend_bb, id_token, hdl)?;

        self.协程当前 = None;
        self.符号.退出作用域();
        Ok(())
    }

    /// R2 核心：填 coroutine 的公共 cleanup 块 + RC 槽 null 初始化。
    ///
    /// cleanup 是 destroy 克隆的唯一路径：**正常完成**（final suspend 后 executor
    /// destroy）与**提前 destroy**（`取消未来`，停在中途挂起点）都走它 —— RC 释放
    /// 挂在这里恰好各执行一次，替代普通函数的 per-exit `弧释放局部` 模型。
    ///
    /// 形状：
    /// ```text
    /// coro.cleanup:
    ///   %v = load ptr, %rc槽_i      ; 逐个 RC 槽（frame 内）
    ///   call release(%v)            ; null 安全（qi_string_free 判 header /
    ///                               ;  qi.release.sN 带 is_null 检查）
    ///   …
    ///   %mem = llvm.coro.free(%id, %hdl)
    ///   call free(%mem)
    ///   br coro.susp
    /// ```
    /// null 初始化：每个 RC 槽的 alloca 之后立刻插 `store null` —— 提前 destroy
    /// 发生在声明语句执行之前时，cleanup 读到 null 安全跳过（不释放垃圾指针）。
    /// null store 紧跟 alloca，先于任何真实 store，不覆盖已赋的值。
    /// cleanup 引用这些槽也迫使 CoroSplit 把它们搬进 frame（跨挂起存活）。
    fn 填协程cleanup(
        &mut self,
        cleanup_bb: BasicBlock<'ctx>,
        suspend_bb: BasicBlock<'ctx>,
        id_token: LLVMValueRef,
        hdl: LLVMValueRef,
    ) -> Result<(), String> {
        let 保存 = self.builder.get_insert_block();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        // RC 槽收集（同 弧释放局部 的过滤：RC 类型且非弱局部）。QI_ARC=0 时不收集
        // （与普通函数「关 ARC 不释放」一致）。
        let 槽们: Vec<(PointerValue<'ctx>, Qi类型)> = if self.弧开() {
            self.变量表
                .iter()
                .filter(|(名, (_, t))| super::所有权::是RC类型(*t) && !self.弱局部.contains(*名))
                .map(|(_, (p, t))| (*p, *t))
                .collect()
        } else {
            Vec::new()
        };

        // 每个 RC 槽：alloca 后紧跟 store null（见函数注释）。
        for (p, _) in &槽们 {
            if let Some(inst) = p.as_instruction() {
                match inst.get_next_instruction() {
                    Some(next) => self.builder.position_before(&next),
                    None => {
                        let bb = inst
                            .get_parent()
                            .ok_or_else(|| "RC 槽 alloca 无所在块".to_string())?;
                        self.builder.position_at_end(bb);
                    }
                }
                self.builder
                    .build_store(*p, ptrt.const_null())
                    .map_err(|e| e.to_string())?;
            }
        }

        // 填 cleanup：释放所有 RC 槽 → coro.free → free → br suspend。
        self.builder.position_at_end(cleanup_bb);
        for (p, t) in 槽们 {
            let v = self
                .builder
                .build_load(ptrt, p, "coro.rc")
                .map_err(|e| e.to_string())?;
            self.弧release任意(v, t);
        }
        unsafe { self.协程free(id_token, hdl)? };
        self.builder
            .build_unconditional_branch(suspend_bb)
            .map_err(|e| e.to_string())?;

        if let Some(b) = 保存 {
            self.builder.position_at_end(b);
        }
        Ok(())
    }

    // ───────────────────────── 挂起点 / 返回 ─────────────────────────

    /// `等待 让出()` / `等待 异步睡眠(ms)` 在协程体内的真挂起点。返回 `Some` 表示
    /// 已处理（是挂起原语）；`None` 表示不是（交回 生成等待 的 eager 路径）。
    pub(super) fn 尝试协程挂起(
        &mut self,
        expr: &AstNode,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let AstNode::函数调用表达式(call) = expr else {
            return Ok(None);
        };
        if call.module_qualifier.is_some() {
            return Ok(None);
        }
        match call.callee.as_str() {
            "让出" | "yield" => {
                self.发协程原语("qi_coro_yield_ready", &[])?;
                self.生成挂起点(false)?;
                // 等待 让出() 无有意义返回值 —— 占位空值（调用处一般作表达式语句丢弃）。
                Ok(Some((
                    self.ctx.i64_type().const_zero().into(),
                    Qi类型::整数,
                )))
            }
            "异步睡眠" | "睡眠" | "sleep" => {
                let ms = if call.arguments.is_empty() {
                    self.ctx.i64_type().const_zero().into()
                } else {
                    let (v, _t) = self
                        .生成表达式(&call.arguments[0])?
                        .ok_or_else(|| "异步睡眠 实参无值".to_string())?;
                    v
                };
                self.发协程原语("qi_coro_sleep", &[ms])?;
                self.生成挂起点(false)?;
                Ok(Some((
                    self.ctx.i64_type().const_zero().into(),
                    Qi类型::整数,
                )))
            }
            _ => Ok(None),
        }
    }

    /// 协程体内的 `返回 [expr]`：把值编码进 promise，再发 final suspend。
    /// 由 语句.rs 的 返回语句 分派进来（当 `协程当前.is_some()`）。
    ///
    /// R2 RC 返回值：与普通 返回 同纪律 —— promise 持有 +1（BORROWED 先 retain，
    /// OWNED 直接转移）。final suspend 后 executor 把 promise 读进 coro future 的
    /// 值槽；cleanup 释放的是各 RC **槽**（局部/参数），promise 的 +1 不受影响，
    /// 随 `等待` 的 take 语义移交调用方。
    pub(super) fn 生成协程返回(&mut self, expr: Option<&AstNode>) -> Result<(), String> {
        if let Some(e) = expr {
            let 内部 = self.当前返回类型.未来内部().unwrap_or(元素类型::整数);
            let 期望 = 内部.标量();
            let (v, vt) = self
                .生成带期望(e, Some(期望))?
                .ok_or_else(|| "协程返回表达式无值".to_string())?;
            // RC 值 → promise 收 +1（镜像 语句.rs 普通返回的 该retain 判定）
            let 该retain = if 期望 == Qi类型::字符串 {
                vt == Qi类型::字符串 && !self.表达式拥有字符串(e)
            } else {
                super::所有权::是RC类型(期望) && !self.表达式拥有RC(e, 期望)
            };
            if self.弧开() && 该retain && v.is_pointer_value() {
                self.弧retain任意(v, 期望);
            }
            let i64v = self.标量编码i64(v, vt)?;
            let promise = self.协程当前.as_ref().unwrap().promise;
            self.builder
                .build_store(promise, i64v)
                .map_err(|e| e.to_string())?;
        }
        self.生成挂起点(true)?;
        Ok(())
    }

    /// 发一个挂起点：coro.save + coro.suspend + switch。
    /// `is_final=false`：普通挂起，`0→resume（续接体）, 1→cleanup, default→suspend`。
    /// `is_final=true` ：final suspend，`0→unreachable, 1→cleanup, default→suspend`。
    fn 生成挂起点(&mut self, is_final: bool) -> Result<(), String> {
        let (cleanup_bb, suspend_bb) = {
            let c = self
                .协程当前
                .as_ref()
                .ok_or_else(|| "挂起点：不在协程体内".to_string())?;
            (c.cleanup_bb, c.suspend_bb)
        };
        let 序号 = {
            let c = self.协程当前.as_mut().unwrap();
            c.计数 += 1;
            c.计数
        };
        let func = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or_else(|| "挂起点：无当前函数".to_string())?;

        let susp_i8 = unsafe { self.协程suspend(is_final)? };
        let i8t = self.ctx.i8_type();

        // 0-case 目标块：普通挂起 → 续接体块；final suspend → trap（final 后被 resume 非法）。
        let case0 = self.ctx.append_basic_block(
            func,
            &if is_final {
                format!("coro.final_trap{}", 序号)
            } else {
                format!("coro.resume{}", 序号)
            },
        );

        self.builder
            .build_switch(
                susp_i8,
                suspend_bb,
                &[
                    (i8t.const_int(0, false), case0),
                    (i8t.const_int(1, false), cleanup_bb),
                ],
            )
            .map_err(|e| e.to_string())?;

        // 定位到 0-case 块续接：普通挂起是续接体；final 则填 unreachable（体已尽）。
        self.builder.position_at_end(case0);
        if is_final {
            self.builder
                .build_unreachable()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // ───────────────────────── 运行时封装 / 注册 ─────────────────────────

    /// emit 四个 coro intrinsic 薄封装（供 runtime executor 注册后回调）。幂等。
    /// `__qi_coro_resume/done/destroy` 与 `__qi_coro_promise_i64`。
    pub(super) fn 生成协程运行时(&mut self) -> Result<(), String> {
        if self.module.get_function("__qi_coro_resume").is_some() {
            return Ok(());
        }
        let voidt = self.ctx.void_type();
        let i1t = self.ctx.bool_type();
        let i32t = self.ctx.i32_type();
        let i64t = self.ctx.i64_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        // 非 token intrinsic：直接 inkwell 声明。
        let resume_i = self.取或声明("llvm.coro.resume", voidt.fn_type(&[ptrt.into()], false));
        let done_i = self.取或声明("llvm.coro.done", i1t.fn_type(&[ptrt.into()], false));
        let destroy_i =
            self.取或声明("llvm.coro.destroy", voidt.fn_type(&[ptrt.into()], false));
        let promise_i = self.取或声明(
            "llvm.coro.promise",
            ptrt.fn_type(&[ptrt.into(), i32t.into(), i1t.into()], false),
        );

        let 保存 = self.builder.get_insert_block();

        // __qi_coro_resume(ptr) { call llvm.coro.resume(ptr); ret }
        {
            let w = self.module.add_function(
                "__qi_coro_resume",
                voidt.fn_type(&[ptrt.into()], false),
                None,
            );
            let bb = self.ctx.append_basic_block(w, "e");
            self.builder.position_at_end(bb);
            let h = w.get_nth_param(0).unwrap();
            self.builder
                .build_call(resume_i, &[h.into()], "")
                .map_err(|e| e.to_string())?;
            self.builder.build_return(None).map_err(|e| e.to_string())?;
        }
        // __qi_coro_done(ptr) -> i1
        {
            let w = self.module.add_function(
                "__qi_coro_done",
                i1t.fn_type(&[ptrt.into()], false),
                None,
            );
            let bb = self.ctx.append_basic_block(w, "e");
            self.builder.position_at_end(bb);
            let h = w.get_nth_param(0).unwrap();
            let d = self
                .builder
                .build_call(done_i, &[h.into()], "d")
                .map_err(|e| e.to_string())?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| "coro.done 无返回".to_string())?;
            self.builder
                .build_return(Some(&d))
                .map_err(|e| e.to_string())?;
        }
        // __qi_coro_destroy(ptr)
        {
            let w = self.module.add_function(
                "__qi_coro_destroy",
                voidt.fn_type(&[ptrt.into()], false),
                None,
            );
            let bb = self.ctx.append_basic_block(w, "e");
            self.builder.position_at_end(bb);
            let h = w.get_nth_param(0).unwrap();
            self.builder
                .build_call(destroy_i, &[h.into()], "")
                .map_err(|e| e.to_string())?;
            self.builder.build_return(None).map_err(|e| e.to_string())?;
        }
        // __qi_coro_promise_i64(ptr) -> i64 { p=coro.promise(h,8,false); load i64,p }
        {
            let w = self.module.add_function(
                "__qi_coro_promise_i64",
                i64t.fn_type(&[ptrt.into()], false),
                None,
            );
            let bb = self.ctx.append_basic_block(w, "e");
            self.builder.position_at_end(bb);
            let h = w.get_nth_param(0).unwrap();
            let p = self
                .builder
                .build_call(
                    promise_i,
                    &[
                        h.into(),
                        i32t.const_int(8, false).into(),
                        i1t.const_zero().into(),
                    ],
                    "p",
                )
                .map_err(|e| e.to_string())?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| "coro.promise 无返回".to_string())?
                .into_pointer_value();
            let v = self
                .builder
                .build_load(i64t, p, "pv")
                .map_err(|e| e.to_string())?;
            self.builder
                .build_return(Some(&v))
                .map_err(|e| e.to_string())?;
        }

        if let Some(b) = 保存 {
            self.builder.position_at_end(b);
        }
        Ok(())
    }

    /// 在当前插入点 emit `qi_coro_register_ops(resume, done, destroy, promise)`。
    /// 由 `生成入口` 在 main 序言调（QI_CORO 且有协程函数时）。前置 `生成协程运行时`。
    pub(super) fn 注册协程ops(&mut self) -> Result<(), String> {
        self.生成协程运行时()?;
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let reg = self.取或声明(
            "qi_coro_register_ops",
            self.ctx
                .void_type()
                .fn_type(&[ptrt.into(), ptrt.into(), ptrt.into(), ptrt.into()], false),
        );
        let fp = |b: &后端<'ctx>, name: &str| {
            b.module
                .get_function(name)
                .unwrap()
                .as_global_value()
                .as_pointer_value()
        };
        let r = fp(self, "__qi_coro_resume");
        let d = fp(self, "__qi_coro_done");
        let de = fp(self, "__qi_coro_destroy");
        let p = fp(self, "__qi_coro_promise_i64");
        self.builder
            .build_call(reg, &[r.into(), d.into(), de.into(), p.into()], "")
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 调用点：把 coroutine ramp 返回的裸 handle 包成 coro future（`qi_coro_spawn`）。
    pub(super) fn 协程包装spawn(
        &mut self,
        hdl: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let spawn = self.取或声明("qi_coro_spawn", ptrt.fn_type(&[ptrt.into()], false));
        let h = if hdl.is_pointer_value() {
            hdl
        } else {
            self.builder
                .build_int_to_ptr(hdl.into_int_value(), ptrt, "h2p")
                .map_err(|e| e.to_string())?
                .into()
        };
        let r = self
            .builder
            .build_call(spawn, &[h.into()], "coro.fut")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "qi_coro_spawn 无返回".to_string())?;
        Ok(r)
    }

    /// `执行器运行全部()` 等协程内建（无模块限定）。返回 `Some(...)` 表示已处理。
    /// QI_CORO 关时返回 `None`（回退普通函数解析 → 未定义函数）。
    pub(super) fn 生成协程内建(
        &mut self,
        callee: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        if !self.协程开() {
            return Ok(None);
        }
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        match callee {
            "执行器运行全部" | "运行执行器" | "协程运行全部" => {
                let run =
                    self.取或声明("qi_coro_run_all", self.ctx.void_type().fn_type(&[], false));
                self.builder
                    .build_call(run, &[], "")
                    .map_err(|e| e.to_string())?;
                Ok(Some(None)) // void
            }
            // 单步驱动：resume 一个就绪协程一次。返回 1=还有排队协程 / 0=队列已空。
            "执行器单步" | "协程单步" => {
                let step = self.取或声明(
                    "qi_coro_step_once",
                    self.ctx.i64_type().fn_type(&[], false),
                );
                let r = self
                    .builder
                    .build_call(step, &[], "coro.step")
                    .map_err(|e| e.to_string())?
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "qi_coro_step_once 无返回".to_string())?;
                Ok(Some(Some((r, Qi类型::整数))))
            }
            // 提前销毁：destroy 未完成的协程（走 cleanup 路径释放 frame 内 RC 槽）。
            // 已完成/已取消的 no-op。取消后勿再 等待 该未来（值槽为 0/null）。
            "取消未来" | "取消协程" => {
                if arguments.is_empty() {
                    return Err("取消未来 需要 1 个实参（协程未来）".to_string());
                }
                let (v, _t) = self
                    .生成表达式(&arguments[0])?
                    .ok_or_else(|| "取消未来 实参无值".to_string())?;
                let p = if v.is_pointer_value() {
                    v
                } else {
                    self.builder
                        .build_int_to_ptr(v.into_int_value(), ptrt, "f2p")
                        .map_err(|e| e.to_string())?
                        .into()
                };
                let cancel = self.取或声明(
                    "qi_coro_cancel",
                    self.ctx.void_type().fn_type(&[ptrt.into()], false),
                );
                self.builder
                    .build_call(cancel, &[p.into()], "")
                    .map_err(|e| e.to_string())?;
                Ok(Some(None)) // void
            }
            _ => Ok(None),
        }
    }

    // ───────────────────────── 底层辅助 ─────────────────────────

    /// 标量值 → i64 位模式（float bitcast / bool·小整数 zext / ptr ptrtoint / i64 原样）。
    fn 标量编码i64(
        &mut self,
        v: BasicValueEnum<'ctx>,
        _vt: Qi类型,
    ) -> Result<IntValue<'ctx>, String> {
        let i64t = self.ctx.i64_type();
        if v.is_float_value() {
            return self
                .builder
                .build_bit_cast(v.into_float_value(), i64t, "f2i")
                .map(|x| x.into_int_value())
                .map_err(|e| e.to_string());
        }
        if v.is_pointer_value() {
            return self
                .builder
                .build_ptr_to_int(v.into_pointer_value(), i64t, "p2i")
                .map_err(|e| e.to_string());
        }
        let iv = v.into_int_value();
        if iv.get_type().get_bit_width() < 64 {
            self.builder
                .build_int_z_extend(iv, i64t, "z64")
                .map_err(|e| e.to_string())
        } else {
            Ok(iv)
        }
    }

    /// 幂等取/声明模块级函数原型（inkwell）。
    fn 取或声明(
        &self,
        name: &str,
        ft: inkwell::types::FunctionType<'ctx>,
    ) -> inkwell::values::FunctionValue<'ctx> {
        match self.module.get_function(name) {
            Some(f) => f,
            None => self.module.add_function(name, ft, None),
        }
    }

    /// 发一个无返回值的 runtime 原语调用（如 qi_coro_yield_ready / qi_coro_sleep(ms)）。
    fn 发协程原语(&mut self, name: &str, args: &[BasicValueEnum<'ctx>]) -> Result<(), String> {
        let i64t = self.ctx.i64_type();
        let 参数类型: Vec<inkwell::types::BasicMetadataTypeEnum> =
            args.iter().map(|_| i64t.into()).collect();
        let f = self.取或声明(name, self.ctx.void_type().fn_type(&参数类型, false));
        let 实参: Vec<inkwell::values::BasicMetadataValueEnum> = args
            .iter()
            .map(|v| {
                // 睡眠毫秒按 i64 传；若非整数（不应发生）就原样。
                if v.is_int_value() {
                    let iv = v.into_int_value();
                    if iv.get_type().get_bit_width() < 64 {
                        self.builder
                            .build_int_s_extend(iv, i64t, "ms64")
                            .map(|x| x.into())
                            .unwrap_or((*v).into())
                    } else {
                        (*v).into()
                    }
                } else {
                    (*v).into()
                }
            })
            .collect();
        self.builder
            .build_call(f, &实参, "")
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── 以下用原始 llvm_sys API 发涉及 `token` 类型的 coro intrinsic ──
    // inkwell 0.8 对 token 类型会 panic（types/enums.rs），故 token 相关调用走裸 LLVM-C。

    /// coro 序言：id / size / malloc / begin。返回 (frame handle ref, id token ref)。
    unsafe fn 协程序言(
        &mut self,
        promise: PointerValue<'ctx>,
    ) -> Result<(LLVMValueRef, LLVMValueRef), String> {
        let ctxref = self.ctx.as_ctx_ref();
        let token_ty = LLVMTokenTypeInContext(ctxref);
        let i32t = self.ctx.i32_type().as_type_ref();
        let i64t_ref = self.ctx.i64_type().as_type_ref();
        let ptrt_ref = self.ctx.ptr_type(AddressSpace::default()).as_type_ref();

        // llvm.coro.id : token (i32 align, ptr promise, ptr null, ptr null)
        let (id_f, id_ty) = self.声明token原型(
            "llvm.coro.id",
            token_ty,
            &mut [i32t, ptrt_ref, ptrt_ref, ptrt_ref],
        );
        let align = self.ctx.i32_type().const_int(8, false).as_value_ref();
        let promise_ref = promise.as_value_ref();
        let nullp = self
            .ctx
            .ptr_type(AddressSpace::default())
            .const_null()
            .as_value_ref();
        let id_token = self.原始调用(
            id_f,
            id_ty,
            &mut [align, promise_ref, nullp, nullp],
            "coro.id",
        );

        // llvm.coro.size.i64 : i64 ()
        let size_f = self.取或声明(
            "llvm.coro.size.i64",
            self.ctx.i64_type().fn_type(&[], false),
        );
        let size = self
            .builder
            .build_call(size_f, &[], "coro.size")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "coro.size 无返回".to_string())?;

        // malloc(size) -> ptr
        let malloc = self.取或声明(
            "malloc",
            self.ctx
                .ptr_type(AddressSpace::default())
                .fn_type(&[self.ctx.i64_type().into()], false),
        );
        let mem = self
            .builder
            .build_call(malloc, &[size.into()], "coro.mem")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "malloc 无返回".to_string())?;

        // llvm.coro.begin : ptr (token, ptr)
        let (begin_f, begin_ty) =
            self.声明token原型("llvm.coro.begin", ptrt_ref, &mut [token_ty, ptrt_ref]);
        let hdl = self.原始调用(
            begin_f,
            begin_ty,
            &mut [id_token, mem.as_value_ref()],
            "coro.hdl",
        );
        let _ = i64t_ref;
        Ok((hdl, id_token))
    }

    /// coro.save + coro.suspend。返回 i8 挂起结果（IntValue）。
    unsafe fn 协程suspend(&mut self, is_final: bool) -> Result<IntValue<'ctx>, String> {
        let ctxref = self.ctx.as_ctx_ref();
        let token_ty = LLVMTokenTypeInContext(ctxref);
        let i8t = self.ctx.i8_type().as_type_ref();
        let i1t = self.ctx.bool_type().as_type_ref();
        let ptrt_ref = self.ctx.ptr_type(AddressSpace::default()).as_type_ref();

        let (save_f, save_ty) = self.声明token原型("llvm.coro.save", token_ty, &mut [ptrt_ref]);
        let nullp = self
            .ctx
            .ptr_type(AddressSpace::default())
            .const_null()
            .as_value_ref();
        let save_tok = self.原始调用(save_f, save_ty, &mut [nullp], "coro.save");

        let (susp_f, susp_ty) =
            self.声明token原型("llvm.coro.suspend", i8t, &mut [token_ty, i1t]);
        let fin = if is_final {
            self.ctx.bool_type().const_int(1, false)
        } else {
            self.ctx.bool_type().const_zero()
        }
        .as_value_ref();
        let c = self.原始调用(susp_f, susp_ty, &mut [save_tok, fin], "coro.suspend");
        Ok(IntValue::new(c))
    }

    /// coro.end(hdl, false, none)。
    unsafe fn 协程end(&mut self, hdl: LLVMValueRef) -> Result<(), String> {
        let ctxref = self.ctx.as_ctx_ref();
        let token_ty = LLVMTokenTypeInContext(ctxref);
        let i1t = self.ctx.bool_type().as_type_ref();
        let ptrt_ref = self.ctx.ptr_type(AddressSpace::default()).as_type_ref();
        let (end_f, end_ty) =
            self.声明token原型("llvm.coro.end", i1t, &mut [ptrt_ref, i1t, token_ty]);
        let false_v = self.ctx.bool_type().const_zero().as_value_ref();
        let none_tok = LLVMConstNull(token_ty);
        let _ = self.原始调用(end_f, end_ty, &mut [hdl, false_v, none_tok], "coro.end");
        Ok(())
    }

    /// coro.free(id, hdl) + free(mem)。
    unsafe fn 协程free(&mut self, id: LLVMValueRef, hdl: LLVMValueRef) -> Result<(), String> {
        let ctxref = self.ctx.as_ctx_ref();
        let token_ty = LLVMTokenTypeInContext(ctxref);
        let ptrt_ref = self.ctx.ptr_type(AddressSpace::default()).as_type_ref();
        let (free_f, free_ty) =
            self.声明token原型("llvm.coro.free", ptrt_ref, &mut [token_ty, ptrt_ref]);
        let mem = self.原始调用(free_f, free_ty, &mut [id, hdl], "coro.free.mem");
        let freef = self.取或声明(
            "free",
            self.ctx
                .void_type()
                .fn_type(&[self.ctx.ptr_type(AddressSpace::default()).into()], false),
        );
        let mem_pv = PointerValue::new(mem);
        self.builder
            .build_call(freef, &[mem_pv.into()], "")
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 幂等声明一个 token 相关 intrinsic 原型（裸 LLVM-C，避开 inkwell token panic）。
    /// 返回 (函数 ref, 函数类型 ref)。
    unsafe fn 声明token原型(
        &self,
        name: &str,
        ret: LLVMTypeRef,
        params: &mut [LLVMTypeRef],
    ) -> (LLVMValueRef, LLVMTypeRef) {
        let m = self.module.as_mut_ptr();
        let cname = CString::new(name).unwrap();
        let fnty = LLVMFunctionType(ret, params.as_mut_ptr(), params.len() as u32, 0);
        let existing = LLVMGetNamedFunction(m, cname.as_ptr());
        if !existing.is_null() {
            return (existing, fnty);
        }
        let f = LLVMAddFunction(m, cname.as_ptr(), fnty);
        (f, fnty)
    }

    /// 裸 LLVMBuildCall2。返回结果 value ref。
    unsafe fn 原始调用(
        &self,
        f: LLVMValueRef,
        fnty: LLVMTypeRef,
        args: &mut [LLVMValueRef],
        name: &str,
    ) -> LLVMValueRef {
        let b = self.builder.as_mut_ptr();
        let cname = CString::new(name).unwrap();
        LLVMBuildCall2(
            b,
            fnty,
            f,
            args.as_mut_ptr(),
            args.len() as u32,
            cname.as_ptr(),
        )
    }
}

/// 递归判定语句序列是否含 `等待`（不下钻闭包边界 —— 闭包体的 await 不使外层协程化）。
pub(super) fn 含等待(body: &[AstNode]) -> bool {
    body.iter().any(节点含等待)
}

fn 节点含等待(n: &AstNode) -> bool {
    if matches!(n, AstNode::等待表达式(_)) {
        return true;
    }
    if matches!(n, AstNode::闭包表达式(_)) {
        return false; // 闭包边界：内部 await 归内部（Round 1 不协程化闭包）
    }
    super::闭包环::子节点(n).iter().any(|c| 节点含等待(c))
}
