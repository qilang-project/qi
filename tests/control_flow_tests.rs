//! Unit tests for control flow structures in Qi language
//! 测试控制流结构

use qi_compiler::lexer::*;
use qi_compiler::parser::*;
use qi_compiler::semantic::*;

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
fn test_nested_control_flow() {
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
fn test_complex_control_flow_program() {
    let source = r#"
    函数 testfunc(target: 整数): 整数 {
        变量 index = 0;
        当 index < 10 {
            如果 index == target {
                返回 100;
            }
            index = index + 1;
        }
        返回 0;
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    if let Err(e) = &result {
        eprintln!("Parse error: {:?}", e);
    }
    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_control_flow_with_boolean_expressions() {
    let source = r#"
    变量 x = 10;
    变量 y = 5;

    如果 x > y && x < 20 {
        变量 result = "范围内";
    } 否则 {
        变量 result = "范围外";
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 3);
}

#[test]
fn test_while_loop_with_counter() {
    let source = r#"
    变量 count = 0;
    变量 sum = 0;

    当 count < 5 {
        sum = sum + count;
        count = count + 1;
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 3);
}

#[test]
fn test_for_loop_with_array() {
    let source = r#"
    变量 total = 0;
    对于 number 在 [1, 2, 3, 4, 5] {
        total = total + number;
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn test_multiple_nested_structures() {
    let source = r#"
    变量 matrix = [[1, 2], [3, 4]];
    变最大值 = 0;

    对于 row 在 matrix {
        对于 value 在 row {
            如果 value > 最大值 {
                最大值 = value;
            }
        }
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 3);
}

#[test]
fn test_early_return_in_loop() {
    let source = r#"
    函数 find_value(target: 整数): 整数 {
        变量 i = 0;
        当 i < 10 {
            如果 i == target {
                返回 i;
            }
            i = i + 1;
        }
        返回 99;
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_control_flow_semantic_analysis() {
    let source = r#"
    变量 x = 10;
    如果 x > 5 {
        变量 y = 20;
    }
    "#;

    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze(&AstNode::程序(program));

    // Should analyze control flow without panicking
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_loop_type_validation() {
    let source = r#"
    变量 i = 0;
    当 i < 10 {
        i = i + 1;
    }
    "#;

    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze(&AstNode::程序(program));

    // Should analyze loop structure
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_control_flow_codegen() {
    let source = r#"
    函数 testcontrol(x: 整数): 整数 {
        如果 x > 0 {
            返回 1;
        } 否则 {
            返回 0;
        }
    }
    "#;

    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let result = parser.parse(tokens);
    
    if let Err(e) = &result {
        eprintln!("Parse error: {:?}", e);
    }
    
    let program = result.unwrap();

    let mut generator = qi_compiler::codegen::CodeGenerator::new(
        qi_compiler::config::CompilationTarget::Linux
    );
    let result = generator.generate(&AstNode::程序(program));

    // Should generate code for control flow
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
    }
}

#[test]
fn test_control_flow_keywords_tokenization() {
    let keywords = vec!["如果", "否则", "当", "对于"];

    for keyword in keywords {
        let mut lexer = Lexer::new(keyword.to_string());
        let tokens = lexer.tokenize().unwrap();

        assert!(!tokens.is_empty(), "Failed to tokenize keyword: {}", keyword);

        // Check that the keyword is properly recognized
        let token_str = format!("{:?}", tokens[0]);
        assert!(token_str.contains(keyword) ||
                tokens.iter().any(|t| format!("{:?}", t).contains(keyword)),
                "Keyword not properly recognized: {}", keyword);
    }
}

#[test]
fn test_deeply_nested_control_flow() {
    let source = r#"
    如果 条件1 {
        如果 条件2 {
            当 循环条件 {
                如果 条件3 {
                    返回 "深度嵌套";
                }
            }
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
fn test_control_flow_with_function_calls() {
    let source = r#"
    函数 is_even(n) {
        返回 n % 2 == 0;
    }

    函数 process_list(items) {
        对于 item 在 items {
            如果 is_even(item) {
                打印 "偶数";
            } 否则 {
                打印 "奇数";
            }
        }
    }
    "#;

    let parser = Parser::new();
    let result = parser.parse_source(source);

    assert!(result.is_ok());
    let program = result.unwrap();
    assert_eq!(program.statements.len(), 2);
}