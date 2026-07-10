//! 真类型检查器 —— 作用域符号表 + 变量/函数/结构体/方法类型推断。
//!
//! 这是本次重写的核心：每个表达式都能问出「你是什么类型」，从而降级时
//! 选对 LLVM 指令 / 找到对应方法。检查器收集函数签名、结构体布局、方法签名，
//! 并按块作用域推断变量类型；表达式类型由 `推断表达式类型` 递归求出。
//!
//! 阶段8 关键：接收者可以是**任意表达式**（含上一次方法调用的结果），
//! 因此链式 `创建().放(1).放(2)` 天生可解析 —— 每步返回类型已知。

use super::类型::Qi类型;
use crate::codegen::module_registry::ModuleRegistry;
use crate::parser::ast::{AstNode, BinaryOperator, LiteralValue};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// 函数签名：参数类型 + 返回类型。
#[derive(Debug, Clone)]
pub struct 函数签名 {
    pub 参数: Vec<Qi类型>,
    pub 返回: Qi类型,
}

/// 结构体布局：字段名 → (索引, 类型)，按声明顺序。
/// `包`：声明它的包名 —— 跨包同名结构体（如 Web::应用 vs CLI::应用）各占独立索引，
/// 绝不共享/覆写彼此的字段布局（否则字段读错内存）。
#[derive(Debug, Clone)]
pub struct 结构体信息 {
    pub 名字: String,
    pub 包: Option<String>,
    pub 字段名: Vec<String>,
    pub 字段类型: Vec<Qi类型>,
}

impl 结构体信息 {
    /// 字段索引 + 类型。防御：字段名/字段类型 长度不一致时返回 None（交上层报编译错误），
    /// 绝不越界 panic。
    pub fn 查字段(&self, name: &str) -> Option<(u32, Qi类型)> {
        let i = self.字段名.iter().position(|f| f == name)?;
        let t = self.字段类型.get(i).copied()?;
        Some((i as u32, t))
    }
}

/// 枚举变体信息：变体名 + tag（声明序号）+ 载荷类型列表（空=无载荷）。
#[derive(Debug, Clone)]
pub struct 枚举变体信息 {
    pub 名字: String,
    pub tag: i64,
    pub 载荷: Vec<Qi类型>,
}

/// 枚举布局：变体表 + 装箱标志。任一变体带载荷 ⇒ 装箱（堆指针，槽0=tag，槽1..=载荷）。
/// `包`：声明包（跨包同名枚举各占独立索引，与结构体同款）。
#[derive(Debug, Clone)]
pub struct 枚举信息 {
    pub 名字: String,
    pub 包: Option<String>,
    pub 变体: Vec<枚举变体信息>,
    pub 装箱: bool,
    /// 装箱时最大载荷槽数（分配 (1+最大载荷槽)*8 字节）。
    pub 最大载荷槽: usize,
}

impl 枚举信息 {
    /// 按变体名查 (tag, 载荷)。
    pub fn 查变体(&self, name: &str) -> Option<&枚举变体信息> {
        self.变体.iter().find(|v| v.名字 == name)
    }
}

/// 用户泛型枚举模板：`枚举 盒装<T> { 满(T), 空盒 }`。
/// 变体载荷存 AST TypeNode（含 T 占位），实例化时按 T→实参 绑定解析。
#[derive(Debug, Clone)]
pub struct 泛型枚举模板 {
    pub 类型参数: Vec<String>,
    /// (变体名, 载荷 TypeNode 列表)
    pub 变体: Vec<(String, Vec<crate::parser::ast::TypeNode>)>,
}

/// 用户泛型结构体模板：`类型 对<T> { T 左; T 右; }`。
#[derive(Debug, Clone)]
pub struct 泛型结构体模板 {
    pub 类型参数: Vec<String>,
    /// (字段名, 字段 TypeNode)
    pub 字段: Vec<(String, crate::parser::ast::TypeNode)>,
}

/// 用户泛型函数模板：AST 原样存档 + 声明包（实例体在末尾统一合成时还原包上下文）。
/// `约束`：与 声明.type_params 逐位对齐的特性约束（`<T: 可比较>` → Some("可比较")）。
#[derive(Clone)]
pub struct 泛型函数模板 {
    pub 声明: crate::parser::ast::FunctionDeclaration,
    pub 包: Option<String>,
    pub 约束: Vec<Option<String>>,
}

/// 特性信息：方法签名表（含默认体 AST，供实现块合成缺省方法）。
/// v1 特性名全局共享（跨包不消歧，与泛型模板同款）。
#[derive(Clone)]
pub struct 特性信息 {
    pub 方法: Vec<crate::parser::ast::TraitMethod>,
}

/// 泛型实例名嵌套深度上限（`盒装$盒装$…` 中 `$` 的个数）。
/// 递归泛型（盒<盒<T>>）的无限展开防护。
const 泛型深度上限: usize = 8;

