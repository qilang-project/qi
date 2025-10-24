//! End-to-end test for Qi compiler: lexer, parser, AST, codegen, and runtime

use qi_compiler::lexer::Lexer;
use qi_compiler::parser::Parser;
use qi_compiler::codegen::CodeGenerator;
use qi_compiler::config::CompilationTarget;

#[test]
fn test_full_compilation_pipeline() {
    // Simple Qi program
    let source = r#"
        函数 主函数() {
            变量 x = 42;
            变量 y = 10;
            变量 z = x + y;
            返回 0;
        }
    "#.to_string();

    // Phase 1: Lexical Analysis
    let mut lexer = Lexer::new(source.clone());
    let tokens = lexer.tokenize().expect("Lexer should tokenize successfully");
    assert!(!tokens.is_empty(), "Tokens should not be empty");

    // Phase 2: Parsing
    let parser = Parser::new();
    let program = parser.parse_source(&source).expect("Parser should parse successfully");
    assert!(!program.statements.is_empty(), "AST should have statements");

    // Phase 3: Code Generation
    let mut codegen = CodeGenerator::new(CompilationTarget::Linux);
    
    // Test each statement
    for statement in &program.statements {
        let ir = codegen.generate(statement);
        assert!(ir.is_ok(), "Code generation should succeed for statement: {:?}", statement);
    }

    println!("✓ Full compilation pipeline test passed");
}

#[test]
fn test_async_runtime_ready() {
    use qi_compiler::runtime::{AsyncRuntime, AsyncRuntimeConfig};

    // Create async runtime
    let config = AsyncRuntimeConfig::default();
    let runtime = AsyncRuntime::new(config);
    assert!(runtime.is_ok(), "Async runtime should be created successfully");

    let runtime = runtime.unwrap();
    
    // Get runtime stats
    let stats = runtime.stats();
    assert_eq!(stats.worker_threads, num_cpus::get());

    println!("✓ Async runtime is ready with {} worker threads", stats.worker_threads);
}

#[test]
fn test_lexer_parser_integration() {
    let test_cases = vec![
        ("变量 x = 42;", "Variable declaration"),
        ("函数 test() { 返回 0; }", "Function declaration"),
        ("如果 x > 5 { 变量 y = 10; }", "If statement"),
    ];

    for (source, description) in test_cases {
        println!("Testing: {}", description);
        
        // Lexer
        let mut lexer = Lexer::new(source.to_string());
        let tokens = lexer.tokenize().expect(&format!("Lexer should tokenize: {}", description));
        assert!(!tokens.is_empty());

        // Parser
        let parser = Parser::new();
        let result = parser.parse_source(source);
        assert!(result.is_ok(), "Parser should parse: {}", description);
        
        println!("  ✓ {}", description);
    }
}

#[test]
fn test_runtime_environment_ready() {
    use qi_compiler::runtime::{RuntimeEnvironment, RuntimeConfig};

    let config = RuntimeConfig::default();
    let mut runtime = RuntimeEnvironment::new(config);
    assert!(runtime.is_ok(), "Runtime environment should be created");

    let mut runtime = runtime.unwrap();
    let init_result = runtime.initialize();
    assert!(init_result.is_ok(), "Runtime should initialize successfully");

    println!("✓ Runtime environment is ready");
}

#[test]
fn test_codegen_simple_expressions() {
    use qi_compiler::parser::ast::*;

    let test_cases = vec![
        "42;",
        "3.14;",
        "\"hello\";",
        "真;",
        "假;",
    ];

    let mut codegen = CodeGenerator::new(CompilationTarget::Linux);

    for source in test_cases {
        println!("Testing codegen for: {}", source);
        
        let parser = Parser::new();
        let program = parser.parse_source(source).expect(&format!("Should parse: {}", source));
        
        for statement in &program.statements {
            let ir = codegen.generate(statement);
            assert!(ir.is_ok(), "Should generate IR for: {}", source);
            
            let ir_code = ir.unwrap();
            assert!(!ir_code.is_empty(), "IR should not be empty for: {}", source);
        }
        
        println!("  ✓ Codegen successful");
    }
}

#[tokio::test]
async fn test_async_task_spawning() {
    use qi_compiler::runtime::{AsyncRuntime, AsyncRuntimeConfig};
    use std::time::Duration;

    let runtime = AsyncRuntime::new(AsyncRuntimeConfig::default())
        .expect("Runtime should be created");

    // Spawn a simple async task
    let handle = runtime.spawn(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
    });

    // Wait for completion
    let result = handle.join().await;
    assert!(result.is_ok(), "Task should complete successfully");

    println!("✓ Async task spawning and execution works");
}

#[test]
fn test_chinese_keyword_support() {
    use qi_compiler::lexer::{TokenKind, Lexer};

    let keywords = vec![
        ("如果", TokenKind::如果),
        ("否则", TokenKind::否则),
        ("循环", TokenKind::循环),
        ("当", TokenKind::当),
        ("对于", TokenKind::对于),
        ("函数", TokenKind::函数),
        ("返回", TokenKind::返回),
        ("变量", TokenKind::变量),
        ("常量", TokenKind::常量),
    ];

    for (keyword, expected_kind) in keywords {
        let mut lexer = Lexer::new(keyword.to_string());
        let tokens = lexer.tokenize().expect("Should tokenize keyword");
        
        assert!(tokens.len() >= 1);
        assert_eq!(tokens[0].kind, expected_kind, "Keyword {} should tokenize correctly", keyword);
    }

    println!("✓ All Chinese keywords are recognized");
}
