//! 反向 FFI —— 把 Qi 函数以 C ABI 导出为库符号，供 C / Rust / Go 等外部语言调用。
//!
//! `外部` 让 Qi 调 C；本模块做反方向：`导出 函数 加法(...)` 生成一个 C ABI 符号，
//! 转发到正常 mangle 的内部 Qi 函数。这样 Qi 函数体照常享有全部特性
//! （ARC / 异常 / 协程 / 调用其他 Qi 函数），C-ABI 的类型与所有权处理全部收在
//! 薄包装里，与 `外部` 的降级逻辑镜像对称。
//!
//! 设计要点：
//!   - C 符号名：`导出 "qi_add" 函数 ...` 显式指定；缺省用函数原名当符号
//!     （clang/gcc/rustc 均接受 UTF-8 标识符，中文名可直接从 C 侧调用）。
//!   - C ABI 类型（v1）：整数↔int64_t、浮点数↔double、布尔↔int64（0/1）、
//!     字符串↔const char*、指针↔void*。
//!   - 字符串参数：C 的 char* → qi_string_from_cstr 拷进 Qi 拥有堆串（rc=1），全程 ARC 安全。
//!   - 字符串返回：Qi 拥有串 → strdup 一份交给 C（C 负责 free()），随后 ARC 释放 Qi 串。
//!     strdup 复制永不 UAF（无论 Qi 侧 ARC 何时回收）；ARC 开时不泄漏，关时与普通 Qi 同基线。
//!   - 运行时初始化：emit 一个 @llvm.global_ctors 项，库加载时自动 qi_runtime_initialize()，
//!     使导出函数首次被 C 调用前协程/ARC/打印全部就绪（无需 C 侧手动 init）。

use super::后端;
use super::类型::Qi类型;
use crate::parser::ast::{AstNode, ExportFunction, Program};
use inkwell::module::Linkage;
use inkwell::types::BasicType;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;

/// 一条导出记录：C 符号 + 内部 Qi 符号 + C ABI 签名（用于生成包装 + 头文件）。
#[derive(Clone)]
pub(crate) struct 导出记录 {
    /// 对外 C 符号名（显式或函数原名）。
    pub c符号: String,
    /// 内部 Qi 函数的 mangle 符号（包内符号名），包装转发到它。
    pub qi内部符号: String,
    /// 函数中文原名（生成头文件注释用）。
    pub 原名: String,
    pub 参数名: Vec<String>,
    pub 参数: Vec<Qi类型>,
    /// C ABI 返回类型。异步导出时是 `未来<T>` 里的 **T**（已解包），
    /// 因此头文件 / 包装的返回类型都是标量（int64_t / double / char* …）。
    pub 返回: Qi类型,
    /// 内部 Qi 函数是否返回 `未来<T>`（async 桥）。为真时包装在转发调用后
    /// 追加一次阻塞 `qi_future_await_*`，把 future 解包成 T 再按 C ABI 返回。
    pub 是异步: bool,
}

