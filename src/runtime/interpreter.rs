//! Qi AST Interpreter
//!
//! Provides a lightweight interpreter for executing Qi programs directly from
//! their parsed AST representation. This enables the compiler pipeline to run
//! programs end-to-end without requiring native code generation.

use std::collections::HashMap;

use crate::parser::ast::*;
use crate::runtime::{RuntimeError, RuntimeResult};

/// Runtime value representation
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    整数(i64),
    浮点数(f64),
    布尔(bool),
    字符串(String),
    空,
}

impl Value {
    fn truthy(&self) -> bool {
        match self {
            Value::整数(v) => *v != 0,
            Value::浮点数(v) => *v != 0.0,
            Value::布尔(v) => *v,
            Value::字符串(s) => !s.is_empty(),
            Value::空 => false,
        }
    }

    fn as_i64(&self) -> RuntimeResult<i64> {
        match self {
            Value::整数(v) => Ok(*v),
            Value::浮点数(v) => Ok(*v as i64),
            Value::布尔(v) => Ok(if *v { 1 } else { 0 }),
            Value::字符串(_) => Err(RuntimeError::program_execution_error(
                "无法将字符串转换为整数",
                "返回值必须是整数或布尔值",
            )),
            Value::空 => Ok(0),
        }
    }

    fn to_string(&self) -> String {
        match self {
            Value::整数(v) => v.to_string(),
            Value::浮点数(v) => v.to_string(),
            Value::布尔(v) => if *v { "真" } else { "假" }.to_string(),
            Value::字符串(s) => s.clone(),
            Value::空 => "空".to_string(),
        }
    }

    fn from_literal(literal: &LiteralValue) -> Self {
        match literal {
            LiteralValue::整数(v) => Value::整数(*v),
            LiteralValue::浮点数(v) => Value::浮点数(*v),
            LiteralValue::字符串(s) => Value::字符串(s.clone()),
            LiteralValue::布尔(v) => Value::布尔(*v),
            LiteralValue::字符(c) => Value::字符串(c.to_string()),
        }
    }
}

/// Variable entry tracked in scope
#[derive(Debug, Clone)]
struct VariableEntry {
    value: Value,
    mutable: bool,
}

impl VariableEntry {
    fn new(value: Value, mutable: bool) -> Self {
        Self { value, mutable }
    }
}

/// Lexical scope stack
#[derive(Debug, Default)]
struct ScopeStack {
    frames: Vec<HashMap<String, VariableEntry>>,
}

impl ScopeStack {
    fn new() -> Self {
        Self {
            frames: vec![HashMap::new()],
        }
    }

    fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.frames.pop();
    }

    fn declare(&mut self, name: &str, value: Value, mutable: bool) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_string(), VariableEntry::new(value, mutable));
        }
    }

    fn assign(&mut self, name: &str, value: Value) -> RuntimeResult<()> {
        for frame in self.frames.iter_mut().rev() {
            if let Some(entry) = frame.get_mut(name) {
                if !entry.mutable {
                    return Err(RuntimeError::program_execution_error(
                        format!("变量 '{}' 是常量，无法重新赋值", name),
                        format!("变量 '{}' 是常量", name),
                    ));
                }
                entry.value = value;
                return Ok(());
            }
        }

        Err(RuntimeError::program_execution_error(
            format!("变量 '{}' 未声明", name),
            format!("变量 '{}' 未声明", name),
        ))
    }

    fn get(&self, name: &str) -> Option<&Value> {
        for frame in self.frames.iter().rev() {
            if let Some(entry) = frame.get(name) {
                return Some(&entry.value);
            }
        }
        None
    }
}

/// Control flow result produced by statement execution
#[derive(Debug, Clone)]
enum ControlFlow {
    None,
    Return(Value),
    Break,
    Continue,
}

/// Qi AST interpreter implementation
#[derive(Debug, Default)]
pub struct Interpreter {
    functions: HashMap<String, FunctionDeclaration>,
}

impl Interpreter {
    /// Create a new interpreter
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    /// Execute an entire program and return the exit code
    pub fn execute(&mut self, program: &Program) -> RuntimeResult<i64> {
        let mut scope = ScopeStack::new();

        // Load functions and execute top-level statements
        for statement in &program.statements {
            match statement {
                AstNode::函数声明(func) => {
                    self.functions.insert(func.name.clone(), func.clone());
                }
                _ => {
                    if let ControlFlow::Return(value) = self.execute_statement(statement, &mut scope)? {
                        return value.as_i64();
                    }
                }
            }
        }

        // Invoke 主函数 or main
        let exit_value = if self.functions.contains_key("主函数") {
            self.call_function("主函数", vec![], &mut scope)?
        } else if self.functions.contains_key("main") {
            self.call_function("main", vec![], &mut scope)?
        } else {
            Value::整数(0)
        };

        exit_value.as_i64()
    }