/// 作用域符号表。函数/结构体/方法签名全局共享；变量类型按块作用域压栈。
#[derive(Default)]
pub struct 符号表 {
    pub 函数: HashMap<String, 函数签名>,
    /// 包内函数签名：(包名, 函数名) → 该名字的**重载集**（按元数区分的多签名）。
    /// 绝大多数名字只有一个元素；同名不同元数的多定义构成重载。
    pub 函数按包: HashMap<(String, String), Vec<函数签名>>,
    /// 结构体注册表：索引即 Qi类型::结构体(idx) 的 idx。
    pub 结构体: Vec<结构体信息>,
    /// (声明包, 结构体名) → 索引。跨包同名结构体各占一条，互不干扰。
    结构体键索引: HashMap<(Option<String>, String), u32>,
    /// 结构体名 → 所有同名索引（无限定名解析的全局唯一性判定 / 歧义诊断用）。
    结构体同名: HashMap<String, Vec<u32>>,
    /// 枚举注册表：索引即 Qi类型::枚举/装箱枚举(idx) 的 idx。
    pub 枚举: Vec<枚举信息>,
    /// (声明包, 枚举名) → 索引。跨包同名枚举各占一条。
    枚举键索引: HashMap<(Option<String>, String), u32>,
    /// 枚举名 → 所有同名索引（无限定名解析 / 歧义诊断用）。
    枚举同名: HashMap<String, Vec<u32>>,
    /// destructure 导入映射：(使用方包, 符号名) → 来源包名。
    /// `导入 Web::{应用}` 让 使用方 里裸名 `应用` 优先解析到 Web 包
    /// （结构体与函数共用 —— 跨包同名符号靠它消歧）。
    符号导入: HashMap<(Option<String>, String), String>,
    /// 同一使用方对同名符号 destructure 导入了多个来源 → 歧义，禁用导入映射。
    符号导入歧义: HashSet<(Option<String>, String)>,
    /// 方法签名：(结构体索引, 方法名) → 签名（参数不含接收者）。
    /// 按索引（而非名字）挂 —— 跨包同名结构体的方法互不串位。
    pub 方法: HashMap<(u32, String), 函数签名>,
    /// 函数默认参数值 AST：函数名 → 每个形参的可选默认值表达式（供调用少传时补齐）。
    pub 函数默认值: HashMap<String, Vec<Option<crate::parser::ast::AstNode>>>,
    /// 函数形参名：函数名 → 形参名列表（签名只存类型，`工具模式` 生成 schema 需要名字）。
    pub 函数参数名: HashMap<String, Vec<String>>,
    /// 变参函数集合：末位形参是 `名字...: T`（签名里已按 数组(T) 登记），
    /// 调用点须把多余尾实参打包成数组后再传。
    pub 函数变参: HashSet<String>,
    /// 外部 C 函数名集合（`外部 "库" { ... }` 声明的）。签名同样存进 函数 表
    /// （供返回类型推断），但调用走 C ABI 专用降级、返回不纳入 ARC。
    pub 外部函数: HashSet<String>,
    /// 函数值签名注册表：索引即 Qi类型::函数值(idx) 的 idx。
    pub 函数值签名: Vec<函数签名>,
    /// 顶层函数名 → 其函数值签名索引（登记函数时预填，供「函数名当值」）。
    函数值索引表: HashMap<String, u32>,
    /// 当前正在生成的函数所属包名（用于跨包同名函数消歧）。
    pub 当前包: Option<String>,
    作用域: Vec<HashMap<String, Qi类型>>,
    // ───────── 用户泛型（单态化）─────────
    /// 泛型枚举模板：模板名 → 模板（跨包共享名字，v1 不做同名消歧）。
    pub 泛型枚举模板: HashMap<String, 泛型枚举模板>,
    /// 泛型结构体模板：模板名 → 模板。
    pub 泛型结构体模板: HashMap<String, 泛型结构体模板>,
    /// 泛型函数模板：模板名 → 模板 AST。
    pub 泛型函数模板: HashMap<String, 泛型函数模板>,
    /// 类型参数绑定栈：解析类型 时 `自定义类型("T")` 先查栈顶。
    /// 泛型函数实例体生成期间压该实例的绑定；实例化枚举/结构体模板时临时再压一层。
    类型参数栈: Vec<HashMap<String, Qi类型>>,
    // ───────── 特性（trait）─────────
    /// 特性注册表：特性名 → 方法签名表（含默认体）。
    pub 特性: HashMap<String, 特性信息>,
    /// 特性实现注册表：(特性名, 结构体索引) —— `实现 特性X 对于 类型Y` 收集。
    /// 泛型约束 `<T: 特性X>` 单态化时据此校验实参类型。
    pub 特性实现: HashSet<(String, u32)>,
}

impl 符号表 {
    pub fn new() -> Self {
        符号表 {
            函数: HashMap::new(),
            函数按包: HashMap::new(),
            结构体: Vec::new(),
            结构体键索引: HashMap::new(),
            结构体同名: HashMap::new(),
            枚举: Vec::new(),
            枚举键索引: HashMap::new(),
            枚举同名: HashMap::new(),
            符号导入: HashMap::new(),
            符号导入歧义: HashSet::new(),
            方法: HashMap::new(),
            函数默认值: HashMap::new(),
            函数参数名: HashMap::new(),
            函数变参: HashSet::new(),
            外部函数: HashSet::new(),
            函数值签名: Vec::new(),
            函数值索引表: HashMap::new(),
            当前包: None,
            作用域: vec![HashMap::new()],
            泛型枚举模板: HashMap::new(),
            泛型结构体模板: HashMap::new(),
            泛型函数模板: HashMap::new(),
            类型参数栈: Vec::new(),
            特性: HashMap::new(),
            特性实现: HashSet::new(),
        }
    }

    /// 名字是否为已声明的特性（且不与具体类型撞名 —— 结构体/枚举优先）。
    pub fn 是特性名(&self, name: &str) -> bool {
        self.特性.contains_key(name)
            && self.结构体索引(name).is_none()
            && self.枚举索引(name).is_none()
    }

    /// 类型 t 是否实现了特性（仅结构体可实现；其他类型一律否）。
    pub fn 类型实现特性(&self, t: Qi类型, 特性名: &str) -> bool {
        match t.结构体索引() {
            Some(idx) => self.特性实现.contains(&(特性名.to_string(), idx)),
            None => false,
        }
    }

    /// 面向用户报错的类型显示名（结构体给裸名，不带索引前缀）。
    pub fn 类型显示名(&self, t: Qi类型) -> String {
        match t {
            Qi类型::结构体(i) => self
                .结构体
                .get(i as usize)
                .map(|s| s.名字.clone())
                .unwrap_or_else(|| format!("结构体{}", i)),
            _ => self.类型名(t),
        }
    }

    /// 压入一层类型参数绑定（泛型实例体生成 / 模板实例化期间）。
    pub fn 压类型参数(&mut self, 绑定: HashMap<String, Qi类型>) {
        self.类型参数栈.push(绑定);
    }

