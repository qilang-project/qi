//! Type checking and semantic analysis for Qi language
//! 类型检查和语义分析

pub mod scope;
pub mod symbol_table;
pub mod type_checker;
pub mod module;
pub mod methods;

pub use symbol_table::SymbolTable;
pub use type_checker::TypeChecker;
use crate::parser::AstNode;
use crate::utils::diagnostics::DiagnosticManager;
use crate::lexer::Span;

/// Semantic analyzer
/// 语义分析器
#[allow(dead_code)]
pub struct SemanticAnalyzer {
    type_checker: TypeChecker,
    method_system: methods::MethodSystem,
    diagnostics: DiagnosticManager,
}

impl SemanticAnalyzer {
    /// Create a new semantic analyzer
    /// 创建新的语义分析器
    pub fn new() -> Self {
        let type_checker = TypeChecker::new();
        Self {
            type_checker,
            method_system: methods::MethodSystem::new(),
            diagnostics: DiagnosticManager::new(),
        }
    }

    /// Get a reference to the diagnostics manager
    /// 获取诊断管理器的引用
    pub fn diagnostics(&self) -> &DiagnosticManager {
        &self.diagnostics
    }

    /// Consume the analyzer and return diagnostics
    /// 消费分析器并返回诊断信息
    pub fn into_diagnostics(self) -> DiagnosticManager {
        self.diagnostics
    }

    
    /// Analyze the AST for semantic correctness
    pub fn analyze(&mut self, ast: &crate::parser::ast::AstNode) -> Result<(), SemanticError> {
        // Use the type checker's symbol table as the single source of truth
        // Enter global scope
        self.type_checker.symbol_table.enter_scope();

        // Analyze the AST
        self.analyze_node(ast)?;

        // Exit global scope
        self.type_checker.symbol_table.exit_scope();

        // Check for errors
        if !self.type_checker.symbol_table.get_errors().is_empty() {
            return Err(SemanticError::ScopeError(
                format!("作用域错误: {:?}", self.type_checker.symbol_table.get_errors())
            ));
        }

        if !self.type_checker.get_errors().is_empty() {
            return Err(SemanticError::TypeMismatch(
                "类型错误".to_string(),
                format!("{:?}", self.type_checker.get_errors())
            ));
        }

        Ok(())
    }

    /// Analyze a single AST node
    fn analyze_node(&mut self, node: &crate::parser::ast::AstNode) -> Result<(), SemanticError> {
        match node {
            AstNode::程序(program) => {
                // Analyze all statements in the program
                for statement in &program.statements {
                    self.analyze_node(statement)?;
                }
                Ok(())
            }
            AstNode::变量声明(decl) => self.analyze_variable_declaration(decl),
            AstNode::函数声明(func) => self.analyze_function_declaration(func),
            AstNode::结构体声明(struct_decl) => self.analyze_struct_declaration(struct_decl),
            AstNode::方法声明(method_decl) => self.analyze_method_declaration(method_decl),
            AstNode::枚举声明(enum_decl) => self.analyze_enum_declaration(enum_decl),
            AstNode::如果语句(if_stmt) => self.analyze_if_statement(if_stmt),
            AstNode::当语句(while_stmt) => self.analyze_while_statement(while_stmt),
            AstNode::循环语句(loop_stmt) => self.analyze_loop_statement(loop_stmt),
            AstNode::对于语句(for_stmt) => self.analyze_for_statement(for_stmt),
            AstNode::返回语句(return_stmt) => self.analyze_return_statement(return_stmt),
            AstNode::表达式语句(expr_stmt) => self.analyze_expression_statement(expr_stmt),
            AstNode::函数调用表达式(call_expr) => self.analyze_function_call(call_expr),
            AstNode::结构体实例化表达式(struct_literal) => self.analyze_struct_literal(struct_literal),
            AstNode::字段访问表达式(field_access) => self.analyze_field_access(field_access),
            AstNode::数组访问表达式(array_access) => self.analyze_array_access(array_access),
            AstNode::数组字面量表达式(array_literal) => self.analyze_array_literal(array_literal),
            AstNode::字符串连接表达式(string_concat) => self.analyze_string_concat(string_concat),
            AstNode::块语句(block_stmt) => self.analyze_block_statement(block_stmt),
            _ => {
                // For expressions, run type checking
                self.type_checker.check(node)
                    .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;
                Ok(())
            }
        }
    }