    fn call_function(
        &self,
        name: &str,
        args: Vec<Value>,
        scope: &mut ScopeStack,
    ) -> RuntimeResult<Value> {
        let function = self
            .functions
            .get(name)
            .ok_or_else(|| RuntimeError::program_execution_error(
                format!("函数 '{}' 未定义", name),
                format!("函数 '{}' 未定义", name),
            ))?
            .clone();

        if function.parameters.len() != args.len() {
            return Err(RuntimeError::program_execution_error(
                format!("函数 '{}' 参数数量不匹配", name),
                format!("函数 '{}' 参数数量不匹配", name),
            ));
        }

        scope.push();
        for (param, arg) in function.parameters.iter().zip(args.into_iter()) {
            scope.declare(&param.name, arg, true);
        }

        let result = self.execute_block(&function.body, scope)?;
        scope.pop();

        Ok(result.unwrap_or(Value::整数(0)))
    }

    fn execute_block(
        &self,
        statements: &[AstNode],
        scope: &mut ScopeStack,
    ) -> RuntimeResult<Option<Value>> {
        scope.push();
        for statement in statements {
            match self.execute_statement(statement, scope)? {
                ControlFlow::None => {}
                ControlFlow::Return(value) => {
                    scope.pop();
                    return Ok(Some(value));
                }
                ControlFlow::Break => {
                    scope.pop();
                    return Ok(None);
                }
                ControlFlow::Continue => {
                    // Continue is handled by loop constructs
                }
            }
        }
        scope.pop();
        Ok(None)
    }

    fn execute_statement(
        &self,
        statement: &AstNode,
        scope: &mut ScopeStack,
    ) -> RuntimeResult<ControlFlow> {
        match statement {
            AstNode::变量声明(decl) => {
                let value = if let Some(initializer) = &decl.initializer {
                    self.evaluate_expression(initializer, scope)?
                } else {
                    Value::空
                };
                scope.declare(&decl.name, value, decl.is_mutable);
                Ok(ControlFlow::None)
            }
            AstNode::表达式语句(expr_stmt) => {
                self.evaluate_expression(&expr_stmt.expression, scope)?;
                Ok(ControlFlow::None)
            }
            AstNode::返回语句(return_stmt) => {
                let value = if let Some(expr) = &return_stmt.value {
                    self.evaluate_expression(expr, scope)?
                } else {
                    Value::整数(0)
                };
                Ok(ControlFlow::Return(value))
            }
            AstNode::块语句(block) => {
                let result = self.execute_block(&block.statements, scope)?;
                Ok(result.map(ControlFlow::Return).unwrap_or(ControlFlow::None))
            }
            AstNode::如果语句(if_stmt) => {
                let condition = self.evaluate_expression(&if_stmt.condition, scope)?;
                if condition.truthy() {
                    if let Some(result) = self.execute_block(&if_stmt.then_branch, scope)? {
                        Ok(ControlFlow::Return(result))
                    } else {
                        Ok(ControlFlow::None)
                    }
                } else if let Some(else_branch) = &if_stmt.else_branch {
                    match else_branch.as_ref() {
                        AstNode::块语句(block) => {
                            if let Some(result) = self.execute_block(&block.statements, scope)? {
                                Ok(ControlFlow::Return(result))
                            } else {
                                Ok(ControlFlow::None)
                            }
                        }
                        other => self.execute_statement(other, scope),
                    }
                } else {
                    Ok(ControlFlow::None)
                }
            }
            AstNode::当语句(while_stmt) => {
                loop {
                    let condition = self.evaluate_expression(&while_stmt.condition, scope)?;
                    if !condition.truthy() {
                        break;
                    }

                    match self.execute_block(&while_stmt.body, scope)? {
                        Some(value) => return Ok(ControlFlow::Return(value)),
                        None => {}
                    }
                }
                Ok(ControlFlow::None)
            }
            AstNode::循环语句(loop_stmt) => {
                loop {
                    match self.execute_block(&loop_stmt.body, scope)? {
                        Some(value) => return Ok(ControlFlow::Return(value)),
                        None => {}
                    }
                }
            }
            AstNode::对于语句(_) => Err(RuntimeError::program_execution_error(
                "暂不支持 '对于' 循环",
                "当前解释器暂不支持 '对于' 循环",
            )),
            _ => Ok(ControlFlow::None),
        }
    }

