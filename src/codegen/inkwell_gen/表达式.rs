//! 表达式降级 —— AST 表达式 → 类型化 LLVM 值。
//!
//! 每个表达式先经类型检查器定型，再据类型选对应 LLVM 指令。
//! 返回 (值, Qi类型)，让调用点无需二次猜测。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::推断表达式类型;
use crate::parser::ast::{AstNode, BinaryOperator, LiteralValue, UnaryOperator};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::{FloatPredicate, IntPredicate};

/// qi-runtime RC buffer 识别 magic（qi_str.rs::QI_STR_MAGIC，位于 data-24 的 header 首 8 字节）。
const QI_STR_MAGIC: u64 = 0x5149_5352_4331_0001;

// ───── 询问/工具 脱糖用的 AST 构造小工具（模块级自由函数，供递归 helper 复用）─────
fn 询_id(name: &str) -> AstNode {
    AstNode::标识符表达式(crate::parser::ast::IdentifierExpression {
        name: name.to_string(),
        span: Default::default(),
    })
}
fn 询_str(s: &str) -> AstNode {
    AstNode::字面量表达式(crate::parser::ast::LiteralExpression {
        value: LiteralValue::字符串(s.to_string()),
        span: Default::default(),
    })
}
fn 询_mcall(obj: AstNode, m: &str, args: Vec<AstNode>) -> AstNode {
    AstNode::方法调用表达式(crate::parser::ast::MethodCallExpression {
        object: Box::new(obj),
        method_name: m.to_string(),
        arguments: args,
        span: Default::default(),
    })
}
/// 不朽引用计数（qi_str.rs::IMMORTAL_RC）：runtime 对 refcount >= 此值的串 retain/free 一律 no-op。
const QI_STR_IMMORTAL_RC: u64 = 1 << 61;

impl<'ctx> 后端<'ctx> {
    /// 把字符串字面量 emit 成带 immortal header 的全局常量，返回指向 data 字段的 i8* 指针。
    ///
    /// 布局与 qi-runtime 的 BufHeader 严格一致（repr(C)，8 对齐，data 在 base-24 处有 header）：
    ///   { i64 magic, i64 refcount=IMMORTAL, i64 capacity=字节长度, [len+1 x i8] data(带 NUL 尾) }
    /// 这样任何字面量指针都可被 runtime 安全 retain/release（immortal ⇒ no-op），
    /// 为 Round C 的逃逸点 ARC 插入铺路。同一模块内相同字面量按内容去重。
    pub(super) fn emit_immortal_string(
        &mut self,
        s: &str,
        name_hint: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        if let Some(p) = self.字符串字面量缓存.get(s) {
            return Ok(*p);
        }
        let i64t = self.ctx.i64_type();
        let bytes = s.as_bytes();
        // data：字节 + NUL 尾（[len+1 x i8]）
        let data = self.ctx.const_string(bytes, true);
        let struct_ty = self.ctx.struct_type(
            &[
                i64t.into(),
                i64t.into(),
                i64t.into(),
                data.get_type().into(),
            ],
            false,
        );
        let init = self.ctx.const_struct(
            &[
                i64t.const_int(QI_STR_MAGIC, false).into(),
                i64t.const_int(QI_STR_IMMORTAL_RC, false).into(),
                i64t.const_int(bytes.len() as u64, false).into(),
                data.into(),
            ],
            false,
        );
        let g = self.module.add_global(struct_ty, None, name_hint);
        g.set_initializer(&init);
        g.set_constant(true);
        g.set_linkage(inkwell::module::Linkage::Private);
        g.set_unnamed_address(inkwell::values::UnnamedAddress::Global);
        // header 24 字节 + 8 对齐 ⇒ data 起点自然 8 对齐
        g.set_alignment(8);
        // GEP 到第 3 个字段（data）的首字节 → i8*，与现有字符串值表示一致
        let i32t = self.ctx.i32_type();
        let ptr = unsafe {
            g.as_pointer_value().const_gep(
                struct_ty,
                &[
                    i32t.const_zero(),
                    i32t.const_int(3, false),
                    i32t.const_zero(),
                ],
            )
        };
        self.字符串字面量缓存.insert(s.to_string(), ptr);
        Ok(ptr)
    }

