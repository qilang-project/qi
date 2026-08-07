//! 全编译单元·宽容语义类型检查器（渐进接入版）
//!
//! # 设计原则：宽容默认（permissive-by-default）
//!
//! 检查器对**不认识的东西必须沉默**：
//! - 未知表达式节点 → 返回未知类型（`None`）且不报错；
//! - 未知函数名，只要「可能来自未播种的来源」（标准库注册表缺口、第三方包、
//!   模块限定调用、导入符号、类型静态方法等）→ 不报未定义；
//! - 只有**有把握确定是错**的才报（红码合同见 qi/tests/类型检查红码/）。
//!
//! 这是渐进接入的生命线：覆盖面逐步长，假阳性始终为零。宁可漏报，不可误报。
//!
//! # 两遍结构
//!
//! 1. **声明收集 pass**：对整个编译单元（entry + 全部被导入模块的 Program）
//!    收集函数/结构体/枚举/特性/联合体/外部 FFI/全局变量签名进全局表——
//!    跨文件符号（qi-web 包内互调、用户 .qi 包函数）由此可见。
//! 2. **检查 pass**：逐 Program 建立其导入环境（标准库别名/用户包别名/
//!    destructure 符号），再走语句/表达式做宽容检查。
//!
//! # 类型真值来源
//!
//! - 内置函数（打印族/类型转换/同步原语/协程原语/AI 语法糖）：镜像
//!   codegen（inkwell_gen/表达式.rs、并发.rs、协程.rs）的特殊分发名单；
//! - 标准库模块函数：直接复用 codegen 的 `ModuleRegistry`（不手抄第二份）；
//!   注册表返回类型按**保守映射**——`整数` 常是句柄语义，映射为未知，
//!   只有 字符串/浮点数/布尔/空 可信；
//! - 用户函数/结构体/枚举：AST 声明原文。
//!
//! # 报错家族（只在两侧类型都有把握时比对）
//!
//! 数值（整数族/浮点数/布尔/字符）｜字符串｜容器（数组/列表/集合/字典）
//! 三族之间的交叉赋值/传参/返回才报 TypeMismatch；涉及「其他」（结构体/
//! 枚举/未来/选项/结果/通道/指针/函数值/空/自定义）一律沉默。
//!
//! # 字面量来源门槛（家族比对的开火条件）
//!
//! 家族错配只对「字面量来源」的值开火：字面量本身、字面量组合表达式、
//! 以及**由字面量初始化且未被非字面量覆写的局部变量**（字面量变量追踪，
//! 见 `设字面量`/`是字面量变量`）。函数返回值/句柄等非字面量来源一律沉默
//! ——绿码句柄惯用法（字符串描述符存 整数 变量）的生命线。
//!
//! # 沉默清单（已知取舍，2026-07 波2 召回提升后修订）
//!
//! 1. 模块限定调用 miss 不报（注册表可能不全），且不查元数；
//! 2. 值接收者方法：**本单元声明的方法查元数**（2026-07 起）；特性默认方法/
//!    嵌入方法/内建通用方法（s.长度() 等）仍沉默；返回未知时下游沉默；
//! 3. 注册表返回 整数/ptr/数组 → 未知（句柄语义）；
//! 4. 非字面量来源（含非字面量变量）的家族错配放行（句柄惯用法）；
//! 5. 闭包/匹配/异步块的值类型未知；间接调用（函数值）不查元数与类型；
//! 6. 模板串洞**部分开火**（波2）：洞原文经 ExprParser 真解析后查
//!    未定义变量/未定义函数；家族比对不开，解析失败沉默，其余报错全弃；
//!    特性默认实现体报错全弃（接收者环境不完整）；
//! 7. 元数重载命中 >1 候选：**全部候选同位同族**时比对字面量实参（波2）；
//!    任何一个候选该位是变参/泛型/未知/异族 → 沉默；泛型实参比对关；
//! 8. 接收者为不可解析裸标识符 → 沉默（可能是未登记包别名）；
//! 9. 用户函数与内置同名 → 沉默；内置/注册表函数不查元数；
//! 10. 条件不要求布尔（qi 惯用整数条件，语言语义未定，不动）；二元操作数
//!     不比对；结构体字面量缺字段不查（codegen 默认零值=特性）；
//!     同作用域重复声明不查（重入声明=特性）；void 带值返回不查
//!     （codegen 求值丢弃后 ret void=合法特性，见 inkwell_gen/语句.rs）。
//!     波2 已收窄：枚举变体载荷元数+**类型**查（字面量来源、跨枚举重名/泛型
//!     载荷沉默）；常量重赋值查（重入声明不误伤）；纯字面量数组混族查
//!     （任一元素非直接字面量 → 沉默）。

use crate::codegen::module_registry::ModuleRegistry;
use crate::parser::ast::*;
use crate::semantic::type_checker::TypeError;
use crate::semantic::诊断渲染::类型显示;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

// 表达式层（推断/调用/成员访问）拆在子模块（同一检查器的另一半 impl，
// 子模块可直接访问本模块私有状态）——单文件 <1000 行约束。
#[path = "单元检查_检查项.rs"]
mod 检查项;
#[path = "单元检查_表达式.rs"]
mod 表达式;

/// 推断结果：`None` = 未知。未知参与的任何比对一律沉默。
type 推断 = Option<TypeNode>;

/// 标准库注册表单例（与 codegen 同一份真值，只读复用）。
fn 注册表() -> &'static ModuleRegistry {
    static REG: OnceLock<ModuleRegistry> = OnceLock::new();
    REG.get_or_init(ModuleRegistry::new)
}