    /// Analyze variable declaration
    fn analyze_variable_declaration(&mut self, decl: &crate::parser::ast::VariableDeclaration) -> Result<(), SemanticError> {
        // The type checker already handles variable declaration and symbol table management
        let result = self.type_checker.check_variable_declaration(decl)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        // Just verify the result
        let _ = result;
        Ok(())
    }

    /// Analyze function declaration
    fn analyze_function_declaration(&mut self, func: &crate::parser::ast::FunctionDeclaration) -> Result<(), SemanticError> {
        // The type checker already handles function declarations properly
        let result = self.type_checker.check_function_declaration(func)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;
        let _ = result;

        Ok(())
    }

    /// Analyze if statement
    fn analyze_if_statement(&mut self, if_stmt: &crate::parser::ast::IfStatement) -> Result<(), SemanticError> {
        // Check condition type (should be boolean)
        let condition_type = self.type_checker.check(&if_stmt.condition)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        if !self.is_boolean_type(&condition_type) {
            return Err(SemanticError::TypeMismatch(
                "条件必须是布尔类型".to_string(),
                format!("{:?}", condition_type)
            ));
        }

        // Enter then branch scope
        self.type_checker.symbol_table.enter_scope();

        // Analyze then branch
        for statement in &if_stmt.then_branch {
            self.analyze_node(statement)?;
        }

        // Exit then branch scope
        self.type_checker.symbol_table.exit_scope();

        // Analyze else branch if exists
        if let Some(else_branch) = &if_stmt.else_branch {
            // Enter else branch scope
            self.type_checker.symbol_table.enter_scope();

            self.analyze_node(else_branch)?;

            // Exit else branch scope
            self.type_checker.symbol_table.exit_scope();
        }

        Ok(())
    }

    /// Analyze while statement
    fn analyze_while_statement(&mut self, while_stmt: &crate::parser::ast::WhileStatement) -> Result<(), SemanticError> {
        // Check condition type (should be boolean)
        let condition_type = self.type_checker.check(&while_stmt.condition)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        if !self.is_boolean_type(&condition_type) {
            return Err(SemanticError::TypeMismatch(
                "循环条件必须是布尔类型".to_string(),
                format!("{:?}", condition_type)
            ));
        }

        // Enter loop body scope
        self.type_checker.symbol_table.enter_scope();

        // Analyze loop body
        for statement in &while_stmt.body {
            self.analyze_node(statement)?;
        }

        // Exit loop body scope
        self.type_checker.symbol_table.exit_scope();

        Ok(())
    }

    /// Analyze loop statement
    fn analyze_loop_statement(&mut self, loop_stmt: &crate::parser::ast::LoopStatement) -> Result<(), SemanticError> {
        // Enter loop body scope
        self.type_checker.symbol_table.enter_scope();

        // Analyze loop body
        for statement in &loop_stmt.body {
            self.analyze_node(statement)?;
        }

        // Exit loop body scope
        self.type_checker.symbol_table.exit_scope();

        Ok(())
    }

    /// Analyze block statement
    fn analyze_block_statement(&mut self, block_stmt: &crate::parser::ast::BlockStatement) -> Result<(), SemanticError> {
        // Enter block scope
        self.type_checker.symbol_table.enter_scope();

        // Analyze all statements in block
        for statement in &block_stmt.statements {
            self.analyze_node(statement)?;
        }

        // Exit block scope
        self.type_checker.symbol_table.exit_scope();

        Ok(())
    }

