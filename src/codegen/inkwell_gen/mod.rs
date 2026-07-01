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
use inkwell::values::PointerValue;
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
        }
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
    }

    /// 生成 入口() → LLVM main。
    fn 生成入口(&mut self, program: &Program) -> Result<(), String> {
        let 入口 = program
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

        self.变量表.clear();
        self.符号.进入作用域();
        self.当前返回类型 = Qi类型::空;

        for stmt in &入口.body {
            self.生成语句(stmt, main_fn)?;
            if self.当前块已终结() {
                break;
            }
        }

        self.符号.退出作用域();

        if !self.当前块已终结() {
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

    // 第一趟：登记所有模块的结构体（其它签名会引用结构体类型，必须最先）
    for p in programs {
        后端值.登记结构体(p)?;
    }

    // 第二趟：登记所有模块的函数 / 方法签名 + LLVM 原型
    for p in programs {
        后端值.登记函数(p)?;
        后端值.登记方法(p)?;
    }

    // 第三趟：生成所有模块的用户函数体（跳过重复的 入口，只 entry 的算数）
    for (i, p) in programs.iter().enumerate() {
        for stmt in &p.statements {
            if let AstNode::函数声明(f) = stmt {
                if f.name == "入口" {
                    continue; // 入口只在最后为 entry 生成 main
                }
                后端值.生成函数体(f)?;
            }
        }
        let _ = i;
    }

    // 第四趟：生成所有模块的方法体
    for p in programs {
        后端值.生成所有方法体(p)?;
    }

    // 入口 → main（只用 entry 程序 programs[0]）
    后端值.生成入口(&programs[0])?;

    后端值
        .module
        .verify()
        .map_err(|e| format!("LLVM 模块校验失败: {}", e.to_string()))?;

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
