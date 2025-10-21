//! Diagnostic and error reporting for Qi language

use std::path::PathBuf;

/// Diagnostic level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    错误,    // Error
    警告,    // Warning
    信息,    // Info
}

/// Diagnostic message
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: String,
    pub message: String,
    pub english_message: String,
    pub file_path: Option<PathBuf>,
    pub span: Option<crate::lexer::Span>,
    pub suggestion: Option<String>,
    pub related_code: Option<String>,
}

/// Diagnostic manager
pub struct DiagnosticManager {
    diagnostics: Vec<Diagnostic>,
    max_errors: usize,
    max_warnings: usize,
}

impl DiagnosticManager {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            max_errors: 100,
            max_warnings: 100,
        }
    }

    /// Add syntax error with code and suggestion
    pub fn syntax_error(&mut self, span: crate::lexer::Span, expected: &str, found: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E001".to_string(),
            message: format!("语法错误: 期望 '{}', 找到 '{}'", expected, found),
            english_message: format!("Syntax error: expected '{}', found '{}'", expected, found),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add type mismatch error with suggestion
    pub fn type_mismatch_error(&mut self, span: crate::lexer::Span, expected: &str, found: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E002".to_string(),
            message: format!("类型不匹配: 期望 '{}', 实际 '{}'", expected, found),
            english_message: format!("Type mismatch: expected '{}', found '{}'", expected, found),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add undefined variable error with suggestion
    pub fn undefined_variable_error(&mut self, span: crate::lexer::Span, var_name: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E003".to_string(),
            message: format!("未定义的变量: '{}'", var_name),
            english_message: format!("Undefined variable: '{}'", var_name),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add function call error with suggestion
    pub fn function_call_error(&mut self, span: crate::lexer::Span, message: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E004".to_string(),
            message: format!("函数调用错误: {}", message),
            english_message: format!("Function call error: {}", message),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add invalid operation error with suggestion
    pub fn invalid_operation_error(&mut self, span: crate::lexer::Span, operation: &str, type_name: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E005".to_string(),
            message: format!("无效操作: '{}' 对于类型 '{}'", operation, type_name),
            english_message: format!("Invalid operation: '{}' for type '{}'", operation, type_name),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add struct field error with suggestion
    pub fn struct_field_error(&mut self, span: crate::lexer::Span, struct_name: &str, field_name: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E006".to_string(),
            message: format!("结构体 '{}' 没有字段 '{}'", struct_name, field_name),
            english_message: format!("Struct '{}' has no field '{}'", struct_name, field_name),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add array access error with suggestion
    pub fn array_access_error(&mut self, span: crate::lexer::Span, message: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E007".to_string(),
            message: format!("数组访问错误: {}", message),
            english_message: format!("Array access error: {}", message),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add general warning with suggestion
    pub fn warning(&mut self, code: &str, message: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: code.to_string(),
            message: message.to_string(),
            english_message: message.to_string(),
            file_path: None,
            span: None,
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add unused variable warning
    pub fn unused_variable_warning(&mut self, span: crate::lexer::Span, var_name: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: "W001".to_string(),
            message: format!("未使用的变量: '{}'", var_name),
            english_message: format!("Unused variable: '{}'", var_name),
            file_path: None,
            span: Some(span),
            suggestion: Some("如果不需要此变量，请考虑删除它或在变量名前添加下划线前缀".to_string()),
            related_code: None,
        });
    }

    /// Add unreachable code warning
    pub fn unreachable_code_warning(&mut self, span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: "W002".to_string(),
            message: "不可达的代码".to_string(),
            english_message: "Unreachable code".to_string(),
            file_path: None,
            span: Some(span),
            suggestion: Some("请删除这段不可达的代码".to_string()),
            related_code: None,
        });
    }

    // ===== 语法错误 | Syntax Errors =====

    /// Add missing semicolon error
    pub fn missing_semicolon_error(&mut self, span: crate::lexer::Span, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E008".to_string(),
            message: "缺少分号".to_string(),
            english_message: "Missing semicolon".to_string(),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add missing closing brace error
    pub fn missing_closing_brace_error(&mut self, span: crate::lexer::Span, expected_brace: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E009".to_string(),
            message: format!("缺少闭合的 '{}'", expected_brace),
            english_message: format!("Missing closing '{}'", expected_brace),
            file_path: None,
            span: Some(span),
            suggestion: Some(format!("在代码末尾添加 '{}'", expected_brace)),
            related_code: None,
        });
    }

    /// Add unmatched brackets error
    pub fn unmatched_brackets_error(&mut self, span: crate::lexer::Span, opening: char, closing: char) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E010".to_string(),
            message: format!("括号不匹配: '{}' 与 '{}' 不匹配", opening, closing),
            english_message: format!("Unmatched brackets: '{}' does not match '{}'", opening, closing),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查括号是否正确配对".to_string()),
            related_code: None,
        });
    }

    /// Add invalid character error
    pub fn invalid_character_error(&mut self, span: crate::lexer::Span, char: char) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E011".to_string(),
            message: format!("无效的字符: '{}'", char),
            english_message: format!("Invalid character: '{}'", char),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查是否使用了有效的字符编码".to_string()),
            related_code: None,
        });
    }

    /// Add unclosed string error
    pub fn unclosed_string_error(&mut self, span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E012".to_string(),
            message: "未闭合的字符串".to_string(),
            english_message: "Unclosed string".to_string(),
            file_path: None,
            span: Some(span),
            suggestion: Some("在字符串末尾添加引号".to_string()),
            related_code: None,
        });
    }

    /// Add invalid escape sequence error
    pub fn invalid_escape_sequence_error(&mut self, span: crate::lexer::Span, sequence: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E013".to_string(),
            message: format!("无效的转义序列: '{}'", sequence),
            english_message: format!("Invalid escape sequence: '{}'", sequence),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查转义序列是否正确".to_string()),
            related_code: None,
        });
    }

    // ===== 语义错误 | Semantic Errors =====

    /// Add variable redeclaration error
    pub fn variable_redeclaration_error(&mut self, span: crate::lexer::Span, var_name: &str, original_span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E014".to_string(),
            message: format!("变量重复声明: '{}'", var_name),
            english_message: format!("Variable redeclaration: '{}'", var_name),
            file_path: None,
            span: Some(span),
            suggestion: Some("使用不同的变量名".to_string()),
            related_code: Some(format!("原始声明在 {}..{}", original_span.start, original_span.end)),
        });
    }

    /// Add constant reassignment error
    pub fn constant_reassignment_error(&mut self, span: crate::lexer::Span, const_name: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E015".to_string(),
            message: format!("常量不能重新赋值: '{}'", const_name),
            english_message: format!("Cannot reassign constant: '{}'", const_name),
            file_path: None,
            span: Some(span),
            suggestion: Some("使用变量而不是常量".to_string()),
            related_code: None,
        });
    }

    /// Add undefined function error
    pub fn undefined_function_error(&mut self, span: crate::lexer::Span, func_name: &str, suggestion: Option<&str>) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E016".to_string(),
            message: format!("未定义的函数: '{}'", func_name),
            english_message: format!("Undefined function: '{}'", func_name),
            file_path: None,
            span: Some(span),
            suggestion: suggestion.map(|s| s.to_string()),
            related_code: None,
        });
    }

    /// Add parameter count mismatch error
    pub fn parameter_count_mismatch_error(&mut self, span: crate::lexer::Span, func_name: &str, expected: usize, found: usize) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E017".to_string(),
            message: format!("函数 '{}' 参数数量不匹配: 期望 {} 个参数, 实际 {} 个", func_name, expected, found),
            english_message: format!("Function '{}' parameter count mismatch: expected {}, found {}", func_name, expected, found),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查函数调用的参数数量".to_string()),
            related_code: None,
        });
    }

    /// Add parameter type mismatch error
    pub fn parameter_type_mismatch_error(&mut self, span: crate::lexer::Span, func_name: &str, param_index: usize, expected: &str, found: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E018".to_string(),
            message: format!("函��� '{}' 第 {} 个参数类型不匹配: 期望 '{}', 实际 '{}'", func_name, param_index + 1, expected, found),
            english_message: format!("Function '{}' parameter {} type mismatch: expected '{}', found '{}'", func_name, param_index + 1, expected, found),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查参数类型是否正确".to_string()),
            related_code: None,
        });
    }

    /// Add return type mismatch error
    pub fn return_type_mismatch_error(&mut self, span: crate::lexer::Span, func_name: &str, expected: &str, found: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E019".to_string(),
            message: format!("函数 '{}' 返回类型不匹配: 期望 '{}', 实际 '{}'", func_name, expected, found),
            english_message: format!("Function '{}' return type mismatch: expected '{}', found '{}'", func_name, expected, found),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查返回值的类型".to_string()),
            related_code: None,
        });
    }

    /// Add missing return statement error
    pub fn missing_return_statement_error(&mut self, span: crate::lexer::Span, func_name: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E020".to_string(),
            message: format!("函数 '{}' 缺少返回语句", func_name),
            english_message: format!("Function '{}' missing return statement", func_name),
            file_path: None,
            span: Some(span),
            suggestion: Some("在函数末尾添加返回语句".to_string()),
            related_code: None,
        });
    }

    /// Add invalid array index type error
    pub fn invalid_array_index_type_error(&mut self, span: crate::lexer::Span, expected: &str, found: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E021".to_string(),
            message: format!("数组索引类型错误: 期望 '{}', 实际 '{}'", expected, found),
            english_message: format!("Array index type error: expected '{}', found '{}'", expected, found),
            file_path: None,
            span: Some(span),
            suggestion: Some("数组索引必须是整数类型".to_string()),
            related_code: None,
        });
    }

    /// Add array out of bounds error
    pub fn array_out_of_bounds_error(&mut self, span: crate::lexer::Span, array_size: usize, index: isize) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E022".to_string(),
            message: format!("数组越界访问: 数组长度 {}, 访问索引 {}", array_size, index),
            english_message: format!("Array out of bounds: array size {}, accessing index {}", array_size, index),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查数组索引是否在有效范围内".to_string()),
            related_code: None,
        });
    }

    /// Add non-boolean condition error
    pub fn non_boolean_condition_error(&mut self, span: crate::lexer::Span, condition_type: &str, context: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "E023".to_string(),
            message: format!("{} 条件必须是布尔类型, 实际是 '{}'", context, condition_type),
            english_message: format!("{} condition must be boolean, actual type is '{}'", context, condition_type),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查条件表达式的类型".to_string()),
            related_code: None,
        });
    }

    // ===== 运行时错误 | Runtime Errors =====

    /// Add division by zero error
    pub fn division_by_zero_error(&mut self, span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "R001".to_string(),
            message: "除零错误".to_string(),
            english_message: "Division by zero".to_string(),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查除数是否为零".to_string()),
            related_code: None,
        });
    }

    /// Add stack overflow error
    pub fn stack_overflow_error(&mut self, span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "R002".to_string(),
            message: "栈溢出".to_string(),
            english_message: "Stack overflow".to_string(),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查是否存在无限递归".to_string()),
            related_code: None,
        });
    }

    /// Add out of memory error
    pub fn out_of_memory_error(&mut self, span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "R003".to_string(),
            message: "内存不足".to_string(),
            english_message: "Out of memory".to_string(),
            file_path: None,
            span: Some(span),
            suggestion: Some("减少内存使用或增加系统内存".to_string()),
            related_code: None,
        });
    }

    /// Add null pointer dereference error
    pub fn null_pointer_dereference_error(&mut self, span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "R004".to_string(),
            message: "空指针解引用".to_string(),
            english_message: "Null pointer dereference".to_string(),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查指针是否为空".to_string()),
            related_code: None,
        });
    }

    /// Add integer overflow error
    pub fn integer_overflow_error(&mut self, span: crate::lexer::Span, operation: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "R005".to_string(),
            message: format!("整数溢出: {}", operation),
            english_message: format!("Integer overflow: {}", operation),
            file_path: None,
            span: Some(span),
            suggestion: Some("使用更大的整数类型或检查数值范围".to_string()),
            related_code: None,
        });
    }

    /// Add type conversion error
    pub fn type_conversion_error(&mut self, span: crate::lexer::Span, from: &str, to: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "R006".to_string(),
            message: format!("类型转换错误: 无法将 '{}' 转换为 '{}'", from, to),
            english_message: format!("Type conversion error: cannot convert '{}' to '{}'", from, to),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查类型转换是否合理".to_string()),
            related_code: None,
        });
    }

    /// Add file not found error
    pub fn file_not_found_error(&mut self, span: crate::lexer::Span, file_path: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "R007".to_string(),
            message: format!("文件未找到: '{}'", file_path),
            english_message: format!("File not found: '{}'", file_path),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查文件路径是否正确".to_string()),
            related_code: None,
        });
    }

    /// Add permission denied error
    pub fn permission_denied_error(&mut self, span: crate::lexer::Span, resource: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: "R008".to_string(),
            message: format!("权限被拒绝: 访问 '{}'", resource),
            english_message: format!("Permission denied: accessing '{}'", resource),
            file_path: None,
            span: Some(span),
            suggestion: Some("检查是否有足够的权限".to_string()),
            related_code: None,
        });
    }

    // ===== 警告 | Warnings =====

    /// Add dead code warning
    pub fn dead_code_warning(&mut self, span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: "W003".to_string(),
            message: "死代码: 这段代码永远不会执行".to_string(),
            english_message: "Dead code: this code will never execute".to_string(),
            file_path: None,
            span: Some(span),
            suggestion: Some("考虑删除这段无用的代码".to_string()),
            related_code: None,
        });
    }

    /// Add unused function parameter warning
    pub fn unused_parameter_warning(&mut self, span: crate::lexer::Span, param_name: &str, func_name: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: "W004".to_string(),
            message: format!("未使用的函数参数: '{}' 在函数 '{}'", param_name, func_name),
            english_message: format!("Unused function parameter: '{}' in function '{}'", param_name, func_name),
            file_path: None,
            span: Some(span),
            suggestion: Some("如果不需要此参数，请考虑删除它或在参数名前添加下划线".to_string()),
            related_code: None,
        });
    }

    /// Add variable shadowing warning
    pub fn variable_shadowing_warning(&mut self, span: crate::lexer::Span, var_name: &str, original_span: crate::lexer::Span) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: "W005".to_string(),
            message: format!("变量遮蔽: '{}' 遮蔽了外层作用域的同名变量", var_name),
            english_message: format!("Variable shadowing: '{}' shadows variable with same name in outer scope", var_name),
            file_path: None,
            span: Some(span),
            suggestion: Some("考虑使用不同的变量名".to_string()),
            related_code: Some(format!("原始变量在 {}..{}", original_span.start, original_span.end)),
        });
    }

    /// Add function name conflict warning
    pub fn function_name_conflict_warning(&mut self, span: crate::lexer::Span, func_name: &str, builtin: bool) {
        let suggestion = if builtin {
            "避免使用内置函数名".to_string()
        } else {
            "使用不同的函数名".to_string()
        };

        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: "W006".to_string(),
            message: format!("函数名冲突: '{}' 与现有函数冲突", func_name),
            english_message: format!("Function name conflict: '{}' conflicts with existing function", func_name),
            file_path: None,
            span: Some(span),
            suggestion: Some(suggestion),
            related_code: None,
        });
    }

    /// Add implicit conversion warning
    pub fn implicit_conversion_warning(&mut self, span: crate::lexer::Span, from: &str, to: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: "W007".to_string(),
            message: format!("隐式类型转换: 从 '{}' 到 '{}'", from, to),
            english_message: format!("Implicit type conversion: from '{}' to '{}'", from, to),
            file_path: None,
            span: Some(span),
            suggestion: Some("考虑使用显式类型转换以提高代码清晰度".to_string()),
            related_code: None,
        });
    }

    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn add_error(&mut self, code: &str, message: &str, english_message: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::错误,
            code: code.to_string(),
            message: message.to_string(),
            english_message: english_message.to_string(),
            file_path: None,
            span: None,
            suggestion: None,
            related_code: None,
        });
    }

    pub fn add_warning(&mut self, code: &str, message: &str, english_message: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::警告,
            code: code.to_string(),
            message: message.to_string(),
            english_message: english_message.to_string(),
            file_path: None,
            span: None,
            suggestion: None,
            related_code: None,
        }
        );
    }

    pub fn add_info(&mut self, code: &str, message: &str, english_message: &str) {
        self.add_diagnostic(Diagnostic {
            level: DiagnosticLevel::信息,
            code: code.to_string(),
            message: message.to_string(),
            english_message: english_message.to_string(),
            file_path: None,
            span: None,
            suggestion: None,
            related_code: None,
        });
    }

    pub fn get_diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn get_errors(&self) -> Vec<&Diagnostic> {
        self.diagnostics.iter()
            .filter(|d| d.level == DiagnosticLevel::错误)
            .collect()
    }

    pub fn get_warnings(&self) -> Vec<&Diagnostic> {
        self.diagnostics.iter()
            .filter(|d| d.level == DiagnosticLevel::警告)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.level == DiagnosticLevel::错误)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics.iter()
            .filter(|d| d.level == DiagnosticLevel::错误)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics.iter()
            .filter(|d| d.level == DiagnosticLevel::警告)
            .count()
    }

    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }

    pub fn set_max_errors(&mut self, max: usize) {
        self.max_errors = max;
    }

    pub fn set_max_warnings(&mut self, max: usize) {
        self.max_warnings = max;
    }

    pub fn format_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        let level_str = match diagnostic.level {
            DiagnosticLevel::错误 => "错误",
            DiagnosticLevel::警告 => "警告",
            DiagnosticLevel::信息 => "信息",
        };

        let mut result = format!("{}: {}", level_str, diagnostic.message);

        if let Some(file_path) = &diagnostic.file_path {
            result.push_str(&format!(" ({})", file_path.display()));
        }

        if let Some(span) = diagnostic.span {
            result.push_str(&format!(" at {}..{}", span.start, span.end));
        }

        if !diagnostic.code.is_empty() {
            result.push_str(&format!(" [{}]", diagnostic.code));
        }

        if let Some(suggestion) = &diagnostic.suggestion {
            result.push_str(&format!("\n  建议: {}", suggestion));
        }

        result
    }

    pub fn print_diagnostics(&self) {
        for diagnostic in &self.diagnostics {
            eprintln!("{}", self.format_diagnostic(diagnostic));
        }
    }

    /// Format all diagnostics as Chinese messages with error codes and suggestions
    pub fn format_chinese_messages(&self) -> String {
        if self.diagnostics.is_empty() {
            return "没有诊断信息".to_string();
        }

        let mut result = String::new();
        let error_count = self.error_count();
        let warning_count = self.warning_count();

        if error_count > 0 {
            result.push_str(&format!("发现 {} 个错误:\n", error_count));
        }
        if warning_count > 0 {
            result.push_str(&format!("发现 {} 个警告:\n", warning_count));
        }
        result.push('\n');

        for (i, diagnostic) in self.diagnostics.iter().enumerate() {
            result.push_str(&format!("{}: {}\n", i + 1, diagnostic.code));
            result.push_str(&format!("  {}\n", diagnostic.message));

            if let Some(suggestion) = &diagnostic.suggestion {
                result.push_str(&format!("  建议: {}\n", suggestion));
            }

            if let Some(related_code) = &diagnostic.related_code {
                if !related_code.is_empty() {
                    result.push_str(&format!("  相关代码: {}\n", related_code));
                }
            }

            if let Some(span) = diagnostic.span {
                result.push_str(&format!("  位置: {}..{}\n", span.start, span.end));
            }

            if i < self.diagnostics.len() - 1 {
                result.push('\n');
            }
        }

        result
    }
}

impl Default for DiagnosticManager {
    fn default() -> Self {
        Self::new()
    }
}