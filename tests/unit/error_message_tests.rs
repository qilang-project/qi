//! Error message validation tests
//!
//! These tests ensure that all error messages are properly formatted
//! and contain the expected Chinese text and error codes.

use qi_compiler::lexer::{Lexer, LexicalError};
use qi_compiler::utils::diagnostics::{DiagnosticManager, DiagnosticLevel};
use qi_compiler::utils::source::create_error_context_from_string;

#[test]
fn test_invalid_character_error_message() {
    let source = "变量 x = 5 @ 3;";
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_err());

    if let Err(LexicalError::InvalidCharacter(c, line, col)) = result {
        assert_eq!(c, '@');
        assert_eq!(line, 1);
        assert_eq!(col, 12);

        // Test that the error displays correctly in Chinese
        let error_str = format!("{}", LexicalError::InvalidCharacter(c, line, col));
        assert!(error_str.contains("无效字符"));
        assert!(error_str.contains("@"));
        assert!(error_str.contains("第 1 行第 12 列"));
    } else {
        panic!("Expected InvalidCharacter error");
    }
}

#[test]
fn test_unterminated_string_error_message() {
    let source = "变量 message = \"hello world;";
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_err());

    if let Err(LexicalError::UnterminatedString(line, col)) = result {
        assert_eq!(line, 1);
        assert_eq!(col, 15);

        // Test that the error displays correctly in Chinese
        let error_str = format!("{}", LexicalError::UnterminatedString(line, col));
        assert!(error_str.contains("未终止的字符串字面量"));
        assert!(error_str.contains("第 1 行第 15 列"));
    } else {
        panic!("Expected UnterminatedString error");
    }
}

#[test]
fn test_invalid_number_error_message() {
    let source = "变量 x = 123.456.789;";
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_err());

    if let Err(LexicalError::InvalidNumber(line, col)) = result {
        assert_eq!(line, 1);
        assert_eq!(col, 10);

        // Test that the error displays correctly in Chinese
        let error_str = format!("{}", LexicalError::InvalidNumber(line, col));
        assert!(error_str.contains("无效的数字格式"));
        assert!(error_str.contains("第 1 行第 10 列"));
    } else {
        panic!("Expected InvalidNumber error");
    }
}

#[test]
fn test_syntax_error_display() {
    let mut diagnostics = DiagnosticManager::new();

    // Test syntax error
    diagnostics.syntax_error(
        crate::lexer::Span::new(10, 11),
        ";",
        "打印",
        Some("在语句末尾添加分号")
    );

    let errors = diagnostics.get_errors();
    assert_eq!(errors.len(), 1);

    let error = &errors[0];
    assert_eq!(error.level, DiagnosticLevel::错误);
    assert!(error.message.contains("语法错误"));
    assert!(error.message.contains("期望 ';'"));
    assert!(error.message.contains("找到 '打印'"));
    assert!(error.code == "E001");
}

#[test]
fn test_type_mismatch_error_display() {
    let mut diagnostics = DiagnosticManager::new();

    // Test type mismatch error
    diagnostics.type_mismatch_error(
        crate::lexer::Span::new(20, 25),
        "整数",
        "字符串",
        Some("确保类型匹配")
    );

    let errors = diagnostics.get_errors();
    assert_eq!(errors.len(), 1);

    let error = &errors[0];
    assert_eq!(error.level, DiagnosticLevel::错误);
    assert!(error.message.contains("类型不匹配"));
    assert!(error.message.contains("期望 '整数'"));
    assert!(error.message.contains("实际 '字符串'"));
    assert!(error.code == "E002");
}

#[test]
fn test_undefined_variable_error_display() {
    let mut diagnostics = DiagnosticManager::new();

    // Test undefined variable error
    diagnostics.undefined_variable_error(
        crate::lexer::Span::new(30, 32),
        "undefined_var",
        Some("检查变量名是否正确")
    );

    let errors = diagnostics.get_errors();
    assert_eq!(errors.len(), 1);

    let error = &errors[0];
    assert_eq!(error.level, DiagnosticLevel::错误);
    assert!(error.message.contains("未定义的变量"));
    assert!(error.message.contains("undefined_var"));
    assert!(error.code == "E003");
}

#[test]
fn test_function_call_error_display() {
    let mut diagnostics = DiagnosticManager::new();

    // Test function call error
    diagnostics.function_call_error(
        crate::lexer::Span::new(40, 45),
        "参数数量不匹配",
        Some("检查函数签名")
    );

    let errors = diagnostics.get_errors();
    assert_eq!(errors.len(), 1);

    let error = &errors[0];
    assert_eq!(error.level, DiagnosticLevel::错误);
    assert!(error.message.contains("函数调用错误"));
    assert!(error.message.contains("参数数量不匹配"));
    assert!(error.code == "E004");
}