    /// 弹出最近一层类型参数绑定。
    pub fn 弹类型参数(&mut self) {
        self.类型参数栈.pop();
    }

    /// 栈顶绑定里查类型参数名（只查最近一层 —— 模板实例化时外层函数的 T 不可见）。
    fn 查类型参数(&self, name: &str) -> Option<Qi类型> {
        self.类型参数栈.last().and_then(|m| m.get(name)).copied()
    }

    /// destructure 导入的来源包（歧义时 None）。
    pub fn 导入来源(&self, name: &str) -> Option<&str> {
        let key = (self.当前包.clone(), name.to_string());
        if self.符号导入歧义.contains(&key) {
            return None;
        }
        self.符号导入.get(&key).map(|s| s.as_str())
    }

    /// 定义了同名函数的所有包（排序保证确定性；歧义诊断用）。
    pub fn 函数候选包(&self, name: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .函数按包
            .keys()
            .filter(|(_, f)| f == name)
            .map(|(p, _)| p.clone())
            .collect();
        v.sort();
        v
    }

    /// 解析一个函数名对应的**重载集**（同一 (包,名) 下按元数区分的多个签名）：
    /// 1) 当前包内；2) destructure 导入来源包；3) 全局唯一定义包。
    /// 多包同名且无法定位 → None（不随机挑 —— 调用处按未定义/歧义报错）。
    pub fn 重载集(&self, name: &str) -> Option<&Vec<函数签名>> {
        if let Some(pkg) = &self.当前包 {
            if let Some(v) = self.函数按包.get(&(pkg.clone(), name.to_string())) {
                return Some(v);
            }
        }
        if let Some(src) = self.导入来源(name) {
            if let Some(v) = self.函数按包.get(&(src.to_string(), name.to_string())) {
                return Some(v);
            }
        }
        match self.函数候选包(name).as_slice() {
            [唯一] => self.函数按包.get(&(唯一.clone(), name.to_string())),
            _ => None,
        }
    }

    /// 解析函数签名（不区分重载 —— 取重载集第一个；重载均要求返回类型一致，
    /// 故返回类型推断用它安全）。无包定义时回退扁平表（无包程序 / 单文件）。
    pub fn 解析函数(&self, name: &str) -> Option<&函数签名> {
        if let Some(v) = self.重载集(name) {
            return v.first();
        }
        self.函数.get(name)
    }

    /// 按实参个数解析具体的那个重载。单一定义 → 直接返回它（保留默认参数/变参的
    /// 下游补齐语义）；多重载 → 按形参个数精确匹配（重载集内禁默认/变参，登记时已保证）。
    pub fn 解析重载(&self, name: &str, 实参数: usize) -> Option<&函数签名> {
        if let Some(v) = self.重载集(name) {
            if v.len() == 1 {
                return v.first();
            }
            return v.iter().find(|s| s.参数.len() == 实参数);
        }
        self.函数.get(name)
    }

    /// 函数是否存在（当前包优先）。
    pub fn 有函数(&self, name: &str) -> bool {
        self.解析函数(name).is_some()
    }

    /// 登记一个函数值签名，返回索引。
    pub fn 登记函数值签名(&mut self, sig: 函数签名) -> u32 {
        let idx = self.函数值签名.len() as u32;
        self.函数值签名.push(sig);
        idx
    }

    /// 按索引拿函数值签名。
    pub fn 函数值签名(&self, idx: u32) -> Option<&函数签名> {
        self.函数值签名.get(idx as usize)
    }

    /// 为一个顶层函数预登记「作为值」的签名索引（登记函数时调用）。
    pub fn 预登记函数值(&mut self, name: &str) {
        if self.函数值索引表.contains_key(name) {
            return;
        }
        if let Some(sig) = self.函数.get(name).cloned() {
            let idx = self.登记函数值签名(sig);
            self.函数值索引表.insert(name.to_string(), idx);
        }
    }

    /// 顶层函数名 → 其函数值类型（供「函数名当值」用）。immutable。
    pub fn 函数为值(&self, name: &str) -> Option<Qi类型> {
        self.函数值索引表.get(name).copied().map(Qi类型::函数值)
    }

    pub fn 进入作用域(&mut self) {
        self.作用域.push(HashMap::new());
    }

    pub fn 退出作用域(&mut self) {
        self.作用域.pop();
    }

    /// 在当前作用域声明变量类型。
    pub fn 声明变量(&mut self, name: &str, t: Qi类型) {
        if let Some(scope) = self.作用域.last_mut() {
            scope.insert(name.to_string(), t);
        }
    }

    /// 在全局作用域（栈底 作用域[0]）声明变量类型。全局无论何时可见。
    pub fn 声明全局(&mut self, name: &str, t: Qi类型) {
        if let Some(scope) = self.作用域.first_mut() {
            scope.insert(name.to_string(), t);
        }
    }

