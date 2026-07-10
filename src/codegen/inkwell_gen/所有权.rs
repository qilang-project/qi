//! QI_ARC=1 门控的保守字符串 ARC —— 所有权判定 + retain/release 插桩工具。
//!
//! 纪律（宁泄漏，绝不 over-release）：每个字符串表达式值静态标注
//! OWNED（+1，我方持有）或 BORROWED（0）：
//!   - 字面量 → BORROWED（immortal，retain/release 皆 no-op）
//!   - runtime FFI 返回串 → OWNED（rc=1 交出）；**例外**见 借用串FFI 名单
//!   - Qi 用户函数 / 闭包 / 方法调用返回 → OWNED（返回约定 +1，全程序同 flag 自洽）
//!   - 变量 / 全局 / 结构体字段 / 数组元素 load、函数参数 → BORROWED
//!   - 拼接（qi_runtime_string_concat）结果 → OWNED
//!
//! 判定是**纯语法 + 符号表**的（不产生 IR）。它必须精确于「说 OWNED 必真 OWNED」；
//! 说 BORROWED 只可能多泄漏（store 时 retain 一次），永远安全。所有插桩点都
//! 先检查实际生成类型是 字符串 才动作，其余类型零影响。
//!
//! 关键机械保证：ARC 开时所有字符串局部槽都在函数 entry 块 alloca + null 初始化
//! （见 弧字符串槽），因此任意 return 点统一 load+release 全部字符串局部
//! 都满足 LLVM 支配关系，且 release(null) 安全。

use super::后端;
use super::导入::注册表类型转qi;
use super::类型::{Qi类型, 元素类型};
use super::类型检查::推断表达式类型;
use crate::parser::ast::{AstNode, BinaryOperator};
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

/// 该 Qi 类型的值是否受 RC 管理（字符串 / 结构体本体 / 数组本体 / 闭包 fat obj）。
/// 未来 句柄本轮不管（保持泄漏；其 Pointer payload 由 await take / future_free 归还）。
///
/// Round E1：函数值（闭包 fat obj）纳入 RC —— 闭包值本质是 {fn, env} 单指针
/// 对象，retain/release 闭包值 = retain/release env 指针（qi_closure_retain /
/// qi_closure_release，按 magic 动态派发，裸句柄误入时静默 no-op）。
#[allow(non_snake_case)]
pub(super) fn 是RC类型(t: Qi类型) -> bool {
    matches!(
        t,
        Qi类型::字符串
            | Qi类型::结构体(_)
            | Qi类型::数组(_)
            | Qi类型::函数值(_)
            | Qi类型::装箱枚举(_)
    )
}

/// 返回「借用串」的 runtime FFI 名单：这些函数返回的指针是内部借用 / 透传，
/// **不是** rc=1 交出的新串。判为 BORROWED（store 时 retain，绝不按 OWNED release）。
///   - qi_web_request_*  / qi_web_match_params：RequestParts / MatchResult 内部
///     RC 串的借引用（parts 持有一份引用，parts_free 释放那份）
///   - qi_future_value_ptr：future 内部指针透传（非 take，仍借用）
///   - qi_future_await_ptr **不在**名单（Round E3）：await 是 take 语义，
///     Pointer payload +1 移交调用方；String payload 每次新分配 → OWNED
const 借用串FFI: &[&str] = &[
    "qi_web_request_method",
    "qi_web_request_path",
    "qi_web_request_query",
    "qi_web_request_headers",
    "qi_web_request_body",
    "qi_web_match_params",
    "qi_future_value_ptr",
];

/// 「借用读对象」的 runtime FFI：RC 对象（数组/结构体）入参只**借用读**
/// （内部拷贝数据、不私藏指针、不释放）。调用点不做「发送即转移」retain，
/// OWNED 临时实参在调用结束后释放（与 借用串FFI 的字符串语义对称）。
/// 目前：向量模块全家（qi_vector_dot/add/magnitude/normalize/scale/cosine_similarity）。
#[allow(non_snake_case)]
pub(super) fn 借用读对象FFI(runtime_name: &str) -> bool {
    runtime_name.starts_with("qi_vector_")
}

impl<'ctx> 后端<'ctx> {
    /// ARC 是否开启（QI_ARC=1）。
    pub(super) fn 弧开(&self) -> bool {
        self.弧
    }

