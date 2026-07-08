//! 外部 C 函数声明 —— 手写 C FFI 绑定。
//!
//! `外部 "库名" { 函数 cos(x: 浮点数): 浮点数; ... }` 让用户直接声明并调用
//! C 库函数（libm/libc/libcrypto…）。这是 C bindgen 自动生成的前置语言特性。
//!
//! 设计要点：
//!   - C 名不 mangle（原样进 LLVM module），裸调用（`cos(3.14)`）与用户函数同名空间。
//!   - C ABI 类型映射：整数→i64、浮点数→f64、布尔→i64(0/1)、字符串→const char*(ptr)。
//!   - 字符串实参传 QiStr 的 data 指针（NUL 结尾，C 只读安全）。
//!   - 返回值不纳入 ARC（C 分配的内存 Qi 不管）。v1 仅支持返回
//!     整数/浮点数/布尔/空；字符串(char*) 返回暂不支持（所有权待 bindgen 处理）。
//!   - 库名收集在 lib.rs 侧扫 AST 完成，链接期加 `-l<库名>`。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::函数签名;
use crate::parser::ast::{ExternBlock, ExternFn, Program};
use inkwell::types::BasicType;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::AddressSpace;

impl<'ctx> 后端<'ctx> {
    /// 登记一个模块里所有外部块的 C 函数：建 LLVM 原型（C 名不修饰）+ 存签名。
    /// 与用户函数同名 → 人话报错（同名空间，撞名不允许）。
    pub(super) fn 登记外部(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let crate::parser::ast::AstNode::外部声明(blk) = stmt {
                self.登记外部块(blk)?;
            }
        }
        Ok(())
    }

    fn 登记外部块(&mut self, blk: &ExternBlock) -> Result<(), String> {
        for f in &blk.functions {
            self.登记外部函数(f, &blk.library)?;
        }
        Ok(())
    }

    fn 登记外部函数(&mut self, f: &ExternFn, 库名: &str) -> Result<(), String> {
        // 撞名检查：与已声明的外部函数 / 用户函数同名 → 报错。
        if self.符号.外部函数.contains(&f.name) {
            return Err(format!(
                "外部函数 {} 重复声明（同一名字只能声明一次外部绑定）",
                f.name
            ));
        }
        if self.符号.函数.contains_key(&f.name) || self.符号.有函数(&f.name) {
            return Err(format!(
                "外部函数 {} 与已定义的函数同名 —— 外部函数与用户函数共用同一命名空间，请改名其一",
                f.name
            ));
        }

        // 参数类型：解析注解 → Qi类型，逐个校验 C ABI 可映射。
        let mut 参数类型: Vec<Qi类型> = Vec::new();
        for p in &f.parameters {
            if p.is_variadic {
                return Err(format!(
                    "外部函数 {} 的参数 {} 使用了变参 `...` —— C FFI 暂不支持变参绑定",
                    f.name, p.name
                ));
            }
            let t = p
                .type_annotation
                .as_ref()
                .map(|t| self.符号.解析类型(t))
                .ok_or_else(|| format!("外部函数 {} 的参数 {} 缺少类型注解", f.name, p.name))?;
            self.校验外部参数类型(&f.name, &p.name, t)?;
            参数类型.push(t);
        }

        // 返回类型：v1 仅支持 整数/浮点数/布尔/空。字符串(char*) 返回暂不支持。
        let 返回类型 = match &f.return_type {
            Some(t) => self.符号.解析类型(t),
            None => Qi类型::空,
        };
        self.校验外部返回类型(&f.name, 返回类型)?;

        // 建 C ABI LLVM 原型（C 名原样，不 mangle）。
        let 参数llvm: Vec<inkwell::types::BasicMetadataTypeEnum> = 参数类型
            .iter()
            .map(|t| self.外部llvm类型(*t).into())
            .collect();
        let fn_type = match 返回类型 {
            Qi类型::空 => self.ctx.void_type().fn_type(&参数llvm, false),
            其他 => self.外部llvm类型(其他).fn_type(&参数llvm, false),
        };
        // 幂等：同名 C 符号跨模块可能重复声明（多个文件都 `外部 "m"`），复用已有原型。
        if self.module.get_function(&f.name).is_none() {
            self.module.add_function(&f.name, fn_type, None);
        }
        let _ = 库名; // 库名在 lib.rs 侧扫 AST 收集用于链接，这里仅登记符号。

        // 签名进扁平表（供返回类型推断），并标记为外部（调用走 C ABI 降级）。
        self.符号.函数.insert(
            f.name.clone(),
            函数签名 {
                参数: 参数类型,
                返回: 返回类型,
            },
        );
        self.符号.外部函数.insert(f.name.clone());
        Ok(())
    }

    /// C ABI 参数类型校验。v1 支持：整数/浮点数/布尔/字符串。
    fn 校验外部参数类型(
        &self,
        函数名: &str,
        参数名: &str,
        t: Qi类型,
    ) -> Result<(), String> {
        match t {
            Qi类型::整数 | Qi类型::浮点数 | Qi类型::布尔 | Qi类型::字符串 => {
                Ok(())
            }
            _ => Err(format!(
                "外部函数 {} 的参数 {} 类型不被 C FFI 支持 —— \
                 参数仅支持 整数/浮点数/布尔/字符串",
                函数名, 参数名
            )),
        }
    }

    /// C ABI 返回类型校验。v1 支持：整数/浮点数/布尔/空。
    /// 字符串(char*) 返回暂不支持（C 分配内存的所有权待 bindgen / 后续处理）。
    fn 校验外部返回类型(&self, 函数名: &str, t: Qi类型) -> Result<(), String> {
        match t {
            Qi类型::整数 | Qi类型::浮点数 | Qi类型::布尔 | Qi类型::空 => Ok(()),
            Qi类型::字符串 => Err(format!(
                "外部函数 {} 返回 字符串(char*) —— v1 暂不支持 char* 返回\
                （C 分配内存的所有权 Qi 无法安全管理，将来由 bindgen 处理）。\
                 请先只用返回 整数/浮点数/布尔 的绑定。",
                函数名
            )),
            _ => Err(format!(
                "外部函数 {} 的返回类型不被 C FFI 支持 —— 返回仅支持 整数/浮点数/布尔/空",
                函数名
            )),
        }
    }

    /// Qi 类型 → C ABI 的 LLVM 基础类型（布尔按 i64 传，C 侧当 int）。
    /// 仅对已通过校验的类型调用。
    fn 外部llvm类型(&self, t: Qi类型) -> inkwell::types::BasicTypeEnum<'ctx> {
        match t {
            Qi类型::整数 | Qi类型::布尔 => self.ctx.i64_type().into(),
            Qi类型::浮点数 => self.ctx.f64_type().into(),
            Qi类型::字符串 => self.ctx.ptr_type(AddressSpace::default()).into(),
            // 校验已排除其余类型；防御性回退 i64。
            _ => self.ctx.i64_type().into(),
        }
    }

    /// 外部 C 函数调用降级。callee 已确认在 符号.外部函数 里。
    /// 按 C ABI 传参（字符串传 data ptr、布尔 zext 到 i64），返回不纳入 ARC。
    pub(super) fn 生成外部调用(
        &mut self,
        call: &crate::parser::ast::FunctionCallExpression,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let sig = self
            .符号
            .函数
            .get(&call.callee)
            .cloned()
            .ok_or_else(|| format!("外部函数签名缺失: {}", call.callee))?;
        let f = self
            .module
            .get_function(&call.callee)
            .ok_or_else(|| format!("外部函数原型缺失: {}", call.callee))?;

        if call.arguments.len() != sig.参数.len() {
            return Err(format!(
                "外部函数 {} 需要 {} 个参数，却传了 {} 个",
                call.callee,
                sig.参数.len(),
                call.arguments.len()
            ));
        }

        let i64t = self.ctx.i64_type();
        let mut args: Vec<BasicMetadataValueEnum> = Vec::new();
        for (i, a) in call.arguments.iter().enumerate() {
            let pt = sig.参数[i];
            let (v, vt) = self
                .生成带期望(a, Some(pt))?
                .ok_or_else(|| "外部函数实参无值".to_string())?;
            // 先按 Qi 语义协调到形参类型（int↔float、ptr 句柄等），再补 C ABI 加宽。
            let v = self.协调到类型(v, vt, pt)?;
            let v = match pt {
                // 布尔：i1 → i64（C 侧当 int）。协调后若仍是窄整数则加宽。
                Qi类型::布尔 => {
                    let iv = v.into_int_value();
                    if iv.get_type().get_bit_width() < 64 {
                        self.builder
                            .build_int_z_extend(iv, i64t, "b2i64")
                            .map_err(|e| e.to_string())?
                            .into()
                    } else {
                        v
                    }
                }
                _ => v,
            };
            args.push(v.into());
        }

        let cs = self
            .builder
            .build_call(f, &args, "cffi")
            .map_err(|e| e.to_string())?;

        match sig.返回 {
            Qi类型::空 => Ok(None),
            Qi类型::布尔 => {
                // C 返回 int(i64) → 截回 i1（!= 0）。
                let rv = cs
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "外部函数应有返回值".to_string())?
                    .into_int_value();
                let b = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::NE, rv, i64t.const_zero(), "c2bool")
                    .map_err(|e| e.to_string())?;
                Ok(Some((b.into(), Qi类型::布尔)))
            }
            其他 => {
                let rv = cs
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "外部函数应有返回值".to_string())?;
                Ok(Some((rv, 其他)))
            }
        }
    }
}
