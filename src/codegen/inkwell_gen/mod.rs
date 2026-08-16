//! inkwell 后端 v2 —— 类型化 LLVM IR 生成。
//!
//! 目标：用 inkwell 的强类型 builder 取代 13k 行文本 IR 拼接，从结构上消除
//! i64/ptr 猜错、方法链式失败、重名冲突、缺 span 这一整类问题。
//! 复用现有前端（Parser::parse_source），语法完全不变。
//!
//! 阶段7：核心语言 + 真类型检查器。已支持：
//!   变量声明（整数/浮点/布尔/字符串）、整数/浮点算术与比较、字符串拼接、
//!   如果/否则、当（while）、用户函数定义与调用、返回、打印族按类型重载。
//!
//! 阶段8：结构体 + `(自身)` 方法 + 链式调用。
//! 阶段9：多文件导入（合并成单模块编译）、标准库分发（复用 ModuleRegistry）、
//!        函数值（顶层函数名当值传递 + 间接调用）。
//!
//! 模块拆分（每文件 <1000 行）：
//!   类型.rs      —— Qi 类型 ↔ LLVM 类型映射
//!   类型检查.rs  —— 作用域符号表 + 表达式类型推断
//!   表达式.rs    —— 表达式降级
//!   语句.rs      —— 语句 / 控制流降级
//!   声明.rs      —— 函数声明降级
//!   结构体.rs    —— 结构体声明/字面量/字段
//!   方法.rs      —— (自身) 方法 + 方法调用（链式）
//!   导入.rs      —— 标准库分发（模块.方法 → qi_runtime_*）+ 导入别名
//!   闭包.rs      —— 函数值 / 函数指针 / 间接调用

#[path = "全局.rs"]
mod 全局;
#[path = "剖析.rs"]
mod 剖析;
#[path = "匹配.rs"]
mod 匹配;
#[path = "协程.rs"]
mod 协程;
#[path = "反射.rs"]
mod 反射;
#[path = "声明.rs"]
mod 声明;
#[path = "外部.rs"]
mod 外部;
#[path = "导入.rs"]
mod 导入;
#[path = "导出.rs"]
mod 导出;
#[path = "并发.rs"]
mod 并发;
#[path = "异常.rs"]
mod 异常;
#[path = "异步.rs"]
mod 异步;
#[path = "所有权.rs"]
mod 所有权;
#[path = "数组.rs"]
mod 数组;
#[path = "方法.rs"]
mod 方法;
#[path = "枚举.rs"]
mod 枚举;
#[path = "泛型.rs"]
mod 泛型;
#[path = "特性.rs"]
mod 特性;
#[path = "环检测.rs"]
mod 环检测;
#[path = "类型.rs"]
mod 类型;
#[path = "类型检查.rs"]
mod 类型检查;
#[path = "结构体.rs"]
mod 结构体;
#[path = "表达式.rs"]
mod 表达式;
#[path = "诊断.rs"]
mod 诊断;
#[path = "语句.rs"]
mod 语句;
#[path = "调试信息/mod.rs"]
mod 调试信息;
#[path = "闭包.rs"]
mod 闭包;
#[path = "闭包环.rs"]
mod 闭包环;

pub use 诊断::{静态分析, 静态诊断};

use crate::codegen::module_registry::ModuleRegistry;
use crate::config::CompilationTarget;
use crate::parser::ast::{AstNode, Program};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::types::StructType;
use inkwell::values::{GlobalValue, PointerValue};
use inkwell::AddressSpace;
use inkwell::OptimizationLevel as LlvmOpt;
use std::collections::HashMap;
use std::path::Path;
use 类型::Qi类型;
use 类型检查::符号表;

/// 中文函数名 → 合法 LLVM 符号。规则与 lib.rs / 旧后端一致：
/// ASCII 原样；非 ASCII 逐字节转十六进制并加 `_Z_` 前缀。
pub(crate) fn mangle_function_name(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii()) {
        return name.to_string();
    }
    let hex: String = name
        .as_bytes()
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect();
    format!("_Z_{}", hex)
}

