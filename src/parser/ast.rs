//! Abstract Syntax Tree definitions for Qi language

use crate::lexer::tokens::Span;

/// Visibility modifier for declarations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    公开,  // public
    私有,  // private (default)
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::私有
    }
}

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
    跳出语句(BreakStatement),
    继续语句(ContinueStatement),
    表达式语句(ExpressionStatement),
    块语句(BlockStatement),

    // Expressions
    字面量表达式(LiteralExpression),
    标识符表达式(IdentifierExpression),
    二元操作表达式(BinaryExpression),
    函数调用表达式(FunctionCallExpression),
    等待表达式(AwaitExpression),
    协程启动表达式(GoroutineSpawnExpression),
    赋值表达式(AssignmentExpression),
    数组访问表达式(ArrayAccessExpression),
    数组字面量表达式(ArrayLiteralExpression),
    字符串连接表达式(StringConcatExpression),
    结构体实例化表达式(StructLiteralExpression),
    字段访问表达式(FieldAccessExpression),
    方法调用表达式(MethodCallExpression),
    通道创建表达式(ChannelCreateExpression),
    通道发送表达式(ChannelSendExpression),
    通道接收表达式(ChannelReceiveExpression),
    选择表达式(SelectExpression),
    取地址表达式(AddressOfExpression),
    解引用表达式(DereferenceExpression),
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
    pub module_path: Vec<String>,  // Changed to Vec for module paths like "标准库.输入输出"
    pub items: Option<Vec<String>>,  // Optional specific items to import
    pub alias: Option<String>,
    pub is_public: bool,  // 是否是公开导入（重新导出）
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
    pub visibility: Visibility,
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

/// Break statement
#[derive(Debug, Clone)]
pub struct BreakStatement {
    pub span: Span,
}

/// Continue statement
#[derive(Debug, Clone)]
pub struct ContinueStatement {
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
    pub module_qualifier: Option<String>, // 模块前缀，如 "数学" 在 "数学.最大值" 中
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
    字典类型(DictionaryType),
    列表类型(ListType),
    集合类型(SetType),
    通道类型(ChannelType),
    指针类型(PointerType),
    引用类型(ReferenceType),
    未来类型(Box<TypeNode>), // Future<T> - 异步操作的返回类型
    自定义类型(String), // 引用已定义的自定义类型(结构体或枚举)
}

/// Basic types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicType {
    // 数值类型
    整数,      // i32
    长整数,    // i64
    短整数,    // i16
    字节,      // u8
    浮点数,    // f64

    // 逻辑和文本类型
    布尔,      // bool
    字符,      // char
    字符串,    // String
    空,        // void/unit

    // 容器类型
    数组,      // array
    字典,      // map/dict
    列表,      // Vec/List
    集合,      // Set

    // 指针和引用类型
    指针,      // pointer
    引用,      // reference
    可变引用,  // mutable reference
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
    pub visibility: Visibility,
    pub span: Span,
}

/// Struct field definition
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub type_annotation: TypeNode,
    pub is_embedded: bool, // 支持嵌入字段（类似Go的匿名字段）
    pub visibility: Visibility,
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
    pub visibility: Visibility,
    pub span: Span,
}

/// Enum declaration
#[derive(Debug, Clone)]
pub struct EnumDeclaration {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub visibility: Visibility,
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

/// Await expression (e.g., 等待 async_function())
#[derive(Debug, Clone)]
pub struct AwaitExpression {
    pub expression: Box<AstNode>,
    pub span: Span,
}

// Additional type definitions for complete type system support

/// Dictionary type (map/dict)
#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryType {
    pub key_type: Box<TypeNode>,
    pub value_type: Box<TypeNode>,
}

/// List type (Vec/List)
#[derive(Debug, Clone, PartialEq)]
pub struct ListType {
    pub element_type: Box<TypeNode>,
}

/// Set type
#[derive(Debug, Clone, PartialEq)]
pub struct SetType {
    pub element_type: Box<TypeNode>,
}

/// Channel type
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelType {
    pub element_type: Box<TypeNode>,
}

/// Pointer type
#[derive(Debug, Clone, PartialEq)]
pub struct PointerType {
    pub target_type: Box<TypeNode>,
}

/// Reference type
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceType {
    pub target_type: Box<TypeNode>,
    pub is_mutable: bool, // true for 可变引用, false for 引用
}

// ===== 并发语法节点 | Concurrency Syntax Nodes =====

/// Goroutine spawn expression (e.g., 启动 function();)
#[derive(Debug, Clone)]
pub struct GoroutineSpawnExpression {
    pub expression: Box<AstNode>,
    pub span: Span,
}

/// Channel create expression (e.g., 通道<类型>())
#[derive(Debug, Clone)]
pub struct ChannelCreateExpression {
    pub element_type: TypeNode,
    pub capacity: Option<Box<AstNode>>, // Optional buffer capacity
    pub span: Span,
}

/// Channel send expression (e.g., channel <- value)
#[derive(Debug, Clone)]
pub struct ChannelSendExpression {
    pub channel: Box<AstNode>,
    pub value: Box<AstNode>,
    pub span: Span,
}

/// Channel receive expression (e.g., <-channel)
#[derive(Debug, Clone)]
pub struct ChannelReceiveExpression {
    pub channel: Box<AstNode>,
    pub span: Span,
}

/// Select expression (e.g., 选择 { case <-channel: ... })
#[derive(Debug, Clone)]
pub struct SelectExpression {
    pub cases: Vec<SelectCase>,
    pub default_case: Option<SelectCase>,
    pub span: Span,
}

/// Select case (branch in select statement)
#[derive(Debug, Clone)]
pub struct SelectCase {
    pub kind: SelectCaseKind,
    pub body: Vec<AstNode>,
    pub span: Span,
}

/// Address-of expression (&variable)
#[derive(Debug, Clone)]
pub struct AddressOfExpression {
    pub expression: Box<AstNode>,
    pub span: Span,
}

/// Dereference expression (*pointer)
#[derive(Debug, Clone)]
pub struct DereferenceExpression {
    pub expression: Box<AstNode>,
    pub span: Span,
}

/// Select case kinds
#[derive(Debug, Clone)]
pub enum SelectCaseKind {
    /// Channel receive case: case <-channel:
    通道接收 { channel: Box<AstNode>, variable: Option<String> },
    /// Channel send case: case channel <- value:
    通道发送 { channel: Box<AstNode>, value: Box<AstNode> },
    /// Default case: 默认:
    默认,
}