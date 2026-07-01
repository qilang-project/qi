//! inkwell 后端 v2 —— 类型化 LLVM IR 生成。
//!
//! 目标：用 inkwell 的强类型 builder 取代 13k 行文本 IR 拼接，从结构上消除
//! i64/ptr 猜错、方法链式失败、重名冲突、缺 span 这一整类问题。
//! 复用现有前端（Parser::parse_source），语法完全不变。
//!
//! 当前进度：hello-world 垂直切片（入口 + 打印行/打印 + 字符串字面量）。逐步长功能。

use crate::config::CompilationTarget;
use crate::parser::ast::{AstNode, LiteralValue, Program};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::AddressSpace;
use inkwell::OptimizationLevel as LlvmOpt;
use std::path::Path;

struct 后端<'ctx> {
    ctx: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
}

impl<'ctx> 后端<'ctx> {
    fn new(ctx: &'ctx Context) -> Self {
        let module = ctx.create_module("qi_program");
        let builder = ctx.create_builder();
        Self {
            ctx,
            module,
            builder,
        }
    }

    /// 声明本切片用到的运行时函数。
    fn 声明运行时(&self) {
        let i32t = self.ctx.i32_type();
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let sig = i32t.fn_type(&[ptrt.into()], false);
        self.module.add_function("qi_runtime_println", sig, None);
        self.module.add_function("qi_runtime_print", sig, None);
    }

    /// 生成 入口() → LLVM main。
    fn 生成入口(&self, program: &Program) -> Result<(), String> {
        let 入口 = program
            .statements
            .iter()
            .find_map(|s| match s {
                AstNode::函数声明(f) if f.name == "入口" => Some(f),
                _ => None,
            })
            .ok_or_else(|| "未找到 入口() 函数".to_string())?;

        let i32t = self.ctx.i32_type();
        let main_fn = self.module.add_function("main", i32t.fn_type(&[], false), None);
        let bb = self.ctx.append_basic_block(main_fn, "entry");
        self.builder.position_at_end(bb);

        for stmt in &入口.body {
            self.生成语句(stmt)?;
        }

        self.builder
            .build_return(Some(&i32t.const_int(0, false)))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn 生成语句(&self, node: &AstNode) -> Result<(), String> {
        match node {
            AstNode::表达式语句(es) => {
                self.生成表达式(&es.expression)?;
                Ok(())
            }
            _ => Ok(()), // 切片阶段：其余语句暂略
        }
    }

    /// 生成表达式，返回其 LLVM 值（切片：字符串字面量 → i8* 指针）。
    fn 生成表达式(&self, node: &AstNode) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        match node {
            AstNode::字面量表达式(lit) => match &lit.value {
                LiteralValue::字符串(s) => {
                    let g = self
                        .builder
                        .build_global_string_ptr(s, "str")
                        .map_err(|e| e.to_string())?;
                    Ok(Some(g.as_pointer_value().into()))
                }
                _ => Ok(None),
            },
            AstNode::函数调用表达式(call) => {
                let rtname = match call.callee.as_str() {
                    "打印行" => Some("qi_runtime_println"),
                    "打印" => Some("qi_runtime_print"),
                    _ => None,
                };
                if let Some(name) = rtname {
                    self.发射调用(name, &call.arguments)?;
                }
                Ok(None)
            }
            AstNode::方法调用表达式(mc) => {
                // 切片：IO.打印行(...) / 打印(...) 归一到运行时
                let rtname = match mc.method_name.as_str() {
                    "打印行" => Some("qi_runtime_println"),
                    "打印" => Some("qi_runtime_print"),
                    _ => None,
                };
                if let Some(name) = rtname {
                    self.发射调用(name, &mc.arguments)?;
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn 发射调用(&self, rtname: &str, arguments: &[AstNode]) -> Result<(), String> {
        let f = self
            .module
            .get_function(rtname)
            .ok_or_else(|| format!("运行时函数未声明: {}", rtname))?;
        let mut args: Vec<BasicMetadataValueEnum> = Vec::new();
        for a in arguments {
            if let Some(v) = self.生成表达式(a)? {
                args.push(v.into());
            }
        }
        self.builder
            .build_call(f, &args, "call")
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 把 Program 编译成目标文件（.o）。切片阶段：host 默认三元组。
pub fn compile_to_object(
    program: &Program,
    out: &Path,
    _target: CompilationTarget,
) -> Result<(), String> {
    let ctx = Context::create();
    let 后端值 = 后端::new(&ctx);
    后端值.声明运行时();
    后端值.生成入口(program)?;

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
