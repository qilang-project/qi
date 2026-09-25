//! 字符串插值 `"…${表达式}…"`（格式字符串）与 `填充模板` 的代码生成。
//!
//! 以前脱糖成左结合 `+` 链：每个 `+` 调一次 qi_runtime_string_concat（分配一次），
//! 每个数值洞先 int/float_to_string（再分配一次），N 段插值要 2N-1 次分配。
//! `"key${i}"` 三对 malloc+free，而 C/Rust 一次。
//!
//! 现在降成**一次** `qi_string_format_parts(段数组, 段数)`：
//!   - 段数组在函数 entry 块 alloca（`[2N x i64]`，与运行时的 `[N x {kind, value}]`
//!     同布局），循环里不吃栈；
//!   - 字面段 → kind 0 + 不朽常量指针；字符串洞 → kind 0 + 指针；
//!     整数/布尔/未知 → kind 1 + i64（零扩展）；浮点 → kind 2 + f64 位模式。
//!     数值洞不再先转串，运行时直接格式化进结果缓冲；
//!   - 结果是 rc=1 的新串（总长 0 时是不朽空串，free 为 no-op）—— 与 所有权.rs
//!     「含洞的格式字符串 → OWNED」的判定一致；
//!   - 字符串洞里「本次新建」的临时串（判定与 `生成拼接操作数` 同一口径）在调用
//!     返回后释放：运行时已把字节拷进结果，释放安全。
//!
//! 所有洞先求值成 SSA 值，最后才写段数组并调用 —— 洞里再嵌插值（各自有自己的
//! 段数组）或挂起点（协程帧自动溢出）都不会踩到这里的数组。

use super::后端;
use super::类型::Qi类型;
use crate::parser::ast::{AstNode, FormatStringExpression, FormatStringPart, LiteralValue};
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

/// 与 qi-runtime `str_format::FMT_PART_*` 一一对应
const PART_STR: u64 = 0;
const PART_INT: u64 = 1;
const PART_FLOAT: u64 = 2;

/// 插值的一段：字面文本或洞里的表达式。
pub(super) enum InterpPart<'a> {
    Text(&'a str),
    Expr(AstNode),
}

fn string_literal(s: &str) -> AstNode {
    AstNode::字面量表达式(crate::parser::ast::LiteralExpression {
        value: LiteralValue::字符串(s.to_string()),
        span: Default::default(),
    })
}

impl<'ctx> 后端<'ctx> {
    /// `f"文本{表达式}更多{另一个}"` / `"文本${表达式}"` 格式字符串。
    pub(super) fn gen_format_string(
        &mut self,
        fs: &FormatStringExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        // parse_format_string 解析失败的洞会退回「名字是洞原文」的裸标识符 ——
        // 名字含非标识符字符（调用/运算/索引…）时用 LALRPOP 的 pub Expr 入口重解析，
        // 解析不了就在这里报「不是合法表达式」。
        let needs_reparse = |name: &str| name.chars().any(|c| !c.is_alphanumeric() && c != '_');
        let mut parts: Vec<InterpPart> = Vec::with_capacity(fs.parts.len());
        for p in fs.parts.iter() {
            parts.push(match p {
                FormatStringPart::文本(t) => InterpPart::Text(t),
                FormatStringPart::表达式 { expr, .. } => match &**expr {
                    AstNode::标识符表达式(id) if needs_reparse(&id.name) => {
                        InterpPart::Expr(crate::parser::ExprParser::new().parse(&id.name).map_err(
                            |e| format!("格式字符串插值 `{{{}}}` 不是合法表达式: {:?}", id.name, e),
                        )?)
                    }
                    other => InterpPart::Expr(other.clone()),
                },
            });
        }
        self.gen_interpolation(&parts)
    }

