//! 表达式降级 —— AST 表达式 → 类型化 LLVM 值。
//!
//! 每个表达式先经类型检查器定型，再据类型选对应 LLVM 指令。
//! 返回 (值, Qi类型)，让调用点无需二次猜测。

use super::类型::Qi类型;
use super::类型检查::推断表达式类型;
use super::后端;
use crate::parser::ast::{AstNode, BinaryOperator, LiteralValue, UnaryOperator};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::{FloatPredicate, IntPredicate};

impl<'ctx> 后端<'ctx> {
    /// 生成表达式，返回其 LLVM 值与 Qi 类型。`空`（如无返回值调用）返回 None。
    pub(super) fn 生成表达式(
        &mut self,
        node: &AstNode,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
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
                let l = self.生成为字符串(&sc.left)?;
                let r = self.生成为字符串(&sc.right)?;
                Ok(Some((self.拼接字符串(l, r)?, Qi类型::字符串)))
            }

            AstNode::函数调用表达式(call) => self.生成函数调用(call),

            AstNode::结构体实例化表达式(lit) => self.生成结构体字面量(lit).map(Some),

            AstNode::字段访问表达式(fa) => self.生成字段访问(&fa.object, &fa.field).map(Some),

            AstNode::方法调用表达式(mc) => {
                // 1) 结构体方法（接收者是结构体，含链式）
                if let Some(v) = self.生成方法调用(&mc.object, &mc.method_name, &mc.arguments)? {
                    return Ok(v);
                }
                // 2) 标准库分发（接收者是模块名/别名，如 IO.打印行 / 字符串::字节长度）
                if let Some(v) =
                    self.尝试标准库调用(&mc.object, &mc.method_name, &mc.arguments)?
                {
                    return Ok(v);
                }
                // 3) 内建打印归一（IO 未导入时的 打印行 兜底）
                if let Some(v) = self.生成打印方法(&mc.method_name, &mc.arguments)? {
                    return Ok(v);
                }
                Err(format!("无法解析方法调用: {}", mc.method_name))
            }

