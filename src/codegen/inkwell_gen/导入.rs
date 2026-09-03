//! 标准库分发 + 导入别名。
//!
//! 模块限定调用 `IO.打印行(x)` / `字符串::字节长度(s)` / `J.解码(s)` / `时间.现在()`
//! → 通过 ModuleRegistry 查到「中文模块.方法 → qi_runtime_* + 签名」，据签名生成
//! 类型正确的 FFI call（参数/返回类型不再猜）。运行时符号按需 declare 一次。
//!
//! 导入别名：`导入 标准库.输入输出 作为 IO` 让 `IO` 解析到模块「输入输出」。

use super::后端;
use super::类型::Qi类型;
use crate::codegen::module_registry::ModuleFunction;
use crate::parser::ast::{AstNode, Program};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::AddressSpace;

impl<'ctx> 后端<'ctx> {
    /// 从一个模块的导入语句收集标准库别名（alias/末段名 → 中文模块名）。
    ///
    /// 顺带校验「标准库.X」里的 X 真的存在。以前不校验，后果比想象的隐蔽：
    /// `导入 标准库.日期时间 作为 时间;` 根本没有「日期时间」这个模块，但因为别名
    /// 恰好叫「时间」，`时间.睡眠毫秒()` 会经「别名当模块名」的回退路径解析到真正的
    /// 「时间」模块 —— **编译通过、运行正常**。换个别名（`作为 钟`）才会在 codegen
    /// 阶段报「模块没有这个函数」，而错误信息指着一个压根不存在的模块名，
    /// 让人以为是函数少了。在导入这一行就拦住，错在哪一目了然。
    pub(super) fn 收集导入别名(&mut self, program: &Program) -> Result<(), String> {
        for imp in &program.imports {
            let is_stdlib = imp.module_path.first().map(|s| s.as_str()) == Some("标准库");
            if !is_stdlib {
                // 用户包别名：`导入 Web.持久会话 作为 会话存储` → 会话存储 → "Web.持久会话"。
                // 记下来才能让 `别名.函数()` 按**那个包**解析；否则只能退回裸名查找，
                // 一旦别名与另一个包的模块同名（如 Harness 也有 会话存储），
                // 就会解析到错的包或直接歧义失败。
                if let Some(alias) = &imp.alias {
                    let 包名 = imp.module_path.join(".");
                    self.包别名.insert(alias.clone(), 包名.clone());
                    // 类型推断那边也得知道 —— 否则 `变量 x = 查.取一条(q)`
                    // 推不出返回类型，变量槽会退化成整数（见 符号表.包别名）
                    self.符号.包别名.insert(alias.clone(), 包名);
                }
                continue;
            }
            // 末段是中文模块名，如 标准库.输入输出 → 输入输出
            let module_name = match imp.module_path.last() {
                Some(m) => m.clone(),
                None => continue,
            };
            // qi 实现的模块（标准库/X.qi）不在 FFI 注册表里 —— 搬完一个模块就会把它
            // 的 FFI 条目撤掉，那之后 has_module 是 false，但导入完全合法。
            let 有qi实现 = crate::qi_stdlib::source(&module_name).is_some();
            if !有qi实现 && !self.注册表.has_module(&module_name) {
                return Err(format!(
                    "标准库里没有模块「{}」{}",
                    module_name,
                    相近模块提示(&self.注册表, &module_name)
                ));
            }
            if 有qi实现 {
                // 别名要按**用户包**登记，`导入 标准库.JSON 作为 J` 之后
                // `J.编码(x)` 才走得到包限定解析（表达式.rs 分支 3）。
                let alias = imp.alias.clone().unwrap_or_else(|| module_name.clone());
                self.包别名.insert(alias.clone(), module_name.clone());
                self.符号.包别名.insert(alias, module_name.clone());
                if !self.注册表.has_module(&module_name) {
                    continue; // 纯 qi 实现，没有 FFI 别名可登记
                }
            }
            // 别名（作为 X）优先；否则用模块名自身
            let alias = imp.alias.clone().unwrap_or_else(|| module_name.clone());
            self.导入别名.insert(alias.clone(), module_name.clone());
            // 类型推断那边也得知道别名，否则 `J.获取字符串(…)` 推不出返回类型
            self.符号.标准库别名.insert(alias, module_name.clone());
            // 模块名本身也可直接用（无别名时的 IO.xxx / 时间.xxx）
            self.导入别名.insert(module_name.clone(), module_name);
        }
        Ok(())
    }

