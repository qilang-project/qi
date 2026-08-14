//! `(自身 X) 方法(...)` 方法降级 + 方法调用（含链式）。
//!
//! 方法编译成普通 LLVM 函数：符号 = mangle("接收者类型_方法名")，
//! 第一个参数是接收者指针 `自身`。调用 `x.方法(args)` 先由类型检查器定
//! `x` 的结构体类型，再找对应方法函数，传 (x指针, args…) 调用。
//! 链式 `a().b().c()` 天然成立：接收者是任意表达式，其结果类型已知。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::函数签名;
use crate::parser::ast::{AstNode, MethodDeclaration, Program};
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue};

/// 方法符号名：接收者结构体的声明包 + 类型名 + 方法名，走统一 mangle。
/// 带包 —— 跨包同名结构体（Web::应用 vs CLI::应用）的同名方法符号不冲突。
fn 方法符号(pkg: Option<&str>, receiver_type: &str, method: &str) -> String {
    match pkg {
        Some(p) => super::mangle_function_name(&format!("{}${}_{}", p, receiver_type, method)),
        None => super::mangle_function_name(&format!("{}_{}", receiver_type, method)),
    }
}

impl<'ctx> 后端<'ctx> {
    /// 登记所有方法签名 + LLVM 原型（顶层 方法声明 + 结构体内嵌方法）。
    pub(super) fn 登记方法(&mut self, program: &Program) -> Result<(), String> {
        let 方法们 = 收集方法(program, &self.符号);
        for m in &方法们 {
            self.声明方法原型(m)?;
        }
        Ok(())
    }

    /// 生成所有方法体。
    pub(super) fn 生成所有方法体(&mut self, program: &Program) -> Result<(), String> {
        let 方法们 = 收集方法(program, &self.符号);
        for m in &方法们 {
            self.生成方法体(m)?;
        }
        Ok(())
    }

    /// 解析方法接收者结构体：索引 + 声明包（符号名用）。解析不到 → 编译报错。
    fn 解析方法接收者(
        &self,
        m: &MethodDeclaration,
    ) -> Result<(u32, Option<String>), String> {
        let idx = self.符号.结构体索引(&m.receiver_type).ok_or_else(|| {
            format!(
                "方法 {} 的接收者类型解析失败：{}",
                m.method_name,
                self.符号.结构体解析错误(&m.receiver_type)
            )
        })?;
        let 包 = self.符号.结构体信息(idx).and_then(|s| s.包.clone());
        Ok((idx, 包))
    }

    /// 声明单个方法的 LLVM 原型，并把签名（不含接收者）存入符号表。
    fn 声明方法原型(&mut self, m: &MethodDeclaration) -> Result<FunctionValue<'ctx>, String> {
        let (recv_idx, recv_pkg) = self.解析方法接收者(m)?;
        let 参数类型: Vec<Qi类型> = m
            .parameters
            .iter()
            .map(|p| {
                p.type_annotation
                    .as_ref()
                    .map(|t| self.符号.解析类型(t))
                    .unwrap_or(Qi类型::整数)
            })
            .collect();
        let 返回类型 = m
            .return_type
            .as_ref()
            .map(|t| self.符号.解析类型(t))
            .unwrap_or(Qi类型::空);

        // LLVM 参数：第一个是接收者指针，其余是方法形参
        let mut 参数llvm: Vec<BasicMetadataTypeEnum> =
            vec![self.ctx.ptr_type(inkwell::AddressSpace::default()).into()];
        for t in &参数类型 {
            参数llvm.push(
                self.llvm基础类型(*t)
                    .map(|b| b.into())
                    .unwrap_or_else(|| self.ctx.i64_type().into()),
            );
        }

        let fn_type = match self.llvm基础类型(返回类型) {
            Some(rt) => rt.fn_type(&参数llvm, false),
            None => self.ctx.void_type().fn_type(&参数llvm, false),
        };

        let sym = 方法符号(recv_pkg.as_deref(), &m.receiver_type, &m.method_name);
        let func = self.module.add_function(&sym, fn_type, None);

