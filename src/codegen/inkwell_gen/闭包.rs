//! 函数值 / 函数指针 / 间接调用。
//!
//! 阶段9 范围：支持**顶层函数名作为值**（取函数指针传递）+ 通过函数值变量间接调用。
//! 这是工具注册、回调（web handler、块回调）的基础。真闭包（捕获环境）暂不支持，
//! 见「已知限制」。
//!
//! 表示：函数值就是一个裸函数指针（不透明 ptr）。间接调用时据函数值签名重建
//! LLVM 函数类型，用 `build_indirect_call` 调用。

use super::类型::Qi类型;
use super::后端;
use crate::parser::ast::AstNode;
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};

impl<'ctx> 后端<'ctx> {
    /// 标识符作为函数值：若它不是局部变量但是顶层函数，返回其函数指针。
    /// 返回 Ok(None) 表示不是函数值场景（交回上层按普通标识符处理）。
    pub(super) fn 标识符作为函数值(
        &mut self,
        name: &str,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        if self.变量表.contains_key(name) {
            return Ok(None);
        }
        let t = match self.符号.函数为值(name) {
            Some(t) => t,
            None => return Ok(None),
        };
        let mangled = super::mangle_function_name(name);
        let f = self
            .module
            .get_function(&mangled)
            .ok_or_else(|| format!("函数值指向未定义函数: {}", name))?;
        let ptr = f.as_global_value().as_pointer_value();
        Ok(Some((ptr.into(), t)))
    }

    /// 间接调用：callee 是一个函数值变量。返回 Ok(None) 表示 callee 不是函数值变量。
    pub(super) fn 尝试间接调用(
        &mut self,
        callee: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        let vt = match self.变量表.get(callee).map(|(_, t)| *t) {
            Some(t) => t,
            None => return Ok(None),
        };
        let idx = match vt.函数值索引() {
            Some(i) => i,
            None => return Ok(None),
        };
        let sig = self
            .符号
            .函数值签名(idx)
            .cloned()
            .ok_or_else(|| "函数值签名缺失".to_string())?;

        // 载入函数指针
        let (ptr_alloca, _) = self.变量表.get(callee).cloned().unwrap();
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let fptr = self
            .builder
            .build_load(ptrt, ptr_alloca, callee)
            .map_err(|e| e.to_string())?
            .into_pointer_value();

        // 重建 LLVM 函数类型
        let 参数llvm: Vec<BasicMetadataTypeEnum> = sig
            .参数
            .iter()
            .map(|t| {
                self.llvm基础类型(*t)
                    .map(|b| b.into())
                    .unwrap_or_else(|| self.ctx.i64_type().into())
            })
            .collect();
        let fn_type = match self.llvm基础类型(sig.返回) {
            Some(rt) => rt.fn_type(&参数llvm, false),
            None => self.ctx.void_type().fn_type(&参数llvm, false),
        };

        // 求实参
        let mut args: Vec<BasicMetadataValueEnum> = Vec::new();
        for (i, a) in arguments.iter().enumerate() {
            let (mut v, at) = self
                .生成表达式(a)?
                .ok_or_else(|| "间接调用实参无值".to_string())?;
            if let Some(pt) = sig.参数.get(i) {
                if pt.是浮点() && !at.是浮点() {
                    v = self
                        .builder
                        .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
                        .map_err(|e| e.to_string())?
                        .into();
                }
            }
            args.push(v.into());
        }

        let cs = self
            .builder
            .build_indirect_call(fn_type, fptr, &args, "icall")
            .map_err(|e| e.to_string())?;
        match cs.try_as_basic_value().left() {
            Some(v) => Ok(Some(Some((v, sig.返回)))),
            None => Ok(Some(None)),
        }
    }
}
