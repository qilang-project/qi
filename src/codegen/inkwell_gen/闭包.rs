//! 函数值 / 函数指针 / 真闭包 / 间接调用 —— fat 对象 ABI（与 closure_ffi.rs 一致）。
//!
//! 统一表示：**所有函数值都是 fat 对象指针** `[fn_ptr@slot0, cap@slot1..]`。
//! 调用约定：`f = qi_closure_get_fn(obj); f(obj, 用户参数...)`（obj 恒为首参）。
//!
//! - 顶层函数当值：生成 trampoline `__tramp_<mangled>(env, args...) { call <mangled>(args...) }`
//!   吃掉首个 env 参，再 `qi_closure_create(tramp, 0)`。
//! - 真闭包 `闭包(...)`：扫自由变量 → 合成 `__closure_N(env, args...)`（序言从 env 读捕获注入
//!   同名局部）→ `qi_closure_create(__closure_N, num_caps)` + 逐个 set 捕获值。
//! - 间接调用：局部/全局/参数变量持 fat obj → fat call。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::函数签名;
use crate::parser::ast::{AstNode, ClosureExpression};
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, PointerValue};
use std::collections::HashSet;

/// 待合成的闭包：合成函数所需的全部信息（表达式位置先登记，末尾统一生成体）。
pub(super) struct 待合成闭包 {
    pub 符号名: String,
    pub 参数类型: Vec<Qi类型>,
    pub 参数名: Vec<String>,
    pub 返回类型: Qi类型,
    pub 捕获: Vec<(String, Qi类型)>,
    /// 弱捕获名单（`闭包 [弱 x]`）：这些名字的捕获走 unowned 语义 ——
    /// 不 retain、env dtor 不 release、闭包体出口不 release，用于打破引用环。
    pub 弱名单: std::collections::HashSet<String>,
    pub body: Vec<AstNode>,
    /// 登记闭包时所在的包（体在末尾统一合成，须还原包上下文才能解析本包结构体/函数）。
    pub 包: Option<String>,
}

impl<'ctx> 后端<'ctx> {
    /// 标识符作为函数值：不是变量但是顶层函数 → box 成 fat obj（带 trampoline）。
    pub(super) fn 标识符作为函数值(
        &mut self,
        name: &str,
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        if self.变量表.contains_key(name) || self.全局变量表.contains_key(name) {
            return Ok(None);
        }
        let t = match self.符号.函数为值(name) {
            Some(t) => t,
            None => return Ok(None),
        };
        // 重载函数当值会有歧义（不知道取哪个元数的那个）—— 直接报错。
        if let Some(集) = self.符号.重载集(name) {
            if 集.len() > 1 {
                return Err(format!(
                    "重载函数「{}」不能作为函数值/指针传递（有多个元数版本，无法确定取哪个）。请改名区分，或包装成闭包。",
                    name
                ));
            }
        }
        let idx = t.函数值索引().ok_or_else(|| "函数值索引缺失".to_string())?;
        let sig = self
            .符号
            .函数值签名(idx)
            .cloned()
            .ok_or_else(|| "函数值签名缺失".to_string())?;
        // 目标真函数符号：当前包优先，否则任意包，否则裸符号（按该函数形参个数 mangle）
        let mangled = self.解析函数符号名(name, sig.参数.len());
        let tramp = self.生成trampoline(&mangled, &sig)?;
        // 顶层函数无捕获 → 走不朽单例缓存（qi_closure_intern）：同一函数全程序共享一个
        // 闭包、不重复分配、不朽不计泄漏。免了「存进指针列表(图/工具注册表)漏闭包」。
        let intern = self.module.get_function("qi_closure_intern").unwrap();
        let cs = self
            .builder
            .build_call(intern, &[tramp.into()], "clo.intern")
            .map_err(|e| e.to_string())?;
        let obj = cs
            .try_as_basic_value()
            .basic()
            .map(|v| v.into_pointer_value())
            .ok_or_else(|| "closure_intern 未返回".to_string())?;
        Ok(Some((obj.into(), t)))
    }

    /// 找一个顶层函数的实际 LLVM 符号名（当前包 → destructure 导入来源包 →
    /// 全局唯一定义包 → 裸）。多包同名不猜（与 尝试解析用户函数 同规则）。
    fn 解析函数符号名(&self, name: &str, 元数: usize) -> String {
        let 当前 = super::包内符号名(self.当前包.as_deref(), name, 元数);
        if self.module.get_function(&当前).is_some() {
            return 当前;
        }
        if let Some(src) = self.符号.导入来源(name) {
            let sym = super::包内符号名(Some(src), name, 元数);
            if self.module.get_function(&sym).is_some() {
                return sym;
            }
        }
        if let [唯一] = self.符号.函数候选包(name).as_slice() {
            let sym = super::包内符号名(Some(唯一), name, 元数);
            if self.module.get_function(&sym).is_some() {
                return sym;
            }
        }
        super::包内符号名(None, name, 元数)
    }

