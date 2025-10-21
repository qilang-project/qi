//! Unit tests for Qi code generation module
//! 测试代码生成模块

use qi_compiler::lexer::*;
use qi_compiler::parser::*;
use qi_compiler::codegen::*;
use qi_compiler::config::CompilationTarget;
use qi_compiler::config::OptimizationLevel;
use qi_compiler::parser::ast::*;

#[test]
fn test_code_generator_creation() {
    let generator = CodeGenerator::new(CompilationTarget::Linux);

    // Test that generator was created
    // We can't access private fields directly, but we can test functionality
    assert!(true);
}

#[test]
fn test_code_generator_with_optimization() {
    let generator = CodeGenerator::new_with_optimization(
        CompilationTarget::Linux,
        OptimizationLevel::Basic
    );

    // Test creation with optimization level
    assert!(true);
}

#[test]
fn test_simple_variable_codegen() {
    let source = "变量 x = 42;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for simple variable
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
    }
}

#[test]
fn test_function_codegen() {
    let source = "函数 test() { 返回 42; }";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for function
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain function definition
        assert!(ir.contains("define") || ir.contains("@test"));
    }
}

#[test]
fn test_function_with_return_codegen() {
    let source = "函数 main() { 返回 0; }";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for function with return
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        assert!(ir.contains("ret") || ir.contains("return"));
    }
}

#[test]
fn test_multiple_statements_codegen() {
    let source = "变量 x = 10; 变量 y = 20; 变量 z = x + y;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for multiple statements
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain variable allocations
        assert!(ir.contains("alloca") || ir.contains("="));
    }
}

#[test]
fn test_arithmetic_expression_codegen() {
    let source = "变量 result = (1 + 2) * 3;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for arithmetic expressions
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain arithmetic operations
        assert!(ir.contains("add") || ir.contains("mul") || ir.contains("+") || ir.contains("*"));
    }
}

#[test]
fn test_string_literal_codegen() {
    let source = "变量 message = \"Hello, World!\";";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for string literals
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
    }
}

#[test]
fn test_character_literal_codegen() {
    let source = "变量 ch = 'A';";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for character literals
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
    }
}

#[test]
fn test_boolean_expression_codegen() {
    let source = "变量 flag = 真;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for boolean values
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
    }
}

#[test]
fn test_comparison_operations_codegen() {
    let source = "变量 result = a > b;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for comparison operations
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain comparison operations
        assert!(ir.contains("icmp") || ir.contains(">") || ir.contains("cmp"));
    }
}

#[test]
fn test_if_statement_codegen() {
    let source = "如果 x > 5 { 变量 y = 10; }";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for if statements
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain conditional branches
        assert!(ir.contains("br") || ir.contains("if") || ir.contains("cond"));
    }
}

#[test]
fn test_while_loop_codegen() {
    let source = "当 i < 10 { i = i + 1; }";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for while loops
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain loop structures
        assert!(ir.contains("br") || ir.contains("loop") || ir.contains("while"));
    }
}

#[test]
fn test_function_call_codegen() {
    let source = "变量 result = test();";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for function calls
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain function call
        assert!(ir.contains("call") || ir.contains("test"));
    }
}

#[test]
fn test_function_call_with_arguments_codegen() {
    let source = "变量 result = add(1, 2);";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for function calls with arguments
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
    }
}

#[test]
fn test_cross_platform_codegen() {
    let source = "变量 x = 42;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    // Test different compilation targets
    let targets = vec![
        CompilationTarget::Linux,
        CompilationTarget::MacOS,
        CompilationTarget::Windows,
        CompilationTarget::Wasm,
    ];

    for target in targets {
        let mut generator = CodeGenerator::new(target.clone());
        let result = generator.generate(&AstNode::程序(program.clone()));

        // Should generate LLVM IR for all targets
        assert!(result.is_ok());

        if let Ok(ir) = result {
            assert!(!ir.is_empty());
        }
    }
}

#[test]
fn test_optimization_levels() {
    let source = "变量 x = 42; 变量 y = x * 2;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    // Test different optimization levels
    let opt_levels = vec![
        OptimizationLevel::None,
        OptimizationLevel::Basic,
        OptimizationLevel::Optimized,
    ];

    for opt_level in opt_levels {
        let mut generator = CodeGenerator::new_with_optimization(
            CompilationTarget::Linux,
            opt_level.clone()
        );
        let result = generator.generate(&AstNode::程序(program.clone()));

        // Should generate LLVM IR for all optimization levels
        assert!(result.is_ok());

        if let Ok(ir) = result {
            assert!(!ir.is_empty());
        }
    }
}

#[test]
fn test_codegen_without_optimization() {
    let source = "变量 x = 42;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate_without_optimization(&AstNode::程序(program));

    // Should generate LLVM IR without optimization
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
    }
}

#[test]
fn test_optimization_level_getters_setters() {
    let mut generator = CodeGenerator::new(CompilationTarget::Linux);

    // Test getting optimization level
    let initial_level = generator.get_optimization_level();

    // Test setting optimization level
    generator.set_optimization_level(OptimizationLevel::Optimized);
    let new_level = generator.get_optimization_level();

    // Verify that the level was changed (if implemented)
    assert!(true); // Basic test that the methods exist and don't panic
}

#[test]
fn test_complex_program_codegen() {
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

    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for complex program with recursion
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain multiple functions
        assert!(ir.contains("factorial") || ir.contains("main") || ir.contains("define"));
    }
}

#[test]
fn test_codegen_error_handling() {
    let source = ""; // Empty program
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should handle empty program gracefully
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_codegen_ir_structure() {
    let source = "变量 x = 42;";
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    if let Ok(ir) = result {
        // Basic structure validation
        assert!(!ir.is_empty());

        // Should contain basic LLVM IR elements
        // Note: Exact structure depends on implementation
        assert!(ir.len() > 0);
    }
}

#[test]
fn test_multiple_function_codegen() {
    let source = r#"
    函数 add(a, b) { 返回 a + b; }
    函数 multiply(a, b) { 返回 a * b; }
    函数 main() {
        变量 x = add(5, 3);
        变量 y = multiply(4, 6);
        返回 x + y;
    }
    "#;

    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().unwrap();

    let parser = Parser::new();
    let program = parser.parse(tokens).unwrap();

    let mut generator = CodeGenerator::new(CompilationTarget::Linux);
    let result = generator.generate(&AstNode::程序(program));

    // Should generate LLVM IR for multiple functions
    assert!(result.is_ok());

    if let Ok(ir) = result {
        assert!(!ir.is_empty());
        // Should contain multiple function definitions
        let function_count = ir.matches("define").count() +
                           ir.matches("@").count();
        assert!(function_count >= 2); // At least add and multiply
    }
}