        self.符号.方法.insert(
            (recv_idx, m.method_name.clone()),
            函数签名 {
                参数: 参数类型,
                返回: 返回类型,
            },
        );
        Ok(func)
    }

    /// 生成方法体：接收者 `自身` 作为结构体指针局部变量。
    fn 生成方法体(&mut self, m: &MethodDeclaration) -> Result<(), String> {
        let (recv_idx, recv_pkg) = self.解析方法接收者(m)?;
        let sym = 方法符号(recv_pkg.as_deref(), &m.receiver_type, &m.method_name);
        let func = self
            .module
            .get_function(&sym)
            .ok_or_else(|| format!("方法原型缺失: {}", sym))?;
        let sig = self
            .符号
            .方法
            .get(&(recv_idx, m.method_name.clone()))
            .cloned()
            .ok_or_else(|| format!("方法签名缺失: {}", m.method_name))?;

        let 接收者类型 = Qi类型::结构体(recv_idx);

        self.变量表.clear();
        self.作用域遮蔽栈.clear();
        self.弧隐藏引用计数槽.clear();
        self.符号.进入作用域();
        self.当前返回类型 = sig.返回;
        self.try深度 = 0; // E4

        let entry = self.ctx.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        // 调试信息 + 剖析共用显示名 = 接收者类型.方法名（消歧同名方法）
        let 剖析显示名 = format!("{}.{}", m.receiver_type, m.method_name);
        // 接收者 自身 不在 sig.参数 里，DWARF 签名前面补一个指针占位，
        // 参数序号才跟 create_parameter_variable 的 1-based 对得上。
        let mut 调试参数 = vec![接收者类型];
        调试参数.extend_from_slice(&sig.参数);
        self.调试_进入函数(func, &剖析显示名, &sym, m.span, &调试参数, sig.返回);
        self.剖析入口(&剖析显示名)?;

        // 接收者 自身：alloca 存指针
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let recv_ptr = self
            .builder
            .build_alloca(ptrt, &m.receiver_name)
            .map_err(|e| e.to_string())?;
        let recv_arg = func
            .get_nth_param(0)
            .ok_or_else(|| "方法缺少接收者参数".to_string())?;
        self.builder
            .build_store(recv_ptr, recv_arg)
            .map_err(|e| e.to_string())?;
        // ARC：接收者（结构体指针，对调用方是借用）落地进局部槽 → retain 一次，
        // 与出口「释放 RC 局部」平衡
        self.弧retain任意(recv_arg, 接收者类型);
        self.调试_局部变量(&m.receiver_name, 接收者类型, recv_ptr, m.span, Some(1));
        self.变量表
            .insert(m.receiver_name.clone(), (recv_ptr, 接收者类型));
        self.符号.声明变量(&m.receiver_name, 接收者类型);

        // 其余形参
        for (i, p) in m.parameters.iter().enumerate() {
            let t = sig.参数.get(i).copied().unwrap_or(Qi类型::整数);
            let llvmt = self
                .llvm基础类型(t)
                .ok_or_else(|| format!("参数 {} 类型无效", p.name))?;
            let ptr = self
                .builder
                .build_alloca(llvmt, &p.name)
                .map_err(|e| e.to_string())?;
            let arg = func
                .get_nth_param((i + 1) as u32)
                .ok_or_else(|| format!("缺少第 {} 个形参", i))?;
            self.builder
                .build_store(ptr, arg)
                .map_err(|e| e.to_string())?;
            // ARC：RC 参数（借用）落地进局部槽 → retain 一次，与出口统一释放平衡
            self.弧retain任意(arg, t);
            // 形参序号从 2 起：1 号是接收者 自身
            self.调试_局部变量(&p.name, t, ptr, p.span, Some(i as u32 + 2));
            self.变量表.insert(p.name.clone(), (ptr, t));
            self.符号.声明变量(&p.name, t);
        }

        for stmt in &m.body {
            self.生成语句(stmt, func)?;
            if self.当前块已终结() {
                break;
            }
        }

        if !self.当前块已终结() {
            self.剖析出口()?; // 剖析：落底出口计时
            self.弧释放局部()?; // ARC：落底默认返回前释放字符串局部
            match self.llvm基础类型(sig.返回) {
                Some(rt) => {
                    let zero = rt.const_zero();
                    self.builder
                        .build_return(Some(&zero))
                        .map_err(|e| e.to_string())?;
                }
                None => {
                    self.builder.build_return(None).map_err(|e| e.to_string())?;
                }
            }
        }

        self.调试_离开函数(); // 位置粘性，见 调试信息.rs 模块头
        self.符号.退出作用域();
        Ok(())
    }

    /// 方法调用 x.方法(args)。返回 None 表示不是结构体方法（交回上层做打印等归一）。
    pub(super) fn 生成方法调用(
        &mut self,
        object: &AstNode,
        method: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        // 先求接收者的类型；不是结构体则不是方法调用。
        let recv_type = super::类型检查::推断表达式类型(object, &self.符号);
        let idx = match recv_type.结构体索引() {
            Some(i) => i,
            None => return Ok(None),
        };
        let (结构体名, 结构体包) = self
            .符号
            .结构体信息(idx)
            .map(|s| (s.名字.clone(), s.包.clone()))
            .ok_or_else(|| "结构体信息缺失".to_string())?;

        let sig = match self.符号.方法.get(&(idx, method.to_string())).cloned() {
            Some(s) => s,
            None => {
                // 函数值字段以方法语法调用：`命令值.持久前置执行函数(ctx)` ——
                // 结构体没有这个方法、但有同名 函数(..) 类型字段 → load 字段 + fat 调用。
                if let Some((_, Qi类型::函数值(fidx))) =
                    self.符号.结构体信息(idx).and_then(|s| s.查字段(method))
                {
                    let fsig = self
                        .符号
                        .函数值签名(fidx)
                        .cloned()
                        .ok_or_else(|| format!("函数值字段签名缺失: {}", method))?;
                    let (fv, _ft) = self.生成字段访问(object, method)?;
                    let r = self.发射fat调用(fv.into_pointer_value(), &fsig, arguments)?;
                    return Ok(Some(r));
                }
                return Err(format!(
                    "结构体 {} 无方法 {}（也没有同名函数类型字段）",
                    结构体名, method
                ));
            }
        };

        // 求接收者指针值（可能是链式的上一个方法调用结果）
        let (recv_val, _t) = self
            .生成表达式(object)?
            .ok_or_else(|| "方法接收者无值".to_string())?;

        let sym = 方法符号(结构体包.as_deref(), &结构体名, method);
        let func = self
            .module
            .get_function(&sym)
            .ok_or_else(|| format!("方法未定义: {}", sym))?;

        let mut args: Vec<BasicMetadataValueEnum> = vec![recv_val.into()];
        let mut 弧待释放: Vec<(BasicValueEnum, Qi类型)> = Vec::new();
        // ARC：链式调用的 OWNED 临时接收者（如 f(x).方法()）—— 方法自会对
        // 自身 retain/release，调用结束后释放这份 +1。
        if self.弧开() && recv_val.is_pointer_value() && self.表达式拥有RC(object, recv_type)
        {
            弧待释放.push((recv_val, recv_type));
        }
        for (i, a) in arguments.iter().enumerate() {
            let (mut v, vt) = self
                .生成表达式(a)?
                .ok_or_else(|| "方法实参无值".to_string())?;
            // ARC：实参借用传递，OWNED RC 临时调用后释放。结构体/数组要求
            // 形参声明类型也是 RC（否则被调方不套 retain 纪律 —— 保守不释放）。
            let 形参rc = sig
                .参数
                .get(i)
                .map(|pt| super::所有权::是RC类型(*pt))
                .unwrap_or(false);
            let 可释放 = match vt {
                Qi类型::字符串 => self.表达式拥有字符串(a),
                Qi类型::结构体(_) | Qi类型::数组(_) | Qi类型::函数值(_) => {
                    形参rc && self.表达式拥有RC(a, vt)
                }
                _ => false,
            };
            if self.弧开() && v.is_pointer_value() && 可释放 {
                弧待释放.push((v, vt));
            }
            if let Some(pt) = sig.参数.get(i) {
                if pt.是浮点() && !vt.是浮点() {
                    v = self.实参转浮点(v)?;
                }
            }
            args.push(v.into());
        }

        let cs = self
            .builder
            .build_call(func, &args, "mcall")
            .map_err(|e| e.to_string())?;
        for (v, vt) in 弧待释放 {
            self.弧release任意(v, vt);
        }
        match cs.try_as_basic_value().basic() {
            Some(v) => Ok(Some(Some((v, sig.返回)))),
            None => Ok(Some(None)),
        }
    }

    /// 整数实参 sitofp → double。
    fn 实参转浮点(&self, v: BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        Ok(self
            .builder
            .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
            .map_err(|e| e.to_string())?
            .into())
    }
}

