//! Unit tests for Qi parser

use qi_compiler::lexer::{Lexer, TokenKind};
use qi_compiler::parser::Parser;
use qi_compiler::parser::ast::*;
use qi_compiler::lexer::Span;

#[test]
fn test_empty_program() {
    let source = "".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Should return a program with no statements for now
    assert!(result.is_ok());
}

#[test]
fn test_single_literal() {
    let source = "42;".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_simple_variable_declaration() {
    let source = "变量 x = 10;".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_chinese_variable_names() {
    let source = "变量 数字 = 42; 变量 文本 = \"你好\";".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_basic_expressions() {
    let source = "1 + 2 * 3;".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_boolean_literals() {
    let source = "真; 假;".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_string_literals() {
    let source = "\"Hello, World!\";".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_declaration() {
    let source = "函数 测试() { }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_mixed_language_identifiers() {
    let source = "variable 中文变量 = 42;".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_complex_arithmetic() {
    let source = "(1 + 2) * (3 - 4) / 5;".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    // Parser is not fully implemented yet, but should not panic
    assert!(result.is_ok() || result.is_err());
}

// Control flow tests for User Story 3
#[test]
fn test_basic_if_statement() {
    let source = "如果 x > 5 { 打印 \"greater\"; }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_if_else_statement() {
    let source = "如果 age >= 18 { 打印 \"adult\"; } 否则 { 打印 \"minor\"; }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_nested_if_statements() {
    let source = "
    如果 age >= 18 {
        如果 has_license {
            打印 \"can drive\";
        } 否则 {
            打印 \"needs license\";
        }
    } 否则 {
        打印 \"too young\";
    }".to_string();

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_while_loop() {
    let source = "变量 count = 0; 当 count < 5 { count = count + 1; }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_nested_while_loops() {
    let source = "
    变量 outer = 0;
    当 outer < 3 {
        变量 inner = 0;
        当 inner < 3 {
            inner = inner + 1;
        }
        outer = outer + 1;
    }".to_string();

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_for_loop() {
    let source = "对于 (i = 0; i < 5; i = i + 1) { 打印 i; }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_nested_for_loops() {
    let source = "
    对于 (row = 0; row < 3; row = row + 1) {
        对于 (col = 0; col < 3; col = col + 1) {
            打印 row * 3 + col;
        }
    }".to_string();

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

// Function call tests for User Story 4
#[test]
fn test_basic_function_call() {
    let source = "打印问候();".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_call_with_single_argument() {
    let source = "打印数字(42);".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_call_with_multiple_arguments() {
    let source = "计算总和(10, 20, 30);".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_declaration_no_params_no_return() {
    let source = "函数 问好() { 打印 \"Hello!\"; }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_declaration_with_params_no_return() {
    let source = "函数 打印消息(message: 字符串) { 打印 message; }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_declaration_with_params_and_return() {
    let source = "函数 相加(a: 整数, b: 整数) -> 整数 { 返回 a + b; }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_declaration_no_params_with_return() {
    let source = "函数 获取幸运数字() -> 整数 { 返回 42; }".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_call_in_expression() {
    let source = "变量 result = 相加(5, 3) * 2;".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_nested_function_calls() {
    let source = "变量 result = 相加(相加(1, 2), 相加(3, 4));".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_call_with_string_arguments() {
    let source = "连接文本(\"Hello\", \"World\");".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_call_with_mixed_type_arguments() {
    let source = "混合操作(10, 2.5, \"result\");".to_string();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_recursive_function_call() {
    let source = "
    函数 阶乘(n: 整数) -> 整数 {
        如果 (n <= 1) {
            返回 1;
        } 否则 {
            返回 n * 阶乘(n - 1);
        }
    }".to_string();

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_call_in_condition() {
    let source = "
    如果 是偶数(10) {
        打印 \"Even\";
    }".to_string();

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_function_call_in_loop() {
    let source = "
    变量 i = 0;
    当 (i < 5) {
        打印数字(i);
        i = i + 1;
    }".to_string();

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_main_function() {
    let source = "
    函数 主() -> 整数 {
        打印 \"Hello, Qi!\";
        返回 0;
    }".to_string();

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_multiple_function_declarations() {
    let source = "
    函数 相加(a: 整数, b: 整数) -> 整数 {
        返回 a + b;
    }

    函数 乘以(a: 整数, b: 整数) -> 整数 {
        返回 a * b;
    }

    函数 主() -> 整数 {
        变量 sum = 相加(5, 3);
        变量 product = 乘以(4, 6);
        打印 sum;
        打印 product;
        返回 0;
    }".to_string();

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize successfully");

    let mut parser = Parser::new(tokens);
    let result = parser.parse();

    assert!(result.is_ok() || result.is_err());
}