    fn evaluate_expression(
        &self,
        expression: &AstNode,
        scope: &mut ScopeStack,
    ) -> RuntimeResult<Value> {
        match expression {
            AstNode::字面量表达式(literal) => Ok(Value::from_literal(&literal.value)),
            AstNode::标识符表达式(ident) => scope
                .get(&ident.name)
                .cloned()
                .ok_or_else(|| RuntimeError::program_execution_error(
                    format!("变量 '{}' 未声明", ident.name),
                    format!("变量 '{}' 未声明", ident.name),
                )),
            AstNode::二元操作表达式(binary) => {
                let left = self.evaluate_expression(&binary.left, scope)?;
                let right = self.evaluate_expression(&binary.right, scope)?;
                self.evaluate_binary(binary.operator, left, right)
            }
            AstNode::赋值表达式(assign) => {
                let value = self.evaluate_expression(&assign.value, scope)?;
                match assign.target.as_ref() {
                    AstNode::标识符表达式(ident) => {
                        scope.assign(&ident.name, value.clone())?;
                        Ok(value)
                    }
                    _ => Err(RuntimeError::program_execution_error(
                        "暂不支持复杂的赋值目标",
                        "当前解释器暂不支持复杂的赋值表达式",
                    )),
                }
            }
            AstNode::函数调用表达式(call) => {
                let mut args = Vec::with_capacity(call.arguments.len());
                for arg in &call.arguments {
                    args.push(self.evaluate_expression(arg, scope)?);
                }
                self.call_function(&call.callee, args, scope)
            }
            AstNode::字符串连接表达式(concat) => {
                let left = self.evaluate_expression(&concat.left, scope)?;
                let right = self.evaluate_expression(&concat.right, scope)?;
                Ok(Value::字符串(format!("{}{}", left.to_string(), right.to_string())))
            }
            AstNode::数组字面量表达式(array) => {
                let mut elements = Vec::new();
                for element in &array.elements {
                    elements.push(self.evaluate_expression(element, scope)?);
                }
                Ok(Value::字符串(
                    elements
                        .into_iter()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                ))
            }
            _ => Err(RuntimeError::program_execution_error(
                format!("暂不支持的表达式: {:?}", expression),
                "当前解释器暂不支持此表达式类型".to_string(),
            )),
        }
    }

    fn evaluate_binary(
        &self,
        operator: BinaryOperator,
        left: Value,
        right: Value,
    ) -> RuntimeResult<Value> {
        use BinaryOperator::*;

        match operator {
            加 => match (left, right) {
                (Value::整数(a), Value::整数(b)) => Ok(Value::整数(a + b)),
                (Value::浮点数(a), Value::浮点数(b)) => Ok(Value::浮点数(a + b)),
                (Value::整数(a), Value::浮点数(b)) => Ok(Value::浮点数(a as f64 + b)),
                (Value::浮点数(a), Value::整数(b)) => Ok(Value::浮点数(a + b as f64)),
                (Value::字符串(a), Value::字符串(b)) => Ok(Value::字符串(format!("{}{}", a, b))),
                (Value::字符串(a), other) => Ok(Value::字符串(format!("{}{}", a, other.to_string()))),
                (other, Value::字符串(b)) => Ok(Value::字符串(format!("{}{}", other.to_string(), b))),
                _ => Err(RuntimeError::program_execution_error(
                    "加法运算仅支持数字或字符串",
                    "加法运算仅支持数字或字符串",
                )),
            },
            减 => match (left, right) {
                (Value::整数(a), Value::整数(b)) => Ok(Value::整数(a - b)),
                (Value::浮点数(a), Value::浮点数(b)) => Ok(Value::浮点数(a - b)),
                (Value::整数(a), Value::浮点数(b)) => Ok(Value::浮点数(a as f64 - b)),
                (Value::浮点数(a), Value::整数(b)) => Ok(Value::浮点数(a - b as f64)),
                _ => Err(RuntimeError::program_execution_error(
                    "减法运算仅支持数字",
                    "减法运算仅支持数字",
                )),
            },
            乘 => match (left, right) {
                (Value::整数(a), Value::整数(b)) => Ok(Value::整数(a * b)),
                (Value::浮点数(a), Value::浮点数(b)) => Ok(Value::浮点数(a * b)),
                (Value::整数(a), Value::浮点数(b)) => Ok(Value::浮点数(a as f64 * b)),
                (Value::浮点数(a), Value::整数(b)) => Ok(Value::浮点数(a * b as f64)),
                _ => Err(RuntimeError::program_execution_error(
                    "乘法运算仅支持数字",
                    "乘法运算仅支持数字",
                )),
            },
            除 => {
                // Check for division by zero
                match &right {
                    Value::整数(0) => {
                        return Err(RuntimeError::program_execution_error(
                            "除数不能为 0",
                            "除法运算时除数不能为 0",
                        ));
                    }
                    Value::浮点数(v) if *v == 0.0 => {
                        return Err(RuntimeError::program_execution_error(
                            "除数不能为 0",
                            "除法运算时除数不能为 0",
                        ));
                    }
                    _ => {}
                }
                
                // Perform division
                match (left, right) {
                    (Value::整数(a), Value::整数(b)) => Ok(Value::整数(a / b)),
                    (Value::浮点数(a), Value::浮点数(b)) => Ok(Value::浮点数(a / b)),
                    (Value::整数(a), Value::浮点数(b)) => Ok(Value::浮点数(a as f64 / b)),
                    (Value::浮点数(a), Value::整数(b)) => Ok(Value::浮点数(a / b as f64)),
                    _ => Err(RuntimeError::program_execution_error(
                        "除法运算仅支持数字",
                        "除法运算仅支持数字",
                    )),
                }
            },
            取余 => match (left, right) {
                (Value::整数(a), Value::整数(b)) => {
                    if b == 0 {
                        Err(RuntimeError::program_execution_error(
                            "取余运算时除数不能为 0",
                            "取余运算时除数不能为 0",
                        ))
                    } else {
                        Ok(Value::整数(a % b))
                    }
                }
                _ => Err(RuntimeError::program_execution_error(
                    "取余运算仅支持整数",
                    "取余运算仅支持整数",
                )),
            },
            等于 => Ok(Value::布尔(left == right)),
            不等于 => Ok(Value::布尔(left != right)),
            大于 => self.compare(left, right, |a, b| a > b),
            小于 => self.compare(left, right, |a, b| a < b),
            大于等于 => self.compare(left, right, |a, b| a >= b),
            小于等于 => self.compare(left, right, |a, b| a <= b),
            与 => {
                let result = left.truthy() && right.truthy();
                Ok(Value::布尔(result))
            }
            或 => {
                let result = left.truthy() || right.truthy();
                Ok(Value::布尔(result))
            }
        }
    }