    /// 由内向外查找变量类型。
    pub fn 查变量(&self, name: &str) -> Option<Qi类型> {
        for scope in self.作用域.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(*t);
            }
        }
        None
    }

    /// 登记一个结构体，返回其索引。按 (包, 名字) 幂等 —— 同包同名只登记一次，
    /// **跨包同名各占独立索引**（布局/方法互不干扰）。
    pub fn 登记结构体(&mut self, 信息: 结构体信息) -> u32 {
        let key = (信息.包.clone(), 信息.名字.clone());
        if let Some(idx) = self.结构体键索引.get(&key) {
            return *idx;
        }
        let idx = self.结构体.len() as u32;
        self.结构体键索引.insert(key, idx);
        self.结构体同名
            .entry(信息.名字.clone())
            .or_default()
            .push(idx);
        self.结构体.push(信息);
        idx
    }

    /// 登记一条 destructure 导入映射：使用方包 里裸名 `名字` 来自 `来源包`。
    /// 同名多来源 → 记歧义（解析时禁用映射，落到全局唯一性判定）。
    pub fn 登记符号导入(&mut self, 使用方: Option<String>, 名字: &str, 来源包: &str) {
        let key = (使用方, 名字.to_string());
        match self.符号导入.get(&key) {
            Some(旧) if 旧 != 来源包 => {
                self.符号导入歧义.insert(key);
            }
            Some(_) => {}
            None => {
                self.符号导入.insert(key, 来源包.to_string());
            }
        }
    }

    /// 按名字解析结构体索引（带包上下文）：
    /// 1) 当前包自己声明的；
    /// 2) 当前包 destructure 导入的来源包声明的；
    /// 3) 全局唯一同名的。
    ///
    /// 多包同名且无法定位 → None（调用处报编译错误，见 结构体解析错误）——
    /// 宁报错不静默选错。
    pub fn 结构体索引(&self, name: &str) -> Option<u32> {
        let key = (self.当前包.clone(), name.to_string());
        if let Some(idx) = self.结构体键索引.get(&key) {
            return Some(*idx);
        }
        if let Some(src) = self.导入来源(name) {
            if let Some(idx) = self
                .结构体键索引
                .get(&(Some(src.to_string()), name.to_string()))
            {
                return Some(*idx);
            }
        }
        match self.结构体同名.get(name).map(|v| v.as_slice()) {
            Some([唯一]) => Some(*唯一),
            _ => None,
        }
    }

    /// 只查**当前包自己声明**的结构体（字段解析第二趟用：声明必然属于本包）。
    pub fn 本包结构体索引(&self, name: &str) -> Option<u32> {
        self.结构体键索引
            .get(&(self.当前包.clone(), name.to_string()))
            .copied()
    }

    /// 结构体名解析失败时的诊断消息：区分「未定义」与「跨包同名歧义」。
    pub fn 结构体解析错误(&self, name: &str) -> String {
        match self.结构体同名.get(name) {
            Some(v) if v.len() > 1 => {
                let 包们: Vec<String> = v
                    .iter()
                    .filter_map(|i| self.结构体.get(*i as usize))
                    .map(|s| s.包.clone().unwrap_or_else(|| "(无包)".to_string()))
                    .collect();
                format!(
                    "结构体名 {} 歧义：同名结构体定义于多个包（{}）。请用 `导入 包::{{{}}}` 指明来源",
                    name,
                    包们.join("、"),
                    name
                )
            }
            _ => format!("未定义的结构体: {}", name),
        }
    }

    /// 按索引拿结构体信息。
    pub fn 结构体信息(&self, idx: u32) -> Option<&结构体信息> {
        self.结构体.get(idx as usize)
    }

    // ───────────────────────── 枚举注册（与结构体同款） ─────────────────────────

    /// 登记一个枚举（按 (包, 名字) 幂等；跨包同名各占独立索引）。返回索引。
    pub fn 登记枚举(&mut self, 信息: 枚举信息) -> u32 {
        let key = (信息.包.clone(), 信息.名字.clone());
        if let Some(idx) = self.枚举键索引.get(&key) {
            return *idx;
        }
        let idx = self.枚举.len() as u32;
        self.枚举键索引.insert(key, idx);
        self.枚举同名
            .entry(信息.名字.clone())
            .or_default()
            .push(idx);
        self.枚举.push(信息);
        idx
    }

    /// 按名字解析枚举索引（当前包 → 导入来源 → 全局唯一），与 结构体索引 同款。
    pub fn 枚举索引(&self, name: &str) -> Option<u32> {
        let key = (self.当前包.clone(), name.to_string());
        if let Some(idx) = self.枚举键索引.get(&key) {
            return Some(*idx);
        }
        if let Some(src) = self.导入来源(name) {
            if let Some(idx) = self
                .枚举键索引
                .get(&(Some(src.to_string()), name.to_string()))
            {
                return Some(*idx);
            }
        }
        match self.枚举同名.get(name).map(|v| v.as_slice()) {
            Some([唯一]) => Some(*唯一),
            _ => None,
        }
    }

    /// 只查本包自己声明的枚举（变体解析第二趟用）。
    pub fn 本包枚举索引(&self, name: &str) -> Option<u32> {
        self.枚举键索引
            .get(&(self.当前包.clone(), name.to_string()))
            .copied()
    }

    /// 按索引拿枚举信息。
    pub fn 枚举信息(&self, idx: u32) -> Option<&枚举信息> {
        self.枚举.get(idx as usize)
    }

    /// 名字是否为已登记枚举类型（构造点消歧用）。
    pub fn 是枚举名(&self, name: &str) -> bool {
        self.枚举索引(name).is_some()
    }

    /// 枚举名 name 对应的 Qi 类型（装箱与否据注册表）。
    pub fn 枚举qi类型(&self, name: &str) -> Option<Qi类型> {
        let idx = self.枚举索引(name)?;
        let info = self.枚举信息(idx)?;
        Some(if info.装箱 {
            Qi类型::装箱枚举(idx)
        } else {
            Qi类型::枚举(idx)
        })
    }

    // ───────────────── 参数化枚举的按需单态实例化（选项/结果） ─────────────────
    //
    // 这是「用户泛型」的地基。实例化入口 `实例化参数枚举(模板名, 类型实参) -> 枚举索引`
    // 与具体的 选项/结果 解耦：它只按 (模板名, 已解析的类型实参) 造出一条具体枚举
    // 布局并登记（幂等）。未来用户 `枚举 X<T>{...}` 只需：把模板变体表存进注册表，
    // 这里按模板名查表 + 逐变体载荷做 T→实参 替换即可，无需为每个泛型名写分支。

    /// 一个已解析类型的稳定命名片段（用于实例枚举的唯一符号名，如 选项$整数）。
    /// 结构体/枚举带注册表索引保证跨包唯一；嵌套实例天然复用内层实例名。
    pub fn 类型名(&self, t: Qi类型) -> String {
        match t {
            Qi类型::整数 => "整数".to_string(),
            Qi类型::浮点数 => "浮点数".to_string(),
            Qi类型::布尔 => "布尔".to_string(),
            Qi类型::字符串 => "字符串".to_string(),
            Qi类型::空 => "空".to_string(),
            Qi类型::指针 => "指针".to_string(),
            Qi类型::结构体(i) => self
                .结构体
                .get(i as usize)
                .map(|s| format!("结构体{}_{}", i, s.名字))
                .unwrap_or_else(|| format!("结构体{}", i)),
            Qi类型::枚举(i) | Qi类型::装箱枚举(i) => self
                .枚举
                .get(i as usize)
                .map(|e| e.名字.clone())
                .unwrap_or_else(|| format!("枚举{}", i)),
            Qi类型::函数值(i) => format!("函数{}", i),
            Qi类型::数组(_) => "数组".to_string(),
            Qi类型::通道(_) => "通道".to_string(),
            Qi类型::未来(_) => "未来".to_string(),
            Qi类型::未知 => "未知".to_string(),
        }
    }

    /// (模板名, 类型实参) 的稳定实例名：`盒装$字符串`、`对$整数`、`映射$整数$字符串`。
    /// 嵌套实例天然复用内层实例名（`盒装$对$整数`）。
    pub fn 泛型实例名(&self, 模板: &str, 实参: &[Qi类型]) -> String {
        let 片段: Vec<String> = 实参.iter().map(|t| self.类型名(*t)).collect();
        format!("{}${}", 模板, 片段.join("$"))
    }

    /// 递归泛型的深度防护：实例名里 `$` 超过上限即报错（人话）。
    pub fn 泛型深度检查(&self, 实例名: &str) -> Result<(), String> {
        if 实例名.matches('$').count() > 泛型深度上限 {
            return Err(format!(
                "泛型嵌套过深（超过 {} 层）：{}。请检查是否存在无限递归的泛型实例化（如 盒<盒<T>> 反复自我包装）",
                泛型深度上限,
                super::枚举::枚举显示名(实例名)
            ));
        }
        Ok(())
    }

    /// 按需把一个参数化枚举模板单态化成具体枚举，登记进注册表，返回其索引。
    /// 幂等（同 模板+实参 复用同一实例，靠 登记枚举 按 (包=None, 实例名) 去重）。
    ///
    /// 内建两个模板：
    ///   选项<T> = { 有(T), 无 }
    ///   结果<T> = { 成(T), 败(字符串) }   // 错误类型固定字符串
    /// 两者恒装箱（有/成 带载荷）。`类型实参[0]` 即 T。
    ///
    /// 其余模板名查 泛型枚举模板 注册表（用户 `枚举 X<T>{...}`）：
    /// 先按模板载荷个数登记占位布局（自引用可解析、装箱标志已定），
    /// 再压 T→实参 绑定逐变体解析载荷类型，回填。
    pub fn 实例化参数枚举(
        &mut self,
        模板: &str,
        类型实参: &[Qi类型],
    ) -> Result<u32, String> {
        let 元素 = 类型实参.first().copied().unwrap_or(Qi类型::未知);
        let 实例名 = self.泛型实例名(模板, 类型实参);
        self.泛型深度检查(&实例名)?;
        let 变体: Vec<枚举变体信息> = match 模板 {
            "选项" => vec![
                枚举变体信息 {
                    名字: "有".to_string(),
                    tag: 0,
                    载荷: vec![元素],
                },
                枚举变体信息 {
                    名字: "无".to_string(),
                    tag: 1,
                    载荷: vec![],
                },
            ],
            "结果" => vec![
                枚举变体信息 {
                    名字: "成".to_string(),
                    tag: 0,
                    载荷: vec![元素],
                },
                枚举变体信息 {
                    名字: "败".to_string(),
                    tag: 1,
                    载荷: vec![Qi类型::字符串],
                },
            ],
            _ => return self.实例化用户枚举(模板, 类型实参, &实例名),
        };
        Ok(self.登记枚举(枚举信息 {
            名字: 实例名,
            包: None,
            变体,
            装箱: true,
            最大载荷槽: 1,
        }))
    }

    /// 用户泛型枚举模板 → 具体实例（实例化参数枚举 的用户模板分支）。
    fn 实例化用户枚举(
        &mut self,
        模板: &str,
        类型实参: &[Qi类型],
        实例名: &str,
    ) -> Result<u32, String> {
        let t = self
            .泛型枚举模板
            .get(模板)
            .cloned()
            .ok_or_else(|| format!("未知的参数化枚举模板: {}", 模板))?;
        if 类型实参.len() != t.类型参数.len() {
            return Err(format!(
                "泛型枚举 {} 需要 {} 个类型实参，实际给了 {} 个",
                模板,
                t.类型参数.len(),
                类型实参.len()
            ));
        }
        // 幂等：已实例化过直接复用
        if let Some(idx) = self.枚举键索引.get(&(None, 实例名.to_string())) {
            return Ok(*idx);
        }
        // 1) 先按占位登记（装箱/最大载荷槽 由模板载荷个数决定，与 T 无关）——
        //    自引用模板（满(盒装<T>)）解析载荷时能查到自己。
        let 装箱 = t.变体.iter().any(|(_, p)| !p.is_empty());
        let 最大载荷槽 = t.变体.iter().map(|(_, p)| p.len()).max().unwrap_or(0);
        let 占位变体: Vec<枚举变体信息> = t
            .变体
            .iter()
            .enumerate()
            .map(|(i, (名, _))| 枚举变体信息 {
                名字: 名.clone(),
                tag: i as i64,
                载荷: vec![],
            })
            .collect();
        let idx = self.登记枚举(枚举信息 {
            名字: 实例名.to_string(),
            包: None,
            变体: 占位变体,
            装箱,
            最大载荷槽,
        });
        // 2) 压 T→实参 绑定，逐变体解析载荷类型，回填
        let 绑定: HashMap<String, Qi类型> = t
            .类型参数
            .iter()
            .cloned()
            .zip(类型实参.iter().copied())
            .collect();
        self.压类型参数(绑定);
        let mut 变体: Vec<枚举变体信息> = Vec::new();
        for (i, (名, 载荷节点)) in t.变体.iter().enumerate() {
            let 载荷: Vec<Qi类型> = 载荷节点.iter().map(|tn| self.解析类型(tn)).collect();
            变体.push(枚举变体信息 {
                名字: 名.clone(),
                tag: i as i64,
                载荷,
            });
        }
        self.弹类型参数();
        self.枚举[idx as usize].变体 = 变体;
        Ok(idx)
    }

    /// 用户泛型结构体模板 → 具体实例，登记进结构体注册表，返回索引。幂等。
    /// LLVM 具名 struct 类型由 后端::取结构体llvm 惰性补建（见 结构体.rs）。
    pub fn 实例化参数结构体(
        &mut self,
        模板: &str,
        类型实参: &[Qi类型],
    ) -> Result<u32, String> {
        let t = self
            .泛型结构体模板
            .get(模板)
            .cloned()
            .ok_or_else(|| format!("未知的泛型结构体模板: {}", 模板))?;
        if 类型实参.len() != t.类型参数.len() {
            return Err(format!(
                "泛型结构体 {} 需要 {} 个类型实参，实际给了 {} 个",
                模板,
                t.类型参数.len(),
                类型实参.len()
            ));
        }
        let 实例名 = self.泛型实例名(模板, 类型实参);
        self.泛型深度检查(&实例名)?;
        if let Some(idx) = self.结构体键索引.get(&(None, 实例名.clone())) {
            return Ok(*idx);
        }
        // 先占位登记（自引用可解析），再绑定解析字段类型回填
        let 字段名: Vec<String> = t.字段.iter().map(|(n, _)| n.clone()).collect();
        let 占位: Vec<Qi类型> = t.字段.iter().map(|_| Qi类型::整数).collect();
        let idx = self.登记结构体(结构体信息 {
            名字: 实例名,
            包: None,
            字段名,
            字段类型: 占位,
        });
        let 绑定: HashMap<String, Qi类型> = t
            .类型参数
            .iter()
            .cloned()
            .zip(类型实参.iter().copied())
            .collect();
        self.压类型参数(绑定);
        let 字段类型: Vec<Qi类型> = t.字段.iter().map(|(_, tn)| self.解析类型(tn)).collect();
        self.弹类型参数();
        self.结构体[idx as usize].字段类型 = 字段类型;
        Ok(idx)
    }

    /// 通用入口：按模板种类把 (模板名, 类型实参) 单态化成具体 Qi 类型。
    /// 结构体模板 → 结构体(idx)；枚举模板（含内建 选项/结果）→ 枚举/装箱枚举(idx)。
    pub fn 实例化参数类型(
        &mut self,
        模板: &str,
        类型实参: &[Qi类型],
    ) -> Result<Qi类型, String> {
        if self.泛型结构体模板.contains_key(模板) {
            return self.实例化参数结构体(模板, 类型实参).map(Qi类型::结构体);
        }
        if 模板 == "选项" || 模板 == "结果" || self.泛型枚举模板.contains_key(模板)
        {
            let idx = self.实例化参数枚举(模板, 类型实参)?;
            let 装箱 = self.枚举信息(idx).map(|e| e.装箱).unwrap_or(true);
            return Ok(if 装箱 {
                Qi类型::装箱枚举(idx)
            } else {
                Qi类型::枚举(idx)
            });
        }
        Err(format!(
            "未定义的泛型类型: {}。可用形式：用户声明的 枚举 {}<T> / 类型 {}<T>，或内建 选项<T> / 结果<T>",
            模板, 模板, 模板
        ))
    }

    /// 解析类型注解为 Qi 类型，自定义类型解析成 结构体(idx)，函数类型解析成 函数值(idx)。
    /// 需 &mut 因为函数类型会登记新签名。
    pub fn 解析类型(&mut self, t: &crate::parser::ast::TypeNode) -> Qi类型 {
        use crate::parser::ast::TypeNode;
        match t {
            // 选项<T> / 结果<T>：按需单态实例化 → 装箱枚举(idx)。注解是规则①②③的定型来源。
            TypeNode::选项类型(ot) => {
                let 元素 = self.解析类型(&ot.inner_type);
                self.实例化参数枚举("选项", &[元素])
                    .map(Qi类型::装箱枚举)
                    .unwrap_or(Qi类型::未知)
            }
            TypeNode::结果类型(rt) => {
                // 错误类型固定字符串（v1），只取成功类型 T
                let ok = self.解析类型(&rt.ok_type);
                self.实例化参数枚举("结果", &[ok])
                    .map(Qi类型::装箱枚举)
                    .unwrap_or(Qi类型::未知)
            }
            TypeNode::自定义类型(name)
            | TypeNode::结构体类型(crate::parser::ast::StructType { name, .. }) => self
                // 泛型体内的 T：先查类型参数绑定（实例体生成/模板实例化期间压栈）
                .查类型参数(name)
                .or_else(|| self.结构体索引(name).map(Qi类型::结构体))
                .or_else(|| self.枚举qi类型(name))
                .unwrap_or(Qi类型::未知),
            // 用户泛型类型注解：对<整数> / 盒装<字符串> / 嵌套 盒装<对<整数> >
            // 递归解析实参后按模板种类单态实例化。解析失败 → 未知（调用处报错）。
            TypeNode::泛型类型(g) => {
                let 实参: Vec<Qi类型> = g.type_arguments.iter().map(|t| self.解析类型(t)).collect();
                self.实例化参数类型(&g.base_type, &实参)
                    .unwrap_or(Qi类型::未知)
            }
            TypeNode::枚举类型(crate::parser::ast::EnumType { name, .. }) => {
                self.枚举qi类型(name).unwrap_or(Qi类型::未知)
            }
            TypeNode::函数类型(ft) => {
                let 参数: Vec<Qi类型> = ft.parameters.iter().map(|p| self.解析类型(p)).collect();
                let 返回 = self.解析类型(&ft.return_type);
                let idx = self.登记函数值签名(函数签名 { 参数, 返回 });
                Qi类型::函数值(idx)
            }
            // 数组<T>：元素类型定型 → 数组(元素)
            TypeNode::数组类型(at) => {
                let 元素 = super::类型::元素类型::从标量(self.解析类型(&at.element_type));
                Qi类型::数组(元素)
            }
            TypeNode::基础类型(crate::parser::ast::BasicType::数组) => {
                Qi类型::数组(super::类型::元素类型::整数)
            }
            // 通道<T>：句柄指针 + 元素类型（收发两端据元素类型做 i64 位模式装/还原）
            TypeNode::通道类型(ct) => {
                let 元素 = super::类型::元素类型::从标量(self.解析类型(&ct.element_type));
                Qi类型::通道(元素)
            }
            // 未来<T>：eager future 句柄，内部类型 T 记进 Qi类型::未来
            TypeNode::未来类型(inner) => {
                Qi类型::未来(super::类型::元素类型::从标量(self.解析类型(inner)))
            }
            _ => Qi类型::从注解(t),
        }
    }

    /// 从内建/用户函数名推断返回类型（当前包优先）。
    /// 用户函数没有时回退标准库注册表（无限定名，如 `MD5哈希(x)`）——
    /// 否则 `变量 h = MD5哈希(x)` 推断成 未知→整数，字符串返回值被按整数打印（指针地址）。
    pub fn 查函数返回(&self, callee: &str) -> Option<Qi类型> {
        if let Some(sig) = self.解析函数(callee) {
            return Some(sig.返回);
        }
        内建返回类型(callee).or_else(|| 标准库函数返回(callee.trim_start_matches(':')))
    }
}