    /// Analyze for statement
    fn analyze_for_statement(&mut self, for_stmt: &crate::parser::ast::ForStatement) -> Result<(), SemanticError> {
        // Check range type
        let range_type = self.type_checker.check(&for_stmt.range)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        // Range should be array-like
        match range_type {
            crate::parser::ast::TypeNode::数组类型(_) => {
                // Good, range is an array
            }
            _ => {
                return Err(SemanticError::TypeMismatch(
                    "对于循环期望数组类型".to_string(),
                    format!("{:?}", range_type)
                ));
            }
        }

        // Enter loop body scope
        self.type_checker.symbol_table.enter_scope();

        // Add loop variable to scope with array element type
        let element_type = match &range_type {
            crate::parser::ast::TypeNode::数组类型(array_type) => *array_type.element_type.clone(),
            _ => crate::parser::ast::TypeNode::基础类型(crate::parser::ast::BasicType::空),
        };

        let loop_var_symbol = crate::semantic::symbol_table::Symbol {
            name: for_stmt.variable.clone(),
            kind: crate::semantic::symbol_table::SymbolKind::变量,
            type_node: element_type.clone(),
            scope_level: self.type_checker.symbol_table.current_scope(),
            span: for_stmt.span,
            is_mutable: false,
        };

        self.type_checker.symbol_table.define_symbol(loop_var_symbol)
            .map_err(|e| SemanticError::ScopeError(e.to_string()))?;

        // Analyze loop body
        for statement in &for_stmt.body {
            self.analyze_node(statement)?;
        }

        // Exit loop body scope
        self.type_checker.symbol_table.exit_scope();


        Ok(())
    }

    /// Analyze return statement
    fn analyze_return_statement(&mut self, return_stmt: &crate::parser::ast::ReturnStatement) -> Result<(), SemanticError> {
        if let Some(value) = &return_stmt.value {
            // Check return value type
            self.type_checker.check(value)
                .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;
        }
        Ok(())
    }

    /// Analyze expression statement
    fn analyze_expression_statement(&mut self, expr_stmt: &crate::parser::ast::ExpressionStatement) -> Result<(), SemanticError> {
        self.type_checker.check(&expr_stmt.expression)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;
        Ok(())
    }

    /// Analyze function call expression
    fn analyze_function_call(&mut self, call_expr: &crate::parser::ast::FunctionCallExpression) -> Result<(), SemanticError> {
        // Check that the function exists
        let symbol = match self.type_checker.symbol_table.lookup_symbol(&call_expr.callee) {
            Some(symbol) => symbol.clone(),
            None => {
                return Err(SemanticError::FunctionCallError(
                    format!("未定义的函数 '{}'", call_expr.callee)
                ));
            }
        };

        // Extract function info if it's a function
        let func_info = match &symbol.kind {
            crate::semantic::symbol_table::SymbolKind::函数(func_info) => func_info.clone(),
            _ => {
                return Err(SemanticError::FunctionCallError(
                    format!("'{}' 不是一个函数", call_expr.callee)
                ));
            }
        };

        // Check argument count
        if call_expr.arguments.len() != func_info.parameters.len() {
            return Err(SemanticError::FunctionCallError(
                format!("函数 '{}' 参数数量不匹配: 期望 {}, 实际 {}",
                    call_expr.callee,
                    func_info.parameters.len(),
                    call_expr.arguments.len())
            ));
        }

        // Check argument types
        for (i, arg) in call_expr.arguments.iter().enumerate() {
            let arg_type = self.type_checker.check(arg)
                .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

            let expected_type = &func_info.parameters[i].type_annotation;
            if let Some(expected) = expected_type {
                if &arg_type != expected {
                    return Err(SemanticError::TypeMismatch(
                        format!("参数 {} 类型不匹配: 期望 {:?}, 实际 {:?}",
                            i + 1, expected, arg_type),
                        "".to_string()
                    ));
                }
            }
        }

        // Return type checking is handled by the expression
        self.type_checker.check_function_call(call_expr)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        Ok(())
    }

    /// Check if a type is boolean
    fn is_boolean_type(&self, type_node: &crate::parser::ast::TypeNode) -> bool {
        matches!(type_node, crate::parser::ast::TypeNode::基础类型(crate::parser::ast::BasicType::布尔))
    }