    /// 真闭包表达式 → 合成函数 + fat obj + 填捕获。
    pub(super) fn 生成闭包表达式(
        &mut self,
        c: &ClosureExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        // 参数类型/名
        let 参数类型: Vec<Qi类型> = c
            .parameters
            .iter()
            .map(|p| {
                p.type_annotation
                    .as_ref()
                    .map(|t| self.符号.解析类型(t))
                    .unwrap_or(Qi类型::整数)
            })
            .collect();
        let 参数名: Vec<String> = c.parameters.iter().map(|p| p.name.clone()).collect();
        let 返回类型 = c
            .return_type
            .as_ref()
            .map(|t| self.符号.解析类型(t))
            .unwrap_or(Qi类型::空);

        // 扫自由变量（捕获）：body 里引用、且当前 变量表 有记录、非参数/非 body 内声明/非函数/非模块。
        let mut 参数集: HashSet<String> = 参数名.iter().cloned().collect();
        let mut 本地声明: HashSet<String> = HashSet::new();
        for s in &c.body {
            self.收集本地声明(s, &mut 本地声明);
        }
        let mut 自由: Vec<String> = Vec::new();
        let mut 已见: HashSet<String> = HashSet::new();
        for s in &c.body {
            self.收集自由标识符(s, &参数集, &本地声明, &mut 自由, &mut 已见);
        }
        参数集.clear();
        // 只保留当前作用域里确有记录的（捕获值）
        let 捕获: Vec<(String, Qi类型)> = 自由
            .into_iter()
            .filter_map(|n| self.变量表.get(&n).map(|(_, t)| (n, *t)))
            .collect();

        // 弱捕获名单（用户 `[弱 x]` 标注且确实被自动捕获的）。
        let 弱名单: std::collections::HashSet<String> = c
            .weak_captures
            .iter()
            .filter(|n| 捕获.iter().any(|(名, _)| 名 == *n))
            .cloned()
            .collect();

        // 登记函数值签名
        let sig = 函数签名 {
            参数: 参数类型.clone(),
            返回: 返回类型,
        };
        let idx = self.符号.登记函数值签名(sig);

        // 生成唯一符号，登记到待合成列表
        let 符号名 = format!("__closure_{}", self.闭包计数);
        self.闭包计数 += 1;
        self.待合成闭包.push(待合成闭包 {
            符号名: 符号名.clone(),
            参数类型,
            参数名,
            返回类型,
            捕获: 捕获.clone(),
            弱名单: 弱名单.clone(),
            body: c.body.clone(),
            包: self.当前包.clone(),
        });

        // 声明合成函数原型（body 稍后生成），建 fat obj
        let fn_val = self.声明闭包原型(&符号名, &捕获, self.待合成闭包.last().unwrap())?;
        // 0 捕获闭包 = 无状态单例（如 工具适配 生成的适配器）→ 走不朽单例缓存，
        // 免「存进注册表/列表无人释放」的闭包泄漏；带捕获的按实例正常建。
        let obj = if 捕获.is_empty() {
            let intern = self.module.get_function("qi_closure_intern").unwrap();
            let cs = self
                .builder
                .build_call(intern, &[fn_val.into()], "clo.intern")
                .map_err(|e| e.to_string())?;
            cs.try_as_basic_value()
                .basic()
                .map(|v| v.into_pointer_value())
                .ok_or_else(|| "closure_intern 未返回".to_string())?
        } else {
            self.创建闭包对象(fn_val, 捕获.len() as u64)?
        };

        // 填捕获：从当前作用域 load 值 → qi_closure_set_int/set_ptr
        for (i, (名, t)) in 捕获.iter().enumerate() {
            let (ptr, _) = self
                .变量表
                .get(名)
                .cloned()
                .ok_or_else(|| format!("捕获变量 {} 缺失", 名))?;
            let llvmt = self
                .llvm基础类型(*t)
                .unwrap_or_else(|| self.ctx.i64_type().into());
            let v = self
                .builder
                .build_load(llvmt, ptr, 名)
                .map_err(|e| e.to_string())?;
            // ARC：RC 值（字符串/结构体/数组/闭包）捕获进闭包 env（借用 load）
            // → retain（env 持有一份；env 归零时由下面挂的 dtor 级联归还）。
            // 弱捕获（unowned）：不 retain —— env 不顶住对象计数，环被打破。
            if !弱名单.contains(名) {
                self.弧retain任意(v, *t);
            }
            self.闭包填槽(obj, i as u64, v, *t)?;
        }
        // ARC（E1）：有 强 RC 捕获 → 合成 env dtor 并挂到 fat obj（refcount 归零
        // 时 runtime 调 dtor 释放捕获，再 free env 本体）。弱捕获不入 dtor（未 retain）。
        self.弧挂env析构(obj, &符号名, &捕获, &弱名单)?;

        Ok((obj.into(), Qi类型::函数值(idx)))
    }