/// 注解 TypeNode → 元素类型（**不可变**解析，供 推断表达式类型 的通道创建臂用）。
/// 基础类型走 从注解；自定义类型查注册表（已登记的结构体/枚举）；
/// 泛型注解（实例可能尚未存在）保守按 指针。
fn 注解元素类型(
    t: &crate::parser::ast::TypeNode,
    表: &符号表,
) -> super::类型::元素类型 {
    use super::类型::元素类型;
    use crate::parser::ast::TypeNode;
    match t {
        TypeNode::自定义类型(name) => {
            if let Some(i) = 表.结构体索引(name) {
                return 元素类型::结构体(i);
            }
            match 表.枚举qi类型(name) {
                Some(Qi类型::装箱枚举(_)) => 元素类型::指针,
                Some(Qi类型::枚举(_)) => 元素类型::整数,
                _ => 元素类型::整数,
            }
        }
        TypeNode::泛型类型(_) => 元素类型::指针,
        _ => 元素类型::从标量(Qi类型::从注解(t)),
    }
}

/// 类型推断专用的标准库注册表单例（内容与后端持有的实例一致，仅查签名不产生 IR）。
fn 推断注册表() -> &'static ModuleRegistry {
    static REG: OnceLock<ModuleRegistry> = OnceLock::new();
    REG.get_or_init(ModuleRegistry::new)
}