    fn compare<F>(&self, left: Value, right: Value, op: F) -> RuntimeResult<Value>
    where
        F: Fn(f64, f64) -> bool,
    {
        match (left, right) {
            (Value::整数(a), Value::整数(b)) => Ok(Value::布尔(op(a as f64, b as f64))),
            (Value::浮点数(a), Value::浮点数(b)) => Ok(Value::布尔(op(a, b))),
            (Value::整数(a), Value::浮点数(b)) => Ok(Value::布尔(op(a as f64, b))),
            (Value::浮点数(a), Value::整数(b)) => Ok(Value::布尔(op(a, b as f64))),
            (Value::字符串(a), Value::字符串(b)) => Ok(Value::布尔(op(
                a.chars().count() as f64,
                b.chars().count() as f64,
            ))),
            _ => Err(RuntimeError::program_execution_error(
                "比较运算仅支持数字或字符串",
                "比较运算仅支持数字或字符串",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_program(source: &str) -> RuntimeResult<i64> {
        let parser = crate::parser::Parser::new();
        let program = parser.parse_source(source).map_err(|_| {
            RuntimeError::program_execution_error("解析失败", "程序解析失败")
        })?;

        let mut interpreter = Interpreter::new();
        interpreter.execute(&program)
    }

    #[test]
    fn test_simple_return() {
        let source = "函数 主函数() { 返回 10; }";
        let result = run_program(source).unwrap();
        assert_eq!(result, 10);
    }

    #[test]
    fn test_variable_assignment() {
        let source = r#"
            函数 主函数() {
                变量 x = 5;
                x = x + 10;
                返回 x;
            }
        "#;
        let result = run_program(source).unwrap();
        assert_eq!(result, 15);
    }

    #[test]
    fn test_if_statement() {
        let source = r#"
            函数 主函数() {
                变量 x = 5;
                如果 x > 3 {
                    返回 1;
                }
                返回 0;
            }
        "#;
        let result = run_program(source).unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_while_loop() {
        let source = r#"
            函数 主函数() {
                变量 i = 0;
                变量 sum = 0;
                当 i < 5 {
                    sum = sum + i;
                    i = i + 1;
                }
                返回 sum;
            }
        "#;
        let result = run_program(source).unwrap();
        assert_eq!(result, 10);
    }

    #[test]
    fn test_recursive_function() {
        let source = r#"
            函数 斐波那契(n) {
                如果 n <= 1 {
                    返回 n;
                }
                返回 斐波那契(n - 1) + 斐波那契(n - 2);
            }

            函数 主函数() {
                返回 斐波那契(5);
            }
        "#;
        let result = run_program(source).unwrap();
        assert_eq!(result, 5);
    }
}