    /// Analyze array access expression: array[index]
    fn analyze_array_access(&mut self, array_access: &crate::parser::ast::ArrayAccessExpression) -> Result<(), SemanticError> {
        // Check array expression
        let array_type = self.type_checker.check(&array_access.array)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        // Check that array is actually an array type
        match array_type {
            crate::parser::ast::TypeNode::数组类型(_) => {
                // Good, it's an array
            }
            _ => {
                return Err(SemanticError::TypeMismatch(
                    format!("期望数组类型，实际 {:?}", array_type),
                    "".to_string()
                ));
            }
        }

        // Check index expression (should be integer)
        let index_type = self.type_checker.check(&array_access.index)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        if !self.is_integer_type(&index_type) {
            return Err(SemanticError::TypeMismatch(
                "数组索引必须是整数类型".to_string(),
                format!("{:?}", index_type)
            ));
        }

        Ok(())
    }

    /// Analyze array literal expression: [1, 2, 3]
    fn analyze_array_literal(&mut self, array_literal: &crate::parser::ast::ArrayLiteralExpression) -> Result<(), SemanticError> {
        // Empty array is allowed
        if array_literal.elements.is_empty() {
            return Ok(());
        }

        // Get the type of the first element
        let first_type = self.type_checker.check(&array_literal.elements[0])
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        // Check that all elements have the same type
        for (i, element) in array_literal.elements.iter().enumerate() {
            let element_type = self.type_checker.check(element)
                .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

            if element_type != first_type {
                return Err(SemanticError::TypeMismatch(
                    format!("数组元素类型不匹配: 元素 {} 类型 {:?} 与第一个元素类型 {:?} 不匹配",
                        i + 1, element_type, first_type),
                    "".to_string()
                ));
            }
        }

        Ok(())
    }

    /// Analyze string concatenation expression: "hello" + " world"
    fn analyze_string_concat(&mut self, string_concat: &crate::parser::ast::StringConcatExpression) -> Result<(), SemanticError> {
        // Check left expression
        let left_type = self.type_checker.check(&string_concat.left)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        // Check right expression
        let right_type = self.type_checker.check(&string_concat.right)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        // At least one operand should be a string
        if !self.is_string_type(&left_type) && !self.is_string_type(&right_type) {
            return Err(SemanticError::TypeMismatch(
                "字符串连接至少需要一个操作数是字符串类型".to_string(),
                format!("左操作数类型: {:?}, 右操作数类型: {:?}", left_type, right_type)
            ));
        }

        Ok(())
    }

    /// Check if a type is integer
    fn is_integer_type(&self, type_node: &crate::parser::ast::TypeNode) -> bool {
        matches!(type_node, crate::parser::ast::TypeNode::基础类型(crate::parser::ast::BasicType::整数))
    }

    /// Check if a type is string
    fn is_string_type(&self, type_node: &crate::parser::ast::TypeNode) -> bool {
        matches!(type_node, crate::parser::ast::TypeNode::基础类型(crate::parser::ast::BasicType::字符串))
    }

    /// Analyze struct declaration
    fn analyze_struct_declaration(&mut self, struct_decl: &crate::parser::ast::StructDeclaration) -> Result<(), SemanticError> {
        // Create struct type information
        let struct_type = crate::parser::ast::StructType {
            name: struct_decl.name.clone(),
            fields: struct_decl.fields.clone(),
            methods: vec![], // 方法在方法声明时添加
        };

        // Add struct type to symbol table
        let symbol = crate::semantic::symbol_table::Symbol {
            name: struct_decl.name.clone(),
            kind: crate::semantic::symbol_table::SymbolKind::类型(
                crate::semantic::symbol_table::TypeInfo {
                    definition: crate::semantic::symbol_table::TypeDefinition {},
                    is_builtin: false,
                }
            ),
            type_node: crate::parser::ast::TypeNode::结构体类型(struct_type),
            scope_level: self.type_checker.symbol_table.current_scope(),
            span: struct_decl.span,
            is_mutable: false,
        };

        self.type_checker.symbol_table.define_symbol(symbol.clone())
            .map_err(|e| SemanticError::ScopeError(e.to_string()))?;

        Ok(())
    }

