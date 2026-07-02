//! 标准库分发 + 导入别名。
//!
//! 模块限定调用 `IO.打印行(x)` / `字符串::字节长度(s)` / `J.解码(s)` / `时间.现在()`
//! → 通过 ModuleRegistry 查到「中文模块.方法 → qi_runtime_* + 签名」，据签名生成
//! 类型正确的 FFI call（参数/返回类型不再猜）。运行时符号按需 declare 一次。
//!
//! 导入别名：`导入 标准库.输入输出 作为 IO` 让 `IO` 解析到模块「输入输出」。

use super::类型::Qi类型;
use super::后端;
use crate::codegen::module_registry::ModuleFunction;
use crate::parser::ast::{AstNode, Program};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::AddressSpace;

impl<'ctx> 后端<'ctx> {
    /// 从一个模块的导入语句收集标准库别名（alias/末段名 → 中文模块名）。
    pub(super) fn 收集导入别名(&mut self, program: &Program) {
        for imp in &program.imports {
            let is_stdlib = imp.module_path.first().map(|s| s.as_str()) == Some("标准库");
            if !is_stdlib {
                continue;
            }
            // 末段是中文模块名，如 标准库.输入输出 → 输入输出
            let module_name = match imp.module_path.last() {
                Some(m) => m.clone(),
                None => continue,
            };
            // 别名（作为 X）优先；否则用模块名自身
            let alias = imp.alias.clone().unwrap_or_else(|| module_name.clone());
            self.导入别名.insert(alias, module_name.clone());
            // 模块名本身也可直接用（无别名时的 IO.xxx / 时间.xxx）
            self.导入别名.insert(module_name.clone(), module_name);
        }
    }

    /// 把「接收者(标识符) + 方法名」当作标准库调用尝试解析。
    /// 返回 Ok(None)：不是标准库调用；Ok(Some(...))：已生成调用（可能 void→None）。
    pub(super) fn 尝试标准库调用(
        &mut self,
        object: &AstNode,
        method: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        // 接收者必须是裸标识符（模块名或别名），且不是局部变量。
        let ident = match object {
            AstNode::标识符表达式(id) => &id.name,
            _ => return Ok(None),
        };
        if self.变量表.contains_key(ident) {
            return Ok(None); // 是个变量，不是模块
        }

        // `字符串::字节长度` 会带 :: 前缀
        let method = method.trim_start_matches(':');

        // 解析模块名：先查别名，否则直接把 ident 当模块名
        let module_name = self
            .导入别名
            .get(ident)
            .cloned()
            .unwrap_or_else(|| ident.clone());

        // 在注册表里找函数（中文模块名直接是 key）
        let mf = match self.注册表.get_function(&module_name, method) {
            Some(f) => f.clone(),
            None => {
                // 试 "标准库.模块" 形式
                let alt = format!("标准库.{}", module_name);
                match self.注册表.get_function(&alt, method) {
                    Some(f) => f.clone(),
                    None => return Ok(None),
                }
            }
        };

        self.发射标准库调用(&mf, arguments).map(Some)
    }

    /// 无模块限定的标准库调用：`MD5哈希(x)`、`创建等待组()` 等（导入了名字但不带模块前缀）。
    /// 在所有已注册模块里找同名函数，命中即按其签名发射。返回 None 表示不是 stdlib 函数。
    pub(super) fn 尝试无限定标准库(
        &mut self,
        name: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        let name = name.trim_start_matches(':');
        let mf = match self.查任意模块函数(name) {
            Some(f) => f,
            None => return Ok(None),
        };
        self.发射标准库调用(&mf, arguments).map(Some)
    }

    /// 在所有已注册模块里找一个同名函数（用于无限定 stdlib 调用），返回其 clone。
    pub(super) fn 查任意模块函数(&self, name: &str) -> Option<ModuleFunction> {
        for path in self.注册表.module_paths() {
            if let Some(m) = self.注册表.get_module(path) {
                if let Some(f) = m.get_function(name) {
                    return Some(f.clone());
                }
            }
        }
        None
    }

    /// 据 ModuleFunction 签名 declare（一次）+ call。
    fn 发射标准库调用(
        &mut self,
        mf: &ModuleFunction,
        arguments: &[AstNode],
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let 返回 = 注册表类型转qi(&mf.return_type);

        // 声明原型（幂等：已存在则复用）
        let func = match self.module.get_function(&mf.runtime_name) {
            Some(f) => f,
            None => {
                let 参数llvm: Vec<BasicMetadataTypeEnum> = mf
                    .param_types
                    .iter()
                    .map(|t| self.注册表参数llvm类型(t))
                    .collect();
                let fn_type = match self.注册表llvm返回(返回) {
                    Some(rt) => rt.fn_type(&参数llvm, false),
                    None => self.ctx.void_type().fn_type(&参数llvm, false),
                };
                self.module.add_function(&mf.runtime_name, fn_type, None)
            }
        };

        // 参数：按声明类型做隐式转换（int↔ptr 句柄、bool→i32、函数值/指针→ptr 等在此归一）
        let mut args: Vec<BasicMetadataValueEnum> = Vec::new();
        // ARC：runtime FFI 借用读入参（内部拷贝，不留指针）——OWNED 字符串
        // 临时在调用结束后释放。
        let mut 弧待释放: Vec<BasicValueEnum> = Vec::new();
        for (i, a) in arguments.iter().enumerate() {
            let (v, vt) = self
                .生成表达式(a)?
                .ok_or_else(|| "标准库实参无值".to_string())?;
            if self.弧开()
                && vt == Qi类型::字符串
                && v.is_pointer_value()
                && self.表达式拥有字符串(a)
            {
                弧待释放.push(v);
            }
            let 原始 = mf.param_types.get(i).map(|s| s.as_str()).unwrap_or("整数");
            // 指针/ptr 形参：实参统一按 ptr 传（fat obj 指针、句柄、字符串指针都可）
            if 原始 == "指针" || 原始 == "ptr" {
                let pv = if v.is_pointer_value() {
                    v.into_pointer_value()
                } else {
                    self.builder
                        .build_int_to_ptr(
                            v.into_int_value(),
                            self.ctx.ptr_type(AddressSpace::default()),
                            "i2p",
                        )
                        .map_err(|e| e.to_string())?
                };
                args.push(pv.into());
                continue;
            }
            let 期望 = 注册表参数类型转qi(原始);
            let cv = self.适配实参(v, vt, 期望)?;
            args.push(cv);
        }

        let cs = self
            .builder
            .build_call(func, &args, "stdcall")
            .map_err(|e| e.to_string())?;
        for v in 弧待释放 {
            self.弧release(v);
        }
        match cs.try_as_basic_value().left() {
            Some(v) => Ok(Some((v, 返回))),
            None => Ok(None),
        }
    }