    /// 生成表达式，返回其 LLVM 值与 Qi 类型。`空`（如无返回值调用）返回 None。
    pub(super) fn 生成表达式(
        &mut self,
        node: &AstNode,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        // 内建构造子（有/无/成/败）—— 无期望上下文时按参数反推（规则④）。
        // 有期望的位置（返回/变量注解/形参/枚举载荷）走 生成带期望 提前定型。
        if let Some((名, 参数)) = self.识别构造子调用(node) {
            return self.生成构造子(&名, &参数, None).map(Some);
        }
        // 用户泛型枚举构造（盒装.满(x) / 盒装.空盒）—— 无期望时按载荷反推
        if let Some((模板, 变体, 参数)) = self.识别泛型枚举构造(node) {
            return self.生成泛型枚举构造(&模板, &变体, &参数, None).map(Some);
        }
        match node {
            AstNode::字面量表达式(lit) => Ok(Some(self.生成字面量(&lit.value)?)),

            AstNode::标识符表达式(id) => {
                // 局部变量优先
                if let Some((ptr, t)) = self.变量表.get(&id.name).cloned() {
                    let llvmt = self
                        .llvm基础类型(t)
                        .ok_or_else(|| format!("变量 {} 类型无效", id.name))?;
                    let v = self
                        .builder
                        .build_load(llvmt, ptr, &id.name)
                        .map_err(|e| e.to_string())?;
                    return Ok(Some((v, t)));
                }
                // 全局变量
                if let Some((g, t)) = self.全局变量表.get(&id.name).map(|(g, t)| (*g, *t)) {
                    let llvmt = self
                        .llvm基础类型(t)
                        .ok_or_else(|| format!("全局 {} 类型无效", id.name))?;
                    let v = self
                        .builder
                        .build_load(llvmt, g.as_pointer_value(), &id.name)
                        .map_err(|e| e.to_string())?;
                    return Ok(Some((v, t)));
                }
                // 否则可能是「函数名当值」→ 取函数指针
                if let Some(v) = self.标识符作为函数值(&id.name)? {
                    return Ok(Some(v));
                }
                Err(format!("未声明的变量: {}", id.name))
            }

            AstNode::二元操作表达式(b) => self.生成二元(b).map(Some),

            AstNode::一元操作表达式(u) => {
                let (v, t) = self
                    .生成表达式(&u.operand)?
                    .ok_or_else(|| "一元操作数无值".to_string())?;
                match u.operator {
                    UnaryOperator::负 => {
                        if t.是浮点() {
                            let r = self
                                .builder
                                .build_float_neg(v.into_float_value(), "fneg")
                                .map_err(|e| e.to_string())?;
                            Ok(Some((r.into(), Qi类型::浮点数)))
                        } else {
                            let r = self
                                .builder
                                .build_int_neg(v.into_int_value(), "ineg")
                                .map_err(|e| e.to_string())?;
                            Ok(Some((r.into(), Qi类型::整数)))
                        }
                    }
                    UnaryOperator::非 => {
                        let r = self
                            .builder
                            .build_not(v.into_int_value(), "not")
                            .map_err(|e| e.to_string())?;
                        Ok(Some((r.into(), Qi类型::布尔)))
                    }
                    UnaryOperator::正 => Ok(Some((v, t))),
                }
            }

            AstNode::字符串连接表达式(sc) => {
                let (l, l新) = self.生成拼接操作数(&sc.left)?;
                let (r, r新) = self.生成拼接操作数(&sc.right)?;
                Ok(Some((
                    self.拼接字符串带释放(l, l新, r, r新)?,
                    Qi类型::字符串,
                )))
            }

            AstNode::函数调用表达式(call) => self.生成函数调用(call),

            AstNode::结构体实例化表达式(lit) => self.生成结构体字面量(lit).map(Some),

            AstNode::闭包表达式(c) => self.生成闭包表达式(c).map(Some),

            AstNode::通道创建表达式(c) => self.生成通道创建(c).map(Some),
            AstNode::通道发送表达式(s) => self.生成通道发送(s).map(Some),
            AstNode::通道接收表达式(r) => self.生成通道接收(r).map(Some),
            AstNode::协程启动表达式(g) => self.生成协程启动(g),

            AstNode::字段访问表达式(fa) => {
                // 枚举构造 `颜色.红` / `形状.点`：接收者是枚举名（非变量）且字段是其变体
                if let AstNode::标识符表达式(id) = fa.object.as_ref() {
                    if !self.变量表.contains_key(&id.name)
                        && !self.全局变量表.contains_key(&id.name)
                        && self.符号.是枚举名(&id.name)
                    {
                        if let Some(eidx) = self.符号.枚举索引(&id.name) {
                            if self
                                .符号
                                .枚举信息(eidx)
                                .and_then(|e| e.查变体(&fa.field))
                                .is_some()
                            {
                                return self.生成枚举构造(&id.name, &fa.field, &[]).map(Some);
                            }
                        }
                    }
                }
                // 数组 .长度 属性
                if fa.field == "长度" && 推断表达式类型(&fa.object, &self.符号).数组元素().is_some()
                {
                    let (av, _) = self
                        .生成表达式(&fa.object)?
                        .ok_or_else(|| "数组表达式无值".to_string())?;
                    return self.生成数组长度(av.into_pointer_value()).map(Some);
                }
                self.生成字段访问(&fa.object, &fa.field).map(Some)
            }

            AstNode::数组字面量表达式(arr) => self.生成数组字面量(arr).map(Some),
            AstNode::数组访问表达式(acc) => self.生成数组访问(acc).map(Some),

            // 取地址：局部/全局变量 → 其 alloca 指针（按 字符串/ptr 语义暴露）
            AstNode::取地址表达式(a) => {
                if let AstNode::标识符表达式(id) = a.expression.as_ref() {
                    let ptr = if let Some((p, _)) = self.变量表.get(&id.name) {
                        *p
                    } else if let Some((g, _)) = self.全局变量表.get(&id.name) {
                        g.as_pointer_value()
                    } else {
                        return Err(format!("取地址：未声明变量 {}", id.name));
                    };
                    return Ok(Some((ptr.into(), Qi类型::字符串)));
                }
                Err("取地址仅支持变量".to_string())
            }
            // 解引用：ptr → load i64（当整数用；指针演示够用）
            AstNode::解引用表达式(d) => {
                let (v, _t) = self
                    .生成表达式(&d.expression)?
                    .ok_or_else(|| "解引用操作数无值".to_string())?;
                let p = if v.is_pointer_value() {
                    v.into_pointer_value()
                } else {
                    self.builder
                        .build_int_to_ptr(
                            v.into_int_value(),
                            self.ctx.ptr_type(inkwell::AddressSpace::default()),
                            "i2p",
                        )
                        .map_err(|e| e.to_string())?
                };
                let val = self
                    .builder
                    .build_load(self.ctx.i64_type(), p, "deref")
                    .map_err(|e| e.to_string())?;
                Ok(Some((val, Qi类型::整数)))
            }

            AstNode::等待表达式(a) => self.生成等待(&a.expression).map(Some),

            AstNode::方法调用表达式(mc) => {
                // 0a) 枚举构造 `形状.圆(3.14)`：接收者是枚举名（非变量）且方法是带载荷变体
                if let AstNode::标识符表达式(id) = mc.object.as_ref() {
                    if !self.变量表.contains_key(&id.name)
                        && !self.全局变量表.contains_key(&id.name)
                        && self.符号.是枚举名(&id.name)
                    {
                        if let Some(eidx) = self.符号.枚举索引(&id.name) {
                            if self
                                .符号
                                .枚举信息(eidx)
                                .and_then(|e| e.查变体(&mc.method_name))
                                .is_some()
                            {
                                return self
                                    .生成枚举构造(&id.name, &mc.method_name, &mc.arguments)
                                    .map(Some);
                            }
                        }
                    }
                }
                // 0) 未来::就绪 / 未来::失败 静态方法
                if matches!(mc.object.as_ref(), AstNode::标识符表达式(id) if id.name == "未来")
                {
                    if let Some(v) =
                        self.生成未来静态方法(&mc.method_name, &mc.arguments)?
                    {
                        return Ok(Some(v));
                    }
                }
                // 1) 结构体方法（接收者是结构体，含链式）
                if let Some(v) =
                    self.生成方法调用(&mc.object, &mc.method_name, &mc.arguments)?
                {
                    return Ok(v);
                }
                // 1.5) IO.打印行/打印：接收者是模块名（非变量）时走通用打印（支持多参 + 各类型重载），
                //      避免 stdlib 单参 qi_runtime_println 收到多参报错。
                if matches!(mc.method_name.as_str(), "打印行" | "打印")
                    && matches!(mc.object.as_ref(), AstNode::标识符表达式(id)
                        if !self.变量表.contains_key(&id.name)
                            && !self.全局变量表.contains_key(&id.name))
                {
                    if let Some(v) = self.生成打印方法(&mc.method_name, &mc.arguments)? {
                        return Ok(v);
                    }
                }
                // 2) 标准库分发（接收者是模块名/别名，如 IO.打印行 / 字符串::字节长度）
                if let Some(v) =
                    self.尝试标准库调用(&mc.object, &mc.method_name, &mc.arguments)?
                {
                    return Ok(v);
                }
                // 3) 用户模块限定调用：别名.函数(...) —— 如 `导入 X.追踪 作为 追踪; 追踪.启用即时打印()`。
                //    接收者是裸标识符且不是局部变量/结构体 → 把 method 当用户函数解析。
                if let AstNode::标识符表达式(id) = mc.object.as_ref() {
                    if !self.变量表.contains_key(&id.name)
                        && !self.全局变量表.contains_key(&id.name)
                        && self.符号.查函数返回(&mc.method_name).is_some()
                    {
                        let call = crate::parser::ast::FunctionCallExpression {
                            module_qualifier: None,
                            callee: mc.method_name.clone(),
                            type_arguments: vec![],
                            arguments: mc.arguments.clone(),
                            span: mc.span.clone(),
                        };
                        return self.生成函数调用(&call);
                    }
                }
                // 4) 内建打印归一（IO 未导入时的 打印行 兜底）
                if let Some(v) = self.生成打印方法(&mc.method_name, &mc.arguments)? {
                    return Ok(v);
                }
                Err(format!("无法解析方法调用: {}", mc.method_name))
            }

            AstNode::赋值表达式(a) => {
                match a.target.as_ref() {
                    AstNode::标识符表达式(id) => {
                        let (mut v, vt) = self
                            .生成表达式(&a.value)?
                            .ok_or_else(|| "赋值右值无值".to_string())?;
                        // 局部变量优先，否则全局
                        let (ptr, target_t) =
                            if let Some((p, t)) = self.变量表.get(&id.name).cloned() {
                                (p, t)
                            } else if let Some((g, t)) =
                                self.全局变量表.get(&id.name).map(|(g, t)| (*g, *t))
                            {
                                (g.as_pointer_value(), t)
                            } else {
                                return Err(format!("赋值给未声明变量: {}", id.name));
                            };
                        // 隐式协调：整数→浮点提升、布尔(i1)→整数(i64) 加宽等
                        v = self.协调到类型(v, vt, target_t)?;
                        // ARC：RC 槽（字符串/结构体/数组）覆写 —— 先 retain 新值
                        // （BORROWED 时；自赋值安全的关键顺序），再释放旧值，最后 store。
                        if self.弧开() && super::所有权::是RC类型(target_t) && v.is_pointer_value()
                        {
                            self.弧存入槽2(v, &a.value, target_t);
                            self.弧释放槽旧值2(ptr, target_t)?;
                        }
                        self.builder
                            .build_store(ptr, v)
                            .map_err(|e| e.to_string())?;
                        Ok(Some((v, target_t)))
                    }
                    // 字段赋值 x.字段 = 值
                    AstNode::字段访问表达式(fa) => {
                        self.生成字段赋值(&fa.object, &fa.field, &a.value).map(Some)
                    }
                    // 下标赋值 甲[i] = 值
                    AstNode::数组访问表达式(acc) => {
                        self.生成数组元素赋值(acc, &a.value).map(Some)
                    }
                    _ => Err("暂只支持标识符 / 字段 / 数组下标赋值".to_string()),
                }
            }

            // 匹配（match）：语句语义的 if-else 链降级（见 匹配.rs），不产生值
            AstNode::匹配表达式(m) => self.生成匹配(m),

            _ => Ok(None),
        }
    }

    /// 字面量降级。
    fn 生成字面量(
        &mut self,
        v: &LiteralValue,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        Ok(match v {
            LiteralValue::整数(n) => (
                self.ctx.i64_type().const_int(*n as u64, true).into(),
                Qi类型::整数,
            ),
            LiteralValue::浮点数(f) => {
                (self.ctx.f64_type().const_float(*f).into(), Qi类型::浮点数)
            }
            LiteralValue::布尔(b) => (
                self.ctx.bool_type().const_int(*b as u64, false).into(),
                Qi类型::布尔,
            ),
            LiteralValue::字符(c) => (
                self.ctx.i64_type().const_int(*c as u64, false).into(),
                Qi类型::整数,
            ),
            LiteralValue::字符串(s) => {
                // 带 immortal header 的全局常量（可安全 retain/release），而非裸 rodata
                let p = self.emit_immortal_string(s, "qi.str")?;
                (p.into(), Qi类型::字符串)
            }
        })
    }