/// 注册表返回类型 → 推断（保守映射）：
/// 只有明确的 字符串/浮点数/布尔/空 可信；`整数` 在注册表里常是**句柄**
/// （哈希表/文件/数据库连接…），映射为未知，避免「句柄赋给字典变量」类误报。
fn 注册表返回保守(t: &str) -> 推断 {
    match t {
        "字符串" => Some(TypeNode::基础类型(BasicType::字符串)),
        "浮点数" | "double" => Some(TypeNode::基础类型(BasicType::浮点数)),
        "布尔" => Some(TypeNode::基础类型(BasicType::布尔)),
        "空" | "void" => Some(TypeNode::基础类型(BasicType::空)),
        _ => None, // 整数句柄 / ptr / 数组 / 其他 → 未知
    }
}

/// 无模块限定时在**所有**注册表模块里找同名函数（镜像 codegen 的 查任意模块函数）。
fn 注册表任意模块(name: &str) -> Option<推断> {
    let reg = 注册表();
    for path in reg.module_paths() {
        if let Some(m) = reg.get_module(path) {
            if let Some(f) = m.get_function(name) {
                return Some(注册表返回保守(&f.return_type));
            }
        }
    }
    None
}

/// 注册表**形参**类型 → 推断（保守）。
/// 与 注册表返回保守 不同：这里 `整数` 是可信的——句柄本身就是整数，
/// 传整数进去永远合法；能抓到的是「把 数组/字符串/浮点 传给整数形参」这类。
/// ptr/数组/未知类型串一律返回 None（不校验），避免误报。
fn 注册表形参保守(t: &str) -> 推断 {
    match t {
        "字符串" => Some(TypeNode::基础类型(BasicType::字符串)),
        "整数" | "i64" | "i32" => Some(TypeNode::基础类型(BasicType::整数)),
        "浮点数" | "double" => Some(TypeNode::基础类型(BasicType::浮点数)),
        "布尔" => Some(TypeNode::基础类型(BasicType::布尔)),
        _ => None,
    }
}

/// 查注册表函数签名：限定模块则只查该模块（含 `标准库.` 前缀形式），
/// 否则镜像 codegen 在所有模块里按名找第一个。返回 (形参类型串, 返回类型串)。
pub(super) fn 注册表签名(模块: Option<&str>, 名: &str) -> Option<(Vec<String>, String)> {
    let reg = 注册表();
    if let Some(m) = 模块 {
        let f = reg
            .get_function(m, 名)
            .or_else(|| reg.get_function(&format!("标准库.{}", m), 名))?;
        return Some((f.param_types.clone(), f.return_type.clone()));
    }
    for path in reg.module_paths() {
        if let Some(md) = reg.get_module(path) {
            if let Some(f) = md.get_function(名) {
                return Some((f.param_types.clone(), f.return_type.clone()));
            }
        }
    }
    None
}

/// 内置函数表（镜像 codegen 特殊分发）。返回 Some(返回类型推断) 表示是内置。
/// 覆盖：打印族、类型转换、同步/定时器原语、协程原语、AI 语法糖、内建长度。
fn 内置函数(name: &str) -> Option<推断> {
    match name {
        // 打印族（变参，任意类型）
        "打印行" | "打印" | "println" | "print" | "printf" => {
            Some(Some(TypeNode::基础类型(BasicType::空)))
        }
        // 内建类型转换（inkwell_gen/表达式.rs 生成内建调用）
        "整数转字符串" | "int_to_string" | "浮点数转字符串" | "float_to_string" => {
            Some(Some(TypeNode::基础类型(BasicType::字符串)))
        }
        "字符串转整数" | "string_to_int" => {
            Some(Some(TypeNode::基础类型(BasicType::整数)))
        }
        "字符串转浮点数" | "string_to_float" | "整数转浮点数" | "int_to_float" => {
            Some(Some(TypeNode::基础类型(BasicType::浮点数)))
        }
        "浮点数转整数" | "float_to_int" => {
            Some(Some(TypeNode::基础类型(BasicType::整数)))
        }
        "__qi_html正文值" | "__qi_html属性值" | "__qi_html条件值" | "__qi_html键值" => {
            Some(Some(TypeNode::基础类型(BasicType::字符串)))
        }
        // 内建 长度(x)：数组读头/串字节长（注册表里也有多个同名——统一按整数）
        "长度" => Some(Some(TypeNode::基础类型(BasicType::整数))),
        // 同步/定时器原语（inkwell_gen/并发.rs 生成同步内建）——句柄语义，返回未知
        "创建等待组" | "新建等待组" | "等待组增加" | "等待组添加" | "添加等待" | "等待组完成"
        | "完成" | "等待组等待" | "创建互斥锁" | "新建互斥锁" | "互斥锁加锁" | "互斥锁锁定"
        | "加锁" | "互斥锁解锁" | "解锁" | "尝试加锁" | "获取时间" | "设置超时" | "创建定时器"
        | "定时器过期" | "停止定时器" => Some(None),
        // 协程原语（inkwell_gen/协程.rs）
        "执行器运行全部"
        | "运行执行器"
        | "协程运行全部"
        | "执行器单步"
        | "协程单步"
        | "取消未来"
        | "取消协程"
        | "让出"
        | "yield"
        | "异步睡眠"
        | "睡眠"
        | "sleep" => Some(None),
        // AI 语法糖（inkwell_gen/表达式.rs 编译期改写）
        "询问" | "尝试询问" | "异步询问" | "工具模式" | "工具适配" | "填充模板" | "流式"
        | "嵌入" | "相似度" => Some(None),
        // 语言级 选项/结果 构造子（inkwell_gen/表达式.rs 构造子定型四规则）
        "有" | "成" | "败" => Some(None),
        _ => None,
    }
}