/// 无模块限定的标准库函数返回类型（镜像 codegen 的 尝试无限定标准库）。
/// 仅当**所有**同名注册项映射到同一 Qi 类型时才给答案；有歧义（如 长度/获取
/// 在不同模块返回类型不同）→ None，交上层维持原默认，避免与 codegen 的
/// 首命中选择不一致。
pub fn 标准库函数返回(name: &str) -> Option<Qi类型> {
    let reg = 推断注册表();
    let mut found: Option<Qi类型> = None;
    for path in reg.module_paths() {
        if let Some(m) = reg.get_module(path) {
            if let Some(f) = m.get_function(name) {
                let t = super::导入::注册表类型转qi(&f.return_type);
                match found {
                    None => found = Some(t),
                    Some(prev) if prev == t => {}
                    Some(_) => return None, // 歧义：不猜
                }
            }
        }
    }
    found
}

/// 模块限定的标准库调用返回类型：`加密.MD5哈希(x)` / `字符串::长度(s)`。
/// 接收者是模块名（精确匹配注册表 key，含 `标准库.X` 变体）时查其签名。
fn 标准库模块方法返回(module: &str, method: &str) -> Option<Qi类型> {
    let reg = 推断注册表();
    let method = method.trim_start_matches(':');
    reg.get_function(module, method)
        .or_else(|| reg.get_function(&format!("标准库.{}", module), method))
        .map(|f| super::导入::注册表类型转qi(&f.return_type))
}