/// 包内用户函数的唯一 LLVM 符号：把 `包名$函数名#元数` 一起 mangle。
/// - 包名：避免跨包同名冲突（主程序.注册 vs Harness.注册）；
/// - 元数（形参个数）：支持同名不同元数的**重载**得到互异符号（同元数在登记处已报错）。
/// `入口` 不修饰（→ main，单独处理）。无包名时退回 `名字#元数`。
/// 路径取文件名（不含扩展名）。符号里带全路径会让同一份代码在不同机器上
/// 产出不同的 LLVM 符号 —— 04_模块限定分发确定性 那条回归就是防这个的。
fn 文件短名(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn 包内符号名(pkg: Option<&str>, name: &str, 元数: usize) -> String {
    match pkg {
        Some(p) => mangle_function_name(&format!("{}${}#{}", p, name, 元数)),
        None => mangle_function_name(&format!("{}#{}", name, 元数)),
    }
}

struct 后端<'ctx> {
    ctx: &'ctx Context,
    /// DWARF 调试信息状态（None = 本次编译不生成调试信息）。
    ///
    /// **必须声明在 module 之前**：结构体字段按声明顺序析构，而
    /// DebugInfoBuilder 的 Drop 会 finalize（往 module 里回写延迟构造的元数据）。
    /// module 先析构掉的话那是对已释放内存的写入。
    调试: Option<调试信息::调试上下文<'ctx>>,
    /// 本次编译是否生成调试信息（`--无调试信息` 关）。默认开：qi 没有
    /// debug/release 之分，调试信息不进代码段、不影响运行性能，只增大产物体积。
    调试开: bool,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    /// 目标机的数据布局。调试信息算结构体字段偏移/类型位宽要它；
    /// 在 compile_to_object_multi 的最开头填好（None 只出现在单测里）。
    目标数据: Option<inkwell::targets::TargetData>,
    /// 符号表：函数签名 + 作用域变量类型。
    符号: 符号表,
    /// 当前函数内的局部变量：名字 → (alloca 指针, Qi 类型)。
    变量表: HashMap<String, (PointerValue<'ctx>, Qi类型)>,
    /// 当前正在生成的函数的返回类型（用于返回值隐式提升）。
    当前返回类型: Qi类型,
    /// 结构体索引 → LLVM 具名 struct 类型（与 符号.结构体 一一对应）。
    结构体llvm: Vec<StructType<'ctx>>,
    /// 标准库模块注册表（中文模块.方法 → FFI 名 + 签名）。
    注册表: ModuleRegistry,
    /// 导入别名 → 标准库模块名。如 `导入 标准库.输入输出 作为 IO` → IO→输入输出。
    导入别名: HashMap<String, String>,
    /// 用户包别名 → 包名。如 `导入 Web.持久会话 作为 会话存储` → 会话存储→"Web.持久会话"。
    /// `别名.函数()` 靠它定位到正确的包，不受同名模块干扰。
    包别名: HashMap<String, String>,
    /// 模块顶层全局变量 / 常量：名字 → (LLVM global, Qi 类型)。跨文件合并编译，同名幂等。
    全局变量表: HashMap<String, (GlobalValue<'ctx>, Qi类型)>,
    /// 已在 main 序言初始化过的全局名（防重复 store）。
    已初始化全局: std::collections::HashSet<String>,
    /// 待合成的闭包函数（表达式位置先建 fat obj，函数体在所有普通函数后统一生成，
    /// 避免污染当前 builder 插入点）。
    待合成闭包: Vec<闭包::待合成闭包>,
    /// 闭包合成计数器（生成唯一符号 __closure_N）。
    闭包计数: u32,
    /// 待合成的泛型函数实例（调用点登记签名+原型，体末尾统一生成，同闭包模式）。
    待合成泛型实例: Vec<泛型::待合成泛型实例>,
    /// 待合成的 `尝试询问::<T>` 包装函数（调用点声明原型 + 改写成普通调用，体统一生成）。
    /// (函数声明 AST, 声明包)。已合成集合防重复（键 = 合成函数名）。
    待合成询问: Vec<(crate::parser::ast::FunctionDeclaration, Option<String>)>,
    已合成询问: std::collections::HashSet<String>,
    /// 已实例化的泛型函数实例名（幂等去重）。
    已实例化泛型函数: std::collections::HashSet<String>,
    /// 是否正在生成 入口→main（main 返回 i32，bare `返回` 要 emit ret i32 0）。
    在入口中: bool,
    /// 当前正在处理的模块包名（跨包同名函数消歧用；与 符号.当前包 同步）。
    当前包: Option<String>,
    /// 正在处理哪个文件（登记签名 / 生成函数体 / 生成调用点 三趟都要）。
    /// 私有函数按文件消歧要它 —— 见 包内符号名 和 声明.rs 的登记逻辑。
    当前文件: Option<String>,
    /// (包, 名, 元数) → 已经被某个文件占了的私有函数。第二个文件再定义同名
    /// 同元数的私有函数时，据此判定「该按文件分符号」而不是报错。
    私有占位: std::collections::HashMap<(String, String, usize), String>,
    /// 调用点写了限定名（界面.家庭布局 / 别名.函数）时，指明去哪个包解析。
    ///
    /// 一次性：生成函数调用 一进来就 take() 掉。**不能**改 当前包 来代替 ——
    /// 实参也是在那次调用里生成的，包上下文会顺着漏进去，把实参里的裸调用
    /// 解析到错的包（踩过：查.选取(…, 查询参数(上下文,"q")) 里的 查询参数
    /// 被解析成 查询 包的三参版本）。
    限定包: Option<String>,
    /// 字符串字面量 → 带 immortal header 的全局常量 data 指针（按内容去重，模块级缓存）。
    字符串字面量缓存: HashMap<String, PointerValue<'ctx>>,
    /// QI_ARC=1 时为 true：插入保守字符串 ARC（retain/release）。默认关，
    /// 关时生成的 IR 与无此功能时完全一致。见 所有权.rs。
    弧: bool,
    /// 当前 `尝试` 嵌套深度（Round E4）：进 try 体 +1，出体/进 catch -1。
    /// `返回` 语句在 ret 前按当前深度补 qi_exc_pop × N，防 longjmp 到已失效帧。
    /// 每个函数体/方法体/闭包体/入口独立清零（帧栈按函数平衡）。
    try深度: u32,
    /// `询问<T>` 脱糖时给临时变量起唯一名字的计数器（防同函数内多次调用/循环里名字打架）。
    询问计数: u32,
    /// 循环栈：(继续目标块, 跳出目标块)。当/对于 进循环体前压栈、出体后弹栈；
    /// `跳出;`/`继续;` 向栈顶目标生成 br。函数体/方法体/闭包体各自独立生成且
    /// 压弹严格配对，跨函数不会串（闭包体在所有函数之后统一合成，彼时栈已空）。
    循环栈: Vec<(
        inkwell::basic_block::BasicBlock<'ctx>,
        inkwell::basic_block::BasicBlock<'ctx>,
    )>,
    /// QI_PROF=1 时为 true：在每个用户函数入口/出口注入 qi_prof_enter/exit 计时。
    /// 默认关，关时不声明任何 qi_prof_ 原型、不注入任何调用，IR 逐字节不变。见 剖析.rs。
    剖析: bool,
    /// 当前正在生成的函数的显示名（immortal 全局常量指针），供各出口 exit 复用。
    剖析名: Option<PointerValue<'ctx>>,
    /// 当前函数 entry 块里存进入时刻（i64 纳秒）的 alloca 槽。
    剖析槽: Option<PointerValue<'ctx>>,
    /// 反向 FFI：本次编译登记的所有 `导出 函数`（C ABI 包装 + 头文件生成用）。
    导出表: Vec<导出::导出记录>,
    /// 当前闭包体内的「弱捕获局部名」集合（`闭包 [弱 x]`）。这些局部走 unowned
    /// 语义：合成时不 retain、出口 弧释放局部 跳过。合成每个闭包体前后 take/restore。
    弱局部: std::collections::HashSet<String>,
    /// 块作用域遮蔽栈：每进一个块（生成块 / 对于 循环变量）压一帧；帧内记录
    /// 本块 变量声明 覆盖掉的 变量表 旧项（None = 此前无同名变量）。退块时逆序
    /// 恢复 —— 内层同名变量不再泄漏到外层（修「嵌套块遮蔽覆盖外层值」bug）。
    作用域遮蔽栈: Vec<Vec<(String, Option<(PointerValue<'ctx>, Qi类型)>)>>,
    /// 被作用域恢复移出 变量表 的内层 RC 槽（名字, 槽, 类型）。这些槽仍是
    /// entry 块 alloca + null 初始化，函数出口（弧释放局部）/ 协程 cleanup
    /// 必须继续释放它们 —— 否则内层遮蔽的字符串/结构体/数组泄漏。
    弧隐藏引用计数槽: Vec<(String, PointerValue<'ctx>, Qi类型)>,
    /// 默认 true：把「返回 未来<T> 且含 等待」的用户函数编译成 LLVM
    /// coroutine（llvm.coro.* stackless 状态机）。QI_CORO=0 退回 eager future（逐字节不变）。
    /// 见 协程.rs。
    协程: bool,
    /// 协程函数的 mangled 符号名集合（QI_CORO 下调用点据此把返回的 handle 包成
    /// coro future）。声明阶段扫描全部函数体填充；QI_CORO 关时恒空。
    协程函数集: std::collections::HashSet<String>,
    /// 当前正在生成的 coroutine 上下文（frame handle / promise / 公共块）。
    /// 仅在协程体生成期间为 Some：`等待 让出/睡眠` 与 `返回` 据此发挂起点 / 存返回值。
    协程当前: Option<协程::协程上下文<'ctx>>,
}

impl<'ctx> 后端<'ctx> {
    fn new(ctx: &'ctx Context, 调试开: bool) -> Self {
        let module = ctx.create_module("qi_program");
        let builder = ctx.create_builder();
        Self {
            ctx,
            调试: None,
            调试开,
            module,
            builder,
            目标数据: None,
            符号: 符号表::new(),
            变量表: HashMap::new(),
            当前返回类型: Qi类型::空,
            结构体llvm: Vec::new(),
            注册表: ModuleRegistry::new(),
            导入别名: HashMap::new(),
            包别名: HashMap::new(),
            全局变量表: HashMap::new(),
            已初始化全局: std::collections::HashSet::new(),
            待合成闭包: Vec::new(),
            闭包计数: 0,
            待合成泛型实例: Vec::new(),
            待合成询问: Vec::new(),
            已合成询问: std::collections::HashSet::new(),
            已实例化泛型函数: std::collections::HashSet::new(),
            在入口中: false,
            当前包: None,
            当前文件: None,
            私有占位: std::collections::HashMap::new(),
            限定包: None,
            字符串字面量缓存: HashMap::new(),
            // ARC 默认开(字符串+结构体+数组+闭包 RC 回收);QI_ARC=0 退回纯泄漏模式(调试用)
            弧: std::env::var("QI_ARC")
                .map(|v| !(v == "0" || v.eq_ignore_ascii_case("false")))
                .unwrap_or(true),
            try深度: 0,
            询问计数: 0,
            循环栈: Vec::new(),
            // 剖析默认关（QI_PROF 未设或为 0/false）：零注入、IR 与无此功能一致。
            剖析: std::env::var("QI_PROF")
                .map(|v| !(v == "0" || v.eq_ignore_ascii_case("false")))
                .unwrap_or(false),
            剖析名: None,
            剖析槽: None,
            导出表: Vec::new(),
            弱局部: std::collections::HashSet::new(),
            作用域遮蔽栈: Vec::new(),
            弧隐藏引用计数槽: Vec::new(),
            // 协程默认开(R1-R6 绿:llvm.coro 状态机/多核 M:N/直接交接/park-wake/ARC 跨挂起/
            // 死锁检测)。门控是函数级:仅「返回 未来<T> 且含 等待」的函数变协程状态机,普通程序
            // 零影响。QI_CORO=0 退回 eager future(调试用)。
            协程: std::env::var("QI_CORO")
                .map(|v| !(v == "0" || v.eq_ignore_ascii_case("false")))
                .unwrap_or(true),
            协程函数集: std::collections::HashSet::new(),
            协程当前: None,
        }
    }

    /// QI_CORO 门控：是否启用真协程变换。
    pub(super) fn 协程开(&self) -> bool {
        self.协程
    }

    /// 设置当前包（同步到符号表，供签名解析）。
    pub(super) fn 设当前包(&mut self, pkg: Option<String>) {
        self.符号.当前包 = pkg.clone();
        self.当前包 = pkg;
    }

    /// 设置当前文件。私有函数按文件消歧要它。
    fn 设当前文件(&mut self, 文件: Option<String>) {
        self.当前文件 = 文件;
    }

    /// **每一趟 per-program 遍历都用这个**，不要单独调 设当前包。
    ///
    /// 包和文件必须一起设：有十几趟遍历，逐个记得设两样必然漏一趟，
    /// 而漏了不报错 —— 只是那一趟里私有符号名算错，症状是「函数找不到」
    /// 或者更糟：链到了另一个文件的同名函数。
    pub(super) fn 设当前程序(&mut self, p: &crate::parser::ast::Program) {
        self.设当前包(p.package_name.clone());
        self.设当前文件(p.source_path.clone());
    }

    /// 文件作用域的私有符号名。
    ///
    /// 一个包铺在十几个文件里时，两个文件各写一个 `函数 一参(x)` 是完全正常的 ——
    /// 它们是各自的局部助手，谁也没打算给别人用。但 包内符号名 只含
    /// 包+名+元数，两个文件就撞在同一个 LLVM 符号上，编译报
    /// 「无法按元数区分」，而错误信息还不说另一个在哪个文件。
    ///
    /// 这里给**第二个及以后**占用同一 (包,名,元数) 的私有函数加文件前缀。
    /// 第一个保持原样 —— 这样同包别的文件对它的调用（Qi 允许私有跨文件调用）
    /// 一个字都不用改，纯增量。
    pub(super) fn 私有符号名(&self, 名: &str, 元数: usize) -> String {
        let 文件 = self.当前文件.as_deref().unwrap_or("");
        包内符号名(
            self.当前包.as_deref(),
            &format!("{}@{}", 文件短名(文件), 名),
            元数,
        )
    }

    /// 在**当前函数的 entry 块**建局部 alloca（LLVM 标准做法）。
    ///
    /// 循环体内的 alloca 每次迭代都吃栈（~50 万次迭代后栈溢出段错误），且
    /// mem2reg 只提升 entry 块的 alloca。故所有函数级局部槽统一插到 entry：
    /// 暂存当前插入点 → 定位到 entry（有终结指令则插在它前）→ alloca → 回位。
    /// 初始化 store 留在调用处原位，同名变量循环内重入声明仍每迭代重新赋值，
    /// 语义不变。当前函数从 builder 插入块反查，调用处无需传 FunctionValue。
    pub(super) fn 入口块alloca(
        &mut self,
        llvmt: inkwell::types::BasicTypeEnum<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, String> {
        let 当前 = self
            .builder
            .get_insert_block()
            .ok_or_else(|| "builder 无插入点".to_string())?;
        let entry = 当前
            .get_parent()
            .and_then(|f| f.get_first_basic_block())
            .ok_or_else(|| "函数缺 entry 块".to_string())?;
        if entry == 当前 {
            // 已在 entry（函数序言 / 无控制流的直线代码）：就地 alloca
            return self
                .builder
                .build_alloca(llvmt, name)
                .map_err(|e| e.to_string());
        }
        match entry.get_terminator() {
            Some(term) => self.builder.position_before(&term),
            None => self.builder.position_at_end(entry),
        }
        let p = self
            .builder
            .build_alloca(llvmt, name)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(当前);
        Ok(p)
    }

    /// 声明阶段7 用到的运行时函数原型。
    fn 声明运行时(&self) {
        let i32t = self.ctx.i32_type();
        let i64t = self.ctx.i64_type();
        let f64t = self.ctx.f64_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());

        // 打印族
        let 收指针 = i32t.fn_type(&[ptrt.into()], false);
        self.module.add_function("qi_runtime_println", 收指针, None);
        self.module.add_function("qi_runtime_print", 收指针, None);

        let 收i64 = i32t.fn_type(&[i64t.into()], false);
        self.module
            .add_function("qi_runtime_println_int", 收i64, None);
        self.module
            .add_function("qi_runtime_print_int", 收i64, None);

        let 收f64 = i32t.fn_type(&[f64t.into()], false);
        self.module
            .add_function("qi_runtime_println_float", 收f64, None);
        self.module
            .add_function("qi_runtime_print_float", 收f64, None);

        let 收i32 = i32t.fn_type(&[i32t.into()], false);
        self.module
            .add_function("qi_runtime_println_bool", 收i32, None);
        self.module
            .add_function("qi_runtime_print_bool", 收i32, None);

        // 字符串
        let 拼接 = ptrt.fn_type(&[ptrt.into(), ptrt.into()], false);
        self.module
            .add_function("qi_runtime_string_concat", 拼接, None);
        // 字符串比较（== / != / </ > 等；返回 <0/0/>0）
        let 比较 = i32t.fn_type(&[ptrt.into(), ptrt.into()], false);
        self.module
            .add_function("qi_runtime_string_compare", 比较, None);
        // 释放临时字符串（拼接链中间结果 / int_to_string 临时值）。
        // 仅对 codegen 结构上确定「新建且不逃逸」的临时值发，绝不对字面量/变量发。
        let 释放串 = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        self.module.add_function("qi_string_free", 释放串, None);
        // 增引用（QI_ARC 插桩用；null/immortal/非 RC 指针皆 no-op）
        let 保留串 = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        self.module.add_function("qi_string_retain", 保留串, None);
        // C FFI char* 返回：把裸 C 字符串拷贝进 Qi 拥有的堆串（rc=1，带 magic header）。
        // 原 C 内存不碰 —— getenv 静态串安全，C malloc 串所有权仍归 C（用户按 指针 free）。
        self.module.add_function(
            "qi_string_from_cstr",
            ptrt.fn_type(&[ptrt.into()], false),
            None,
        );

        // 类型转换
        self.module.add_function(
            "qi_runtime_int_to_string",
            ptrt.fn_type(&[i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_runtime_float_to_string",
            ptrt.fn_type(&[f64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_runtime_string_to_int",
            i64t.fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_runtime_string_to_float",
            f64t.fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_runtime_int_to_float",
            f64t.fn_type(&[i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_runtime_float_to_int",
            i64t.fn_type(&[f64t.into()], false),
            None,
        );

        // 内存分配（结构体堆分配用）：qi_runtime_alloc(size: usize) -> *mut u8
        self.module.add_function(
            "qi_runtime_alloc",
            ptrt.fn_type(&[i64t.into()], false),
            None,
        );

        // RC 对象分配器（QI_ARC=1 时结构体/数组本体走这里；ptr-24 藏 header，
        // 零初始化，绕开 memory_manager 的全局 RwLock）。ARC 关时无人调用。
        self.module
            .add_function("qi_obj_alloc", ptrt.fn_type(&[i64t.into()], false), None);
        let 收指针void = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        self.module.add_function("qi_obj_retain", 收指针void, None);
        self.module
            .add_function("qi_obj_dec", i64t.fn_type(&[ptrt.into()], false), None);
        self.module.add_function("qi_obj_free", 收指针void, None);
        // 动态派发释放（数组<指针> 元素等编译期类型未知的保守路径）
        self.module
            .add_function("qi_rc_release_any", 收指针void, None);

        // 通道（阶段10 退化并发）
        self.module.add_function(
            "qi_runtime_create_channel",
            ptrt.fn_type(&[i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_runtime_channel_send",
            i32t.fn_type(&[ptrt.into(), i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_runtime_channel_receive",
            i32t.fn_type(&[ptrt.into(), ptrt.into()], false),
            None,
        );

        // 协程真并发 spawn（qi-runtime async_runtime/ffi）：
        //   qi_runtime_spawn_goroutine(fn())            —— fire-and-forget 裸函数
        //   qi_runtime_spawn_goroutine_with_args(fn(*const i64), *const i64, i64)
        //     —— runtime 拷贝 args 数组后在线程池调 wrapper(args_ptr)
        self.module.add_function(
            "qi_runtime_spawn_goroutine",
            self.ctx.void_type().fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_runtime_spawn_goroutine_with_args",
            self.ctx
                .void_type()
                .fn_type(&[ptrt.into(), ptrt.into(), i64t.into()], false),
            None,
        );

        // 闭包 / 函数值 fat 对象 ABI（closure_ffi.rs）
        self.module.add_function(
            "qi_closure_create",
            ptrt.fn_type(&[ptrt.into(), i64t.into()], false),
            None,
        );
        // 无捕获顶层函数取址为值：不朽单例缓存（免每次引用泄漏一个闭包）
        self.module.add_function(
            "qi_closure_intern",
            ptrt.fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_closure_get_fn",
            ptrt.fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_closure_get_int",
            i64t.fn_type(&[ptrt.into(), i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_closure_get_ptr",
            ptrt.fn_type(&[ptrt.into(), i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_closure_set_int",
            self.ctx
                .void_type()
                .fn_type(&[ptrt.into(), i64t.into(), i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_closure_set_ptr",
            self.ctx
                .void_type()
                .fn_type(&[ptrt.into(), i64t.into(), ptrt.into()], false),
            None,
        );
        // 闭包 RC（Round E1）：retain/release 按 magic 动态派发；set_dtor 把
        // codegen 合成的 env 析构函数挂进 fat obj 末槽（归零时级联释放捕获）
        self.module.add_function(
            "qi_closure_retain",
            self.ctx.void_type().fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_closure_release",
            self.ctx.void_type().fn_type(&[ptrt.into()], false),
            None,
        );
        self.module.add_function(
            "qi_closure_set_dtor",
            self.ctx
                .void_type()
                .fn_type(&[ptrt.into(), ptrt.into()], false),
            None,
        );

        // 剖析器（QI_PROF=1 专用）：仅在开启时声明原型 —— 关时无这三行 declare，
        // 保证未开启时整个模块 IR 与无此功能逐字节一致（真·零开销）。
        if self.剖析 {
            self.module
                .add_function("qi_prof_enter", i64t.fn_type(&[ptrt.into()], false), None);
            self.module.add_function(
                "qi_prof_exit",
                self.ctx
                    .void_type()
                    .fn_type(&[ptrt.into(), i64t.into()], false),
                None,
            );
            self.module.add_function(
                "qi_prof_report",
                self.ctx.void_type().fn_type(&[], false),
                None,
            );
        }

        // future / async（eager future 模型）
        self.声明future运行时();

        // 反射注册运行时原型（qi_reflect_register_*）——供 main 序言登记元数据。
        self.声明反射运行时();
    }

    /// 生成 入口() → LLVM main。
    fn 生成入口(&mut self, programs: &[Program]) -> Result<(), String> {
        let 入口 = programs[0]
            .statements
            .iter()
            .find_map(|s| match s {
                AstNode::函数声明(f) if f.name == "入口" => Some(f.clone()),
                _ => None,
            })
            .ok_or_else(|| "未找到 入口() 函数".to_string())?;

        let i32t = self.ctx.i32_type();
        let main_fn = self
            .module
            .add_function("main", i32t.fn_type(&[], false), None);
        let bb = self.ctx.append_basic_block(main_fn, "entry");
        self.builder.position_at_end(bb);

        self.设当前程序(&programs[0]);
        // 调试信息：main 的 DISubprogram 用 入口 的中文名 + 声明行，backtrace
        // 里显示成「入口」而不是「main」，跟源码对得上。设当前程序 之后调 ——
        // 它要靠 当前文件 定位源码。
        self.调试_进入函数(main_fn, "入口", "main", 入口.span, &[], Qi类型::空);
        self.变量表.clear();
        self.作用域遮蔽栈.clear();
        self.弧隐藏引用计数槽.clear();
        self.符号.进入作用域();
        self.当前返回类型 = Qi类型::空;
        self.try深度 = 0; // E4
        self.在入口中 = true; // main 返回 i32：bare `返回` 要 ret i32 0

        // 剖析：main（入口）序言计时。atexit 报告在进程退出、main 返回之后打印。
        self.剖析入口("入口")?;

        // 反射：把用户函数 / 结构体 / 枚举 元数据登记进运行时注册表
        //（早于任何用户代码，含全局初始化）。供 反射.* 自省。
        self.生成反射注册()?;

        // QI_CORO：有协程函数时，main 序言注册 coro intrinsic 包装（executor 回调用）。
        // 早于任何 启动协程 / 等待 —— resume/done/destroy/promise 就位。关时不触发。
        if self.协程开() && !self.协程函数集.is_empty() {
            self.注册协程ops()?;
        }

        // 全局变量初始化（所有模块的带初值全局，在 body 之前 store）
        self.生成全局初始化(programs)?;

        for stmt in &入口.body {
            self.生成语句(stmt, main_fn)?;
            if self.当前块已终结() {
                break;
            }
        }

        self.在入口中 = false;
        self.符号.退出作用域();

        if !self.当前块已终结() {
            self.剖析出口()?; // 剖析：main 落底出口计时
                              // ARC：main 顺利落底时释放入口的字符串局部 + 全局 RC
            self.弧释放局部()?;
            self.弧释放全局()?;
            self.builder
                .build_return(Some(&i32t.const_int(0, false)))
                .map_err(|e| e.to_string())?;
        }
        // 位置是粘性的：不清会漏进后面合成的闭包体 / 释放函数（它们没有
        // DISubprogram），verifier 报 wrong subprogram。
        self.调试_离开函数();
        Ok(())
    }
}

/// 一个 C 导出函数的头文件信息（供 lib.rs 生成 .h）。C 类型已是最终字面量。
#[derive(Debug, Clone)]
pub struct CExportInfo {
    /// 对外 C 符号名。
    pub c_name: String,
    /// 函数中文原名（头文件注释用）。
    pub qi_name: String,
    /// 参数列表：(C 类型, 参数名)。
    pub params: Vec<(String, String)>,
    /// 返回类型的 C 字面量（如 "int64_t" / "char*" / "void"）。
    pub ret: String,
}

/// Qi 类型 → C 头文件类型字面量。`是返回` 区分字符串的 const 限定（入参只读 / 返回值 C 拥有）。
fn qi类型转c(t: Qi类型, 是返回: bool) -> &'static str {
    match t {
        Qi类型::整数 | Qi类型::布尔 => "int64_t",
        Qi类型::浮点数 => "double",
        Qi类型::字符串 => {
            if 是返回 {
                "char*"
            } else {
                "const char*"
            }
        }
        Qi类型::指针 => "void*",
        Qi类型::空 => "void",
        _ => "int64_t",
    }
}

/// 把单个 Program 编译成目标文件（.o）。保留给单文件调用点。
pub fn compile_to_object(
    program: &Program,
    out: &Path,
    target: CompilationTarget,
) -> Result<(), String> {
    compile_to_object_multi(
        std::slice::from_ref(program),
        out,
        target,
        None,
        crate::config::OptimizationLevel::Basic,
        false,
        true,
    )
    .map(|_| ())
}

/// 把多个 Program（entry + 所有用户模块）合并进同一个 LLVM 模块，编成一个 .o。
/// programs[0] 是 entry（含 入口()）。跨模块符号共享同一 mangle，天然可见。
/// `arch`：交叉编译目标架构（x86_64 / aarch64 / loongarch64），None 时默认 x86_64。
/// `opt`：优化级别（None/Basic/Standard/Maximum → O0/O1/O2/O3 管线 + 同级目标机 opt）。
///
/// `库模式`：true 时不生成 @main（不要求 入口()），改为生成所有 `导出 函数` 的 C ABI
/// 包装 + 库加载构造器；返回导出函数的头文件信息（供 .h 生成）。
///
/// `带调试`：生成 DWARF 行号表 + 函数条目（lldb/gdb 可按 .qi 源码断点/单步）。
/// 默认开，`--无调试信息` 关。
/// 目标三元组 + 目标机。宿主==目标 时走默认三元组（本地路径零风险）；
/// 交叉时按 平台+架构 显式构造（cutover 到 inkwell 时曾丢失，产物错成宿主 Mach-O）。
fn 建目标机(
    target: CompilationTarget,
    arch: Option<&str>,
    opt: crate::config::OptimizationLevel,
) -> Result<(inkwell::targets::TargetTriple, TargetMachine), String> {
    let 宿主即目标 = match target {
        CompilationTarget::Linux => cfg!(target_os = "linux"),
        CompilationTarget::MacOS => cfg!(target_os = "macos"),
        CompilationTarget::Windows => cfg!(target_os = "windows"),
        CompilationTarget::Wasm => false,
    } && arch.is_none_or(|a| a == std::env::consts::ARCH);
    let triple = if 宿主即目标 {
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| format!("初始化目标失败: {}", e))?;
        TargetMachine::get_default_triple()
    } else {
        Target::initialize_all(&InitializationConfig::default());
        let a = arch.unwrap_or("x86_64");
        let t = match target {
            CompilationTarget::Linux => format!("{}-unknown-linux-gnu", a),
            CompilationTarget::Windows => format!("{}-pc-windows-msvc", a),
            CompilationTarget::MacOS => format!("{}-apple-darwin", a),
            CompilationTarget::Wasm => "wasm32-unknown-unknown".to_string(),
        };
        inkwell::targets::TargetTriple::create(&t)
    };
    let tm_opt = match opt {
        crate::config::OptimizationLevel::None => LlvmOpt::None,
        crate::config::OptimizationLevel::Basic => LlvmOpt::Less,
        crate::config::OptimizationLevel::Standard => LlvmOpt::Default,
        crate::config::OptimizationLevel::Maximum => LlvmOpt::Aggressive,
    };
    let t = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let tm = t
        .create_target_machine(
            &triple,
            "generic",
            "",
            tm_opt,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| "无法创建 target machine".to_string())?;
    Ok((triple, tm))
}

pub fn compile_to_object_multi(
    programs: &[Program],
    out: &Path,
    target: CompilationTarget,
    arch: Option<&str>,
    opt: crate::config::OptimizationLevel,
    库模式: bool,
    带调试: bool,
) -> Result<Vec<CExportInfo>, String> {
    if programs.is_empty() {
        return Err("没有可编译的模块".to_string());
    }
    let ctx = Context::create();
    let mut 后端值 = 后端::new(&ctx, 带调试);

    // 目标机/数据布局要在**任何 codegen 之前**定好。原先它排在最后，够用是因为
    // 之前没人在生成期问过「这个结构体的第 k 个字段在第几字节」；复合类型的
    // DWARF 条目要填 DW_AT_data_member_location，非问不可 —— 而字段偏移只有
    // TargetData 知道（`{i1, i64}` 的第二个字段在 8 不在 1）。顺带把
    // set_data_layout 提前也更正确：优化前就按目标布局折叠常量。
    let (triple, tm) = 建目标机(target, arch, opt)?;
    后端值.module.set_triple(&triple);
    后端值
        .module
        .set_data_layout(&tm.get_target_data().get_data_layout());
    后端值.目标数据 = Some(tm.get_target_data());

    // 编译单元要最先建：模块标志（Debug Info Version）得在任何元数据之前落位。
    后端值.调试_建立单元(programs[0].source_path.as_deref());
    后端值.声明运行时();

    // 收集所有模块的导入别名（标准库导入）+ destructure 导入映射（跨包同名结构体消歧）
    for p in programs {
        后端值.收集导入别名(p)?;
        后端值.收集符号导入(p);
    }

    // 第一趟：结构体三步（名字→字段类型→LLVM 类型）。名字必须先全部登记，
    // 跨模块 / 前向引用的字段类型（如 代理.工具表: 注册表）才能解析成 结构体(idx)。
    // 两步都必须带包上下文 —— 结构体按 (包, 名字) 登记，跨包同名各占独立索引。
    for p in programs {
        后端值.设当前程序(p);
        后端值.登记结构体名字(p)?;
    }
    // 枚举名字紧随结构体名字登记：结构体字段 / 枚举载荷 里跨引用彼此都能解析成型。
    for p in programs {
        后端值.设当前程序(p);
        后端值.登记枚举名字(p)?;
    }
    // 特性两趟：先收全部声明（特性可跨模块引用），再登记实现关系 + 完整性校验。
    // 在函数登记之前 —— 泛型约束 `<T: 特性>` / 特性作参数类型 都要查特性注册表。
    for p in programs {
        后端值.设当前程序(p);
        后端值.登记特性(p)?;
    }
    for p in programs {
        后端值.设当前程序(p);
        后端值.登记特性实现(p)?;
    }
    for p in programs {
        后端值.设当前程序(p);
        后端值.解析结构体字段(p)?;
    }
    for p in programs {
        后端值.设当前程序(p);
        后端值.解析枚举变体(p)?;
    }
    // 装箱标志回填后归一化：结构体字段/枚举载荷里残留的裸 枚举(i)（i 装箱）
    // → 装箱枚举(i)。否则该字段是 i64 槽 + 非 RC → 释放函数跳过 → 泄漏。
    后端值.符号.归一化装箱枚举();
    后端值.建结构体llvm类型()?;

    // ARC 循环引用静态检测（类型全部登记后、codegen 主体前跑一趟）。
    // 纯引用计数无环收集器：强引用环 = 永久内存泄漏。默认打警告不阻断；
    // QI_LINT=0 关闭；QI_LINT=strict 升为编译错误（供 CI 卡关）。
    环检测::检测循环引用(&后端值.符号)?;

    // 闭包自引用环（`OBJ.字段 = 闭包 { …OBJ… }`）—— 精确赋值点分析。未用 `弱`
    // 打破的强环是确定性内存泄漏且修复方式唯一，直接**编译错误**焊死（不受
    // QI_LINT 门控）；已写 `闭包 [弱 OBJ] {…}` 的环已断，放行。
    {
        let 强环 = 闭包环::收集闭包环(programs);
        let 强环: Vec<_> = 强环.into_iter().filter(|f| !f.已弱).collect();
        if let Some(f) = 强环.first() {
            let mut 报告 = String::from(
                "[闭包环] 错误: 检测到闭包自引用环（引用环 → 内存泄漏，纯 ARC 无环收集器）:\n",
            );
            for f in &强环 {
                报告.push_str(&format!(
                    "  在 {} 中: {}.{} = 闭包 {{ …引用 {}… }} —— 字段强持有闭包，闭包又强捕获 {}，双方计数永不归零。\n",
                    f.位置, f.对象, f.字段, f.对象, f.对象
                ));
            }
            报告.push_str(&format!(
                "  修复: 用弱捕获打破环 —— 把闭包写成 `闭包 [弱 {}] (…) {{ … }}`。\n         弱捕获不增引用计数（unowned 语义）：持有者释放时闭包随之释放，环断开。",
                f.对象
            ));
            return Err(报告);
        }
    }

    // QI_ARC=1：为所有结构体类型 + 两类数组 emit 释放函数（先声明后定义，
    // 递归类型可用；函数体生成前就位，插桩点直接引用）。关闭时不产生任何 IR。
    后端值.弧生成释放函数()?;
    // 装箱枚举的按-tag 级联释放函数（qi.release.e<idx>）
    后端值.弧生成枚举释放函数()?;

    // 第一趟半：登记所有模块顶层全局变量 / 常量（函数体会引用，须在函数体前）。
    // 带包上下文：全局的结构体类型注解按声明包解析。
    for p in programs {
        后端值.设当前程序(p);
        后端值.登记全局变量(p)?;
    }

    // 第二趟：登记所有模块的函数 / 方法签名 + LLVM 原型（按包消歧）
    for p in programs {
        后端值.设当前程序(p);
        后端值.登记函数(p)?;
        后端值.登记方法(p)?;
    }

    // 函数名遮蔽校验：本地函数不得与 destructure 导入的同名函数冲突（Qi 不按签名解析）。
    // 在 登记函数 之后 —— 依赖 函数按包 判定导入项是否为函数。
    for p in programs {
        后端值.检查导入遮蔽(p)?;
        // 导入的名字必须真实存在（拼错 / 依赖是旧版还没这个 API → 编译期拦下，
        // 而不是编过之后运行时静默失灵）
        后端值.检查导入存在(p)?;
    }

    // QI_CORO：收集协程函数（返回 未来<T> 且含 等待）的 mangled 名 —— 在生成任何
    // 函数体 / 调用点之前就位（调用点据此把返回 handle 包成 coro future）。关时恒空。
    后端值.收集协程函数(programs);

    // 外部 C 函数声明（`外部 "库" { ... }`）：建 C 名原型 + 存签名。
    // 在用户函数登记之后 —— 撞名（外部与用户函数同名）在此报错。
    for p in programs {
        后端值.设当前程序(p);
        后端值.登记外部(p)?;
    }

    // 反向 FFI：登记所有 `导出 函数`（建内部 Qi 原型 + 记录导出元数据）。
    // 在用户函数登记之后 —— 内部原型与普通用户函数共用同一命名空间。
    for p in programs {
        后端值.设当前程序(p);
        后端值.登记导出(p)?;
    }

    // 第三趟：生成所有模块的用户函数体（跳过重复的 入口，只 entry 的算数；
    // 泛型函数模板不直接生成 —— 按调用点单态实例化，体在末尾统一合成）
    for p in programs {
        后端值.设当前程序(p);
        for stmt in &p.statements {
            match stmt {
                AstNode::函数声明(f) => {
                    if f.name == "入口"
                        || !f.type_params.is_empty()
                        // 特性作参数类型 → 登记时已隐式泛型化（AST 上 type_params 仍空）
                        || 后端值.符号.泛型函数模板.contains_key(&f.name)
                    {
                        continue; // 入口只在最后为 entry 生成 main
                    }
                    后端值.生成函数体(f)?;
                }
                // 导出函数的内部 Qi 函数体（包装稍后统一转发到它）
                AstNode::导出函数(ef) => {
                    if let AstNode::函数声明(f) = ef.decl.as_ref() {
                        后端值.生成函数体(f)?;
                    }
                }
                _ => {}
            }
        }
    }

    // 第四趟：生成所有模块的方法体
    for p in programs {
        后端值.设当前程序(p);
        后端值.生成所有方法体(p)?;
    }

    // 入口 → main（entry=programs[0]；全局初始化用所有 programs）。
    // 库模式无 main、不要求 入口()；改由导出包装 + global_ctors 承接。
    if !库模式 {
        后端值.生成入口(programs)?;
    }

    // 合成所有待处理闭包函数（函数体/方法体/入口里遇到的匿名闭包）+
    // 泛型函数实例体。两者互相可能再产生对方（闭包里调泛型函数 / 泛型体里有
    // 闭包），交替循环到双双清空。
    loop {
        后端值.合成待处理闭包()?;
        后端值.合成待处理泛型实例()?;
        后端值.合成待处理询问()?;
        if 后端值.待合成闭包.is_empty()
            && 后端值.待合成泛型实例.is_empty()
            && 后端值.待合成询问.is_empty()
        {
            break;
        }
    }

    // 参数化枚举（选项/结果/用户泛型枚举）是在函数体/签名解析时按需单态实例化的，
    // 晚于第一次枚举释放函数生成。此处再跑一次（幂等）为所有新实例定义
    // qi.release.e<idx>，否则实例的释放函数只有原型、无函数体 → 链接期未定义符号。
    后端值.弧生成枚举释放函数()?;
    // 泛型结构体实例同理：补 qi.release.s<idx> / qi.release.arr.s<idx> 的定义
    //（幂等；内部先 确保结构体llvm齐全）。
    后端值.弧生成释放函数()?;

    // 反向 FFI：库模式下为所有导出函数生成 C ABI 包装 + 运行时构造器。
    // 放在所有 Qi 函数体 / 闭包 / 泛型实例之后 —— 转发目标全部就位。
    if 库模式 {
        后端值.生成导出包装()?;
    }

    // 调试元数据落实必须在 verify 之前：verifier 看半成品会报「invalid
    // debug info」，而那时元数据还只是占位节点，报错信息完全指不到真问题。
    后端值.调试_收尾();

    后端值
        .module
        .verify()
        .map_err(|e| format!("LLVM 模块校验失败: {}", e.to_string()))?;

    // 调试：QI_EMIT_LL=路径 时把类型化 IR 落盘（人工检查字面量 header / ARC 插入等）。
    if let Ok(p) = std::env::var("QI_EMIT_LL") {
        let _ = 后端值.module.print_to_file(Path::new(&p));
    }

    // 优化管线串（目标机档位已在建目标机时定好）。
    let pass_pipeline: Option<String> = match opt {
        crate::config::OptimizationLevel::None => None,
        crate::config::OptimizationLevel::Basic => Some("default<O1>".into()),
        crate::config::OptimizationLevel::Standard => Some("default<O2>".into()),
        crate::config::OptimizationLevel::Maximum => Some("default<O3>".into()),
    };
    // QI_CORO：确保 coroutine 变换的 CoroEarly/CoroSplit/CoroCleanup 一定运行。
    // - default<O1/O2/O3> 已含 coro pass（对无协程模块 no-op）→ 保持不变。
    // - -O0（pass_pipeline=None）下须显式补 coro-only 管线，否则 llvm.coro.* 不 lower。
    // QI_CORO 关时此块不触发 → 默认路径逐字节不变。
    let pass_pipeline = if 后端值.协程开() && pass_pipeline.is_none() {
        Some("coro-early,cgscc(coro-split),coro-cleanup".to_string())
    } else {
        pass_pipeline
    };
    // 模块级优化管线（新 PassManager）。setjmp 已带 returns_twice、
    // retain/release 是不透明外部调用，O3 下语义安全。
    if let Some(pipeline) = &pass_pipeline {
        后端值
            .module
            .run_passes(pipeline, &tm, inkwell::passes::PassBuilderOptions::create())
            .map_err(|e| format!("优化管线失败({}): {}", pipeline, e))?;
    }
    if let Ok(p) = std::env::var("QI_EMIT_LL_POST") {
        let _ = 后端值.module.print_to_file(Path::new(&p));
    }
    tm.write_to_file(&后端值.module, FileType::Object, out)
        .map_err(|e| e.to_string())?;

    // 导出头文件信息（供 lib.rs 生成 .h）。
    let exports: Vec<CExportInfo> = 后端值
        .导出表
        .iter()
        .map(|r| CExportInfo {
            c_name: r.c符号.clone(),
            qi_name: r.原名.clone(),
            params: r
                .参数
                .iter()
                .zip(r.参数名.iter())
                .map(|(t, n)| (qi类型转c(*t, false).to_string(), n.clone()))
                .collect(),
            ret: qi类型转c(r.返回, true).to_string(),
        })
        .collect();
    Ok(exports)
}