// 「字面量来源」判定挪到 单元检查器::是字面量来源（需要作用域内的
// 字面量变量追踪）——见 表达式 子模块调用点与下方实现。

/// 类型家族：只有 数值/字符串/容器 三族之间的交叉才报错；其他一律沉默。
#[derive(PartialEq, Clone, Copy)]
enum 家族 {
    数值,
    字符串族,
    容器,
    沉默, // 结构体/枚举/未来/选项/结果/通道/指针/函数/空/自定义 …
}

fn 类型家族(t: &TypeNode) -> 家族 {
    match t {
        TypeNode::基础类型(b) => match b {
            BasicType::整数
            | BasicType::长整数
            | BasicType::短整数
            | BasicType::字节
            | BasicType::浮点数
            | BasicType::布尔
            | BasicType::字符 => 家族::数值,
            BasicType::字符串 => 家族::字符串族,
            BasicType::数组 | BasicType::列表 | BasicType::集合 | BasicType::字典 => {
                家族::容器
            }
            _ => 家族::沉默,
        },
        TypeNode::数组类型(_)
        | TypeNode::列表类型(_)
        | TypeNode::集合类型(_)
        | TypeNode::字典类型(_) => 家族::容器,
        _ => 家族::沉默,
    }
}

/// 「已知结构体喂给标量形参」—— 这一类**绝不可能是对的**，单独判。
///
/// 家族矩阵把结构体/自定义类型一律归入「沉默」，宽容是对的（句柄惯用法、
/// 类型别名、泛型参数都在里面），但它顺带放过了一整类真错：
///
///     取查询(上下文值, "saved")     // 形参是 字符串，实参是 上下文
///
/// 编译得过，运行时恒返回空串 —— 表现是「保存后『已保存』永远不出现，
/// 而 cookie 其实存对了」，没有任何报错。
///
/// 一个结构体值不可能是字符串/数字/数组，所以只要**确认它是本单元声明过的
/// 结构体**（不是泛型参数 T、不是没见过的名字）就可以放心报。
fn 是已知结构体名(名: &str, 结构体表: &HashMap<String, 结构体信息>, 类型参数: &HashSet<String>) -> bool {
    !类型参数.contains(名) && 结构体表.contains_key(名)
}

/// 两个**已知**类型是否可放行（宽容矩阵）：任一侧家族为沉默 → 放行；
/// 同族 → 放行（整数↔浮点↔布尔互转、数组尺寸差异等都在族内）。
fn 家族相容(a: &TypeNode, b: &TypeNode) -> bool {
    let fa = 类型家族(a);
    let fb = 类型家族(b);
    fa == 家族::沉默 || fb == 家族::沉默 || fa == fb
}

/// 用户函数签名（含元数重载所需信息）。
#[derive(Clone)]
struct 函数签名 {
    参数: Vec<Parameter>,
    返回: Option<TypeNode>,
    类型参数: Vec<String>,
    变参: bool,
}

impl 函数签名 {
    fn 从函数声明(f: &FunctionDeclaration) -> Self {
        Self {
            参数: f.parameters.clone(),
            返回: f.return_type.clone(),
            类型参数: f.type_params.clone(),
            变参: f.parameters.iter().any(|p| p.is_variadic),
        }
    }

    /// 实参个数是否可被本签名接受（默认值可省、变参吃尾部任意多）。
    fn 接受个数(&self, n: usize) -> bool {
        let 必需 = self
            .参数
            .iter()
            .filter(|p| p.default_value.is_none() && !p.is_variadic)
            .count();
        if self.变参 {
            n >= 必需
        } else {
            n >= 必需 && n <= self.参数.len()
        }
    }
}

/// 结构体信息（字段名/类型检查所需的最小集）。
struct 结构体信息 {
    字段: Vec<StructField>,
    类型参数: Vec<String>,
    /// 有嵌入字段（Go 式匿名字段）时字段集不封闭 → 字段名检查沉默。
    有嵌入: bool,
}

/// 全编译单元检查器。见模块级文档。
pub struct 单元检查器 {
    // ===== 全局（跨文件）符号，声明收集 pass 填充 =====
    函数表: HashMap<String, Vec<函数签名>>,
    结构体表: HashMap<String, 结构体信息>,
    /// 枚举名 → 变体名集合
    枚举表: HashMap<String, HashSet<String>>,
    /// 变体名 → 所属枚举名（裸变体构造/引用；首见优先，重名不覆盖）
    变体归属: HashMap<String, String>,
    /// (枚举名, 变体名) → 载荷元数（`圆(浮点数)` → 1；无载荷 → 0）
    变体元数: HashMap<(String, String), usize>,
    /// (枚举名, 变体名) → 载荷类型列表（元数匹配后按位做字面量家族比对）
    变体载荷: HashMap<(String, String), Vec<TypeNode>>,
    /// 枚举名 → 泛型类型参数名（载荷类型涉及类型参数 → 该位比对沉默）
    枚举类型参数: HashMap<String, Vec<String>>,
    /// 跨枚举重名的变体名——裸构造归属歧义，元数检查沉默
    歧义变体: HashSet<String>,
    特性集: HashSet<String>,
    联合体集: HashSet<String>,
    /// (类型名, 方法名) → 方法签名重载集（结构体方法/实现块/独立方法声明）
    类型方法: HashMap<(String, String), Vec<函数签名>>,
    /// 顶层 变量/常量：名 → 类型（注解或字面量直推；推不出为未知）
    全局变量: HashMap<String, 推断>,
    /// 编译单元里出现过的包名（`包 X;`）——`导入 X;` 后 `X.函数(...)` 用
    包名集: HashSet<String>,

