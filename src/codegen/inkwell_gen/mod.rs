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
#[path = "反射.rs"]
mod 反射;
#[path = "剖析.rs"]
mod 剖析;
#[path = "匹配.rs"]
mod 匹配;
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
fn 包内符号名(pkg: Option<&str>, name: &str, 元数: usize) -> String {
    match pkg {
        Some(p) => mangle_function_name(&format!("{}${}#{}", p, name, 元数)),
        None => mangle_function_name(&format!("{}#{}", name, 元数)),
    }
}

struct 后端<'ctx> {
    ctx: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
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
    /// 已实例化的泛型函数实例名（幂等去重）。
    已实例化泛型函数: std::collections::HashSet<String>,
    /// 是否正在生成 入口→main（main 返回 i32，bare `返回` 要 emit ret i32 0）。
    在入口中: bool,
    /// 当前正在处理的模块包名（跨包同名函数消歧用；与 符号.当前包 同步）。
    当前包: Option<String>,
    /// 字符串字面量 → 带 immortal header 的全局常量 data 指针（按内容去重，模块级缓存）。
    字符串字面量缓存: HashMap<String, PointerValue<'ctx>>,
    /// QI_ARC=1 时为 true：插入保守字符串 ARC（retain/release）。默认关，
    /// 关时生成的 IR 与无此功能时完全一致。见 所有权.rs。
    弧: bool,
    /// 当前 `尝试` 嵌套深度（Round E4）：进 try 体 +1，出体/进 catch -1。
    /// `返回` 语句在 ret 前按当前深度补 qi_exc_pop × N，防 longjmp 到已失效帧。
    /// 每个函数体/方法体/闭包体/入口独立清零（帧栈按函数平衡）。
    try深度: u32,
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
}

impl<'ctx> 后端<'ctx> {
    fn new(ctx: &'ctx Context) -> Self {
        let module = ctx.create_module("qi_program");
        let builder = ctx.create_builder();
        Self {
            ctx,
            module,
            builder,
            符号: 符号表::new(),
            变量表: HashMap::new(),
            当前返回类型: Qi类型::空,
            结构体llvm: Vec::new(),
            注册表: ModuleRegistry::new(),
            导入别名: HashMap::new(),
            全局变量表: HashMap::new(),
            已初始化全局: std::collections::HashSet::new(),
            待合成闭包: Vec::new(),
            闭包计数: 0,
            待合成泛型实例: Vec::new(),
            已实例化泛型函数: std::collections::HashSet::new(),
            在入口中: false,
            当前包: None,
            字符串字面量缓存: HashMap::new(),
            // ARC 默认开(字符串+结构体+数组+闭包 RC 回收);QI_ARC=0 退回纯泄漏模式(调试用)
            弧: std::env::var("QI_ARC")
                .map(|v| !(v == "0" || v.eq_ignore_ascii_case("false")))
                .unwrap_or(true),
            try深度: 0,
            循环栈: Vec::new(),
            // 剖析默认关（QI_PROF 未设或为 0/false）：零注入、IR 与无此功能一致。
            剖析: std::env::var("QI_PROF")
                .map(|v| !(v == "0" || v.eq_ignore_ascii_case("false")))
                .unwrap_or(false),
            剖析名: None,
            剖析槽: None,
            导出表: Vec::new(),
            弱局部: std::collections::HashSet::new(),
        }
    }