    /// 二元运算：据操作数类型选 int/float 指令；比较返回 i1。
    fn 生成二元(
        &mut self,
        b: &crate::parser::ast::BinaryExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        use BinaryOperator::*;

        // 字符串 + 字符串 → 拼接
        if b.operator == 加 {
            let lt = 推断表达式类型(&b.left, &self.符号);
            let rt = 推断表达式类型(&b.right, &self.符号);
            if lt == Qi类型::字符串 || rt == Qi类型::字符串 {
                let (l, l新) = self.生成拼接操作数(&b.left)?;
                let (r, r新) = self.生成拼接操作数(&b.right)?;
                return Ok((self.拼接字符串带释放(l, l新, r, r新)?, Qi类型::字符串));
            }
        }

        // 逻辑与/或：位运算即可（操作数为 i1）
        if b.operator == 与 || b.operator == 或 {
            let l = self.生成为i1(&b.left)?;
            let r = self.生成为i1(&b.right)?;
            let v = if b.operator == 与 {
                self.builder.build_and(l, r, "and")
            } else {
                self.builder.build_or(l, r, "or")
            }
            .map_err(|e| e.to_string())?;
            return Ok((v.into(), Qi类型::布尔));
        }

        // 位运算：& | ^ << >>（仅整数合法）
        if matches!(b.operator, 位与 | 位或 | 位异或 | 左移 | 右移) {
            return self.生成位运算(b);
        }

        let (lv, lt) = self
            .生成表达式(&b.left)?
            .ok_or_else(|| "二元左操作数无值".to_string())?;
        let (rv, rt) = self
            .生成表达式(&b.right)?
            .ok_or_else(|| "二元右操作数无值".to_string())?;

        // 字符串比较：== / != / < / <= / > / >= → qi_runtime_string_compare(返回 <0/0/>0)
        if (lt == Qi类型::字符串 || rt == Qi类型::字符串)
            && matches!(
                b.operator,
                等于 | 不等于 | 大于 | 小于 | 大于等于 | 小于等于
            )
        {
            let cmp = self.调用返回i32("qi_runtime_string_compare", &[lv.into(), rv.into()])?;
            // ARC：比较已读完字节，OWNED 操作数（拼接/FFI 返回串）释放
            self.弧消费后释放(lv, lt, &b.left);
            self.弧消费后释放(rv, rt, &b.right);
            let zero = self.ctx.i32_type().const_zero();
            let pred = match b.operator {
                等于 => IntPredicate::EQ,
                不等于 => IntPredicate::NE,
                大于 => IntPredicate::SGT,
                小于 => IntPredicate::SLT,
                大于等于 => IntPredicate::SGE,
                小于等于 => IntPredicate::SLE,
                _ => unreachable!(),
            };
            let c = self
                .builder
                .build_int_compare(pred, cmp, zero, "strcmp")
                .map_err(|e| e.to_string())?;
            return Ok((c.into(), Qi类型::布尔));
        }

        // 指针比较：任一操作数是 指针 → 两侧统一 ptrtoint 成 i64 再整数比较。
        // 覆盖 `malloc(n) != 0`（右侧是整数字面量）、指针==指针、指针!=指针。
        // 指针不参与算术（malloc 结果只能比较/传回 C），算术运算人话报错，
        // 也避免指针值落进下面的 into_int_value panic。
        if lt == Qi类型::指针 || rt == Qi类型::指针 {
            if !matches!(
                b.operator,
                等于 | 不等于 | 大于 | 小于 | 大于等于 | 小于等于
            ) {
                return Err(
                    "指针不支持算术运算 —— 只能做比较（== != 大于 小于 …）或传回 C 函数"
                        .to_string(),
                );
            }
            let i64t = self.ctx.i64_type();
            let li = if lv.is_pointer_value() {
                self.builder
                    .build_ptr_to_int(lv.into_pointer_value(), i64t, "p2i")
                    .map_err(|e| e.to_string())?
            } else {
                let iv = lv.into_int_value();
                if iv.get_type().get_bit_width() < 64 {
                    self.整数加宽到i64(iv)?
                } else {
                    iv
                }
            };
            let ri = if rv.is_pointer_value() {
                self.builder
                    .build_ptr_to_int(rv.into_pointer_value(), i64t, "p2i")
                    .map_err(|e| e.to_string())?
            } else {
                let iv = rv.into_int_value();
                if iv.get_type().get_bit_width() < 64 {
                    self.整数加宽到i64(iv)?
                } else {
                    iv
                }
            };
            let pred = match b.operator {
                等于 => IntPredicate::EQ,
                不等于 => IntPredicate::NE,
                // 地址序比较按无符号（指针无正负）
                大于 => IntPredicate::UGT,
                小于 => IntPredicate::ULT,
                大于等于 => IntPredicate::UGE,
                小于等于 => IntPredicate::ULE,
                _ => unreachable!(),
            };
            let c = self
                .builder
                .build_int_compare(pred, li, ri, "ptrcmp")
                .map_err(|e| e.to_string())?;
            return Ok((c.into(), Qi类型::布尔));
        }

        // 复合类型（结构体/装箱枚举/数组/…）不支持算术与比较 —— 人话报错，
        // 不落进 int 路径（指针值 into_int_value 会 panic）。泛型 T 实例化成
        // 这类类型时用 `>` 等比较也在此拦截。无载荷枚举是 i64 tag，放行。
        for (t, 侧) in [(lt, "左"), (rt, "右")] {
            let 种类 = match t {
                Qi类型::结构体(_) => Some("结构体"),
                Qi类型::装箱枚举(_) => Some("带载荷枚举"),
                Qi类型::数组(_) => Some("数组"),
                Qi类型::函数值(_) => Some("函数值"),
                Qi类型::未来(_) => Some("未来"),
                Qi类型::通道(_) => Some("通道"),
                _ => None,
            };
            if let Some(n) = 种类 {
                return Err(format!(
                    "比较/算术运算不支持{}操作数（{}）。比较只对 整数/浮点数/字符串 合法；枚举请用 匹配 解构后再比较",
                    侧, n
                ));
            }
        }

        let 用浮点 = lt.是浮点() || rt.是浮点();

        if 用浮点 {
            let lf = self.转浮点(lv, lt)?;
            let rf = self.转浮点(rv, rt)?;
            let v: BasicValueEnum = match b.operator {
                加 => self
                    .builder
                    .build_float_add(lf, rf, "fadd")
                    .map_err(|e| e.to_string())?
                    .into(),
                减 => self
                    .builder
                    .build_float_sub(lf, rf, "fsub")
                    .map_err(|e| e.to_string())?
                    .into(),
                乘 => self
                    .builder
                    .build_float_mul(lf, rf, "fmul")
                    .map_err(|e| e.to_string())?
                    .into(),
                除 => self
                    .builder
                    .build_float_div(lf, rf, "fdiv")
                    .map_err(|e| e.to_string())?
                    .into(),
                取余 => self
                    .builder
                    .build_float_rem(lf, rf, "frem")
                    .map_err(|e| e.to_string())?
                    .into(),
                _ => {
                    let pred = match b.operator {
                        等于 => FloatPredicate::OEQ,
                        不等于 => FloatPredicate::ONE,
                        大于 => FloatPredicate::OGT,
                        小于 => FloatPredicate::OLT,
                        大于等于 => FloatPredicate::OGE,
                        小于等于 => FloatPredicate::OLE,
                        _ => return Err("非法浮点比较".to_string()),
                    };
                    let c = self
                        .builder
                        .build_float_compare(pred, lf, rf, "fcmp")
                        .map_err(|e| e.to_string())?;
                    return Ok((c.into(), Qi类型::布尔));
                }
            };
            Ok((v, Qi类型::浮点数))
        } else {
            let li = lv.into_int_value();
            let ri = rv.into_int_value();
            let v: BasicValueEnum = match b.operator {
                加 => self
                    .builder
                    .build_int_add(li, ri, "iadd")
                    .map_err(|e| e.to_string())?
                    .into(),
                减 => self
                    .builder
                    .build_int_sub(li, ri, "isub")
                    .map_err(|e| e.to_string())?
                    .into(),
                乘 => self
                    .builder
                    .build_int_mul(li, ri, "imul")
                    .map_err(|e| e.to_string())?
                    .into(),
                除 => self
                    .builder
                    .build_int_signed_div(li, ri, "idiv")
                    .map_err(|e| e.to_string())?
                    .into(),
                取余 => self
                    .builder
                    .build_int_signed_rem(li, ri, "irem")
                    .map_err(|e| e.to_string())?
                    .into(),
                _ => {
                    let pred = match b.operator {
                        等于 => IntPredicate::EQ,
                        不等于 => IntPredicate::NE,
                        大于 => IntPredicate::SGT,
                        小于 => IntPredicate::SLT,
                        大于等于 => IntPredicate::SGE,
                        小于等于 => IntPredicate::SLE,
                        _ => return Err("非法整数比较".to_string()),
                    };
                    let c = self
                        .builder
                        .build_int_compare(pred, li, ri, "icmp")
                        .map_err(|e| e.to_string())?;
                    return Ok((c.into(), Qi类型::布尔));
                }
            };
            Ok((v, Qi类型::整数))
        }
    }

    /// 位运算 & | ^ << >>：只对整数合法（i64 按位 / 移位，>> 为算术右移）。
    /// 浮点 / 字符串操作数直接编译报错；布尔(i1) 按隐式加宽规则 zext 成 i64 参与。
    fn 生成位运算(
        &mut self,
        b: &crate::parser::ast::BinaryExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        use BinaryOperator::*;
        let 符号 = match b.operator {
            位与 => "&",
            位或 => "|",
            位异或 => "^",
            左移 => "<<",
            右移 => ">>",
            _ => unreachable!(),
        };
        // 先按推断类型把浮点/字符串挡在编译期（人话报错）
        for (side, 侧名) in [(&b.left, "左"), (&b.right, "右")] {
            let t = 推断表达式类型(side, &self.符号);
            match t {
                Qi类型::浮点数 => {
                    return Err(format!(
                        "位运算 {} 只能用于整数，{}操作数是浮点数。请先用 浮点数转整数(...) 转换",
                        符号, 侧名
                    ));
                }
                Qi类型::字符串 => {
                    return Err(format!(
                        "位运算 {} 只能用于整数，{}操作数是字符串",
                        符号, 侧名
                    ));
                }
                _ => {}
            }
        }
        let (lv, lt) = self
            .生成表达式(&b.left)?
            .ok_or_else(|| "位运算左操作数无值".to_string())?;
        let (rv, rt) = self
            .生成表达式(&b.right)?
            .ok_or_else(|| "位运算右操作数无值".to_string())?;
        // 生成后再兜一层（推断为 未知 但实际生成出浮点/指针值的情况）
        if !lv.is_int_value() || !rv.is_int_value() {
            return Err(format!("位运算 {} 只能用于整数", 符号));
        }
        let li = self.整数加宽到i64(lv.into_int_value())?;
        let ri = self.整数加宽到i64(rv.into_int_value())?;
        let _ = (lt, rt);
        let v: BasicValueEnum = match b.operator {
            位与 => self
                .builder
                .build_and(li, ri, "band")
                .map_err(|e| e.to_string())?
                .into(),
            位或 => self
                .builder
                .build_or(li, ri, "bor")
                .map_err(|e| e.to_string())?
                .into(),
            位异或 => self
                .builder
                .build_xor(li, ri, "bxor")
                .map_err(|e| e.to_string())?
                .into(),
            左移 => self
                .builder
                .build_left_shift(li, ri, "shl")
                .map_err(|e| e.to_string())?
                .into(),
            // 算术右移（带符号，i64）
            右移 => self
                .builder
                .build_right_shift(li, ri, true, "ashr")
                .map_err(|e| e.to_string())?
                .into(),
            _ => unreachable!(),
        };
        Ok((v, Qi类型::整数))
    }