    // ===== 当前 Program 的导入环境（每个文件重建） =====
    /// 标准库别名：别名/模块名 → 注册表中文模块名
    模块别名: HashMap<String, String>,
    /// 非标准库导入（用户包/qi_packages 子模块）的别名与全路径——不透明，成员访问沉默
    不透明别名: HashSet<String>,
    /// destructure 导入的裸符号（`导入 Web::{行集, 查询器}`）——类型未知，调用沉默
    导入符号: HashSet<String>,

    // ===== 检查 pass 状态 =====
    作用域: Vec<HashMap<String, 推断>>,
    /// 与 作用域 平行：本层被标记为「字面量来源」的变量名。
    /// 变量由字面量初始化/赋值 → 标记；被非字面量赋值 → 清除。
    /// 家族比对对「解析到字面量变量的标识符」也开火（红码 b01/b12 类）。
    字面量变量: Vec<HashSet<String>>,
    /// 与 作用域 平行：本层以 `常量` 声明的名字。赋值目标解析到常量 → 报错。
    /// 同作用域「重入声明」是新声明（绑定 时清旧标记），不误伤。
    常量集: Vec<HashSet<String>>,
    /// 当前函数的泛型类型参数名（体内视为类型变量，涉及即未知）
    当前类型参数: HashSet<String>,
    /// 当前函数声明的返回类型（未声明/含类型参数 → None → 返回检查沉默）
    当前返回: Option<TypeNode>,
    错误: Vec<TypeError>,
}

/// 顶层全局变量的字面量直推（声明收集 pass 用，不依赖作用域/函数表）。
fn 字面量直推(node: &AstNode) -> 推断 {
    match node {
        AstNode::字面量表达式(l) => Some(match &l.value {
            LiteralValue::整数(_) => TypeNode::基础类型(BasicType::整数),
            LiteralValue::浮点数(_) => TypeNode::基础类型(BasicType::浮点数),
            LiteralValue::字符串(_) => TypeNode::基础类型(BasicType::字符串),
            LiteralValue::布尔(_) => TypeNode::基础类型(BasicType::布尔),
            LiteralValue::字符(_) => TypeNode::基础类型(BasicType::字符),
        }),
        AstNode::格式字符串表达式(_) | AstNode::字符串连接表达式(_) => {
            Some(TypeNode::基础类型(BasicType::字符串))
        }
        _ => None,
    }
}

/// 对外入口：一次分析整个编译单元（entry + 所有被导入模块），返回结构化错误列表。
/// 「硬错」：**证明是错**的那一类，跟 QI_TYPECHECK 档位无关，一律拦下。
///
/// 目前只有一条：已知结构体喂给标量形参（取查询(上下文值, "k") 这种）。
/// 它零误报 —— 只认本单元声明过的结构体名，泛型参数和没见过的名字都放过 ——
/// 所以没有理由只给个警告让人一路带到线上。
///
/// 别的检查仍然是度量性质（默认不跑，QI_TYPECHECK=1 打印、strict 才致命）：
/// 仓库里还有一批历史写法过不了，现在就全上会让所有项目当场编不过。
pub fn 是硬错(e: &TypeError) -> bool {
    matches!(e, TypeError::TypeMismatch { actual, .. } if actual.starts_with("实参却是结构体"))
}

/// 只跑硬错那一档。编译路径每次都调它 —— 比全量检查便宜，且零误报。
pub fn 硬错检查(programs: &[Program]) -> Vec<TypeError> {
    分析编译单元(programs).into_iter().filter(是硬错).collect()
}

pub fn 分析编译单元(programs: &[Program]) -> Vec<TypeError> {
    分析编译单元_分组(programs).into_iter().flatten().collect()
}

/// 同 分析编译单元，但错误按 Program 分组返回（返回值[i] 是 programs[i] 的错误）。
/// `qi check` 渲染「文件:行:列」时需要知道错误属于哪份源码——span 是相对
/// 各自文件的字节偏移，拿 entry 源码换算被导入模块的错误会得到错行列。
pub fn 分析编译单元_分组(programs: &[Program]) -> Vec<Vec<TypeError>> {
    let mut c = 单元检查器::新建();
    for p in programs {
        c.收集声明(p);
    }
    let mut 组 = Vec::with_capacity(programs.len());
    for p in programs {
        c.检查程序(p);
        组.push(std::mem::take(&mut c.错误));
    }
    组
}

impl 单元检查器 {
    fn 新建() -> Self {
        Self {
            函数表: HashMap::new(),
            结构体表: HashMap::new(),
            枚举表: HashMap::new(),
            变体归属: HashMap::new(),
            变体元数: HashMap::new(),
            变体载荷: HashMap::new(),
            枚举类型参数: HashMap::new(),
            歧义变体: HashSet::new(),
            特性集: HashSet::new(),
            联合体集: HashSet::new(),
            类型方法: HashMap::new(),
            全局变量: HashMap::new(),
            包名集: HashSet::new(),
            模块别名: HashMap::new(),
            不透明别名: HashSet::new(),
            导入符号: HashSet::new(),
            作用域: Vec::new(),
            字面量变量: Vec::new(),
            常量集: Vec::new(),
            当前类型参数: HashSet::new(),
            当前返回: None,
            错误: Vec::new(),
        }
    }

