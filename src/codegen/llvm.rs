//! LLVM integration for Qi language

use crate::config::CompilationTarget;
#[cfg(feature = "llvm")]
use crate::parser::ast::{AstNode, FunctionDeclaration, Parameter, TypeNode, BasicType};
#[cfg(feature = "llvm")]
use inkwell::context::Context;
#[cfg(feature = "llvm")]
use inkwell::module::Module;
#[cfg(feature = "llvm")]
use inkwell::builder::Builder;
#[cfg(feature = "llvm")]
use inkwell::types::BasicType;
#[cfg(feature = "llvm")]
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue};

#[cfg(feature = "llvm")]
/// LLVM code generator
pub struct LlvmCodeGenerator {
    context: Context,
    module: Module,
    builder: Builder,
    target: CompilationTarget,
    current_function: Option<FunctionValue>,
    parameter_values: std::collections::HashMap<String, BasicValueEnum>,
}

#[cfg(not(feature = "llvm"))]
/// LLVM code generator placeholder
pub struct LlvmCodeGenerator {
    _private: (),
}

#[cfg(feature = "llvm")]
impl LlvmCodeGenerator {
    pub fn new(target: CompilationTarget) -> Result<Self, LlvmError> {
        let context = Context::create();
        let module = context.create_module("qi_program");
        let builder = context.create_builder();

        let mut generator = Self {
            context,
            module,
            builder,
            target,
            current_function: None,
            parameter_values: std::collections::HashMap::new(),
        };

        generator.setup_target()?;
        Ok(generator)
    }
}

#[cfg(not(feature = "llvm"))]
impl LlvmCodeGenerator {
    pub fn new(_target: crate::config::CompilationTarget) -> Result<Self, LlvmError> {
        Ok(Self { _private: () })
    }
}

#[cfg(feature = "llvm")]
impl LlvmCodeGenerator {
    fn setup_target(&mut self) -> Result<(), LlvmError> {
        // TODO: Set up target triple and data layout
        match self.target {
            CompilationTarget::Linux => {
                // self.module.set_target_triple("x86_64-unknown-linux-gnu");
            }
            CompilationTarget::Windows => {
                // self.module.set_target_triple("x86_64-pc-windows-msvc");
            }
            CompilationTarget::MacOS => {
                // self.module.set_target_triple("x86_64-apple-macosx");
            }
            CompilationTarget::Wasm => {
                // self.module.set_target_triple("wasm32-unknown-unknown");
            }
        }
        Ok(())
    }

    pub fn generate_ir(&mut self, ir: &str) -> Result<String, LlvmError> {
        // TODO: Convert IR to LLVM IR
        todo!("Implement IR to LLVM conversion")
    }

    pub fn optimize(&mut self, level: crate::config::OptimizationLevel) -> Result<(), LlvmError> {
        // TODO: Implement LLVM optimization passes
        todo!("Implement LLVM optimization")
    }

    pub fn write_object_file(&self, path: &str) -> Result<(), LlvmError> {
        // TODO: Write object file
        todo!("Implement object file writing")
    }

    pub fn get_module(&self) -> &Module {
        &self.module
    }

    /// Convert Qi type node to LLVM type
    pub fn qi_type_to_llvm_type(&self, qi_type: &TypeNode) -> Result<inkwell::types::BasicTypeEnum, LlvmError> {
        match qi_type {
            TypeNode::基础类型(basic_type) => {
                match basic_type {
                    BasicType::整数 => Ok(self.context.i64_type().as_basic_type_enum()),
                    BasicType::浮点数 => Ok(self.context.f64_type().as_basic_type_enum()),
                    BasicType::布尔 => Ok(self.context.bool_type().as_basic_type_enum()),
                    BasicType::字符 => Ok(self.context.i8_type().as_basic_type_enum()),
                    BasicType::字符串 => Ok(self.context.i8_type().ptr_type(inkwell::AddressSpace::default()).as_basic_type_enum()),
                    BasicType::空 => Err(LlvmError::IrGeneration("空类型无法转换为 LLVM 类型".to_string())),
                }
            }
            TypeNode::函数类型(_) => Err(LlvmError::IrGeneration("函数类型作为参数暂不支持".to_string())),
            TypeNode::数组类型(_) => Err(LlvmError::IrGeneration("数组类型作为参数暂不支持".to_string())),
            TypeNode::结构体类型(_) => Err(LlvmError::IrGeneration("结构体类型作为参数暂不支持".to_string())),
            TypeNode::枚举类型(_) => Err(LlvmError::IrGeneration("枚举类型作为参数暂不支持".to_string())),
        }
    }