    /// 窄位宽整数（典型 i1 布尔）zext 到 i64；已是 i64 原样返回。
    pub(super) fn 整数加宽到i64(
        &mut self,
        iv: inkwell::values::IntValue<'ctx>,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        if iv.get_type().get_bit_width() < 64 {
            self.builder
                .build_int_z_extend(iv, self.ctx.i64_type(), "zext")
                .map_err(|e| e.to_string())
        } else {
            Ok(iv)
        }
    }

    /// 把任意数值值转 double（int→sitofp，float 原样）。
    fn 转浮点(
        &mut self,
        v: BasicValueEnum<'ctx>,
        t: Qi类型,
    ) -> Result<inkwell::values::FloatValue<'ctx>, String> {
        if t.是浮点() {
            Ok(v.into_float_value())
        } else {
            self.builder
                .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
                .map_err(|e| e.to_string())
        }
    }

    /// 把一个值协调到目标 Qi 类型的 LLVM 表示（实参传递 / 赋值等处）。
    /// int↔float 提升；ptr(字符串/结构体/数组/函数值) ↔ i64 句柄 双向；位宽对齐。
    pub(super) fn 协调到类型(
        &mut self,
        v: BasicValueEnum<'ctx>,
        实际: Qi类型,
        期望: Qi类型,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let 目标ptr = matches!(
            期望,
            Qi类型::字符串
                | Qi类型::指针
                | Qi类型::结构体(_)
                | Qi类型::装箱枚举(_)
                | Qi类型::函数值(_)
                | Qi类型::数组(_)
                | Qi类型::通道(_)
                | Qi类型::未来(_)
        );
        if 期望.是浮点() && !实际.是浮点() && v.is_int_value() {
            return Ok(self.转浮点(v, 实际)?.into());
        }
        if v.is_pointer_value() && !目标ptr && 期望 != Qi类型::未知 {
            // ptr → i64 句柄（如通道句柄 / 传给 整数 形参）
            return Ok(self
                .builder
                .build_ptr_to_int(v.into_pointer_value(), self.ctx.i64_type(), "p2i")
                .map_err(|e| e.to_string())?
                .into());
        }
        if v.is_int_value() && 目标ptr {
            // i64 句柄 → ptr（如整数句柄传给 字符串/结构体 形参）
            return Ok(self
                .builder
                .build_int_to_ptr(
                    v.into_int_value(),
                    self.ctx.ptr_type(inkwell::AddressSpace::default()),
                    "i2p",
                )
                .map_err(|e| e.to_string())?
                .into());
        }
        // 布尔(i1) → 整数(i64) 隐式加宽：比较/逻辑结果当整数用（真=1 假=0）。
        // 覆盖 实参 / 变量初值与赋值 / 数组元素 等一切「期待整数拿到 i1」处。
        if v.is_int_value() && 期望 == Qi类型::整数 {
            let iv = v.into_int_value();
            if iv.get_type().get_bit_width() < 64 {
                return Ok(self.整数加宽到i64(iv)?.into());
            }
        }
        Ok(v)
    }

    /// 生成表达式并保证结果是 i8*（字符串）；数值先转字符串。
    pub(super) fn 生成为字符串(
        &mut self,
        node: &AstNode,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        Ok(self.生成拼接操作数(node)?.0)
    }

    /// 拼接场景专用：生成字符串操作数，并返回它是否是「本次新建、绝不逃逸的临时值」。
    ///
    /// 新建 = true 仅当结构上 100% 确定该指针是本表达式刚 alloc 出来、
    /// 且只会作为上层拼接的输入被消费一次（拼接读完字节即可安全释放）：
    ///   - 数值 → int_to_string / float_to_string 的产物
    ///   - 字符串拼接表达式 / 字符串 `+` 表达式的产物（拼接链中间结果）
    /// 其余（字面量、变量/字段/函数调用返回的字符串）一律 false —— 可能是活变量、
    /// 可能逃逸（存进结构体 / session / 全局）、可能被后续再引用，绝不能释放。
    /// 保守铁律：拿不准就 false（宁泄漏不 over-release）。
    pub(super) fn 生成拼接操作数(
        &mut self,
        node: &AstNode,
    ) -> Result<(inkwell::values::PointerValue<'ctx>, bool), String> {
        // 先据 AST 结构判断「若结果是字符串，是否为新建临时」。
        // QI_ARC=1：改用统一所有权判定（OWNED = 拼接后可释放），覆盖 FFI 返回串 /
        // 用户函数返回串等更多回收点；关闭时保留旧启发式，行为不变。
        let 结构上新建 = if self.弧开() {
            self.表达式拥有字符串(node)
        } else {
            是新建字符串表达式(node)
        };
        let (v, t) = self
            .生成表达式(node)?
            .ok_or_else(|| "字符串拼接操作数无值".to_string())?;
        match t {
            Qi类型::字符串 => Ok((v.into_pointer_value(), 结构上新建)),
            Qi类型::整数 | Qi类型::布尔 | Qi类型::未知 => {
                let iv = v.into_int_value();
                // 布尔/未知先扩展到 i64
                let iv = if iv.get_type().get_bit_width() != 64 {
                    self.builder
                        .build_int_z_extend(iv, self.ctx.i64_type(), "zext")
                        .map_err(|e| e.to_string())?
                } else {
                    iv
                };
                // int_to_string 产物永远是新建临时
                Ok((
                    self.调用返回指针("qi_runtime_int_to_string", &[iv.into()])?,
                    true,
                ))
            }
            Qi类型::浮点数 => Ok((
                self.调用返回指针(
                    "qi_runtime_float_to_string",
                    &[v.into_float_value().into()],
                )?,
                true,
            )),
            _ => Ok((self.生成为字符串_旧(node, v, t)?, 结构上新建)),
        }
    }

    /// 旧路径的错误分支（结构体/函数值/数组/未来/空 → 报错），供 生成拼接操作数 复用。
    fn 生成为字符串_旧(
        &mut self,
        _node: &AstNode,
        v: BasicValueEnum<'ctx>,
        t: Qi类型,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        match t {
            Qi类型::字符串 => Ok(v.into_pointer_value()),
            Qi类型::结构体(_) => Err("结构体不能直接拼接为字符串".to_string()),
            Qi类型::函数值(_) => Err("函数值不能拼接为字符串".to_string()),
            Qi类型::数组(_) => Err("数组不能直接拼接为字符串".to_string()),
            Qi类型::通道(_) => Err("通道不能拼接为字符串".to_string()),
            Qi类型::未来(_) => Err("未来不能直接拼接为字符串（先 等待）".to_string()),
            Qi类型::枚举(_) | Qi类型::装箱枚举(_) => {
                Err("枚举不能直接拼接为字符串（用 匹配 解构）".to_string())
            }
            Qi类型::空 => Err("空值不能拼接".to_string()),
            Qi类型::指针 => Err("C 指针不能直接拼接为字符串（先 作为 整数）".to_string()),
            // 数值类型在 生成拼接操作数 里已转字符串处理，不会走到这里
            Qi类型::整数 | Qi类型::布尔 | Qi类型::浮点数 | Qi类型::未知 => {
                Err("数值应先经 生成拼接操作数 转字符串".to_string())
            }
        }
    }

    /// 生成表达式并保证结果是 i1。
    fn 生成为i1(&mut self, node: &AstNode) -> Result<inkwell::values::IntValue<'ctx>, String> {
        let (v, _t) = self
            .生成表达式(node)?
            .ok_or_else(|| "布尔操作数无值".to_string())?;
        let iv = v.into_int_value();
        if iv.get_type().get_bit_width() == 1 {
            Ok(iv)
        } else {
            // 非零即真
            self.builder
                .build_int_compare(IntPredicate::NE, iv, iv.get_type().const_zero(), "tobool")
                .map_err(|e| e.to_string())
        }
    }

    /// 调 qi_runtime_string_concat 拼两串。
    fn 拼接字符串(
        &mut self,
        l: inkwell::values::PointerValue<'ctx>,
        r: inkwell::values::PointerValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        self.拼接字符串带释放(l, false, r, false)
    }

    /// 拼两串，并在 concat 读完字节后释放新建的临时操作数。
    ///
    /// 安全前提（调用点必须保证）：`l新建`/`r新建` 仅当该指针是本次刚 alloc、
    /// 只作为这次拼接输入、绝不逃逸也绝不被再引用的临时值时才为 true。
    /// concat 内部先把两侧字节完整拷进新 buffer 再返回，所以返回后释放操作数安全。
    fn 拼接字符串带释放(
        &mut self,
        l: inkwell::values::PointerValue<'ctx>,
        l新建: bool,
        r: inkwell::values::PointerValue<'ctx>,
        r新建: bool,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let p = self.调用返回指针("qi_runtime_string_concat", &[l.into(), r.into()])?;
        // concat 已完成，操作数字节不再需要 —— 释放本次新建的临时值。
        if l新建 {
            self.释放临时串(l);
        }
        if r新建 {
            self.释放临时串(r);
        }
        Ok(p.into())
    }

    /// 对一个「确定新建且已消费完毕」的临时字符串发 qi_string_free。
    fn 释放临时串(&mut self, ptr: inkwell::values::PointerValue<'ctx>) {
        if let Some(f) = self.module.get_function("qi_string_free") {
            let _ = self.builder.build_call(f, &[ptr.into()], "");
        }
    }

    /// 调用一个返回 ptr 的运行时函数。
    pub(super) fn 调用返回指针(
        &mut self,
        rtname: &str,
        args: &[BasicMetadataValueEnum<'ctx>],
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let f = self
            .module
            .get_function(rtname)
            .ok_or_else(|| format!("运行时函数未声明: {}", rtname))?;
        let cs = self
            .builder
            .build_call(f, args, "call")
            .map_err(|e| e.to_string())?;
        cs.try_as_basic_value()
            .basic()
            .map(|v| v.into_pointer_value())
            .ok_or_else(|| format!("{} 未返回值", rtname))
    }

    /// 调用一个返回 i32 的运行时函数（如 qi_runtime_string_compare），返回 IntValue。
    fn 调用返回i32(
        &mut self,
        rtname: &str,
        args: &[BasicMetadataValueEnum<'ctx>],
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        let f = self
            .module
            .get_function(rtname)
            .ok_or_else(|| format!("运行时函数未声明: {}", rtname))?;
        let cs = self
            .builder
            .build_call(f, args, "call")
            .map_err(|e| e.to_string())?;
        cs.try_as_basic_value()
            .basic()
            .map(|v| v.into_int_value())
            .ok_or_else(|| format!("{} 未返回值", rtname))
    }

    /// `工具模式(函数名)` —— 编译期从函数签名生成 OpenAI tool 的 parameters JSON schema。
    /// 参数（标量）→ JSON 类型：整数/浮点数→number、字符串→string、布尔→boolean。
    /// 返回该 schema 字符串常量（复用字符串字面量 codegen）。非「工具模式(标识符)」→ Ok(None)。
    pub(super) fn 生成工具模式(
        &mut self,
        call: &crate::parser::ast::FunctionCallExpression,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        use crate::parser::ast::LiteralExpression;
        let 函数名 = match &call.arguments[0] {
            AstNode::标识符表达式(id) => id.name.clone(),
            _ => return Ok(None),
        };
        let 参数名 = self
            .符号
            .函数参数名
            .get(&函数名)
            .cloned()
            .ok_or_else(|| format!("工具模式：找不到函数「{}」的形参信息", 函数名))?;
        let sig = self
            .符号
            .解析函数(&函数名)
            .cloned()
            .ok_or_else(|| format!("工具模式：找不到函数「{}」的签名", 函数名))?;

        let json类型 = |t: &Qi类型| -> Result<&'static str, String> {
            match t {
                Qi类型::整数 => Ok("integer"),
                Qi类型::浮点数 => Ok("number"),
                Qi类型::字符串 => Ok("string"),
                Qi类型::布尔 => Ok("boolean"),
                _ => Err(format!(
                    "工具模式：函数「{}」含非标量参数，v1 仅支持 整数/浮点数/字符串/布尔",
                    函数名
                )),
            }
        };

        let mut 属性 = String::new();
        let mut 必需 = String::new();
        for (i, 名) in 参数名.iter().enumerate() {
            let t = sig.参数.get(i).copied().unwrap_or(Qi类型::字符串);
            if i > 0 {
                属性.push(',');
                必需.push(',');
            }
            属性.push_str(&format!("\"{}\":{{\"type\":\"{}\"}}", 名, json类型(&t)?));
            必需.push_str(&format!("\"{}\"", 名));
        }
        let schema = format!(
            "{{\"type\":\"object\",\"properties\":{{{}}},\"required\":[{}]}}",
            属性, 必需
        );

        // 复用字符串字面量 codegen（得到 RC 字符串值）
        let lit = AstNode::字面量表达式(LiteralExpression {
            value: LiteralValue::字符串(schema),
            span: Default::default(),
        });
        self.生成表达式(&lit)
    }

    /// `工具适配(函数名)` —— 合成一个 `函数(字符串): 字符串` 适配器闭包：
    /// 收到模型的 JSON args → JSON.解码 → 逐参数 JSON.获取X 拆成类型化实参 → 调原函数。
    /// 复用闭包 codegen 生成函数值。要求原函数返回 字符串（工具处理约定）、参数为标量。
    pub(super) fn 生成工具适配(
        &mut self,
        call: &crate::parser::ast::FunctionCallExpression,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        use crate::parser::ast::{
            BasicType, ClosureExpression, FunctionCallExpression, IdentifierExpression,
            LiteralExpression, MethodCallExpression, Parameter, ReturnStatement, TypeNode,
            VariableDeclaration,
        };
        let 函数名 = match &call.arguments[0] {
            AstNode::标识符表达式(id) => id.name.clone(),
            _ => return Ok(None),
        };
        let 参数名 = self
            .符号
            .函数参数名
            .get(&函数名)
            .cloned()
            .ok_or_else(|| format!("工具适配：找不到函数「{}」的形参信息", 函数名))?;
        let sig = self
            .符号
            .解析函数(&函数名)
            .cloned()
            .ok_or_else(|| format!("工具适配：找不到函数「{}」的签名", 函数名))?;
        if sig.返回 != Qi类型::字符串 {
            return Err(format!(
                "工具适配：工具函数「{}」必须返回 字符串（工具结果约定）",
                函数名
            ));
        }

        let id = |name: &str| {
            AstNode::标识符表达式(IdentifierExpression {
                name: name.to_string(),
                span: Default::default(),
            })
        };
        let strlit = |s: &str| {
            AstNode::字面量表达式(LiteralExpression {
                value: LiteralValue::字符串(s.to_string()),
                span: Default::default(),
            })
        };
        let mcall = |obj: AstNode, m: &str, args: Vec<AstNode>| {
            AstNode::方法调用表达式(MethodCallExpression {
                object: Box::new(obj),
                method_name: m.to_string(),
                arguments: args,
                span: Default::default(),
            })
        };

        // 每个参数：J.获取X(__args_j, "参数名")
        let mut 实参: Vec<AstNode> = Vec::new();
        for (i, 名) in 参数名.iter().enumerate() {
            let getter = match sig.参数.get(i).copied().unwrap_or(Qi类型::字符串) {
                Qi类型::整数 => "获取整数",
                Qi类型::浮点数 => "获取浮点数",
                Qi类型::字符串 => "获取字符串",
                Qi类型::布尔 => "获取布尔",
                _ => {
                    return Err(format!(
                        "工具适配：函数「{}」含非标量参数，v1 仅支持标量",
                        函数名
                    ))
                }
            };
            实参.push(mcall(id("JSON"), getter, vec![id("__args_j"), strlit(名)]));
        }

        // 闭包体：变量 __args_j = JSON.解码(参数JSON); 返回 原函数(实参...);
        let 解码语句 = AstNode::变量声明(VariableDeclaration {
            name: "__args_j".to_string(),
            type_annotation: Some(TypeNode::基础类型(BasicType::整数)),
            initializer: Some(Box::new(mcall(id("JSON"), "解码", vec![id("参数JSON")]))),
            is_mutable: false,
            span: Default::default(),
        });
        let 返回语句 = AstNode::返回语句(ReturnStatement {
            value: Some(Box::new(AstNode::函数调用表达式(FunctionCallExpression {
                module_qualifier: None,
                callee: 函数名.clone(),
                type_arguments: vec![],
                arguments: 实参,
                span: Default::default(),
            }))),
            span: Default::default(),
        });

        let 闭包 = ClosureExpression {
            parameters: vec![Parameter {
                name: "参数JSON".to_string(),
                type_annotation: Some(TypeNode::基础类型(BasicType::字符串)),
                default_value: None,
                is_variadic: false,
                span: Default::default(),
            }],
            return_type: Some(TypeNode::基础类型(BasicType::字符串)),
            body: vec![解码语句, 返回语句],
            captures: vec![],
            weak_captures: vec![],
            span: Default::default(),
        };
        self.生成闭包表达式(&闭包).map(Some)
    }

    /// 递归构造「从 JSON 句柄反序列化出结构体」的 AST（结构体实例化表达式）。
    /// 标量→JSON.获取X；无载荷枚举→枚举序号；**嵌套结构体→递归**（句柄换成 JSON.获取对象）。
    /// 句柄 是产出该 JSON 对象的表达式（顶层 = id(临时j)，嵌套 = 获取对象(父句柄,字段)）。
    pub(super) fn 询问建结构体(
        &self,
        idx: u32,
        句柄: crate::parser::ast::AstNode,
    ) -> Result<crate::parser::ast::StructLiteralExpression, String> {
        use crate::parser::ast::{StructFieldValue, StructLiteralExpression};
        let 信息 = self
            .符号
            .结构体
            .get(idx as usize)
            .ok_or_else(|| "询问：结构体未登记".to_string())?
            .clone();
        let mut 字段值: Vec<StructFieldValue> = Vec::new();
        for (名, ty) in 信息.字段名.iter().zip(信息.字段类型.iter()) {
            let 值 = match ty {
                Qi类型::整数 => 询_mcall(询_id("JSON"), "获取整数", vec![句柄.clone(), 询_str(名)]),
                Qi类型::浮点数 => {
                    询_mcall(询_id("JSON"), "获取浮点数", vec![句柄.clone(), 询_str(名)])
                }
                Qi类型::字符串 => {
                    询_mcall(询_id("JSON"), "获取字符串", vec![句柄.clone(), 询_str(名)])
                }
                Qi类型::布尔 => 询_mcall(询_id("JSON"), "获取布尔", vec![句柄.clone(), 询_str(名)]),
                Qi类型::枚举(eidx) => {
                    let ei = self
                        .符号
                        .枚举
                        .get(*eidx as usize)
                        .ok_or_else(|| format!("询问：字段「{}」枚举未登记", 名))?;
                    if ei.装箱 {
                        return Err(format!("询问 v1 不支持带载荷枚举字段「{}」", 名));
                    }
                    let csv: String = ei
                        .变体
                        .iter()
                        .map(|v| v.名字.clone())
                        .collect::<Vec<_>>()
                        .join(",");
                    询_mcall(
                        询_id("JSON"),
                        "枚举序号",
                        vec![
                            询_str(&csv),
                            询_mcall(询_id("JSON"), "获取字符串", vec![句柄.clone(), 询_str(名)]),
                        ],
                    )
                }
                Qi类型::结构体(子idx) => {
                    // 嵌套：递归从子对象句柄建子结构体
                    let 子句柄 =
                        询_mcall(询_id("JSON"), "获取对象", vec![句柄.clone(), 询_str(名)]);
                    crate::parser::ast::AstNode::结构体实例化表达式(
                        self.询问建结构体(*子idx, 子句柄)?,
                    )
                }
                _ => {
                    return Err(format!(
                        "询问：字段「{}」类型不支持（v1：标量/无载荷枚举/嵌套结构体；数组暂不支持）",
                        名
                    ))
                }
            };
            字段值.push(StructFieldValue {
                name: 名.clone(),
                value: Box::new(值),
                span: Default::default(),
            });
        }
        Ok(StructLiteralExpression {
            struct_name: 信息.名字.clone(),
            type_arguments: vec![],
            fields: 字段值,
            span: Default::default(),
        })
    }

    /// 递归生成结构体的 **JSON Schema**（strict 模式用）：标量映射 string/integer/number/boolean，
    /// 无载荷枚举 → {"type":"string","enum":[...变体名]}，嵌套结构体递归；
    /// 每层都 required 全字段 + additionalProperties:false（OpenAI strict 要求）。
    pub(super) fn 询问schema(&self, idx: u32, 深: u32) -> Result<String, String> {
        if 深 > 5 {
            return Err("询问：结构体嵌套超过 5 层（疑似自引用）".to_string());
        }
        let 信息 = self
            .符号
            .结构体
            .get(idx as usize)
            .ok_or_else(|| "询问：结构体未登记".to_string())?
            .clone();
        let mut props: Vec<String> = Vec::new();
        let mut req: Vec<String> = Vec::new();
        for (名, ty) in 信息.字段名.iter().zip(信息.字段类型.iter()) {
            let 子 = match ty {
                Qi类型::整数 => "{\"type\":\"integer\"}".to_string(),
                Qi类型::浮点数 => "{\"type\":\"number\"}".to_string(),
                Qi类型::字符串 => "{\"type\":\"string\"}".to_string(),
                Qi类型::布尔 => "{\"type\":\"boolean\"}".to_string(),
                Qi类型::枚举(eidx) => {
                    let ei = self
                        .符号
                        .枚举
                        .get(*eidx as usize)
                        .ok_or_else(|| format!("询问：字段「{}」枚举未登记", 名))?;
                    let 候选: Vec<String> =
                        ei.变体.iter().map(|v| format!("\"{}\"", v.名字)).collect();
                    format!("{{\"type\":\"string\",\"enum\":[{}]}}", 候选.join(","))
                }
                Qi类型::结构体(子idx) => self.询问schema(*子idx, 深 + 1)?,
                _ => return Err(format!("询问：字段「{}」类型不支持 schema 生成", 名)),
            };
            props.push(format!("\"{}\":{}", 名, 子));
            req.push(format!("\"{}\"", 名));
        }
        Ok(format!(
            "{{\"type\":\"object\",\"properties\":{{{}}},\"required\":[{}],\"additionalProperties\":false}}",
            props.join(","),
            req.join(",")
        ))
    }

    /// 递归生成给模型看的字段结构说明（嵌套结构体展开成「(含：…)」，枚举列出候选）。
    pub(super) fn 询问字段说明(&self, idx: u32, 深: u32) -> Result<String, String> {
        if 深 > 5 {
            return Ok(String::new()); // 防病态递归/自引用类型
        }
        let 信息 = self
            .符号
            .结构体
            .get(idx as usize)
            .ok_or_else(|| "询问：结构体未登记".to_string())?
            .clone();
        let mut 部件: Vec<String> = Vec::new();
        for (名, ty) in 信息.字段名.iter().zip(信息.字段类型.iter()) {
            let 描述 = match ty {
                Qi类型::枚举(eidx) => {
                    let ei = self.符号.枚举.get(*eidx as usize);
                    let 候选 = ei
                        .map(|e| {
                            e.变体
                                .iter()
                                .map(|v| v.名字.clone())
                                .collect::<Vec<_>>()
                                .join("/")
                        })
                        .unwrap_or_default();
                    format!("\"{}\"(必须是: {})", 名, 候选)
                }
                Qi类型::结构体(子idx) => {
                    format!("\"{}\"(对象，含 {})", 名, self.询问字段说明(*子idx, 深 + 1)?)
                }
                _ => format!("\"{}\"", 名),
            };
            部件.push(描述);
        }
        Ok(部件.join("、"))
    }

    /// `询问<结构体>(会话, 提示)` 脱糖 —— 类型定向结构化输出。
    /// 编译期从结构体字段生成结构说明提示 + 开 provider JSON 模式(response_format)，
    /// 调 `大模型.对话`，`JSON.解码` 后**递归**反序列化回强类型结构体（支持嵌套结构体、
    /// 无载荷枚举字段）。全部复用现成 codegen（stdlib 方法调用 + 结构体字面量）。
    /// v1：T 必须是结构体；字段支持 标量/无载荷枚举/嵌套结构体；数组字段暂不支持。
    pub(super) fn 生成询问(
        &mut self,
        call: &crate::parser::ast::FunctionCallExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        use crate::parser::ast::BinaryExpression;
        if call.arguments.len() != 2 {
            return Err("询问<T>(会话, 提示) 需要正好 2 个实参（会话句柄, 提示）".to_string());
        }
        let idx = match self.符号.解析类型(&call.type_arguments[0]) {
            Qi类型::结构体(i) => i,
            _ => return Err("询问<T> 的 T 必须是结构体类型".to_string()),
        };

        // 递归结构说明 → hint（含嵌套结构体展开 + 枚举候选）
        let hint = format!(
            "\n\n严格要求：只输出一个 JSON 对象，含字段：{}。不要任何解释、不要代码块标记。",
            self.询问字段说明(idx, 0)?
        );

        let n = self.询问计数;
        self.询问计数 += 1;
        let 会话名 = format!("__wd_s{}", n);
        let jj名 = format!("__wd_j{}", n);

        // AST 构造小工具
        let id = 询_id;
        let strlit = 询_str;
        let mcall = 询_mcall;

        // 1) 会话落 i64 临时（非 RC，安全；供多处引用不重复求值）
        let (sess_v, _) = self
            .生成表达式(&call.arguments[0])?
            .ok_or_else(|| "询问：会话参数无值".to_string())?;
        let s_ptr = self
            .builder
            .build_alloca(self.ctx.i64_type(), &会话名)
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(s_ptr, sess_v)
            .map_err(|e| e.to_string())?;
        self.变量表.insert(会话名.clone(), (s_ptr, Qi类型::整数));

        // 2) 开结构化输出：编译期生成完整 json_schema **strict** 模式（服务端语法约束，
        // 支持的 provider 如 OpenAI/qwen 严格保证结构；不支持的 provider 会忽略/报错——
        // 文字 hint（上面已拼进提示）仍在，作为兜底。
        let strict = format!(
            "{{\"type\":\"json_schema\",\"json_schema\":{{\"name\":\"resp\",\"strict\":true,\"schema\":{}}}}}",
            self.询问schema(idx, 0)?
        );
        let 开 = mcall(
            id("大模型"),
            "设置配置",
            vec![id(&会话名), strlit("response_format"), strlit(&strict)],
        );
        self.生成表达式(&开)?;

        // 3) j = JSON.解码(大模型.对话(会话, 提示 + hint))  —— 对话只调一次
        let 提示加提示 = AstNode::二元操作表达式(BinaryExpression {
            left: Box::new(call.arguments[1].clone()),
            operator: BinaryOperator::加,
            right: Box::new(strlit(&hint)),
            span: Default::default(),
        });
        let 对话 = mcall(id("大模型"), "对话", vec![id(&会话名), 提示加提示]);
        let 解码 = mcall(id("JSON"), "解码", vec![对话]);
        let (j_v, _) = self
            .生成表达式(&解码)?
            .ok_or_else(|| "询问：解码无值".to_string())?;
        let j_ptr = self
            .builder
            .build_alloca(self.ctx.i64_type(), &jj名)
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(j_ptr, j_v)
            .map_err(|e| e.to_string())?;
        self.变量表.insert(jj名.clone(), (j_ptr, Qi类型::整数));

        // 4) 复原 response_format
        let 关 = mcall(
            id("大模型"),
            "设置配置",
            vec![id(&会话名), strlit("response_format"), strlit("")],
        );
        self.生成表达式(&关)?;

        // 5) 递归反序列化，构造结构体字面量（嵌套结构体自动递归）
        let 字面量 = self.询问建结构体(idx, id(&jj名))?;
        self.生成结构体字面量(&字面量)
    }

    /// `尝试询问::<T>(会话, 提示)` 登记：合成 `__尝试询问$T名(会话,提示): 结果<T,字符串>`
    /// 包装函数（声明原型 + 入待合成队列，体稍后统一生成——成/败 靠函数返回类型标注
    /// 统一为 结果<T,字符串>），返回改写后的普通调用 AST。幂等（同 T 只合成一次）。
    pub(super) fn 登记尝试询问(
        &mut self,
        call: &crate::parser::ast::FunctionCallExpression,
    ) -> Result<crate::parser::ast::FunctionCallExpression, String> {
        use crate::parser::ast::{
            AstNode, BasicType, BinaryExpression, ExpressionStatement, FunctionCallExpression,
            FunctionDeclaration, IfStatement, Parameter, ResultType, ReturnStatement, TypeNode,
            VariableDeclaration, Visibility,
        };
        if call.arguments.len() != 2 {
            return Err("尝试询问<T>(会话, 提示) 需要正好 2 个实参".to_string());
        }
        let idx = match self.符号.解析类型(&call.type_arguments[0]) {
            Qi类型::结构体(i) => i,
            _ => return Err("尝试询问<T> 的 T 必须是结构体类型".to_string()),
        };
        let t名 = self
            .符号
            .结构体
            .get(idx as usize)
            .map(|s| s.名字.clone())
            .ok_or_else(|| "尝试询问：结构体未登记".to_string())?;
        let 函数名 = format!("__尝试询问${}", t名);

        // 改写后的调用（无论是否已合成都一样）
        let 改写 = FunctionCallExpression {
            module_qualifier: None,
            callee: 函数名.clone(),
            type_arguments: vec![],
            arguments: call.arguments.clone(),
            span: call.span.clone(),
        };
        if self.已合成询问.contains(&函数名) {
            return Ok(改写);
        }

        // ── 构造包装函数 AST ──
        let strict = format!(
            "{{\"type\":\"json_schema\",\"json_schema\":{{\"name\":\"resp\",\"strict\":true,\"schema\":{}}}}}",
            self.询问schema(idx, 0)?
        );
        let hint = format!(
            "\n\n严格要求：只输出一个 JSON 对象，含字段：{}。不要任何解释、不要代码块标记。",
            self.询问字段说明(idx, 0)?
        );
        let 语句 = |e: AstNode| {
            AstNode::表达式语句(ExpressionStatement {
                expression: Box::new(e),
                span: Default::default(),
            })
        };
        let 参 = |n: &str, t: BasicType| Parameter {
            name: n.to_string(),
            type_annotation: Some(TypeNode::基础类型(t)),
            default_value: None,
            is_variadic: false,
            span: Default::default(),
        };
        // body
        let 开 = 语句(询_mcall(
            询_id("大模型"),
            "设置配置",
            vec![询_id("会话"), 询_str("response_format"), 询_str(&strict)],
        ));
        let 取回复 = AstNode::变量声明(VariableDeclaration {
            name: "__r".to_string(),
            type_annotation: Some(TypeNode::基础类型(BasicType::字符串)),
            initializer: Some(Box::new(询_mcall(
                询_id("大模型"),
                "对话",
                vec![
                    询_id("会话"),
                    AstNode::二元操作表达式(BinaryExpression {
                        left: Box::new(询_id("提示")),
                        operator: BinaryOperator::加,
                        right: Box::new(询_str(&hint)),
                        span: Default::default(),
                    }),
                ],
            ))),
            is_mutable: false,
            span: Default::default(),
        });
        let 关 = 语句(询_mcall(
            询_id("大模型"),
            "设置配置",
            vec![询_id("会话"), 询_str("response_format"), 询_str("")],
        ));
        let 解码 = AstNode::变量声明(VariableDeclaration {
            name: "__j".to_string(),
            type_annotation: Some(TypeNode::基础类型(BasicType::整数)),
            initializer: Some(Box::new(询_mcall(询_id("JSON"), "解码", vec![询_id("__r")]))),
            is_mutable: false,
            span: Default::default(),
        });
        let 构造子 = |名: &str, 实参: AstNode| {
            AstNode::函数调用表达式(FunctionCallExpression {
                module_qualifier: None,
                callee: 名.to_string(),
                type_arguments: vec![],
                arguments: vec![实参],
                span: Default::default(),
            })
        };
        // 如果 (__j < 1) { 返回 败(__r); }
        let 失败分支 = AstNode::如果语句(IfStatement {
            condition: Box::new(AstNode::二元操作表达式(BinaryExpression {
                left: Box::new(询_id("__j")),
                operator: BinaryOperator::小于,
                right: Box::new(AstNode::字面量表达式(
                    crate::parser::ast::LiteralExpression {
                        value: LiteralValue::整数(1),
                        span: Default::default(),
                    },
                )),
                span: Default::default(),
            })),
            then_branch: vec![AstNode::返回语句(ReturnStatement {
                value: Some(Box::new(构造子("败", 询_id("__r")))),
                span: Default::default(),
            })],
            else_branch: None,
            span: Default::default(),
        });
        // 返回 成(T{…递归反序列化…});
        let 成功返回 = AstNode::返回语句(ReturnStatement {
            value: Some(Box::new(构造子(
                "成",
                AstNode::结构体实例化表达式(self.询问建结构体(idx, 询_id("__j"))?),
            ))),
            span: Default::default(),
        });

        let f = FunctionDeclaration {
            name: 函数名.clone(),
            type_params: vec![],
            parameters: vec![参("会话", BasicType::整数), 参("提示", BasicType::字符串)],
            return_type: Some(TypeNode::结果类型(ResultType {
                ok_type: Box::new(TypeNode::自定义类型(t名)),
                err_type: Box::new(TypeNode::基础类型(BasicType::字符串)),
            })),
            body: vec![开, 取回复, 关, 解码, 失败分支, 成功返回],
            visibility: Visibility::私有,
            is_inline: false,
            is_async: false,
            span: Default::default(),
        };
        // 声明原型（签名进符号表 + LLVM 原型，供本次调用点解析）；体入队稍后统一生成
        self.声明函数原型(&f)?;
        self.待合成询问.push((f, self.当前包.clone()));
        self.已合成询问.insert(函数名);
        Ok(改写)
    }

    /// 生成所有待合成 `尝试询问` 包装函数的函数体（循环到清空；体内的结构体字面量
    /// 不会再产生新的 询问，但保持与闭包/泛型同款的循环调用习惯）。
    pub(super) fn 合成待处理询问(&mut self) -> Result<(), String> {
        while let Some((f, 包)) = self.待合成询问.pop() {
            let 原包 = self.当前包.clone();
            self.设当前包(包);
            let r = self.生成函数体(&f);
            self.设当前包(原包);
            r?;
        }
        Ok(())
    }

    /// 解析用户函数调用目标：当前包 → destructure 导入来源包 → 全局唯一定义包 → 裸符号。
    /// 多包同名且无法定位 → None（**绝不随机挑包** —— 符号与签名必须来自同一个包，
    /// 否则实参适配/返回类型全错）。调用处据 函数候选包 报歧义错误。
    pub(super) fn 尝试解析用户函数(
        &self,
        name: &str,
        实参数: usize,
    ) -> Option<(
        inkwell::values::FunctionValue<'ctx>,
        Option<super::类型检查::函数签名>,
    )> {
        // 先按实参个数选定重载签名（单一定义 → 直接它；多重载 → 元数精确匹配）。
        // 元数（= 选中签名的形参个数）用于 mangle —— 与声明侧一致，重载各得互异符号。
        let sig = self.符号.解析重载(name, 实参数).cloned();
        let 元数 = sig.as_ref().map(|s| s.参数.len()).unwrap_or(实参数);

        // 当前包符号
        let 当前 = super::包内符号名(self.当前包.as_deref(), name, 元数);
        if let Some(f) = self.module.get_function(&当前) {
            return Some((f, sig));
        }
        // destructure 导入指明的来源包
        if let Some(src) = self.符号.导入来源(name) {
            let sym = super::包内符号名(Some(src), name, 元数);
            if let Some(f) = self.module.get_function(&sym) {
                return Some((f, sig));
            }
        }
        // 全局唯一定义包（多包同名 → 不猜，返回 None 交调用点报歧义）
        if let [唯一] = self.符号.函数候选包(name).as_slice() {
            let sym = super::包内符号名(Some(唯一), name, 元数);
            if let Some(f) = self.module.get_function(&sym) {
                return Some((f, sig));
            }
        }
        // 再回退：无包符号（无包程序 / 单文件）
        let 裸 = super::包内符号名(None, name, 元数);
        if let Some(f) = self.module.get_function(&裸) {
            return Some((f, sig.or_else(|| self.符号.函数.get(name).cloned())));
        }
        None
    }

    /// 用户函数调用 / 内建函数调用 / 打印调用。
    fn 生成函数调用(
        &mut self,
        call: &crate::parser::ast::FunctionCallExpression,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        // 类型定向结构化输出：`询问<结构体>(会话, 提示)` —— 编译期从结构体派生
        // JSON schema、发 response_format、把回复反序列化回强类型结构体。
        if call.callee == "询问" && !call.type_arguments.is_empty() {
            return self.生成询问(call).map(Some);
        }

        // 可失败版：`尝试询问::<T>(会话, 提示)` 返回 结果<T, 字符串> —— 解析失败走 败(原始回复)，
        // 成功走 成(T)。合成一个带返回类型标注的包装函数（成/败 靠标注统一类型），
        // 体在所有函数后统一生成（同 待合成闭包 机制），调用点改写成普通调用。
        if call.callee == "尝试询问" && !call.type_arguments.is_empty() {
            let 改写 = self.登记尝试询问(call)?;
            return self.生成函数调用(&改写);
        }

        // 工具 schema 自动生成：`工具模式(某函数)` —— 编译期从函数签名（参数名+类型）
        // 生成 OpenAI tool 的 parameters JSON schema 字符串，免手写、免与函数漂移。
        if call.callee == "工具模式" && call.arguments.len() == 1 {
            if let Some(v) = self.生成工具模式(call)? {
                return Ok(Some(v));
            }
        }

        // 工具参数自动反序列化：`工具适配(某函数)` —— 生成一个 函数(字符串):字符串 适配器，
        // 把模型给的 JSON args 拆成类型化实参再调该函数。省去每个工具手写解析 + 消除参数错配。
        if call.callee == "工具适配" && call.arguments.len() == 1 {
            if let Some(v) = self.生成工具适配(call)? {
                return Ok(Some(v));
            }
        }

        // 间接调用：callee 是一个函数值变量（如参数 f: 函数(整数):整数）
        if let Some(v) = self.尝试间接调用(&call.callee, &call.arguments)? {
            return Ok(v);
        }

        // 打印
        if let Some(v) = self.生成打印方法(&call.callee, &call.arguments)? {
            return Ok(v);
        }

        // 内建类型转换
        if let Some(v) = self.生成内建调用(&call.callee, &call.arguments)? {
            return Ok(Some(v));
        }

        // 同步 / 定时器内建（创建等待组 / 互斥锁 / 定时器 等无限定并发原语）
        if let Some(v) = self.生成同步内建(&call.callee, &call.arguments)? {
            return Ok(v);
        }

        // 外部 C 函数（`外部 "库" { ... }`）：走 C ABI 专用降级（不 mangle、不碰 ARC）。
        if self.符号.外部函数.contains(&call.callee) {
            return self.生成外部调用(call);
        }

        // 用户泛型函数：定型（显式类型实参 / 实参推断）→ 单态实例化 → 以实例名调用
        if let Some(v) = self.生成泛型函数调用(call)? {
            return Ok(v);
        }

        // 用户函数（跨包同名消歧：当前包符号优先，否则回退裸/任意包符号）；
        // 找不到用户函数时，回退无模块限定的标准库函数（如 MD5哈希(x)、创建等待组()）。
        let (f, sig) = match self.尝试解析用户函数(&call.callee, call.arguments.len()) {
            Some(x) => x,
            None => {
                // 重载存在但没有匹配实参个数的那个 → 清晰报「无匹配重载」
                if let Some(集) = self.符号.重载集(&call.callee) {
                    if 集.len() > 1 {
                        let mut 元数集: Vec<String> =
                            集.iter().map(|s| s.参数.len().to_string()).collect();
                        元数集.sort();
                        return Err(format!(
                            "函数「{}」有重载，但没有接收 {} 个实参的版本（可选形参个数：{}）。",
                            call.callee,
                            call.arguments.len(),
                            元数集.join(" / ")
                        ));
                    }
                }
                // 多包同名的用户函数：先报歧义 —— 绝不静默落到同名 stdlib 函数
                let 候选 = self.符号.函数候选包(&call.callee);
                if 候选.len() > 1 {
                    return Err(format!(
                        "函数名 {} 歧义：定义于多个包（{}）。请用 `导入 包::{{{}}}` 指明来源",
                        call.callee,
                        候选.join("、"),
                        call.callee
                    ));
                }
                if let Some(v) = self.尝试无限定标准库(&call.callee, &call.arguments)? {
                    return Ok(v);
                }
                return Err(format!("未定义的函数: {}", call.callee));
            }
        };

        // 少传实参时用默认参数值补齐（形参个数由签名决定）；
        // 变参函数：固定形参之外的尾实参打包成数组字面量，与签名末位 数组(T) 对齐，
        // 保证 call 实参个数 == 函数类型形参个数（否则 LLVM 模块校验失败）。
        let 形参数 = sig
            .as_ref()
            .map(|s| s.参数.len())
            .unwrap_or(call.arguments.len());
        let 变参 = self.符号.函数变参.contains(&call.callee);
        let 固定 = if 变参 {
            形参数.saturating_sub(1)
        } else {
            形参数
        };
        let mut 实参: Vec<AstNode> = call.arguments.clone();
        let 尾实参: Vec<AstNode> = if 变参 && 实参.len() > 固定 {
            实参.split_off(固定)
        } else {
            Vec::new()
        };
        if 实参.len() < 固定 {
            if let Some(defs) = self.符号.函数默认值.get(&call.callee).cloned() {
                for i in 实参.len()..固定 {
                    if let Some(Some(d)) = defs.get(i) {
                        实参.push(d.clone());
                    }
                }
            }
        }
        if 变参 {
            实参.push(AstNode::数组字面量表达式(
                crate::parser::ast::ArrayLiteralExpression {
                    elements: 尾实参,
                    span: call.span,
                },
            ));
        }

        let mut args: Vec<BasicMetadataValueEnum> = Vec::new();
        let mut 弧待释放: Vec<(BasicValueEnum, Qi类型)> = Vec::new();
        for (i, a) in 实参.iter().enumerate() {
            // 规则③：实参位以形参声明类型为构造子期望（选项/结果 定型）
            let 形参期望 = sig.as_ref().and_then(|s| s.参数.get(i)).copied();
            let (v, vt) = self
                .生成带期望(a, 形参期望)?
                .ok_or_else(|| "函数实参无值".to_string())?;
            // ARC：实参是借用传递 —— OWNED RC 临时（拼接串/结构体字面量/
            // 变参打包数组）在调用结束后释放（被调方需要保留会自行 retain）。
            // 结构体/数组额外要求形参声明类型也是 RC（否则被调方按整数句柄收，
            // 不套 retain 纪律，可能私藏指针 —— 保守不释放，宁泄漏）。
            let 形参rc = sig
                .as_ref()
                .and_then(|s| s.参数.get(i))
                .map(|pt| super::所有权::是RC类型(*pt))
                .unwrap_or(false);
            let 可释放 = match vt {
                Qi类型::字符串 => self.表达式拥有字符串(a),
                Qi类型::结构体(_) | Qi类型::数组(_) | Qi类型::函数值(_) | Qi类型::装箱枚举(_) => {
                    形参rc && self.表达式拥有RC(a, vt)
                }
                _ => false,
            };
            if self.弧开() && v.is_pointer_value() && 可释放 {
                弧待释放.push((v, vt));
            }
            // 按形参声明类型协调（int↔float、ptr↔i64 句柄等），保证与函数原型一致
            let v = match sig.as_ref().and_then(|s| s.参数.get(i)) {
                Some(pt) => self.协调到类型(v, vt, *pt)?,
                None => v,
            };
            args.push(v.into());
        }

        let ret = sig.as_ref().map(|s| s.返回).unwrap_or(Qi类型::整数);
        let cs = self
            .builder
            .build_call(f, &args, "call")
            .map_err(|e| e.to_string())?;
        for (v, vt) in 弧待释放 {
            self.弧release任意(v, vt);
        }
        match cs.try_as_basic_value().basic() {
            Some(v) => Ok(Some((v, ret))),
            None => Ok(None),
        }
    }

    /// 内建类型转换函数（整数转字符串 等）。返回 None 表示不是内建。
    fn 生成内建调用(
        &mut self,
        callee: &str,
        arguments: &[AstNode],
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let (rtname, ret) = match callee {
            "整数转字符串" | "int_to_string" => ("qi_runtime_int_to_string", Qi类型::字符串),
            "浮点数转字符串" | "float_to_string" => {
                ("qi_runtime_float_to_string", Qi类型::字符串)
            }
            "字符串转整数" | "string_to_int" => ("qi_runtime_string_to_int", Qi类型::整数),
            "字符串转浮点数" | "string_to_float" => {
                ("qi_runtime_string_to_float", Qi类型::浮点数)
            }
            "整数转浮点数" | "int_to_float" => ("qi_runtime_int_to_float", Qi类型::浮点数),
            "浮点数转整数" | "float_to_int" => ("qi_runtime_float_to_int", Qi类型::整数),
            _ => return Ok(None),
        };
        let f = self
            .module
            .get_function(rtname)
            .ok_or_else(|| format!("运行时函数未声明: {}", rtname))?;
        let mut args: Vec<BasicMetadataValueEnum> = Vec::new();
        let mut 弧待释放: Vec<(BasicValueEnum, Qi类型, &AstNode)> = Vec::new();
        for a in arguments {
            let (v, t) = self
                .生成表达式(a)?
                .ok_or_else(|| "内建实参无值".to_string())?;
            if t == Qi类型::字符串 {
                弧待释放.push((v, t, a)); // 弧消费后释放 内部再判 OWNED
            }
            args.push(v.into());
        }
        let cs = self
            .builder
            .build_call(f, &args, "call")
            .map_err(|e| e.to_string())?;
        // ARC：如 字符串转整数(拼接结果) —— 转换读完即释 OWNED 实参
        for (v, t, a) in 弧待释放 {
            self.弧消费后释放(v, t, a);
        }
        let v = cs
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| format!("{} 未返回值", rtname))?;
        Ok(Some((v, ret)))
    }

    /// 打印族（打印/打印行）—— 据实参类型选 int/float/bool/字符串 运行时。
    /// 返回 Ok(None) 表示 method 不是打印；Ok(Some(None)) 表示打印成功（void）。
    fn 生成打印方法(
        &mut self,
        method: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        let 换行 = match method {
            "打印行" | "println" => true,
            "打印" | "print" | "printf" => false,
            _ => return Ok(None),
        };

        for a in arguments {
            let (v, t) = self
                .生成表达式(a)?
                .ok_or_else(|| "打印实参无值".to_string())?;
            let (rtname, arg): (&str, BasicMetadataValueEnum) = match t {
                Qi类型::整数 | Qi类型::未知 => (
                    if 换行 {
                        "qi_runtime_println_int"
                    } else {
                        "qi_runtime_print_int"
                    },
                    v.into(),
                ),
                Qi类型::浮点数 => (
                    if 换行 {
                        "qi_runtime_println_float"
                    } else {
                        "qi_runtime_print_float"
                    },
                    v.into(),
                ),
                Qi类型::布尔 => {
                    // 运行时 bool 版收 i32，i1 先扩展
                    let i32v = self
                        .builder
                        .build_int_z_extend(v.into_int_value(), self.ctx.i32_type(), "b2i")
                        .map_err(|e| e.to_string())?;
                    (
                        if 换行 {
                            "qi_runtime_println_bool"
                        } else {
                            "qi_runtime_print_bool"
                        },
                        i32v.into(),
                    )
                }
                Qi类型::字符串 => (
                    if 换行 {
                        "qi_runtime_println"
                    } else {
                        "qi_runtime_print"
                    },
                    v.into(),
                ),
                // 无载荷枚举 = i64 tag，直接按整数打印（变体序号）
                Qi类型::枚举(_) => (
                    if 换行 {
                        "qi_runtime_println_int"
                    } else {
                        "qi_runtime_print_int"
                    },
                    v.into(),
                ),
                // C 指针：按整数地址打印（ptr→i64），便于观察句柄是否为空/有效
                Qi类型::指针 => {
                    let iv = self
                        .builder
                        .build_ptr_to_int(v.into_pointer_value(), self.ctx.i64_type(), "p2i")
                        .map_err(|e| e.to_string())?;
                    (
                        if 换行 {
                            "qi_runtime_println_int"
                        } else {
                            "qi_runtime_print_int"
                        },
                        iv.into(),
                    )
                }
                Qi类型::装箱枚举(_) => {
                    return Err("不能直接打印带载荷枚举（用 匹配 解构后打印字段）".to_string())
                }
                Qi类型::结构体(_) => return Err("不能直接打印结构体".to_string()),
                Qi类型::函数值(_) => return Err("不能直接打印函数值".to_string()),
                Qi类型::数组(_) => return Err("不能直接打印数组".to_string()),
                Qi类型::通道(_) => return Err("不能直接打印通道".to_string()),
                Qi类型::未来(_) => return Err("不能直接打印未来（先 等待）".to_string()),
                Qi类型::空 => return Err("不能打印空值".to_string()),
            };
            let f = self
                .module
                .get_function(rtname)
                .ok_or_else(|| format!("运行时函数未声明: {}", rtname))?;
            self.builder
                .build_call(f, &[arg], "call")
                .map_err(|e| e.to_string())?;
            // ARC：打印读完即释 —— 典型 打印行(整数转字符串(x)) / 打印行(a + b)
            self.弧消费后释放(v, t, a);
        }
        Ok(Some(None))
    }
}

/// 该表达式若结果为字符串，是否是「本次新建、绝不逃逸、只被消费一次」的临时值？
///
/// 保守：只认拼接产物（字符串拼接表达式 / 字符串 `+`）。这类中间结果必然是
/// qi_runtime_string_concat 刚 alloc 的堆串，且在 AST 里只作为上层拼接的输入出现，
/// 消费完即死，释放绝对安全。
///
/// 其它一律 false（宁泄漏不 over-release）：
///   - 字面量：immortal header 全局常量，free 是 no-op，但也没必要发
///   - 标识符/字段访问：活变量或可能逃逸的值
///   - 函数调用返回：所有权语义不明（可能被缓存 / 存进结构体 / 是借引用）
///
/// 数值 → int/float_to_string 的新建性不在这里判（在 生成拼接操作数 里直接标 true，
/// 因为那两个函数的产物一定是新建临时）。
fn 是新建字符串表达式(node: &AstNode) -> bool {
    match node {
        AstNode::字符串连接表达式(_) => true,
        AstNode::二元操作表达式(b) => b.operator == crate::parser::ast::BinaryOperator::加,
        _ => false,
    }
}