    /// 设置当前包（同步到符号表，供签名解析）。
    fn 设当前包(&mut self, pkg: Option<String>) {
        self.符号.当前包 = pkg.clone();
        self.当前包 = pkg;
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

        self.设当前包(programs[0].package_name.clone());
        self.变量表.clear();
        self.符号.进入作用域();
        self.当前返回类型 = Qi类型::空;
        self.try深度 = 0; // E4
        self.在入口中 = true; // main 返回 i32：bare `返回` 要 ret i32 0

        // 剖析：main（入口）序言计时。atexit 报告在进程退出、main 返回之后打印。
        self.剖析入口("入口")?;

        // 反射：把用户函数 / 结构体 / 枚举 元数据登记进运行时注册表
        //（早于任何用户代码，含全局初始化）。供 反射.* 自省。
        self.生成反射注册()?;

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
                              // ARC：main 顺利落底时释放入口的字符串局部
            self.弧释放局部()?;
            self.builder
                .build_return(Some(&i32t.const_int(0, false)))
                .map_err(|e| e.to_string())?;
        }
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
pub fn compile_to_object_multi(
    programs: &[Program],
    out: &Path,
    target: CompilationTarget,
    arch: Option<&str>,
    opt: crate::config::OptimizationLevel,
    库模式: bool,
) -> Result<Vec<CExportInfo>, String> {
    if programs.is_empty() {
        return Err("没有可编译的模块".to_string());
    }
    let ctx = Context::create();
    let mut 后端值 = 后端::new(&ctx);
    后端值.声明运行时();

    // 收集所有模块的导入别名（标准库导入）+ destructure 导入映射（跨包同名结构体消歧）
    for p in programs {
        后端值.收集导入别名(p);
        后端值.收集符号导入(p);
    }

    // 第一趟：结构体三步（名字→字段类型→LLVM 类型）。名字必须先全部登记，
    // 跨模块 / 前向引用的字段类型（如 代理.工具表: 注册表）才能解析成 结构体(idx)。
    // 两步都必须带包上下文 —— 结构体按 (包, 名字) 登记，跨包同名各占独立索引。
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.登记结构体名字(p)?;
    }
    // 枚举名字紧随结构体名字登记：结构体字段 / 枚举载荷 里跨引用彼此都能解析成型。
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.登记枚举名字(p)?;
    }
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.解析结构体字段(p)?;
    }
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.解析枚举变体(p)?;
    }
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
        后端值.设当前包(p.package_name.clone());
        后端值.登记全局变量(p)?;
    }

    // 第二趟：登记所有模块的函数 / 方法签名 + LLVM 原型（按包消歧）
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.登记函数(p)?;
        后端值.登记方法(p)?;
    }

    // 函数名遮蔽校验：本地函数不得与 destructure 导入的同名函数冲突（Qi 不按签名解析）。
    // 在 登记函数 之后 —— 依赖 函数按包 判定导入项是否为函数。
    for p in programs {
        后端值.检查导入遮蔽(p)?;
    }

    // 外部 C 函数声明（`外部 "库" { ... }`）：建 C 名原型 + 存签名。
    // 在用户函数登记之后 —— 撞名（外部与用户函数同名）在此报错。
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.登记外部(p)?;
    }

    // 反向 FFI：登记所有 `导出 函数`（建内部 Qi 原型 + 记录导出元数据）。
    // 在用户函数登记之后 —— 内部原型与普通用户函数共用同一命名空间。
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.登记导出(p)?;
    }

    // 第三趟：生成所有模块的用户函数体（跳过重复的 入口，只 entry 的算数；
    // 泛型函数模板不直接生成 —— 按调用点单态实例化，体在末尾统一合成）
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        for stmt in &p.statements {
            match stmt {
                AstNode::函数声明(f) => {
                    if f.name == "入口" || !f.type_params.is_empty() {
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
        后端值.设当前包(p.package_name.clone());
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
        if 后端值.待合成闭包.is_empty() && 后端值.待合成泛型实例.is_empty() {
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

    后端值
        .module
        .verify()
        .map_err(|e| format!("LLVM 模块校验失败: {}", e.to_string()))?;

    // 调试：QI_EMIT_LL=路径 时把类型化 IR 落盘（人工检查字面量 header / ARC 插入等）。
    if let Ok(p) = std::env::var("QI_EMIT_LL") {
        let _ = 后端值.module.print_to_file(Path::new(&p));
    }

    // 目标三元组：宿主==目标 时走默认三元组（本地路径零风险）；
    // 交叉时按 平台+架构 显式构造（cutover 到 inkwell 时曾丢失，产物错成宿主 Mach-O）。
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
    // 优化级别 → (目标机 codegen 级别, 新 PassManager 管线串)
    let (tm_opt, pass_pipeline) = match opt {
        crate::config::OptimizationLevel::None => (LlvmOpt::None, None),
        crate::config::OptimizationLevel::Basic => (LlvmOpt::Less, Some("default<O1>")),
        crate::config::OptimizationLevel::Standard => (LlvmOpt::Default, Some("default<O2>")),
        crate::config::OptimizationLevel::Maximum => (LlvmOpt::Aggressive, Some("default<O3>")),
    };
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let tm = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            tm_opt,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| "无法创建 target machine".to_string())?;
    后端值.module.set_triple(&triple);
    // 数据布局必须跟目标机匹配（结构体偏移/对齐在跨架构时不同）；
    // 也必须在跑优化 pass 之前设好，否则按错误布局折叠常量/对齐。
    后端值
        .module
        .set_data_layout(&tm.get_target_data().get_data_layout());
    // 模块级优化管线（新 PassManager）。setjmp 已带 returns_twice、
    // retain/release 是不透明外部调用，O3 下语义安全。
    if let Some(pipeline) = pass_pipeline {
        后端值
            .module
            .run_passes(pipeline, &tm, inkwell::passes::PassBuilderOptions::create())
            .map_err(|e| format!("优化管线失败({}): {}", pipeline, e))?;
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