    /// Analyze method declaration
    fn analyze_method_declaration(&mut self, method_decl: &crate::parser::ast::MethodDeclaration) -> Result<(), SemanticError> {
        // Register the method in the method system
        // Note: The receiver type should be determined from the context or explicit type annotation
        // For now, we'll use a placeholder approach
        let receiver_type_name = method_decl.receiver_type.clone(); // This would need to be resolved

        self.method_system.register_method(receiver_type_name, method_decl.clone())
            .map_err(|e| SemanticError::ScopeError(e.to_string()))?;

        Ok(())
    }

    /// Analyze enum declaration
    fn analyze_enum_declaration(&mut self, enum_decl: &crate::parser::ast::EnumDeclaration) -> Result<(), SemanticError> {
        // Create enum type information
        let enum_type = crate::parser::ast::EnumType {
            name: enum_decl.name.clone(),
            variants: enum_decl.variants.clone(),
        };

        // Add enum type to symbol table
        let symbol = crate::semantic::symbol_table::Symbol {
            name: enum_decl.name.clone(),
            kind: crate::semantic::symbol_table::SymbolKind::类型(
                crate::semantic::symbol_table::TypeInfo {
                    definition: crate::semantic::symbol_table::TypeDefinition {},
                    is_builtin: false,
                }
            ),
            type_node: crate::parser::ast::TypeNode::枚举类型(enum_type),
            scope_level: self.type_checker.symbol_table.current_scope(),
            span: enum_decl.span,
            is_mutable: false,
        };

        self.type_checker.symbol_table.define_symbol(symbol.clone())
            .map_err(|e| SemanticError::ScopeError(e.to_string()))?;

        Ok(())
    }

    /// Analyze struct literal expression
    fn analyze_struct_literal(&mut self, struct_literal: &crate::parser::ast::StructLiteralExpression) -> Result<(), SemanticError> {
        // Check that the struct type exists
        let symbol = match self.type_checker.symbol_table.lookup_symbol(&struct_literal.struct_name) {
            Some(symbol) => symbol.clone(),
            None => {
                return Err(SemanticError::TypeMismatch(
                    format!("未定义的结构体类型 '{}'", struct_literal.struct_name),
                    "".to_string()
                ));
            }
        };

        // Extract struct type if it's a struct
        let struct_type = match &symbol.type_node {
            crate::parser::ast::TypeNode::结构体类型(struct_type) => struct_type.clone(),
            _ => {
                return Err(SemanticError::TypeMismatch(
                    format!("'{}' 不是一个结构体类型", struct_literal.struct_name),
                    "".to_string()
                ));
            }
        };

        // Check that all required fields are provided
        for field in &struct_type.fields {
            let provided = struct_literal.fields.iter()
                .any(|f| f.name == field.name);

            if !provided {
                return Err(SemanticError::TypeMismatch(
                    format!("结构体 '{}' 缺少必填字段 '{}'", struct_literal.struct_name, field.name),
                    "".to_string()
                ));
            }
        }

        // Check field types
        for provided_field in &struct_literal.fields {
            let expected_field = struct_type.fields.iter()
                .find(|f| f.name == provided_field.name);

            if let Some(expected_field) = expected_field {
                let provided_type = self.type_checker.check(&provided_field.value)
                    .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

                if provided_type != expected_field.type_annotation {
                    return Err(SemanticError::TypeMismatch(
                        format!("字段 '{}' 类型不匹配: 期望 {:?}, 实际 {:?}",
                            provided_field.name, expected_field.type_annotation, provided_type),
                        "".to_string()
                    ));
                }
            } else {
                return Err(SemanticError::TypeMismatch(
                    format!("结构体 '{}' 没有字段 '{}'", struct_literal.struct_name, provided_field.name),
                    "".to_string()
                ));
            }
        }

        Ok(())
    }

