//! Abstract Syntax Tree definitions for Qi language

use crate::lexer::tokens::Span;

/// AST node types
#[derive(Debug, Clone)]
pub enum AstNode {
    /// Top-level program
    程序(Program),

    // Statements
    变量声明(VariableDeclaration),
    函数声明(FunctionDeclaration),
    结构体声明(StructDeclaration),
    方法声明(MethodDeclaration),
    枚举声明(EnumDeclaration),
    如果语句(IfStatement),
    循环语句(LoopStatement),
    当语句(WhileStatement),
    对于语句(ForStatement),
    返回语句(ReturnStatement),
    表达式语句(ExpressionStatement),
    块语句(BlockStatement),

    // Expressions
    字面量表达式(LiteralExpression),
    标识符表达式(IdentifierExpression),
    二元操作表达式(BinaryExpression),
    函数调用表达式(FunctionCallExpression),
    赋值表达式(AssignmentExpression),
    数组访问表达式(ArrayAccessExpression),
    数组字面量表达式(ArrayLiteralExpression),
    字符串连接表达式(StringConcatExpression),
    结构体实例化表达式(StructLiteralExpression),
    字段访问表达式(FieldAccessExpression),
    方法调用表达式(MethodCallExpression),
}

/// Program node
#[derive(Debug, Clone)]
pub struct Program {
    pub package_name: Option<String>,
    pub imports: Vec<ImportStatement>,
    pub statements: Vec<AstNode>,
    pub source_span: Span,
}

/// Import statement
#[derive(Debug, Clone)]
pub struct ImportStatement {
    pub module_path: String,
    pub alias: Option<String>,
    pub span: Span,
}

/// Variable declaration
#[derive(Debug, Clone)]
pub struct VariableDeclaration {
    pub name: String,
    pub type_annotation: Option<TypeNode>,
    pub initializer: Option<Box<AstNode>>,
    pub is_mutable: bool,
    pub span: Span,
}

/// Function declaration
#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeNode>,
    pub body: Vec<AstNode>,
    pub span: Span,
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_annotation: Option<TypeNode>,
    pub span: Span,
}

/// If statement
#[derive(Debug, Clone)]
pub struct IfStatement {
    pub condition: Box<AstNode>,
    pub then_branch: Vec<AstNode>,
    pub else_branch: Option<Box<AstNode>>,
    pub span: Span,
}

/// Block statement
#[derive(Debug, Clone)]
pub struct BlockStatement {
    pub statements: Vec<AstNode>,
    pub span: Span,
}

/// Loop statement
#[derive(Debug, Clone)]
pub struct LoopStatement {
    pub body: Vec<AstNode>,
    pub span: Span,
}

/// While statement
#[derive(Debug, Clone)]
pub struct WhileStatement {
    pub condition: Box<AstNode>,
    pub body: Vec<AstNode>,
    pub span: Span,
}

/// For statement
#[derive(Debug, Clone)]
pub struct ForStatement {
    pub variable: String,
    pub range: Box<AstNode>,
    pub body: Vec<AstNode>,
    pub span: Span,
}

/// Return statement
#[derive(Debug, Clone)]
pub struct ReturnStatement {
    pub value: Option<Box<AstNode>>,
    pub span: Span,
}

/// Expression statement
#[derive(Debug, Clone)]
pub struct ExpressionStatement {
    pub expression: Box<AstNode>,
    pub span: Span,
}

/// Literal expression
#[derive(Debug, Clone)]
pub struct LiteralExpression {
    pub value: LiteralValue,
    pub span: Span,
}

/// Literal values
#[derive(Debug, Clone)]
pub enum LiteralValue {
    整数(i64),
    浮点数(f64),
    字符串(String),
    布尔(bool),
    字符(char),
}

/// Identifier expression
#[derive(Debug, Clone)]
pub struct IdentifierExpression {
    pub name: String,
    pub span: Span,
}

/// Binary expression
#[derive(Debug, Clone)]
pub struct BinaryExpression {
    pub left: Box<AstNode>,
    pub operator: BinaryOperator,
    pub right: Box<AstNode>,
    pub span: Span,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    加, 减, 乘, 除, 取余,
    等于, 不等于, 大于, 小于, 大于等于, 小于等于,
    与, 或,
}