    /// 实参适配：布尔→i32、整数↔浮点、指针句柄→i64、其它原样。
    fn 适配实参(
        &self,
        v: BasicValueEnum<'ctx>,
        实际: Qi类型,
        期望: Qi类型,
    ) -> Result<BasicMetadataValueEnum<'ctx>, String> {
        // 布尔实参进 i64 整数参数：扩展
        if 实际 == Qi类型::布尔 && (期望 == Qi类型::整数 || 期望 == Qi类型::未知) {
            let ext = self
                .builder
                .build_int_z_extend(v.into_int_value(), self.ctx.i64_type(), "b2i")
                .map_err(|e| e.to_string())?;
            return Ok(ext.into());
        }
        if 期望.是浮点() && !实际.是浮点() && 实际 != Qi类型::字符串 {
            let f = self
                .builder
                .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
                .map_err(|e| e.to_string())?;
            return Ok(f.into());
        }
        // 期望整数(i64 句柄) 但实参是指针语义值（函数值/结构体/字符串误判）：ptr→int
        if 期望 == Qi类型::整数 && v.is_pointer_value() {
            let iv = self
                .builder
                .build_ptr_to_int(v.into_pointer_value(), self.ctx.i64_type(), "p2i")
                .map_err(|e| e.to_string())?;
            return Ok(iv.into());
        }
        // 期望字符串(ptr) 但实参是整数：int→ptr
        if 期望 == Qi类型::字符串 && v.is_int_value() {
            let pv = self
                .builder
                .build_int_to_ptr(
                    v.into_int_value(),
                    self.ctx.ptr_type(AddressSpace::default()),
                    "i2p",
                )
                .map_err(|e| e.to_string())?;
            return Ok(pv.into());
        }
        Ok(v.into())
    }

    /// 注册表参数类型字符串 → LLVM 元参数类型。
    fn 注册表参数llvm类型(&self, t: &str) -> BasicMetadataTypeEnum<'ctx> {
        // 指针/ptr 形参声明为 ptr（吃 fat obj 指针 / 字符串指针 / 句柄）
        if t == "指针" || t == "ptr" {
            return self.ctx.ptr_type(AddressSpace::default()).into();
        }
        match 注册表参数类型转qi(t) {
            Qi类型::浮点数 => self.ctx.f64_type().into(),
            Qi类型::字符串 => self.ctx.ptr_type(AddressSpace::default()).into(),
            // 整数 / 句柄 / 布尔（运行时多收 i32/i64，这里统一按 i64 收，call 前已扩展）
            _ => self.ctx.i64_type().into(),
        }
    }

    /// 返回 Qi 类型 → LLVM 基础类型（用于 fn_type 返回）。
    fn 注册表llvm返回(&self, 返回: Qi类型) -> Option<BasicTypeEnum<'ctx>> {
        match 返回 {
            Qi类型::空 => None,
            Qi类型::浮点数 => Some(self.ctx.f64_type().into()),
            Qi类型::字符串 => Some(self.ctx.ptr_type(AddressSpace::default()).into()),
            _ => Some(self.ctx.i64_type().into()),
        }
    }
}

/// 注册表返回类型字符串 → Qi 类型。
/// 关键（3-G）：裸 `ptr`/`指针` 在 registry 里几乎都表示**返回字符串**
/// （qi_db_query / qi_path_* / qi_random_uuid / qi_compress_gzip_string / 部分大模型/MCP），
/// 故返回侧把 ptr/指针 当字符串处理，后续拼接/字节长度/JSON.解码 类型才对。
pub(super) fn 注册表类型转qi(t: &str) -> Qi类型 {
    match t {
        "字符串" | "ptr" | "指针" => Qi类型::字符串,
        "浮点数" | "double" => Qi类型::浮点数,
        "布尔" => Qi类型::布尔,
        "空" | "void" => Qi类型::空,
        // 整数/i32/i64/句柄/数组/未来<..> 一律按整数（句柄）处理
        _ => Qi类型::整数,
    }
}

/// 注册表**参数**类型字符串 → Qi 类型。参数侧的 ptr/指针 多是句柄整数或 fat obj 指针，
/// 在 64 位平台按 i64 收发等价 —— 故参数侧仍把 ptr/指针 当整数，避免把句柄误当字符串。
pub(super) fn 注册表参数类型转qi(t: &str) -> Qi类型 {
    match t {
        "字符串" => Qi类型::字符串,
        "浮点数" | "double" => Qi类型::浮点数,
        "布尔" => Qi类型::布尔,
        "空" | "void" => Qi类型::空,
        _ => Qi类型::整数,
    }
}
