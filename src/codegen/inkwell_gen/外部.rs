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

    /// C ABI 参数类型校验。v2 支持：整数/浮点数/布尔/字符串/指针/小结构体/函数值。
    ///   - 指针：不透明 void*（malloc/free/上下文句柄）。
    ///   - 结构体：仅「全整数/浮点字段、总大小 ≤16 字节」的小结构体按值传（C ABI）。
    ///   - 函数值：C 回调（裸函数指针）—— 仅无捕获顶层函数可传，实参处校验。
    fn 校验外部参数类型(
        &self,
        函数名: &str,
        参数名: &str,
        t: Qi类型,
    ) -> Result<(), String> {
        match t {
            Qi类型::整数
            | Qi类型::浮点数
            | Qi类型::布尔
            | Qi类型::字符串
            | Qi类型::指针
            | Qi类型::函数值(_) => Ok(()),
            Qi类型::结构体(idx) => self.校验小结构体(函数名, idx).map(|_| ()),
            _ => Err(format!(
                "外部函数 {} 的参数 {} 类型不被 C FFI 支持 —— \
                 参数仅支持 整数/浮点数/布尔/字符串/指针/小结构体/函数指针",
                函数名, 参数名
            )),
        }
    }

    /// C ABI 返回类型校验。v2 支持：整数/浮点数/布尔/空/指针/字符串(char*)/小结构体。
    ///   - 字符串(char*) 返回：调用点拷贝进 Qi 拥有的堆串（rc=1），全程 ARC 安全；
    ///     原 C 内存不碰（getenv 静态串安全；C malloc 的原串需用户按 指针 手动 free）。
    fn 校验外部返回类型(&self, 函数名: &str, t: Qi类型) -> Result<(), String> {
        match t {
            Qi类型::整数
            | Qi类型::浮点数
            | Qi类型::布尔
            | Qi类型::空
            | Qi类型::指针
            | Qi类型::字符串 => Ok(()),
            Qi类型::结构体(idx) => self.校验小结构体(函数名, idx).map(|_| ()),
            _ => Err(format!(
                "外部函数 {} 的返回类型不被 C FFI 支持 —— \
                 返回仅支持 整数/浮点数/布尔/空/指针/字符串(char*)/小结构体",
                函数名
            )),
        }
    }

    /// 小结构体按值 C ABI 校验：字段全为 整数/浮点数/布尔，且总大小 ≤16 字节
    /// （每字段按 8 字节计 —— Qi 结构体字段全 8 字节槽）。返回 8 字节字段数（≤2）。
    /// 超范围报人话错误（大结构体 / 含指针·字符串·嵌套结构体字段留后续）。
    fn 校验小结构体(&self, 函数名: &str, idx: u32) -> Result<usize, String> {
        let info = self
            .符号
            .结构体信息(idx)
            .ok_or_else(|| format!("外部函数 {} 引用了未知结构体", 函数名))?;
        for (名, ft) in info.字段名.iter().zip(info.字段类型.iter()) {
            match ft {
                Qi类型::整数 | Qi类型::浮点数 | Qi类型::布尔 => {}
                _ => {
                    return Err(format!(
                        "外部函数 {} 的结构体 {} 字段 {} 类型不支持按值传 C —— \
                         v2 小结构体字段仅支持 整数/浮点数/布尔",
                        函数名, info.名字, 名
                    ))
                }
            }
        }
        let 字段数 = info.字段名.len();
        if 字段数 == 0 || 字段数 > 2 {
            return Err(format!(
                "外部函数 {} 的结构体 {} 有 {} 个字段（{} 字节）—— \
                 v2 按值传 C 仅支持 1~2 个 8 字节字段的小结构体（≤16 字节）。\
                 更大的结构体请改用 指针 传递。",
                函数名,
                info.名字,
                字段数,
                字段数 * 8
            ));
        }
        Ok(字段数)
    }

    /// Qi 类型 → C ABI 的 LLVM 基础类型（布尔按 i64 传，C 侧当 int）。
    /// 仅对已通过校验的类型调用。
    fn 外部llvm类型(&self, t: Qi类型) -> inkwell::types::BasicTypeEnum<'ctx> {
        match t {
            Qi类型::整数 | Qi类型::布尔 => self.ctx.i64_type().into(),
            Qi类型::浮点数 => self.ctx.f64_type().into(),
            Qi类型::字符串 | Qi类型::指针 | Qi类型::函数值(_) => {
                self.ctx.ptr_type(AddressSpace::default()).into()
            }
            // 小结构体按值：用具名 LLVM struct 类型（{i64/f64 ...}），由 LLVM 走
            // SysV/AArch64 小结构体寄存器传参（≤16B INTEGER/SSE 类）。
            Qi类型::结构体(idx) => self.外部结构体llvm(idx).into(),
            // 校验已排除其余类型；防御性回退 i64。
            _ => self.ctx.i64_type().into(),
        }
    }

    /// 小结构体的按值 C ABI LLVM 类型：字段逐个映射为 i64/f64 组成的匿名 struct。
    /// 布尔按 i64（C int 宽化到 8 字节槽）。仅对已过 校验小结构体 的索引调用。
    fn 外部结构体llvm(&self, idx: u32) -> inkwell::types::StructType<'ctx> {
        let i64t = self.ctx.i64_type();
        let f64t = self.ctx.f64_type();
        let 字段: Vec<inkwell::types::BasicTypeEnum<'ctx>> = self
            .符号
            .结构体信息(idx)
            .map(|info| {
                info.字段类型
                    .iter()
                    .map(|ft| match ft {
                        Qi类型::浮点数 => f64t.into(),
                        _ => i64t.into(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.ctx.struct_type(&字段, false)
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
            match pt {
                // 小结构体按值：从 Qi 堆结构体 load 出字段，组成 LLVM struct 值传入。
                Qi类型::结构体(idx) => {
                    let sv = self.外部装配结构体实参(a, idx)?;
                    args.push(sv.into());
                    continue;
                }
                // C 回调：取无捕获顶层函数的裸 LLVM 函数指针（fat 闭包不行）。
                Qi类型::函数值(_) => {
                    let fp = self.外部函数指针实参(a, &call.callee)?;
                    args.push(fp.into());
                    continue;
                }
                _ => {}
            }
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
            // char* 返回：C 给的裸指针拷贝进 Qi 拥有的堆串（rc=1，带 magic header），
            // 全程 ARC 安全（原 C 内存不碰）。null → 空串。
            Qi类型::字符串 => {
                let raw = cs
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "外部函数应返回 char*".to_string())?;
                let copyf = self
                    .module
                    .get_function("qi_string_from_cstr")
                    .ok_or_else(|| "运行时函数未声明: qi_string_from_cstr".to_string())?;
                let owned = self
                    .builder
                    .build_call(copyf, &[raw.into()], "cstr2qi")
                    .map_err(|e| e.to_string())?
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "qi_string_from_cstr 未返回".to_string())?;
                Ok(Some((owned, Qi类型::字符串)))
            }
            // 小结构体按值返回：C 给的 LLVM struct 值 store 进新 qi_obj 堆结构体（带 ARC header）。
            Qi类型::结构体(idx) => {
                let rv = cs
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "外部函数应返回结构体".to_string())?;
                let p = self.外部拆解结构体返回(rv, idx)?;
                Ok(Some((p.into(), Qi类型::结构体(idx))))
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

    /// 小结构体实参装配：a 求值成 Qi 堆结构体(idx)，逐字段 load 组成按值 LLVM struct。
    /// 布尔字段 i1→i64 加宽；整数/浮点直读。
    fn 外部装配结构体实参(
        &mut self,
        a: &crate::parser::ast::AstNode,
        idx: u32,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let (v, vt) = self
            .生成带期望(a, Some(Qi类型::结构体(idx)))?
            .ok_or_else(|| "外部结构体实参无值".to_string())?;
        let 实idx = vt.结构体索引().ok_or_else(|| {
            format!("外部函数结构体实参类型不符（期望结构体 {}，实为 {:?}）", idx, vt)
        })?;
        if 实idx != idx {
            return Err(format!(
                "外部函数结构体实参类型不符（期望结构体索引 {}，实为 {}）",
                idx, 实idx
            ));
        }
        let base = v.into_pointer_value();
        let heap_st = self.取结构体llvm(idx)?;
        let byval_ty = self.外部结构体llvm(idx);
        let i64t = self.ctx.i64_type();
        let 字段类型 = self
            .符号
            .结构体信息(idx)
            .map(|s| s.字段类型.clone())
            .unwrap_or_default();
        let 字段名 = self
            .符号
            .结构体信息(idx)
            .map(|s| s.字段名.clone())
            .unwrap_or_default();
        let mut agg = byval_ty.get_undef();
        for (fi, ft) in 字段类型.iter().enumerate() {
            let name = 字段名.get(fi).map(|s| s.as_str()).unwrap_or("f");
            let fptr = self.字段指针(base, heap_st, fi as u32, name)?;
            let llvmt = self
                .llvm基础类型(*ft)
                .ok_or_else(|| "结构体字段类型无效".to_string())?;
            let mut fv = self
                .builder
                .build_load(llvmt, fptr, "sf")
                .map_err(|e| e.to_string())?;
            if *ft == Qi类型::布尔 {
                let iv = fv.into_int_value();
                if iv.get_type().get_bit_width() < 64 {
                    fv = self
                        .builder
                        .build_int_z_extend(iv, i64t, "b2i64")
                        .map_err(|e| e.to_string())?
                        .into();
                }
            }
            agg = self
                .builder
                .build_insert_value(agg, fv, fi as u32, "ins")
                .map_err(|e| e.to_string())?
                .into_struct_value();
        }
        Ok(agg.into())
    }

    /// C 回调实参：只接受无捕获的顶层函数名，取其裸 LLVM 函数指针。
    /// 有捕获闭包 / 闭包变量 → 人话报错（C 无处放 env）。
    fn 外部函数指针实参(
        &mut self,
        a: &crate::parser::ast::AstNode,
        callee: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        use crate::parser::ast::AstNode;
        if let AstNode::标识符表达式(id) = a {
            // 必须是顶层函数（不是持有闭包的局部/全局变量）
            let 是局部 = self.变量表.contains_key(&id.name)
                || self.全局变量表.contains_key(&id.name);
            if !是局部 {
                if let Some((f, _)) = self.尝试解析用户函数(&id.name) {
                    return Ok(f.as_global_value().as_pointer_value());
                }
            }
            return Err(format!(
                "外部函数 {} 的回调实参 {} 不是无捕获顶层函数 —— \
                 C 回调只能传裸函数指针，有捕获的闭包 / 闭包变量无处安放 env。\
                 请传一个顶层 函数 名。",
                callee, id.name
            ));
        }
        Err(format!(
            "外部函数 {} 的回调实参必须是无捕获顶层函数名（不能是闭包字面量 / 表达式）。",
            callee
        ))
    }

    /// 小结构体返回拆解：C 给的按值 LLVM struct 存进新 qi_obj 堆结构体（带 ARC header），
    /// 返回堆指针（类型 结构体(idx)，OWNED rc=1）。布尔字段 i64→i1 截断。
    fn 外部拆解结构体返回(
        &mut self,
        rv: BasicValueEnum<'ctx>,
        idx: u32,
    ) -> Result<inkwell::values::PointerValue<'ctx>, String> {
        let heap_st = self.取结构体llvm(idx)?;
        let 字段类型 = self
            .符号
            .结构体信息(idx)
            .map(|s| s.字段类型.clone())
            .unwrap_or_default();
        let 字段名 = self
            .符号
            .结构体信息(idx)
            .map(|s| s.字段名.clone())
            .unwrap_or_default();
        let size = self
            .ctx
            .i64_type()
            .const_int((字段类型.len() as u64) * 8, false);
        let alloc名 = if self.弧开() {
            "qi_obj_alloc"
        } else {
            "qi_runtime_alloc"
        };
        let alloc = self
            .module
            .get_function(alloc名)
            .ok_or_else(|| format!("运行时函数未声明: {}", alloc名))?;
        let base = self
            .builder
            .build_call(alloc, &[size.into()], "cstructmem")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "结构体分配未返回指针".to_string())?
            .into_pointer_value();
        let sv = rv.into_struct_value();
        for (fi, ft) in 字段类型.iter().enumerate() {
            let name = 字段名.get(fi).map(|s| s.as_str()).unwrap_or("f");
            let mut fv = self
                .builder
                .build_extract_value(sv, fi as u32, "ex")
                .map_err(|e| e.to_string())?;
            if *ft == Qi类型::布尔 {
                let iv = fv.into_int_value();
                let b = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        iv,
                        self.ctx.i64_type().const_zero(),
                        "c2bool",
                    )
                    .map_err(|e| e.to_string())?;
                fv = b.into();
            }
            let fptr = self.字段指针(base, heap_st, fi as u32, name)?;
            self.builder
                .build_store(fptr, fv)
                .map_err(|e| e.to_string())?;
        }
        Ok(base)
    }
}