    /// Analyze field access expression
    fn analyze_field_access(&mut self, field_access: &crate::parser::ast::FieldAccessExpression) -> Result<(), SemanticError> {
        // Check object type
        let object_type = self.type_checker.check(&field_access.object)
            .map_err(|e| SemanticError::TypeMismatch(e.to_string(), "".to_string()))?;

        // Check that object is a struct type
        match object_type {
            crate::parser::ast::TypeNode::结构体类型(struct_type) => {
                // Check that field exists
                let field_exists = struct_type.fields.iter()
                    .any(|f| f.name == field_access.field);

                if !field_exists {
                    return Err(SemanticError::TypeMismatch(
                        format!("结构体 '{}' 没有字段 '{}'", struct_type.name, field_access.field),
                        "".to_string()
                    ));
                }

                // Field access is valid, return field type
                Ok(())
            }
            _ => {
                return Err(SemanticError::TypeMismatch(
                    format!("期望结构体类型，实际 {:?}", object_type),
                    "".to_string()
                ));
            }
        }
    }

    // ===== Enhanced Error Reporting Methods | 增强错误报告方法 =====

    /// Report undefined variable error with detailed context
    /// 报告未定义变量错误及详细上下文
    #[allow(dead_code)]
    fn report_undefined_variable_error(&mut self, var_name: &str, span: Span) {
        let suggestion = format!("检查变量名 '{}' 是否正确拼写，或者在使用前先声明变量", var_name);
        self.diagnostics.undefined_variable_error(span, var_name, Some(&suggestion));
    }

    /// Report type mismatch error with detailed suggestions
    /// 报告类型不匹配错误及详细建议
    #[allow(dead_code)]
    fn report_type_mismatch_error(&mut self, expected: &str, found: &str, span: Span, context: &str) {
        let suggestion = match context {
            "assignment" => format!("确保赋值的表达式类型与变量声明类型 '{}' 匹配", expected),
            "function_call" => format!("检查函数调用参数类型，期望 '{}', 实际 '{}'", expected, found),
            "return" => format!("确保返回值类型与函数声明类型 '{}' 匹配", expected),
            "operation" => format!("检查操作数类型是否支持此操作，期望 '{}', 实际 '{}'", expected, found),
            _ => format!("类型不匹配，期望 '{}', 实际 '{}', 请检查类型转换", expected, found),
        };

        self.diagnostics.type_mismatch_error(span, expected, found, Some(&suggestion));
    }

    /// Report function call error with detailed suggestions
    /// 报告函数调用错误及详细建议
    #[allow(dead_code)]
    fn report_function_call_error(&mut self, func_name: &str, error_type: &str, span: Span, details: &str) {
        let message = format!("函数调用错误: {} - {}", func_name, details);
        let suggestion = match error_type {
            "undefined" => format!("检查函数名 '{}' 是否正确，或者先定义函数", func_name),
            "wrong_type" => format!("'{}' 不是一个函数，检查标识符类型", func_name),
            "arity_mismatch" => format!("检查函数 '{}' 的参数数量是否正确", func_name),
            "parameter_type" => format!("检查函数 '{}' 的参数类型是否匹配", func_name),
            _ => format!("检查函数 '{}' 的调用语法是否正确", func_name),
        };

        self.diagnostics.function_call_error(span, &message, Some(&suggestion));
    }

    /// Report array access error with detailed suggestions
    /// 报告数组访问错误及详细建议
    #[allow(dead_code)]
    fn report_array_access_error(&mut self, error_type: &str, span: Span, details: &str) {
        let message = format!("数组访问错误: {}", details);
        let suggestion = match error_type {
            "not_array" => "确保访问的对象是数组类型".to_string(),
            "invalid_index" => "数组索引必须是整数类型".to_string(),
            "out_of_bounds" => "检查数组索引是否在有效范围内".to_string(),
            _ => "检查数组访问语法是否正确".to_string(),
        };

        self.diagnostics.array_access_error(span, &message, Some(&suggestion));
    }