    /// Create LLVM function type from Qi function declaration
    pub fn create_function_type(&self, func_decl: &FunctionDeclaration) -> Result<inkwell::types::FunctionType, LlvmError> {
        // Convert parameter types
        let mut param_types = Vec::new();
        for param in &func_decl.parameters {
            if let Some(type_annotation) = &param.type_annotation {
                param_types.push(self.qi_type_to_llvm_type(type_annotation)?);
            } else {
                // Default to i64 for parameters without type annotation
                param_types.push(self.context.i64_type().as_basic_type_enum());
            }
        }

        // Determine return type
        let return_type = if let Some(return_type) = &func_decl.return_type {
            self.qi_type_to_llvm_type(return_type)?
        } else {
            // Default to void if no return type specified
            self.context.void_type().as_basic_type_enum()
        };

        match return_type {
            inkwell::types::BasicTypeEnum::VoidType(void_type) => {
                Ok(void_type.fn_type(&param_types, false))
            }
            inkwell::types::BasicTypeEnum::IntType(int_type) => {
                Ok(int_type.fn_type(&param_types, false))
            }
            inkwell::types::BasicTypeEnum::FloatType(float_type) => {
                Ok(float_type.fn_type(&param_types, false))
            }
            inkwell::types::BasicTypeEnum::PointerType(ptr_type) => {
                Ok(ptr_type.fn_type(&param_types, false))
            }
            inkwell::types::BasicTypeEnum::StructType(struct_type) => {
                Ok(struct_type.fn_type(&param_types, false))
            }
            inkwell::types::BasicTypeEnum::ArrayType(array_type) => {
                Ok(array_type.fn_type(&param_types, false))
            }
            inkwell::types::BasicTypeEnum::VectorType(vector_type) => {
                Ok(vector_type.fn_type(&param_types, false))
            }
        }
    }

    /// Declare a function in the LLVM module
    pub fn declare_function(&mut self, func_decl: &FunctionDeclaration) -> Result<FunctionValue, LlvmError> {
        let func_type = self.create_function_type(func_decl)?;
        let function_value = self.module.add_function(&func_decl.name, func_type, None);

        // Set parameter names
        for (i, param) in func_decl.parameters.iter().enumerate() {
            if let Some(param_value) = function_value.get_nth_param(i as u32) {
                param_value.set_name(&param.name);
            }
        }

        Ok(function_value)
    }

    /// Start generating function body with parameter handling
    pub fn start_function_body(&mut self, function: FunctionValue, func_decl: &FunctionDeclaration) -> Result<(), LlvmError> {
        self.current_function = Some(function);
        self.parameter_values.clear();

        // Create entry block
        let entry_block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry_block);

        // Store parameter values for later use
        for (i, param) in func_decl.parameters.iter().enumerate() {
            if let Some(param_value) = function.get_nth_param(i as u32) {
                // Create an alloca for the parameter
                let param_type = param_value.get_type();
                let alloca = self.builder.build_alloca(param_type, &format!("{}_addr", param.name));

                // Store the parameter value in the alloca
                self.builder.build_store(alloca, param_value);

                // Store the alloca address for parameter access
                self.parameter_values.insert(param.name.clone(), alloca.into());
            }
        }

