//! 函数声明降级 —— 用户函数（非 入口）定义为类型化 LLVM function。
//!
//! 两趟：先登记所有函数签名（供前向调用 / 类型检查），再逐个生成函数体。
//! 参数用 alloca+store 落地成局部变量，body 内当普通变量读写，语义与旧后端一致。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::函数签名;
use crate::parser::ast::{FunctionDeclaration, Program};
use inkwell::types::BasicType;
use inkwell::values::FunctionValue;

impl<'ctx> 后端<'ctx> {
    /// 第一趟：登记所有顶层函数签名 + 声明 LLVM function 原型。
    /// 泛型函数（带 <T>）不声明原型 —— AST 模板进注册表，按调用点单态实例化。
    pub(super) fn 登记函数(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let crate::parser::ast::AstNode::函数声明(f) = stmt {
                if f.name == "入口" {
                    continue; // 入口单独变 main
                }
                if !f.type_params.is_empty() {
                    self.登记泛型函数模板(f)?;
                    continue;
                }
                self.声明函数原型(f)?;
                // 预登记「函数名当值」的签名索引，供函数值传递
                self.符号.预登记函数值(&f.name);
            }
        }
        Ok(())
    }

    /// 登记一个泛型函数模板（AST 原样入表，供调用点单态化）。
    fn 登记泛型函数模板(&mut self, f: &FunctionDeclaration) -> Result<(), String> {
        if f.type_params.len() > 2 {
            return Err(format!(
                "泛型函数 {} 声明了 {} 个类型参数，最多支持 2 个（<T> 或 <T, E>）",
                f.name,
                f.type_params.len()
            ));
        }
        if super::枚举::是保留构造子(&f.name) {
            return Err(format!(
                "「{}」是奇语内建构造子（选项/结果），不能用作函数名。请换一个名字。",
                f.name
            ));
        }
        if self.符号.有函数(&f.name) {
            return Err(format!(
                "泛型函数 {} 与同名非泛型函数冲突，请改名其一",
                f.name
            ));
        }
        self.符号.泛型函数模板.insert(
            f.name.clone(),
            super::类型检查::泛型函数模板 {
                声明: f.clone(),
                包: self.当前包.clone(),
            },
        );
        Ok(())
    }

    /// 声明单个用户函数的 LLVM 原型，并把签名存入符号表。
    pub(super) fn 声明函数原型(
        &mut self,
        f: &FunctionDeclaration,
    ) -> Result<FunctionValue<'ctx>, String> {
        // 保留构造子名（有/无/成/败）：选项/结果 的内建构造子，用户不能定义同名函数。
        if super::枚举::是保留构造子(&f.name) {
            return Err(format!(
                "「{}」是奇语内建构造子（选项/结果），不能用作函数名。请换一个名字。",
                f.name
            ));
        }
        let 参数类型: Vec<Qi类型> = f
            .parameters
            .iter()
            .map(|p| {
                let t = p
                    .type_annotation
                    .as_ref()
                    .map(|t| self.符号.解析类型(t))
                    .unwrap_or(Qi类型::整数);
                if p.is_variadic {
                    // 变参 `名字...: T` 在签名/函数体内是 数组(T)：
                    // 调用点把尾实参打包成数组传入（见 表达式.rs 生成函数调用）。
                    Qi类型::数组(super::类型::元素类型::从标量(t))
                } else {
                    t
                }
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

        // 包内唯一符号，避免跨包同名（主程序.注册 vs Harness.注册）冲突
        let mangled = super::包内符号名(self.当前包.as_deref(), &f.name);
        let func = self.module.add_function(&mangled, fn_type, None);

        let sig = 函数签名 {
            参数: 参数类型,
            返回: 返回类型,
        };
        // 包内签名（消歧优先）+ 扁平签名（fallback / 未标包时）
        if let Some(pkg) = self.当前包.clone() {
            // 同包跨文件重名函数会 mangle 成同一 LLVM 符号 → 静默 last-write-wins，
            // 调用点绑到不确定的那个（伪装成「参数个数错 / 结构体无字段 X」，极难查）。
            // Qi 暂不支持按签名重载，直接编译期拦下，要求改名其一。
            let key = (pkg, f.name.clone());
            if self.符号.函数按包.contains_key(&key) {
                return Err(format!(
                    "函数重复定义：包「{pkg}」里有多个同名函数「{name}」（通常是同包多个文件各定义了一份）。\n\
                     Qi 暂不按签名重载解析——同名会 mangle 成同一符号、静默相互覆盖，\n\
                     调用点绑向不确定的那个。请给其中一个改名。",
                    pkg = key.0,
                    name = key.1,
                ));
            }
            self.符号.函数按包.insert(key, sig.clone());
        }
        self.符号.函数.entry(f.name.clone()).or_insert(sig);
        // 记录默认参数值（供调用少传时补齐）
        let 默认: Vec<Option<crate::parser::ast::AstNode>> = f
            .parameters
            .iter()
            .map(|p| p.default_value.as_ref().map(|b| (**b).clone()))
            .collect();
        if 默认.iter().any(|d| d.is_some()) {
            self.符号.函数默认值.entry(f.name.clone()).or_insert(默认);
        }
        // 记录变参函数（末位形参带 `...`），调用点据此打包尾实参
        if f.parameters.last().map(|p| p.is_variadic).unwrap_or(false) {
            self.符号.函数变参.insert(f.name.clone());
        }
        Ok(func)
    }

    /// 第二趟：生成一个用户函数的函数体。
    pub(super) fn 生成函数体(&mut self, f: &FunctionDeclaration) -> Result<(), String> {
        let mangled = super::包内符号名(self.当前包.as_deref(), &f.name);
        let func = self
            .module
            .get_function(&mangled)
            .ok_or_else(|| format!("函数原型缺失: {}", f.name))?;

        let sig = self
            .符号
            .解析函数(&f.name)
            .cloned()
            .ok_or_else(|| format!("函数签名缺失: {}", f.name))?;

        // 每个函数独立的局部变量表 / 作用域 / 返回类型 / try 深度
        self.变量表.clear();
        self.符号.进入作用域();
        self.当前返回类型 = sig.返回;
        self.try深度 = 0; // E4：异常 frame 栈按函数平衡，跨函数不串

        let entry = self.ctx.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        // 剖析：序言计时（QI_PROF 关时空操作），用中文原名当显示名
        self.剖析入口(&f.name)?;

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
            self.builder
                .build_store(ptr, arg)
                .map_err(|e| e.to_string())?;
            // ARC：RC 参数（字符串/结构体/数组，对调用方是借用）落地进局部槽
            // → retain 一次，与出口「释放所有 RC 局部」平衡；覆写时释放旧值也自洽。
            self.弧retain任意(arg, t);
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
            self.剖析出口()?; // 剖析：落底出口计时
            self.弧释放局部()?; // ARC：落底出口释放字符串局部
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

        self.符号.退出作用域();
        Ok(())
    }
}
