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

use super::类型::Qi类型;
use super::类型检查::推断表达式类型;
use super::导入::注册表类型转qi;
use super::后端;
use crate::parser::ast::{AstNode, BinaryOperator};
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

/// 返回「借用串」的 runtime FFI 名单：这些函数返回的指针是内部借用 / 透传，
/// **不是** rc=1 交出的新串。判为 BORROWED（store 时 retain，绝不按 OWNED release）。
///   - qi_web_request_*  / qi_web_match_params：RequestParts / MatchResult 内部
///     RC 串的借引用（parts 持有一份引用，parts_free 释放那份）
///   - qi_future_await_ptr / qi_future_value_ptr：future 内部指针透传
const 借用串FFI: &[&str] = &[
    "qi_web_request_method",
    "qi_web_request_path",
    "qi_web_request_query",
    "qi_web_request_headers",
    "qi_web_request_body",
    "qi_web_match_params",
    "qi_future_await_ptr",
    "qi_future_value_ptr",
];

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
                self.module
                    .add_function(name, self.ctx.void_type().fn_type(&[ptrt.into()], false), None)
            }
        }
    }

    /// 对指针值发 qi_string_retain（ARC 关 / 非指针值 → 不动作）。
    pub(super) fn 弧retain(&mut self, v: BasicValueEnum<'ctx>) {
        if self.弧 && v.is_pointer_value() {
            let f = self.弧函数("qi_string_retain");
            let _ = self.builder.build_call(f, &[v.into_pointer_value().into()], "");
        }
    }

    /// 对指针值发 qi_string_free（ARC 关 / 非指针值 → 不动作）。
    pub(super) fn 弧release(&mut self, v: BasicValueEnum<'ctx>) {
        if self.弧 && v.is_pointer_value() {
            let f = self.弧函数("qi_string_free");
            let _ = self.builder.build_call(f, &[v.into_pointer_value().into()], "");
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
    /// release 所有字符串类型局部槽。参数也在内 —— 序言已对字符串参数 retain
    /// 一次，净额平衡（参数本身对调用方仍是借用）。
    pub(super) fn 弧释放局部(&mut self) -> Result<(), String> {
        if !self.弧 {
            return Ok(());
        }
        let 槽们: Vec<PointerValue<'ctx>> = self
            .变量表
            .values()
            .filter(|(_, t)| *t == Qi类型::字符串)
            .map(|(p, _)| *p)
            .collect();
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        for p in 槽们 {
            let v = self
                .builder
                .build_load(ptrt, p, "arc.exit")
                .map_err(|e| e.to_string())?;
            self.弧release(v);
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
                if matches!(mc.object.as_ref(), AstNode::标识符表达式(id) if id.name == "未来") {
                    return false;
                }
                // 1) 结构体方法（返回约定 +1）
                let recv = 推断表达式类型(&mc.object, &self.符号);
                if let Some(idx) = recv.结构体索引() {
                    if let Some(info) = self.符号.结构体信息(idx) {
                        if let Some(sig) = self
                            .符号
                            .方法
                            .get(&(info.名字.clone(), mc.method_name.clone()))
                        {
                            return sig.返回 == Qi类型::字符串;
                        }
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
                        let mf = self
                            .注册表
                            .get_function(&module_name, m)
                            .or_else(|| {
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

            // 等待：生成等待 走 qi_future_await_ptr（透传，BORROWED）
            AstNode::等待表达式(_) => false,
            // 其余（字面量 / 标识符 / 字段 / 数组元素 / 赋值值 / 通道收 …）→ BORROWED
            _ => false,
        }
    }

    /// 按名字的调用（函数调用表达式 / 用户模块限定调用）是否返回 OWNED 字符串。
    /// 镜像 生成函数调用 的分发顺序。
    fn 调用拥有字符串(&self, callee: &str) -> bool {
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
        // 3) 内建类型转换：int/float→string 是 rc=1 新串；其余非字符串
        match callee {
            "整数转字符串" | "int_to_string" | "浮点数转字符串" | "float_to_string" => {
                return true
            }
            "字符串转整数" | "string_to_int" | "字符串转浮点数" | "string_to_float"
            | "整数转浮点数" | "int_to_float" | "浮点数转整数" | "float_to_int" => {
                return false
            }
            _ => {}
        }
        // 4) 同步/定时器内建都返回整数句柄，非字符串 —— 若命中同名用户函数下面
        //    误判也无妨：插桩点都先验实际类型是 字符串 才动作。
        // 5) 用户函数（返回约定 +1）
        if let Some((_f, sig)) = self.尝试解析用户函数(callee) {
            return sig.map(|s| s.返回 == Qi类型::字符串).unwrap_or(false);
        }
        // 6) 无模块限定标准库（镜像 尝试无限定标准库）
        if let Some(mf) = self.查任意模块函数(callee.trim_start_matches(':')) {
            return 注册表类型转qi(&mf.return_type) == Qi类型::字符串
                && !借用串FFI.contains(&mf.runtime_name.as_str());
        }
        false
    }
}