        Ok(())
    }

    /// Get parameter value by name
    pub fn get_parameter_value(&self, param_name: &str) -> Option<BasicValueEnum> {
        self.parameter_values.get(param_name).copied()
    }

    /// Load parameter value from its alloca
    pub fn load_parameter(&self, param_name: &str) -> Result<BasicValueEnum, LlvmError> {
        if let Some(param_alloca) = self.parameter_values.get(param_name) {
            Ok(self.builder.build_load(*param_alloca, param_name))
        } else {
            Err(LlvmError::IrGeneration(format!("参数 '{}' 未找到", param_name)))
        }
    }

    /// Generate function call with argument passing
    pub fn generate_function_call(&mut self, callee: &str, arguments: &[AstNode]) -> Result<BasicValueEnum, LlvmError> {
        // Look up function in module
        let function = self.module.get_function(callee)
            .ok_or_else(|| LlvmError::IrGeneration(format!("函数 '{}' 未定义", callee)))?;

        // Check argument count
        if function.count_params() as usize != arguments.len() {
            return Err(LlvmError::IrGeneration(
                format!("函数 '{}' 期望 {} 个参数，实际提供 {} 个",
                    callee, function.count_params(), arguments.len())
            ));
        }

        // Generate argument values
        let mut arg_values = Vec::new();
        for (i, arg) in arguments.iter().enumerate() {
            let arg_value = self.generate_expression(arg)?;

            // Check if type conversion is needed
            let expected_param_type = function.get_nth_param(i as u32)
                .ok_or_else(|| LlvmError::IrGeneration(format!("参数 {} 类型获取失败", i)))?
                .get_type();

            let arg_type = arg_value.get_type();
            if arg_type != expected_param_type {
                // Try to perform type conversion
                let converted_value = self.convert_type(arg_value, arg_type, expected_param_type)?;
                arg_values.push(converted_value);
            } else {
                arg_values.push(arg_value);
            }
        }

        // Build function call
        let call_result = self.builder.build_call(function, &arg_values, "calltmp");

        // Handle void return type
        if function.get_return_type().map_or(false, |t| t.is_void_type()) {
            Ok(self.context.i64_type().const_zero().as_basic_value_enum()) // Return dummy value for void functions
        } else {
            call_result.try_as_basic_value()
                .left()
                .ok_or_else(|| LlvmError::IrGeneration("函数调用返回值处理失败".to_string()))
        }
    }

    /// Generate function call expression with module prefix support
    pub fn generate_function_call_expr(&mut self, call: &crate::parser::ast::FunctionCallExpression) -> Result<BasicValueEnum, LlvmError> {
        // 构建完整的函数名
        let function_name = if let Some(module_qualifier) = &call.module_qualifier {
            // 模块前缀调用，如 数学.最大值
            format!("{}_{}", module_qualifier, call.callee)
        } else {
            // 普通函数调用
            call.callee.clone()
        };

        self.generate_function_call(&function_name, &call.arguments)
    }

    /// Generate expression (simplified implementation)
    pub fn generate_expression(&mut self, expr: &AstNode) -> Result<BasicValueEnum, LlvmError> {
        match expr {
            AstNode::字面量表达式(literal) => self.generate_literal(literal),
            AstNode::标识符表达式(identifier) => self.generate_identifier(identifier),
            AstNode::二元操作表达式(binary) => self.generate_binary_expression(binary),
            AstNode::函数调用表达式(call) => self.generate_function_call_expr(call),
            _ => Err(LlvmError::IrGeneration(format!("不支持的表达式类型: {:?}", expr))),
        }
    }

    /// Generate literal expression
    pub fn generate_literal(&self, literal: &crate::parser::ast::LiteralExpression) -> Result<BasicValueEnum, LlvmError> {
        match &literal.value {
            crate::parser::ast::LiteralValue::整数(value) => {
                Ok(self.context.i64_type().const_int(*value as u64, false).as_basic_value_enum())
            }
            crate::parser::ast::LiteralValue::浮点数(value) => {
                Ok(self.context.f64_type().const_float(*value).as_basic_value_enum())
            }
            crate::parser::ast::LiteralValue::布尔(value) => {
                Ok(self.context.bool_type().const_int(*value as u64, false).as_basic_value_enum())
            }
            crate::parser::ast::LiteralValue::字符(value) => {
                Ok(self.context.i8_type().const_int(*value as u64, false).as_basic_value_enum())
            }
            crate::parser::ast::LiteralValue::字符串(_) => {
                Err(LlvmError::IrGeneration("字符串字面量生成暂未实现".to_string()))
            }
        }
    }

    /// Generate identifier expression (load variable or parameter)
    pub fn generate_identifier(&self, identifier: &crate::parser::ast::IdentifierExpression) -> Result<BasicValueEnum, LlvmError> {
        // First try to load as parameter
        if let Some(param_value) = self.load_parameter(&identifier.name) {
            return Ok(param_value);
        }

        // TODO: Look up in variable symbol table
        Err(LlvmError::IrGeneration(format!("标识符 '{}' 未找到", identifier.name)))
    }

    /// Generate binary expression
    pub fn generate_binary_expression(&mut self, binary: &crate::parser::ast::BinaryExpression) -> Result<BasicValueEnum, LlvmError> {
        let left_value = self.generate_expression(&binary.left)?;
        let right_value = self.generate_expression(&binary.right)?;

        match binary.operator {
            crate::parser::ast::BinaryOperator::加 => {
                if left_value.is_int_value() && right_value.is_int_value() {
                    let left_int = left_value.into_int_value();
                    let right_int = right_value.into_int_value();
                    Ok(self.builder.build_int_add(left_int, right_int, "addtmp").as_basic_value_enum())
                } else if left_value.is_float_value() && right_value.is_float_value() {
                    let left_float = left_value.into_float_value();
                    let right_float = right_value.into_float_value();
                    Ok(self.builder.build_float_add(left_float, right_float, "addtmp").as_basic_value_enum())
                } else {
                    Err(LlvmError::IrGeneration("加法操作类型不匹配".to_string()))
                }
            }
            crate::parser::ast::BinaryOperator::减 => {
                if left_value.is_int_value() && right_value.is_int_value() {
                    let left_int = left_value.into_int_value();
                    let right_int = right_value.into_int_value();
                    Ok(self.builder.build_int_sub(left_int, right_int, "subtmp").as_basic_value_enum())
                } else if left_value.is_float_value() && right_value.is_float_value() {
                    let left_float = left_value.into_float_value();
                    let right_float = right_value.into_float_value();
                    Ok(self.builder.build_float_sub(left_float, right_float, "subtmp").as_basic_value_enum())
                } else {
                    Err(LlvmError::IrGeneration("减法操作类型不匹配".to_string()))
                }
            }
            crate::parser::ast::BinaryOperator::乘 => {
                if left_value.is_int_value() && right_value.is_int_value() {
                    let left_int = left_value.into_int_value();
                    let right_int = right_value.into_int_value();
                    Ok(self.builder.build_int_mul(left_int, right_int, "multmp").as_basic_value_enum())
                } else if left_value.is_float_value() && right_value.is_float_value() {
                    let left_float = left_value.into_float_value();
                    let right_float = right_value.into_float_value();
                    Ok(self.builder.build_float_mul(left_float, right_float, "multmp").as_basic_value_enum())
                } else {
                    Err(LlvmError::IrGeneration("乘法操作类型不匹配".to_string()))
                }
            }
            crate::parser::ast::BinaryOperator::除 => {
                if left_value.is_int_value() && right_value.is_int_value() {
                    let left_int = left_value.into_int_value();
                    let right_int = right_value.into_int_value();
                    Ok(self.builder.build_int_signed_div(left_int, right_int, "divtmp").as_basic_value_enum())
                } else if left_value.is_float_value() && right_value.is_float_value() {
                    let left_float = left_value.into_float_value();
                    let right_float = right_value.into_float_value();
                    Ok(self.builder.build_float_div(left_float, right_float, "divtmp").as_basic_value_enum())
                } else {
                    Err(LlvmError::IrGeneration("除法操作类型不匹配".to_string()))
                }
            }
            _ => Err(LlvmError::IrGeneration(format!("不支持的操作符: {:?}", binary.operator))),
        }
    }

    /// Type conversion between compatible types
    pub fn convert_type(&self, value: BasicValueEnum, from_type: inkwell::types::BasicTypeEnum, to_type: inkwell::types::BasicTypeEnum) -> Result<BasicValueEnum, LlvmError> {
        if from_type == to_type {
            return Ok(value);
        }

        match (from_type, to_type) {
            (inkwell::types::BasicTypeEnum::IntType(_), inkwell::types::BasicTypeEnum::FloatType(_)) => {
                let int_val = value.into_int_value();
                Ok(self.builder.build_sitofp(int_val, to_type.into_float_type(), "sitofp").as_basic_value_enum())
            }
            (inkwell::types::BasicTypeEnum::FloatType(_), inkwell::types::BasicTypeEnum::IntType(_)) => {
                let float_val = value.into_float_value();
                Ok(self.builder.build_fptosi(float_val, to_type.into_int_type(), "fptosi").as_basic_value_enum())
            }
            _ => Err(LlvmError::IrGeneration(format!("不支持的类型转换: {:?} -> {:?}", from_type, to_type))),
        }
    }

    /// End function body generation
    pub fn end_function_body(&mut self) -> Result<(), LlvmError> {
        self.current_function = None;
        self.parameter_values.clear();
        Ok(())
    }
}

