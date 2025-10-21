//! Type checking and inference for Qi language

use crate::parser::ast::{AstNode, TypeNode, BasicType};
use crate::semantic::symbol_table::SymbolTable;

/// Type checker
pub struct TypeChecker {
    pub symbol_table: SymbolTable,
    errors: Vec<TypeError>,
}

/// Type checking errors
#[derive(Debug, thiserror::Error)]
pub enum TypeError {
    /// Type mismatch
    #[error("类型不匹配: 期望 {expected}, 实际 {actual}")]
    TypeMismatch {
        expected: String,
        actual: String,
        span: crate::lexer::Span,
    },

    /// Undefined variable
    #[error("未定义的变量: {name}")]
    UndefinedVariable {
        name: String,
        span: crate::lexer::Span,
    },

    /// Invalid operation
    #[error("无效的操作: {operation} 对于类型 {type_name}")]
    InvalidOperation {
        operation: String,
        type_name: String,
        span: crate::lexer::Span,
    },

    /// Function call error
    #[error("函数调用错误: {message}")]
    FunctionCallError {
        message: String,
        span: crate::lexer::Span,
    },

    /// Generic type error
    #[error("类型错误: {message}")]
    General {
        message: String,
        span: crate::lexer::Span,
    },
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            errors: Vec::new(),
        }
    }

    pub fn with_symbol_table(symbol_table: SymbolTable) -> Self {
        Self {
            symbol_table,
            errors: Vec::new(),
        }
    }

    pub fn check(&mut self, ast: &AstNode) -> Result<TypeNode, TypeError> {
        match ast {
            AstNode::字面量表达式(literal) => self.check_literal(literal),
            AstNode::标识符表达式(identifier) => self.check_identifier(identifier),
            AstNode::二元操作表达式(binary) => self.check_binary(binary),
            AstNode::函数调用表达式(call) => self.check_function_call(call),
            AstNode::赋值表达式(assignment) => self.check_assignment(assignment),
            AstNode::变量声明(decl) => self.check_variable_declaration(decl),
            AstNode::函数声明(func) => self.check_function_declaration(func),
            AstNode::结构体实例化表达式(struct_literal) => self.check_struct_literal(struct_literal),
            AstNode::字段访问表达式(field_access) => self.check_field_access(field_access),
            AstNode::数组访问表达式(array_access) => self.check_array_access(array_access),
            AstNode::数组字面量表达式(array_literal) => self.check_array_literal(array_literal),
            AstNode::字符串连接表达式(string_concat) => self.check_string_concat(string_concat),
            AstNode::如果语句(if_stmt) => self.check_if_statement(if_stmt),
            AstNode::当语句(while_stmt) => self.check_while_statement(while_stmt),
            AstNode::对于语句(for_stmt) => self.check_for_statement(for_stmt),
            AstNode::返回语句(return_stmt) => self.check_return_statement(return_stmt),
            AstNode::表达式语句(expr_stmt) => self.check_expression_statement(expr_stmt),
            AstNode::程序(program) => self.check_program(program),
            AstNode::结构体声明(struct_decl) => self.check_struct_declaration(struct_decl),
            AstNode::枚举声明(enum_decl) => self.check_enum_declaration(enum_decl),
            AstNode::打印语句(print_stmt) => self.check_print_statement(print_stmt),
            AstNode::循环语句(loop_stmt) => self.check_loop_statement(loop_stmt),
            AstNode::块语句(block_stmt) => self.check_block_statement(block_stmt),
        }
    }

    fn check_literal(&self, literal: &crate::parser::ast::LiteralExpression) -> Result<TypeNode, TypeError> {
        let type_node = match literal.value {
            crate::parser::ast::LiteralValue::整数(_) => TypeNode::基础类型(BasicType::整数),
            crate::parser::ast::LiteralValue::浮点数(_) => TypeNode::基础类型(BasicType::浮点数),
            crate::parser::ast::LiteralValue::字符串(_) => TypeNode::基础类型(BasicType::字符串),
            crate::parser::ast::LiteralValue::布尔(_) => TypeNode::基础类型(BasicType::布尔),
            crate::parser::ast::LiteralValue::字符(_) => TypeNode::基础类型(BasicType::字符),
        };
        Ok(type_node)
    }

    fn check_identifier(&self, identifier: &crate::parser::ast::IdentifierExpression) -> Result<TypeNode, TypeError> {
        match self.symbol_table.lookup_symbol(&identifier.name) {
            Some(symbol) => Ok(symbol.type_node.clone()),
            None => Err(TypeError::UndefinedVariable {
                name: identifier.name.clone(),
                span: identifier.span,
            }),
        }
    }

    fn check_binary(&mut self, binary: &crate::parser::ast::BinaryExpression) -> Result<TypeNode, TypeError> {
        let left_type = self.check(&binary.left)?;
        let right_type = self.check(&binary.right)?;

        // Check operator and determine result type
        match binary.operator {
            // Comparison operators always return boolean
            crate::parser::ast::BinaryOperator::大于 |
            crate::parser::ast::BinaryOperator::小于 |
            crate::parser::ast::BinaryOperator::大于等于 |
            crate::parser::ast::BinaryOperator::小于等于 |
            crate::parser::ast::BinaryOperator::等于 |
            crate::parser::ast::BinaryOperator::不等于 => {
                // Check that operands are compatible (both numeric or both strings for equality)
                if self.are_comparable_types(&left_type, &right_type) {
                    Ok(TypeNode::基础类型(BasicType::布尔))
                } else {
                    Err(TypeError::TypeMismatch {
                        expected: format!("{:?}", left_type),
                        actual: format!("{:?}", right_type),
                        span: binary.span,
                    })
                }
            }
            // Arithmetic operators
            crate::parser::ast::BinaryOperator::加 => {
                // Special handling for string concatenation
                let is_left_string = matches!(left_type, TypeNode::基础类型(BasicType::字符串));
                let is_right_string = matches!(right_type, TypeNode::基础类型(BasicType::字符串));

                if is_left_string && is_right_string {
                    // Both operands are strings - string concatenation
                    Ok(TypeNode::基础类型(BasicType::字符串))
                } else if is_left_string || is_right_string {
                    // One operand is string, other is not - invalid operation
                    return Err(TypeError::InvalidOperation {
                        operation: "字符串连接".to_string(),
                        type_name: format!("{:?} + {:?}", left_type, right_type),
                        span: binary.span,
                    });
                } else if left_type == right_type {
                    // Both operands are the same numeric type
                    Ok(left_type)
                } else {
                    Err(TypeError::TypeMismatch {
                        expected: format!("{:?}", left_type),
                        actual: format!("{:?}", right_type),
                        span: binary.span,
                    })
                }
            }
            crate::parser::ast::BinaryOperator::减 |
            crate::parser::ast::BinaryOperator::乘 |
            crate::parser::ast::BinaryOperator::除 |
            crate::parser::ast::BinaryOperator::取余 => {
                if left_type == right_type {
                    Ok(left_type)
                } else {
                    Err(TypeError::TypeMismatch {
                        expected: format!("{:?}", left_type),
                        actual: format!("{:?}", right_type),
                        span: binary.span,
                    })
                }
            }
            // Logical operators work with boolean operands and return boolean
            crate::parser::ast::BinaryOperator::与 |
            crate::parser::ast::BinaryOperator::或 => {
                match (&left_type, &right_type) {
                    (TypeNode::基础类型(BasicType::布尔), TypeNode::基础类型(BasicType::布尔)) => {
                        Ok(TypeNode::基础类型(BasicType::布尔))
                    }
                    _ => {
                        Err(TypeError::TypeMismatch {
                            expected: "布尔".to_string(),
                            actual: format!("{:?} 和 {:?}", left_type, right_type),
                            span: binary.span,
                        })
                    }
                }
            }
        }
    }

    /// Check if two types are compatible (for assignment/initialization)
    fn are_types_compatible(&self, expected: &TypeNode, actual: &TypeNode) -> bool {
        match (expected, actual) {
            // Exact types are always compatible
            (expected, actual) if expected == actual => true,

            // Array type compatibility: Array<T> is compatible with Array<T>[N] regardless of size
            (TypeNode::数组类型(expected_array), TypeNode::数组类型(actual_array)) => {
                // Check if element types are compatible, ignore size differences
                self.are_types_compatible(&expected_array.element_type, &actual_array.element_type)
            }

            // Basic type compatibility with implicit conversions
            (TypeNode::基础类型(BasicType::整数), TypeNode::基础类型(BasicType::浮点数)) => true,
            (TypeNode::基础类型(BasicType::浮点数), TypeNode::基础类型(BasicType::整数)) => true,

            // Function type compatibility
            (TypeNode::函数类型(expected_func), TypeNode::函数类型(actual_func)) => {
                if expected_func.parameters.len() != actual_func.parameters.len() {
                    return false;
                }

                // Check parameter types (contravariant)
                for (exp_param, act_param) in expected_func.parameters.iter().zip(actual_func.parameters.iter()) {
                    if !self.are_types_compatible(act_param, exp_param) {
                        return false;
                    }
                }

                // Check return types (covariant)
                self.are_types_compatible(&expected_func.return_type, &actual_func.return_type)
            }

            _ => false,
        }
    }

    /// Check if two types are comparable
    fn are_comparable_types(&self, left: &TypeNode, right: &TypeNode) -> bool {
        match (left, right) {
            // Numbers can be compared with each other
            (TypeNode::基础类型(BasicType::整数), TypeNode::基础类型(BasicType::整数)) => true,
            (TypeNode::基础类型(BasicType::浮点数), TypeNode::基础类型(BasicType::浮点数)) => true,
            (TypeNode::基础类型(BasicType::整数), TypeNode::基础类型(BasicType::浮点数)) => true,
            (TypeNode::基础类型(BasicType::浮点数), TypeNode::基础类型(BasicType::整数)) => true,
            // Strings can be compared with each other (for equality)
            (TypeNode::基础类型(BasicType::字符串), TypeNode::基础类型(BasicType::字符串)) => true,
            // Booleans can be compared with each other
            (TypeNode::基础类型(BasicType::布尔), TypeNode::基础类型(BasicType::布尔)) => true,
            // Otherwise not comparable
            _ => false,
        }
    }

    pub fn check_function_call(&self, call: &crate::parser::ast::FunctionCallExpression) -> Result<TypeNode, TypeError> {
        match self.symbol_table.lookup_symbol(&call.callee) {
            Some(symbol) => {
                if let crate::semantic::symbol_table::SymbolKind::函数(func_info) = &symbol.kind {
                    // TODO: Check parameter count and types
                    Ok(func_info.return_type.clone())
                } else {
                    Err(TypeError::FunctionCallError {
                        message: format!("'{}' 不是一个函数", call.callee),
                        span: call.span,
                    })
                }
            }
            None => Err(TypeError::FunctionCallError {
                message: format!("未定义的函数 '{}'", call.callee),
                span: call.span,
            }),
        }
    }

    fn check_assignment(&mut self, assignment: &crate::parser::ast::AssignmentExpression) -> Result<TypeNode, TypeError> {
        let value_type = self.check(&assignment.value)?;

        // Check if target variable exists
        match self.symbol_table.lookup_symbol(&assignment.target) {
            Some(symbol) => {
                if symbol.type_node == value_type {
                    Ok(value_type)
                } else {
                    Err(TypeError::TypeMismatch {
                        expected: format!("{:?}", symbol.type_node),
                        actual: format!("{:?}", value_type),
                        span: assignment.span,
                    })
                }
            }
            None => Err(TypeError::UndefinedVariable {
                name: assignment.target.clone(),
                span: assignment.span,
            }),
        }
    }

    pub fn check_variable_declaration(&mut self, decl: &crate::parser::ast::VariableDeclaration) -> Result<TypeNode, TypeError> {
        let declared_type = decl.type_annotation.clone();

        let initializer_type = if let Some(initializer) = &decl.initializer {
            Some(self.check(initializer)?)
        } else {
            None
        };

        // If no type annotation, use type inference
        let final_type = if let Some(declared) = declared_type {
            // Check type compatibility
            if let Some(init_type) = initializer_type {
                if !self.are_types_compatible(&declared, &init_type) {
                    return Err(TypeError::TypeMismatch {
                        expected: format!("{:?}", declared),
                        actual: format!("{:?}", init_type),
                        span: decl.span,
                    });
                }
            }
            declared
        } else {
            // Type inference: use initializer type or default to 空
            initializer_type.unwrap_or_else(|| TypeNode::基础类型(BasicType::空))
        };

        // Register the variable in the symbol table
        let var_symbol = crate::semantic::symbol_table::Symbol {
            name: decl.name.clone(),
            kind: crate::semantic::symbol_table::SymbolKind::变量,
            type_node: final_type.clone(),
            scope_level: self.symbol_table.current_scope(),
            span: decl.span,
            is_mutable: decl.is_mutable,
        };

        if let Err(scope_error) = self.symbol_table.define_symbol(var_symbol) {
            return Err(TypeError::General {
                message: format!("变量定义错误: {}", scope_error),
                span: decl.span,
            });
        }

        Ok(final_type)
    }

    pub fn check_function_declaration(&mut self, func: &crate::parser::ast::FunctionDeclaration) -> Result<TypeNode, TypeError> {
        // Create function info
        let function_info = crate::semantic::symbol_table::FunctionInfo {
            parameters: func.parameters.clone(),
            return_type: func.return_type.clone().unwrap_or_else(|| TypeNode::基础类型(BasicType::空)),
            is_defined: true,
        };

        // Register function in current scope
        let function_symbol = crate::semantic::symbol_table::Symbol {
            name: func.name.clone(),
            kind: crate::semantic::symbol_table::SymbolKind::函数(function_info),
            type_node: func.return_type.clone().unwrap_or_else(|| TypeNode::基础类型(BasicType::空)),
            scope_level: self.symbol_table.current_scope(),
            span: func.span,
            is_mutable: false,
        };

        if let Err(scope_error) = self.symbol_table.define_symbol(function_symbol) {
            return Err(TypeError::General {
                message: format!("符号定义错误: {}", scope_error),
                span: func.span,
            });
        }

        // Enter new scope for function parameters and body
        self.symbol_table.enter_scope();

        // Process parameters and add them to function scope
        for param in &func.parameters {
            let param_type = param.type_annotation.clone()
                .unwrap_or_else(|| TypeNode::基础类型(BasicType::空));

            let param_symbol = crate::semantic::symbol_table::Symbol {
                name: param.name.clone(),
                kind: crate::semantic::symbol_table::SymbolKind::变量,
                type_node: param_type,
                scope_level: self.symbol_table.current_scope(),
                span: param.span,
                is_mutable: false,
            };

            if let Err(scope_error) = self.symbol_table.define_symbol(param_symbol) {
                self.errors.push(TypeError::General {
                    message: format!("参数定义错误: {}", scope_error),
                    span: param.span,
                });
            }
        }

        // Type check function body statements
        for stmt in &func.body {
            if let Err(e) = self.check(stmt) {
                self.errors.push(e);
            }
        }

        // Exit function scope
        self.symbol_table.exit_scope();

        // Return function type
        Ok(func.return_type.clone().unwrap_or_else(|| TypeNode::基础类型(BasicType::空)))
    }

    fn check_array_access(&mut self, array_access: &crate::parser::ast::ArrayAccessExpression) -> Result<TypeNode, TypeError> {
        // Check array type
        let array_type = self.check(&array_access.array)?;

        // Check that it's an array type and extract element type
        let element_type = match array_type {
            TypeNode::数组类型(array_type) => *array_type.element_type,
            _ => {
                return Err(TypeError::InvalidOperation {
                    operation: "数组访问".to_string(),
                    type_name: format!("{:?}", array_type),
                    span: array_access.span,
                });
            }
        };

        // Check index type (should be integer)
        let index_type = self.check(&array_access.index)?;
        match index_type {
            TypeNode::基础类型(BasicType::整数) => {
                // Good, index is integer
            }
            _ => {
                return Err(TypeError::TypeMismatch {
                    expected: "整数".to_string(),
                    actual: format!("{:?}", index_type),
                    span: array_access.span,
                });
            }
        }

        Ok(element_type)
    }

    fn check_array_literal(&mut self, array_literal: &crate::parser::ast::ArrayLiteralExpression) -> Result<TypeNode, TypeError> {
        // Empty array: infer as empty integer array for now
        if array_literal.elements.is_empty() {
            return Ok(TypeNode::数组类型(crate::parser::ast::ArrayType {
                element_type: Box::new(TypeNode::基础类型(BasicType::整数)),
                size: Some(0),
            }));
        }

        // Get type of first element
        let first_type = self.check(&array_literal.elements[0])?;

        // Check that all elements have the same type
        for element in &array_literal.elements[1..] {
            let element_type = self.check(element)?;
            if element_type != first_type {
                return Err(TypeError::TypeMismatch {
                    expected: format!("{:?}", first_type),
                    actual: format!("{:?}", element_type),
                    span: array_literal.span, // Use array literal span as fallback
                });
            }
        }

        Ok(TypeNode::数组类型(crate::parser::ast::ArrayType {
            element_type: Box::new(first_type),
            size: Some(array_literal.elements.len()),
        }))
    }

    fn check_string_concat(&mut self, string_concat: &crate::parser::ast::StringConcatExpression) -> Result<TypeNode, TypeError> {
        let left_type = self.check(&string_concat.left)?;
        let right_type = self.check(&string_concat.right)?;

        // At least one operand should be string
        let is_left_string = matches!(left_type, TypeNode::基础类型(BasicType::字符串));
        let is_right_string = matches!(right_type, TypeNode::基础类型(BasicType::字符串));

        if !is_left_string && !is_right_string {
            return Err(TypeError::InvalidOperation {
                operation: "字符串连接".to_string(),
                type_name: format!("{:?} + {:?}", left_type, right_type),
                span: string_concat.span,
            });
        }

        // String concatenation always results in string type
        Ok(TypeNode::基础类型(BasicType::字符串))
    }

    fn check_struct_literal(&mut self, struct_literal: &crate::parser::ast::StructLiteralExpression) -> Result<TypeNode, TypeError> {
        // Look up struct type definition
        let struct_type = match self.symbol_table.lookup_symbol(&struct_literal.struct_name) {
            Some(symbol) => {
                match &symbol.type_node {
                    TypeNode::结构体类型(struct_type) => struct_type.clone(),
                    _ => {
                        return Err(TypeError::General {
                            message: format!("'{}' 不是一个结构体类型", struct_literal.struct_name),
                            span: struct_literal.span,
                        });
                    }
                }
            }
            None => {
                return Err(TypeError::General {
                    message: format!("未定义的结构体类型 '{}'", struct_literal.struct_name),
                    span: struct_literal.span,
                });
            }
        };

        // Check that all required fields are provided
        for field in &struct_type.fields {
            let provided = struct_literal.fields.iter()
                .any(|f| f.name == field.name);

            if !provided {
                return Err(TypeError::General {
                    message: format!("结构体 '{}' 缺少必填字段 '{}'", struct_literal.struct_name, field.name),
                    span: struct_literal.span,
                });
            }
        }

        // Check field types
        for provided_field in &struct_literal.fields {
            let expected_field = struct_type.fields.iter()
                .find(|f| f.name == provided_field.name);

            if let Some(expected_field) = expected_field {
                let provided_type = self.check(&provided_field.value)?;
                if provided_type != expected_field.type_annotation {
                    return Err(TypeError::TypeMismatch {
                        expected: format!("{:?}", expected_field.type_annotation),
                        actual: format!("{:?}", provided_type),
                        span: provided_field.span,
                    });
                }
            } else {
                return Err(TypeError::General {
                    message: format!("结构体 '{}' 没有字段 '{}'", struct_literal.struct_name, provided_field.name),
                    span: provided_field.span,
                });
            }
        }

        // Return the struct type
        Ok(TypeNode::结构体类型(struct_type))
    }

    fn check_field_access(&mut self, field_access: &crate::parser::ast::FieldAccessExpression) -> Result<TypeNode, TypeError> {
        // Check object type
        let object_type = self.check(&field_access.object)?;

        // Check that object is a struct type
        match object_type {
            TypeNode::结构体类型(struct_type) => {
                // Check that field exists
                if let Some(field) = struct_type.fields.iter().find(|f| f.name == field_access.field) {
                    // Return the field's type
                    Ok(field.type_annotation.clone())
                } else {
                    Err(TypeError::General {
                        message: format!("结构体 '{}' 没有字段 '{}'", struct_type.name, field_access.field),
                        span: field_access.span,
                    })
                }
            }
            _ => {
                Err(TypeError::InvalidOperation {
                    operation: "字段访问".to_string(),
                    type_name: format!("{:?}", object_type),
                    span: field_access.span,
                })
            }
        }
    }

    fn check_if_statement(&mut self, if_stmt: &crate::parser::ast::IfStatement) -> Result<TypeNode, TypeError> {
        // Check condition type
        let condition_type = self.check(&if_stmt.condition)?;
        match condition_type {
            TypeNode::基础类型(BasicType::布尔) => {
                // Good, condition is boolean
            }
            _ => {
                return Err(TypeError::TypeMismatch {
                    expected: "布尔".to_string(),
                    actual: format!("{:?}", condition_type),
                    span: if_stmt.span,
                });
            }
        }

        // Enter new scope for then branch
        self.symbol_table.enter_scope();
        for stmt in &if_stmt.then_branch {
            if let Err(e) = self.check(stmt) {
                self.errors.push(e);
            }
        }
        self.symbol_table.exit_scope();

        // Type check else branch if present
        if let Some(else_branch) = &if_stmt.else_branch {
            self.symbol_table.enter_scope();
            if let Err(e) = self.check(else_branch) {
                self.errors.push(e);
            }
            self.symbol_table.exit_scope();
        }
        Ok(TypeNode::基础类型(BasicType::空))
    }

    fn check_while_statement(&mut self, while_stmt: &crate::parser::ast::WhileStatement) -> Result<TypeNode, TypeError> {
        // Check condition type
        let condition_type = self.check(&while_stmt.condition)?;
        match condition_type {
            TypeNode::基础类型(BasicType::布尔) => {
                // Good, condition is boolean
            }
            _ => {
                return Err(TypeError::TypeMismatch {
                    expected: "布尔".to_string(),
                    actual: format!("{:?}", condition_type),
                    span: while_stmt.span,
                });
            }
        }

        // Enter new scope for loop body
        self.symbol_table.enter_scope();

        // Type check loop body
        for stmt in &while_stmt.body {
            if let Err(e) = self.check(stmt) {
                self.errors.push(e);
            }
        }

        // Exit loop scope
        self.symbol_table.exit_scope();

        // While statement doesn't produce a value
        Ok(TypeNode::基础类型(BasicType::空))
    }

    fn check_for_statement(&mut self, for_stmt: &crate::parser::ast::ForStatement) -> Result<TypeNode, TypeError> {
        // Check range type
        let range_type = self.check(&for_stmt.range)?;

        // Range should be array-like
        match range_type {
            TypeNode::数组类型(_) => {
                // Good, range is an array
            }
            _ => {
                return Err(TypeError::TypeMismatch {
                    expected: "数组".to_string(),
                    actual: format!("{:?}", range_type),
                    span: for_stmt.span,
                });
            }
        }

        // Enter new scope for loop body
        self.symbol_table.enter_scope();

        // Add loop variable to scope with array element type
        let element_type = match &range_type {
            TypeNode::数组类型(array_type) => *array_type.element_type.clone(),
            _ => TypeNode::基础类型(BasicType::空),
        };

        let loop_var_symbol = crate::semantic::symbol_table::Symbol {
            name: for_stmt.variable.clone(),
            kind: crate::semantic::symbol_table::SymbolKind::变量,
            type_node: element_type.clone(),
            scope_level: self.symbol_table.current_scope(),
            span: for_stmt.span,
            is_mutable: false,
        };

        if let Err(scope_error) = self.symbol_table.define_symbol(loop_var_symbol) {
            self.errors.push(TypeError::General {
                message: format!("循环变量定义错误: {}", scope_error),
                span: for_stmt.span,
            });
        }

        // Type check loop body
        for stmt in &for_stmt.body {
            if let Err(e) = self.check(stmt) {
                self.errors.push(e);
            }
        }

        // Exit loop scope
        self.symbol_table.exit_scope();

        // For statement doesn't produce a value
        Ok(TypeNode::基础类型(BasicType::空))
    }

    fn check_return_statement(&mut self, return_stmt: &crate::parser::ast::ReturnStatement) -> Result<TypeNode, TypeError> {
        // Type check return value if present
        if let Some(value) = &return_stmt.value {
            let value_type = self.check(value)?;
            // TODO: Check that return type matches function's declared return type
            Ok(value_type)
        } else {
            // Return without value returns 空 type
            Ok(TypeNode::基础类型(BasicType::空))
        }
    }

    fn check_expression_statement(&mut self, expr_stmt: &crate::parser::ast::ExpressionStatement) -> Result<TypeNode, TypeError> {
        // Type check the expression
        self.check(&expr_stmt.expression)
    }

    fn check_block_statement(&mut self, block_stmt: &crate::parser::ast::BlockStatement) -> Result<TypeNode, TypeError> {
        // Enter new scope for block
        self.symbol_table.enter_scope();

        // Type check all statements in block
        for stmt in &block_stmt.statements {
            if let Err(e) = self.check(stmt) {
                self.errors.push(e);
            }
        }

        // Exit block scope
        self.symbol_table.exit_scope();

        // Block statement doesn't produce a value
        Ok(TypeNode::基础类型(BasicType::空))
    }

    fn check_print_statement(&mut self, print_stmt: &crate::parser::ast::PrintStatement) -> Result<TypeNode, TypeError> {
        // Type check the expression to be printed
        self.check(&print_stmt.value)?;

        // Print statement doesn't produce a value
        Ok(TypeNode::基础类型(BasicType::空))
    }

    fn check_program(&mut self, program: &crate::parser::ast::Program) -> Result<TypeNode, TypeError> {
        // Enter global scope
        self.symbol_table.enter_scope();

        // Type check all statements
        for stmt in &program.statements {
            if let Err(e) = self.check(stmt) {
                self.errors.push(e);
            }
        }

        // Exit global scope
        self.symbol_table.exit_scope();

        Ok(TypeNode::基础类型(BasicType::空))
    }

    fn check_struct_declaration(&mut self, struct_decl: &crate::parser::ast::StructDeclaration) -> Result<TypeNode, TypeError> {
        // Create struct type
        let struct_type = TypeNode::结构体类型(crate::parser::ast::StructType {
            name: struct_decl.name.clone(),
            fields: struct_decl.fields.clone(),
        });

        // Create type info for struct
        let type_info = crate::semantic::symbol_table::TypeInfo {
            definition: crate::semantic::symbol_table::TypeDefinition {},
            is_builtin: false,
        };

        // Register struct type in symbol table
        let struct_symbol = crate::semantic::symbol_table::Symbol {
            name: struct_decl.name.clone(),
            kind: crate::semantic::symbol_table::SymbolKind::类型(type_info),
            type_node: struct_type.clone(),
            scope_level: self.symbol_table.current_scope(),
            span: struct_decl.span,
            is_mutable: false,
        };

        if let Err(scope_error) = self.symbol_table.define_symbol(struct_symbol) {
            return Err(TypeError::General {
                message: format!("结构体定义错误: {}", scope_error),
                span: struct_decl.span,
            });
        }

        Ok(struct_type)
    }

    fn check_enum_declaration(&mut self, enum_decl: &crate::parser::ast::EnumDeclaration) -> Result<TypeNode, TypeError> {
        // Create enum type
        let enum_type = TypeNode::枚举类型(crate::parser::ast::EnumType {
            name: enum_decl.name.clone(),
            variants: enum_decl.variants.clone(),
        });

        // Create type info for enum
        let type_info = crate::semantic::symbol_table::TypeInfo {
            definition: crate::semantic::symbol_table::TypeDefinition {},
            is_builtin: false,
        };

        // Register enum type in symbol table
        let enum_symbol = crate::semantic::symbol_table::Symbol {
            name: enum_decl.name.clone(),
            kind: crate::semantic::symbol_table::SymbolKind::类型(type_info),
            type_node: enum_type.clone(),
            scope_level: self.symbol_table.current_scope(),
            span: enum_decl.span,
            is_mutable: false,
        };

        if let Err(scope_error) = self.symbol_table.define_symbol(enum_symbol) {
            return Err(TypeError::General {
                message: format!("枚举定义错误: {}", scope_error),
                span: enum_decl.span,
            });
        }

        Ok(enum_type)
    }


    fn check_loop_statement(&mut self, loop_stmt: &crate::parser::ast::LoopStatement) -> Result<TypeNode, TypeError> {
        // Enter new scope for loop body
        self.symbol_table.enter_scope();

        // Type check loop body
        for stmt in &loop_stmt.body {
            if let Err(e) = self.check(stmt) {
                self.errors.push(e);
            }
        }

        // Exit loop scope
        self.symbol_table.exit_scope();

        // Loop statement doesn't produce a value
        Ok(TypeNode::基础类型(BasicType::空))
    }

    pub fn get_errors(&self) -> &[TypeError] {
        &self.errors
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