    fn 报(&mut self, e: TypeError) {
        self.错误.push(e);
    }

    // =====================================================================
    // 声明收集 pass
    // =====================================================================

    /// 结构体方法/实现块方法/独立方法声明 → 类型方法重载集。
    fn 收集方法(&mut self, 类型名: &str, m: &MethodDeclaration) {
        self.类型方法
            .entry((类型名.to_string(), m.method_name.clone()))
            .or_default()
            .push(函数签名 {
                参数: m.parameters.clone(),
                返回: m.return_type.clone(),
                类型参数: vec![],
                变参: m.parameters.iter().any(|p| p.is_variadic),
            });
    }

    fn 收集声明(&mut self, p: &Program) {
        if let Some(pkg) = &p.package_name {
            self.包名集.insert(pkg.clone());
        }
        for stmt in &p.statements {
            self.收集声明节点(stmt);
        }
    }

    fn 收集声明节点(&mut self, node: &AstNode) {
        match node {
            AstNode::函数声明(f) => {
                self.函数表
                    .entry(f.name.clone())
                    .or_default()
                    .push(函数签名::从函数声明(f));
            }
            AstNode::导出函数(ef) => self.收集声明节点(&ef.decl),
            // 外部 C FFI：签名可信（变参用 is_variadic 标注）
            AstNode::外部声明(块) => {
                for f in &块.functions {
                    self.函数表
                        .entry(f.name.clone())
                        .or_default()
                        .push(函数签名 {
                            参数: f.parameters.clone(),
                            返回: f.return_type.clone(),
                            类型参数: vec![],
                            变参: f.parameters.iter().any(|p| p.is_variadic),
                        });
                }
            }
            AstNode::结构体声明(s) => {
                self.结构体表.insert(
                    s.name.clone(),
                    结构体信息 {
                        字段: s.fields.clone(),
                        类型参数: s.type_params.clone(),
                        有嵌入: s.fields.iter().any(|f| f.is_embedded),
                    },
                );
                for m in &s.methods {
                    self.收集方法(&s.name, m);
                }
            }
            AstNode::枚举声明(e) => {
                let 变体集: HashSet<String> = e.variants.iter().map(|v| v.name.clone()).collect();
                for v in &e.variants {
                    // 首见优先：跨枚举重名变体不覆盖；重名记入歧义集（元数检查沉默）
                    match self.变体归属.entry(v.name.clone()) {
                        std::collections::hash_map::Entry::Occupied(o) if o.get() != &e.name => {
                            self.歧义变体.insert(v.name.clone());
                        }
                        std::collections::hash_map::Entry::Occupied(_) => {}
                        std::collections::hash_map::Entry::Vacant(槽) => {
                            槽.insert(e.name.clone());
                        }
                    }
                    self.变体元数
                        .insert((e.name.clone(), v.name.clone()), v.payload.len());
                    self.变体载荷
                        .insert((e.name.clone(), v.name.clone()), v.payload.clone());
                }
                self.枚举类型参数
                    .insert(e.name.clone(), e.type_params.clone());
                self.枚举表.insert(e.name.clone(), 变体集);
            }
            AstNode::特性声明(t) => {
                self.特性集.insert(t.name.clone());
            }
            AstNode::联合体声明(u) => {
                self.联合体集.insert(u.name.clone());
            }
            AstNode::实现块(imp) => {
                for m in &imp.methods {
                    self.收集方法(&imp.target_type, m);
                }
            }
            AstNode::方法声明(m) => {
                self.收集方法(&m.receiver_type, m);
            }
            // 顶层全局变量/常量：类型 = 注解，否则字面量直推，否则未知
            AstNode::变量声明(d) => {
                let t: 推断 = d
                    .type_annotation
                    .clone()
                    .or_else(|| d.initializer.as_deref().and_then(字面量直推));
                self.全局变量.insert(d.name.clone(), t);
            }
            _ => {}
        }
    }

    // =====================================================================
    // 检查 pass
    // =====================================================================

    fn 检查程序(&mut self, p: &Program) {
        // 每个文件重建导入环境（导入是文件作用域）
        self.模块别名.clear();
        self.不透明别名.clear();
        self.导入符号.clear();
        for imp in &p.imports {
            let 首段 = match imp.module_path.first() {
                Some(s) => s.as_str(),
                None => continue,
            };
            let 末段 = imp.module_path.last().cloned().unwrap_or_default();
            if 首段 == "标准库" {
                // `导入 标准库.JSON 作为 J` → J 与 JSON 都解析到注册表模块
                let alias = imp.alias.clone().unwrap_or_else(|| 末段.clone());
                self.模块别名.insert(alias, 末段.clone());
                self.模块别名.insert(末段.clone(), 末段);
            } else {
                // 用户包 / qi_packages / 相对导入：别名与全路径都记为不透明模块
                if let Some(items) = &imp.items {
                    for it in items {
                        self.导入符号.insert(it.clone());
                    }
                }
                let alias = imp.alias.clone().unwrap_or_else(|| 末段.clone());
                self.不透明别名.insert(alias);
                self.不透明别名.insert(imp.module_path.join("."));
            }
        }

        self.作用域.clear();
        self.字面量变量.clear();
        self.进作用域();
        for stmt in &p.statements {
            self.走语句(stmt);
        }
        self.出作用域();
    }

    // ---------------------------------------------------------------------
    // 作用域与符号解析
    // ---------------------------------------------------------------------