/// 内建（运行时）函数的返回类型。
pub fn 内建返回类型(callee: &str) -> Option<Qi类型> {
    Some(match callee {
        "整数转字符串" | "int_to_string" | "浮点数转字符串" | "float_to_string" => {
            Qi类型::字符串
        }
        "字符串转整数" | "string_to_int" => Qi类型::整数,
        "字符串转浮点数" | "string_to_float" => Qi类型::浮点数,
        "整数转浮点数" | "int_to_float" => Qi类型::浮点数,
        "浮点数转整数" | "float_to_int" => Qi类型::整数,
        _ => return None,
    })
}

/// 推断表达式的 Qi 类型（不产生 IR，仅用于选指令 / 声明变量 / 找方法）。
pub fn 推断表达式类型(node: &AstNode, 表: &符号表) -> Qi类型 {
    match node {
        AstNode::字面量表达式(lit) => match &lit.value {
            LiteralValue::整数(_) => Qi类型::整数,
            LiteralValue::浮点数(_) => Qi类型::浮点数,
            LiteralValue::字符串(_) => Qi类型::字符串,
            LiteralValue::布尔(_) => Qi类型::布尔,
            LiteralValue::字符(_) => Qi类型::整数,
        },
        AstNode::标识符表达式(id) => 表
            .查变量(&id.name)
            .or_else(|| 表.函数为值(&id.name)) // 变量没有则看是不是顶层函数名（当值用）
            .unwrap_or(Qi类型::未知),
        AstNode::二元操作表达式(b) => {
            use BinaryOperator::*;
            match b.operator {
                等于 | 不等于 | 大于 | 小于 | 大于等于 | 小于等于 | 与 | 或 => {
                    Qi类型::布尔
                }
                // 位运算只对整数合法，结果恒为整数
                位与 | 位或 | 位异或 | 左移 | 右移 => Qi类型::整数,
                加 | 减 | 乘 | 除 | 取余 => {
                    let l = 推断表达式类型(&b.left, 表);
                    let r = 推断表达式类型(&b.right, 表);
                    if b.operator == 加 && (l == Qi类型::字符串 || r == Qi类型::字符串) {
                        Qi类型::字符串
                    } else if l == Qi类型::浮点数 || r == Qi类型::浮点数 {
                        Qi类型::浮点数
                    } else {
                        Qi类型::整数
                    }
                }
            }
        }
        AstNode::字符串连接表达式(_) => Qi类型::字符串,
        AstNode::格式字符串表达式(_) => Qi类型::字符串,
        AstNode::一元操作表达式(u) => 推断表达式类型(&u.operand, 表),
        AstNode::函数调用表达式(call) => {
            // 优先：callee 是局部函数值变量 → 用其签名返回类型（间接调用）
            if let Some(t) = 表.查变量(&call.callee) {
                if let Some(idx) = t.函数值索引() {
                    if let Some(sig) = 表.函数值签名(idx) {
                        return sig.返回;
                    }
                }
            }
            表.查函数返回(&call.callee).unwrap_or(Qi类型::未知)
        }
        AstNode::赋值表达式(a) => 推断表达式类型(&a.value, 表),
        AstNode::结构体实例化表达式(lit) => 表
            .结构体索引(&lit.struct_name)
            .map(Qi类型::结构体)
            .unwrap_or(Qi类型::未知),
        AstNode::字段访问表达式(fa) => {
            // 枚举构造 `颜色.红`：接收者是枚举名（非变量）且字段是其变体 → 枚举类型
            if let AstNode::标识符表达式(id) = fa.object.as_ref() {
                if 表.查变量(&id.name).is_none() && 表.是枚举名(&id.name) {
                    if let Some(idx) = 表.枚举索引(&id.name) {
                        if 表.枚举信息(idx).and_then(|e| e.查变体(&fa.field)).is_some() {
                            return 表.枚举qi类型(&id.name).unwrap_or(Qi类型::未知);
                        }
                    }
                }
            }
            let obj = 推断表达式类型(&fa.object, 表);
            if let Some(idx) = obj.结构体索引() {
                if let Some(info) = 表.结构体信息(idx) {
                    if let Some((_, ft)) = info.查字段(&fa.field) {
                        return ft;
                    }
                }
            }
            Qi类型::未知
        }
        AstNode::方法调用表达式(mc) => {
            // 枚举构造 `形状.圆(3.14)`：接收者是枚举名（非变量）且方法是带载荷变体 → 枚举类型
            if let AstNode::标识符表达式(id) = mc.object.as_ref() {
                if 表.查变量(&id.name).is_none() && 表.是枚举名(&id.name) {
                    if let Some(idx) = 表.枚举索引(&id.name) {
                        if 表
                            .枚举信息(idx)
                            .and_then(|e| e.查变体(&mc.method_name))
                            .is_some()
                        {
                            return 表.枚举qi类型(&id.name).unwrap_or(Qi类型::未知);
                        }
                    }
                }
            }
            // 接收者是任意表达式（链式关键）：先定其类型，再查方法返回。
            let recv = 推断表达式类型(&mc.object, 表);
            if let Some(idx) = recv.结构体索引() {
                if let Some(sig) = 表.方法.get(&(idx, mc.method_name.clone())) {
                    return sig.返回;
                }
                // 函数值字段以方法语法调用：`命令值.执行函数(ctx)` → 字段签名的返回类型
                if let Some(info) = 表.结构体信息(idx) {
                    if let Some((_, Qi类型::函数值(fi))) = info.查字段(&mc.method_name) {
                        if let Some(sig) = 表.函数值签名(fi) {
                            return sig.返回;
                        }
                    }
                }
            }
            // 接收者是模块名（非变量的裸标识符）：标准库注册表签名 ——
            // `变量 h = 加密.MD5哈希(x)` 不带注解时也要推出 字符串。
            if let AstNode::标识符表达式(id) = mc.object.as_ref() {
                if 表.查变量(&id.name).is_none() {
                    if let Some(t) = 标准库模块方法返回(&id.name, &mc.method_name) {
                        return t;
                    }
                }
            }
            Qi类型::未知
        }
        AstNode::数组字面量表达式(arr) => {
            let elem = arr
                .elements
                .first()
                .map(|e| super::类型::元素类型::从标量(推断表达式类型(e, 表)))
                .unwrap_or(super::类型::元素类型::整数);
            Qi类型::数组(elem)
        }
        AstNode::数组访问表达式(acc) => {
            let arr = 推断表达式类型(&acc.array, 表);
            arr.数组元素().map(|e| e.标量()).unwrap_or(Qi类型::整数)
        }
        // 通道创建 → 通道(元素类型)；通道接收 → 元素标量类型
        AstNode::通道创建表达式(c) => Qi类型::通道(注解元素类型(&c.element_type, 表)),
        AstNode::通道接收表达式(r) => 推断表达式类型(&r.channel, 表)
            .通道元素()
            .map(|e| e.标量())
            .unwrap_or(Qi类型::整数),
        // 取地址 → 指针（按 字符串/ptr 语义）；解引用 → 整数（指针演示够用）
        AstNode::取地址表达式(_) => Qi类型::字符串,
        AstNode::解引用表达式(_) => Qi类型::整数,
        // 等待 fut → fut 的内部类型
        AstNode::等待表达式(a) => {
            let ft = 推断表达式类型(&a.expression, 表);
            ft.未来内部().map(|e| e.标量()).unwrap_or(Qi类型::整数)
        }
        _ => Qi类型::未知,
    }
}