    /// 把一段语句体合成为 **nullary 闭包**（无参无返回），返回其 fat obj 指针。
    /// 复用闭包机制：扫自由变量 → 登记待合成闭包（body 稍后统一生成）→ 建 fat obj + 填捕获。
    /// 供 `启动`（协程）复用：协程体就是执行某表达式并丢弃结果。
    pub(super) fn 合成nullary闭包(
        &mut self,
        body: Vec<AstNode>,
    ) -> Result<PointerValue<'ctx>, String> {
        // 扫自由变量（捕获）：body 内引用、且当前作用域确有记录、非 body 内声明。
        let 空参: HashSet<String> = HashSet::new();
        let mut 本地声明: HashSet<String> = HashSet::new();
        for s in &body {
            self.收集本地声明(s, &mut 本地声明);
        }
        let mut 自由: Vec<String> = Vec::new();
        let mut 已见: HashSet<String> = HashSet::new();
        for s in &body {
            self.收集自由标识符(s, &空参, &本地声明, &mut 自由, &mut 已见);
        }
        let 捕获: Vec<(String, Qi类型)> = 自由
            .into_iter()
            .filter_map(|n| self.变量表.get(&n).map(|(_, t)| (n, *t)))
            .collect();

        let 符号名 = format!("__closure_{}", self.闭包计数);
        self.闭包计数 += 1;
        self.待合成闭包.push(待合成闭包 {
            符号名: 符号名.clone(),
            参数类型: Vec::new(),
            参数名: Vec::new(),
            返回类型: Qi类型::空,
            捕获: 捕获.clone(),
            弱名单: std::collections::HashSet::new(), // nullary 协程闭包无捕获列表语法
            body,
            包: self.当前包.clone(),
        });

        let fn_val = self.声明闭包原型(&符号名, &捕获, self.待合成闭包.last().unwrap())?;
        let obj = self.创建闭包对象(fn_val, 捕获.len() as u64)?;

