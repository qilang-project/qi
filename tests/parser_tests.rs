//! Unit tests for Qi parser module
//! 测试解析器模块

use qi_compiler::lexer::*;
use qi_compiler::parser::*;

#[test]
fn test_parse_empty_program() {
    let source = "";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let result = parser.parse(tokens);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 0);
}

#[test]
fn test_parse_direct_source() {
    let source = "42;";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_simple_variable_declaration() {
    let source = "变量 x = 10;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let result = parser.parse(tokens);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_multiple_statements() {
    let source = "变量 x = 10; 变量 y = 20;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let result = parser.parse(tokens);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn test_parse_chinese_variable_names() {
    let source = "变量 数字 = 42; 变量 文本 = \"你好\";";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn test_parse_basic_expressions() {
    let source = "1 + 2 * 3;";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_function_declaration() {
    let source = "函数 测试() { }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_function_with_return() {
    let source = "函数 main() { 返回 42; }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_function_with_parameters() {
    let source = "函数 add(x, y) { 返回 x + y; }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_if_statement() {
    let source = "如果 x > 5 { 变量 y = 10; }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_if_else_statement() {
    let source = "如果 x > 5 { 变量 y = 10; } 否则 { 变量 y = 0; }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_while_loop() {
    let source = "当 i < 10 { 变量 x = x + 1; }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_for_loop() {
    let source = "对于 i 在 [1, 2, 3] { 总和 = 总和 + i; }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_nested_control_flow() {
    let source = r#"
    如果 x > 5 {
        当 i < 10 {
            如果 i == 5 {
                返回 "找到";
            }
            i = i + 1;
        }
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_function_call() {
    let source = "变量 result = test();";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_function_call_with_arguments() {
    let source = "变量 result = add(1, 2);";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_complex_expressions() {
    let source = "变量 result = (1 + 2) * (3 - 4) / 5;";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_string_literals() {
    let source = "变量 message = \"Hello, World!\";";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_character_literals() {
    let source = "变量 ch = 'A';";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_boolean_expressions() {
    let source = "变量 x = 10; 如果 x > 5 { 返回 真; }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn test_parse_comparison_operators() {
    let source = "如果 a == b && c != d { 返回 真; }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_array_literal() {
    let source = "变量 arr = [1, 2, 3];";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_struct_declaration() {
    let source = "结构体 Person { name: 字符串, age: 整数 }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_empty_struct() {
    let source = "结构体 Empty { }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_struct_with_single_field() {
    let source = "结构体 Point { x: 整数 }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_struct_with_multiple_fields() {
    let source = "结构体 Rectangle { width: 浮点数, height: 浮点数, color: 字符串 }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_struct_with_chinese_field_names() {
    let source = "结构体 学生 { 姓名: 字符串, 年龄: 整数, 成绩: 浮点数 }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_multiple_struct_declarations() {
    let source = "结构体 Person { name: 字符串 } 结构体 Point { x: 整数, y: 整数 }";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn test_parse_assignment() {
    let source = "变量 x = 10; x = x + 1;";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn test_parse_complex_program() {
    let source = r#"
    函数 factorial(n) {
        如果 n <= 1 {
            返回 1;
        } 否则 {
            返回 n * factorial(n - 1);
        }
    }

    函数 main() {
        变量 result = factorial(5);
        返回 result;
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn test_parse_error_handling() {
    let source = "变量 x = ;"; // Incomplete assignment
    let parser = Parser::new();
    let result = parser.parse_source(source);

    // Should handle parse errors gracefully
    assert!(result.is_err());
}

#[test]
fn test_parse_unterminated_string() {
    let source = "变量 s = \"unclosed string;";
    let parser = Parser::new();
    let result = parser.parse_source(source);

    // Should handle unterminated string error
    assert!(result.is_err());
}

#[test]
fn test_parse_mismatched_brackets() {
    let source = "如果 x > 5 { 变量 y = 10;"; // Missing closing brace
    let parser = Parser::new();
    let result = parser.parse_source(source);

    // Should handle mismatched brackets error
    assert!(result.is_err());
}

#[test]
fn test_parser_default_trait() {
    let parser = Parser::default();
    let source = "变量 x = 42;";
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
#[ignore = "Triple-quoted multi-line comments not yet supported"]
fn test_parse_with_whitespace_and_comments() {
    let source = r#"
    // This is a comment
    变量 x = 10; /* Another comment */ 变量 y = 20;

    """
    Multi-line comment
    """
    函数 test() {
        返回 42;
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 3);
}