impl<'ctx> 后端<'ctx> {
    /// 扫一个模块登记所有 `导出 函数`：建内部 Qi 原型（供 Qi 内部调用 + 附着函数体）
    /// + 记录导出元数据（供包装 / 头文件生成）。
    pub(super) fn 登记导出(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let AstNode::导出函数(ef) = stmt {
                self.登记导出函数(ef)?;
            }
        }
        Ok(())
    }

    fn 登记导出函数(&mut self, ef: &ExportFunction) -> Result<(), String> {
        let f =
            match ef.decl.as_ref() {
                AstNode::函数声明(f) => f,
                _ => return Err(
                    "只能导出普通函数（`导出 函数 名(...)`）—— 方法 / 内联函数不可导出为 C 符号"
                        .to_string(),
                ),
            };
        // 泛型 / 异步 / 变参 无法映射到固定 C ABI
        if !f.type_params.is_empty() {
            return Err(format!(
                "导出函数 {} 不能是泛型 —— C ABI 需要单一固定签名，请写具体类型。",
                f.name
            ));
        }
        if f.parameters.iter().any(|p| p.is_variadic) {
            return Err(format!(
                "导出函数 {} 使用了变参 `...` —— C ABI 导出暂不支持变参。",
                f.name
            ));
        }

        // 参数 / 返回类型校验（限 C ABI 可映射子集）
        let mut 参数类型: Vec<Qi类型> = Vec::new();
        let mut 参数名: Vec<String> = Vec::new();
        for p in &f.parameters {
            let t = p
                .type_annotation
                .as_ref()
                .map(|t| self.符号.解析类型(t))
                .ok_or_else(|| format!("导出函数 {} 的参数 {} 缺少类型注解", f.name, p.name))?;
            self.校验导出类型(&f.name, t, false)?;
            参数类型.push(t);
            参数名.push(p.name.clone());
        }
        let 声明返回 = f
            .return_type
            .as_ref()
            .map(|t| self.符号.解析类型(t))
            .unwrap_or(Qi类型::空);
        // async 桥：返回 `未来<T>` 的函数（不管有没有 `异步` 关键字）导出为
        // 「同步拿到 T」的 C 符号 —— 包装内部 spawn/调用后阻塞 await 出 T。
        // C ABI 返回类型是解包后的标量 T。
        let (返回, 是异步) = match 声明返回.未来内部() {
            Some(元素) => {
                let 内 = 元素.标量();
                if !matches!(
                    内,
                    Qi类型::整数 | Qi类型::浮点数 | Qi类型::布尔 | Qi类型::字符串
                ) {
                    return Err(format!(
                        "导出异步函数 {} 的 未来<T> 内部类型不被 C ABI 支持 —— \
                         v1 支持 未来<整数/浮点数/布尔/字符串>。",
                        f.name
                    ));
                }
                (内, true)
            }
            None => (声明返回, false),
        };
        self.校验导出类型(&f.name, 返回, true)?;

        // 建内部 Qi 函数原型（正常 mangle，Qi 内部可调用；导出包装再转发到它）。
        self.声明函数原型(f)?;
        self.符号.预登记函数值(&f.name);

        let c符号 = ef.c_name.clone().unwrap_or_else(|| f.name.clone());
        if self.导出表.iter().any(|r| r.c符号 == c符号) {
            return Err(format!(
                "C 导出符号 `{}` 重复 —— 每个导出符号只能定义一次。",
                c符号
            ));
        }
        let qi内部符号 = super::包内符号名(self.当前包.as_deref(), &f.name, f.parameters.len());
        self.导出表.push(导出记录 {
            c符号,
            qi内部符号,
            原名: f.name.clone(),
            参数名,
            参数: 参数类型,
            返回,
            是异步,
        });
        Ok(())
    }

    fn 校验导出类型(
        &self, 函数名: &str, t: Qi类型, 是返回: bool
    ) -> Result<(), String> {
        match t {
            Qi类型::整数 | Qi类型::浮点数 | Qi类型::布尔 | Qi类型::字符串 | Qi类型::指针 => {
                Ok(())
            }
            Qi类型::空 if 是返回 => Ok(()),
            _ => Err(format!(
                "导出函数 {} 的{}类型不被 C ABI 导出支持 —— \
                 v1 支持 整数/浮点数/布尔/字符串/指针{}。更复杂的数据请用 指针 传句柄。",
                函数名,
                if 是返回 { "返回" } else { "参数" },
                if 是返回 { "/空" } else { "" }
            )),
        }
    }

    /// Qi 类型 → C ABI LLVM 类型（布尔按 i64，字符串/指针按 ptr）。
    fn 导出c类型(&self, t: Qi类型) -> inkwell::types::BasicTypeEnum<'ctx> {
        match t {
            Qi类型::整数 | Qi类型::布尔 => self.ctx.i64_type().into(),
            Qi类型::浮点数 => self.ctx.f64_type().into(),
            _ => self.ctx.ptr_type(AddressSpace::default()).into(),
        }
    }

    /// 为所有导出函数生成 C ABI 包装 + 库加载构造器（运行时自初始化）。
    /// 在所有 Qi 函数体生成完毕后调用（转发目标已就位）。
    pub(super) fn 生成导出包装(&mut self) -> Result<(), String> {
        if self.导出表.is_empty() {
            return Ok(());
        }
        let 记录: Vec<导出记录> = self.导出表.clone();
        for r in &记录 {
            self.生成单个导出包装(r)?;
        }
        self.生成运行时构造器()?;
        Ok(())
    }

    fn 生成单个导出包装(&mut self, r: &导出记录) -> Result<(), String> {
        // 幂等（跨模块同符号只生成一次）
        if self.module.get_function(&r.c符号).is_some() {
            return Ok(());
        }
        let i64t = self.ctx.i64_type();

        let inner = self
            .module
            .get_function(&r.qi内部符号)
            .ok_or_else(|| format!("导出内部函数原型缺失: {}", r.原名))?;

        let 参数c: Vec<inkwell::types::BasicMetadataTypeEnum> =
            r.参数.iter().map(|t| self.导出c类型(*t).into()).collect();
        let fn_type = match r.返回 {
            Qi类型::空 => self.ctx.void_type().fn_type(&参数c, false),
            其他 => self.导出c类型(其他).fn_type(&参数c, false),
        };
        let wrapper = self
            .module
            .add_function(&r.c符号, fn_type, Some(Linkage::External));
        let bb = self.ctx.append_basic_block(wrapper, "entry");
        self.builder.position_at_end(bb);

        // C 实参 → Qi 值
        let mut 转发: Vec<BasicMetadataValueEnum> = Vec::new();
        // 字符串形参在 C→Qi 时拷成 wrapper 拥有的 rc=1 堆串（qi_string_from_cstr）。
        // wrapper 是这些拷贝的所有者：inner 只借用，调用结束后须 ARC 释放，否则每次
        // 带字符串参数的导出调用泄漏一条串（QI_RC_REPORT 会看到 活跃字符串 累积）。
        let mut 字符串参数拷贝: Vec<BasicValueEnum> = Vec::new();
        for (i, pt) in r.参数.iter().enumerate() {
            let a = wrapper
                .get_nth_param(i as u32)
                .ok_or_else(|| "缺导出形参".to_string())?;
            let v: BasicValueEnum = match pt {
                // C i64 → i1（内部 布尔 是 i1）
                Qi类型::布尔 => {
                    let iv = a.into_int_value();
                    self.builder
                        .build_int_compare(inkwell::IntPredicate::NE, iv, i64t.const_zero(), "c2b")
                        .map_err(|e| e.to_string())?
                        .into()
                }
                // C char* → 拷进 Qi 拥有堆串（rc=1，带 magic header）
                Qi类型::字符串 => {
                    let copyf = self
                        .module
                        .get_function("qi_string_from_cstr")
                        .ok_or_else(|| "运行时函数未声明: qi_string_from_cstr".to_string())?;
                    let copy = self
                        .builder
                        .build_call(copyf, &[a.into()], "cstr2qi")
                        .map_err(|e| e.to_string())?
                        .try_as_basic_value()
                        .basic()
                        .ok_or_else(|| "qi_string_from_cstr 未返回".to_string())?;
                    字符串参数拷贝.push(copy);
                    copy
                }
                // 整数 / 浮点数 / 指针：原样
                _ => a,
            };
            转发.push(v.into());
        }

        let call = self
            .builder
            .build_call(inner, &转发, "qicall")
            .map_err(|e| e.to_string())?;

        // async 桥：inner 返回 `未来<T>`（future 指针）。阻塞 await 出 T 值，
        // 再走下面与同步完全一致的 C ABI 返回转换。await 在**外部宿主线程**上
        // 阻塞（qi_future_await_* → 全局 tokio block_on / block_in_place），
        // 外部线程阻塞自己无害；多外部线程各 await 各的 future，互不干扰。
        // 归一化到「同步 inner 返回」的表示：布尔→i1、字符串→Qi 拥有 rc 串、
        // 整数/浮点数→原值，好让后面的 match 分支原样复用。
        let 结果值: Option<BasicValueEnum> = if r.是异步 && r.返回 != Qi类型::空 {
            let fut = call
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| "导出异步内部应返回未来指针".to_string())?;
            let 解包 = self.导出await解包(r.返回, fut)?;
            Some(解包)
        } else {
            None
        };

        // wrapper 拥有的字符串形参拷贝：调用结束即释放（inner 若私藏会先 retain，
        // 释放的是 wrapper 这一份所有权）。字符串返回场景在 strdup 取出返回值**之后**
        // 再释放，规避「函数直接返回入参串」时提前归零导致 strdup 读已释放内存。
        macro_rules! 释放字符串参数 {
            () => {
                for c in &字符串参数拷贝 {
                    self.弧release(*c);
                }
            };
        }

        // 同步取 inner 返回值；异步时已 await 解包到 结果值。
        macro_rules! 取返回值 {
            ($e:expr) => {
                match 结果值 {
                    Some(v) => v,
                    None => call.try_as_basic_value().basic().ok_or_else(|| $e.to_string())?,
                }
            };
        }

        match r.返回 {
            Qi类型::空 => {
                释放字符串参数!();
                self.builder.build_return(None).map_err(|e| e.to_string())?;
            }
            // 内部 i1 → C i64
            Qi类型::布尔 => {
                let rv = 取返回值!("导出内部应返回布尔").into_int_value();
                let w = self
                    .builder
                    .build_int_z_extend(rv, i64t, "b2i64")
                    .map_err(|e| e.to_string())?;
                释放字符串参数!();
                self.builder
                    .build_return(Some(&w))
                    .map_err(|e| e.to_string())?;
            }
            // Qi 拥有串 → strdup 交给 C（C free），再 ARC 释放 Qi 串。
            Qi类型::字符串 => {
                let qs = 取返回值!("导出内部应返回字符串");
                let strdup = self.取strdup();
                let cptr = self
                    .builder
                    .build_call(strdup, &[qs.into()], "strdup")
                    .map_err(|e| e.to_string())?
                    .try_as_basic_value()
                    .basic()
                    .ok_or_else(|| "strdup 未返回".to_string())?;
                self.弧release(qs); // ARC 关时 no-op
                释放字符串参数!(); // strdup 已取出返回值，此刻释放入参拷贝安全
                self.builder
                    .build_return(Some(&cptr))
                    .map_err(|e| e.to_string())?;
            }
            // 整数 / 浮点数 / 指针：原样
            _ => {
                let rv = 取返回值!("导出内部应有返回值");
                释放字符串参数!();
                self.builder
                    .build_return(Some(&rv))
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    /// async 桥：阻塞 await 一个 future 指针，取回标量 T，并 free 掉 future 本体。
    /// 返回值归一化到「同步 inner 返回」的表示（布尔→i1，其余原样），供包装的
    /// C ABI 返回转换原样复用。
    fn 导出await解包(
        &mut self,
        标量: Qi类型,
        fut: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let ffi = match 标量 {
            Qi类型::整数 => "qi_future_await_i64",
            Qi类型::浮点数 => "qi_future_await_f64",
            Qi类型::布尔 => "qi_future_await_bool",
            Qi类型::字符串 => "qi_future_await_string",
            _ => return Err("导出异步返回类型无对应 await".to_string()),
        };
        let awaitf = self
            .module
            .get_function(ffi)
            .ok_or_else(|| format!("future 运行时未声明: {}", ffi))?;
        let mut v = self
            .builder
            .build_call(awaitf, &[fut.into()], "await")
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "await 未返回".to_string())?;
        // await_bool 回来是 i32，截回 i1 以复用同步 布尔 分支（它再 zext 到 i64）
        if 标量 == Qi类型::布尔 {
            v = self
                .builder
                .build_int_truncate(v.into_int_value(), self.ctx.bool_type(), "b2i1")
                .map_err(|e| e.to_string())?
                .into();
        }
        // await 出值后 free future 本体（避免每次异步导出调用泄漏一个 Future Box）。
        // 字符串 payload 已被 await_string 拷成新 rc 串；ptr payload 我们没用
        // await_ptr（未 take），此处按 future_free 内部逻辑安全回收。
        let freef = self.取future_free();
        self.builder
            .build_call(freef, &[fut.into()], "")
            .map_err(|e| e.to_string())?;
        Ok(v)
    }

    fn 取future_free(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("qi_future_free") {
            return f;
        }
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        self.module.add_function(
            "qi_future_free",
            self.ctx.void_type().fn_type(&[ptrt.into()], false),
            None,
        )
    }

    fn 取strdup(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("strdup") {
            return f;
        }
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        self.module
            .add_function("strdup", ptrt.fn_type(&[ptrt.into()], false), None)
    }

    /// emit 一个 @llvm.global_ctors 项：库加载时自动 qi_runtime_initialize()。
    /// 使导出函数首次被 C 调用前运行时已就绪（协程/ARC/打印可用），C 侧无需手动 init。
    fn 生成运行时构造器(&mut self) -> Result<(), String> {
        if self.module.get_function("qi_lib_ctor").is_some() {
            return Ok(());
        }
        let i32t = self.ctx.i32_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        let initf = self
            .module
            .get_function("qi_runtime_initialize")
            .unwrap_or_else(|| {
                self.module
                    .add_function("qi_runtime_initialize", i32t.fn_type(&[], false), None)
            });

        // ctor 函数：void()（internal linkage，只被 global_ctors 引用）
        let ctor = self.module.add_function(
            "qi_lib_ctor",
            self.ctx.void_type().fn_type(&[], false),
            Some(Linkage::Internal),
        );
        let bb = self.ctx.append_basic_block(ctor, "entry");
        let 存 = self.builder.get_insert_block();
        self.builder.position_at_end(bb);
        self.builder
            .build_call(initf, &[], "")
            .map_err(|e| e.to_string())?;
        self.builder.build_return(None).map_err(|e| e.to_string())?;
        if let Some(b) = 存 {
            self.builder.position_at_end(b);
        }

        // @llvm.global_ctors = appending [ { i32 priority, void()* ctor, i8* data } ]
        let entry_ty = self
            .ctx
            .struct_type(&[i32t.into(), ptrt.into(), ptrt.into()], false);
        let entry_val = entry_ty.const_named_struct(&[
            i32t.const_int(65535, false).into(),
            ctor.as_global_value().as_pointer_value().into(),
            ptrt.const_null().into(),
        ]);
        let arr_ty = entry_ty.array_type(1);
        let arr_val = entry_ty.const_array(&[entry_val]);
        let g = self.module.add_global(arr_ty, None, "llvm.global_ctors");
        g.set_linkage(Linkage::Appending);
        g.set_initializer(&arr_val);
        Ok(())
    }
}