    /// 从一个模块的 destructure 导入收集「裸名 → 来源包」映射（结构体+函数共用）：
    /// `导入 Web::{应用, 创建应用}` → 本模块里裸名 `应用`/`创建应用` 优先解析到 Web 包。
    /// 项目里跨包同名符号（Web::应用 vs CLI::应用）靠这个消歧；
    /// 同一模块从多个来源导入同名符号 → 记歧义（解析时报编译错误）。
    pub(super) fn 收集符号导入(&mut self, program: &Program) {
        let 使用方 = program.package_name.clone();
        for imp in &program.imports {
            let 首段 = match imp.module_path.first() {
                Some(s) => s.as_str(),
                None => continue,
            };
            // 标准库 / 相对路径导入没有跨包结构体归属问题
            if 首段 == "标准库" || 首段 == "." || 首段 == ".." {
                continue;
            }
            let items = match &imp.items {
                Some(v) if !v.is_empty() => v,
                _ => continue,
            };
            // 记**全路径**：这里还看不到被导入文件的 `包` 声明，而声明与路径没有
            // 固定关系（包 Web.持久会话; / 包 Web; / 包 账户; 三种写法并存）。
            // 真实包名由解析处按 符号表::导入来源候选（全路径 → 首段 → … → 末段）
            // 逐个试出来 —— 只有真的注册了该符号的候选会命中。
            let 来源 = imp.module_path.join(".");
            for item in items {
                self.符号.登记符号导入(使用方.clone(), item, &来源);
            }
        }
    }

    /// 校验：destructure 导入的名字必须真实存在。
    ///
    /// 之前 `导入 Web::{压根没有这个函数}` 编译通过、运行静默不工作 —— 写 AI 笔记
    /// 时用了 qi-harness 新版才有的 API，而 qi_packages 里是旧副本，编译一路绿灯，
    /// 跑起来检索永远空，排查了很久才发现函数根本不存在。
    ///
    /// 判定刻意保守：只要这个名字在**整个编译单元**里作为函数/结构体/枚举/枚举变体
    /// 存在，就放行 —— 包再导出（`公开 导入 X;`）会让符号的实际归属包与导入路径
    /// 不一致，按来源包严格比对必然误报。真正拦的是「整个程序里根本没有这个名字」。
    pub(super) fn 检查导入存在(&self, program: &Program) -> Result<(), String> {
        for imp in &program.imports {
            let 首段 = match imp.module_path.first() {
                Some(s) => s.as_str(),
                None => continue,
            };
            // 标准库走注册表，相对导入不做包级校验
            if 首段 == "标准库" || 首段 == "." || 首段 == ".." {
                continue;
            }
            let items = match &imp.items {
                Some(v) if !v.is_empty() => v,
                _ => continue,
            };
            for item in items {
                if self.符号.名字存在于任意包(item) {
                    continue;
                }
                return Err(format!(
                    "导入的符号「{name}」不存在：`导入 {src}::{{{name}}}` 里没有这个名字。\n\
                     整个编译单元里都找不到叫「{name}」的函数/结构体/枚举/变体。\n\
                     常见原因：拼错了，或者 qi_packages 里的那份依赖是旧版本、还没有这个 API。",
                    name = item,
                    src = imp.module_path.join("."),
                ));
            }
        }
        Ok(())
    }