/// Function call expression
#[derive(Debug, Clone)]
pub struct FunctionCallExpression {
    pub callee: String,
    pub arguments: Vec<AstNode>,
    pub span: Span,
}

/// Method call expression (e.g., obj.method(args))
#[derive(Debug, Clone)]
pub struct MethodCallExpression {
    pub object: Box<AstNode>,
    pub method_name: String,
    pub arguments: Vec<AstNode>,
    pub span: Span,
}

/// Assignment expression
#[derive(Debug, Clone)]
pub struct AssignmentExpression {
    pub target: Box<AstNode>,  // Changed from String to Box<AstNode> to support complex LValues
    pub value: Box<AstNode>,
    pub span: Span,
}

/// Type node
#[derive(Debug, Clone, PartialEq)]
pub enum TypeNode {
    基础类型(BasicType),
    函数类型(FunctionType),
    数组类型(ArrayType),
    结构体类型(StructType),
    枚举类型(EnumType),
    自定义类型(String), // 引用已定义的自定义类型(结构体或枚举)
}

/// Basic types
#[derive(Debug, Clone, PartialEq)]
pub enum BasicType {
    整数,
    浮点数,
    布尔,
    字符,
    字符串,
    空,
}

/// Function type
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionType {
    pub parameters: Vec<TypeNode>,
    pub return_type: Box<TypeNode>,
}

/// Array type
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayType {
    pub element_type: Box<TypeNode>,
    pub size: Option<usize>,
}

/// Array access expression (e.g., array[index])
#[derive(Debug, Clone)]
pub struct ArrayAccessExpression {
    pub array: Box<AstNode>,
    pub index: Box<AstNode>,
    pub span: Span,
}

/// Array literal expression (e.g., [1, 2, 3])
#[derive(Debug, Clone)]
pub struct ArrayLiteralExpression {
    pub elements: Vec<AstNode>,
    pub span: Span,
}

/// String concatenation expression (e.g., "hello" + " world")
#[derive(Debug, Clone)]
pub struct StringConcatExpression {
    pub left: Box<AstNode>,
    pub right: Box<AstNode>,
    pub span: Span,
}

/// Struct declaration
#[derive(Debug, Clone)]
pub struct StructDeclaration {
    pub name: String,
    pub fields: Vec<StructField>,
    pub methods: Vec<MethodDeclaration>,
    pub span: Span,
}

/// Struct field definition
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub type_annotation: TypeNode,
    pub is_embedded: bool, // 支持嵌入字段（类似Go的匿名字段）
    pub span: Span,
}

/// Method declaration (associated with a struct)
#[derive(Debug, Clone)]
pub struct MethodDeclaration {
    pub receiver_name: String,        // 接收者变量名，如 "自己"
    pub receiver_type: String,        // 接收者类型名
    pub is_receiver_mutable: bool,    // 接收者是否可变
    pub method_name: String,          // 方法名
    pub parameters: Vec<Parameter>,   // 方法参数
    pub return_type: Option<TypeNode>, // 返回类型
    pub body: Vec<AstNode>,           // 方法体
    pub span: Span,
}

/// Enum declaration
#[derive(Debug, Clone)]
pub struct EnumDeclaration {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// Enum variant definition
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub value: Option<i64>, // Optional explicit value
    pub span: Span,
}

/// Struct type
#[derive(Debug, Clone, PartialEq)]
pub struct StructType {
    pub name: String,
    pub fields: Vec<StructField>,
    pub methods: Vec<String>, // 方法名列表
}

/// Enum type
#[derive(Debug, Clone, PartialEq)]
pub struct EnumType {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

/// Struct literal expression (e.g., Point { x: 1, y: 2 })
#[derive(Debug, Clone)]
pub struct StructLiteralExpression {
    pub struct_name: String,
    pub fields: Vec<StructFieldValue>,
    pub span: Span,
}

/// Struct field value in literal
#[derive(Debug, Clone)]
pub struct StructFieldValue {
    pub name: String,
    pub value: Box<AstNode>,
    pub span: Span,
}

/// Field access expression (e.g., point.x)
#[derive(Debug, Clone)]
pub struct FieldAccessExpression {
    pub object: Box<AstNode>,
    pub field: String,
    pub span: Span,
}