    fn 进作用域(&mut self) {
        self.作用域.push(HashMap::new());
        self.字面量变量.push(HashSet::new());
        self.常量集.push(HashSet::new());
    }
    fn 出作用域(&mut self) {
        self.作用域.pop();
        self.字面量变量.pop();
        self.常量集.pop();
    }
    fn 绑定(&mut self, name: &str, t: 推断) {
        if let Some(top) = self.作用域.last_mut() {
            top.insert(name.to_string(), t);
        }
        // 新绑定默认非字面量/非常量（同层重入声明清旧标记；标记由调用方随后补）
        if let Some(top) = self.字面量变量.last_mut() {
            top.remove(name);
        }
        if let Some(top) = self.常量集.last_mut() {
            top.remove(name);
        }
    }

    /// 名字是否解析到「常量绑定」（按最近绑定层判定；遮蔽正确性同 是字面量变量）。
    fn 是常量(&self, name: &str) -> bool {
        for (i, s) in self.作用域.iter().enumerate().rev() {
            if s.contains_key(name) {
                return self
                    .常量集
                    .get(i)
                    .map(|m| m.contains(name))
                    .unwrap_or(false);
            }
        }
        false
    }

    /// 在名字的**绑定层**上打/清「字面量来源」标记（遮蔽正确性）。
    /// 顶层（文件作用域，即全局变量）不追踪——跨函数赋值顺序不可知。
    fn 设字面量(&mut self, name: &str, on: bool) {
        for (i, s) in self.作用域.iter().enumerate().rev() {
            if s.contains_key(name) {
                if i == 0 {
                    return; // 顶层全局：不标记（clear 也无标记可清）
                }
                if let Some(m) = self.字面量变量.get_mut(i) {
                    if on {
                        m.insert(name.to_string());
                    } else {
                        m.remove(name);
                    }
                }
                return;
            }
        }
    }

    /// 名字是否解析到「字面量来源变量」（按最近绑定层判定）。
    fn 是字面量变量(&self, name: &str) -> bool {
        for (i, s) in self.作用域.iter().enumerate().rev() {
            if s.contains_key(name) {
                return self
                    .字面量变量
                    .get(i)
                    .map(|m| m.contains(name))
                    .unwrap_or(false);
            }
        }
        false
    }

    /// 值是否是「字面量来源」：字面量本身、由字面量组合出的表达式，或
    /// 解析到「由字面量初始化且未被非字面量覆写」的局部变量。
    /// 家族类型比对**只对字面量来源的值**报错——绿码里大量存在「句柄惯用法」
    /// （字符串描述符存进 整数 变量、再当句柄传参），codegen 以 ptr↔i64 协调
    /// 放行；只有字面量（或其确定载体）直怼错类型才是有把握的真错。
    /// 形参是标量（字符串/数值/容器），实参是**本单元声明过的结构体** —— 必错。
    ///
    /// 只认已声明的结构体名：泛型参数 T、没见过的名字（可能是别的包的类型别名）
    /// 一律放过，宁可漏报也不误报。
    pub(super) fn 结构体喂给标量(&self, 形参: &TypeNode, 实参: &TypeNode) -> bool {
        let 形参族 = 类型家族(形参);
        if 形参族 == 家族::沉默 {
            return false;
        }
        match 实参 {
            TypeNode::自定义类型(名) => {
                是已知结构体名(名, &self.结构体表, &self.当前类型参数)
            }
            _ => false,
        }
    }

    pub(super) fn 是字面量来源(&self, node: &AstNode) -> bool {
        match node {
            AstNode::字面量表达式(_)
            | AstNode::格式字符串表达式(_)
            | AstNode::数组字面量表达式(_) => true,
            AstNode::字符串连接表达式(sc) => {
                self.是字面量来源(&sc.left) || self.是字面量来源(&sc.right)
            }
            AstNode::一元操作表达式(u) => self.是字面量来源(&u.operand),
            AstNode::二元操作表达式(b) => {
                self.是字面量来源(&b.left) && self.是字面量来源(&b.right)
            }
            AstNode::标识符表达式(id) => self.是字面量变量(&id.name),
            _ => false,
        }
    }

    fn 查局部(&self, name: &str) -> Option<&推断> {
        self.作用域.iter().rev().find_map(|s| s.get(name))
    }

    /// 名字是否可解析为「某种已知实体」（不含局部变量）——用于沉默判定。
    fn 是已知非变量实体(&self, name: &str) -> bool {
        self.函数表.contains_key(name)
            || self.结构体表.contains_key(name)
            || self.枚举表.contains_key(name)
            || self.变体归属.contains_key(name)
            || self.特性集.contains(name)
            || self.联合体集.contains(name)
            || self.模块别名.contains_key(name)
            || self.不透明别名.contains(name)
            || self.导入符号.contains(name)
            || self.包名集.contains(name)
            || self.当前类型参数.contains(name)
            || 注册表().has_module(name)
            || 注册表().has_module(&format!("标准库.{}", name))
    }

    /// 表达式位置的标识符解析。`要报错=false` 用于接收者等可能是模块名的位置。
    fn 解析标识符(&mut self, id: &IdentifierExpression, 要报错: bool) -> 推断 {
        if let Some(t) = self.查局部(&id.name) {
            return t.clone();
        }
        if let Some(t) = self.全局变量.get(&id.name) {
            return t.clone();
        }
        // 裸枚举变体当值用 → 枚举类型
        if let Some(枚举名) = self.变体归属.get(&id.name) {
            return Some(TypeNode::自定义类型(枚举名.clone()));
        }
        if self.是已知非变量实体(&id.name) {
            return None; // 函数当值/类型名/模块名/导入符号 → 未知但合法
        }
        // 内建常量/类名兜底（未来::就绪 等静态接收者、裸 无 构造子、空值字面量等）
        if matches!(
            id.name.as_str(),
            "未来" | "通道" | "选项" | "结果" | "空值" | "无" | "_"
        ) {
            return None;
        }
        if 要报错 {
            self.报(TypeError::UndefinedVariable {
                name: id.name.clone(),
                span: id.span,
            });
        }
        None
    }