    /// 校验：本模块本地定义的函数名，不得与它 destructure 导入进来的同名**函数**冲突。
    ///
    /// Qi 目前**不按签名做重载解析**：当一个名字既有本地定义、又被导入时，本地定义会
    /// 遮蔽导入，调用点静默绑到本地那个。若你本想调导入的那个、实参却不匹配本地签名，
    /// 就会报成「参数个数错」或「结构体无字段 X」——伪装成别的错误，极难排查
    /// （qi-harness 的 `评估.运行` vs `代理.运行` 就踩过这个坑）。这里编译期直接拦下。
    ///
    /// 必须在 登记函数 之后调用（依赖 符号.函数按包 判定导入项是否为函数）。
    pub(super) fn 检查导入遮蔽(&self, program: &Program) -> Result<(), String> {
        use std::collections::HashSet;
        let mut 本地函数: HashSet<&str> = HashSet::new();
        for stmt in &program.statements {
            if let crate::parser::ast::AstNode::函数声明(f) = stmt {
                本地函数.insert(f.name.as_str());
            }
        }
        if 本地函数.is_empty() {
            return Ok(());
        }
        for imp in &program.imports {
            let 首段 = match imp.module_path.first() {
                Some(s) => s.as_str(),
                None => continue,
            };
            if 首段 == "标准库" || 首段 == "." || 首段 == ".." {
                continue;
            }
            // 同包导入（来源包 == 本模块所属包）放行：同包内同名不同元数是合法重载，
            // 共用同一 (包,名) 重载集，不构成「本地遮蔽外部导入」。只拦真正的跨包遮蔽。
            if program.package_name.as_deref() == Some(首段) {
                continue;
            }
            let items = match &imp.items {
                Some(v) if !v.is_empty() => v,
                _ => continue,
            };
            for item in items {
                // 仅当：本地也定义同名函数，且该名字在来源包里确实是个函数
                if 本地函数.contains(item.as_str())
                    && self
                        .符号
                        .函数按包
                        .contains_key(&(首段.to_string(), item.clone()))
                {
                    return Err(format!(
                        "函数名冲突：本模块定义了函数「{name}」，又从 `{src}` 导入了同名函数。\n\
                         Qi 暂不按签名重载解析——本地定义会遮蔽导入，调用点会静默绑到本地那个，\n\
                         实参不符时报成「参数个数错」或「结构体无字段 X」，极难排查。\n\
                         修复：给本地函数改个名（如「{name}套件」之类），或删掉该导入项。",
                        name = item,
                        src = imp.module_path.join("."),
                    ));
                }
            }
        }
        Ok(())
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

        // 有 qi 实现的标准库模块：让路给用户模块解析（表达式.rs 的分支 3），
        // 不要在这儿命中 FFI 注册表。
        //
        // 两份实现会同时存在一段时间 —— 模块是一个一个搬过去的，FFI 表里那份
        // 还留着当逃生口（QI_STDLIB_FFI）。所以这里必须显式定优先级，否则
        // `导入 标准库.JSON` 之后 `JSON.编码(x)` 仍然会落到 qi_json_encode，
        // 搬过去的 qi 代码一行都不会被执行，而且毫无迹象。
        //
        // **必须同时满足「这个模块真有 qi 实现」**。只看 函数按包 里有没有同名
        // 函数是不够的：用户包完全可以跟标准库模块**重名**。qi-grpc 就是
        // `包 gRPC;` 外加 `导入 标准库.gRPC 作为 运行时`，两边还都有
        // `调用带元数据` / `开流`。只按名字让路的话，`运行时.调用带元数据(五个参数)`
        // 会被解析到本包那个签名不同的同名函数上，LLVM 模块校验当场报
        // 「Incorrect number of arguments」。
        if crate::qi_stdlib::source(&module_name).is_some()
            && self
                .符号
                .函数按包
                .contains_key(&(module_name.clone(), method.to_string()))
        {
            return Ok(None);
        }

        // 在注册表里找函数（中文模块名直接是 key）
        let mf = match self.注册表.get_function(&module_name, method) {
            Some(f) => f.clone(),
            None => {
                // 试 "标准库.模块" 形式
                let alt = format!("标准库.{}", module_name);
                match self.注册表.get_function(&alt, method) {
                    Some(f) => f.clone(),
                    None => {
                        // bug ② 修复：接收者确定是标准库模块（经 导入 标准库.X 登记的
                        // 别名）而模块里没有这个方法 → 直接报错。旧行为返回 None 后
                        // 落到「按名跨模块查找」，JSON.解析 会被随机分发到
                        // qi_datetime_parse / qi_multipart_parse（HashMap 序）。
                        if self.导入别名.contains_key(ident) {
                            let mut 有此名的模块: Vec<String> = Vec::new();
                            let mut paths = self.注册表.module_paths();
                            paths.sort();
                            for p in paths {
                                if p.starts_with("标准库.") {
                                    continue; // 双注册去重（只报短名）
                                }
                                if let Some(m) = self.注册表.get_module(p) {
                                    if m.get_function(method).is_some() {
                                        有此名的模块.push(p.clone());
                                    }
                                }
                            }
                            let 提示 = if 有此名的模块.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    "（名为「{}」的函数定义在：{}）",
                                    method,
                                    有此名的模块.join("、")
                                )
                            };
                            return Err(format!(
                                "标准库模块「{}」没有函数「{}」{}",
                                module_name, method, 提示
                            ));
                        }
                        return Ok(None);
                    }
                }
            }
        };

        self.发射标准库调用(&mf, arguments).map(Some)
    }

    /// 无模块限定的标准库调用：`MD5哈希(x)`、`创建等待组()` 等（导入了名字但不带模块前缀）。
    /// 在所有已注册模块里找同名函数，命中即按其签名发射。返回 None 表示不是 stdlib 函数。
    ///
    /// bug ② 修复：同名函数注册在多个模块（映射到**不同** runtime 符号，如 解析 →
    /// qi_multipart_parse / qi_cli_parse / qi_datetime_parse）时，旧实现按 HashMap
    /// 迭代序取首命中 —— 同一份源码每次编译分发目标都可能不同。现在：
    /// 歧义 → 直接报编译错误，要求写模块限定（时间.解析(x) 等）；
    /// 唯一 → 正常发射（多数无限定调用，如 MD5哈希）。
    pub(super) fn 尝试无限定标准库(
        &mut self,
        name: &str,
        arguments: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        let name = name.trim_start_matches(':');
        // 收集全部候选（模块名排序保证确定性；"X" 与 "标准库.X" 是同一模块的
        // 双注册，按 runtime 符号去重后不构成歧义）
        let mut paths = self.注册表.module_paths();
        paths.sort();
        let mut 候选: Vec<(String, ModuleFunction)> = Vec::new();
        for path in paths {
            if let Some(m) = self.注册表.get_module(path) {
                if let Some(f) = m.get_function(name) {
                    if !候选.iter().any(|(_, g)| g.runtime_name == f.runtime_name) {
                        候选.push((path.clone(), f.clone()));
                    }
                }
            }
        }
        match 候选.len() {
            0 => Ok(None),
            1 => self.发射标准库调用(&候选[0].1, arguments).map(Some),
            _ => {
                let 模块们: Vec<String> = 候选
                    .iter()
                    .map(|(p, _)| p.trim_start_matches("标准库.").to_string())
                    .collect();
                Err(format!(
                    "函数「{}」在多个标准库模块中都有定义（{}），无限定调用有歧义。\
                     请写模块限定形式，如 {}.{}(...)",
                    name,
                    模块们.join("、"),
                    模块们[0],
                    name
                ))
            }
        }
    }

    /// 在所有已注册模块里找一个同名函数（用于无限定 stdlib 调用），返回其 clone。
    /// 模块路径**排序后**遍历 —— HashMap 迭代序随机，否则同名函数（如 解析）
    /// 每次编译命中的模块都可能不同（bug ②：同源码随机行为）。
    pub(super) fn 查任意模块函数(&self, name: &str) -> Option<ModuleFunction> {
        let mut paths = self.注册表.module_paths();
        paths.sort();
        for path in paths {
            if let Some(m) = self.注册表.get_module(path) {
                if let Some(f) = m.get_function(name) {
                    return Some(f.clone());
                }
            }
        }
        None
    }

    /// 按（注册表模块名, 方法名）精确发射一次标准库调用 —— 供内建分发
    /// （如 `长度(字符串)` → 字符串.字符数量）等编译器内部改写用，
    /// 不经过任何按名跨模块查找。
    pub(super) fn 发射指定模块函数(
        &mut self,
        模块: &str,
        方法: &str,
        arguments: &[AstNode],
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let mf = self
            .注册表
            .get_function(模块, 方法)
            .or_else(|| self.注册表.get_function(&format!("标准库.{}", 模块), 方法))
            .cloned()
            .ok_or_else(|| format!("标准库注册表缺 {}.{}", 模块, 方法))?;
        self.发射标准库调用(&mf, arguments)
    }

    /// 据 ModuleFunction 签名 declare（一次）+ call。
    fn 发射标准库调用(
        &mut self,
        mf: &ModuleFunction,
        arguments: &[AstNode],
    ) -> Result<Option<(BasicValueEnum<'ctx>, Qi类型)>, String> {
        let 返回 = 注册表类型转qi(&mf.return_type);

        // 雷 B：实参个数必须与注册签名严格相等。少传 → 被调原生 FFI 读到未初始化
        // 寄存器（常是野指针）→ `CStr::from_ptr(garbage)` → **内容相关的偶发段错误**
        // （崩不崩取决于那个寄存器上次的残留值，极难定位）。多传 → 多余实参虽被忽略，
        // 但基本是调用点写错了，一并拦下。
        // 注册表里所有条目都是**定长签名**（ModuleFunction.param_types 是固定 Vec，
        // 无变参/可选参标记 —— 已核对全表 590 条，无同 runtime_name 多 arity 项），
        // 故此处要求 arguments.len() == param_types.len()。
        // 早于 LLVM 原型构建拦截：给出带函数名的清晰中文错误，而不是让 LLVM 校验器
        // 吐一句无源码定位的英文「Incorrect number of arguments」，也堵死「原型被
        // 别处以更低 arity 预声明 → 校验器放行 → 运行时踩野指针」的静默漏洞。
        if arguments.len() != mf.param_types.len() {
            return Err(format!(
                "标准库函数「{}」需要 {} 个实参，实际传了 {} 个。\n\
                 实参个数不符 —— 少传会让原生函数读到未初始化寄存器（野指针），\n\
                 引发内容相关的偶发段错误；请核对调用点的参数个数。",
                mf.name,
                mf.param_types.len(),
                arguments.len()
            ));
        }

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
        // ARC：借用读 FFI（向量.* 等）的 OWNED 对象临时（数组字面量直传实参）
        // 同理调用结束后按类型释放。
        let mut 弧待释放对象: Vec<(BasicValueEnum, Qi类型)> = Vec::new();
        let 借用读对象 = super::所有权::借用读对象FFI(&mf.runtime_name);
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
            if self.弧开()
                && matches!(vt, Qi类型::结构体(_) | Qi类型::数组(_) | Qi类型::函数值(_))
                && v.is_pointer_value()
            {
                if 借用读对象 {
                    // 借用读 FFI（内部只拷贝数据，不私藏指针）：BORROWED 原样借出；
                    // OWNED 临时（如 向量.点积([1.0], [2.0]) 的字面量）调用后释放。
                    if self.表达式拥有RC(a, vt) {
                        弧待释放对象.push((v, vt));
                    }
                } else if !self.表达式拥有RC(a, vt) {
                    // ARC：结构体/数组/闭包指针进 runtime FFI（列表::设置指针 / 网络::
                    // 异步服务 / 回调注册等可能**私藏**指针）→ 发送即转移：BORROWED 先
                    // retain（对应引用随藏点泄漏，宁泄漏不悬垂）；OWNED 直接转移，
                    // 不 retain 也不释放。
                    self.弧retain任意(v, vt);
                }
            }
            let 原始 = mf.param_types.get(i).map(|s| s.as_str()).unwrap_or("整数");
            // 指针/ptr/数组 形参：实参统一按 ptr 传（fat obj 指针、句柄、字符串指针、
            // Qi 数组本体指针都可）
            if 原始 == "指针" || 原始 == "ptr" || 原始 == "数组" || 原始 == "浮点数组"
            {
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
            let cv = self.适配实参(v, vt, 期望, &mf.name, i)?;
            args.push(cv);
        }

        let cs = self
            .builder
            .build_call(func, &args, "stdcall")
            .map_err(|e| e.to_string())?;
        for v in 弧待释放 {
            self.弧release(v);
        }
        for (v, t) in 弧待释放对象 {
            self.弧release任意(v, t);
        }
        match cs.try_as_basic_value().basic() {
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
        函数名: &str,
        第几个: usize,
    ) -> Result<BasicMetadataValueEnum<'ctx>, String> {
        // 布尔实参进 i64 整数参数：扩展
        if 实际 == Qi类型::布尔 && (期望 == Qi类型::整数 || 期望 == Qi类型::未知)
        {
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
        // 期望字符串(ptr)、实参却是整数 —— **这里以前是静默 int→ptr**。
        //
        // 那个转换几乎不可能是对的：形参声明成 字符串，被调的 Rust 侧就会
        // `CStr::from_ptr(它)`。把一个句柄/长度/下标当指针解引用 = 段错误，
        // 而且报出来只有一句「退出码 None」，源码里一个线索都没有。
        // 真正需要「整数当指针传」的形参在注册表里写的是 指针 / ptr / 数组，
        // 那几种在上面已经单独处理掉了。
        //
        // 实际踩到的：HTTP.获取状态码 收的是 URL 字符串，我按「句柄」传了个整数
        // 进去，编译一声不吭，运行直接崩。
        if 期望 == Qi类型::字符串 && v.is_int_value() {
            return Err(format!(
                "标准库函数「{}」第 {} 个参数要 字符串，实际传的是 整数。\n\
                 整数当字符串指针传进原生函数会被 CStr::from_ptr 解引用 —— 段错误，\n\
                 而且没有任何源码线索。请核对参数顺序和类型。",
                函数名,
                第几个 + 1
            ));
        }
        Ok(v.into())
    }

    /// 注册表参数类型字符串 → LLVM 元参数类型。
    fn 注册表参数llvm类型(&self, t: &str) -> BasicMetadataTypeEnum<'ctx> {
        // 指针/ptr/数组 形参声明为 ptr（吃 fat obj 指针 / 字符串指针 / 句柄 / Qi 数组本体）
        if t == "指针" || t == "ptr" || t == "数组" || t == "浮点数组" {
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
            Qi类型::字符串 | Qi类型::数组(_) => {
                Some(self.ctx.ptr_type(AddressSpace::default()).into())
            }
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
        // 浮点数组：FFI 按 Qi 数组布局新分配（qi_obj_alloc，rc=1 交出）的
        // f64 元素数组（向量.加/归一化/数乘）
        "浮点数组" => Qi类型::数组(super::类型::元素类型::浮点数),
        // 字符串数组：FFI 按 Qi 数组布局新分配（qi_obj_alloc 本体 + 每元素 rc=1 串）的
        // 字符串元素数组（反射.函数列表/结构体列表 等）。元素按指针存 → 访问返回字符串。
        "字符串数组" => Qi类型::数组(super::类型::元素类型::指针),
        // 整数数组：同布局，i64 元素（JSON.字段整数数组 等）
        "整数数组" => Qi类型::数组(super::类型::元素类型::整数),
        // 整数/i32/i64/未来<..> 一律按整数处理
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

/// 「你是不是想写 X」：先按子串/被包含挑近似的，一个都没有就把全部模块列出来。
///
/// 模块名都不长且是中文，编辑距离在这儿不如「包含关系」好使 ——
/// 实际的错法基本都是 日期时间/时间、IO/输入输出、集合/列表 这种多字少字。
fn 相近模块提示(
    注册表: &crate::codegen::module_registry::ModuleRegistry,
    写的: &str,
) -> String {
    let mut 全部: Vec<String> = 注册表
        .module_paths()
        .into_iter()
        .filter(|p| !p.starts_with("标准库.")) // 双注册去重，只留短名
        .cloned()
        .collect();
    全部.sort();

    let 相近: Vec<String> = 全部
        .iter()
        .filter(|m| m.contains(写的) || 写的.contains(m.as_str()))
        .cloned()
        .collect();
    if !相近.is_empty() {
        return format!("，是不是想写：{}", 相近.join(" / "));
    }
    format!("。可用的标准库模块：{}", 全部.join("、"))
}