/// 收集所有方法声明：顶层 方法声明 + 结构体内嵌 methods + 实现块方法
/// （含特性默认方法的按类型合成）。
fn 收集方法(
    program: &Program, 符号: &super::类型检查::符号表
) -> Vec<MethodDeclaration> {
    let mut out = Vec::new();
    for stmt in &program.statements {
        match stmt {
            AstNode::方法声明(m) => out.push(m.clone()),
            AstNode::结构体声明(sd) => {
                for m in &sd.methods {
                    out.push(m.clone());
                }
            }
            // 实现块 `实现 [特性 对于] 类型 { 函数 ... }`：方法的 receiver_type 空，
            // 从 impl.target_type 补上（parser 里留空由 codegen 填）。
            AstNode::实现块(imp) => {
                for m in &imp.methods {
                    let mut m = m.clone();
                    if m.receiver_type.is_empty() {
                        m.receiver_type = imp.target_type.clone();
                    }
                    out.push(m);
                }
                // 特性默认方法：特性里带默认体、实现块没覆盖的方法 →
                // 拿默认体 AST 以具体类型合成普通方法（`自己` 即绑定到该类型，
                // 体内 自己.抽象方法() 走正常方法解析 —— 模板方法模式）。
                // 覆盖优先：实现块写了同名方法就用实现块的。
                if let Some(特性名) = &imp.trait_name {
                    if let Some(info) = 符号.特性.get(特性名) {
                        for tm in &info.方法 {
                            let Some(默认体) = &tm.default_body else {
                                continue;
                            };
                            if imp.methods.iter().any(|m| m.method_name == tm.name) {
                                continue; // 已覆盖
                            }
                            out.push(MethodDeclaration {
                                receiver_name: "自己".to_string(),
                                receiver_type: imp.target_type.clone(),
                                is_receiver_mutable: false,
                                method_name: tm.name.clone(),
                                parameters: tm.parameters.clone(),
                                return_type: tm.return_type.clone(),
                                body: 默认体.clone(),
                                visibility: crate::parser::ast::Visibility::公开,
                                span: tm.span,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}