            AstNode::赋值表达式(a) => {
                match a.target.as_ref() {
                    AstNode::标识符表达式(id) => {
                        let (v, t) = self
                            .生成表达式(&a.value)?
                            .ok_or_else(|| "赋值右值无值".to_string())?;
                        let (ptr, _) = self
                            .变量表
                            .get(&id.name)
                            .cloned()
                            .ok_or_else(|| format!("赋值给未声明变量: {}", id.name))?;
                        self.builder
                            .build_store(ptr, v)
                            .map_err(|e| e.to_string())?;
                        Ok(Some((v, t)))
                    }
                    // 字段赋值 x.字段 = 值
                    AstNode::字段访问表达式(fa) => {
                        self.生成字段赋值(&fa.object, &fa.field, &a.value).map(Some)
                    }
                    _ => Err("暂只支持标识符 / 字段赋值".to_string()),
                }
            }

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
            LiteralValue::浮点数(f) => (
                self.ctx.f64_type().const_float(*f).into(),
                Qi类型::浮点数,
            ),
            LiteralValue::布尔(b) => (
                self.ctx.bool_type().const_int(*b as u64, false).into(),
                Qi类型::布尔,
            ),
            LiteralValue::字符(c) => (
                self.ctx.i64_type().const_int(*c as u64, false).into(),
                Qi类型::整数,
            ),
            LiteralValue::字符串(s) => {
                let g = self
                    .builder
                    .build_global_string_ptr(s, "str")
                    .map_err(|e| e.to_string())?;
                (g.as_pointer_value().into(), Qi类型::字符串)
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
                let l = self.生成为字符串(&b.left)?;
                let r = self.生成为字符串(&b.right)?;
                return Ok((self.拼接字符串(l, r)?, Qi类型::字符串));
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

        let (lv, lt) = self
            .生成表达式(&b.left)?
            .ok_or_else(|| "二元左操作数无值".to_string())?;
        let (rv, rt) = self
            .生成表达式(&b.right)?
            .ok_or_else(|| "二元右操作数无值".to_string())?;

        let 用浮点 = lt.是浮点() || rt.是浮点();

        if 用浮点 {
            let lf = self.转浮点(lv, lt)?;
            let rf = self.转浮点(rv, rt)?;
            let v: BasicValueEnum = match b.operator {
                加 => self.builder.build_float_add(lf, rf, "fadd").map_err(|e| e.to_string())?.into(),
                减 => self.builder.build_float_sub(lf, rf, "fsub").map_err(|e| e.to_string())?.into(),
                乘 => self.builder.build_float_mul(lf, rf, "fmul").map_err(|e| e.to_string())?.into(),
                除 => self.builder.build_float_div(lf, rf, "fdiv").map_err(|e| e.to_string())?.into(),
                取余 => self.builder.build_float_rem(lf, rf, "frem").map_err(|e| e.to_string())?.into(),
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
                加 => self.builder.build_int_add(li, ri, "iadd").map_err(|e| e.to_string())?.into(),
                减 => self.builder.build_int_sub(li, ri, "isub").map_err(|e| e.to_string())?.into(),
                乘 => self.builder.build_int_mul(li, ri, "imul").map_err(|e| e.to_string())?.into(),
                除 => self.builder.build_int_signed_div(li, ri, "idiv").map_err(|e| e.to_string())?.into(),
                取余 => self.builder.build_int_signed_rem(li, ri, "irem").map_err(|e| e.to_string())?.into(),
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

    /// 生成表达式并保证结果是 i8*（字符串）；数值先转字符串。
    pub(super) fn 生成为字符串(
        &mut self,
        node: &AstNode,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let (v, t) = self
            .生成表达式(node)?
            .ok_or_else(|| "字符串拼接操作数无值".to_string())?;
        match t {
            Qi类型::字符串 => Ok(v.into_pointer_value()),
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
                self.调用返回指针("qi_runtime_int_to_string", &[iv.into()])
            }
            Qi类型::浮点数 => {
                self.调用返回指针("qi_runtime_float_to_string", &[v.into_float_value().into()])
            }
            Qi类型::结构体(_) => Err("结构体不能直接拼接为字符串".to_string()),
            Qi类型::函数值(_) => Err("函数值不能拼接为字符串".to_string()),
            Qi类型::空 => Err("空值不能拼接".to_string()),
        }
    }

    /// 生成表达式并保证结果是 i1。
    fn 生成为i1(
        &mut self,
        node: &AstNode,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        let (v, _t) = self
            .生成表达式(node)?
            .ok_or_else(|| "布尔操作数无值".to_string())?;
        let iv = v.into_int_value();
        if iv.get_type().get_bit_width() == 1 {
            Ok(iv)
        } else {
            // 非零即真
            self.builder
                .build_int_compare(
                    IntPredicate::NE,
                    iv,
                    iv.get_type().const_zero(),
                    "tobool",
                )
                .map_err(|e| e.to_string())
        }
    }

    /// 调 qi_runtime_string_concat 拼两串。
    fn 拼接字符串(
        &mut self,
        l: inkwell::values::PointerValue<'ctx>,
        r: inkwell::values::PointerValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let p = self.调用返回指针("qi_runtime_string_concat", &[l.into(), r.into()])?;
        Ok(p.into())
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
            .left()
            .map(|v| v.into_pointer_value())
            .ok_or_else(|| format!("{} 未返回值", rtname))
    }

    /// 用户函数调用 / 内建函数调用 / 打印调用。
    fn 生成函数调用(
        &mut self,
        call: &crate::parser::ast::FunctionCallExpression,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
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

        // 用户函数
        let mangled = super::mangle_function_name(&call.callee);
        let f = self
            .module
            .get_function(&mangled)
            .ok_or_else(|| format!("未定义的函数: {}", call.callee))?;
        let sig = self.符号.函数.get(&call.callee).cloned();

        let mut args: Vec<BasicMetadataValueEnum> = Vec::new();
        for (i, a) in call.arguments.iter().enumerate() {
            let (mut v, vt) = self
                .生成表达式(a)?
                .ok_or_else(|| "函数实参无值".to_string())?;
            // 若形参声明为浮点而实参是整数，隐式转换
            if let Some(s) = &sig {
                if let Some(pt) = s.参数.get(i) {
                    if pt.是浮点() && !vt.是浮点() {
                        v = self.转浮点(v, vt)?.into();
                    }
                }
            }
            args.push(v.into());
        }

        let ret = sig.as_ref().map(|s| s.返回).unwrap_or(Qi类型::整数);
        let cs = self
            .builder
            .build_call(f, &args, "call")
            .map_err(|e| e.to_string())?;
        match cs.try_as_basic_value().left() {
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
        for a in arguments {
            let (v, _t) = self
                .生成表达式(a)?
                .ok_or_else(|| "内建实参无值".to_string())?;
            args.push(v.into());
        }
        let cs = self
            .builder
            .build_call(f, &args, "call")
            .map_err(|e| e.to_string())?;
        let v = cs
            .try_as_basic_value()
            .left()
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
                Qi类型::整数 | Qi类型::未知 => {
                    (if 换行 { "qi_runtime_println_int" } else { "qi_runtime_print_int" }, v.into())
                }
                Qi类型::浮点数 => (
                    if 换行 { "qi_runtime_println_float" } else { "qi_runtime_print_float" },
                    v.into(),
                ),
                Qi类型::布尔 => {
                    // 运行时 bool 版收 i32，i1 先扩展
                    let i32v = self
                        .builder
                        .build_int_z_extend(v.into_int_value(), self.ctx.i32_type(), "b2i")
                        .map_err(|e| e.to_string())?;
                    (if 换行 { "qi_runtime_println_bool" } else { "qi_runtime_print_bool" }, i32v.into())
                }
                Qi类型::字符串 => {
                    (if 换行 { "qi_runtime_println" } else { "qi_runtime_print" }, v.into())
                }
                Qi类型::结构体(_) => return Err("不能直接打印结构体".to_string()),
                Qi类型::函数值(_) => return Err("不能直接打印函数值".to_string()),
                Qi类型::空 => return Err("不能打印空值".to_string()),
            };
            let f = self
                .module
                .get_function(rtname)
                .ok_or_else(|| format!("运行时函数未声明: {}", rtname))?;
            self.builder
                .build_call(f, &[arg], "call")
                .map_err(|e| e.to_string())?;
        }
        Ok(Some(None))
    }
}
