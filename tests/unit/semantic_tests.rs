//! 语义分析单元测试
//! Semantic analysis unit tests

use qi_compiler::semantic::*;
use qi_compiler::parser::ast::*;
use qi_compiler::lexer::tokens::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_type_inference() {
        let mut type_checker = TypeChecker::new();

        // 测试整数类型推断
        let int_literal = AstNode::字面量表达式(LiteralExpression {
            value: LiteralValue::整数(42),
            span: Default::default(),
        });

        let inferred_type = type_checker.infer_type(&int_literal);
        assert_eq!(inferred_type, TypeNode::基础类型(BasicType::整数));
    }

    #[test]
    fn test_variable_declaration_type() {
        let mut symbol_table = SymbolTable::new();

        // 创建变量符号
        let var_symbol = Symbol {
            name: "计数器".to_string(),
            kind: SymbolKind::变量,
            type_node: TypeNode::基础类型(BasicType::整数),
            scope_level: 0,
            span: Default::default(),
            is_mutable: true,
        };

        // 添加到符号表
        assert!(symbol_table.define_symbol(var_symbol).is_ok());

        // 查找符号
        let found_symbol = symbol_table.lookup_symbol("计数器");
        assert!(found_symbol.is_some());
        assert_eq!(found_symbol.unwrap().type_node, TypeNode::基础类型(BasicType::整数));
    }

    #[test]
    fn test_type_compatibility() {
        let type_checker = TypeChecker::new();

        let int_type = TypeNode::基础类型(BasicType::整数);
        let string_type = TypeNode::基础类型(BasicType::字符串);

        // 测试相同类型兼容性
        assert!(type_checker.check_type_compatibility(&int_type, &int_type));

        // 测试不同类型不兼容性
        assert!(!type_checker.check_type_compatibility(&int_type, &string_type));
    }

    #[test]
    fn test_scope_management() {
        let mut symbol_table = SymbolTable::new();

        // 在全局作用域定义变量
        let global_var = Symbol {
            name: "全局变量".to_string(),
            kind: SymbolKind::变量,
            type_node: TypeNode::基础类型(BasicType::整数),
            scope_level: 0,
            span: Default::default(),
            is_mutable: true,
        };
        symbol_table.define_symbol(global_var).unwrap();

        // 进入新作用域
        symbol_table.enter_scope();

        // 在局部作用域定义同名变量
        let local_var = Symbol {
            name: "全局变量".to_string(),
            kind: SymbolKind::变量,
            type_node: TypeNode::基础类型(BasicType::字符串),
            scope_level: 1,
            span: Default::default(),
            is_mutable: true,
        };
        symbol_table.define_symbol(local_var).unwrap();

        // 应该找到局部作用域的变量
        let found = symbol_table.lookup_symbol("全局变量").unwrap();
        assert_eq!(found.type_node, TypeNode::基础类型(BasicType::字符串));
        assert_eq!(found.scope_level, 1);

        // 退出作用域
        symbol_table.exit_scope();

        // 应该找到全局作用域的变量
        let found = symbol_table.lookup_symbol("全局变量").unwrap();
        assert_eq!(found.type_node, TypeNode::基础类型(BasicType::整数));
        assert_eq!(found.scope_level, 0);
    }

    #[test]
    fn test_name_conflict_detection() {
        let mut symbol_table = SymbolTable::new();

        // 定义第一个变量
        let var1 = Symbol {
            name: "重复名称".to_string(),
            kind: SymbolKind::变量,
            type_node: TypeNode::基础类型(BasicType::整数),
            scope_level: 0,
            span: Default::default(),
            is_mutable: true,
        };
        symbol_table.define_symbol(var1).unwrap();

        // 尝试定义同名变量 - 应该报错
        let var2 = Symbol {
            name: "重复名称".to_string(),
            kind: SymbolKind::变量,
            type_node: TypeNode::基础类型(BasicType::字符串),
            scope_level: 0,
            span: Default::default(),
            is_mutable: true,
        };

        let result = symbol_table.define_symbol(var2);
        assert!(result.is_err());
        match result.unwrap_err() {
            ScopeError::NameConflict { name, .. } => {
                assert_eq!(name, "重复名称");
            }
            _ => panic!("Expected NameConflict error"),
        }
    }

    #[test]
    fn test_function_type() {
        let param_type = TypeNode::基础类型(BasicType::整数);
        let return_type = TypeNode::基础类型(BasicType::整数);

        let func_type = TypeNode::函数类型(FunctionType {
            parameters: vec![param_type.clone()],
            return_type: Box::new(return_type.clone()),
        });

        // 测试函数类型兼容性
        let other_func_type = TypeNode::函数类型(FunctionType {
            parameters: vec![param_type],
            return_type: Box::new(return_type),
        });

        let type_checker = TypeChecker::new();
        assert!(type_checker.check_type_compatibility(&func_type, &other_func_type));
    }

    #[test]
    fn test_string_operations() {
        let mut type_checker = TypeChecker::new();

        let string_type = TypeNode::基础类型(BasicType::字符串);
        let string_literal = AstNode::字面量表达式(LiteralExpression {
            value: LiteralValue::字符串字面量("测试".to_string()),
            span: Default::default(),
        });

        let inferred_type = type_checker.infer_type(&string_literal);
        assert_eq!(inferred_type, string_type);

        // 测试字符串连接表达式类型
        let concat_expr = AstNode::二元操作表达式(BinaryExpression {
            left: Box::new(string_literal),
            operator: BinaryOperator::加,
            right: Box::new(AstNode::字面量表达式(LiteralExpression {
                value: LiteralValue::字符串字面量("字符串".to_string()),
                span: Default::default(),
            })),
            span: Default::default(),
        });

        let concat_type = type_checker.infer_type(&concat_expr);
        // 字符串连接应该产生字符串类型（如果支持）
        // 这取决于具体的类型规则实现
    }

    #[test]
    fn test_array_type() {
        let element_type = TypeNode::基础类型(BasicType::整数);
        let array_type = TypeNode::数组类型(ArrayType {
            element_type: Box::new(element_type),
            size: Some(5), // 固定大小数组
        });

        // 测试数组索引操作
        let array_var = AstNode::标识符表达式(IdentifierExpression {
            name: "数组".to_string(),
            span: Default::default(),
        });

        let index_expr = AstNode::字面量表达式(LiteralExpression {
            value: LiteralValue::整数(0),
            span: Default::default(),
        });

        // 数组访问表达式的类型应该是元素类型
        // 这需要在具体的类型检查器中实现
    }

    #[test]
    fn test_constant_assignment() {
        let mut symbol_table = SymbolTable::new();

        // 定义常量
        let const_symbol = Symbol {
            name: "常量值".to_string(),
            kind: SymbolKind::常量,
            type_node: TypeNode::基础类型(BasicType::整数),
            scope_level: 0,
            span: Default::default(),
            is_mutable: false,
        };

        symbol_table.define_symbol(const_symbol).unwrap();

        // 检查常量是否为可变
        let found = symbol_table.lookup_symbol("常量值").unwrap();
        assert!(!found.is_mutable);
        assert!(matches!(found.kind, SymbolKind::常量));
    }

    // ===== Function Call Tests =====

    #[test]
    fn test_function_symbol_definition() {
        let mut symbol_table = SymbolTable::new();

        // 定义函数符号
        let func_symbol = Symbol {
            name: "相加".to_string(),
            kind: SymbolKind::函数,
            type_node: TypeNode::函数类型(FunctionType {
                parameters: vec![
                    TypeNode::基础类型(BasicType::整数),
                    TypeNode::基础类型(BasicType::整数),
                ],
                return_type: Box::new(TypeNode::基础类型(BasicType::整数)),
            }),
            scope_level: 0,
            span: Default::default(),
            is_mutable: false,
        };

        assert!(symbol_table.define_symbol(func_symbol).is_ok());

        // 查找函数符号
        let found_symbol = symbol_table.lookup_symbol("相加");
        assert!(found_symbol.is_some());
        assert!(matches!(found_symbol.unwrap().kind, SymbolKind::函数));
    }

    #[test]
    fn test_function_call_type_inference() {
        let mut type_checker = TypeChecker::new();

        // 创建函数调用表达式: add(5, 3)
        let function_call = AstNode::函数调用表达式(FunctionCallExpression {
            callee: "add".to_string(),
            arguments: vec![
                AstNode::字面量表达式(LiteralExpression {
                    value: LiteralValue::整数(5),
                    span: Default::default(),
                }),
                AstNode::字面量表达式(LiteralExpression {
                    value: LiteralValue::整数(3),
                    span: Default::default(),
                }),
            ],
            span: Default::default(),
        });

        // 类型检查函数调用（需要在符号表中查找函数定义）
        // 这里测试类型推断的基本结构
        let inferred_type = type_checker.infer_type(&function_call);
        // 实际实现需要在符号表中查找函数签名
    }

    #[test]
    fn test_function_parameter_types() {
        let mut symbol_table = SymbolTable::new();

        // 进入函数作用域
        symbol_table.enter_scope();

        // 定义函数参数符号
        let param1 = Symbol {
            name: "参数1".to_string(),
            kind: SymbolKind::参数,
            type_node: TypeNode::基础类型(BasicType::整数),
            scope_level: 1,
            span: Default::default(),
            is_mutable: true,
        };

        let param2 = Symbol {
            name: "参数2".to_string(),
            kind: SymbolKind::参数,
            type_node: TypeNode::基础类型(BasicType::字符串),
            scope_level: 1,
            span: Default::default(),
            is_mutable: true,
        };

        assert!(symbol_table.define_symbol(param1).is_ok());
        assert!(symbol_table.define_symbol(param2).is_ok());

        // 验证参数类型
        let found_param1 = symbol_table.lookup_symbol("参数1").unwrap();
        let found_param2 = symbol_table.lookup_symbol("参数2").unwrap();

        assert_eq!(found_param1.type_node, TypeNode::基础类型(BasicType::整数));
        assert_eq!(found_param2.type_node, TypeNode::基础类型(BasicType::字符串));
        assert!(matches!(found_param1.kind, SymbolKind::参数));
        assert!(matches!(found_param2.kind, SymbolKind::参数));

        symbol_table.exit_scope();
    }

    #[test]
    fn test_recursive_function_type() {
        let mut symbol_table = SymbolTable::new();

        // 定义递归函数符号
        let recursive_func = Symbol {
            name: "阶乘".to_string(),
            kind: SymbolKind::函数,
            type_node: TypeNode::函数类型(FunctionType {
                parameters: vec![TypeNode::基础类型(BasicType::整数)],
                return_type: Box::new(TypeNode::基础类型(BasicType::整数)),
            }),
            scope_level: 0,
            span: Default::default(),
            is_mutable: false,
        };

        symbol_table.define_symbol(recursive_func).unwrap();

        // 验证递归函数可以自引用
        let found = symbol_table.lookup_symbol("阶乘");
        assert!(found.is_some());
        assert!(matches!(found.unwrap().kind, SymbolKind::函数));
    }

    #[test]
    fn test_function_overload_detection() {
        let mut symbol_table = SymbolTable::new();

        // 定义第一个函数
        let func1 = Symbol {
            name: "处理".to_string(),
            kind: SymbolKind::函数,
            type_node: TypeNode::函数类型(FunctionType {
                parameters: vec![TypeNode::基础类型(BasicType::整数)],
                return_type: Box::new(TypeNode::基础类型(BasicType::字符串)),
            }),
            scope_level: 0,
            span: Default::default(),
            is_mutable: false,
        };

        symbol_table.define_symbol(func1).unwrap();

        // 尝试定义同名函数（函数重载）
        let func2 = Symbol {
            name: "处理".to_string(),
            kind: SymbolKind::函数,
            type_node: TypeNode::函数类型(FunctionType {
                parameters: vec![TypeNode::基础类型(BasicType::字符串)],
                return_type: Box::new(TypeNode::基础类型(BasicType::字符串)),
            }),
            scope_level: 0,
            span: Default::default(),
            is_mutable: false,
        };

        // 当前实现可能不支持函数重载，应该报错
        let result = symbol_table.define_symbol(func2);
        // 根据实际实现调整这个断言
        assert!(result.is_err()); // 如果不支持重载
    }

    #[test]
    fn test_higher_order_function_types() {
        // 测试高阶函数类型（函数作为参数或返回值）
        let param_func_type = TypeNode::函数类型(FunctionType {
            parameters: vec![TypeNode::基础类型(BasicType::整数)],
            return_type: Box::new(TypeNode::基础类型(BasicType::布尔)),
        });

        let higher_order_func_type = TypeNode::函数类型(FunctionType {
            parameters: vec![param_func_type],
            return_type: Box::new(TypeNode::基础类型(BasicType::整数)),
        });

        // 验证高阶函数类型的结构
        if let TypeNode::函数类型(func_type) = higher_order_func_type {
            assert_eq!(func_type.parameters.len(), 1);
            if let TypeNode::函数类型(param_type) = &func_type.parameters[0] {
                assert_eq!(param_type.parameters.len(), 1);
                assert!(matches!(*param_type.return_type, TypeNode::基础类型(BasicType::布尔)));
            }
        }
    }

    #[test]
    fn test_void_function_type() {
        // 测试无返回值函数类型
        let void_func_type = TypeNode::函数类型(FunctionType {
            parameters: vec![],
            return_type: Box::new(TypeNode::基础类型(BasicType::空)),
        });

        // 验证空返回类型
        if let TypeNode::函数类型(func_type) = void_func_type {
            assert!(func_type.parameters.is_empty());
            assert!(matches!(*func_type.return_type, TypeNode::基础类型(BasicType::空)));
        }
    }
}