    /// 拿 retain/free 声明（声明运行时 已注册，这里兜底幂等）。
    fn 弧函数(&mut self, name: &str) -> FunctionValue<'ctx> {
        match self.module.get_function(name) {
            Some(f) => f,
            None => {
                let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
                self.module.add_function(
                    name,
                    self.ctx.void_type().fn_type(&[ptrt.into()], false),
                    None,
                )
            }
        }
    }

    /// 对指针值发 qi_string_retain（ARC 关 / 非指针值 → 不动作）。
    pub(super) fn 弧retain(&mut self, v: BasicValueEnum<'ctx>) {
        if self.弧 && v.is_pointer_value() {
            let f = self.弧函数("qi_string_retain");
            let _ = self
                .builder
                .build_call(f, &[v.into_pointer_value().into()], "");
        }
    }

    /// 对指针值发 qi_string_free（ARC 关 / 非指针值 → 不动作）。
    pub(super) fn 弧release(&mut self, v: BasicValueEnum<'ctx>) {
        if self.弧 && v.is_pointer_value() {
            let f = self.弧函数("qi_string_free");
            let _ = self
                .builder
                .build_call(f, &[v.into_pointer_value().into()], "");
        }
    }

    /// 值 v（由表达式 值node 生成，目标槽类型 字符串）即将存入槽位：
    /// BORROWED → retain（+1 归槽）；OWNED → 直接转移，不动作。
    pub(super) fn 弧存入槽(&mut self, v: BasicValueEnum<'ctx>, 值node: &AstNode) {
        if self.弧 && !self.表达式拥有字符串(值node) {
            self.弧retain(v);
        }
    }

    /// 释放字符串槽位的旧值：load + release。槽必须已初始化
    /// （entry null 初始化 / 全局 zeroinit / 已存过值），release(null) 安全。
    pub(super) fn 弧释放槽旧值(&mut self, slot: PointerValue<'ctx>) -> Result<(), String> {
        if !self.弧 {
            return Ok(());
        }
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let old = self
            .builder
            .build_load(ptrt, slot, "arc.old")
            .map_err(|e| e.to_string())?;
        self.弧release(old);
        Ok(())
    }

    /// 一个已在本语句内被消费完毕（打印实参 / FFI 借用实参 / 比较操作数 / 表达式
    /// 语句丢弃）的字符串值：OWNED → release；BORROWED → 不动作。
    pub(super) fn 弧消费后释放(
        &mut self,
        v: BasicValueEnum<'ctx>,
        t: Qi类型,
        node: &AstNode,
    ) {
        if self.弧 && t == Qi类型::字符串 && self.表达式拥有字符串(node) {
            self.弧release(v);
        }
    }

    // ───────────── RC 对象（结构体本体 / 数组本体）类型感知工具 ─────────────

    /// 类型感知 retain：字符串 → qi_string_retain；结构体/数组 → qi_obj_retain；
    /// 函数值（闭包 fat obj）→ qi_closure_retain（跨 magic 互认；全不符静默/警告
    /// 后泄漏）。其余类型不动作。
    pub(super) fn 弧retain任意(&mut self, v: BasicValueEnum<'ctx>, t: Qi类型) {
        if !self.弧 || !v.is_pointer_value() {
            return;
        }
        match t {
            Qi类型::字符串 => self.弧retain(v),
            Qi类型::结构体(_) | Qi类型::数组(_) | Qi类型::装箱枚举(_) => {
                let f = self.弧函数("qi_obj_retain");
                let _ = self
                    .builder
                    .build_call(f, &[v.into_pointer_value().into()], "");
            }
            Qi类型::函数值(_) => {
                let f = self.弧函数("qi_closure_retain");
                let _ = self
                    .builder
                    .build_call(f, &[v.into_pointer_value().into()], "");
            }
            _ => {}
        }
    }

    /// 类型感知 release：字符串 → qi_string_free；结构体(i) → qi.release.s<i>
    /// （每类型释放函数：dec 归零时递归释放字段再 free 本体）；
    /// 数组(e) → qi.release.arr.{v|p|s<i>}；函数值 → qi_closure_release
    /// （归零时调 env dtor 级联释放捕获，再 free 本体）。其余类型不动作。
    pub(super) fn 弧release任意(&mut self, v: BasicValueEnum<'ctx>, t: Qi类型) {
        if !self.弧 || !v.is_pointer_value() {
            return;
        }
        match t {
            Qi类型::字符串 => self.弧release(v),
            Qi类型::结构体(idx) => {
                let f = self.弧函数(&结构体释放名(idx));
                let _ = self
                    .builder
                    .build_call(f, &[v.into_pointer_value().into()], "");
            }
            Qi类型::数组(e) => {
                let f = self.弧函数(&数组释放名(e));
                let _ = self
                    .builder
                    .build_call(f, &[v.into_pointer_value().into()], "");
            }
            Qi类型::装箱枚举(idx) => {
                let f = self.弧函数(&super::枚举::枚举释放名(idx));
                let _ = self
                    .builder
                    .build_call(f, &[v.into_pointer_value().into()], "");
            }
            Qi类型::函数值(_) => {
                let f = self.弧函数("qi_closure_release");
                let _ = self
                    .builder
                    .build_call(f, &[v.into_pointer_value().into()], "");
            }
            _ => {}
        }
    }

    /// 值 v（表达式 值node 生成，目标槽/字段声明类型 t）即将存入：
    /// BORROWED → retain（+1 归槽）；OWNED → 转移，不动作。t 非 RC 类型不动作。
    pub(super) fn 弧存入槽2(
        &mut self, v: BasicValueEnum<'ctx>, 值node: &AstNode, t: Qi类型
    ) {
        if self.弧 && 是RC类型(t) && !self.表达式拥有RC(值node, t) {
            self.弧retain任意(v, t);
        }
    }

    /// 释放 RC 槽位的旧值（类型感知）：load + release任意。
    /// 槽必须已初始化（entry null / zeroinit / 已存过值），release(null) 安全。
    pub(super) fn 弧释放槽旧值2(
        &mut self,
        slot: PointerValue<'ctx>,
        t: Qi类型,
    ) -> Result<(), String> {
        if !self.弧 || !是RC类型(t) {
            return Ok(());
        }
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let old = self
            .builder
            .build_load(ptrt, slot, "arc.old")
            .map_err(|e| e.to_string())?;
        self.弧release任意(old, t);
        Ok(())
    }

    /// 类型感知的"消费完毕释放"：OWNED 的 RC 值（字符串/结构体/数组）→ release。
    /// 仅用于已审计"值确实被丢弃/拷贝完毕"的调用点（表达式语句丢弃、实参消费）。
    pub(super) fn 弧消费后释放2(
        &mut self,
        v: BasicValueEnum<'ctx>,
        t: Qi类型,
        node: &AstNode,
    ) {
        if self.弧 && 是RC类型(t) && self.表达式拥有RC(node, t) {
            self.弧release任意(v, t);
        }
    }

    /// 在函数 entry 块建一个字符串局部槽：alloca + store null 都插在 entry 块
    /// （若 entry 已有终结指令则插在它之前）。保证槽支配所有 return 点，
    /// 出口统一释放合法；首次 release 旧值时 load 到 null（release(null) no-op）。
    pub(super) fn 弧字符串槽(
        &mut self,
        func: FunctionValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, String> {
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let 当前 = self.builder.get_insert_block();
        let entry = func
            .get_first_basic_block()
            .ok_or_else(|| "函数缺 entry 块".to_string())?;
        match entry.get_terminator() {
            Some(term) => self.builder.position_before(&term),
            None => self.builder.position_at_end(entry),
        }
        let p = self
            .builder
            .build_alloca(ptrt, name)
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(p, ptrt.const_null())
            .map_err(|e| e.to_string())?;
        if let Some(bb) = 当前 {
            self.builder.position_at_end(bb);
        }
        Ok(p)
    }

    /// 函数出口（每个 return 点、以及落底默认返回处，ret 之前）：
    /// release 所有 RC 类型（字符串/结构体/数组）局部槽。参数也在内 ——
    /// 序言已对 RC 参数 retain 一次，净额平衡（参数对调用方仍是借用）。
    /// 所有 RC 槽都保证 entry 块 alloca + null/立即初始化，release(null) 安全。
    pub(super) fn 弧释放局部(&mut self) -> Result<(), String> {
        if !self.弧 {
            return Ok(());
        }
        // 弱捕获局部（unowned）不 release —— 填槽/落地时都未 retain。
        let 槽们: Vec<(PointerValue<'ctx>, Qi类型)> = self
            .变量表
            .iter()
            .filter(|(名, (_, t))| 是RC类型(*t) && !self.弱局部.contains(*名))
            .map(|(_, (p, t))| (*p, *t))
            .collect();
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        for (p, t) in 槽们 {
            let v = self
                .builder
                .build_load(ptrt, p, "arc.exit")
                .map_err(|e| e.to_string())?;
            self.弧release任意(v, t);
        }
        Ok(())
    }

    // ───────────────── 所有权判定（纯语法 + 符号表，不产生 IR）─────────────────

    /// 该表达式的结果（若为字符串）是否 OWNED（+1 交到我手上）。
    /// 铁律：说 true 必须结构上 100% 确定；一切拿不准 → false（BORROWED，宁泄漏）。
    /// 分发逻辑逐条镜像 生成表达式 / 生成函数调用 的解析顺序。
    pub(super) fn 表达式拥有字符串(&self, node: &AstNode) -> bool {
        if !self.弧 {
            return false;
        }
        match node {
            // 拼接产物：qi_runtime_string_concat 刚 alloc 的新串
            AstNode::字符串连接表达式(_) => true,
            AstNode::二元操作表达式(b) => {
                b.operator == BinaryOperator::加
                    && (推断表达式类型(&b.left, &self.符号) == Qi类型::字符串
                        || 推断表达式类型(&b.right, &self.符号) == Qi类型::字符串)
            }

            AstNode::函数调用表达式(call) => self.调用拥有字符串(&call.callee),

            AstNode::方法调用表达式(mc) => {
                // 0) 未来:: 静态方法 → 返回 future 指针，非字符串
                if matches!(mc.object.as_ref(), AstNode::标识符表达式(id) if id.name == "未来")
                {
                    return false;
                }
                // 1) 结构体方法（返回约定 +1）；函数值字段以方法语法调用（fat call）同约定
                let recv = 推断表达式类型(&mc.object, &self.符号);
                if let Some(idx) = recv.结构体索引() {
                    if let Some(sig) = self.符号.方法.get(&(idx, mc.method_name.clone())) {
                        return sig.返回 == Qi类型::字符串;
                    }
                    if let Some((_, Qi类型::函数值(fi))) = self
                        .符号
                        .结构体信息(idx)
                        .and_then(|s| s.查字段(&mc.method_name))
                    {
                        return self
                            .符号
                            .函数值签名(fi)
                            .map(|s| s.返回 == Qi类型::字符串)
                            .unwrap_or(false);
                    }
                    return false;
                }
                // 1.5 / 4) 打印族无返回值
                if matches!(
                    mc.method_name.as_str(),
                    "打印行" | "打印" | "println" | "print" | "printf"
                ) {
                    return false;
                }
                // 2) 标准库分发（镜像 尝试标准库调用：接收者是非变量的裸标识符）
                if let AstNode::标识符表达式(id) = mc.object.as_ref() {
                    if !self.变量表.contains_key(&id.name) {
                        let m = mc.method_name.trim_start_matches(':');
                        let module_name = self
                            .导入别名
                            .get(&id.name)
                            .cloned()
                            .unwrap_or_else(|| id.name.clone());
                        let mf = self.注册表.get_function(&module_name, m).or_else(|| {
                            self.注册表
                                .get_function(&format!("标准库.{}", module_name), m)
                        });
                        if let Some(mf) = mf {
                            return 注册表类型转qi(&mf.return_type) == Qi类型::字符串
                                && !借用串FFI.contains(&mf.runtime_name.as_str());
                        }
                        // 3) 用户模块限定调用 别名.函数(...)（镜像分发条件）
                        if !self.全局变量表.contains_key(&id.name)
                            && self.符号.查函数返回(&mc.method_name).is_some()
                        {
                            return self.调用拥有字符串(&mc.method_name);
                        }
                    }
                }
                false
            }

            // 等待：Round E3 起 qi_future_await_ptr 是 take/新分配语义 → OWNED
            // （Pointer payload +1 移交；String payload 每次 rc_cstr 新分配）。
            // 数值 future 的 await 结果非指针，插桩点先验类型，不受影响。
            AstNode::等待表达式(_) => true,
            // 通道接收：发送即转移 —— 发端 retain/转移的那 +1 随值移交接收方
            AstNode::通道接收表达式(_) => true,
            // 其余（字面量 / 标识符 / 字段 / 数组元素 / 赋值值 …）→ BORROWED
            _ => false,
        }
    }

    /// 按名字的调用（函数调用表达式 / 用户模块限定调用）是否返回 OWNED 字符串。
    /// 镜像 生成函数调用 的分发顺序。
    fn 调用拥有字符串(&self, callee: &str) -> bool {
        // 外部 C 函数：char* 返回在调用点拷贝成 Qi 拥有的堆串（rc=1）→ OWNED。
        // 其余外部返回（整数/浮点/布尔/指针）非字符串，与此判定无关。
        if self.符号.外部函数.contains(callee) {
            return self
                .符号
                .函数
                .get(callee)
                .map(|s| s.返回 == Qi类型::字符串)
                .unwrap_or(false);
        }
        // 1) 间接调用：callee 是函数值局部/全局 → 合成闭包 / trampoline 均遵守
        //    返回约定 +1
        if let Some((_, t)) = self.变量表.get(callee) {
            if let Some(idx) = t.函数值索引() {
                return self
                    .符号
                    .函数值签名(idx)
                    .map(|s| s.返回 == Qi类型::字符串)
                    .unwrap_or(false);
            }
            // 是变量但不是函数值：生成函数调用 会继续按名字分发，镜像继续。
        } else if let Some((_, t)) = self.全局变量表.get(callee) {
            if let Some(idx) = t.函数值索引() {
                return self
                    .符号
                    .函数值签名(idx)
                    .map(|s| s.返回 == Qi类型::字符串)
                    .unwrap_or(false);
            }
        }
        // 2) 打印族：无返回值
        if matches!(callee, "打印行" | "println" | "打印" | "print" | "printf") {
            return false;
        }
        // 2.1) 工具模式(f)：返回编译期生成的 rc=1 新字符串（schema），自有。
        if callee == "工具模式" {
            return true;
        }
        // 2.5) 泛型函数模板：实例是普通用户函数，遵守返回约定 +1（插桩点先验
        //      实际类型是 字符串 才动作，非字符串实例不受影响）
        if self.符号.泛型函数模板.contains_key(callee) {
            return true;
        }
        // 3) 内建类型转换：int/float→string 是 rc=1 新串；其余非字符串
        match callee {
            "整数转字符串" | "int_to_string" | "浮点数转字符串" | "float_to_string" => {
                return true
            }
            "字符串转整数"
            | "string_to_int"
            | "字符串转浮点数"
            | "string_to_float"
            | "整数转浮点数"
            | "int_to_float"
            | "浮点数转整数"
            | "float_to_int" => return false,
            _ => {}
        }
        // 4) 同步/定时器内建都返回整数句柄，非字符串 —— 若命中同名用户函数下面
        //    误判也无妨：插桩点都先验实际类型是 字符串 才动作。
        // 5) 用户函数（返回约定 +1）—— 只看返回类型，重载各版本返回类型一致，取任一即可
        if let Some(s) = self.符号.解析函数(callee) {
            return s.返回 == Qi类型::字符串;
        }
        // 6) 无模块限定标准库（镜像 尝试无限定标准库）
        if let Some(mf) = self.查任意模块函数(callee.trim_start_matches(':')) {
            return 注册表类型转qi(&mf.return_type) == Qi类型::字符串
                && !借用串FFI.contains(&mf.runtime_name.as_str());
        }
        false
    }

    // ───────────── RC 对象所有权判定（结构体本体 / 数组本体） ─────────────

    /// 统一入口：该表达式的结果（其类型为 t）是否 OWNED（+1 交到我手上）。
    /// t 为字符串走字符串判定；结构体/数组/函数值走对象判定；其余恒 false。
    #[allow(non_snake_case)]
    pub(super) fn 表达式拥有RC(&self, node: &AstNode, t: Qi类型) -> bool {
        match t {
            Qi类型::字符串 => self.表达式拥有字符串(node),
            Qi类型::结构体(_) | Qi类型::数组(_) | Qi类型::函数值(_) | Qi类型::装箱枚举(_) => {
                self.表达式拥有对象(node)
            }
            _ => false,
        }
    }

    /// 该表达式是否为「装箱枚举构造」（`形状.圆(x)` 方法式 / `形状.点` 字段式）。
    /// 装箱枚举构造 = qi_obj_alloc rc=1 新对象 → OWNED。
    pub(super) fn 枚举构造是装箱(&self, object: &AstNode, 变体: &str) -> bool {
        if let AstNode::标识符表达式(id) = object {
            if !self.变量表.contains_key(&id.name) && !self.全局变量表.contains_key(&id.name)
            {
                if let Some(idx) = self.符号.枚举索引(&id.name) {
                    if let Some(info) = self.符号.枚举信息(idx) {
                        return info.装箱 && info.查变体(变体).is_some();
                    }
                }
            }
        }
        false
    }

    /// 结构体/数组/闭包值的 OWNED 判定。铁律同字符串：说 true 必须结构上 100% 确定；
    /// 拿不准 → false（BORROWED，宁泄漏）。OWNED 来源：
    ///   - 结构体字面量 / 数组字面量（qi_obj_alloc 刚分配，rc=1）
    ///   - 闭包字面量（qi_closure_create 刚分配，rc=1）
    ///   - 顶层函数名当值（每次提及新建 trampoline fat obj，rc=1）
    ///   - Qi 用户函数 / 方法 / 闭包（fat call）返回（返回约定 +1）
    ///   - `等待`（Round E3 take 语义：future 持有的 +1 移交）
    ///   - runtime FFI 永远不按 +1 交出对象 → false
    fn 表达式拥有对象(&self, node: &AstNode) -> bool {
        if !self.弧 {
            return false;
        }
        // 内建构造子（有/无/成/败）→ qi_obj_alloc rc=1 新装箱枚举 → OWNED
        if self.识别构造子调用(node).is_some() {
            return true;
        }
        // 用户泛型枚举构造（盒装.满(x) / 盒装.空盒）→ 同上 rc=1 新装箱枚举
        if self.识别泛型枚举构造(node).is_some() {
            return true;
        }
        match node {
            AstNode::结构体实例化表达式(_) => true,
            AstNode::数组字面量表达式(_) => true,
            // 闭包字面量：qi_closure_create rc=1 交出
            AstNode::闭包表达式(_) => true,
            // `等待 fut`：take 语义，future 内那份 +1 移交（见 表达式拥有字符串 同款注释）
            AstNode::等待表达式(_) => true,
            // 通道接收：发送即转移，+1 随值移交接收方
            AstNode::通道接收表达式(_) => true,
            // 顶层函数名当值：镜像 生成表达式 的标识符分发 —— 变量/全局优先
            // （load → BORROWED），都不是且确是已登记函数 → 每次新建 fat obj（OWNED）
            AstNode::标识符表达式(id) => {
                !self.变量表.contains_key(&id.name)
                    && !self.全局变量表.contains_key(&id.name)
                    && self.符号.函数为值(&id.name).is_some()
            }

            AstNode::函数调用表达式(call) => self.调用拥有对象(&call.callee),

            // 装箱枚举字段式构造 `形状.点`（无载荷变体仍是装箱指针）→ OWNED
            AstNode::字段访问表达式(fa) => self.枚举构造是装箱(&fa.object, &fa.field),

            AstNode::方法调用表达式(mc) => {
                // 装箱枚举方法式构造 `形状.圆(x)` → OWNED
                if self.枚举构造是装箱(&mc.object, &mc.method_name) {
                    return true;
                }
                // 未来:: 静态方法 → future 指针，非对象
                if matches!(mc.object.as_ref(), AstNode::标识符表达式(id) if id.name == "未来")
                {
                    return false;
                }
                // 结构体方法（返回约定 +1）；函数值字段以方法语法调用（fat call）同约定
                let recv = 推断表达式类型(&mc.object, &self.符号);
                if let Some(idx) = recv.结构体索引() {
                    if let Some(sig) = self.符号.方法.get(&(idx, mc.method_name.clone())) {
                        return 是对象类型(sig.返回);
                    }
                    if let Some((_, Qi类型::函数值(fi))) = self
                        .符号
                        .结构体信息(idx)
                        .and_then(|s| s.查字段(&mc.method_name))
                    {
                        return self
                            .符号
                            .函数值签名(fi)
                            .map(|s| 是对象类型(s.返回))
                            .unwrap_or(false);
                    }
                    return false;
                }
                // 标准库分发：FFI 一般不交对象所有权 → false；**例外**：注册表
                // 返回类型为 浮点数组 的 FFI（向量.加/归一化/数乘）按约定
                // qi_obj_alloc rc=1 新数组交出 → OWNED。
                // 用户模块限定调用 别名.函数(...)（镜像 生成表达式 的分发顺序：
                // 注册表两种 key 形式都命中不了才轮到用户函数）
                if let AstNode::标识符表达式(id) = mc.object.as_ref() {
                    if !self.变量表.contains_key(&id.name)
                        && !self.全局变量表.contains_key(&id.name)
                    {
                        let m = mc.method_name.trim_start_matches(':');
                        let module_name = self
                            .导入别名
                            .get(&id.name)
                            .cloned()
                            .unwrap_or_else(|| id.name.clone());
                        let mf = self.注册表.get_function(&module_name, m).or_else(|| {
                            self.注册表
                                .get_function(&format!("标准库.{}", module_name), m)
                        });
                        if let Some(mf) = mf {
                            return matches!(注册表类型转qi(&mf.return_type), Qi类型::数组(_));
                        }
                        if self.符号.查函数返回(&mc.method_name).is_some() {
                            return self.调用拥有对象(&mc.method_name);
                        }
                    }
                }
                false
            }

            // 其余（标识符 / 字段 / 数组元素 / 等待 / 赋值值 …）→ BORROWED
            _ => false,
        }
    }

    /// 按名字的调用是否返回 OWNED 对象（结构体/数组）。镜像 调用拥有字符串。
    fn 调用拥有对象(&self, callee: &str) -> bool {
        // 外部 C 函数：小结构体按值返回在调用点 store 进新 qi_obj 堆结构体（rc=1）→ OWNED。
        if self.符号.外部函数.contains(callee) {
            return self
                .符号
                .函数
                .get(callee)
                .map(|s| 是对象类型(s.返回))
                .unwrap_or(false);
        }
        // 1) 间接调用：函数值变量 → fat call 遵守返回约定 +1
        if let Some((_, t)) = self.变量表.get(callee) {
            if let Some(idx) = t.函数值索引() {
                return self
                    .符号
                    .函数值签名(idx)
                    .map(|s| 是对象类型(s.返回))
                    .unwrap_or(false);
            }
        } else if let Some((_, t)) = self.全局变量表.get(callee) {
            if let Some(idx) = t.函数值索引() {
                return self
                    .符号
                    .函数值签名(idx)
                    .map(|s| 是对象类型(s.返回))
                    .unwrap_or(false);
            }
        }
        // 2) 打印族 / 内建转换 / 同步内建：不返回对象
        if matches!(callee, "打印行" | "println" | "打印" | "print" | "printf") {
            return false;
        }
        // 2.1) 询问::<结构体>(...)：脱糖后返回一个 rc=1 的新结构体（自有），赋值不应再 retain。
        //      尝试询问::<T>(...)：改写为合成包装函数调用，返回自有 结果<T,字符串>（装箱枚举）
        //      —— 匹配/赋值按自有处理，否则匹配临时不释放，枚举本体+载荷级联泄漏。
        if callee == "询问" || callee == "尝试询问" {
            return true;
        }
        // 2.5) 泛型函数模板：实例是普通用户函数，遵守返回约定 +1（插桩点先验
        //      实际类型是 RC 才动作）
        if self.符号.泛型函数模板.contains_key(callee) {
            return true;
        }
        // 3) 用户函数（返回约定 +1）—— 只看返回类型，重载各版本返回类型一致，取任一即可
        if let Some(s) = self.符号.解析函数(callee) {
            return 是对象类型(s.返回);
        }
        // 4) 标准库 FFI：一般不交对象所有权；例外：返回 浮点数组 的 FFI
        //    （向量.*）按约定 rc=1 新数组交出 → OWNED（镜像 调用拥有字符串 6）
        if let Some(mf) = self.查任意模块函数(callee.trim_start_matches(':')) {
            return matches!(注册表类型转qi(&mf.return_type), Qi类型::数组(_));
        }
        false
    }

    // ───────────── 每类型释放函数 emit（QI_ARC=1 专用） ─────────────

    /// 为所有已登记结构体类型 + 两类数组 emit 释放函数（先全部声明再定义，
    /// 递归/互引用类型自然可用；循环引用会泄漏 —— 已知限制，接受）。
    /// 在 建结构体llvm类型 之后、任何函数体生成之前调用一次；
    /// 末尾再跑一次（幂等）为迟到的泛型结构体实例补定义。
    pub(super) fn 弧生成释放函数(&mut self) -> Result<(), String> {
        if !self.弧 {
            return Ok(());
        }
        // 泛型结构体实例迟到登记：先补齐 LLVM 类型（释放函数体要做 typed GEP）
        self.确保结构体llvm齐全();
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let voidt = self.ctx.void_type();
        let sig = voidt.fn_type(&[ptrt.into()], false);

        // 先声明全部（结构体 n 个 + 数组 2+n 个），定义体内可互相引用
        let n = self.符号.结构体.len();
        for i in 0..n {
            let name = 结构体释放名(i as u32);
            if self.module.get_function(&name).is_none() {
                self.module.add_function(&name, sig, None);
            }
        }
        for name in ["qi.release.arr.v", "qi.release.arr.p"] {
            if self.module.get_function(name).is_none() {
                self.module.add_function(name, sig, None);
            }
        }
        // E2：每个结构体类型一个类型化数组释放函数 qi.release.arr.s<i>
        for i in 0..n {
            let name = 数组释放名(元素类型::结构体(i as u32));
            if self.module.get_function(&name).is_none() {
                self.module.add_function(&name, sig, None);
            }
        }

        for i in 0..n {
            self.弧定义结构体释放(i as u32)?;
        }
        self.弧定义数组释放(数组元素释放::无)?; //   标量元素：只释放本体
        self.弧定义数组释放(数组元素释放::动态)?; // 指针元素：逐槽 qi_rc_release_any
        for i in 0..n {
            // 结构体元素：逐槽 qi.release.s<i> 类型化级联
            self.弧定义数组释放(数组元素释放::结构体(i as u32))?;
        }
        Ok(())
    }

    /// 定义 qi.release.s<idx>：
    ///   null → ret；qi_obj_dec(p) 旧值 != 1 → ret；
    ///   逐 RC 字段（字符串/嵌套结构体/数组）load + release；qi_obj_free(p)。
    fn 弧定义结构体释放(&mut self, idx: u32) -> Result<(), String> {
        let func = self
            .module
            .get_function(&结构体释放名(idx))
            .ok_or_else(|| "释放函数原型缺失".to_string())?;
        if func.get_first_basic_block().is_some() {
            return Ok(()); // 已定义（幂等）
        }
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let i64t = self.ctx.i64_type();
        let p = func.get_nth_param(0).unwrap().into_pointer_value();

        let entry = self.ctx.append_basic_block(func, "entry");
        let body = self.ctx.append_basic_block(func, "dec");
        let free_bb = self.ctx.append_basic_block(func, "free");
        let ret_bb = self.ctx.append_basic_block(func, "ret");

        self.builder.position_at_end(entry);
        let isnull = self
            .builder
            .build_is_null(p, "isnull")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(isnull, ret_bb, body)
            .map_err(|e| e.to_string())?;

        // dec：旧值 == 1 才走释放路径
        self.builder.position_at_end(body);
        let dec = self
            .module
            .get_function("qi_obj_dec")
            .ok_or("qi_obj_dec 未声明")?;
        let old = self
            .builder
            .build_call(dec, &[p.into()], "old")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or("qi_obj_dec 未返回")?
            .into_int_value();
        let last = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                old,
                i64t.const_int(1, false),
                "last",
            )
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(last, free_bb, ret_bb)
            .map_err(|e| e.to_string())?;

        // free：释放 RC 字段 → 释放本体
        self.builder.position_at_end(free_bb);
        let st = self.结构体llvm[idx as usize];
        let 字段类型 = self.符号.结构体[idx as usize].字段类型.clone();
        for (fi, ft) in 字段类型.iter().enumerate() {
            if !是RC类型(*ft) {
                continue;
            }
            let fptr = self
                .builder
                .build_struct_gep(st, p, fi as u32, "f")
                .map_err(|_| "释放函数字段 GEP 失败".to_string())?;
            let fv = self
                .builder
                .build_load(ptrt, fptr, "fv")
                .map_err(|e| e.to_string())?;
            self.弧release任意(fv, *ft);
        }
        let free = self
            .module
            .get_function("qi_obj_free")
            .ok_or("qi_obj_free 未声明")?;
        self.builder
            .build_call(free, &[p.into()], "")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_unconditional_branch(ret_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(ret_bb);
        self.builder.build_return(None).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 定义 qi.release.arr.{v|p|s<i>}：
    ///   null → ret；dec 旧值 != 1 → ret；
    ///   指针元素时按长度头逐槽释放（动态 → qi_rc_release_any：串→完整、
    ///   对象→浅、闭包→完整、其它→静默；结构体元素 → qi.release.s<i> 级联）；
    ///   qi_obj_free(p)。
    fn 弧定义数组释放(&mut self, 模式: 数组元素释放) -> Result<(), String> {
        let name = match 模式 {
            数组元素释放::无 => "qi.release.arr.v".to_string(),
            数组元素释放::动态 => "qi.release.arr.p".to_string(),
            数组元素释放::结构体(i) => 数组释放名(元素类型::结构体(i)),
        };
        let func = self
            .module
            .get_function(&name)
            .ok_or_else(|| "数组释放原型缺失".to_string())?;
        if func.get_first_basic_block().is_some() {
            return Ok(());
        }
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let i64t = self.ctx.i64_type();
        let p = func.get_nth_param(0).unwrap().into_pointer_value();

        let entry = self.ctx.append_basic_block(func, "entry");
        let body = self.ctx.append_basic_block(func, "dec");
        let free_bb = self.ctx.append_basic_block(func, "free");
        let ret_bb = self.ctx.append_basic_block(func, "ret");

        self.builder.position_at_end(entry);
        let isnull = self
            .builder
            .build_is_null(p, "isnull")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(isnull, ret_bb, body)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(body);
        let dec = self
            .module
            .get_function("qi_obj_dec")
            .ok_or("qi_obj_dec 未声明")?;
        let old = self
            .builder
            .build_call(dec, &[p.into()], "old")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or("qi_obj_dec 未返回")?
            .into_int_value();
        let last = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                old,
                i64t.const_int(1, false),
                "last",
            )
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(last, free_bb, ret_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(free_bb);
        if !matches!(模式, 数组元素释放::无) {
            // len = p[0]; for i in 0..len { 元素release(p[i+1]) }
            let head = self.槽指针(p, i64t.into(), 0)?;
            let len = self
                .builder
                .build_load(i64t, head, "len")
                .map_err(|e| e.to_string())?
                .into_int_value();
            let ivar = self
                .builder
                .build_alloca(i64t, "i")
                .map_err(|e| e.to_string())?;
            self.builder
                .build_store(ivar, i64t.const_zero())
                .map_err(|e| e.to_string())?;
            let cond_bb = self.ctx.append_basic_block(func, "loop.cond");
            let loop_bb = self.ctx.append_basic_block(func, "loop.body");
            let done_bb = self.ctx.append_basic_block(func, "loop.done");
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| e.to_string())?;

            self.builder.position_at_end(cond_bb);
            let i = self
                .builder
                .build_load(i64t, ivar, "iv")
                .map_err(|e| e.to_string())?
                .into_int_value();
            let c = self
                .builder
                .build_int_compare(inkwell::IntPredicate::SLT, i, len, "c")
                .map_err(|e| e.to_string())?;
            self.builder
                .build_conditional_branch(c, loop_bb, done_bb)
                .map_err(|e| e.to_string())?;

            self.builder.position_at_end(loop_bb);
            let off = self
                .builder
                .build_int_add(i, i64t.const_int(1, false), "off")
                .map_err(|e| e.to_string())?;
            let slot = self.槽指针动态(p, ptrt.into(), off)?;
            let ev = self
                .builder
                .build_load(ptrt, slot, "ev")
                .map_err(|e| e.to_string())?;
            let 元素release = match 模式 {
                数组元素释放::结构体(si) => self.弧函数(&结构体释放名(si)),
                _ => self.弧函数("qi_rc_release_any"),
            };
            self.builder
                .build_call(元素release, &[ev.into_pointer_value().into()], "")
                .map_err(|e| e.to_string())?;
            let inext = self
                .builder
                .build_int_add(i, i64t.const_int(1, false), "inext")
                .map_err(|e| e.to_string())?;
            self.builder
                .build_store(ivar, inext)
                .map_err(|e| e.to_string())?;
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| e.to_string())?;

            self.builder.position_at_end(done_bb);
        }
        let free = self
            .module
            .get_function("qi_obj_free")
            .ok_or("qi_obj_free 未声明")?;
        self.builder
            .build_call(free, &[p.into()], "")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_unconditional_branch(ret_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(ret_bb);
        self.builder.build_return(None).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 结构体类型 idx 的释放函数符号名。
fn 结构体释放名(idx: u32) -> String {
    format!("qi.release.s{}", idx)
}

/// 数组释放函数符号名（按元素类别分派）：
///   标量元素 → arr.v（只回收本体）
///   指针元素（类型不明）→ arr.p（逐槽 qi_rc_release_any 动态释放）
///   结构体元素（E2 类型化）→ arr.s<idx>（逐槽 qi.release.s<idx> 级联）
fn 数组释放名(e: 元素类型) -> String {
    match e {
        元素类型::指针 => "qi.release.arr.p".to_string(),
        元素类型::结构体(i) => format!("qi.release.arr.s{}", i),
        _ => "qi.release.arr.v".to_string(),
    }
}

/// 数组释放函数的元素处理模式。
#[derive(Clone, Copy)]
enum 数组元素释放 {
    /// 标量元素：不逐槽，只回收本体。
    无,
    /// 指针元素（类型不明）：逐槽 qi_rc_release_any 动态派发。
    动态,
    /// 结构体元素（类型已知）：逐槽 qi.release.s<idx> 类型化级联。
    结构体(u32),
}

/// 返回类型是否对象（结构体/数组/闭包/装箱枚举）—— 返回约定 +1 的判定用。
/// 装箱枚举（选项/结果 及带载荷用户枚举）是 qi_obj_alloc rc=1 对象，返回同约定。
fn 是对象类型(t: Qi类型) -> bool {
    matches!(
        t,
        Qi类型::结构体(_) | Qi类型::数组(_) | Qi类型::函数值(_) | Qi类型::装箱枚举(_)
    )
}