    /// 类型注解 → 推断：涉及当前泛型类型参数的注解视为未知（体内 T 是类型变量）。
    fn 解析类型(&self, t: &TypeNode) -> 推断 {
        if self.含类型参数(t) {
            None
        } else {
            Some(t.clone())
        }
    }

    fn 含类型参数(&self, t: &TypeNode) -> bool {
        if self.当前类型参数.is_empty() {
            return false;
        }
        match t {
            TypeNode::自定义类型(n) => self.当前类型参数.contains(n),
            TypeNode::数组类型(a) => self.含类型参数(&a.element_type),
            TypeNode::列表类型(l) => self.含类型参数(&l.element_type),
            TypeNode::集合类型(s) => self.含类型参数(&s.element_type),
            TypeNode::字典类型(d) => {
                self.含类型参数(&d.key_type) || self.含类型参数(&d.value_type)
            }
            TypeNode::通道类型(c) => self.含类型参数(&c.element_type),
            TypeNode::指针类型(p) => self.含类型参数(&p.target_type),
            TypeNode::引用类型(r) => self.含类型参数(&r.target_type),
            TypeNode::未来类型(inner) => self.含类型参数(inner),
            TypeNode::选项类型(o) => self.含类型参数(&o.inner_type),
            TypeNode::结果类型(r) => {
                self.含类型参数(&r.ok_type) || self.含类型参数(&r.err_type)
            }
            TypeNode::泛型类型(g) => {
                self.当前类型参数.contains(&g.base_type)
                    || g.type_arguments.iter().any(|a| self.含类型参数(a))
            }
            TypeNode::函数类型(f) => {
                f.parameters.iter().any(|p| self.含类型参数(p))
                    || self.含类型参数(&f.return_type)
            }
            _ => false,
        }
    }

    // ---------------------------------------------------------------------
    // 语句
    // ---------------------------------------------------------------------

    fn 走语句(&mut self, node: &AstNode) {
        match node {
            AstNode::变量声明(d) => self.检查变量声明(d),
            AstNode::函数声明(f) => {
                // 局部（嵌套）函数：名字绑进当前作用域（体内可调）。
                // 顶层函数已进全局函数表 —— 不能再绑成局部变量，否则调用点
                // 会被当成「函数值间接调用」而跳过元数/类型检查。
                if self.作用域.len() > 1 && !self.函数表.contains_key(&f.name) {
                    self.绑定(&f.name, None);
                }
                self.检查函数体(&f.type_params, &f.parameters, &f.return_type, &f.body, None);
                // 异步函数必须返回 未来<T>（镜像 codegen 语义规则）
                if f.is_async && !matches!(f.return_type, Some(TypeNode::未来类型(_))) {
                    self.报(TypeError::General {
                        message: format!("函数 `{}`：异步函数必须返回 未来<T>", f.name),
                        span: f.span,
                    });
                }
            }
            AstNode::导出函数(ef) => self.走语句(&ef.decl),
            AstNode::结构体声明(s) => {
                for m in &s.methods {
                    self.检查方法体(m);
                }
            }
            AstNode::实现块(imp) => {
                for m in &imp.methods {
                    self.检查方法体(m);
                }
            }
            AstNode::方法声明(m) => self.检查方法体(m),
            AstNode::特性声明(t) => {
                // 特性默认实现体：接收者环境不完整，宽容起见只走不报（静默检查）
                for m in &t.methods {
                    if let Some(body) = &m.default_body {
                        let 起 = self.错误.len();
                        self.进作用域();
                        self.绑定("自己", None);
                        self.绑定("自身", None);
                        for p in &m.parameters {
                            let t = p.type_annotation.as_ref().and_then(|a| self.解析类型(a));
                            self.绑定(&p.name, t);
                        }
                        for stmt in body {
                            self.走语句(stmt);
                        }
                        self.出作用域();
                        self.错误.truncate(起); // 默认体报错全部丢弃（沉默）
                    }
                }
            }
            AstNode::枚举声明(_)
            | AstNode::联合体声明(_)
            | AstNode::外部声明(_)
            | AstNode::跳出语句(_)
            | AstNode::继续语句(_) => {}
            AstNode::如果语句(s) => {
                self.推断表达式(&s.condition);
                self.进作用域();
                for st in &s.then_branch {
                    self.走语句(st);
                }
                self.出作用域();
                if let Some(e) = &s.else_branch {
                    self.进作用域();
                    self.走语句(e);
                    self.出作用域();
                }
            }
            AstNode::当语句(s) => {
                self.推断表达式(&s.condition);
                self.进作用域();
                for st in &s.body {
                    self.走语句(st);
                }
                self.出作用域();
            }
            AstNode::循环语句(s) => {
                self.进作用域();
                for st in &s.body {
                    self.走语句(st);
                }
                self.出作用域();
            }
            AstNode::对于语句(s) => self.检查对于(s),
            AstNode::返回语句(s) => self.检查返回(s),
            AstNode::块语句(s) => {
                self.进作用域();
                for st in &s.statements {
                    self.走语句(st);
                }
                self.出作用域();
            }
            AstNode::表达式语句(s) => {
                self.推断表达式(&s.expression);
            }
            AstNode::尝试语句(s) => {
                self.进作用域();
                for st in &s.try_body {
                    self.走语句(st);
                }
                self.出作用域();
                for c in &s.catch_clauses {
                    self.进作用域();
                    if let Some(v) = &c.error_var {
                        self.绑定(v, None); // 错误值类型不定 → 未知
                    }
                    for st in &c.body {
                        self.走语句(st);
                    }
                    self.出作用域();
                }
                if let Some(fin) = &s.finally_body {
                    self.进作用域();
                    for st in fin {
                        self.走语句(st);
                    }
                    self.出作用域();
                }
            }
            AstNode::抛出语句(s) => {
                self.推断表达式(&s.expression);
            }
            // 其余（表达式直接当语句等）→ 走表达式推断
            other => {
                self.推断表达式(other);
            }
        }
    }