    /// Report struct field error with detailed suggestions
    /// 报告结构体字段错误及详细建议
    #[allow(dead_code)]
    fn report_struct_field_error(&mut self, struct_name: &str, field_name: &str, span: Span, error_type: &str) {
        let _message = match error_type {
            "field_not_found" => format!("结构体 '{}' 没有字段 '{}'", struct_name, field_name),
            "not_struct" => format!("'{}' 不是一个结构体类型", struct_name),
            _ => format!("结构体字段访问错误: {}.{}", struct_name, field_name),
        };

        let suggestion = match error_type {
            "field_not_found" => format!("检查结构体 '{}' 的字段名称是否正确", struct_name),
            "not_struct" => "确保访问的对象是结构体类型".to_string(),
            _ => "检查结构体字段访问语法".to_string(),
        };

        self.diagnostics.struct_field_error(span, struct_name, field_name, Some(&suggestion));
    }

    /// Report invalid operation error with detailed suggestions
    /// 报告无效操作错误及详细建议
    #[allow(dead_code)]
    fn report_invalid_operation_error(&mut self, operation: &str, type_name: &str, span: Span) {
        let _message = format!("无效操作: '{}' 对于类型 '{}'", operation, type_name);
        let suggestion = format!("检查类型 '{}' 是否支持操作 '{}'", type_name, operation);

        self.diagnostics.invalid_operation_error(span, operation, type_name, Some(&suggestion));
    }

    /// Report variable redeclaration error
    /// 报告变量重复声明错误
    #[allow(dead_code)]
    fn report_variable_redeclaration_error(&mut self, var_name: &str, span: Span, original_span: Span) {
        self.diagnostics.variable_redeclaration_error(span, var_name, original_span);
    }

    /// Report constant reassignment error
    /// 报告常量重新赋值错误
    #[allow(dead_code)]
    fn report_constant_reassignment_error(&mut self, const_name: &str, span: Span) {
        self.diagnostics.constant_reassignment_error(span, const_name);
    }

    /// Report non-boolean condition error
    /// 报告非布尔条件错误
    #[allow(dead_code)]
    fn report_non_boolean_condition_error(&mut self, condition_type: &str, span: Span, context: &str) {
        let context_str = match context {
            "if" => "如果语句",
            "while" => "当循环",
            "for" => "对于循环",
            _ => "条件语句",
        };

        self.diagnostics.non_boolean_condition_error(span, condition_type, context_str);
    }

    /// Get error summary statistics
    /// 获取错误摘要统计
    pub fn get_error_summary(&self) -> (usize, usize) {
        (self.diagnostics.error_count(), self.diagnostics.warning_count())
    }

    /// Check if any critical errors occurred
    /// 检查是否发生了关键错误
    pub fn has_critical_errors(&self) -> bool {
        self.diagnostics.error_count() > 0
    }

    /// Format all diagnostics as Chinese messages
    /// 将所有诊断信息格式化为中文消息
    pub fn format_diagnostics(&self) -> String {
        self.diagnostics.format_chinese_messages()
    }

    /// Add warning for unused variable
    /// 添加未使用变量警告
    pub fn add_unused_variable_warning(&mut self, var_name: &str, span: Span) {
        self.diagnostics.unused_variable_warning(span, var_name);
    }

    /// Add warning for unreachable code
    /// 添加不可达代码警告
    pub fn add_unreachable_code_warning(&mut self, span: Span) {
        self.diagnostics.unreachable_code_warning(span);
    }

    /// Add warning for dead code
    /// 添加死代码警告
    pub fn add_dead_code_warning(&mut self, span: Span) {
        self.diagnostics.dead_code_warning(span);
    }
}

/// Semantic analysis errors
/// 语义分析错误
#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    /// Undeclared variable
    #[error("未声明的变量: {0}")]
    UndeclaredVariable(String),

    /// Type mismatch
    #[error("类型不匹配: 期望 {0}, 实际 {1}")]
    TypeMismatch(String, String),

    /// Function call error
    #[error("函数调用错误: {0}")]
    FunctionCallError(String),

    /// Scope error
    #[error("作用域错误: {0}")]
    ScopeError(String),
}