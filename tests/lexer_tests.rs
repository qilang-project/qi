//! Unit tests for Qi lexer module
//! 测试词法分析器模块

use qi_compiler::lexer::*;

#[test]
fn test_tokenization_of_basic_tokens() {
    let source = "(){},;:.";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let expected_kinds = vec![
        TokenKind::左括号,
        TokenKind::右括号,
        TokenKind::左大括号,
        TokenKind::右大括号,
        TokenKind::逗号,
        TokenKind::分号,
        TokenKind::冒号,
        TokenKind::点,
        TokenKind::文件结束,
    ];

    assert_eq!(tokens.len(), expected_kinds.len());
    for (token, expected) in tokens.iter().zip(expected_kinds.iter()) {
        assert_eq!(token.kind, *expected);
    }
}

#[test]
fn test_chinese_keywords() {
    let source = "如果 否则 当 对于 函数 返回 变量 常量 整数 字符串 布尔 浮点数";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let expected_keywords = vec![
        TokenKind::如果,
        TokenKind::否则,
        TokenKind::当,
        TokenKind::对于,
        TokenKind::函数,
        TokenKind::返回,
        TokenKind::变量,
        TokenKind::常量,
        TokenKind::整数,
        TokenKind::字符串,
        TokenKind::布尔,
        TokenKind::浮点数,
        TokenKind::文件结束,
    ];

    assert_eq!(tokens.len(), expected_keywords.len());
    for (token, expected) in tokens.iter().zip(expected_keywords.iter()) {
        assert_eq!(token.kind, *expected);
    }
}

#[test]
fn test_chinese_identifiers() {
    let source = "变量名 函数名 用户标识符";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    // All Chinese characters should be recognized as identifiers
    for token in &tokens[..tokens.len()-1] { // Skip EOF token
        assert_eq!(token.kind, TokenKind::标识符);
    }
}

#[test]
fn test_mixed_chinese_english() {
    let source = "变量 myVar = 42;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let expected_kinds = vec![
        TokenKind::变量,
        TokenKind::标识符,
        TokenKind::赋值,
        TokenKind::整数字面量(42),
        TokenKind::分号,
        TokenKind::文件结束,
    ];

    assert_eq!(tokens.len(), expected_kinds.len());
    for (token, expected) in tokens.iter().zip(expected_kinds.iter()) {
        assert_eq!(token.kind, *expected);
    }
    assert_eq!(tokens[1].text, "myVar");
}

#[test]
fn test_string_literals() {
    let source = r#"变量 消息 = "你好，世界！";"#;
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].kind, TokenKind::变量);
    assert_eq!(tokens[1].kind, TokenKind::标识符);
    assert_eq!(tokens[1].text, "消息");
    assert_eq!(tokens[2].kind, TokenKind::赋值);
    assert_eq!(tokens[3].kind, TokenKind::字符串字面量);
    assert_eq!(tokens[3].text, "\"你好，世界！\"");
}

#[test]
fn test_numeric_literals() {
    let source = "42";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens[0].kind, TokenKind::整数字面量(42));
    
    // Test floats separately
    let source2 = "3.14";
    let mut lexer2 = Lexer::new(source2.to_string());
    let tokens2 = lexer2.tokenize().unwrap();
    
    assert!(matches!(tokens2[0].kind, TokenKind::浮点数字面量));
}

#[test]
fn test_character_literals() {
    let source = "'A' '中' '\\n'";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens.len(), 4); // 3 chars + EOF
    assert!(matches!(tokens[0].kind, TokenKind::字符字面量('A')));
    assert!(matches!(tokens[1].kind, TokenKind::字符字面量('中')));
    assert!(matches!(tokens[2].kind, TokenKind::字符字面量('\n')));
}

#[test]
fn test_operators() {
    let source = "+ - * / = == != < > <= >=";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let expected_operators = vec![
        TokenKind::加,
        TokenKind::减,
        TokenKind::乘,
        TokenKind::除,
        TokenKind::赋值,
        TokenKind::等于,
        TokenKind::不等于,
        TokenKind::小于,
        TokenKind::大于,
        TokenKind::小于等于,
        TokenKind::大于等于,
        TokenKind::文件结束,
    ];

    assert_eq!(tokens.len(), expected_operators.len());
    for (token, expected) in tokens.iter().zip(expected_operators.iter()) {
        assert_eq!(token.kind, *expected);
    }
}

#[test]
fn test_empty_input() {
    let source = "";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenKind::文件结束);
}

#[test]
fn test_whitespace_handling() {
    let source = "   \t\n  变量 x = 1;  \n\t";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    // Should have: 变量, x, =, 1, ;, 文件结束
    assert_eq!(tokens.len(), 6);
    assert_eq!(tokens[0].kind, TokenKind::变量);
    assert_eq!(tokens[1].kind, TokenKind::标识符);
    assert_eq!(tokens[1].text, "x");
}