    fn 检查变量声明(&mut self, d: &VariableDeclaration) {
        let 初值型: 推断 = d.initializer.as_deref().and_then(|e| self.推断表达式(e));
        let 声明型: 推断 = d.type_annotation.as_ref().and_then(|t| self.解析类型(t));
        let 可报 = d
            .initializer
            .as_deref()
            .map(|e| self.是字面量来源(e))
            .unwrap_or(false);
        if let (Some(dt), Some(it), true) = (&声明型, &初值型, 可报) {
            if !家族相容(dt, it) {
                self.报(TypeError::TypeMismatch {
                    expected: format!("变量 `{}` 声明为 {}", d.name, 类型显示(dt)),
                    actual: format!("初值却是 {}", 类型显示(it)),
                    span: d.span,
                });
            }
        }
        self.绑定(&d.name, 声明型.or(初值型));
        if 可报 {
            self.设字面量(&d.name, true); // 字面量初始化 → 变量成为字面量来源载体
        }
        if !d.is_mutable {
            if let Some(top) = self.常量集.last_mut() {
                top.insert(d.name.clone());
            }
        }
    }

    fn 检查对于(&mut self, s: &ForStatement) {
        let 元素型: 推断 = match s.range.as_ref() {
            // 整数区间：起..止 / 起 到 止 / 起 直到 止
            AstNode::区间表达式(r) => {
                self.推断表达式(&r.start);
                self.推断表达式(&r.end);
                Some(TypeNode::基础类型(BasicType::整数))
            }
            other => match self.推断表达式(other) {
                Some(TypeNode::数组类型(a)) => self.解析类型(&a.element_type),
                Some(TypeNode::列表类型(l)) => self.解析类型(&l.element_type),
                // 字典/字符串/流式/未知 → 循环变量未知（键值对、字符、流元素…）
                _ => None,
            },
        };
        self.进作用域();
        self.绑定(&s.variable, 元素型);
        for st in &s.body {
            self.走语句(st);
        }
        self.出作用域();
    }

    fn 检查返回(&mut self, s: &ReturnStatement) {
        let 值型 = s.value.as_deref().and_then(|v| self.推断表达式(v));
        // 只对字面量来源的返回值做家族比对（句柄惯用法沉默）
        if !s
            .value
            .as_deref()
            .map(|v| self.是字面量来源(v))
            .unwrap_or(false)
        {
            return;
        }
        if let (Some(声明), Some(实际)) = (self.当前返回.clone(), 值型) {
            // 异步：返回 未来<T> 的函数体内可 `返回 T 的值`（自动装箱）
            let 目标 = match &声明 {
                TypeNode::未来类型(inner) => inner.as_ref(),
                t => t,
            };
            // 实际是 未来<…>（转发另一个异步调用）→ 沉默
            if matches!(实际, TypeNode::未来类型(_)) {
                return;
            }
            if !家族相容(目标, &实际) {
                self.报(TypeError::TypeMismatch {
                    expected: format!("函数返回类型声明为 {}", 类型显示(目标)),
                    actual: format!("返回值却是 {}", 类型显示(&实际)),
                    span: s.span,
                });
            }
        }
    }

    /// 函数体检查（含泛型环境与返回类型环境的建立/恢复）。
    fn 检查函数体(
        &mut self,
        type_params: &[String],
        params: &[Parameter],
        return_type: &Option<TypeNode>,
        body: &[AstNode],
        receiver: Option<(&str, 推断)>,
    ) {
        let 旧类型参数 = std::mem::replace(
            &mut self.当前类型参数,
            type_params.iter().cloned().collect(),
        );
        let 旧返回 = self.当前返回.take();
        self.当前返回 = return_type.as_ref().and_then(|t| self.解析类型(t));

        self.进作用域();
        if let Some((名, 型)) = receiver {
            self.绑定(名, 型);
        }
        for p in params {
            let t = p.type_annotation.as_ref().and_then(|a| self.解析类型(a));
            self.绑定(&p.name, t);
        }
        for st in body {
            self.走语句(st);
        }
        self.出作用域();

        self.当前类型参数 = 旧类型参数;
        self.当前返回 = 旧返回;
    }

    fn 检查方法体(&mut self, m: &MethodDeclaration) {
        // 接收者类型已知（本单元结构体/枚举）→ 绑定具体类型，否则未知
        let 接收者型: 推断 = if self.结构体表.contains_key(&m.receiver_type)
            || self.枚举表.contains_key(&m.receiver_type)
        {
            Some(TypeNode::自定义类型(m.receiver_type.clone()))
        } else {
            None
        };
        self.检查函数体(
            &[],
            &m.parameters,
            &m.return_type,
            &m.body,
            Some((m.receiver_name.as_str(), 接收者型)),
        );
    }
}