    /// 段序列 → 字符串。没有洞 → 各字面段合成一个不朽字面量（BORROWED）；
    /// 有洞 → 一次 qi_string_format_parts（OWNED 新串）。
    pub(super) fn gen_interpolation(
        &mut self,
        parts: &[InterpPart],
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        if !parts.iter().any(|p| matches!(p, InterpPart::Expr(_))) {
            let text: String = parts
                .iter()
                .map(|p| match p {
                    InterpPart::Text(t) => *t,
                    InterpPart::Expr(_) => "",
                })
                .collect();
            return self
                .生成表达式(&string_literal(&text))?
                .ok_or_else(|| "格式字符串：字面量无值".to_string());
        }

        let i64t = self.ctx.i64_type();
        // 1) 逐段求值成 (kind, i64 值)，记下要在调用后释放的新建临时串
        let mut slots: Vec<(u64, IntValue<'ctx>)> = Vec::with_capacity(parts.len());
        let mut temps: Vec<PointerValue<'ctx>> = Vec::new();
        for part in parts {
            match part {
                InterpPart::Text(t) => {
                    if t.is_empty() {
                        continue;
                    }
                    let p = self.emit_immortal_string(t, "qi.str")?;
                    slots.push((PART_STR, self.ptr_as_i64(p)?));
                }
                InterpPart::Expr(node) => {
                    let (kind, value, temp) = self.gen_interp_hole(node)?;
                    slots.push((kind, value));
                    temps.extend(temp);
                }
            }
        }

        // 2) entry 块里的段数组：[2N x i64]，偶数位 kind、奇数位值
        let n = slots.len() as u32;
        let array_type = i64t.array_type(2 * n);
        let array = self.入口块alloca(array_type.into(), "fmt.parts")?;
        for (i, (kind, value)) in slots.iter().enumerate() {
            let kind_slot = self.i64_slot(array, array_type, 2 * i as u64)?;
            self.builder
                .build_store(kind_slot, i64t.const_int(*kind, false))
                .map_err(|e| e.to_string())?;
            let value_slot = self.i64_slot(array, array_type, 2 * i as u64 + 1)?;
            self.builder
                .build_store(value_slot, *value)
                .map_err(|e| e.to_string())?;
        }

        // 3) 一次分配拼完；随后释放本次新建的字符串临时值
        let result = self.调用返回指针(
            "qi_string_format_parts",
            &[array.into(), i64t.const_int(n as u64, false).into()],
        )?;
        if !temps.is_empty() {
            let free = self
                .module
                .get_function("qi_string_free")
                .ok_or_else(|| "运行时函数未声明: qi_string_free".to_string())?;
            for t in temps {
                self.builder
                    .build_call(free, &[t.into()], "")
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok((result.into(), Qi类型::字符串))
    }

    /// 一个洞 → (kind, 值, 调用后要释放的临时串)。
    ///
    /// 「新建临时」的判定与 `生成拼接操作数` 完全同口径：ARC 开时用统一所有权判定
    /// （OWNED 即可在消费后释放），关时用老的结构启发式。数值洞不再产生临时串。
    fn gen_interp_hole(
        &mut self,
        node: &AstNode,
    ) -> Result<(u64, IntValue<'ctx>, Option<PointerValue<'ctx>>), String> {
        let owned = if self.弧开() {
            self.表达式拥有字符串(node)
        } else {
            super::表达式::是新建字符串表达式(node)
        };
        let (v, t) = self
            .生成表达式(node)?
            .ok_or_else(|| "字符串插值的洞无值".to_string())?;
        match t {
            Qi类型::字符串 => {
                let p = v.into_pointer_value();
                Ok((PART_STR, self.ptr_as_i64(p)?, owned.then_some(p)))
            }
            Qi类型::整数 | Qi类型::布尔 | Qi类型::未知 => {
                if !v.is_int_value() {
                    return Err("字符串插值：洞的值类型无法确定，无法转成字符串".to_string());
                }
                let iv = self.整数加宽到i64(v.into_int_value())?;
                Ok((PART_INT, iv, None))
            }
            Qi类型::浮点数 => {
                let bits = self
                    .builder
                    .build_bit_cast(v.into_float_value(), self.ctx.i64_type(), "f2bits")
                    .map_err(|e| e.to_string())?
                    .into_int_value();
                Ok((PART_FLOAT, bits, None))
            }
            // 结构体 / 数组 / 函数值 / 空 … 报与 `+` 拼接同样的错
            _ => {
                self.生成为字符串_旧(node, v, t)?;
                Err("字符串插值：不支持的洞类型".to_string())
            }
        }
    }

    fn ptr_as_i64(&mut self, p: PointerValue<'ctx>) -> Result<IntValue<'ctx>, String> {
        self.builder
            .build_ptr_to_int(p, self.ctx.i64_type(), "p2i")
            .map_err(|e| e.to_string())
    }

    fn i64_slot(
        &mut self,
        array: PointerValue<'ctx>,
        array_type: inkwell::types::ArrayType<'ctx>,
        index: u64,
    ) -> Result<PointerValue<'ctx>, String> {
        let i64t = self.ctx.i64_type();
        unsafe {
            self.builder
                .build_in_bounds_gep(
                    array_type,
                    array,
                    &[i64t.const_zero(), i64t.const_int(index, false)],
                    "fmt.slot",
                )
                .map_err(|e| e.to_string())
        }
    }
}
