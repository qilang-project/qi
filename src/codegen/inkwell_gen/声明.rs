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
                // 特性作参数类型（impl-Trait 式）：`函数 让它叫(动物: 会叫)` →
                // 隐式泛型化成 `让它叫<__T0: 会叫>(动物: __T0)`，复用泛型单态化全套机制。
                if let Some(g) = self.试隐式泛型化(f)? {
                    self.登记泛型函数模板(&g)?;
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
    /// type_params 里可能带编码的特性约束（`"T:可比较"`，见 grammar TypeParamDecl）：
    /// 此处拆成 裸名 + 约束表，约束必须是已声明的特性；存表的 声明.type_params 只留裸名。
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
        // 拆约束：`T:可比较` → (T, Some(可比较))；裸 `T` → (T, None)
        let mut 名表: Vec<String> = Vec::new();
        let mut 约束表: Vec<Option<String>> = Vec::new();
        for tp in &f.type_params {
            match tp.split_once(':') {
                Some((n, b)) => {
                    if !self.符号.特性.contains_key(b) {
                        return Err(format!(
                            "泛型函数 {} 的类型参数 {} 的约束「{}」不是已声明的特性。约束只能写特性名，如 <{}: 某特性>",
                            f.name, n, b, n
                        ));
                    }
                    名表.push(n.to_string());
                    约束表.push(Some(b.to_string()));
                }
                None => {
                    名表.push(tp.clone());
                    约束表.push(None);
                }
            }
        }
        let mut 声明 = f.clone();
        声明.type_params = 名表;
        self.符号.泛型函数模板.insert(
            f.name.clone(),
            super::类型检查::泛型函数模板 {
                声明,
                包: self.当前包.clone(),
                约束: 约束表,
            },
        );
        Ok(())
    }

    /// 特性作参数类型的隐式泛型化：参数注解是**裸特性名**（不与结构体/枚举撞名）
    /// 的非泛型函数，改写成带约束的泛型模板 —— 每个特性参数一个独立类型参数
    /// `__T0: 特性`（多个特性参数各自独立，互不要求同型）。
    /// 无特性参数 → Ok(None)（走普通函数登记）。
    /// 返回类型是特性名 → v1 明确报错（不静默生成错误代码）。
    fn 试隐式泛型化(
        &mut self,
        f: &FunctionDeclaration,
    ) -> Result<Option<FunctionDeclaration>, String> {
        use crate::parser::ast::TypeNode;
        // 有没有特性参数？（先扫一遍，多数函数在此直接出去）
        let 有特性参数 = f.parameters.iter().any(|p| {
            matches!(&p.type_annotation, Some(TypeNode::自定义类型(n)) if self.符号.是特性名(n))
        });
        // 返回类型是特性名：v1 不支持（无论有没有特性参数都要拦）
        if let Some(TypeNode::自定义类型(n)) = &f.return_type {
            if self.符号.是特性名(n) {
                return Err(format!(
                    "函数 {} 的返回类型是特性「{}」—— 暂不支持特性作返回类型（需要装箱/存在类型，Phase 2）。请改成具体类型或泛型 <T: {}>",
                    f.name, n, n
                ));
            }
        }
        if !有特性参数 {
            return Ok(None);
        }
        // 改写：特性参数 → 合成类型参数 __TN（编码约束 "__TN:特性"，登记泛型函数模板 拆）
        let mut g = f.clone();
        let mut n = 0usize;
        for p in g.parameters.iter_mut() {
            let Some(TypeNode::自定义类型(名)) = &p.type_annotation else {
                continue;
            };
            if !self.符号.是特性名(名) {
                continue;
            }
            let tp = format!("__T{}", n);
            n += 1;
            g.type_params.push(format!("{}:{}", tp, 名));
            p.type_annotation = Some(TypeNode::自定义类型(tp));
        }
        Ok(Some(g))
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

        let 元数 = 参数类型.len();

        // 重载校验（同一 (包,名) 下按元数区分多个签名）。不改状态，先查后建。
        // 本定义是否带默认参数 / 变参 —— 这两者会让「按元数解析」产生歧义，故重载集内禁用。
        let 本def有默认 = f.parameters.iter().any(|p| p.default_value.is_some());
        let 本def变参 = f.parameters.last().map(|p| p.is_variadic).unwrap_or(false);
        if let Some(pkg) = self.当前包.clone() {
            if let Some(现有) = self.符号.函数按包.get(&(pkg.clone(), f.name.clone())) {
                if !现有.is_empty() {
                    // 1) 同元数 —— 无法区分
                    if 现有.iter().any(|s| s.参数.len() == 元数) {
                        return Err(format!(
                            "函数「{name}」在包「{pkg}」里有两个形参个数都是 {n} 的定义，无法按元数区分。请改名或调整参数个数。",
                            name = f.name, pkg = pkg, n = 元数
                        ));
                    }
                    // 2) 返回类型须一致（简化返回类型推断：任一重载的返回类型都通用）
                    if 现有[0].返回 != 返回类型 {
                        return Err(format!(
                            "重载函数「{name}」各定义的返回类型必须一致（现有 {a:?}，新增 {b:?}）。",
                            name = f.name, a = 现有[0].返回, b = 返回类型
                        ));
                    }
                    // 3) 重载禁默认参数 / 变参
                    if 本def有默认
                        || 本def变参
                        || self.符号.函数默认值.contains_key(&f.name)
                        || self.符号.函数变参.contains(&f.name)
                    {
                        return Err(format!(
                            "重载函数「{name}」不支持默认参数或变参（会让按元数解析产生歧义）。请去掉默认/变参，或改名。",
                            name = f.name
                        ));
                    }
                }
            }
        }

        // 包内唯一符号：把元数纳入 mangle —— 不同元数的重载得到互异 LLVM 符号，
        // 同元数已在上面报错。跨包同名（主程序.注册 vs Harness.注册）也天然分开。
        let mangled = super::包内符号名(self.当前包.as_deref(), &f.name, 元数);
        let func = self.module.add_function(&mangled, fn_type, None);

        let sig = 函数签名 {
            参数: 参数类型,
            返回: 返回类型,
        };
        // 包内签名（重载集）+ 扁平签名（fallback / 未标包时）
        if let Some(pkg) = self.当前包.clone() {
            self.符号
                .函数按包
                .entry((pkg, f.name.clone()))
                .or_default()
                .push(sig.clone());
        }
        self.符号.函数.entry(f.name.clone()).or_insert(sig);
        // 记录形参名（`工具模式` 生成 tool schema 时需要参数名）
        self.符号
            .函数参数名
            .entry(f.name.clone())
            .or_insert_with(|| f.parameters.iter().map(|p| p.name.clone()).collect());
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
        // mangle / 签名都按本定义的形参个数取 —— 精确对应本 f（重载各元数独立符号）。
        let 元数 = f.parameters.len();
        let mangled = super::包内符号名(self.当前包.as_deref(), &f.name, 元数);
        // QI_CORO：协程函数（返回 未来<T> 且含 等待）走 coro 变换（llvm.coro.*）。
        if self.协程函数集.contains(&mangled) {
            return self.生成协程函数体(f);
        }
        let func = self
            .module
            .get_function(&mangled)
            .ok_or_else(|| format!("函数原型缺失: {}", f.name))?;

        let sig = self
            .符号
            .解析重载(&f.name, 元数)
            .cloned()
            .ok_or_else(|| format!("函数签名缺失: {}", f.name))?;

        // 每个函数独立的局部变量表 / 作用域 / 返回类型 / try 深度
        self.变量表.clear();
        self.作用域遮蔽栈.clear();
        self.弧隐藏引用计数槽.clear();
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