        for (i, (名, t)) in 捕获.iter().enumerate() {
            let (ptr, _) = self
                .变量表
                .get(名)
                .cloned()
                .ok_or_else(|| format!("捕获变量 {} 缺失", 名))?;
            let llvmt = self
                .llvm基础类型(*t)
                .unwrap_or_else(|| self.ctx.i64_type().into());
            let v = self
                .builder
                .build_load(llvmt, ptr, 名)
                .map_err(|e| e.to_string())?;
            // ARC：RC 捕获（含 spawn/协程捕获 —— 发送即转移）→ retain，
            // 同 生成闭包表达式
            self.弧retain任意(v, *t);
            self.闭包填槽(obj, i as u64, v, *t)?;
        }
        // ARC（E1）：挂 env dtor（goroutine 跑完 qi_go_tramp 尾部 release env
        // → 归零时级联释放捕获）。nullary 闭包无弱捕获。
        self.弧挂env析构(obj, &符号名, &捕获, &std::collections::HashSet::new())?;
        Ok(obj)
    }

    /// ARC（E1）：为闭包 env 合成析构函数 `qi.env.drop.<闭包符号名>` 并挂到
    /// fat obj 末槽（qi_closure_set_dtor）。dtor 只负责释放 RC 捕获槽 ——
    /// dec / free 本体由 runtime 的 qi_closure_release 统一管。
    /// 无 RC 捕获 / ARC 关 → 不合成不挂（dtor 槽保持 0，release 直接 free）。
    fn 弧挂env析构(
        &mut self,
        obj: PointerValue<'ctx>,
        闭包符号名: &str,
        捕获: &[(String, Qi类型)],
        弱名单: &std::collections::HashSet<String>,
    ) -> Result<(), String> {
        if !self.弧开() {
            return Ok(());
        }
        // 弱捕获（unowned）不入 dtor —— 填槽时未 retain，dtor 释放会 over-release。
        let rc捕获: Vec<(usize, Qi类型)> = 捕获
            .iter()
            .enumerate()
            .filter(|(_, (名, t))| super::所有权::是RC类型(*t) && !弱名单.contains(名))
            .map(|(i, (_, t))| (i, *t))
            .collect();
        if rc捕获.is_empty() {
            return Ok(());
        }

        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let i64t = self.ctx.i64_type();
        let dtor名 = format!("qi.env.drop.{}", 闭包符号名);
        let dtor = match self.module.get_function(&dtor名) {
            Some(f) => f,
            None => {
                let f = self.module.add_function(
                    &dtor名,
                    self.ctx.void_type().fn_type(&[ptrt.into()], false),
                    None,
                );
                // 生成 dtor 体（保存/恢复 builder 插入点，与 生成trampoline 同法）
                let 保存 = self.builder.get_insert_block();
                let entry = self.ctx.append_basic_block(f, "entry");
                self.builder.position_at_end(entry);
                let env = f.get_nth_param(0).unwrap().into_pointer_value();
                let getp = self.module.get_function("qi_closure_get_ptr").unwrap();
                for (i, t) in &rc捕获 {
                    let idxv = i64t.const_int(*i as u64, false);
                    let v = self
                        .builder
                        .build_call(getp, &[env.into(), idxv.into()], "cap")
                        .map_err(|e| e.to_string())?
                        .try_as_basic_value()
                        .basic()
                        .ok_or_else(|| "get_ptr 未返回".to_string())?;
                    self.弧release任意(v, *t);
                }
                self.builder.build_return(None).map_err(|e| e.to_string())?;
                if let Some(bb) = 保存 {
                    self.builder.position_at_end(bb);
                }
                f
            }
        };

        let set = self
            .module
            .get_function("qi_closure_set_dtor")
            .ok_or_else(|| "运行时函数未声明: qi_closure_set_dtor".to_string())?;
        self.builder
            .build_call(
                set,
                &[obj.into(), dtor.as_global_value().as_pointer_value().into()],
                "",
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 间接调用（fat call）：callee 是持 fat obj 的变量/全局/参数。
    pub(super) fn 尝试间接调用(
        &mut self,
        callee: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        // 定位 fat obj 指针 + 函数值签名
        let (obj_ptr, sig) = if let Some((ptr, t)) = self.变量表.get(callee).cloned() {
            match t.函数值索引() {
                Some(idx) => {
                    let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
                    let obj = self
                        .builder
                        .build_load(ptrt, ptr, callee)
                        .map_err(|e| e.to_string())?
                        .into_pointer_value();
                    (obj, self.符号.函数值签名(idx).cloned().unwrap())
                }
                None => return Ok(None),
            }
        } else if let Some((g, t)) = self.全局变量表.get(callee).map(|(g, t)| (*g, *t)) {
            match t.函数值索引() {
                Some(idx) => {
                    let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
                    let obj = self
                        .builder
                        .build_load(ptrt, g.as_pointer_value(), callee)
                        .map_err(|e| e.to_string())?
                        .into_pointer_value();
                    (obj, self.符号.函数值签名(idx).cloned().unwrap())
                }
                None => return Ok(None),
            }
        } else {
            return Ok(None);
        };

        let r = self.发射fat调用(obj_ptr, &sig, arguments)?;
        Ok(Some(r))
    }

    /// fat 调用：`f = qi_closure_get_fn(obj); f(obj, args...)`。
    pub(super) fn 发射fat调用(
        &mut self,
        obj: PointerValue<'ctx>,
        sig: &函数签名,
        arguments: &[AstNode],
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let getfn = self.module.get_function("qi_closure_get_fn").unwrap();
        let fptr = self
            .builder
            .build_call(getfn, &[obj.into()], "getfn")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "get_fn 未返回".to_string())?
            .into_pointer_value();

        // fn 类型：ret (ptr env, 参数...)
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let mut 参数llvm: Vec<BasicMetadataTypeEnum> = vec![ptrt.into()];
        for t in &sig.参数 {
            参数llvm.push(
                self.llvm基础类型(*t)
                    .map(|b| b.into())
                    .unwrap_or_else(|| self.ctx.i64_type().into()),
            );
        }
        let fn_type = match self.llvm基础类型(sig.返回) {
            Some(rt) => rt.fn_type(&参数llvm, false),
            None => self.ctx.void_type().fn_type(&参数llvm, false),
        };

        let mut args: Vec<BasicMetadataValueEnum> = vec![obj.into()];
        let mut 弧待释放: Vec<(BasicValueEnum, Qi类型)> = Vec::new();
        for (i, a) in arguments.iter().enumerate() {
            let (mut v, at) = self
                .生成表达式(a)?
                .ok_or_else(|| "fat 调用实参无值".to_string())?;
            // ARC：实参借用传递，OWNED RC 临时调用后释放。结构体/数组要求
            // 形参声明类型也是 RC（否则被调方不套 retain 纪律 —— 保守不释放）。
            let 形参rc = sig
                .参数
                .get(i)
                .map(|pt| super::所有权::是RC类型(*pt))
                .unwrap_or(false);
            let 可释放 = match at {
                Qi类型::字符串 => self.表达式拥有字符串(a),
                Qi类型::结构体(_) | Qi类型::数组(_) | Qi类型::函数值(_) => {
                    形参rc && self.表达式拥有RC(a, at)
                }
                _ => false,
            };
            if self.弧开() && v.is_pointer_value() && 可释放 {
                弧待释放.push((v, at));
            }
            if let Some(pt) = sig.参数.get(i) {
                if pt.是浮点() && !at.是浮点() {
                    v = self
                        .builder
                        .build_signed_int_to_float(
                            v.into_int_value(),
                            self.ctx.f64_type(),
                            "sitofp",
                        )
                        .map_err(|e| e.to_string())?
                        .into();
                }
            }
            args.push(v.into());
        }

        let cs = self
            .builder
            .build_indirect_call(fn_type, fptr, &args, "fatcall")
            .map_err(|e| e.to_string())?;
        for (v, vt) in 弧待释放 {
            self.弧release任意(v, vt);
        }
        match cs.try_as_basic_value().basic() {
            Some(v) => Ok(Some((v, sig.返回))),
            None => Ok(None),
        }
    }

    /// 生成所有待合成闭包的函数体（循环到清空；闭包体内可能再产生闭包）。
    pub(super) fn 合成待处理闭包(&mut self) -> Result<(), String> {
        while let Some(cl) = self.待合成闭包.pop() {
            self.合成一个闭包(cl)?;
        }
        Ok(())
    }

    // ───────── 内部工具 ─────────

    /// 为顶层函数生成 trampoline（吃掉 env 首参，转发到真函数）。幂等。
    fn 生成trampoline(
        &mut self,
        mangled: &str,
        sig: &函数签名,
    ) -> Result<PointerValue<'ctx>, String> {
        let tramp_name = format!("__tramp_{}", mangled);
        if let Some(f) = self.module.get_function(&tramp_name) {
            return Ok(f.as_global_value().as_pointer_value());
        }
        let real = self
            .module
            .get_function(mangled)
            .ok_or_else(|| format!("trampoline 目标缺失: {}", mangled))?;

        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let mut 参数llvm: Vec<BasicMetadataTypeEnum> = vec![ptrt.into()];
        for t in &sig.参数 {
            参数llvm.push(
                self.llvm基础类型(*t)
                    .map(|b| b.into())
                    .unwrap_or_else(|| self.ctx.i64_type().into()),
            );
        }
        let fn_type = match self.llvm基础类型(sig.返回) {
            Some(rt) => rt.fn_type(&参数llvm, false),
            None => self.ctx.void_type().fn_type(&参数llvm, false),
        };
        let tramp = self.module.add_function(&tramp_name, fn_type, None);

        // 保存 builder 位置，生成 tramp 体，再恢复
        let 保存 = self.builder.get_insert_block();
        let entry = self.ctx.append_basic_block(tramp, "entry");
        self.builder.position_at_end(entry);

        // 转发 args（跳过 env=param0）
        let mut fwd: Vec<BasicMetadataValueEnum> = Vec::new();
        for i in 0..sig.参数.len() {
            let p = tramp
                .get_nth_param((i + 1) as u32)
                .ok_or_else(|| "trampoline 缺形参".to_string())?;
            fwd.push(p.into());
        }
        let cs = self
            .builder
            .build_call(real, &fwd, "fwd")
            .map_err(|e| e.to_string())?;
        match cs.try_as_basic_value().basic() {
            Some(v) => {
                self.builder
                    .build_return(Some(&v))
                    .map_err(|e| e.to_string())?;
            }
            None => {
                self.builder.build_return(None).map_err(|e| e.to_string())?;
            }
        }

        if let Some(bb) = 保存 {
            self.builder.position_at_end(bb);
        }
        Ok(tramp.as_global_value().as_pointer_value())
    }

    /// 声明闭包合成函数原型：ret (ptr env, 参数...)。
    fn 声明闭包原型(
        &self,
        符号名: &str,
        _捕获: &[(String, Qi类型)],
        cl: &待合成闭包,
    ) -> Result<PointerValue<'ctx>, String> {
        if let Some(f) = self.module.get_function(符号名) {
            return Ok(f.as_global_value().as_pointer_value());
        }
        let ptrt = self.ctx.ptr_type(inkwell::AddressSpace::default());
        let mut 参数llvm: Vec<BasicMetadataTypeEnum> = vec![ptrt.into()];
        for t in &cl.参数类型 {
            参数llvm.push(
                self.llvm基础类型(*t)
                    .map(|b| b.into())
                    .unwrap_or_else(|| self.ctx.i64_type().into()),
            );
        }
        let fn_type = match self.llvm基础类型(cl.返回类型) {
            Some(rt) => rt.fn_type(&参数llvm, false),
            None => self.ctx.void_type().fn_type(&参数llvm, false),
        };
        let f = self.module.add_function(符号名, fn_type, None);
        Ok(f.as_global_value().as_pointer_value())
    }

    /// 生成一个闭包的函数体（序言从 env 读捕获注入同名局部 + body）。
    fn 合成一个闭包(&mut self, cl: 待合成闭包) -> Result<(), String> {
        let func = self
            .module
            .get_function(&cl.符号名)
            .ok_or_else(|| format!("闭包原型缺失: {}", cl.符号名))?;

        // 保存 builder / 变量表 / 返回类型 / try 深度 / 包上下文（合成函数独立环境）
        let 保存位置 = self.builder.get_insert_block();
        let 保存变量 = std::mem::take(&mut self.变量表);
        let 保存弱局部 = std::mem::take(&mut self.弱局部);
        let 保存遮蔽栈 = std::mem::take(&mut self.作用域遮蔽栈);
        let 保存隐藏槽 = std::mem::take(&mut self.弧隐藏引用计数槽);
        let 保存返回 = self.当前返回类型;
        let 保存try深度 = self.try深度;
        let 保存包 = self.当前包.clone();
        self.设当前包(cl.包.clone());

        self.符号.进入作用域();
        self.当前返回类型 = cl.返回类型;
        self.try深度 = 0; // E4：闭包体的异常 frame 独立平衡

        let entry = self.ctx.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        // 剖析：序言计时（QI_PROF 关时空操作），显示名 = 闭包合成符号（__closure_N，天然唯一）
        let 剖析显示名 = cl.符号名.clone();
        self.剖析入口(&剖析显示名)?;

        let env = func.get_nth_param(0).unwrap().into_pointer_value();

        // 从 env 读捕获 → alloca 同名局部
        for (i, (名, t)) in cl.捕获.iter().enumerate() {
            let 是弱 = cl.弱名单.contains(名);
            let v = self.闭包读槽(env, i as u64, *t)?;
            let llvmt = self
                .llvm基础类型(*t)
                .unwrap_or_else(|| self.ctx.i64_type().into());
            let a = self
                .builder
                .build_alloca(llvmt, 名)
                .map_err(|e| e.to_string())?;
            self.builder.build_store(a, v).map_err(|e| e.to_string())?;
            // ARC：RC 捕获（env 里的借用）落地进局部槽 → retain，与出口释放平衡。
            // 弱捕获（unowned）：不 retain，且记入 弱局部 让出口 弧释放局部 跳过它
            // —— 借裸指针用，不参与计数，对象存活由环结构（持有者活着闭包才被调）保证。
            if 是弱 {
                self.弱局部.insert(名.clone());
            } else {
                self.弧retain任意(v, *t);
            }
            self.变量表.insert(名.clone(), (a, *t));
            self.符号.声明变量(名, *t);
        }

        // 参数（从 param1.. 落地）
        for (i, 名) in cl.参数名.iter().enumerate() {
            let t = cl.参数类型.get(i).copied().unwrap_or(Qi类型::整数);
            let llvmt = self
                .llvm基础类型(t)
                .unwrap_or_else(|| self.ctx.i64_type().into());
            let a = self
                .builder
                .build_alloca(llvmt, 名)
                .map_err(|e| e.to_string())?;
            let p = func
                .get_nth_param((i + 1) as u32)
                .ok_or_else(|| "闭包缺形参".to_string())?;
            self.builder.build_store(a, p).map_err(|e| e.to_string())?;
            // ARC：RC 参数（借用）→ retain，与出口释放平衡
            self.弧retain任意(p, t);
            self.变量表.insert(名.clone(), (a, t));
            self.符号.声明变量(名, t);
        }

        for stmt in &cl.body {
            self.生成语句(stmt, func)?;
            if self.当前块已终结() {
                break;
            }
        }
        if !self.当前块已终结() {
            self.剖析出口()?; // 剖析：闭包落底出口计时
            self.弧释放局部()?; // ARC：闭包落底出口释放字符串局部
            match self.llvm基础类型(cl.返回类型) {
                Some(rt) => {
                    let z = rt.const_zero();
                    self.builder
                        .build_return(Some(&z))
                        .map_err(|e| e.to_string())?;
                }
                None => {
                    self.builder.build_return(None).map_err(|e| e.to_string())?;
                }
            }
        }

        self.符号.退出作用域();
        self.变量表 = 保存变量;
        self.弱局部 = 保存弱局部;
        self.作用域遮蔽栈 = 保存遮蔽栈;
        self.弧隐藏引用计数槽 = 保存隐藏槽;
        self.当前返回类型 = 保存返回;
        self.try深度 = 保存try深度;
        self.设当前包(保存包);
        if let Some(bb) = 保存位置 {
            self.builder.position_at_end(bb);
        }
        Ok(())
    }

    /// qi_closure_create(fn_ptr, num_caps) → fat obj ptr。
    fn 创建闭包对象(
        &mut self,
        fn_ptr: PointerValue<'ctx>,
        num_caps: u64,
    ) -> Result<PointerValue<'ctx>, String> {
        let create = self.module.get_function("qi_closure_create").unwrap();
        let n = self.ctx.i64_type().const_int(num_caps, false);
        let cs = self
            .builder
            .build_call(create, &[fn_ptr.into(), n.into()], "clo")
            .map_err(|e| e.to_string())?;
        cs.try_as_basic_value()
            .basic()
            .map(|v| v.into_pointer_value())
            .ok_or_else(|| "closure_create 未返回".to_string())
    }

    /// 往 fat obj 填一个捕获槽（int 走 set_int，ptr 语义走 set_ptr）。
    fn 闭包填槽(
        &mut self,
        obj: PointerValue<'ctx>,
        idx: u64,
        v: BasicValueEnum<'ctx>,
        t: Qi类型,
    ) -> Result<(), String> {
        let idxv = self.ctx.i64_type().const_int(idx, false);
        if 指针语义(t) {
            let set = self.module.get_function("qi_closure_set_ptr").unwrap();
            let pv = if v.is_pointer_value() {
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
            self.builder
                .build_call(set, &[obj.into(), idxv.into(), pv.into()], "setp")
                .map_err(|e| e.to_string())?;
        } else {
            let set = self.module.get_function("qi_closure_set_int").unwrap();
            let iv = if v.is_int_value() {
                let iv = v.into_int_value();
                if iv.get_type().get_bit_width() != 64 {
                    self.builder
                        .build_int_z_extend(iv, self.ctx.i64_type(), "z")
                        .map_err(|e| e.to_string())?
                } else {
                    iv
                }
            } else {
                return Err("非整数捕获走了 int 槽".to_string());
            };
            self.builder
                .build_call(set, &[obj.into(), idxv.into(), iv.into()], "seti")
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 从 fat obj 读一个捕获槽。
    fn 闭包读槽(
        &mut self,
        env: PointerValue<'ctx>,
        idx: u64,
        t: Qi类型,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let idxv = self.ctx.i64_type().const_int(idx, false);
        if 指针语义(t) {
            let get = self.module.get_function("qi_closure_get_ptr").unwrap();
            let cs = self
                .builder
                .build_call(get, &[env.into(), idxv.into()], "getp")
                .map_err(|e| e.to_string())?;
            Ok(cs.try_as_basic_value().basic().unwrap())
        } else {
            let get = self.module.get_function("qi_closure_get_int").unwrap();
            let cs = self
                .builder
                .build_call(get, &[env.into(), idxv.into()], "geti")
                .map_err(|e| e.to_string())?;
            let mut v = cs.try_as_basic_value().basic().unwrap().into_int_value();
            // 布尔捕获：截回 i1
            if t == Qi类型::布尔 {
                v = self
                    .builder
                    .build_int_truncate(v, self.ctx.bool_type(), "trunc")
                    .map_err(|e| e.to_string())?;
            }
            Ok(v.into())
        }
    }

    // ───────── 自由变量扫描 ─────────

    fn 收集本地声明(&self, node: &AstNode, out: &mut HashSet<String>) {
        if let AstNode::变量声明(vd) = node {
            out.insert(vd.name.clone());
        }
    }

    #[allow(clippy::only_used_in_recursion)]
    fn 收集自由标识符(
        &self,
        node: &AstNode,
        参数: &HashSet<String>,
        本地: &HashSet<String>,
        out: &mut Vec<String>,
        已见: &mut HashSet<String>,
    ) {
        match node {
            AstNode::标识符表达式(id) => {
                let n = &id.name;
                if 参数.contains(n) || 本地.contains(n) || 已见.contains(n) {
                    return;
                }
                // 只捕获当前作用域里确有的局部变量（函数名/模块名不捕获）
                if self.变量表.contains_key(n) {
                    已见.insert(n.clone());
                    out.push(n.clone());
                }
            }
            AstNode::二元操作表达式(b) => {
                self.收集自由标识符(&b.left, 参数, 本地, out, 已见);
                self.收集自由标识符(&b.right, 参数, 本地, out, 已见);
            }
            AstNode::一元操作表达式(u) => {
                self.收集自由标识符(&u.operand, 参数, 本地, out, 已见)
            }
            AstNode::字符串连接表达式(s) => {
                self.收集自由标识符(&s.left, 参数, 本地, out, 已见);
                self.收集自由标识符(&s.right, 参数, 本地, out, 已见);
            }
            AstNode::赋值表达式(a) => {
                self.收集自由标识符(&a.target, 参数, 本地, out, 已见);
                self.收集自由标识符(&a.value, 参数, 本地, out, 已见);
            }
            AstNode::函数调用表达式(c) => {
                // callee 也可能是被捕获的函数值变量
                if self.变量表.contains_key(&c.callee)
                    && !参数.contains(&c.callee)
                    && !本地.contains(&c.callee)
                    && !已见.contains(&c.callee)
                {
                    已见.insert(c.callee.clone());
                    out.push(c.callee.clone());
                }
                for a in &c.arguments {
                    self.收集自由标识符(a, 参数, 本地, out, 已见);
                }
            }
            AstNode::方法调用表达式(m) => {
                self.收集自由标识符(&m.object, 参数, 本地, out, 已见);
                for a in &m.arguments {
                    self.收集自由标识符(a, 参数, 本地, out, 已见);
                }
            }
            AstNode::字段访问表达式(f) => {
                self.收集自由标识符(&f.object, 参数, 本地, out, 已见)
            }
            AstNode::表达式语句(e) => {
                self.收集自由标识符(&e.expression, 参数, 本地, out, 已见)
            }
            AstNode::返回语句(r) => {
                if let Some(v) = &r.value {
                    self.收集自由标识符(v, 参数, 本地, out, 已见);
                }
            }
            AstNode::变量声明(vd) => {
                if let Some(init) = &vd.initializer {
                    self.收集自由标识符(init, 参数, 本地, out, 已见);
                }
            }
            AstNode::如果语句(i) => {
                self.收集自由标识符(&i.condition, 参数, 本地, out, 已见);
                for s in &i.then_branch {
                    self.收集自由标识符(s, 参数, 本地, out, 已见);
                }
                if let Some(e) = &i.else_branch {
                    self.收集自由标识符(e, 参数, 本地, out, 已见);
                }
            }
            AstNode::当语句(w) => {
                self.收集自由标识符(&w.condition, 参数, 本地, out, 已见);
                for s in &w.body {
                    self.收集自由标识符(s, 参数, 本地, out, 已见);
                }
            }
            AstNode::对于语句(f) => {
                self.收集自由标识符(&f.range, 参数, 本地, out, 已见);
                for s in &f.body {
                    self.收集自由标识符(s, 参数, 本地, out, 已见);
                }
            }
            AstNode::块语句(b) => {
                for s in &b.statements {
                    self.收集自由标识符(s, 参数, 本地, out, 已见);
                }
            }
            _ => {}
        }
    }
}

/// 该类型是否按指针存进 fat 槽（字符串/结构体/函数值/未知句柄按 ptr；整数/布尔按 int）。
fn 指针语义(t: Qi类型) -> bool {
    matches!(
        t,
        Qi类型::字符串
            | Qi类型::结构体(_)
            | Qi类型::函数值(_)
            | Qi类型::数组(_)
            | Qi类型::装箱枚举(_)
            | Qi类型::通道(_)
            | Qi类型::未来(_)
    )
}