#[test]
fn test_warning_display() {
    let mut diagnostics = DiagnosticManager::new();

    // Test unused variable warning
    diagnostics.unused_variable_warning(
        crate::lexer::Span::new(50, 55),
        "unused_var"
    );

    let warnings = diagnostics.get_warnings();
    assert_eq!(warnings.len(), 1);

    let warning = &warnings[0];
    assert_eq!(warning.level, DiagnosticLevel::警告);
    assert!(warning.message.contains("未使用的变量"));
    assert!(warning.message.contains("unused_var"));
    assert!(warning.code == "W001");
}

#[test]
fn test_source_context_creation() {
    let source = r#"第1行
第2行有错误在这里
第3行"#;

    let context = create_error_context_from_string(source, 2, 8);

    assert_eq!(context.line_number, 2);
    assert_eq!(context.column_number, 8);
    assert_eq!(context.source_line, "第2行有错误在这里");
    assert_eq!(context.line_before, Some("第1行".to_string()));
    assert_eq!(context.line_after, Some("第3行".to_string()));
    assert_eq!(context.pointer, "       ↑");
}

#[test]
fn test_source_context_formatting() {
    let source = r#"第1行
第2行有错误
第3行"#;

    let context = create_error_context_from_string(source, 2, 6);
    let formatted = context.format();

    assert!(formatted.contains("第2行有错误"));
    assert!(formatted.contains("     ↑"));
    assert!(formatted.contains("第 2 行第 6 列"));
}

#[test]
fn test_source_context_first_line() {
    let source = "第一行有错误\n第二行";
    let context = create_error_context_from_string(source, 1, 5);

    assert_eq!(context.line_before, None);
    assert_eq!(context.source_line, "第一行有错误");
    assert_eq!(context.line_after, Some("第二行".to_string()));
}

#[test]
fn test_source_context_last_line() {
    let source = "第一行\n最后一行有错误";
    let context = create_error_context_from_string(source, 2, 4);

    assert_eq!(context.line_before, Some("第一行".to_string()));
    assert_eq!(context.source_line, "最后一行有错误");
    assert_eq!(context.line_after, None);
}

#[test]
fn test_diagnostic_formatting() {
    let mut diagnostics = DiagnosticManager::new();

    diagnostics.add_error(
        "E999",
        "这是一个测试错误",
        "This is a test error"
    );

    diagnostics.add_warning(
        "W999",
        "这是一个测试警告",
        "This is a test warning"
    );

    let errors = diagnostics.get_errors();
    let warnings = diagnostics.get_warnings();

    assert_eq!(errors.len(), 1);
    assert_eq!(warnings.len(), 1);

    let error_str = diagnostics.format_diagnostic(&errors[0]);
    assert!(error_str.contains("错误: 这是一个测试错误"));
    assert!(error_str.contains("[E999]"));

    let warning_str = diagnostics.format_diagnostic(&warnings[0]);
    assert!(warning_str.contains("警告: 这是一个测试警告"));
    assert!(warning_str.contains("[W999]"));
}

#[test]
fn test_error_count_and_detection() {
    let mut diagnostics = DiagnosticManager::new();

    assert!(!diagnostics.has_errors());
    assert_eq!(diagnostics.error_count(), 0);
    assert_eq!(diagnostics.warning_count(), 0);

    diagnostics.add_error("E001", "错误1", "Error 1");
    diagnostics.add_warning("W001", "警告1", "Warning 1");
    diagnostics.add_error("E002", "错误2", "Error 2");

    assert!(diagnostics.has_errors());
    assert_eq!(diagnostics.error_count(), 2);
    assert_eq!(diagnostics.warning_count(), 1);
}

#[test]
fn test_compilation_error_chinese_display() {
    use qi_compiler::{CompilerError, QiCompiler};
    use std::path::PathBuf;

    let source_with_error = "变量 x = 5 @ 3;"; // Invalid character

    let compiler = QiCompiler::new();
    let temp_file = PathBuf::from("test_temp.qi");
    std::fs::write(&temp_file, source_with_error).unwrap();

    let result = compiler.compile(temp_file);

    assert!(result.is_err());

    if let Err(compiler_error) = result {
        let error_str = format!("{}", compiler_error);
        assert!(error_str.contains("词法错误"));
        assert!(error_str.contains("无效字符"));
        assert!(error_str.contains("@"));
    }

    // Clean up
    let _ = std::fs::remove_file("test_temp.qi");
}