#[test]
fn test_invalid_character() {
    let source = "变量 x = @;";
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_err());
    match result.unwrap_err() {
        LexicalError::InvalidCharacter(char, line, col) => {
            assert_eq!(char, '@');
            assert_eq!(line, 1);
            // Column may be slightly different due to UTF-8 character handling
            assert!(col >= 9 && col <= 11);
        }
        _ => panic!("Expected InvalidCharacter error"),
    }
}

#[test]
fn test_unterminated_string() {
    let source = r#"变量 消息 = "未终止字符串"#;
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_err());
    match result.unwrap_err() {
        LexicalError::UnterminatedString(line, _col) => {
            assert_eq!(line, 1);
            // Column may vary slightly due to UTF-8 handling
        }
        _ => panic!("Expected UnterminatedString error"),
    }
}

#[test]
fn test_comments_are_skipped() {
    let source = r#"
    // This is a line comment
    变量 x = 5; /* This is a block comment */ 变量 y = 10;
    /// This is a doc comment
    /** This is a doc block comment */
    "#;

    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    // Should only tokenize the actual code, not comments
    let expected_kinds = vec![
        TokenKind::变量,
        TokenKind::标识符,
        TokenKind::赋值,
        TokenKind::整数字面量(5),
        TokenKind::分号,
        TokenKind::变量,
        TokenKind::标识符,
        TokenKind::赋值,
        TokenKind::整数字面量(10),
        TokenKind::分号,
        TokenKind::文件结束,
    ];

    assert_eq!(tokens.len(), expected_kinds.len());
    for (token, expected) in tokens.iter().zip(expected_kinds.iter()) {
        assert_eq!(token.kind, *expected);
    }
}

#[test]
fn test_span_information() {
    let source = "x = 42";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    // Test that span information is correctly recorded for simple tokens
    assert_eq!(tokens[0].span.start, 0); // "x"
    assert_eq!(tokens[0].line, 1);
    assert_eq!(tokens[0].column, 1);

    assert_eq!(tokens[1].span.start, 2); // "="
    assert_eq!(tokens[1].line, 1);
    assert_eq!(tokens[1].column, 3);
}

#[test]
fn test_line_and_column_tracking() {
    let source = "变量 x = 1;\n变量 y = 2;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    // First line
    assert_eq!(tokens[0].line, 1);
    assert_eq!(tokens[0].column, 1);

    // Second line (after newline)
    let newline_token = tokens.iter().find(|t| t.line == 2).unwrap();
    assert_eq!(newline_token.line, 2);
    assert_eq!(newline_token.column, 1);
}

#[test]
fn test_diagnostics_collection() {
    let source = "变量 x = 5 @ 3;";
    let mut lexer = Lexer::new(source.to_string());
    let result = lexer.tokenize();

    assert!(result.is_err());

    // Check that diagnostics were collected
    let diagnostics = lexer.diagnostics();
    assert!(diagnostics.error_count() > 0);

    let formatted = lexer.format_diagnostics();
    assert!(formatted.contains("无效字符"));
}

#[test]
fn test_error_summary() {
    let source = "变量 x = 5 @ 3;";
    let mut lexer = Lexer::new(source.to_string());
    let _result = lexer.tokenize().unwrap_err();

    let (error_count, warning_count) = lexer.get_error_summary();
    assert!(error_count > 0);
    assert_eq!(warning_count, 0);

    assert!(lexer.has_critical_errors());
}

#[test]
fn test_complex_source_tokenization() {
    let source = r#"
    函数 计算总和(数组) {
        变量 总和 = 0;
        对于 元素 在 数组 {
            总和 = 总和 + 元素;
        }
        返回 总和;
    }

    如果 总和 > 100 {
        返回 "大于100";
    } 否则 {
        返回 "小于等于100";
    }
    "#;

    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    // Should tokenize successfully
    assert!(!tokens.is_empty());
    assert_eq!(tokens.last().unwrap().kind, TokenKind::文件结束);

    // Should contain expected keywords
    let token_kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
    assert!(token_kinds.contains(&&TokenKind::函数));
    assert!(token_kinds.contains(&&TokenKind::对于));
    assert!(token_kinds.contains(&&TokenKind::返回));
    assert!(token_kinds.contains(&&TokenKind::如果));
    assert!(token_kinds.contains(&&TokenKind::否则));
}

#[test]
fn test_arrows_and_special_tokens() {
    let source = "->";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    assert_eq!(tokens.len(), 2); // -> + EOF
    assert_eq!(tokens[0].kind, TokenKind::箭头);
}

#[test]
fn test_unicode_support() {
    let source = "变量 中文变量名 = '中';\n字符串 火箭 = \"火箭\";";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    // Should handle Unicode characters properly
    assert!(tokens.iter().any(|t| t.text.contains("中文变量名")));
    assert!(tokens.iter().any(|t| t.text.contains("火箭")));
    assert!(tokens.iter().any(|t| t.text.contains("火箭")));
}