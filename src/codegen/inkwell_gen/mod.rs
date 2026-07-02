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

#[path = "所有权.rs"]
mod 所有权;
#[path = "全局.rs"]
mod 全局;
#[path = "异步.rs"]
mod 异步;
#[path = "数组.rs"]
mod 数组;
#[path = "并发.rs"]
mod 并发;
#[path = "声明.rs"]
mod 声明;
#[path = "导入.rs"]
mod 导入;
#[path = "方法.rs"]
mod 方法;
#[path = "结构体.rs"]
mod 结构体;
#[path = "闭包.rs"]
mod 闭包;
#[path = "表达式.rs"]
mod 表达式;
#[path = "语句.rs"]
mod 语句;
#[path = "类型.rs"]
mod 类型;
#[path = "类型检查.rs"]
mod 类型检查;

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
    let hex: String = name.as_bytes().iter().map(|b| format!("{:02X}", b)).collect();
    format!("_Z_{}", hex)
}

/// 包内用户函数的唯一 LLVM 符号：把 `包名$函数名` 一起 mangle，避免跨包同名冲突
/// （如 主程序.注册 vs Harness.注册）。`入口` 不修饰（→ main，单独处理）。
/// 无包名时退回裸 mangle（与旧行为一致）。
fn 包内符号名(pkg: Option<&str>, name: &str) -> String {
    match pkg {
        Some(p) => mangle_function_name(&format!("{}${}", p, name)),
        None => mangle_function_name(name),
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
    /// 是否正在生成 入口→main（main 返回 i32，bare `返回` 要 emit ret i32 0）。
    在入口中: bool,
    /// 当前正在处理的模块包名（跨包同名函数消歧用；与 符号.当前包 同步）。
    当前包: Option<String>,
    /// 字符串字面量 → 带 immortal header 的全局常量 data 指针（按内容去重，模块级缓存）。
    字符串字面量缓存: HashMap<String, PointerValue<'ctx>>,
    /// QI_ARC=1 时为 true：插入保守字符串 ARC（retain/release）。默认关，
    /// 关时生成的 IR 与无此功能时完全一致。见 所有权.rs。
    弧: bool,
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
            在入口中: false,
            当前包: None,
            字符串字面量缓存: HashMap::new(),
            弧: std::env::var("QI_ARC")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }

    /// 设置当前包（同步到符号表，供签名解析）。
    fn 设当前包(&mut self, pkg: Option<String>) {
        self.符号.当前包 = pkg.clone();
        self.当前包 = pkg;
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
        self.module.add_function("qi_runtime_println_int", 收i64, None);
        self.module.add_function("qi_runtime_print_int", 收i64, None);

        let 收f64 = i32t.fn_type(&[f64t.into()], false);
        self.module.add_function("qi_runtime_println_float", 收f64, None);
        self.module.add_function("qi_runtime_print_float", 收f64, None);

        let 收i32 = i32t.fn_type(&[i32t.into()], false);
        self.module.add_function("qi_runtime_println_bool", 收i32, None);
        self.module.add_function("qi_runtime_print_bool", 收i32, None);

        // 字符串
        let 拼接 = ptrt.fn_type(&[ptrt.into(), ptrt.into()], false);
        self.module.add_function("qi_runtime_string_concat", 拼接, None);
        // 字符串比较（== / != / </ > 等；返回 <0/0/>0）
        let 比较 = i32t.fn_type(&[ptrt.into(), ptrt.into()], false);
        self.module.add_function("qi_runtime_string_compare", 比较, None);
        // 释放临时字符串（拼接链中间结果 / int_to_string 临时值）。
        // 仅对 codegen 结构上确定「新建且不逃逸」的临时值发，绝不对字面量/变量发。
        let 释放串 = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        self.module.add_function("qi_string_free", 释放串, None);
        // 增引用（QI_ARC 插桩用；null/immortal/非 RC 指针皆 no-op）
        let 保留串 = self.ctx.void_type().fn_type(&[ptrt.into()], false);
        self.module.add_function("qi_string_retain", 保留串, None);

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
        self.module
            .add_function("qi_closure_get_fn", ptrt.fn_type(&[ptrt.into()], false), None);
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
            self.ctx.void_type().fn_type(&[ptrt.into(), i64t.into(), i64t.into()], false),
            None,
        );
        self.module.add_function(
            "qi_closure_set_ptr",
            self.ctx.void_type().fn_type(&[ptrt.into(), i64t.into(), ptrt.into()], false),
            None,
        );

        // future / async（eager future 模型）
        self.声明future运行时();
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
        let main_fn = self.module.add_function("main", i32t.fn_type(&[], false), None);
        let bb = self.ctx.append_basic_block(main_fn, "entry");
        self.builder.position_at_end(bb);

        self.设当前包(programs[0].package_name.clone());
        self.变量表.clear();
        self.符号.进入作用域();
        self.当前返回类型 = Qi类型::空;
        self.在入口中 = true; // main 返回 i32：bare `返回` 要 ret i32 0

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
            // ARC：main 顺利落底时释放入口的字符串局部
            self.弧释放局部()?;
            self.builder
                .build_return(Some(&i32t.const_int(0, false)))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// 把单个 Program 编译成目标文件（.o）。保留给单文件调用点。
pub fn compile_to_object(
    program: &Program,
    out: &Path,
    target: CompilationTarget,
) -> Result<(), String> {
    compile_to_object_multi(std::slice::from_ref(program), out, target)
}

/// 把多个 Program（entry + 所有用户模块）合并进同一个 LLVM 模块，编成一个 .o。
/// programs[0] 是 entry（含 入口()）。跨模块符号共享同一 mangle，天然可见。
pub fn compile_to_object_multi(
    programs: &[Program],
    out: &Path,
    _target: CompilationTarget,
) -> Result<(), String> {
    if programs.is_empty() {
        return Err("没有可编译的模块".to_string());
    }
    let ctx = Context::create();
    let mut 后端值 = 后端::new(&ctx);
    后端值.声明运行时();

    // 收集所有模块的导入别名（标准库导入）
    for p in programs {
        后端值.收集导入别名(p);
    }

    // 第一趟：结构体三步（名字→字段类型→LLVM 类型）。名字必须先全部登记，
    // 跨模块 / 前向引用的字段类型（如 代理.工具表: 注册表）才能解析成 结构体(idx)。
    for p in programs {
        后端值.登记结构体名字(p)?;
    }
    for p in programs {
        后端值.解析结构体字段(p)?;
    }
    后端值.建结构体llvm类型()?;

    // 第一趟半：登记所有模块顶层全局变量 / 常量（函数体会引用，须在函数体前）
    for p in programs {
        后端值.登记全局变量(p)?;
    }

    // 第二趟：登记所有模块的函数 / 方法签名 + LLVM 原型（按包消歧）
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.登记函数(p)?;
        后端值.登记方法(p)?;
    }

    // 第三趟：生成所有模块的用户函数体（跳过重复的 入口，只 entry 的算数）
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        for stmt in &p.statements {
            if let AstNode::函数声明(f) = stmt {
                if f.name == "入口" {
                    continue; // 入口只在最后为 entry 生成 main
                }
                后端值.生成函数体(f)?;
            }
        }
    }

    // 第四趟：生成所有模块的方法体
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.生成所有方法体(p)?;
    }

    // 入口 → main（entry=programs[0]；全局初始化用所有 programs）
    后端值.生成入口(programs)?;

    // 合成所有待处理闭包函数（函数体/方法体/入口里遇到的匿名闭包）。
    // 闭包体内可能再产生闭包，循环到清空为止。
    后端值.合成待处理闭包()?;

    后端值
        .module
        .verify()
        .map_err(|e| format!("LLVM 模块校验失败: {}", e.to_string()))?;

    // 调试：QI_EMIT_LL=路径 时把类型化 IR 落盘（人工检查字面量 header / ARC 插入等）。
    if let Ok(p) = std::env::var("QI_EMIT_LL") {
        let _ = 后端值.module.print_to_file(Path::new(&p));
    }

    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("初始化目标失败: {}", e))?;
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let tm = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            LlvmOpt::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| "无法创建 target machine".to_string())?;
    后端值.module.set_triple(&triple);
    tm.write_to_file(&后端值.module, FileType::Object, out)
        .map_err(|e| e.to_string())?;
    Ok(())
}