#[cfg(not(feature = "llvm"))]
impl LlvmCodeGenerator {
    pub fn generate_ir(&mut self, _ir: &str) -> Result<String, LlvmError> {
        Err(LlvmError::UnsupportedTarget(crate::config::CompilationTarget::Linux))
    }

    pub fn optimize(&mut self, _level: crate::config::OptimizationLevel) -> Result<(), LlvmError> {
        Err(LlvmError::UnsupportedTarget(crate::config::CompilationTarget::Linux))
    }

    pub fn write_object_file(&self, _path: &str) -> Result<(), LlvmError> {
        Err(LlvmError::UnsupportedTarget(crate::config::CompilationTarget::Linux))
    }

    pub fn get_module(&self) -> &() {
        &self._private
    }
}

/// LLVM errors
#[derive(Debug, thiserror::Error)]
pub enum LlvmError {
    /// LLVM initialization error
    #[error("LLVM 初始化错误: {0}")]
    Initialization(String),

    /// Target not supported
    #[error("不支持的目标平台: {0}")]
    UnsupportedTarget(CompilationTarget),

    /// IR generation error
    #[error("IR 生成错误: {0}")]
    IrGeneration(String),

    /// Optimization error
    #[error("优化错误: {0}")]
    Optimization(String),

    /// Object file writing error
    #[error("对象文件写入错误: {0}")]
    ObjectFileWrite(String),
}