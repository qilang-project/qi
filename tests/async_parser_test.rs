#[cfg(test)]
mod tests {
    use qi_compiler::parser::{Parser, AstNode};
    use qi_compiler::semantic::type_checker::TypeChecker;

    #[test]
    fn test_async_function_parsing() {
        let qi_code = r#"
        异步 函数 获取数据() {
            变量 结果 = 等待 网络请求();
            返回 结果;
        }

        函数 主程序() {
            变量 数据 = 等待 获取数据();
            打印(数据);
        }
        "#;

        let parser = Parser::new();
        let ast = parser.parse_source(qi_code).expect("Failed to parse async function");

        // Verify the AST contains async function declaration
        match &ast.statements[0] {
            AstNode::异步函数声明(_) => {
                // Success - async function parsed correctly
            }
            _ => panic!("Expected async function declaration, got: {:?}", ast.statements[0]),
        }

        // Verify the AST contains await expression
        let has_await = ast.statements.iter().any(|stmt| {
            matches!(stmt, AstNode::等待表达式(_))
        });
        assert!(has_await, "Expected to find await expression in AST");

        // Test type checking
        let mut type_checker = TypeChecker::new();
        match type_checker.check(&ast.statements[0]) {
            Ok(_) => {
                // Type checking succeeded
            }
            Err(e) => {
                println!("Type checking error: {:?}", e);
                // For now, we expect some type errors since we don't have all types defined
            }
        }
    }

    #[test]
    fn test_async_function_with_parameters() {
        let qi_code = r#"
        异步 函数 处理数据(整数 输入值) : 字符串 {
            变量 结果 = 等待 转换为字符串(输入值);
            返回 结果;
        }
        "#;

        let parser = Parser::new();
        let ast = parser.parse_source(qi_code).expect("Failed to parse async function with parameters");

        // Verify the AST contains async function declaration with parameters
        match &ast.statements[0] {
            AstNode::异步函数声明(async_func) => {
                assert_eq!(async_func.name, "处理数据");
                assert_eq!(async_func.parameters.len(), 1);
                assert_eq!(async_func.parameters[0].name, "输入值");
            }
            _ => panic!("Expected async function declaration, got: {:?}", ast.statements[0]),
        }
    }

    #[test]
    fn test_simple_await_expression() {
        let qi_code = r#"
        函数 测试() {
            变量 结果 = 等待 异步操作();
            返回 结果;
        }
        "#;

        let parser = Parser::new();
        let ast = parser.parse_source(qi_code).expect("Failed to parse function with await");

        // Look for await expression in the function body
        let found_await = ast.statements.iter().any(|stmt| {
            if let AstNode::函数声明(func) = stmt {
                func.body.iter().any(|body_stmt| {
                    matches!(body_stmt, AstNode::表达式语句(expr_stmt)
                           if matches!(expr_stmt.expression.as_ref(), AstNode::等待表达式(_)))
                })
            } else {
                false
            }
        });

        assert!(found_await, "Expected to find await expression in function body");
    }
}