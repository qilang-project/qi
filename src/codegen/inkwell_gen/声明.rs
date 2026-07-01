//! 函数声明降级 —— 用户函数（非 入口）定义为类型化 LLVM function。
//!
//! 两趟：先登记所有函数签名（供前向调用 / 类型检查），再逐个生成函数体。
//! 参数用 alloca+store 落地成局部变量，body 内当普通变量读写，语义与旧后端一致。

use super::类型::Qi类型;
use super::类型检查::函数签名;
use super::后端;
use crate::parser::ast::{FunctionDeclaration, Program};
use inkwell::types::BasicType;
use inkwell::values::FunctionValue;

impl<'ctx> 后端<'ctx> {
    /// 第一趟：登记所有顶层函数签名 + 声明 LLVM function 原型。
    pub(super) fn 登记函数(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let crate::parser::ast::AstNode::函数声明(f) = stmt {
                if f.name == "入口" {
                    continue; // 入口单独变 main
                }
                self.声明函数原型(f)?;
            }
        }
        Ok(())
    }

    /// 声明单个用户函数的 LLVM 原型，并把签名存入符号表。
    fn 声明函数原型(&mut self, f: &FunctionDeclaration) -> Result<FunctionValue<'ctx>, String> {
        let 参数类型: Vec<Qi类型> = f
            .parameters
            .iter()
            .map(|p| {
                p.type_annotation
                    .as_ref()
                    .map(|t| self.符号.解析类型(t))
                    .unwrap_or(Qi类型::整数)
            })
            .collect();
        let 返回类型 = f
            .return_type
            .as_ref()
            .map(|t| self.符号.解析类型(t))
            .unwrap_or(Qi类型::空);

        // 构造 LLVM 函数类型
        let 参数llvm: Vec<inkwell::types::BasicMetadataTypeEnum> = 参数类型
            .iter()
            .map(|t| {
                self.llvm基础类型(*t)
                    .map(|b| b.into())
                    .unwrap_or_else(|| self.ctx.i64_type().into())
            })
            .collect();

        let fn_type = match self.llvm基础类型(返回类型) {
            Some(rt) => rt.fn_type(&参数llvm, false),
            None => self.ctx.void_type().fn_type(&参数llvm, false),
        };

        let mangled = super::mangle_function_name(&f.name);
        let func = self.module.add_function(&mangled, fn_type, None);

        self.符号.函数.insert(
            f.name.clone(),
            函数签名 {
                参数: 参数类型,
                返回: 返回类型,
            },
        );
        Ok(func)
    }

    /// 第二趟：生成一个用户函数的函数体。
    pub(super) fn 生成函数体(&mut self, f: &FunctionDeclaration) -> Result<(), String> {
        let mangled = super::mangle_function_name(&f.name);
        let func = self
            .module
            .get_function(&mangled)
            .ok_or_else(|| format!("函数原型缺失: {}", f.name))?;

        let sig = self
            .符号
            .函数
            .get(&f.name)
            .cloned()
            .ok_or_else(|| format!("函数签名缺失: {}", f.name))?;

        // 每个函数独立的局部变量表 / 作用域 / 返回类型
        self.变量表.clear();
        self.符号.进入作用域();
        self.当前返回类型 = sig.返回;

        let entry = self.ctx.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        // 形参落地为 alloca 局部变量
        for (i, p) in f.parameters.iter().enumerate() {
            let t = sig.参数.get(i).copied().unwrap_or(Qi类型::整数);
            let llvmt = self
                .llvm基础类型(t)
                .ok_or_else(|| format!("参数 {} 类型无效", p.name))?;
            let ptr = self
                .builder
                .build_alloca(llvmt, &p.name)
                .map_err(|e| e.to_string())?;
            let arg = func
                .get_nth_param(i as u32)
                .ok_or_else(|| format!("缺少第 {} 个形参", i))?;
            self.builder.build_store(ptr, arg).map_err(|e| e.to_string())?;
            self.变量表.insert(p.name.clone(), (ptr, t));
            self.符号.声明变量(&p.name, t);
        }

        for stmt in &f.body {
            self.生成语句(stmt, func)?;
            if self.当前块已终结() {
                break;
            }
        }

        // body 未显式 return 时补默认返回
        if !self.当前块已终结() {
            match self.llvm基础类型(sig.返回) {
                Some(rt) => {
                    let zero = rt.const_zero();
                    self.builder.build_return(Some(&zero)).map_err(|e| e.to_string())?;
                }
                None => {
                    self.builder.build_return(None).map_err(|e| e.to_string())?;
                }
            }
        }

        self.符号.退出作用域();
        Ok(())
    }
}
