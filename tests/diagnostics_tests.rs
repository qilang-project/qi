//! Unit tests for diagnostic system
//! 测试诊断系统

use qi_compiler::lexer::*;
use qi_compiler::utils::diagnostics::DiagnosticManager;

#[test]
fn test_lexer_diagnostics_integration() {
    let source = "变量 x = 5 @ 3;"; // Invalid character
    let mut lexer = Lexer::new(source.to_string());
    let _result = lexer.tokenize().unwrap_err();

    // Test that lexer collects diagnostics
    let diagnostics = lexer.diagnostics();
    assert!(diagnostics.error_count() > 0);

    let formatted = lexer.format_diagnostics();
    assert!(formatted.contains("无效字符"));
}

#[test]
fn test_lexer_unterminated_string_diagnostics() {
    let source = r#"变量 message = "unclosed string;"#;
    let mut lexer = Lexer::new(source.to_string());
    let _result = lexer.tokenize().unwrap_err();

    let diagnostics = lexer.diagnostics();
    assert!(diagnostics.error_count() > 0);

    let formatted = lexer.format_diagnostics();
    assert!(formatted.contains("未终止的字符串"));
}

#[test]
fn test_lexer_multiple_errors() {
    let source = "变量 x = @; 变量 y = 'unclosed; 变量 z = 123.456.789;";
    let mut lexer = Lexer::new(source.to_string());
    let _result = lexer.tokenize().unwrap_err();

    let diagnostics = lexer.diagnostics();
    let (error_count, _) = lexer.get_error_summary();

    // Should collect multiple errors
    assert!(error_count > 1);
    assert!(diagnostics.has_errors());
}

#[test]
fn test_lexer_span_information_in_diagnostics() {
    let source = "变量 x = @invalid;";
    let mut lexer = Lexer::new(source.to_string());
    let _result = lexer.tokenize().unwrap_err();

    let _diagnostics = lexer.diagnostics();
    let formatted = lexer.format_diagnostics();

    // Should include position information
    assert!(formatted.contains("@") || formatted.contains("invalid"));
}

#[test]
fn test_empty_source_diagnostics() {
    let source = "";
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_ok());

    let diagnostics = lexer.diagnostics();
    assert_eq!(diagnostics.error_count(), 0);
    assert!(!diagnostics.has_errors());
}

#[test]
fn test_valid_source_no_diagnostics() {
    let source = "变量 x = 42; 变量 y = \"hello\";";
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_ok());

    let diagnostics = lexer.diagnostics();
    assert_eq!(diagnostics.error_count(), 0);
    assert!(!diagnostics.has_errors());
}

#[test]
fn test_diagnostics_format_chinese() {
    let source = "变量 x = @;";
    let mut lexer = Lexer::new(source.to_string());
    let _result = lexer.tokenize().unwrap_err();

    let formatted = lexer.format_diagnostics();

    // Should format in Chinese
    assert!(formatted.contains("无效") || formatted.contains("错误"));
}

#[test]
fn test_diagnostics_error_summary() {
    let source = "变量 x = @; 变量 y = #;";
    let mut lexer = Lexer::new(source.to_string());
    let _result = lexer.tokenize().unwrap_err();

    let (error_count, warning_count) = lexer.get_error_summary();

    assert!(error_count > 0);
    assert_eq!(warning_count, 0); // Lexer typically produces errors, not warnings
    assert!(lexer.has_critical_errors());
}

#[test]
fn test_unicode_error_diagnostics() {
    let source = "变量 中文 = 🚀;";
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    // Should handle Unicode without errors
    assert!(result.is_ok());

    let diagnostics = lexer.diagnostics();
    assert_eq!(diagnostics.error_count(), 0);
}

#[test]
fn test_comment_handling_no_diagnostics() {
    let source = r#"
    // This is a comment
    变量 x = 5; /* Block comment */ 变量 y = 10;
    /// Doc comment
    /** Doc block comment */
    "#;

    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_ok());

    let diagnostics = lexer.diagnostics();
    assert_eq!(diagnostics.error_count(), 0);
}

#[test]
fn test_diagnostic_manager_creation() {
  
    let manager = DiagnosticManager::new();
    assert_eq!(manager.error_count(), 0);
    assert_eq!(manager.warning_count(), 0);
    assert!(!manager.has_errors());
    assert_eq!(manager.warning_count(), 0);
}

#[test]
fn test_diagnostics_with_line_and_column() {
    let source = "变量 x = 5;\n变量 y = @;\n变量 z = 10;";
    let mut lexer = Lexer::new(source.to_string());
    let _result = lexer.tokenize().unwrap_err();

    let formatted = lexer.format_diagnostics();

    // Should include line information for multi-line source
    assert!(formatted.contains("行") || formatted.contains("@"));
}