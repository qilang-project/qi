//! IR builder for Qi language

use crate::parser::ast::{AstNode, BinaryOperator};

/// 简单分析：函数体里有没有直接 `返回 闭包(...)` 的语句。
/// 用于推断函数是否返回闭包对象（决定调用方是否把结果标为 closure 变量）。
fn function_body_returns_closure(body: &[AstNode]) -> bool {
    for stmt in body {
        if let AstNode::返回语句(ret) = stmt {
            if let Some(val) = &ret.value {
                if matches!(val.as_ref(), AstNode::闭包表达式(_)) {
                    return true;
                }
            }
        }
        // 块嵌套时简化处理 — 实际场景大多是顶层 return；嵌套见到再扩展
    }
    false
}

/// §4-4 await 返回表达式 — 三种 Operand 任意 + / * 组合：
///   - Literal(n)   字面量整数
///   - Awaited      局部变量 = 等待 X() 拿到的值（仅单 await 路径用）
///   - Local(idx)   多 await 路径的第 idx 个 等待 结果
///   - Param(idx)   外层第 idx 个参数
/// AwaitReturn = Single(operand) | BinOp(op, left, right)
#[derive(Debug, Clone, Copy)]
enum AwaitOperand {
    Literal(i64),
    Awaited,      // 单 await 路径
    Local(usize), // 多 await 路径，索引到 K 个 awaited 结果
    Param(usize),
}
#[derive(Debug, Clone, Copy)]
enum AwaitReturn {
    Single(AwaitOperand),
    BinOp(char, AwaitOperand, AwaitOperand),
}

/// §4-4 单 await 形态识别（泛化版）：
///   异步 函数 X(args*: 整数): 未来<整数> {
///       变量 NAME: 整数 = 等待 IDENT(literal_args*);
///       返回 EXPR;     // EXPR 是 字面量 / NAME / 参数 / 任意二元 + / *
///   }
///
/// 命中返回 (local_name, awaited_func_name, awaited_args, return_form)，否则 None。
fn single_await_async_body(
    func: &crate::parser::ast::FunctionDeclaration,
) -> Option<(String, String, Vec<i64>, AwaitReturn)> {
    use crate::parser::ast::{AstNode, BinaryOperator, LiteralValue};
    // 外层参数全 整数
    for p in &func.parameters {
        match &p.type_annotation {
            Some(crate::parser::ast::TypeNode::基础类型(
                crate::parser::ast::BasicType::整数,
            )) => {}
            _ => return None,
        }
    }
    if func.body.len() != 2 {
        return None;
    }
    let AstNode::变量声明(decl) = &func.body[0] else {
        return None;
    };
    let init = decl.initializer.as_ref()?;
    let AstNode::等待表达式(await_expr) = init.as_ref() else {
        return None;
    };
    let AstNode::函数调用表达式(call) = await_expr.expression.as_ref() else {
        return None;
    };
    // awaited args 全字面量
    let mut awaited_args: Vec<i64> = Vec::with_capacity(call.arguments.len());
    for arg in &call.arguments {
        match arg {
            AstNode::字面量表达式(lit) => {
                if let LiteralValue::整数(n) = &lit.value {
                    awaited_args.push(*n);
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    match &decl.type_annotation {
        Some(crate::parser::ast::TypeNode::基础类型(crate::parser::ast::BasicType::整数)) => {
        }
        _ => return None,
    }
    // body[1]: 返回 EXPR
    let AstNode::返回语句(ret) = &func.body[1] else {
        return None;
    };
    let val = ret.value.as_ref()?;

    // 通用 operand 识别
    let to_operand = |n: &AstNode| -> Option<AwaitOperand> {
        // 字面量
        if let AstNode::字面量表达式(l) = n {
            if let LiteralValue::整数(v) = &l.value {
                return Some(AwaitOperand::Literal(*v));
            }
            return None;
        }
        // identifier — awaited 局部 或 外层参数
        if let AstNode::标识符表达式(id) = n {
            if id.name == decl.name {
                return Some(AwaitOperand::Awaited);
            }
            if let Some(idx) = func.parameters.iter().position(|p| p.name == id.name) {
                return Some(AwaitOperand::Param(idx));
            }
            return None;
        }
        None
    };

    let form: AwaitReturn = if let Some(operand) = to_operand(val) {
        AwaitReturn::Single(operand)
    } else if let AstNode::二元操作表达式(bin) = val.as_ref() {
        let op = match bin.operator {
            BinaryOperator::加 => '+',
            BinaryOperator::乘 => '*',
            _ => return None,
        };
        let left = to_operand(&bin.left)?;
        let right = to_operand(&bin.right)?;
        AwaitReturn::BinOp(op, left, right)
    } else {
        return None;
    };
    Some((decl.name.clone(), call.callee.clone(), awaited_args, form))
}

/// §4-4 增量 5：多 await 串联（K ≥ 2 个 等待）：
///   异步 函数 X(args*: 整数): 未来<整数> {
///       变量 NAME_0: 整数 = 等待 IDENT_0(lit_args*);
///       变量 NAME_1: 整数 = 等待 IDENT_1(lit_args*);
///       ...
///       返回 EXPR;   // EXPR 用 Literal/Local(i)/Param(i) 任意 + /*  组合
///   }
///
/// 命中返回 (awaited_calls, locals, return_form)，否则 None。
fn multi_await_async_body(
    func: &crate::parser::ast::FunctionDeclaration,
) -> Option<(Vec<(String, Vec<i64>)>, Vec<String>, AwaitReturn)> {
    use crate::parser::ast::{AstNode, BinaryOperator, LiteralValue};
    // 外层参数全 整数
    for p in &func.parameters {
        match &p.type_annotation {
            Some(crate::parser::ast::TypeNode::基础类型(
                crate::parser::ast::BasicType::整数,
            )) => {}
            _ => return None,
        }
    }
    // body = K 个 变量声明 + 1 个 返回，K ≥ 2
    if func.body.len() < 3 {
        return None;
    }
    let k = func.body.len() - 1;
    let mut awaited_calls: Vec<(String, Vec<i64>)> = Vec::with_capacity(k);
    let mut locals: Vec<String> = Vec::with_capacity(k);
    for i in 0..k {
        let AstNode::变量声明(decl) = &func.body[i] else {
            return None;
        };
        let init = decl.initializer.as_ref()?;
        let AstNode::等待表达式(await_expr) = init.as_ref() else {
            return None;
        };
        let AstNode::函数调用表达式(call) = await_expr.expression.as_ref() else {
            return None;
        };
        match &decl.type_annotation {
            Some(crate::parser::ast::TypeNode::基础类型(
                crate::parser::ast::BasicType::整数,
            )) => {}
            _ => return None,
        }
        // awaited args 全字面量
        let mut a_args: Vec<i64> = Vec::with_capacity(call.arguments.len());
        for arg in &call.arguments {
            match arg {
                AstNode::字面量表达式(lit) => {
                    if let LiteralValue::整数(n) = &lit.value {
                        a_args.push(*n);
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        awaited_calls.push((call.callee.clone(), a_args));
        locals.push(decl.name.clone());
    }
    // 最后一条必须是 返回
    let AstNode::返回语句(ret) = &func.body[k] else {
        return None;
    };
    let val = ret.value.as_ref()?;

    // operand 识别（Local(i) 走 locals 列表查找）
    let to_operand = |n: &AstNode| -> Option<AwaitOperand> {
        if let AstNode::字面量表达式(l) = n {
            if let LiteralValue::整数(v) = &l.value {
                return Some(AwaitOperand::Literal(*v));
            }
            return None;
        }
        if let AstNode::标识符表达式(id) = n {
            // 先看 locals
            if let Some(idx) = locals.iter().position(|n| *n == id.name) {
                return Some(AwaitOperand::Local(idx));
            }
            // 再看外层 params
            if let Some(idx) = func.parameters.iter().position(|p| p.name == id.name) {
                return Some(AwaitOperand::Param(idx));
            }
            return None;
        }
        None
    };

    let form: AwaitReturn = if let Some(operand) = to_operand(val) {
        AwaitReturn::Single(operand)
    } else if let AstNode::二元操作表达式(bin) = val.as_ref() {
        let op = match bin.operator {
            BinaryOperator::加 => '+',
            BinaryOperator::乘 => '*',
            _ => return None,
        };
        let left = to_operand(&bin.left)?;
        let right = to_operand(&bin.right)?;
        AwaitReturn::BinOp(op, left, right)
    } else {
        return None;
    };
    Some((awaited_calls, locals, form))
}

/// §4-3 状态机 MVP 返回形态识别（不限参数，仅看 body 形状）。
/// 下面四种都支持：
///   - `返回 字面量整数`         AsyncReturn::Literal(n)
///   - `返回 单一参数名`         AsyncReturn::Param(idx)  — 透传第 idx 个参数
///   - `返回 单一参数 + 字面量`  AsyncReturn::ParamAddLit(idx, n)
///   - `返回 单一参数 * 字面量`  AsyncReturn::ParamMulLit(idx, n)
/// 其他形态返回 None — fall through 到老 sync wrap 路径。
///
/// 下次会话扩展：
/// - §4-4 加 等待 翻译（多 state）
/// - 局部变量 + if/loop / 多 返回 路径
/// - 任意表达式（复用 build_node 在 poll 上下文）
#[derive(Debug, Clone, Copy)]
enum AsyncReturn {
    Literal(i64),
    Param(usize),
    ParamAddLit(usize, i64),
    ParamMulLit(usize, i64),
}

fn trivial_async_int_body(func: &crate::parser::ast::FunctionDeclaration) -> Option<AsyncReturn> {
    use crate::parser::ast::{AstNode, BinaryOperator, LiteralValue};
    // 参数全 i64 才支持（混合类型 frame 布局复杂，留给 §4-4）
    for p in &func.parameters {
        match &p.type_annotation {
            Some(crate::parser::ast::TypeNode::基础类型(
                crate::parser::ast::BasicType::整数,
            )) => {}
            _ => return None,
        }
    }
    if func.body.len() != 1 {
        return None;
    }
    let AstNode::返回语句(ret) = &func.body[0] else {
        return None;
    };
    let value = ret.value.as_ref()?;

    // 1. 字面量
    if let AstNode::字面量表达式(lit) = value.as_ref() {
        if let LiteralValue::整数(n) = &lit.value {
            return Some(AsyncReturn::Literal(*n));
        }
    }
    // 2. 单一参数名
    if let AstNode::标识符表达式(id) = value.as_ref() {
        let idx = func.parameters.iter().position(|p| p.name == id.name)?;
        return Some(AsyncReturn::Param(idx));
    }
    // 3. 二元 (param, literal) 或 (literal, param) — 仅 + 和 *
    if let AstNode::二元操作表达式(bin) = value.as_ref() {
        let op = match bin.operator {
            BinaryOperator::加 => '+',
            BinaryOperator::乘 => '*',
            _ => return None,
        };
        let extract_param_idx = |n: &AstNode| -> Option<usize> {
            if let AstNode::标识符表达式(id) = n {
                func.parameters.iter().position(|p| p.name == id.name)
            } else {
                None
            }
        };
        let extract_lit = |n: &AstNode| -> Option<i64> {
            if let AstNode::字面量表达式(l) = n {
                if let LiteralValue::整数(v) = &l.value {
                    return Some(*v);
                }
            }
            None
        };
        // (param, literal)
        if let (Some(idx), Some(lit)) = (extract_param_idx(&bin.left), extract_lit(&bin.right)) {
            return Some(if op == '+' {
                AsyncReturn::ParamAddLit(idx, lit)
            } else {
                AsyncReturn::ParamMulLit(idx, lit)
            });
        }
        // (literal, param)
        if let (Some(lit), Some(idx)) = (extract_lit(&bin.left), extract_param_idx(&bin.right)) {
            return Some(if op == '+' {
                AsyncReturn::ParamAddLit(idx, lit)
            } else {
                AsyncReturn::ParamMulLit(idx, lit)
            });
        }
    }
    None
}
use super::module_registry::{ModuleFunction, ModuleRegistry};

/// IR instruction
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum IrInstruction {
    /// Allocate a variable
    分配 { dest: String, type_name: String },

    /// Global variable declaration
    全局变量声明 {
        name: String,
        type_name: String,
        initializer: Option<String>,
        is_constant: bool,
    },

    /// Store a value
    存储 {
        target: String,
        value: String,
        value_type: Option<String>,
    },

    /// Load a value
    加载 {
        dest: String,
        source: String,
        load_type: Option<String>, // Explicit type to load
    },

    /// Binary operation
    二元操作 {
        dest: String,
        left: String,
        operator: BinaryOperator,
        right: String,
        operand_type: String, // "i64", "double", "i1", etc. - the type of left and right operands
    },

    /// Function call
    函数调用 {
        dest: Option<String>,
        callee: String,
        arguments: Vec<String>,
    },

    /// Return from function
    返回 { value: Option<String> },

    /// Jump to label
    跳转 { label: String },

    /// Conditional jump
    条件跳转 {
        condition: String,
        true_label: String,
        false_label: String,
    },

    /// String constant
    字符串常量 { name: String },

    /// Integer constant
    整数常量 { dest: String, value: i64 },

    /// Boolean constant
    布尔常量 {
        dest: String,
        value: i8, // Use i8 to represent 0 or 1
    },

    /// Float constant
    浮点数常量 { dest: String, value: f64 },

    /// Label
    标签 { name: String },

    /// Array access (getelementptr)
    数组访问 {
        dest: String,
        array: String,
        index: String,
    },

    /// Array allocation
    数组分配 {
        dest: String,
        size: String,
        element_type: String, // "i64", "double", etc.
    },

    /// Array store
    数组存储 {
        array: String,
        index: String,
        value: String,
        element_type: String, // "i64", "double", etc.
    },

    /// String concatenation
    字符串连接 {
        dest: String,
        left: String,
        right: String,
    },

    /// XOR operation (for logical not)
    异或 {
        dest: String,
        left: String,
        right: String,
    },

    /// Type conversion/casting
    类型转换 {
        dest: String,
        value: String,
        from_type: String,
        to_type: String,
        cast_type: String, // sitofp, fptosi, trunc, sext, zext, etc.
    },

    /// Unreachable instruction (for dead code paths like after infinite loops)
    不可达,

    /// Field access (getelementptr for struct fields)
    字段访问 {
        dest: String,
        object: String,
        field: String,
        struct_type: String, // The struct type name (e.g., "点")
    },

    /// Await expression
    等待表达式 { dest: String, future: String },

    /// Create async task
    创建异步任务 {
        dest: String,
        function: String,
        arguments: Vec<String>,
    },

    /// Spawn goroutine
    协程启动 {
        function: String,
        arguments: Vec<String>,
    },

    /// Create channel
    创建通道 {
        dest: String,
        channel_type: String,
        buffer_size: Option<String>,
    },

    /// Send to channel
    通道发送 { channel: String, value: String },

    /// Receive from channel
    通道接收 { dest: String, channel: String },

    /// Select statement
    选择语句 {
        cases: Vec<SelectCase>,
        default_case: Option<String>,
    },
}

/// Select case for channel operations
#[derive(Debug, Clone)]
pub struct SelectCase {
    pub operation_type: SelectOperationType,
    pub channel: String,
    pub value: Option<String>, // For send operations
    pub dest: Option<String>,  // For receive operations
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum SelectOperationType {
    接收, // Receive
    发送, // Send
}

/// Memory allocation target (stack or heap)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationTarget {
    /// Stack allocation: small local variables, clear lifetime
    Stack,
    /// Heap allocation: large objects, escaping objects, dynamic size
    Heap,
}

/// Information about a memory allocation
#[derive(Debug, Clone)]
pub struct AllocationInfo {
    /// LLVM temporary variable name
    pub ptr: String,
    /// Allocation size in bytes
    pub size: usize,
    /// Type name
    pub type_name: String,
    /// Scope depth level
    pub scope_level: usize,
    /// Whether this is a heap allocation
    pub is_heap: bool,
}

/// IR builder
pub struct IrBuilder {
    instructions: Vec<IrInstruction>,
    temp_counter: usize,
    label_counter: usize,
    /// Track variable types for better code generation
    variable_types: std::collections::HashMap<String, String>,
    /// Map user-facing identifier → internal unique LLVM name.
    /// Used for catch error variables to avoid alloca-name collisions when
    /// the same identifier is used in multiple `try`/`catch` blocks within
    /// one function. Looked up by 标识符表达式 / 赋值表达式 before mangling.
    variable_alias: std::collections::HashMap<String, String>,
    /// Set of LLVM local alloca names (without %) already emitted in the
    /// current function. Used to uniquify 变量声明 allocas so that two
    /// declarations of the same Qi name in different blocks of one function
    /// don't collide ("multiple definition of local value"). Reset per function.
    used_local_names: std::collections::HashSet<String>,
    /// Track constant variables (cannot be reassigned)
    constant_variables: std::collections::HashSet<String>,
    /// Track Future variable inner types (variable_name -> inner_type like "i64", "i1", "double")
    future_inner_types: std::collections::HashMap<String, String>,
    /// Track function Future return inner types (function_name -> inner_type)
    function_future_inner_types: std::collections::HashMap<String, String>,
    /// Track variables that are semantically boolean (even if stored as i32)
    boolean_variables: std::collections::HashSet<String>,
    /// Track async function return types
    async_function_types: std::collections::HashMap<String, String>,
    /// Track all function return types (including sync functions)
    function_return_types: std::collections::HashMap<String, String>,
    /// Track functions that return struct pointers (mangled_name -> struct_type_name)
    function_return_struct_types: std::collections::HashMap<String, String>,
    /// Track defined function parameter types (name -> params)
    function_param_types: std::collections::HashMap<String, Vec<String>>,
    /// Track original function parameter declarations for default and variadic lowering
    function_parameters: std::collections::HashMap<String, Vec<crate::parser::ast::Parameter>>,
    /// Track function pointer variable signatures (variable_name -> (param_types, return_type))
    function_pointer_signatures: std::collections::HashMap<String, (Vec<String>, String)>,
    /// Track if we're currently inside an async function
    in_async_context: bool,
    /// Track defined functions in current module
    defined_functions: std::collections::HashSet<String>,
    /// 本模块顶层函数的「裸名」集合（在首遍签名收集前预填）。
    /// 用于符号修饰：入口（主程序）模块里、本模块自己定义的函数，符号要按包名加前缀，
    /// 避免与导入库的同名「公开」函数在链接期撞符号（库模块保持裸符号，公开 ABI 不变）。
    module_function_names: std::collections::HashSet<String>,
    /// Track global variables (both original and mangled names)
    global_variables: std::collections::HashSet<String>,
    /// Track global variable LLVM types (name/mangled -> "ptr"|"i64"|"double"|"i1"|...).
    /// Needed because `variable_types` is cleared on every function entry, which would
    /// otherwise lose the type of module-level 变量/常量 and make them load as i64.
    global_variable_types: std::collections::HashMap<String, String>,
    /// Track external function signatures (name -> (params, return_type))
    external_functions: std::collections::HashMap<String, (Vec<String>, String)>,
    /// Track which external functions return struct pointers (mangled_name -> struct_type_name)
    external_function_return_struct_types: std::collections::HashMap<String, String>,
    /// Track struct definitions (name -> field_types)
    struct_definitions: std::collections::HashMap<String, Vec<String>>,
    /// Track struct field names (struct_name -> field_names)
    struct_field_names: std::collections::HashMap<String, Vec<String>>,
    /// Track struct function pointer fields ((struct_name, field_name) -> (param_types, return_type))
    struct_field_function_signatures:
        std::collections::HashMap<(String, String), (Vec<String>, String)>,
    /// Track trait definitions (trait_name -> [(method_name, param_types, return_type)])
    trait_definitions:
        std::collections::HashMap<String, Vec<(String, Vec<String>, Option<String>)>>,
    /// Track variable struct types (variable_name -> struct_type_name)
    variable_struct_types: std::collections::HashMap<String, String>,
    /// Track array element types (variable_name -> element_type like "i64", "double")
    array_element_types: std::collections::HashMap<String, String>,
    /// Track array sizes (variable_name -> size)
    array_sizes: std::collections::HashMap<String, usize>,
    /// Track import aliases (alias -> actual_module_name)
    import_aliases: std::collections::HashMap<String, String>,
    /// Track current module/package name
    current_package_name: Option<String>,
    /// Track loop labels for break/continue (continue_label, break_label)
    loop_stack: Vec<(String, String)>,
    /// Wrapper functions for goroutine spawn (generated at the end)
    goroutine_wrappers: Vec<String>,
    /// 闭包：待生成的顶层函数 AST。主流程结束后再处理。
    pending_closures: Vec<AstNode>,
    /// 闭包计数器（生成 __closure_N 名字）
    closure_counter: usize,
    /// 已知的闭包变量签名（var_name → (param_types, ret_type)）
    closure_signatures: std::collections::HashMap<String, (Vec<String>, String)>,
    /// 标记某变量持有闭包对象（LLVM 类型仍是 ptr，但调用走 fat call）
    closure_variables: std::collections::HashSet<String>,
    /// 标记某函数返回闭包对象（让调用方把返回值传播为 closure_variable）
    functions_returning_closure: std::collections::HashSet<String>,
    /// 需要生成 trampoline 的函数（被 box 成闭包对象用），mangled 名字
    pending_trampolines: std::collections::HashSet<String>,
    /// Module registry for standard library modules
    module_registry: ModuleRegistry,
    /// Imported modules in current compilation unit (module_path -> alias or module_name)
    imported_modules: std::collections::HashMap<String, String>,
    /// Track all memory allocations for lifetime management
    allocations: Vec<AllocationInfo>,
    /// Current scope depth level
    scope_level: usize,
    /// Current function name being processed (for return type lookup)
    current_function_name: Option<String>,
    /// Current function's AST return type (for Future wrapping detection)
    current_function_ast_return_type: Option<crate::parser::ast::TypeNode>,
    /// Verbose debug output
    verbose: bool,
    /// Whether this module is the entry module (contains the main entry point)
    is_entry_module: bool,
}

impl IrBuilder {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
            variable_types: std::collections::HashMap::new(),
            variable_alias: std::collections::HashMap::new(),
            used_local_names: std::collections::HashSet::new(),
            constant_variables: std::collections::HashSet::new(),
            future_inner_types: std::collections::HashMap::new(),
            function_future_inner_types: std::collections::HashMap::new(),
            boolean_variables: std::collections::HashSet::new(),
            async_function_types: std::collections::HashMap::new(),
            function_return_types: std::collections::HashMap::new(),
            function_return_struct_types: std::collections::HashMap::new(),
            function_param_types: std::collections::HashMap::new(),
            function_parameters: std::collections::HashMap::new(),
            function_pointer_signatures: std::collections::HashMap::new(),
            in_async_context: false,
            defined_functions: std::collections::HashSet::new(),
            module_function_names: std::collections::HashSet::new(),
            global_variables: std::collections::HashSet::new(),
            global_variable_types: std::collections::HashMap::new(),
            external_functions: std::collections::HashMap::new(),
            external_function_return_struct_types: std::collections::HashMap::new(),
            struct_definitions: std::collections::HashMap::new(),
            struct_field_names: std::collections::HashMap::new(),
            struct_field_function_signatures: std::collections::HashMap::new(),
            trait_definitions: std::collections::HashMap::new(),
            variable_struct_types: std::collections::HashMap::new(),
            array_element_types: std::collections::HashMap::new(),
            array_sizes: std::collections::HashMap::new(),
            import_aliases: std::collections::HashMap::new(),
            current_package_name: None,
            loop_stack: Vec::new(),
            goroutine_wrappers: Vec::new(),
            pending_closures: Vec::new(),
            closure_counter: 0,
            closure_signatures: std::collections::HashMap::new(),
            closure_variables: std::collections::HashSet::new(),
            functions_returning_closure: std::collections::HashSet::new(),
            pending_trampolines: std::collections::HashSet::new(),
            module_registry: ModuleRegistry::new(),
            imported_modules: std::collections::HashMap::new(),
            allocations: Vec::new(),
            scope_level: 0,
            current_function_name: None,
            current_function_ast_return_type: None,
            verbose: false,
            is_entry_module: true,
        }
        .register_runtime_functions()
    }

    /// Set verbose mode
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    pub fn set_is_entry_module(&mut self, is_entry: bool) {
        self.is_entry_module = is_entry;
    }

    /// Register runtime function signatures
    fn register_runtime_functions(mut self) -> Self {
        // Future type functions - integer
        self.external_functions.insert(
            "qi_future_ready_i64".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_future_await_i64".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );

        // Future type functions - float
        self.external_functions.insert(
            "qi_future_ready_f64".to_string(),
            (vec!["double".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_future_await_f64".to_string(),
            (vec!["ptr".to_string()], "double".to_string()),
        );

        // Future type functions - boolean
        self.external_functions.insert(
            "qi_future_ready_bool".to_string(),
            (vec!["i32".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_future_await_bool".to_string(),
            (vec!["ptr".to_string()], "i32".to_string()),
        );

        // Future type functions - string
        self.external_functions.insert(
            "qi_future_ready_string".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_future_await_string".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );

        // Future type functions - pointer (for structs)
        self.external_functions.insert(
            "qi_future_ready_ptr".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_future_await_ptr".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );

        // Future type functions - common
        self.external_functions.insert(
            "qi_future_failed".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_future_is_completed".to_string(),
            (vec!["ptr".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_future_free".to_string(),
            (vec!["ptr".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_string_free".to_string(),
            (vec!["ptr".to_string()], "void".to_string()),
        );

        // §4-3 / §4-5 异步 状态机 FFI（runtime/async_runtime/state_machine.rs）
        self.external_functions.insert(
            "qi_async_alloc_frame".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_async_free_frame".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "void".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_async_spawn_poll".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "void".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_future_register_waker".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string(), "ptr".to_string()],
                "void".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_future_is_ready".to_string(),
            (vec!["ptr".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_future_value_i64".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_future_complete_i64".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "void".to_string(),
            ),
        );
        self.external_functions
            .insert("qi_future_pending".to_string(), (vec![], "ptr".to_string()));

        // String utility functions
        self.external_functions.insert(
            "strlen".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );

        // Memory allocation functions
        self.external_functions.insert(
            "malloc".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "free".to_string(),
            (vec!["ptr".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_alloc".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_dealloc".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "i32".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_runtime_gc_should_collect".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_gc_collect".to_string(),
            (vec![], "void".to_string()),
        );

        // Crypto module FFI — 跨包调用时需要这些签名才能正确推类型
        // （之前只在 declare 段写了，没注册到 external_functions，导致 qi-web 等
        //  下游包调 加密.HMAC_SHA256 时 codegen 把 ptr 参数误判 i64）
        self.external_functions.insert(
            "qi_crypto_md5".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_crypto_sha256".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_crypto_sha512".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_crypto_base64_encode".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_crypto_base64_decode".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_crypto_hmac_sha256".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );

        // LLM module FFI — 跨包调用注册（qi-harness 等下游包用）
        self.external_functions.insert(
            "qi_llm_create_session".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_chat".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_set_config".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_clear_history".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_get_history_count".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_close_session".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_chat_async".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_stream_chat".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_stream_next".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_stream_close".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_register_tool".to_string(),
            (
                vec![
                    "i64".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                ],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_clear_tools".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_chat_with_tools".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_continue_with_tools".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_has_tool_call".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_get_tool_call_id".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_get_tool_call_name".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_get_tool_call_arguments".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_add_tool_result".to_string(),
            (
                vec![
                    "i64".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                ],
                "i64".to_string(),
            ),
        );
        // Parallel tool_calls 支持
        self.external_functions.insert(
            "qi_llm_get_tool_call_count".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_llm_get_tool_call_id_at".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_get_tool_call_name_at".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_llm_get_tool_call_arguments_at".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );

        // String module functions (标准库.文本)
        self.external_functions.insert(
            "qi_string_length".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_string_concat".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_substring".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_contains".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_starts_with".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_ends_with".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_find".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_replace".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_trim".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_string_to_upper".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_string_to_lower".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_string_compare".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i32".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_equals".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_byte_length".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_string_char_count".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_string_find_from".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_substring_from".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_string_split".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );

        // Other runtime functions can be added here if needed
        self.external_functions.insert(
            "qi_runtime_gc_add_root".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_gc_remove_root".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_gc_add_reference".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_runtime_gc_clear_references".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );

        // JSON module functions
        self.external_functions.insert(
            "qi_json_encode".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_json_decode".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_json_create_object".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_json_create_array".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_json_set_string".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_set_int".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_set_float".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "double".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_set_bool".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_set_object".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_set_array".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_get_string".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_get_int".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_get_float".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "double".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_get_bool".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_get_object".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_get_array".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_array_push_string".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_array_push_int".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_array_push_float".to_string(),
            (
                vec!["i64".to_string(), "double".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_array_push_bool".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_json_free".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );

        // Conversion runtime functions
        self.external_functions.insert(
            "qi_runtime_int_to_string".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_float_to_string".to_string(),
            (vec!["double".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_string_to_int".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_string_to_float".to_string(),
            (vec!["ptr".to_string()], "double".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_int_to_float".to_string(),
            (vec!["i64".to_string()], "double".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_float_to_int".to_string(),
            (vec!["double".to_string()], "i64".to_string()),
        );

        // Print runtime functions
        self.external_functions.insert(
            "qi_runtime_print_int".to_string(),
            (vec!["i64".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_println_int".to_string(),
            (vec!["i64".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_print_float".to_string(),
            (vec!["double".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_println_float".to_string(),
            (vec!["double".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_print_string".to_string(),
            (vec!["ptr".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_println_string".to_string(),
            (vec!["ptr".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_print_bool".to_string(),
            (vec!["i32".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_println_bool".to_string(),
            (vec!["i32".to_string()], "i32".to_string()),
        );

        // List module functions
        self.external_functions.insert(
            "qi_list_int_create".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_int_push".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_int_get".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_int_set".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_int_size".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_int_pop".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_int_clear".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_int_remove".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_int_insert".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_int_contains".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_int_index_of".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_float_create".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_float_push".to_string(),
            (
                vec!["i64".to_string(), "double".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_float_get".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "double".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_float_size".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_string_create".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_string_push".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_string_get".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_string_size".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_ptr_create".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_ptr_push".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_ptr_get".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_ptr_set".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_list_ptr_size".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_list_free".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );

        // Hashmap module functions
        self.external_functions.insert(
            "qi_hashmap_int_create".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_hashmap_int_set".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_hashmap_int_get".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_hashmap_int_contains".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_hashmap_int_remove".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_hashmap_int_size".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_hashmap_int_clear".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_hashmap_float_create".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_hashmap_float_set".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "double".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_hashmap_float_get".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "double".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_hashmap_float_size".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_hashmap_string_create".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_hashmap_string_set".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_hashmap_string_get".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_hashmap_string_size".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_hashmap_free".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );

        // Random module functions
        self.external_functions.insert(
            "qi_random_int".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_random_float".to_string(),
            (
                vec!["double".to_string(), "double".to_string()],
                "double".to_string(),
            ),
        );
        self.external_functions
            .insert("qi_random_bool".to_string(), (vec![], "i32".to_string()));
        self.external_functions.insert(
            "qi_random_string".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions
            .insert("qi_random_uuid".to_string(), (vec![], "ptr".to_string()));

        // Web runtime helper
        self.external_functions.insert(
            "qi_web_call_handler_safe".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_web_safe_process_request".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string(), "ptr".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_web_panic_for_test".to_string(),
            (vec![], "i64".to_string()),
        );

        // TLS module functions
        self.external_functions.insert(
            "qi_tls_create_config".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_tls_free_config".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_tls_listen".to_string(),
            (
                vec![
                    "ptr".to_string(),
                    "i64".to_string(),
                    "i64".to_string(),
                    "i64".to_string(),
                ],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_tls_accept".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_tls_read_string".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_tls_write_string".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_tls_close".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_tls_server_close".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_tls_free_string".to_string(),
            (vec!["ptr".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_h2_serve".to_string(),
            (
                vec![
                    "ptr".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                    "i64".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                ],
                "i64".to_string(),
            ),
        );

        // Bytes module functions
        self.external_functions
            .insert("qi_bytes_create".to_string(), (vec![], "i64".to_string()));
        self.external_functions.insert(
            "qi_bytes_with_capacity".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_from_string".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_to_string".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_length".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_get".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_bytes_set".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_bytes_push".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_bytes_push_string".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_bytes_extend".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_bytes_slice".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_bytes_compare".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_bytes_find".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_bytes_to_hex".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_from_hex".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_to_base64".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_from_base64".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_free".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_bytes_free_string".to_string(),
            (vec!["ptr".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_closure_create".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_closure_get_fn".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_closure_get_int".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_closure_get_ptr".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_closure_set_int".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string(), "i64".to_string()],
                "void".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_closure_set_ptr".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string(), "ptr".to_string()],
                "void".to_string(),
            ),
        );

        self.external_functions.insert(
            "qi_exc_alloc_frame".to_string(),
            (vec![], "ptr".to_string()),
        );
        self.external_functions.insert(
            "setjmp".to_string(),
            (vec!["ptr".to_string()], "i32".to_string()),
        );
        self.external_functions
            .insert("qi_exc_pop".to_string(), (vec![], "void".to_string()));
        self.external_functions.insert(
            "qi_exc_throw".to_string(),
            (vec!["ptr".to_string()], "void".to_string()),
        );
        self.external_functions
            .insert("qi_exc_message".to_string(), (vec![], "ptr".to_string()));
        self.external_functions
            .insert("qi_exc_clear".to_string(), (vec![], "void".to_string()));
        self.external_functions.insert(
            "qi_exc_free_message".to_string(),
            (vec!["ptr".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_signal_install_shutdown".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_signal_should_shutdown".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions
            .insert("qi_signal_reset".to_string(), (vec![], "i64".to_string()));
        self.external_functions.insert(
            "qi_network_tcp_listener_set_nonblocking".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_network_tcp_read_bytes".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_network_tcp_write_bytes".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_network_async_tcp_connect".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_network_async_tcp_read_bytes".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_network_async_tcp_write_bytes".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_network_async_tcp_close".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_network_async_tcp_listen".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_network_async_tcp_accept".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_network_async_tcp_listener_close".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_compress_gzip_bytes".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_compress_gunzip_bytes".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_async_serve".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_runtime_serialize_http_response".to_string(),
            (
                vec![
                    "i64".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                ],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_web_parse_request_bytes".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_web_parse_request_cstr".to_string(),
            (vec!["ptr".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_web_request_method".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_web_request_path".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_web_request_query".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_web_request_headers".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_web_request_body".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_web_request_parts_free".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_web_request_keep_alive".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_runtime_serialize_http_response_ka".to_string(),
            (
                vec![
                    "i64".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                    "ptr".to_string(),
                    "i64".to_string(),
                ],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_web_router_register".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_web_router_match".to_string(),
            (
                vec!["ptr".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_web_match_handler".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_web_match_path_hit".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_web_match_params".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_web_match_method_mask".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_web_match_free".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_web_build_request_id".to_string(),
            (
                vec!["ptr".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_multipart_parse".to_string(),
            (
                vec!["i64".to_string(), "ptr".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_multipart_extract_boundary".to_string(),
            (vec!["ptr".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_multipart_count".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_multipart_name".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_multipart_filename".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_multipart_content_type".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "ptr".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_multipart_body".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_multipart_free".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );

        // DateTime sleep functions
        self.external_functions.insert(
            "qi_datetime_sleep_millis".to_string(),
            (vec!["i64".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_datetime_async_sleep_millis".to_string(),
            (vec!["i64".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_datetime_async_sleep_future".to_string(),
            (vec!["i64".to_string()], "ptr".to_string()),
        );
        self.external_functions.insert(
            "qi_datetime_sleep_seconds".to_string(),
            (vec!["i64".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_datetime_sleep_micros".to_string(),
            (vec!["i64".to_string()], "void".to_string()),
        );
        self.external_functions.insert(
            "qi_datetime_now_millis".to_string(),
            (vec![], "i64".to_string()),
        );

        // 同步原语 (标准库.同步) — 互斥锁
        self.external_functions.insert(
            "qi_sync_mutex_create".to_string(),
            (vec![], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_sync_mutex_lock".to_string(),
            (vec!["i64".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_sync_mutex_unlock".to_string(),
            (vec!["i64".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_sync_mutex_trylock".to_string(),
            (vec!["i64".to_string()], "i32".to_string()),
        );
        self.external_functions.insert(
            "qi_sync_mutex_destroy".to_string(),
            (vec!["i64".to_string()], "i32".to_string()),
        );
        // 同步原语 (标准库.同步) — 原子整数
        self.external_functions.insert(
            "qi_sync_atomic_create".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_sync_atomic_load".to_string(),
            (vec!["i64".to_string()], "i64".to_string()),
        );
        self.external_functions.insert(
            "qi_sync_atomic_store".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i32".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_sync_atomic_add".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string()],
                "i64".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_sync_atomic_cas".to_string(),
            (
                vec!["i64".to_string(), "i64".to_string(), "i64".to_string()],
                "i32".to_string(),
            ),
        );
        self.external_functions.insert(
            "qi_sync_atomic_destroy".to_string(),
            (vec!["i64".to_string()], "i32".to_string()),
        );

        self
    }

    pub fn build(&mut self, ast: &AstNode) -> Result<String, String> {
        self.instructions.clear();
        self.temp_counter = 0;
        self.label_counter = 0;
        self.variable_types.clear();
        self.async_function_types.clear();
        // Note: We don't clear defined_functions and external_functions here
        // so they can be set before calling build()

        // 预遍：记录本模块包名 + 所有顶层函数裸名，供符号修饰判断「入口模块本地函数」。
        // 必须在首遍签名收集之前，因为签名 key 也走 mangle_function_name，要和定义/调用一致。
        if let AstNode::程序(program) = ast {
            self.current_package_name = program.package_name.clone();
            for stmt in &program.statements {
                if let AstNode::函数声明(f) = stmt {
                    self.module_function_names.insert(f.name.clone());
                }
            }
        }

        // First pass: collect all function signatures
        self.collect_function_signatures(ast)?;

        // Second pass: generate code
        self.build_node(ast)?;

        // Third pass: emit pending closure top-level functions（嵌套闭包可能再产生 pending）
        while let Some(closure_ast) = self.pending_closures.pop() {
            self.collect_function_signatures(&closure_ast)?;
            self.build_node(&closure_ast)?;
        }

        // Fourth pass: 给被 box 成 closure 的函数生成 trampoline
        // trampoline 接受 (env, 用户参数...) 调用真函数（忽略 env）
        let trampolines: Vec<String> = self.pending_trampolines.iter().cloned().collect();
        for fn_name in trampolines {
            self.emit_trampoline(&fn_name);
        }

        self.emit_llvm_ir()
    }

    /// 为函数 `fn_name` 生成 trampoline `fn_name__t(env, args...) → fn_name(args...)`
    /// 直接 push 字符串 IR 到 goroutine_wrappers 列表（emit 末尾会输出）
    fn emit_trampoline(&mut self, fn_name: &str) {
        // 优先用本模块签名；否则用 external_functions 跨模块签名
        let (param_types, ret_type) =
            if let Some(p) = self.function_param_types.get(fn_name).cloned() {
                let r = self
                    .function_return_types
                    .get(fn_name)
                    .cloned()
                    .unwrap_or_else(|| "i64".to_string());
                (p, r)
            } else if let Some((p, r)) = self.external_functions.get(fn_name).cloned() {
                (p, r)
            } else {
                return;
            };

        let mut def = String::new();
        // 参数：第一个 ptr %env，后面是用户参数
        let mut params_ir = vec!["ptr %env".to_string()];
        let mut call_args = Vec::new();
        for (i, ty) in param_types.iter().enumerate() {
            let pname = format!("%a{}", i);
            params_ir.push(format!("{} {}", ty, pname));
            call_args.push(format!("{} {}", ty, pname));
        }

        let trampoline_name = format!("{}__t", fn_name);
        if ret_type == "void" {
            def.push_str(&format!(
                "define void @{}({}) {{\n",
                trampoline_name,
                params_ir.join(", ")
            ));
            def.push_str(&format!(
                "  call void @{}({})\n",
                fn_name,
                call_args.join(", ")
            ));
            def.push_str("  ret void\n}\n");
        } else {
            def.push_str(&format!(
                "define {} @{}({}) {{\n",
                ret_type,
                trampoline_name,
                params_ir.join(", ")
            ));
            let null = if ret_type == "ptr" { "null" } else { "0" };
            def.push_str(&format!(
                "  %r = call {} @{}({})\n",
                ret_type,
                fn_name,
                call_args.join(", ")
            ));
            // 如果 fn_name 没注册（外部模块）— null/0 fallback；但此分支不会进，因为 pending 只追加我们看见的函数
            let _ = null;
            def.push_str(&format!("  ret {} %r\n}}\n", ret_type));
        }
        self.goroutine_wrappers.push(def);
    }

    /// First pass: collect function signatures (parameter types and return types)
    /// This allows goroutine spawns to know the correct types even if the function
    /// is defined later in the source file
    fn collect_function_signatures(&mut self, node: &AstNode) -> Result<(), String> {
        match node {
            AstNode::程序(program) => {
                // Process all statements to find function declarations
                for stmt in &program.statements {
                    self.collect_function_signatures(stmt)?;
                }
            }
            AstNode::函数声明(func_decl) => {
                // Mangle function name
                let func_name: String = match func_decl.name.as_str() {
                    "入口" => {
                        if self.is_entry_module {
                            "main".to_string()
                        } else {
                            self.mangle_function_name("入口")
                        }
                    }
                    name => {
                        if name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(name)
                        } else {
                            name.to_string()
                        }
                    }
                };

                // Collect parameter types
                let param_types: Vec<String> = func_decl
                    .parameters
                    .iter()
                    .map(|p| self.get_llvm_type(&p.type_annotation))
                    .collect();

                // Determine return type
                let return_type =
                    if (func_decl.name == "入口" || func_name == "main") && self.is_entry_module {
                        "i32".to_string()
                    } else if let Some(_) = func_decl.return_type {
                        self.get_return_type(&func_decl.return_type)
                    } else {
                        // For now, default to void if no return type specified
                        "void".to_string()
                    };

                // Store in function_param_types and function_return_types
                self.function_param_types
                    .insert(func_name.clone(), param_types);
                self.function_return_types
                    .insert(func_name.clone(), return_type);

                // 如果函数返回类型是 函数(...) 且函数体直接 `返回 闭包(...)`，
                // 标记它返回闭包，让调用方把结果当 closure 处理
                if matches!(
                    func_decl.return_type.as_ref(),
                    Some(crate::parser::ast::TypeNode::函数类型(_))
                ) && function_body_returns_closure(&func_decl.body)
                {
                    self.functions_returning_closure
                        .insert(func_decl.name.clone());
                    self.functions_returning_closure.insert(func_name.clone());
                }

                if self.verbose {
                    eprintln!(
                        "[DEBUG] Collected signature for {}: {:?} -> {:?}",
                        func_name,
                        self.function_param_types.get(&func_name),
                        self.function_return_types.get(&func_name)
                    );
                }
            }
            _ => {
                // Ignore other node types in signature collection pass
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn generate_temp(&mut self) -> String {
        self.temp_counter += 1;
        format!("%t{}", self.temp_counter)
    }

    /// Look up the LLVM type of a struct field by field name
    fn get_struct_field_type(&self, struct_name: &str, field_name: &str) -> Option<String> {
        if let Some(field_names) = self.struct_field_names.get(struct_name) {
            if let Some(idx) = field_names.iter().position(|n| n == field_name) {
                if let Some(field_types) = self.struct_definitions.get(struct_name) {
                    return field_types.get(idx).cloned();
                }
            }
        }
        None
    }

    /// Look up the struct type name for a struct field that is itself a struct pointer
    fn get_struct_field_struct_type(
        &self,
        _struct_name: &str,
        _field_name: &str,
    ) -> Option<String> {
        // TODO: implement nested struct type tracking via struct_field_struct_types
        None
    }

    #[allow(dead_code)]
    fn generate_label(&mut self) -> String {
        self.label_counter += 1;
        format!("L{}", self.label_counter)
    }

    fn infer_ir_value_type(&self, value: &str) -> Option<String> {
        if value.starts_with('@') || value.contains("getelementptr") {
            Some("ptr".to_string())
        } else if value.contains('.') {
            Some("double".to_string())
        } else if value.starts_with('%') {
            let var_name = value.trim_start_matches('%');
            self.variable_types.get(var_name).cloned()
        } else if value == "0" || value == "1" {
            Some("i1".to_string())
        } else if value.parse::<i64>().is_ok() {
            Some("i64".to_string())
        } else {
            None
        }
    }

    /// 递归扫描闭包体，收集自由标识符（不在 local_names 里的标识符）。
    /// 同时把局部声明（变量声明）追加到 local_names。
    fn collect_free_identifiers(
        &self,
        node: &AstNode,
        local_names: &mut std::collections::HashSet<String>,
        frees: &mut Vec<String>,
    ) {
        use crate::parser::ast::*;
        match node {
            AstNode::标识符表达式(id) => {
                if !local_names.contains(&id.name) {
                    frees.push(id.name.clone());
                }
            }
            AstNode::变量声明(decl) => {
                if let Some(init) = &decl.initializer {
                    self.collect_free_identifiers(init, local_names, frees);
                }
                local_names.insert(decl.name.clone());
            }
            AstNode::赋值表达式(assign) => {
                self.collect_free_identifiers(&assign.target, local_names, frees);
                self.collect_free_identifiers(&assign.value, local_names, frees);
            }
            AstNode::二元操作表达式(b) => {
                self.collect_free_identifiers(&b.left, local_names, frees);
                self.collect_free_identifiers(&b.right, local_names, frees);
            }
            AstNode::一元操作表达式(u) => {
                self.collect_free_identifiers(&u.operand, local_names, frees);
            }
            AstNode::函数调用表达式(call) => {
                // callee 是字符串名（不是 AST 节点）— 仍可能引用外层变量（函数指针变量）
                if !local_names.contains(&call.callee) {
                    frees.push(call.callee.clone());
                }
                for a in &call.arguments {
                    self.collect_free_identifiers(a, local_names, frees);
                }
            }
            AstNode::方法调用表达式(mc) => {
                self.collect_free_identifiers(&mc.object, local_names, frees);
                for a in &mc.arguments {
                    self.collect_free_identifiers(a, local_names, frees);
                }
            }
            AstNode::字段访问表达式(f) => {
                self.collect_free_identifiers(&f.object, local_names, frees);
            }
            AstNode::数组访问表达式(a) => {
                self.collect_free_identifiers(&a.array, local_names, frees);
                self.collect_free_identifiers(&a.index, local_names, frees);
            }
            AstNode::返回语句(r) => {
                if let Some(v) = &r.value {
                    self.collect_free_identifiers(v, local_names, frees);
                }
            }
            AstNode::如果语句(if_stmt) => {
                self.collect_free_identifiers(&if_stmt.condition, local_names, frees);
                let mut sub = local_names.clone();
                for s in &if_stmt.then_branch {
                    self.collect_free_identifiers(s, &mut sub, frees);
                }
                if let Some(else_b) = &if_stmt.else_branch {
                    let mut sub2 = local_names.clone();
                    self.collect_free_identifiers(else_b, &mut sub2, frees);
                }
            }
            AstNode::当语句(while_stmt) => {
                self.collect_free_identifiers(&while_stmt.condition, local_names, frees);
                let mut sub = local_names.clone();
                for s in &while_stmt.body {
                    self.collect_free_identifiers(s, &mut sub, frees);
                }
            }
            AstNode::块语句(block) => {
                let mut sub = local_names.clone();
                for s in &block.statements {
                    self.collect_free_identifiers(s, &mut sub, frees);
                }
            }
            AstNode::表达式语句(e) => {
                self.collect_free_identifiers(&e.expression, local_names, frees);
            }
            AstNode::字符串连接表达式(concat) => {
                self.collect_free_identifiers(&concat.left, local_names, frees);
                self.collect_free_identifiers(&concat.right, local_names, frees);
            }
            AstNode::结构体实例化表达式(s) => {
                for f in &s.fields {
                    self.collect_free_identifiers(&f.value, local_names, frees);
                }
            }
            // 其他节点类型：保守起见跳过，闭包不捕获就行
            _ => {}
        }
    }

    /// 把闭包表达式合成为顶层函数声明：env 参数 + 序言（从 env 读 caps）+ 用户体。
    /// 用户体里直接引用 freevar 名字，会被捕获到的本地变量满足。
    fn synthesize_closure_function(
        &self,
        name: &str,
        closure_expr: &crate::parser::ast::ClosureExpression,
        captured: &[(String, String)],
    ) -> AstNode {
        use crate::parser::ast::*;

        // env 参数
        let env_param = Parameter {
            name: "__env".to_string(),
            type_annotation: Some(TypeNode::基础类型(BasicType::指针)),
            default_value: None,
            is_variadic: false,
            span: Default::default(),
        };

        // 用户参数
        let mut params: Vec<Parameter> = vec![env_param];
        params.extend(closure_expr.parameters.iter().cloned());

        // 序言：每个 cap 一行 `变量 name: <ty> = qi_closure_get_int/ptr(__env, idx);`
        let mut body: Vec<AstNode> = Vec::new();
        for (i, (cap_name, ty)) in captured.iter().enumerate() {
            let getter_fn = if ty == "ptr" {
                "qi_closure_get_ptr"
            } else {
                "qi_closure_get_int"
            };
            let env_arg = AstNode::标识符表达式(IdentifierExpression {
                name: "__env".to_string(),
                span: Default::default(),
            });
            let idx_arg = AstNode::字面量表达式(LiteralExpression {
                value: LiteralValue::整数(i as i64),
                span: Default::default(),
            });
            let call = AstNode::函数调用表达式(FunctionCallExpression {
                module_qualifier: None,
                callee: getter_fn.to_string(),
                arguments: vec![env_arg, idx_arg],
                span: Default::default(),
            });

            // 类型还原：ptr 用字符串/指针，i64 用整数
            let var_type = if ty == "ptr" {
                Some(TypeNode::基础类型(BasicType::字符串))
            } else {
                Some(TypeNode::基础类型(BasicType::整数))
            };

            body.push(AstNode::变量声明(VariableDeclaration {
                name: cap_name.clone(),
                type_annotation: var_type,
                initializer: Some(Box::new(call)),
                is_mutable: true,
                span: Default::default(),
            }));
        }

        // 用户体
        body.extend(closure_expr.body.iter().cloned());

        AstNode::函数声明(FunctionDeclaration {
            name: name.to_string(),
            visibility: Visibility::私有,
            parameters: params,
            return_type: closure_expr.return_type.clone(),
            body,
            is_inline: false,
            is_async: false,
            span: Default::default(),
        })
    }

    fn mangled_bare_name(&self, name: &str) -> String {
        if name.chars().any(|c| !c.is_ascii()) {
            self.mangle_function_name(name)
        } else {
            name.to_string()
        }
    }

    fn variadic_array_temp(&mut self, values: Vec<String>, element_type: &str) -> String {
        let temp = self.generate_temp();
        let temp_name = temp.trim_start_matches('%').to_string();
        let size = values.len();

        self.array_element_types
            .insert(temp_name.clone(), element_type.to_string());
        self.array_sizes.insert(temp_name, size);
        self.variable_types
            .insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

        self.add_instruction(IrInstruction::数组分配 {
            dest: temp.clone(),
            size: size.to_string(),
            element_type: element_type.to_string(),
        });

        for (index, value) in values.into_iter().enumerate() {
            self.add_instruction(IrInstruction::数组存储 {
                array: temp.clone(),
                index: index.to_string(),
                value,
                element_type: element_type.to_string(),
            });
        }

        temp
    }

    fn is_pointer_value(&self, value: &str) -> bool {
        if value.starts_with('@') {
            return true;
        }
        if value.starts_with('%') {
            let name = value.trim_start_matches('%');
            return self
                .variable_types
                .get(name)
                .map(|t| t == "ptr")
                .unwrap_or(false);
        }
        false
    }

    /// 判断指针值是不是 .rodata 字面量（@.str* 字符串常量），不需要 GC 追踪。
    /// rodata 段永不被 GC 释放，对它们调用 add_reference 是无效负担 — qi-web hot
    /// path 上 25 次 字段赋值 / 多次 struct 构造每个字符串字段都触发这个，是
    /// 真正的瓶颈。
    fn is_rodata_literal(&self, value: &str) -> bool {
        // qi codegen 当前发出的字符串字面量名都是 @.strN 格式，见 字符串常量 emit
        value.starts_with("@.str")
    }

    fn lower_call_arguments(
        &mut self,
        callee: &str,
        supplied_args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        let params = match self.function_parameters.get(callee).cloned() {
            Some(params) => params,
            None => return Ok(supplied_args),
        };

        let variadic_index = params.iter().position(|p| p.is_variadic);
        let fixed_count = variadic_index.unwrap_or(params.len());
        let mut lowered = supplied_args;

        // Fill missing non-variadic arguments with default values.
        while lowered.len() < fixed_count {
            let param = &params[lowered.len()];
            if let Some(default_value) = &param.default_value {
                let default_temp = self.build_node(default_value)?;
                lowered.push(default_temp);
            } else {
                return Err(format!("函数 '{}' 缺少参数 '{}'", callee, param.name));
            }
        }

        if let Some(index) = variadic_index {
            let variadic_values = if lowered.len() > index {
                lowered.split_off(index)
            } else {
                Vec::new()
            };
            let element_type = self.get_llvm_type(&params[index].type_annotation);
            let variadic_len = variadic_values.len();
            let array_temp = self.variadic_array_temp(variadic_values, &element_type);
            lowered.push(array_temp);
            lowered.push(variadic_len.to_string());
        }

        Ok(lowered)
    }

    /// Get the full function name from a function call expression, including module prefix
    fn get_full_function_name(
        &self,
        call_expr: &crate::parser::ast::FunctionCallExpression,
    ) -> String {
        if let Some(module_qualifier) = &call_expr.module_qualifier {
            // 检查是否为导入的模块（存在于 import_aliases 中）
            if self.import_aliases.contains_key(module_qualifier) {
                // 这是导入的函数，直接使用函数名，不加模块前缀
                // 例如：数学.最大值 -> 最大值（直接使用导入的函数名）
                call_expr.callee.clone()
            } else {
                // 这是本地模块，使用模块前缀
                // 模块前缀调用，如 数学工具.最大值 -> 数学_最大值
                format!("{}_{}", module_qualifier, call_expr.callee)
            }
        } else {
            // 普通函数调用
            call_expr.callee.clone()
        }
    }

    #[allow(dead_code)]
    fn add_instruction(&mut self, instruction: IrInstruction) {
        self.instructions.push(instruction);
    }

    pub fn get_instructions(&self) -> &[IrInstruction] {
        &self.instructions
    }

    pub fn clear(&mut self) {
        self.instructions.clear();
        self.temp_counter = 0;
        self.label_counter = 0;
        self.async_function_types.clear();
    }

    /// Set external function signatures for cross-module calls
    pub fn set_external_functions(
        &mut self,
        funcs: std::collections::HashMap<String, (Vec<String>, String)>,
    ) {
        // Merge the provided functions with existing ones (don't replace)
        // This preserves built-in functions like malloc/free that are added in new()
        for (name, sig) in funcs {
            self.external_functions.insert(name, sig);
        }
    }

    /// Set defined functions in the current module
    pub fn set_defined_functions(&mut self, funcs: std::collections::HashSet<String>) {
        self.defined_functions = funcs;
    }

    /// Set import aliases for namespace resolution
    pub fn set_import_aliases(&mut self, aliases: std::collections::HashMap<String, String>) {
        self.import_aliases = aliases;
    }

    /// Set external function return struct types (for cross-module struct pointer returns)
    pub fn set_external_function_return_struct_types(
        &mut self,
        map: std::collections::HashMap<String, String>,
    ) {
        for (k, v) in map {
            self.external_function_return_struct_types.insert(k, v);
        }
    }

    pub fn set_external_struct_definitions(
        &mut self,
        definitions: std::collections::HashMap<String, Vec<String>>,
        field_names: std::collections::HashMap<String, Vec<String>>,
        function_fields: std::collections::HashMap<(String, String), (Vec<String>, String)>,
    ) {
        for (name, fields) in definitions {
            self.struct_definitions.entry(name).or_insert(fields);
        }
        for (name, fields) in field_names {
            self.struct_field_names.entry(name).or_insert(fields);
        }
        for (key, signature) in function_fields {
            self.struct_field_function_signatures
                .entry(key)
                .or_insert(signature);
        }
    }

    /// Process an import statement and register the imported module
    fn process_import(
        &mut self,
        import_stmt: &crate::parser::ast::ImportStatement,
    ) -> Result<(), String> {
        // Check if this is a relative path (starts with . or ..)
        let is_relative_path = !import_stmt.module_path.is_empty()
            && (import_stmt.module_path[0] == "." || import_stmt.module_path[0] == "..");

        // For relative paths, we don't use the ModuleRegistry
        // They will be resolved by the compiler's resolve_import_path function
        if is_relative_path {
            // For relative imports, use the last component as the module name
            let module_name = import_stmt
                .module_path
                .last()
                .ok_or_else(|| "导入路径为空".to_string())?
                .clone();

            // Use alias if provided, otherwise use the last path component
            let import_key = import_stmt.alias.clone().unwrap_or(module_name.clone());

            // For relative imports, register them with a special marker
            // We'll use the full path joined with / as the "module path"
            let relative_path_key = import_stmt.module_path.join("/");
            self.import_aliases
                .insert(import_key.clone(), relative_path_key.clone());

            return Ok(());
        }

        // For single-component imports in a package context, treat as intra-package import
        // e.g., when in "数学" package: "导入 最大值;" means importing from the same package
        if import_stmt.module_path.len() == 1 && self.current_package_name.is_some() {
            let submodule_name = &import_stmt.module_path[0];

            // For intra-package imports, we don't need to resolve or register them
            // The functions from the same package are already available
            // Just record the alias if provided
            let import_key = import_stmt
                .alias
                .clone()
                .unwrap_or_else(|| submodule_name.clone());

            // For intra-package imports, we use the package name as the module path
            // This allows functions from different files in the same package to be accessible
            if let Some(package_name) = &self.current_package_name {
                self.import_aliases.insert(import_key, package_name.clone());
            }

            return Ok(());
        }

        // For non-standard-library multi-component imports (user-defined packages like Web.控制器),
        // lib.rs has already resolved the import and populated import_aliases before codegen.
        // Skip the module_registry lookup in that case to avoid "module not found" errors.
        let first_component = import_stmt
            .module_path
            .first()
            .map(|s| s.as_str())
            .unwrap_or("");
        let is_stdlib = first_component == "标准库";
        if !is_stdlib && import_stmt.module_path.len() > 1 {
            // User-defined package import: use last component as alias key if not already registered
            let last_component = import_stmt
                .module_path
                .last()
                .ok_or_else(|| "导入路径为空".to_string())?
                .clone();
            let import_key = import_stmt.alias.clone().unwrap_or(last_component);
            let module_dot_path = import_stmt.module_path.join(".");
            // Register if not already set by lib.rs pre-processing
            if !self.import_aliases.contains_key(&import_key) {
                self.import_aliases.insert(import_key, module_dot_path);
            }
            return Ok(());
        }

        // Resolve module path from import statement (for standard library and global modules)
        let module_path = self
            .module_registry
            .resolve_module_path(&import_stmt.module_path)
            .ok_or_else(|| {
                format!(
                    "模块 '{}' 不存在。可用的模块: {:?}",
                    import_stmt.module_path.join("."),
                    self.module_registry.module_paths()
                )
            })?;

        // Get the last component as the module name
        let module_name = import_stmt
            .module_path
            .last()
            .ok_or_else(|| "导入路径为空".to_string())?
            .clone();

        // Use alias if provided, otherwise use the module name
        let import_key = import_stmt.alias.clone().unwrap_or(module_name);

        // Register the import
        self.imported_modules
            .insert(module_path.clone(), import_key.clone());
        self.import_aliases.insert(import_key, module_path);

        Ok(())
    }

    /// Check if a function is available in an imported module
    fn check_module_function_available(
        &self,
        module_name: &str,
        function_name: &str,
    ) -> Result<&ModuleFunction, String> {
        // Resolve the module name (could be an alias)
        let module_path = self.import_aliases.get(module_name).ok_or_else(|| {
            format!(
                "模块 '{}' 未导入。请先使用 '导入 标准库.{};'",
                module_name, module_name
            )
        })?;

        // Get the function from the module
        self.module_registry
            .get_function(module_path, function_name)
            .ok_or_else(|| format!("模块 '{}' 中不存在函数 '{}'", module_name, function_name))
    }

    /// Escape special characters in strings for LLVM IR
    fn escape_string(&self, s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                '\n' => result.push_str("\\0A"),
                '\r' => result.push_str("\\0D"),
                '\t' => result.push_str("\\09"),
                '"' => result.push_str("\\22"),
                '\\' => result.push_str("\\\\"),
                _ if c.is_ascii() && (c as u8) < 32 => {
                    result.push_str(&format!("\\{:02X}", c as u8));
                }
                _ if (c as u32) > 127 => {
                    // For Unicode characters, use hex escape sequences in LLVM format
                    let mut buf = [0u8; 4];
                    let encoded = c.encode_utf8(&mut buf);
                    for &byte in encoded.as_bytes() {
                        result.push_str(&format!("\\{:02X}", byte));
                    }
                }
                _ => result.push(c),
            }
        }
        result
    }

    /// Mangle Chinese function names using UTF-8 + Hex encoding
    /// Prefix with _Z_ to avoid conflicts with C library symbols
    /// 进入一个词法块前快照 variable_alias，块结束后用它还原。
    /// 这样块内的同名局部变量唯一化（shadowing）不会泄漏到块外，
    /// 块外对同一标识符的引用回退到外层 alloca。
    /// 注意：只还原 alias，不还原 used_local_names —— 已用的 alloca 裸名
    /// 必须在整个函数内保持唯一（含兄弟块），否则又会撞名。
    fn snapshot_alias(&self) -> std::collections::HashMap<String, String> {
        self.variable_alias.clone()
    }
    fn restore_alias(&mut self, snapshot: std::collections::HashMap<String, String>) {
        self.variable_alias = snapshot;
    }

    fn mangle_function_name(&self, name: &str) -> String {
        // 入口（主程序）模块里、本模块自己定义的函数：符号按包名加前缀。
        // 否则用户程序定义的同名函数会和导入库的「公开」函数（库符号是裸名）在链接期撞符号
        // （duplicate symbol）。库模块（非入口）保持裸符号不变，对外 ABI 不受影响。
        // 因定义 / 调用 / 取地址 / 签名收集都走本函数，加前缀后三者天然一致。
        // 注意：入口函数 `入口`(→ main) 不修饰；局部变量/形参虽也走本函数，但只有名字恰好
        // 等于本模块顶层函数名时才会被修饰，而局部引用同样全程走本函数、且用 % 前缀，故自洽。
        let qualified;
        let name = if self.is_entry_module
            && name != "入口"
            && self.module_function_names.contains(name)
            && name.chars().any(|c| !c.is_ascii())
        {
            if let Some(pkg) = self.current_package_name.as_ref() {
                qualified = format!("{}_{}", pkg, name);
                qualified.as_str()
            } else {
                name
            }
        } else {
            name
        };

        // ASCII names remain unchanged (except main function special case)
        if name.chars().all(|c| c.is_ascii()) {
            return name.to_string();
        }

        // Convert UTF-8 bytes to hex representation
        let utf8_bytes = name.as_bytes();
        let hex_string: String = utf8_bytes
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect();

        // Add prefix to prevent symbol conflicts
        format!("_Z_{}", hex_string)
    }

    /// Get the alignment requirement for a type
    fn get_type_alignment(&self, type_name: &str) -> usize {
        match type_name {
            "i64" => 8,
            "i32" => 4,
            "i16" => 2,
            "i8" | "bool" => 1,
            "double" => 8,
            "float" => 4,
            "ptr" => 8, // Pointer alignment on 64-bit systems
            _ => {
                // Default alignment for custom types
                if type_name.contains("ptr") || type_name.starts_with('%') {
                    8 // Assume pointer alignment for struct types
                } else if type_name.contains("i64") {
                    8
                } else if type_name.contains("i32") {
                    4
                } else if type_name.contains("double") {
                    8
                } else {
                    4 // Default fallback
                }
            }
        }
    }

    /// Determine the appropriate Future creation function based on inner type
    fn get_future_ready_function(&self, inner_type: &crate::parser::ast::TypeNode) -> &'static str {
        use crate::parser::ast::{BasicType, TypeNode};
        match inner_type {
            TypeNode::基础类型(
                BasicType::整数 | BasicType::长整数 | BasicType::短整数 | BasicType::字节,
            ) => "qi_future_ready_i64",
            TypeNode::基础类型(BasicType::浮点数) => "qi_future_ready_f64",
            TypeNode::基础类型(BasicType::布尔) => "qi_future_ready_bool",
            TypeNode::基础类型(BasicType::字符串) => "qi_future_ready_string",
            TypeNode::结构体类型(_) | TypeNode::自定义类型(_) | TypeNode::指针类型(_) => {
                "qi_future_ready_ptr"
            }
            _ => {
                // Default to i64 for unknown types
                "qi_future_ready_i64"
            }
        }
    }

    /// Determine the appropriate Future await function based on inner type
    fn get_future_await_function(&self, inner_type: &crate::parser::ast::TypeNode) -> &'static str {
        use crate::parser::ast::{BasicType, TypeNode};
        match inner_type {
            TypeNode::基础类型(
                BasicType::整数 | BasicType::长整数 | BasicType::短整数 | BasicType::字节,
            ) => "qi_future_await_i64",
            TypeNode::基础类型(BasicType::浮点数) => "qi_future_await_f64",
            TypeNode::基础类型(BasicType::布尔) => "qi_future_await_bool",
            TypeNode::基础类型(BasicType::字符串) => "qi_future_await_string",
            TypeNode::结构体类型(_) | TypeNode::自定义类型(_) | TypeNode::指针类型(_) => {
                "qi_future_await_ptr"
            }
            _ => {
                // Default to i64 for unknown types
                "qi_future_await_i64"
            }
        }
    }

    /// Get LLVM type string from TypeNode
    fn get_llvm_type_from_ast(&self, type_node: &crate::parser::ast::TypeNode) -> String {
        use crate::parser::ast::{BasicType, TypeNode};
        match type_node {
            TypeNode::基础类型(
                BasicType::整数 | BasicType::长整数 | BasicType::短整数 | BasicType::字节,
            ) => "i64".to_string(),
            TypeNode::基础类型(BasicType::浮点数) => "double".to_string(),
            TypeNode::基础类型(BasicType::布尔) => "i1".to_string(),
            TypeNode::基础类型(BasicType::字符串) => "ptr".to_string(),
            TypeNode::数组类型(array_type) => {
                // Arrays are represented as pointers in LLVM IR
                // The element type information is tracked separately
                "ptr".to_string()
            }
            TypeNode::结构体类型(_) | TypeNode::自定义类型(_) | TypeNode::指针类型(_) => {
                "ptr".to_string()
            }
            TypeNode::函数类型(_) => "ptr".to_string(),
            _ => "i64".to_string(),
        }
    }

    fn get_struct_field_llvm_type(&self, struct_name: &str, field_name: &str) -> Option<String> {
        let field_names = self.struct_field_names.get(struct_name)?;
        let field_index = field_names.iter().position(|name| name == field_name)?;
        self.struct_definitions
            .get(struct_name)
            .and_then(|field_types| field_types.get(field_index))
            .cloned()
    }

    fn function_type_signature(
        &self,
        function_type: &crate::parser::ast::FunctionType,
    ) -> (Vec<String>, String) {
        let param_types = function_type
            .parameters
            .iter()
            .map(|param| self.get_llvm_type_from_ast(param))
            .collect();
        let return_type = self.get_llvm_type_from_ast(&function_type.return_type);
        (param_types, return_type)
    }

    fn record_function_pointer_signature(
        &mut self,
        name: &str,
        type_annotation: &Option<crate::parser::ast::TypeNode>,
    ) {
        if let Some(crate::parser::ast::TypeNode::函数类型(function_type)) = type_annotation {
            let signature = self.function_type_signature(function_type);
            self.function_pointer_signatures
                .insert(name.to_string(), signature.clone());
            let mangled_name = self.mangled_bare_name(name);
            self.function_pointer_signatures
                .insert(mangled_name.clone(), signature.clone());
            self.function_pointer_signatures
                .insert(format!("param_{}", mangled_name), signature);
        }
    }

    fn lookup_function_pointer_signature(&self, name: &str) -> Option<(Vec<String>, String)> {
        let mangled_name = self.mangled_bare_name(name);
        self.function_pointer_signatures
            .get(name)
            .or_else(|| self.function_pointer_signatures.get(&mangled_name))
            .or_else(|| {
                self.function_pointer_signatures
                    .get(&format!("param_{}", name))
            })
            .or_else(|| {
                self.function_pointer_signatures
                    .get(&format!("param_{}", mangled_name))
            })
            .cloned()
    }

    fn function_pointer_value_ref(&self, name: &str) -> String {
        let mangled_name = self.mangled_bare_name(name);
        if self.variable_types.contains_key(&format!("param_{}", name))
            || self
                .variable_types
                .contains_key(&format!("param_{}", mangled_name))
        {
            format!("%{}", mangled_name)
        } else {
            format!("%{}", mangled_name)
        }
    }

    fn get_array_element_llvm_type(
        &self,
        type_annotation: &Option<crate::parser::ast::TypeNode>,
    ) -> Option<String> {
        match type_annotation {
            Some(crate::parser::ast::TypeNode::数组类型(array_type)) => {
                Some(self.get_llvm_type_from_ast(array_type.element_type.as_ref()))
            }
            _ => None,
        }
    }

    /// Mangle type names (similar to function names)
    /// For struct types, this handles Chinese characters in type names
    fn mangle_type_name(&self, name: &str) -> String {
        // Remove .type suffix if present
        let base_name = name.strip_suffix(".type").unwrap_or(name);

        // ASCII names remain unchanged
        if base_name.chars().all(|c| c.is_ascii()) {
            return name.to_string();
        }

        // Convert UTF-8 bytes to hex representation
        let utf8_bytes = base_name.as_bytes();
        let hex_string: String = utf8_bytes
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect();

        // Add prefix and .type suffix - use %struct. prefix for LLVM compatibility
        if name.ends_with(".type") {
            format!("%struct.ZT_{}", hex_string)
        } else {
            format!("struct.ZT_{}", hex_string)
        }
    }

    /// Map Chinese function names to runtime function names.
    ///
    /// Bridges Qi language function names to runtime C function names. Only
    /// **unambiguous** aliases are listed here — bare common verbs like `关闭`
    /// / `打开` / `读取` / `写入` / `长度` / `连接` were removed because they
    /// hijacked any user function with the same name. Use the qualified form
    /// (`打开文件`, `字符串长度`) or module syntax (`字符串::字节长度`) for
    /// clarity and to avoid collisions.
    fn map_to_runtime_function(&self, name: &str) -> Option<String> {
        let runtime_func = match name {
            // String operations
            "字符串长度" => Some("qi_runtime_string_length"),
            "字符串连接" => Some("qi_runtime_string_concat"),
            "字符串切片" => Some("qi_runtime_string_slice"),
            "字符串比较" => Some("qi_runtime_string_compare"),

            // Math operations — bare 根号/绝对值/向下取整 removed (too common as
            // user identifiers). Keep explicit 平方根/求平方根 + English forms.
            "平方根" | "求平方根" | "sqrt" => Some("qi_runtime_math_sqrt"),
            "幂" | "pow" => Some("qi_runtime_math_pow"),
            "正弦" | "sin" => Some("qi_runtime_math_sin"),
            "余弦" | "cos" => Some("qi_runtime_math_cos"),
            "正切" | "tan" => Some("qi_runtime_math_tan"),
            "求绝对值" | "abs" => Some("qi_runtime_math_abs_int"),
            "向下取整" | "floor" => Some("qi_runtime_math_floor"),
            "向上取整" | "ceil" => Some("qi_runtime_math_ceil"),
            "四舍五入" | "round" => Some("qi_runtime_math_round"),

            // File I/O operations — bare 打开/读取/写入/关闭 removed; user can
            // define `函数 关闭(代理)` without it being hijacked into file close.
            "打开文件" | "open" => Some("qi_runtime_file_open"),
            "读取文件" | "读取文本" | "read" => Some("qi_runtime_file_read_string"),
            "写入文件" | "写入文本" | "write" => Some("qi_runtime_file_write_string"),
            "关闭文件" | "close" => Some("qi_runtime_file_close"),

            // Array operations
            "创建数组" | "create_array" => Some("qi_runtime_array_create"),
            "数组长度" | "array_len" => Some("qi_runtime_array_length"),

            // Type conversions
            "整数转字符串" | "int_to_string" => Some("qi_runtime_int_to_string"),
            "浮点数转字符串" | "float_to_string" => Some("qi_runtime_float_to_string"),
            "字符串转整数" | "string_to_int" => Some("qi_runtime_string_to_int"),
            "字符串转浮点数" | "string_to_float" => Some("qi_runtime_string_to_float"),
            "整数转浮点数" | "int_to_float" => Some("qi_runtime_int_to_float"),
            "浮点数转整数" | "float_to_int" => Some("qi_runtime_float_to_int"),

            // Memory operations
            "分配内存" | "alloc" => Some("qi_runtime_alloc"),
            "释放内存" | "dealloc" => Some("qi_runtime_dealloc"),

            // Print operations
            "打印" | "print" | "printf" => Some("qi_runtime_print"),
            "打印行" | "println" => Some("qi_runtime_println"),

            // Synchronization operations
            "创建等待组" | "新建等待组" | "new_waitgroup" => {
                Some("qi_runtime_waitgroup_create")
            }
            "等待组增加" | "等待组添加" | "waitgroup_add" | "添加等待" => {
                Some("qi_runtime_waitgroup_add")
            }
            "等待组完成" | "waitgroup_done" | "完成" => Some("qi_runtime_waitgroup_done"),
            "等待组等待" | "waitgroup_wait" | "等待" => Some("qi_runtime_waitgroup_wait"),

            "创建互斥锁" | "新建互斥锁" | "new_mutex" => Some("qi_runtime_mutex_create"),
            "互斥锁加锁" | "互斥锁锁定" | "mutex_lock" | "加锁" => {
                Some("qi_runtime_mutex_lock")
            }
            "互斥锁解锁" | "mutex_unlock" | "解锁" => Some("qi_runtime_mutex_unlock"),
            "尝试加锁" | "try_lock" => Some("qi_runtime_mutex_trylock"),

            // Channel operations
            "创建通道" => Some("qi_runtime_create_channel"),
            "发送" | "send" => Some("qi_runtime_channel_send"), // Default to int for now
            "接收" | "receive" => Some("qi_runtime_channel_receive"), // Default to int for now
            "关闭通道" | "close_channel" => Some("qi_runtime_channel_close"),

            // Timeout and error handling operations
            "设置超时" | "set_timeout" | "timeout" => Some("qi_runtime_set_timeout"),
            "获取时间" | "get_time" => Some("qi_runtime_get_time_ms"),
            "检查超时" | "check_timeout" => Some("qi_runtime_check_timeout"),
            "创建定时器" | "new_timer" => Some("qi_runtime_timer_create"),
            "定时器过期" | "timer_expired" => Some("qi_runtime_timer_expired"),
            "停止定时器" | "stop_timer" => Some("qi_runtime_timer_stop"),
            "重试操作" | "retry_operation" => Some("e9_87_8d_e8_af_95_e6_93_8d_e4_bd_9c"), // Chinese function name

            // Crypto operations
            "MD5哈希" | "md5" => Some("qi_crypto_md5"),
            "SHA256哈希" | "sha256" => Some("qi_crypto_sha256"),
            "SHA512哈希" | "sha512" => Some("qi_crypto_sha512"),
            "Base64编码" | "base64_encode" => Some("qi_crypto_base64_encode"),
            "Base64解码" | "base64_decode" => Some("qi_crypto_base64_decode"),
            "HMAC_SHA256" | "hmac_sha256" => Some("qi_crypto_hmac_sha256"),

            _ => None,
        };

        runtime_func.map(|s| s.to_string())
    }

    /// Infer a function return type from its body if not explicitly annotated
    /// Returns Some(llvm_ty) if a non-void type is inferred, otherwise None
    fn infer_return_type_from_body(&self, body: &[AstNode]) -> Option<String> {
        // Walk statements recursively to find the first return with a value
        fn infer_from_node(node: &AstNode) -> Option<String> {
            match node {
                AstNode::返回语句(ret) => {
                    if let Some(expr) = &ret.value {
                        if let AstNode::字面量表达式(lit) = &**expr {
                            use crate::parser::ast::LiteralValue as LV;
                            return Some(match &lit.value {
                                LV::整数(_) => "i64".to_string(),
                                LV::浮点数(_) => "double".to_string(),
                                LV::布尔(_) => "i1".to_string(),
                                LV::字符串(_) => "ptr".to_string(),
                                LV::字符(_) => "i8".to_string(),
                            });
                        }
                        return Some("i64".to_string());
                    }
                    None
                }
                AstNode::如果语句(if_stmt) => {
                    // Check then branch, then else branch
                    for s in &if_stmt.then_branch {
                        if let Some(t) = infer_from_node(s) {
                            return Some(t);
                        }
                    }
                    if let Some(else_branch) = &if_stmt.else_branch {
                        if let Some(t) = infer_from_node(else_branch) {
                            return Some(t);
                        }
                    }
                    None
                }
                AstNode::当语句(while_stmt) => {
                    for s in &while_stmt.body {
                        if let Some(t) = infer_from_node(s) {
                            return Some(t);
                        }
                    }
                    None
                }
                AstNode::循环语句(loop_stmt) => {
                    for s in &loop_stmt.body {
                        if let Some(t) = infer_from_node(s) {
                            return Some(t);
                        }
                    }
                    None
                }
                // Program and other containers
                AstNode::程序(p) => {
                    for s in &p.statements {
                        if let Some(t) = infer_from_node(s) {
                            return Some(t);
                        }
                    }
                    None
                }
                _ => None,
            }
        }

        for stmt in body {
            if let Some(t) = infer_from_node(stmt) {
                return Some(t);
            }
        }
        None
    }

    /// Build IR for an AST node
    #[allow(unreachable_patterns)]
    fn build_node(&mut self, node: &AstNode) -> Result<String, String> {
        match node {
            AstNode::程序(program) => {
                // Save the package name for function aliasing
                self.current_package_name = program.package_name.clone();

                // First, process all import statements
                for import_stmt in &program.imports {
                    self.process_import(import_stmt)?;
                }

                // Then process all statements in the program (functions, variables, etc.)
                for stmt in &program.statements {
                    self.build_node(stmt)?;
                }
                Ok("main".to_string())
            }
            AstNode::变量声明(decl) => {
                // Mangle variable names for Chinese characters.
                // NOTE: 这里先算出“裸名”(不带 %)，下面在局部声明时可能会因同函数内
                // 跨块重名而追加后缀唯一化（见 used_local_names）。
                let mut var_name = if decl.name.chars().any(|c| !c.is_ascii()) {
                    format!("%{}", self.mangle_function_name(&decl.name))
                } else {
                    format!("%{}", decl.name)
                };

                // Determine the type based on the initializer or type annotation
                // For binary expressions, we need to evaluate them first to get their type
                //
                // 模块作用域（全局 变量/常量）且初始化器不是简单字面量时：绝对不能在这里
                // build_node 求值，否则 call/load 等指令会落在任何函数之外，生成非法 LLVM IR
                // （如全局 `通道<整数>()` 会在模块顶层 emit `call ptr @qi_runtime_create_channel`）。
                // 此时只按 AST/类型注解推断类型，全局体置 null/0；运行期初始化暂不在此处接线。
                let global_non_literal_init = self.current_function_name.is_none()
                    && decl
                        .initializer
                        .as_ref()
                        .map(|init| !matches!(init.as_ref(), AstNode::字面量表达式(_)))
                        .unwrap_or(false);
                let (type_name, pre_evaluated_init) = if global_non_literal_init {
                    // 只推断类型，不发任何指令
                    let init = decl.initializer.as_ref().unwrap();
                    let ty = match init.as_ref() {
                        AstNode::数组字面量表达式(_)
                        | AstNode::字符串连接表达式(_)
                        | AstNode::结构体实例化表达式(_)
                        | AstNode::通道创建表达式(_)
                        | AstNode::闭包表达式(_)
                        | AstNode::取地址表达式(_) => "ptr".to_string(),
                        _ => self.get_llvm_type(&decl.type_annotation),
                    };
                    (ty, None)
                } else if let Some(initializer) = &decl.initializer {
                    match &**initializer {
                        AstNode::字面量表达式(literal) => {
                            let ty = match &literal.value {
                                crate::parser::ast::LiteralValue::字符串(_) => "ptr",
                                crate::parser::ast::LiteralValue::整数(_) => "i64",
                                crate::parser::ast::LiteralValue::浮点数(_) => "double",
                                crate::parser::ast::LiteralValue::布尔(_) => "i1",
                                crate::parser::ast::LiteralValue::字符(_) => "i8",
                            };
                            (ty.to_string(), None)
                        }
                        AstNode::数组字面量表达式(_) => {
                            // Array literals return pointers to arrays, so use ptr type
                            let init_value = self.build_node(&**initializer)?;
                            // Propagate the array size info from temp to variable
                            let init_var_name = init_value.trim_start_matches('%');
                            if let Some(array_size) = self.array_sizes.get(init_var_name) {
                                self.array_sizes.insert(decl.name.clone(), *array_size);
                            }
                            // Also propagate array element type
                            if let Some(element_type) = self.array_element_types.get(init_var_name)
                            {
                                self.array_element_types
                                    .insert(decl.name.clone(), element_type.clone());
                            }
                            ("ptr".to_string(), Some(init_value))
                        }
                        AstNode::二元操作表达式(_) => {
                            // Build the initializer first to determine its type
                            let init_value = self.build_node(&**initializer)?;
                            let init_var_name = init_value.trim_start_matches('%');
                            let ty = self
                                .variable_types
                                .get(init_var_name)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "i64".to_string());
                            (ty, Some(init_value))
                        }
                        AstNode::字符串连接表达式(_) => {
                            // String concatenation always returns ptr
                            let init_value = self.build_node(&**initializer)?;
                            ("ptr".to_string(), Some(init_value))
                        }
                        AstNode::函数调用表达式(call_expr) => {
                            // Check if this is a function call that returns a string or number
                            let function_name = self.get_full_function_name(call_expr);
                            let ty = if let Some(runtime_func) =
                                self.map_to_runtime_function(&function_name)
                            {
                                if runtime_func.contains("math_sqrt")
                                    || runtime_func.contains("math_pow")
                                    || runtime_func.contains("math_sin")
                                    || runtime_func.contains("math_cos")
                                    || runtime_func.contains("math_tan")
                                    || runtime_func.contains("math_floor")
                                    || runtime_func.contains("math_ceil")
                                    || runtime_func.contains("math_round")
                                    || runtime_func.contains("math_abs_float")
                                    || runtime_func.contains("int_to_float")
                                    || runtime_func.contains("string_to_float")
                                {
                                    "double"
                                } else if runtime_func.contains("string_length")
                                    || runtime_func.contains("string_to_int")
                                {
                                    "i64" // string_length and string_to_int return integer, not string
                                } else if runtime_func.starts_with("qi_crypto_")
                                    && runtime_func != "qi_crypto_free_string"
                                {
                                    "ptr" // All crypto functions return string (ptr)
                                } else if runtime_func.contains("read_string")
                                    || runtime_func.contains("int_to_string")
                                    || runtime_func.contains("float_to_string")
                                    || runtime_func.contains("string")
                                    || runtime_func == "qi_runtime_string_concat"
                                {
                                    "ptr"
                                } else if runtime_func.contains("math_abs_int")
                                    || runtime_func.contains("float_to_int")
                                    || runtime_func.contains("array_length")
                                    || runtime_func.contains("get_time_ms")
                                    || runtime_func.contains("file_open")
                                    || runtime_func.contains("file_read")
                                    || runtime_func.contains("file_write")
                                    || runtime_func.contains("tcp_connect")
                                {
                                    "i64" // Functions that return i64
                                } else if runtime_func.contains("waitgroup_create")
                                    || runtime_func.contains("mutex_create")
                                    || runtime_func.contains("rwlock_create")
                                    || runtime_func.contains("timer_create")
                                    || runtime_func.contains("create_channel")
                                    || runtime_func.contains("create_task")
                                {
                                    "ptr" // Synchronization primitives and async constructs return pointers
                                } else if runtime_func == "qi_runtime_set_timeout"
                                    || runtime_func == "qi_runtime_timer_expired"
                                    || runtime_func == "qi_runtime_timer_stop"
                                {
                                    "i64" // Timer status functions return i64
                                } else if runtime_func.contains("trylock")
                                    || runtime_func.contains("timeout")
                                    || runtime_func.contains("retry")
                                    || runtime_func.contains("catch_error")
                                    || runtime_func.contains("mutex")
                                    || runtime_func.contains("waitgroup")
                                    || runtime_func.contains("channel")
                                {
                                    "i32" // Most synchronization and status functions return i32 status codes
                                } else {
                                    "i32" // Default to i32 for unknown runtime functions (most return status codes)
                                }
                            } else if let Some(ret_type) = self
                                .function_return_types
                                .get(&self.mangle_function_name(&function_name) as &str)
                            {
                                ret_type // Use stored return type from function signature
                            } else {
                                let mangled = self.mangle_function_name(&function_name);
                                if let Some((_pt, rt)) = self
                                    .external_functions
                                    .get(&mangled as &str)
                                    .or_else(|| self.external_functions.get(&function_name as &str))
                                {
                                    rt.as_str() // Use return type from cross-module external function
                                } else if let Some(type_ann) = &decl.type_annotation {
                                    // Fall back to the variable's declared type when the callee is
                                    // unknown (e.g. higher-order calls through a function pointer)
                                    let llvm_ty = self.get_llvm_type_from_ast(type_ann);
                                    if llvm_ty == "ptr" {
                                        "ptr"
                                    } else if llvm_ty == "double" {
                                        "double"
                                    } else if llvm_ty == "i1" {
                                        "i1"
                                    } else {
                                        "i64"
                                    }
                                } else {
                                    "i64"
                                }
                            };
                            // Propagate struct type from type annotation when type is ptr
                            if ty == "ptr" {
                                if let Some(type_ann) = &decl.type_annotation {
                                    let struct_name = match type_ann {
                                        crate::parser::ast::TypeNode::自定义类型(tn) => {
                                            Some(tn.clone())
                                        }
                                        crate::parser::ast::TypeNode::结构体类型(st) => {
                                            Some(st.name.clone())
                                        }
                                        _ => None,
                                    };
                                    if let Some(sn) = struct_name {
                                        self.variable_struct_types.insert(decl.name.clone(), sn);
                                    }
                                }
                            }
                            (ty.to_string(), None)
                        }
                        AstNode::取地址表达式(_) => {
                            // Address-of expressions always return pointers
                            // Pre-evaluate to ensure we have the value
                            let init_value = self.build_node(&**initializer)?;
                            // Register the result as a pointer type
                            let temp_name = init_value.trim_start_matches('%');
                            self.variable_types
                                .insert(temp_name.to_string(), "ptr".to_string());
                            ("ptr".to_string(), Some(init_value))
                        }
                        AstNode::等待表达式(await_expr) => {
                            // Determine type from Future inner type BEFORE building the await expression
                            // This is necessary because variable allocation needs to know the type
                            let (ty, struct_name) = if let AstNode::标识符表达式(ident) =
                                await_expr.expression.as_ref()
                            {
                                let future_var = &ident.name;
                                if let Some(inner_type_info) =
                                    self.future_inner_types.get(future_var)
                                {
                                    if inner_type_info.starts_with("struct.") {
                                        // Extract struct type name
                                        let struct_name =
                                            inner_type_info.strip_prefix("struct.").unwrap();
                                        ("ptr".to_string(), Some(struct_name.to_string()))
                                    } else {
                                        // Basic type from Future inner type
                                        (inner_type_info.to_string(), None)
                                    }
                                } else {
                                    ("i64".to_string(), None)
                                }
                            } else if let AstNode::函数调用表达式(call_expr) =
                                await_expr.expression.as_ref()
                            {
                                // Awaiting a function call - infer from function's Future<T> return type
                                let function_name = self.get_full_function_name(call_expr);
                                let mangled = if function_name.chars().any(|c| !c.is_ascii()) {
                                    self.mangle_function_name(&function_name)
                                } else {
                                    function_name.clone()
                                };

                                if let Some(inner_type_info) =
                                    self.function_future_inner_types.get(&mangled)
                                {
                                    if inner_type_info.starts_with("struct.") {
                                        let struct_name =
                                            inner_type_info.strip_prefix("struct.").unwrap();
                                        ("ptr".to_string(), Some(struct_name.to_string()))
                                    } else {
                                        (inner_type_info.to_string(), None)
                                    }
                                } else {
                                    ("i64".to_string(), None)
                                }
                            } else {
                                ("i64".to_string(), None)
                            };

                            // Preserve struct type information if present
                            if let Some(struct_name) = struct_name {
                                self.variable_struct_types
                                    .insert(decl.name.clone(), struct_name);
                            }

                            // Now build the await expression
                            let init_value = self.build_node(&**initializer)?;

                            (ty, Some(init_value))
                        }
                        AstNode::闭包表达式(_) => {
                            // 闭包表达式 → ptr (堆上的 closure 对象)；额外标记会在末尾传播
                            let init_value = self.build_node(&**initializer)?;
                            ("ptr".to_string(), Some(init_value))
                        }
                        AstNode::结构体实例化表达式(struct_lit) => {
                            // Struct literals return pointers
                            let init_value = self.build_node(&**initializer)?;
                            // Also propagate the struct type info
                            let init_var_name = init_value.trim_start_matches('%');
                            if let Some(struct_type_name) =
                                self.variable_struct_types.get(init_var_name)
                            {
                                self.variable_struct_types
                                    .insert(decl.name.clone(), struct_type_name.clone());
                            }
                            ("ptr".to_string(), Some(init_value))
                        }
                        AstNode::通道创建表达式(_) => {
                            // Channel creation returns a pointer to the channel
                            let init_value = self.build_node(&**initializer)?;
                            ("ptr".to_string(), Some(init_value))
                        }
                        _ => {
                            let ty = self.get_llvm_type(&decl.type_annotation);
                            (ty.to_string(), None)
                        }
                    }
                } else {
                    let ty = self.get_llvm_type(&decl.type_annotation);
                    // For uninitialized struct-type declarations, pre-register struct type
                    let type_str = ty.to_string();
                    if type_str == "ptr" {
                        if let Some(type_ann) = &decl.type_annotation {
                            let struct_type_name = match type_ann {
                                crate::parser::ast::TypeNode::自定义类型(tn) => {
                                    Some(tn.clone())
                                }
                                crate::parser::ast::TypeNode::结构体类型(st) => {
                                    Some(st.name.clone())
                                }
                                _ => None,
                            };
                            if let Some(stn) = struct_type_name {
                                self.variable_struct_types
                                    .insert(decl.name.clone(), stn.clone());
                            }
                        }
                    }
                    (type_str, None)
                };

                // 显式类型注解是结构体/自定义类型时，权威地记录变量的结构体类型。
                // 即使初始化器是返回裸 ptr 的函数（如 列表库.获取指针 等 FFI），
                // 后续 `变量.字段` 也能拿到正确的结构体类型，而不是生成 unknown.type。
                if type_name == "ptr" {
                    if let Some(type_ann) = &decl.type_annotation {
                        let 注解结构体 = match type_ann {
                            crate::parser::ast::TypeNode::自定义类型(tn) => Some(tn.clone()),
                            crate::parser::ast::TypeNode::结构体类型(st) => Some(st.name.clone()),
                            _ => None,
                        };
                        if let Some(sn) = 注解结构体 {
                            self.variable_struct_types.insert(decl.name.clone(), sn);
                        }
                    }
                }

                // Record the variable type for later use (both original and mangled names)
                let mut mangled_name = if decl.name.chars().any(|c| !c.is_ascii()) {
                    format!(
                        "_Z_{}",
                        self.mangle_function_name(&decl.name)
                            .trim_start_matches("_Z_")
                    )
                } else {
                    decl.name.clone()
                };

                // —— 同名局部变量跨块 alloca 撞名修复 ——
                // 局部 `变量` 声明时，LLVM 的 alloca 名仅来源于（mangle 后的）变量名，
                // 于是同一函数内两个块各声明一个同名变量会生成两条 `%X = alloca`，
                // LLVM 报 "multiple definition of local value named 'X'"。
                // 解决：每个函数维护一份“已用裸名集合”，碰撞时给本次声明追加 `_N`
                // 后缀得到唯一裸名，并把它写进 variable_alias，让后续对该标识符的
                // 加载/存储/赋值解析到正确的 alloca（参考路径会先查 variable_alias）。
                // 初始化表达式在此之前已经 build 过（见上面各分支的 pre_evaluated_init），
                // 所以自引用初始化（如 `变量 值 = 值 + 1` 引用外层 值）仍解析到旧 alloca。
                // 注意：alias 值是纯 ASCII 的唯一裸名，参考路径不会再 mangle 它。
                if self.current_function_name.is_some() {
                    let bare = var_name.trim_start_matches('%').to_string();
                    if self.used_local_names.contains(&bare) {
                        let mut n = 1usize;
                        let mut unique = format!("{}_{}", bare, n);
                        while self.used_local_names.contains(&unique) {
                            n += 1;
                            unique = format!("{}_{}", bare, n);
                        }
                        self.used_local_names.insert(unique.clone());
                        var_name = format!("%{}", unique);
                        mangled_name = unique.clone();
                        // 用户标识符 → 唯一内部裸名（ASCII，不会被再次 mangle）
                        self.variable_alias.insert(decl.name.clone(), unique);
                    } else {
                        self.used_local_names.insert(bare);
                        // 该名字未撞名：清掉可能残留的旧 alias（外层同名 alias 失效），
                        // 让标识符解析回退到直接 mangle，对应本次的 alloca。
                        self.variable_alias.remove(&decl.name);
                    }
                }

                self.variable_types
                    .insert(decl.name.clone(), type_name.to_string());
                self.variable_types
                    .insert(mangled_name.clone(), type_name.to_string());
                self.record_function_pointer_signature(&decl.name, &decl.type_annotation);

                // 类型注解是 函数(...) → 变量持有 closure 对象，调用走 fat call
                if let Some(crate::parser::ast::TypeNode::函数类型(ft)) = &decl.type_annotation
                {
                    self.closure_variables.insert(decl.name.clone());
                    self.closure_variables.insert(mangled_name.clone());
                    let pts: Vec<String> = ft
                        .parameters
                        .iter()
                        .map(|p| self.get_llvm_type_from_ast(p))
                        .collect();
                    let rt = self.get_llvm_type_from_ast(&ft.return_type);
                    self.closure_signatures
                        .insert(decl.name.clone(), (pts.clone(), rt.clone()));
                    self.closure_signatures
                        .insert(mangled_name.clone(), (pts, rt));
                }

                // 闭包初始化 — 三种情况都把 LHS 标为 closure 变量：
                //   (a) RHS 直接是 `闭包(...)` 表达式
                //   (b) RHS 是返回闭包的函数调用
                if let Some(initializer) = &decl.initializer {
                    let mut should_mark = false;
                    let mut sig_to_propagate = None;

                    match initializer.as_ref() {
                        AstNode::闭包表达式(_) => {
                            if let Some(value) = pre_evaluated_init.as_ref() {
                                let rhs_key = value.trim_start_matches('%').to_string();
                                if self.closure_variables.contains(&rhs_key) {
                                    should_mark = true;
                                    sig_to_propagate =
                                        self.closure_signatures.get(&rhs_key).cloned();
                                }
                            }
                        }
                        AstNode::函数调用表达式(call) => {
                            let mangled = if call.callee.chars().any(|c| !c.is_ascii()) {
                                self.mangle_function_name(&call.callee)
                            } else {
                                call.callee.clone()
                            };
                            if self.functions_returning_closure.contains(&call.callee)
                                || self.functions_returning_closure.contains(&mangled)
                            {
                                should_mark = true;
                                // 用类型注解里的函数签名推断闭包签名
                                if let Some(crate::parser::ast::TypeNode::函数类型(ft)) =
                                    &decl.type_annotation
                                {
                                    let pts: Vec<String> = ft
                                        .parameters
                                        .iter()
                                        .map(|p| self.get_llvm_type_from_ast(p))
                                        .collect();
                                    let rt = self.get_llvm_type_from_ast(&ft.return_type);
                                    sig_to_propagate = Some((pts, rt));
                                }
                            }
                        }
                        _ => {}
                    }

                    if should_mark {
                        self.closure_variables.insert(decl.name.clone());
                        self.closure_variables.insert(mangled_name.clone());
                        if let Some(sig) = sig_to_propagate {
                            self.closure_signatures
                                .insert(decl.name.clone(), sig.clone());
                            self.closure_signatures.insert(mangled_name.clone(), sig);
                        }
                    }
                }

                // Track constant variables (is_mutable == false means it's a constant)
                if !decl.is_mutable {
                    self.constant_variables.insert(decl.name.clone());
                    self.constant_variables.insert(mangled_name.clone());
                }

                // Track Future inner types for await expressions
                // and track variables that are semantically boolean
                if let Some(type_ann) = &decl.type_annotation {
                    if let crate::parser::ast::TypeNode::未来类型(inner_type) = type_ann {
                        // For struct types, we need to preserve the struct type name
                        // so we can distinguish between string pointers and struct pointers
                        let inner_type_info = match inner_type.as_ref() {
                            crate::parser::ast::TypeNode::自定义类型(type_name) => {
                                // Store struct type name instead of just "ptr"
                                format!("struct.{}", type_name)
                            }
                            crate::parser::ast::TypeNode::结构体类型(struct_type) => {
                                // Extract struct name from StructType
                                format!("struct.{}", struct_type.name)
                            }
                            _ => {
                                // For basic types, use LLVM type
                                self.get_llvm_type_from_ast(inner_type)
                            }
                        };
                        self.future_inner_types
                            .insert(decl.name.clone(), inner_type_info.clone());
                        self.future_inner_types
                            .insert(mangled_name.clone(), inner_type_info);
                    } else if let crate::parser::ast::TypeNode::基础类型(
                        crate::parser::ast::BasicType::布尔,
                    ) = type_ann
                    {
                        // Track boolean variables (even if they end up stored as i32)
                        self.boolean_variables.insert(decl.name.clone());
                        self.boolean_variables.insert(mangled_name.clone());
                    }
                }

                // CHECK FOR GLOBAL SCOPE
                if self.current_function_name.is_none() {
                    // Global variable declaration
                    let global_name = format!("@{}", mangled_name);

                    // For global variables, we need the initializer value directly if it's a literal
                    let init_val = if let Some(initializer) = &decl.initializer {
                        match &**initializer {
                            AstNode::字面量表达式(literal) => {
                                match &literal.value {
                                    crate::parser::ast::LiteralValue::整数(i) => {
                                        Some(i.to_string())
                                    }
                                    crate::parser::ast::LiteralValue::浮点数(f) => {
                                        Some(format!("{:?}", f))
                                    } // Use Debug for full precision
                                    crate::parser::ast::LiteralValue::布尔(b) => {
                                        Some(if *b { "1".to_string() } else { "0".to_string() })
                                    }
                                    crate::parser::ast::LiteralValue::字符串(_) => {
                                        // String literals are complex globals, for now handle via build_node which creates a constant string
                                        let val = self.build_node(initializer)?;
                                        Some(val)
                                    }
                                    _ => None,
                                }
                            }
                            _ => None, // Complex initializers not supported for globals yet
                        }
                    } else {
                        // No initializer → let zero_for_ty pick the type-correct zero
                        // (null for ptr, 0.0 for double, 0 for ints/bool). Returning a bare
                        // "0" here would emit invalid IR like `global ptr 0`.
                        None
                    };

                    self.add_instruction(IrInstruction::全局变量声明 {
                        name: global_name.clone(),
                        type_name: type_name.to_string(),
                        initializer: init_val,
                        is_constant: !decl.is_mutable,
                    });

                    // Track global variables
                    self.global_variables.insert(decl.name.clone());
                    self.global_variables.insert(mangled_name.clone());

                    // Remember the global's LLVM type so it survives the per-function
                    // `variable_types` clear (see 函数声明). Without this, references to a
                    // non-integer global (string/float/bool/channel...) inside a function
                    // would default to i64 and load a pointer-as-int.
                    self.global_variable_types
                        .insert(decl.name.clone(), type_name.to_string());
                    self.global_variable_types
                        .insert(mangled_name.clone(), type_name.to_string());

                    Ok(global_name)
                } else {
                    // Local variable declaration
                    self.add_instruction(IrInstruction::分配 {
                        dest: var_name.clone(),
                        type_name: type_name.to_string(),
                    });

                    // Initialize if there's an initializer
                    if let Some(initializer) = &decl.initializer {
                        // Use pre-evaluated value if available, otherwise evaluate now
                        let value = if let Some(pre_eval) = pre_evaluated_init {
                            pre_eval
                        } else {
                            self.build_node(initializer)?
                        };

                        // If the initializer is an array, propagate element type info
                        let value_name = value.trim_start_matches('%');
                        if let Some(elem_type) = self.array_element_types.get(value_name).cloned() {
                            self.array_element_types
                                .insert(decl.name.clone(), elem_type.clone());
                            self.array_element_types
                                .insert(mangled_name.clone(), elem_type);
                        }

                        // Propagate struct type from source temp to the declared variable
                        if let Some(src_struct_type) =
                            self.variable_struct_types.get(value_name).cloned()
                        {
                            self.variable_struct_types
                                .insert(decl.name.clone(), src_struct_type.clone());
                            self.variable_struct_types
                                .insert(mangled_name.clone(), src_struct_type);
                        }

                        // Determine the actual type of the value being stored
                        let value_var_name = value.trim_start_matches('%');
                        let inferred_value_type = self
                            .variable_types
                            .get(value_var_name)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| type_name.to_string());
                        // If the variable has a type annotation (type_name != ""), and the annotation
                        // says "ptr" but the inferred type says something else (e.g. "i64"), prefer
                        // the annotation type. This handles cases where stdlib functions are
                        // incorrectly inferred as returning i64 instead of ptr (string).
                        let actual_value_type = if type_name == "ptr"
                            && inferred_value_type != "ptr"
                            && inferred_value_type != "double"
                        {
                            "ptr".to_string()
                        } else {
                            inferred_value_type
                        };

                        self.add_instruction(IrInstruction::存储 {
                            target: var_name.clone(),
                            value,
                            value_type: Some(actual_value_type),
                        });
                    }

                    Ok(var_name)
                }
            }
            AstNode::函数声明(func_decl) => {
                // §4-2 异步函数 语义检查（docs/编译器异步状态机里程碑.md）：
                // 异步 函数 必须返回 未来<T>，不能同时是 内联。
                // codegen 阶段强制——TypeChecker 当前未接入 compile 流水线，所以
                // 语义错误必须在这里抛，否则用户写错没人提示。
                if func_decl.is_async {
                    if func_decl.is_inline {
                        return Err(format!(
                            "函数 `{}`：异步函数 不能同时是 内联（异步需要状态机帧，内联会展开抹掉）",
                            func_decl.name
                        ));
                    }
                    match &func_decl.return_type {
                        Some(crate::parser::ast::TypeNode::未来类型(_)) => { /* OK */ }
                        Some(other) => {
                            return Err(format!(
                                "函数 `{}`：异步函数 必须返回 未来<T>，当前返回类型不是 未来<T> ({:?})",
                                func_decl.name, other
                            ));
                        }
                        None => {
                            return Err(format!(
                                "函数 `{}`：异步函数 必须显式声明返回类型 未来<T>",
                                func_decl.name
                            ));
                        }
                    }

                    // §4-4 单 await 子集（multi-state 状态机首次落地）：
                    //   异步 函数 X(args*): 未来<整数> {
                    //       变量 a: 整数 = 等待 Y();   // Y 必须零参数 + 返回 未来<整数>
                    //       返回 a;                     // 直接透传
                    //   }
                    // §4-4 增量 5：多 await 串联 (K ≥ 2)。先尝试 multi-await 路径，
                    // 没命中再 fall through 到 single-await。
                    if let Some((awaited_calls, _locals, ret_form)) =
                        multi_await_async_body(func_decl)
                    {
                        let mangled = if func_decl.name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&func_decl.name)
                        } else {
                            func_decl.name.clone()
                        };
                        // mangle each awaited fn name up front to avoid closure borrow conflicts
                        let awaited_mangled_names: Vec<String> = awaited_calls
                            .iter()
                            .map(|(name, _)| {
                                if name.chars().any(|c| !c.is_ascii()) {
                                    self.mangle_function_name(name)
                                } else {
                                    name.clone()
                                }
                            })
                            .collect();
                        let emit_raw = |this: &mut Self, line: &str| {
                            this.add_instruction(IrInstruction::标签 {
                                name: format!("{} ;", line),
                            });
                        };

                        let k = awaited_calls.len(); // K 个 await
                        let n_args = func_decl.parameters.len() as i64;
                        // frame: state@0 + ret_fut@8 + args@16..16+8N + slots@16+8N..16+8N+8K
                        let args_start: i64 = 16;
                        let slots_start: i64 = args_start + 8 * n_args;
                        let frame_size: i64 = slots_start + 8 * k as i64;

                        // entry fn
                        let param_decls: Vec<String> =
                            (0..n_args).map(|i| format!("i64 %a{}", i)).collect();
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("define ptr @{}({}) {{", mangled, param_decls.join(", ")),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "entry:".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  %frame = call ptr @qi_async_alloc_frame(i64 {})",
                                frame_size
                            ),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %fut = call ptr @qi_future_pending()".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %ret_p = getelementptr i8, ptr %frame, i64 8".to_string(),
                        });
                        emit_raw(self, "  store ptr %fut, ptr %ret_p");
                        for i in 0..n_args {
                            let off = args_start + 8 * i;
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %arg{}_p = getelementptr i8, ptr %frame, i64 {}",
                                    i, off
                                ),
                            });
                            emit_raw(self, &format!("  store i64 %a{}, ptr %arg{}_p", i, i));
                        }
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_async_spawn_poll(ptr @{}_poll, ptr %frame)",
                                mangled
                            ),
                        );
                        emit_raw(self, "  ret ptr %fut");
                        self.add_instruction(IrInstruction::标签 {
                            name: "}".to_string(),
                        });

                        // poll fn — K+1 个 state（0..K）
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("define void @{}_poll(ptr %pframe) {{", mangled),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "entry:".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %state_p = getelementptr i8, ptr %pframe, i64 0".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %state = load i64, ptr %state_p".to_string(),
                        });
                        let switch_cases: String = (0..=k)
                            .map(|i| format!("i64 {}, label %s{}", i, i))
                            .collect::<Vec<_>>()
                            .join(" ");
                        emit_raw(
                            self,
                            &format!("  switch i64 %state, label %s0 [ {} ]", switch_cases),
                        );

                        // states 0..K-1：调 awaited_i + suspend
                        for i in 0..k {
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("s{}:", i),
                            });
                            // 进入 state i (i>=1) 时把 slot_{i-1} 从 future 转 i64 值（前个 await 的结果）
                            if i >= 1 {
                                let prev_slot_off = slots_start + 8 * (i as i64 - 1);
                                self.add_instruction(IrInstruction::标签 {
                                    name: format!(
                                        "  %prev{}_p = getelementptr i8, ptr %pframe, i64 {}",
                                        i, prev_slot_off
                                    ),
                                });
                                self.add_instruction(IrInstruction::标签 {
                                    name: format!("  %prev{}_fut = load ptr, ptr %prev{}_p", i, i),
                                });
                                self.add_instruction(IrInstruction::标签 {
                                    name: format!(
                                        "  %prev{}_val = call i64 @qi_future_value_i64(ptr %prev{}_fut)",
                                        i, i
                                    ),
                                });
                                emit_raw(
                                    self,
                                    &format!("  store i64 %prev{}_val, ptr %prev{}_p", i, i),
                                );
                            }
                            // 调 awaited_i
                            let (_awaited_name, awaited_args) = &awaited_calls[i];
                            let awaited_mangled = &awaited_mangled_names[i];
                            let awaited_args_str: String = awaited_args
                                .iter()
                                .map(|n| format!("i64 {}", n))
                                .collect::<Vec<_>>()
                                .join(", ");
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %fut_call{} = call ptr @{}({})",
                                    i, awaited_mangled, awaited_args_str
                                ),
                            });
                            let slot_off = slots_start + 8 * i as i64;
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %slot{}_p = getelementptr i8, ptr %pframe, i64 {}",
                                    i, slot_off
                                ),
                            });
                            emit_raw(
                                self,
                                &format!("  store ptr %fut_call{}, ptr %slot{}_p", i, i),
                            );
                            // state ← i+1
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %sp_s{} = getelementptr i8, ptr %pframe, i64 0",
                                    i
                                ),
                            });
                            emit_raw(self, &format!("  store i64 {}, ptr %sp_s{}", i + 1, i));
                            emit_raw(
                                self,
                                &format!(
                                    "  call void @qi_future_register_waker(ptr %fut_call{}, ptr @{}_poll, ptr %pframe)",
                                    i, mangled
                                ),
                            );
                            emit_raw(self, "  ret void");
                        }

                        // state K：所有 await 完成，先把最后一个 slot 转值，然后 load 全部，算 EXPR
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("s{}:", k),
                        });
                        // 把 slot_{K-1} 从 future 转 i64
                        let last_slot_off = slots_start + 8 * (k as i64 - 1);
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  %lastpre_p = getelementptr i8, ptr %pframe, i64 {}",
                                last_slot_off
                            ),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %lastpre_fut = load ptr, ptr %lastpre_p".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name:
                                "  %lastpre_val = call i64 @qi_future_value_i64(ptr %lastpre_fut)"
                                    .to_string(),
                        });
                        emit_raw(self, "  store i64 %lastpre_val, ptr %lastpre_p");
                        // load 全部 slot 作为 i64
                        for i in 0..k {
                            let off = slots_start + 8 * i as i64;
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %loc{}_p = getelementptr i8, ptr %pframe, i64 {}",
                                    i, off
                                ),
                            });
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("  %loc{} = load i64, ptr %loc{}_p", i, i),
                            });
                        }
                        // load 全部 外层 param
                        for i in 0..n_args {
                            let off = args_start + 8 * i;
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %parg{}_p = getelementptr i8, ptr %pframe, i64 {}",
                                    i, off
                                ),
                            });
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("  %parg{} = load i64, ptr %parg{}_p", i, i),
                            });
                        }
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %pret_pK = getelementptr i8, ptr %pframe, i64 8".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %pretK = load ptr, ptr %pret_pK".to_string(),
                        });

                        // operand → IR
                        let operand_str = |op: AwaitOperand| -> String {
                            match op {
                                AwaitOperand::Literal(n) => format!("{}", n),
                                AwaitOperand::Awaited => "%loc0".to_string(), // 单 await 兼容
                                AwaitOperand::Local(i) => format!("%loc{}", i),
                                AwaitOperand::Param(i) => format!("%parg{}", i),
                            }
                        };
                        let final_val: String = match ret_form {
                            AwaitReturn::Single(op) => operand_str(op),
                            AwaitReturn::BinOp(op, l, r) => {
                                let l_str = operand_str(l);
                                let r_str = operand_str(r);
                                let llvm_op = if op == '+' { "add" } else { "mul" };
                                if let (AwaitOperand::Literal(ln), AwaitOperand::Literal(rn)) =
                                    (l, r)
                                {
                                    let val = if op == '+' { ln + rn } else { ln * rn };
                                    format!("{}", val)
                                } else {
                                    self.add_instruction(IrInstruction::标签 {
                                        name: format!(
                                            "  %final_valK = {} i64 {}, {}",
                                            llvm_op, l_str, r_str
                                        ),
                                    });
                                    "%final_valK".to_string()
                                }
                            }
                        };
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_future_complete_i64(ptr %pretK, i64 {})",
                                final_val
                            ),
                        );
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_async_free_frame(ptr %pframe, i64 {})",
                                frame_size
                            ),
                        );
                        emit_raw(self, "  ret void");
                        self.add_instruction(IrInstruction::标签 {
                            name: "}".to_string(),
                        });

                        // 注册签名
                        let param_types: Vec<String> =
                            (0..n_args).map(|_| "i64".to_string()).collect();
                        self.function_param_types
                            .insert(mangled.clone(), param_types);
                        self.function_parameters
                            .insert(mangled.clone(), func_decl.parameters.clone());
                        self.function_return_types
                            .insert(mangled.clone(), "ptr".to_string());
                        self.function_future_inner_types
                            .insert(mangled.clone(), "i64".to_string());
                        self.defined_functions.insert(mangled.clone());

                        return Ok(String::new());
                    }

                    if let Some((local_name, awaited_fn, awaited_args, ret_form)) =
                        single_await_async_body(func_decl)
                    {
                        let _ = local_name; // 仅用于检测，IR 里用 SSA 名 %final_val
                        let mangled = if func_decl.name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&func_decl.name)
                        } else {
                            func_decl.name.clone()
                        };
                        let awaited_mangled = if awaited_fn.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&awaited_fn)
                        } else {
                            awaited_fn.clone()
                        };
                        let emit_raw = |this: &mut Self, line: &str| {
                            this.add_instruction(IrInstruction::标签 {
                                name: format!("{} ;", line),
                            });
                        };

                        // frame layout：
                        //   state(i64@0) + return_future(ptr@8)
                        //   + args(i64*N @16..16+8N) + awaited_future(ptr@16+8N)
                        let n_args = func_decl.parameters.len() as i64;
                        let args_start: i64 = 16;
                        let awaited_off: i64 = args_start + 8 * n_args;
                        let frame_size: i64 = awaited_off + 8; // awaited 占 8 字节
                        let frame_size = (frame_size + 7) & !7; // 8 字节对齐（已经对齐）

                        // entry fn — 含 N 个 i64 参数
                        let param_decls: Vec<String> =
                            (0..n_args).map(|i| format!("i64 %a{}", i)).collect();
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("define ptr @{}({}) {{", mangled, param_decls.join(", ")),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "entry:".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  %frame = call ptr @qi_async_alloc_frame(i64 {})",
                                frame_size
                            ),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %fut = call ptr @qi_future_pending()".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %ret_p = getelementptr i8, ptr %frame, i64 8".to_string(),
                        });
                        emit_raw(self, "  store ptr %fut, ptr %ret_p");
                        // 把外层 args 写进 frame[16 + 8*i]
                        for i in 0..n_args {
                            let off = args_start + 8 * i;
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %arg{}_p = getelementptr i8, ptr %frame, i64 {}",
                                    i, off
                                ),
                            });
                            emit_raw(self, &format!("  store i64 %a{}, ptr %arg{}_p", i, i));
                        }
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_async_spawn_poll(ptr @{}_poll, ptr %frame)",
                                mangled
                            ),
                        );
                        emit_raw(self, "  ret ptr %fut");
                        self.add_instruction(IrInstruction::标签 {
                            name: "}".to_string(),
                        });

                        // poll fn — multi-state with switch on frame[0]
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("define void @{}_poll(ptr %pframe) {{", mangled),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "entry:".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %state_p = getelementptr i8, ptr %pframe, i64 0".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %state = load i64, ptr %state_p".to_string(),
                        });
                        emit_raw(
                            self,
                            "  switch i64 %state, label %s0 [ i64 0, label %s0 i64 1, label %s1 ]",
                        );

                        // state 0：call awaited，store + register waker → ret
                        self.add_instruction(IrInstruction::标签 {
                            name: "s0:".to_string(),
                        });
                        let awaited_args_str: String = awaited_args
                            .iter()
                            .map(|n| format!("i64 {}", n))
                            .collect::<Vec<_>>()
                            .join(", ");
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  %fut1 = call ptr @{}({})",
                                awaited_mangled, awaited_args_str
                            ),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  %await_p = getelementptr i8, ptr %pframe, i64 {}",
                                awaited_off
                            ),
                        });
                        emit_raw(self, "  store ptr %fut1, ptr %await_p");
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %sp = getelementptr i8, ptr %pframe, i64 0".to_string(),
                        });
                        emit_raw(self, "  store i64 1, ptr %sp");
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_future_register_waker(ptr %fut1, ptr @{}_poll, ptr %pframe)",
                                mangled
                            ),
                        );
                        emit_raw(self, "  ret void");

                        // state 1：load awaited + 外层 args，按 ret_form 算结果
                        self.add_instruction(IrInstruction::标签 {
                            name: "s1:".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  %await_p2 = getelementptr i8, ptr %pframe, i64 {}",
                                awaited_off
                            ),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %fut2 = load ptr, ptr %await_p2".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %a_val = call i64 @qi_future_value_i64(ptr %fut2)".to_string(),
                        });
                        // load 外层 args（按需）
                        for i in 0..n_args {
                            let off = args_start + 8 * i;
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %parg{}_p = getelementptr i8, ptr %pframe, i64 {}",
                                    i, off
                                ),
                            });
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("  %parg{} = load i64, ptr %parg{}_p", i, i),
                            });
                        }
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %pret_p2 = getelementptr i8, ptr %pframe, i64 8".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %pret2 = load ptr, ptr %pret_p2".to_string(),
                        });

                        // operand → IR 表示
                        let operand_str = |op: AwaitOperand| -> String {
                            match op {
                                AwaitOperand::Literal(n) => format!("{}", n),
                                AwaitOperand::Awaited => "%a_val".to_string(),
                                AwaitOperand::Local(_) => "%a_val".to_string(), // 单 await 路径用 a_val 别名
                                AwaitOperand::Param(i) => format!("%parg{}", i),
                            }
                        };

                        let final_val: String = match ret_form {
                            AwaitReturn::Single(op) => operand_str(op),
                            AwaitReturn::BinOp(op, l, r) => {
                                let l_str = operand_str(l);
                                let r_str = operand_str(r);
                                let llvm_op = if op == '+' { "add" } else { "mul" };
                                // 若两边都是字面量，常量折叠（避免 add i64 N, M 这种）
                                if let (AwaitOperand::Literal(ln), AwaitOperand::Literal(rn)) =
                                    (l, r)
                                {
                                    let val = if op == '+' { ln + rn } else { ln * rn };
                                    format!("{}", val)
                                } else {
                                    self.add_instruction(IrInstruction::标签 {
                                        name: format!(
                                            "  %final_val = {} i64 {}, {}",
                                            llvm_op, l_str, r_str
                                        ),
                                    });
                                    "%final_val".to_string()
                                }
                            }
                        };
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_future_complete_i64(ptr %pret2, i64 {})",
                                final_val
                            ),
                        );
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_async_free_frame(ptr %pframe, i64 {})",
                                frame_size
                            ),
                        );
                        emit_raw(self, "  ret void");
                        self.add_instruction(IrInstruction::标签 {
                            name: "}".to_string(),
                        });

                        // 注册函数签名（外层参数 N 个 i64）
                        let param_types: Vec<String> =
                            (0..n_args).map(|_| "i64".to_string()).collect();
                        self.function_param_types
                            .insert(mangled.clone(), param_types);
                        self.function_parameters
                            .insert(mangled.clone(), func_decl.parameters.clone());
                        self.function_return_types
                            .insert(mangled.clone(), "ptr".to_string());
                        self.function_future_inner_types
                            .insert(mangled.clone(), "i64".to_string());
                        self.defined_functions.insert(mangled.clone());

                        return Ok(String::new());
                    }

                    // §4-3 状态机 codegen MVP — 现支持
                    //   返回 字面量 / 返回 参数 / 返回 参数+字面量 / 返回 参数*字面量
                    //   参数全 i64
                    // 其他形态（多返回 / 等待 / 局部变量 / 任意表达式）下次会话 §4-4 扩
                    if let Some(ret_form) = trivial_async_int_body(func_decl) {
                        let mangled = if func_decl.name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&func_decl.name)
                        } else {
                            func_decl.name.clone()
                        };
                        // emit_label 行尾加 `;` 注释，绕开 IrInstruction::标签 给非
                        // instruction/label 行自动加冒号的逻辑（store/call void/ret 都没
                        // ` = ` 也不以 `:`/`@` 结尾）。LLVM IR `;` 起行尾注释。
                        let emit_raw = |this: &mut Self, line: &str| {
                            this.add_instruction(IrInstruction::标签 {
                                name: format!("{} ;", line),
                            });
                        };

                        // frame layout：state(i64@0) + return_future(ptr@8) + args(i64*N @16+)
                        let n_args = func_decl.parameters.len();
                        let frame_size = 16 + 8 * n_args as i64;

                        // 入口 fn 参数列表 + mangled 名
                        let param_decls: Vec<String> =
                            (0..n_args).map(|i| format!("i64 %a{}", i)).collect();
                        let param_decls_str = param_decls.join(", ");

                        // entry fn
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("define ptr @{}({}) {{", mangled, param_decls_str),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "entry:".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  %frame = call ptr @qi_async_alloc_frame(i64 {})",
                                frame_size
                            ),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %fut = call ptr @qi_future_pending()".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %ret_p = getelementptr i8, ptr %frame, i64 8".to_string(),
                        });
                        emit_raw(self, "  store ptr %fut, ptr %ret_p");
                        // 把每个参数写进 frame[16 + 8*i]
                        for i in 0..n_args {
                            let off = 16 + 8 * i as i64;
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %arg{}_p = getelementptr i8, ptr %frame, i64 {}",
                                    i, off
                                ),
                            });
                            emit_raw(self, &format!("  store i64 %a{}, ptr %arg{}_p", i, i));
                        }
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_async_spawn_poll(ptr @{}_poll, ptr %frame)",
                                mangled
                            ),
                        );
                        emit_raw(self, "  ret ptr %fut");
                        self.add_instruction(IrInstruction::标签 {
                            name: "}".to_string(),
                        });

                        // poll fn：从 frame 读 return_future + 各参数，按 ret_form 计算结果
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("define void @{}_poll(ptr %pframe) {{", mangled),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "entry:".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %pret_p = getelementptr i8, ptr %pframe, i64 8".to_string(),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: "  %pret = load ptr, ptr %pret_p".to_string(),
                        });
                        // 按需 load 参数（仅当 ret_form 用到时；为简化全 load）
                        for i in 0..n_args {
                            let off = 16 + 8 * i as i64;
                            self.add_instruction(IrInstruction::标签 {
                                name: format!(
                                    "  %parg{}_p = getelementptr i8, ptr %pframe, i64 {}",
                                    i, off
                                ),
                            });
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("  %parg{} = load i64, ptr %parg{}_p", i, i),
                            });
                        }
                        // 算 result（i64）— 按 ret_form 形态分别 emit
                        let result_expr: String = match ret_form {
                            AsyncReturn::Literal(n) => format!("{}", n),
                            AsyncReturn::Param(i) => {
                                // load 已做，直接用
                                format!("%parg{}", i)
                            }
                            AsyncReturn::ParamAddLit(i, n) => {
                                self.add_instruction(IrInstruction::标签 {
                                    name: format!("  %presult = add i64 %parg{}, {}", i, n),
                                });
                                "%presult".to_string()
                            }
                            AsyncReturn::ParamMulLit(i, n) => {
                                self.add_instruction(IrInstruction::标签 {
                                    name: format!("  %presult = mul i64 %parg{}, {}", i, n),
                                });
                                "%presult".to_string()
                            }
                        };
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_future_complete_i64(ptr %pret, i64 {})",
                                result_expr
                            ),
                        );
                        emit_raw(
                            self,
                            &format!(
                                "  call void @qi_async_free_frame(ptr %pframe, i64 {})",
                                frame_size
                            ),
                        );
                        emit_raw(self, "  ret void");
                        self.add_instruction(IrInstruction::标签 {
                            name: "}".to_string(),
                        });

                        // 注册函数签名 — caller 用原名调用，签名跟入口 fn 对齐
                        let param_types: Vec<String> =
                            (0..n_args).map(|_| "i64".to_string()).collect();
                        self.function_param_types
                            .insert(mangled.clone(), param_types);
                        self.function_parameters
                            .insert(mangled.clone(), func_decl.parameters.clone());
                        self.function_return_types
                            .insert(mangled.clone(), "ptr".to_string());
                        self.function_future_inner_types
                            .insert(mangled.clone(), "i64".to_string());
                        self.defined_functions.insert(mangled.clone());

                        return Ok(String::new());
                    }
                    // 非 MVP 形态，fall through 走旧路径（sync wrap with qi_future_ready_*）。
                    // 旧路径仍然 work，只是不是真状态机。下次会话扩 §4-4 后改。
                }

                // Handle special cases and apply name mangling for Chinese function names
                let func_name: String = match func_decl.name.as_str() {
                    "入口" => {
                        if self.is_entry_module {
                            "main".to_string()
                        } else {
                            self.mangle_function_name("入口")
                        }
                    } // Special case for main function
                    name => {
                        if name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(name)
                        } else {
                            name.to_string()
                        }
                    }
                };

                let is_main =
                    (func_decl.name == "入口" || func_name == "main") && self.is_entry_module;

                // Clear variable types for this new function scope
                // (but keep function_param_types, function_return_types, etc.)
                // IMPORTANT: Preserve temp variable types (t1, t2, etc.) as they may be referenced
                // by instructions from previous functions during IR emission
                self.variable_types.retain(|k, _| {
                    k.starts_with('t') && k[1..].chars().all(|c| c.is_ascii_digit())
                });
                self.variable_struct_types.clear();

                // Global variables are module-scoped: their types must remain visible
                // inside every function body. Re-seed them after clearing the local scope
                // so loads/prints of a global resolve to its real type (ptr/double/i1/...)
                // instead of defaulting to i64.
                for (k, v) in &self.global_variable_types {
                    self.variable_types.insert(k.clone(), v.clone());
                }

                // Build parameter list with mangled names for Chinese identifiers
                // For array parameters, add hidden length parameters
                let mut params: Vec<String> = Vec::new();
                for p in &func_decl.parameters {
                    let element_type_str = self.get_llvm_type(&p.type_annotation);
                    let type_str = if p.is_variadic {
                        "ptr".to_string()
                    } else {
                        element_type_str.clone()
                    };
                    let bare_param_name = self.mangled_bare_name(&p.name);
                    let param_name = format!("%{}", bare_param_name);
                    params.push(format!("{} {}", type_str, param_name));

                    // If this is an array or variadic parameter, add a hidden length parameter
                    if let Some(ref type_ann) = p.type_annotation {
                        if p.is_variadic
                            || matches!(type_ann, crate::parser::ast::TypeNode::数组类型(_))
                        {
                            let length_param_name = format!("{}_length", param_name);
                            params.push(format!("i64 {}", length_param_name));
                        }
                    }
                }

                // Mark parameters as direct values (not pointers) in variable_types
                for param in &func_decl.parameters {
                    let param_name = if param.name.chars().any(|c| !c.is_ascii()) {
                        self.mangle_function_name(&param.name)
                    } else {
                        param.name.clone()
                    };
                    let element_type_str = self.get_llvm_type(&param.type_annotation);
                    let type_str = if param.is_variadic {
                        "ptr".to_string()
                    } else {
                        element_type_str.clone()
                    };
                    // Store with a special prefix to indicate this is a parameter (direct value)
                    self.variable_types
                        .insert(format!("param_{}", param_name), type_str.clone());
                    self.variable_types
                        .insert(param_name.clone(), type_str.clone());
                    self.record_function_pointer_signature(&param.name, &param.type_annotation);

                    // 函数指针类型参数 — 自动标 closure_variable，调用时走 fat call
                    if let Some(crate::parser::ast::TypeNode::函数类型(ft)) = &param.type_annotation
                    {
                        self.closure_variables.insert(param.name.clone());
                        self.closure_variables.insert(param_name.clone());
                        let pts: Vec<String> = ft
                            .parameters
                            .iter()
                            .map(|p| self.get_llvm_type_from_ast(p))
                            .collect();
                        let rt = self.get_llvm_type_from_ast(&ft.return_type);
                        self.closure_signatures
                            .insert(param.name.clone(), (pts.clone(), rt.clone()));
                        self.closure_signatures
                            .insert(param_name.clone(), (pts, rt));
                    }

                    if let Some(ref type_ann) = param.type_annotation {
                        match type_ann {
                            crate::parser::ast::TypeNode::自定义类型(type_name) => {
                                self.variable_struct_types
                                    .insert(param_name.clone(), type_name.clone());
                            }
                            crate::parser::ast::TypeNode::结构体类型(struct_type) => {
                                self.variable_struct_types
                                    .insert(param_name.clone(), struct_type.name.clone());
                            }
                            _ => {}
                        }
                    }

                    // If this is an array or variadic parameter, mark it as having a dynamic length from the hidden parameter
                    if let Some(ref type_ann) = param.type_annotation {
                        if param.is_variadic
                            || matches!(type_ann, crate::parser::ast::TypeNode::数组类型(_))
                        {
                            // Store a special marker that this array's length comes from a parameter
                            // We'll use this marker when accessing .长度
                            let length_param_name = format!("{}_length", param_name);
                            self.variable_types
                                .insert(length_param_name.clone(), "i64".to_string());
                            self.variable_types
                                .insert(format!("param_{}", length_param_name), "i64".to_string());
                            let array_element_type = if param.is_variadic {
                                element_type_str.clone()
                            } else {
                                self.get_array_element_llvm_type(&param.type_annotation)
                                    .unwrap_or_else(|| "i64".to_string())
                            };
                            self.array_element_types
                                .insert(param_name.clone(), array_element_type);
                        }
                    }

                    if self.verbose {
                        eprintln!(
                            "[DEBUG] Function {} parameter: {} -> type {}",
                            func_name, param_name, type_str
                        );
                    }
                }

                let params_str = if params.is_empty() {
                    String::new()
                } else {
                    format!(" {}", params.join(", "))
                };

                // Determine return type
                let return_type = if is_main {
                    "i32".to_string()
                } else if let Some(_) = func_decl.return_type {
                    self.get_return_type(&func_decl.return_type)
                } else {
                    // Infer from body if there's an explicit return with a value
                    self.infer_return_type_from_body(&func_decl.body)
                        .unwrap_or_else(|| "void".to_string())
                };

                // Record the function's parameter types for later function calls
                let mut param_types: Vec<String> = Vec::new();
                for p in &func_decl.parameters {
                    if p.is_variadic {
                        param_types.push("ptr".to_string());
                        param_types.push("i64".to_string());
                    } else {
                        param_types.push(self.get_llvm_type(&p.type_annotation));
                    }
                }
                self.function_param_types
                    .insert(func_name.clone(), param_types);
                self.function_parameters
                    .insert(func_name.clone(), func_decl.parameters.clone());

                // Record the function's return type for later function calls
                self.function_return_types
                    .insert(func_name.clone(), return_type.clone());

                // Track which functions return struct pointers, for variable_struct_types propagation
                if return_type == "ptr" {
                    if let Some(ref ret_type_node) = func_decl.return_type {
                        let struct_name = match ret_type_node {
                            crate::parser::ast::TypeNode::自定义类型(tn) => Some(tn.clone()),
                            crate::parser::ast::TypeNode::结构体类型(st) => {
                                Some(st.name.clone())
                            }
                            _ => None,
                        };
                        if let Some(sn) = struct_name {
                            self.function_return_struct_types
                                .insert(func_name.clone(), sn);
                        }
                    }
                }

                // If function returns Future<T>, track the inner type
                if let Some(ref ret_type_node) = func_decl.return_type {
                    if let crate::parser::ast::TypeNode::未来类型(inner_type) = ret_type_node {
                        let inner_llvm_type = self.get_llvm_type_from_ast(inner_type);
                        self.function_future_inner_types
                            .insert(func_name.clone(), inner_llvm_type);
                    }
                }

                // Record this function as defined in current module
                self.defined_functions.insert(func_name.clone());

                // Set current function name for return statement processing
                self.current_function_name = Some(func_name.clone());
                // 新函数体：清空“已用局部 alloca 名”集合，避免跨函数误判撞名
                self.used_local_names.clear();

                // Set AST return type for Future wrapping detection
                self.current_function_ast_return_type = func_decl.return_type.clone();

                // Add function header label
                self.add_instruction(IrInstruction::标签 {
                    name: format!("define {} @{}({}) {{", return_type, func_name, params_str),
                });

                // Add entry block
                self.add_instruction(IrInstruction::标签 {
                    name: "entry:".to_string(),
                });

                // If this is main, initialize the runtime
                if is_main {
                    let init_result = self.generate_temp();
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(init_result),
                        callee: "qi_runtime_initialize".to_string(),
                        arguments: vec![],
                    });
                }

                // Remember current instruction index to detect explicit returns
                let start_len = self.instructions.len();

                // Process function body
                for stmt in &func_decl.body {
                    self.build_node(stmt)?;
                }

                // Check if the last instruction is a terminator
                let last_is_terminator = if let Some(last) = self.instructions.last() {
                    matches!(
                        last,
                        IrInstruction::返回 { .. }
                            | IrInstruction::跳转 { .. }
                            | IrInstruction::条件跳转 { .. }
                            | IrInstruction::不可达
                    )
                } else {
                    false
                };

                // Add implicit return if needed
                if !last_is_terminator {
                    if is_main {
                        // Call runtime shutdown before returning from main
                        let shutdown_result = self.generate_temp();
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(shutdown_result),
                            callee: "qi_runtime_shutdown".to_string(),
                            arguments: vec![],
                        });
                        // main returns i32 0 by default
                        self.add_instruction(IrInstruction::返回 {
                            value: Some("0".to_string()),
                        });
                    } else if return_type == "void" {
                        // Non-main, no explicit return -> ret void
                        self.add_instruction(IrInstruction::返回 { value: None });
                    } else {
                        // Non-void function but no explicit return: return zero of the type (simple default)
                        let zero_val = match return_type.as_str() {
                            "i1" => "0",
                            "i8" => "0",
                            "i32" => "0",
                            "i64" => "0",
                            "double" => "0.0",
                            "ptr" => "null",
                            _ => "0",
                        };
                        self.add_instruction(IrInstruction::返回 {
                            value: Some(zero_val.to_string()),
                        });
                    }
                }

                // // Function is already properly closed by function body processing
                self.add_instruction(IrInstruction::标签 {
                    name: "}".to_string(),
                });

                // Clear current function name and return type after function ends
                self.current_function_name = None;
                self.current_function_ast_return_type = None;

                // Add module-qualified alias if we have a package name and this is not main.
                // 入口模块跳过：其本地函数主符号本就已按包名修饰（见 mangle_function_name），
                // 再发别名会得到 `@pkg_fn = alias ptr @pkg_fn` 自引用。库模块仍需别名以支持
                // 模块限定调用解析到裸主符号。
                if let Some(package_name) = &self.current_package_name {
                    if !is_main && !self.is_entry_module {
                        // Create an alias: @数学_最大值 = alias i64 (i64, i64), ptr @最大值
                        let alias_name = format!("{}_{}", package_name, &func_decl.name);
                        let alias_mangled = self.mangle_function_name(&alias_name);

                        // Build parameter types list (without parameter names)
                        let param_types: Vec<String> = func_decl
                            .parameters
                            .iter()
                            .map(|p| self.get_llvm_type(&p.type_annotation))
                            .collect();
                        let param_types_str = param_types.join(", ");

                        // Generate function signature for alias
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "@{} = alias {} ({}), ptr @{}",
                                alias_mangled, return_type, param_types_str, func_name
                            ),
                        });
                    }
                }

                Ok(func_name.to_string())
            }
            AstNode::返回语句(return_stmt) => {
                let value = if let Some(expr) = &return_stmt.value {
                    Some(self.build_node(expr)?)
                } else {
                    None
                };

                // Check if current function returns a Future type
                // If so, wrap the return value with the appropriate qi_future_ready_* function
                let final_value =
                    if let Some(ref ast_return_type) = self.current_function_ast_return_type {
                        if let crate::parser::ast::TypeNode::未来类型(inner_type) = ast_return_type
                        {
                            // Function returns Future<T>, wrap the return value
                            let ready_func = self.get_future_ready_function(inner_type);

                            if let Some(val) = value {
                                let future_temp = self.generate_temp();

                                // Determine arguments based on function type
                                let args = if ready_func == "qi_future_ready_string" {
                                    // String: for now just pass the pointer directly
                                    // TODO: need proper string length handling
                                    vec![val, "0".to_string()]
                                } else if ready_func == "qi_future_ready_bool" {
                                    // Boolean: need to convert i1 to i32
                                    let bool_i32 = self.generate_temp();
                                    // Generate zext instruction directly - add trailing colon to prevent auto-colon addition
                                    self.add_instruction(IrInstruction::标签 {
                                        name: format!("{} = zext i1 {} to i32:", bool_i32, val),
                                    });
                                    // Track the type of the converted value
                                    let bool_var = bool_i32.trim_start_matches('%');
                                    self.variable_types
                                        .insert(bool_var.to_string(), "i32".to_string());
                                    vec![bool_i32]
                                } else if ready_func == "qi_future_ready_ptr" {
                                    // Pointer (struct/custom types): determine the actual type of the value
                                    // The value might be a variable or a temporary
                                    let val_var = val.trim_start_matches('%');
                                    let val_type = self
                                        .variable_types
                                        .get(val_var)
                                        .map(|s| s.as_str())
                                        .unwrap_or("ptr");

                                    // If value type is ptr, pass it directly with type annotation
                                    // Otherwise, need to convert to ptr (shouldn't happen for structs)
                                    if val_type == "ptr" {
                                        vec![val]
                                    } else {
                                        // Fallback: assume it's ptr
                                        vec![val]
                                    }
                                } else {
                                    // Integer, float: single argument
                                    vec![val]
                                };

                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(future_temp.clone()),
                                    callee: ready_func.to_string(),
                                    arguments: args,
                                });
                                Some(future_temp)
                            } else {
                                // No return value, create a Future with default value
                                let future_temp = self.generate_temp();
                                let default_args = if ready_func == "qi_future_ready_string" {
                                    // Empty string: null pointer + 0 length
                                    vec!["null".to_string(), "0".to_string()]
                                } else if ready_func == "qi_future_ready_ptr" {
                                    // Null pointer
                                    vec!["null".to_string()]
                                } else if ready_func == "qi_future_ready_f64" {
                                    // Float: 0.0
                                    vec!["0.0".to_string()]
                                } else if ready_func == "qi_future_ready_bool" {
                                    // Boolean: 0 (false)
                                    vec!["0".to_string()]
                                } else {
                                    // Integer: 0
                                    vec!["0".to_string()]
                                };

                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(future_temp.clone()),
                                    callee: ready_func.to_string(),
                                    arguments: default_args,
                                });
                                Some(future_temp)
                            }
                        } else {
                            value
                        }
                    } else {
                        value
                    };

                self.add_instruction(IrInstruction::返回 { value: final_value });
                Ok("ret".to_string())
            }
            AstNode::跳出语句(_) => {
                // Break: jump to the end label of the innermost loop
                if let Some((_, end_label)) = self.loop_stack.last() {
                    self.add_instruction(IrInstruction::跳转 {
                        label: end_label.clone(),
                    });
                    Ok("break".to_string())
                } else {
                    Err("跳出语句必须在循环内使用".to_string())
                }
            }
            AstNode::继续语句(_) => {
                // Continue: jump to the start label of the innermost loop
                if let Some((start_label, _)) = self.loop_stack.last() {
                    self.add_instruction(IrInstruction::跳转 {
                        label: start_label.clone(),
                    });
                    Ok("continue".to_string())
                } else {
                    Err("继续语句必须在循环内使用".to_string())
                }
            }
            AstNode::表达式语句(expr_stmt) => self.build_node(&expr_stmt.expression),
            AstNode::如果语句(if_stmt) => {
                // Build condition - this should already generate a comparison (i1 result)
                let condition = self.build_node(&if_stmt.condition)?;

                // Generate labels
                let then_label = self.generate_label();
                let else_label = self.generate_label();
                let end_label = self.generate_label();

                // The condition should already be an i1 value from the comparison operation
                // Use it directly for conditional jump
                self.add_instruction(IrInstruction::条件跳转 {
                    condition: condition,
                    true_label: then_label.clone(),
                    false_label: else_label.clone(),
                });

                // Then branch
                self.add_instruction(IrInstruction::标签 {
                    name: then_label.clone(),
                });
                let then_has_return = self.contains_return(&if_stmt.then_branch);
                let then_alias_snap = self.snapshot_alias();
                for stmt in &if_stmt.then_branch {
                    self.build_node(stmt)?;
                }
                self.restore_alias(then_alias_snap);
                // Only add jump if there's no return
                if !then_has_return {
                    self.add_instruction(IrInstruction::跳转 {
                        label: end_label.clone(),
                    });
                }

                // Else branch (if exists)
                self.add_instruction(IrInstruction::标签 {
                    name: else_label.clone(),
                });
                let else_has_return = if let Some(else_branch) = &if_stmt.else_branch {
                    let has_ret = self.node_contains_return(else_branch);
                    let else_alias_snap = self.snapshot_alias();
                    self.build_node(else_branch)?;
                    self.restore_alias(else_alias_snap);
                    has_ret
                } else {
                    false
                };

                // Only add jump if there's no return
                if !else_has_return {
                    self.add_instruction(IrInstruction::跳转 {
                        label: end_label.clone(),
                    });
                }

                // Only add end label if at least one branch doesn't return
                if !then_has_return || !else_has_return {
                    self.add_instruction(IrInstruction::标签 {
                        name: end_label.clone(),
                    });
                }

                Ok("if".to_string())
            }
            AstNode::当语句(while_stmt) => {
                // Generate labels
                let start_label = self.generate_label();
                let body_label = self.generate_label();
                let end_label = self.generate_label();

                // Push loop labels onto stack for break/continue
                self.loop_stack
                    .push((start_label.clone(), end_label.clone()));

                // Jump to start label (condition check)
                self.add_instruction(IrInstruction::跳转 {
                    label: start_label.clone(),
                });

                // Start label (condition check)
                self.add_instruction(IrInstruction::标签 {
                    name: start_label.clone(),
                });

                // Build condition - this should already generate a comparison (i1 result)
                let condition = self.build_node(&while_stmt.condition)?;

                // The condition should already be an i1 value from the comparison operation
                // Use it directly for conditional jump
                self.add_instruction(IrInstruction::条件跳转 {
                    condition: condition,
                    true_label: body_label.clone(), // Go to body if condition is true
                    false_label: end_label.clone(), // Exit loop if condition is false
                });

                // Body label
                self.add_instruction(IrInstruction::标签 {
                    name: body_label.clone(),
                });

                // Body
                let while_alias_snap = self.snapshot_alias();
                for stmt in &while_stmt.body {
                    self.build_node(stmt)?;
                }
                self.restore_alias(while_alias_snap);

                // Jump back to start to check condition again
                self.add_instruction(IrInstruction::跳转 {
                    label: start_label.clone(),
                });

                // End label
                self.add_instruction(IrInstruction::标签 {
                    name: end_label.clone(),
                });

                // Check if this might be an infinite loop (condition is literal true)
                // In that case, add an unreachable instruction so the label has valid IR
                let is_infinite_loop = matches!(&*while_stmt.condition, AstNode::字面量表达式(lit)
                    if matches!(&lit.value, crate::parser::ast::LiteralValue::布尔(true)));
                if is_infinite_loop {
                    self.add_instruction(IrInstruction::不可达);
                }

                // Pop loop labels from stack
                self.loop_stack.pop();

                Ok("while".to_string())
            }
            AstNode::循环语句(loop_stmt) => {
                // Generate labels
                let start_label = self.generate_label();
                let end_label = self.generate_label();

                // Start label
                self.add_instruction(IrInstruction::标签 {
                    name: start_label.clone(),
                });

                // Body
                let loop_alias_snap = self.snapshot_alias();
                for stmt in &loop_stmt.body {
                    self.build_node(stmt)?;
                }
                self.restore_alias(loop_alias_snap);

                // Jump back to start (infinite loop)
                self.add_instruction(IrInstruction::跳转 {
                    label: start_label.clone(),
                });

                // End label (unreachable in current implementation)
                self.add_instruction(IrInstruction::标签 {
                    name: end_label.clone(),
                });

                Ok("loop".to_string())
            }
            AstNode::对于语句(for_stmt) => {
                // Handle: for var in [1, 2, 3] { ... }
                // For now, support array literals only

                // First, evaluate the range expression to get the array
                let array_val = self.build_node(&for_stmt.range)?;

                // Check if range is an array literal - if so, we know the size
                let max_iterations = match &*for_stmt.range {
                    AstNode::数组字面量表达式(arr_lit) => {
                        arr_lit.elements.len().to_string()
                    }
                    AstNode::标识符表达式(ident) => {
                        let bare_name = self.mangled_bare_name(&ident.name);
                        if let Some(size) = self
                            .array_sizes
                            .get(&ident.name)
                            .or_else(|| self.array_sizes.get(&bare_name))
                        {
                            size.to_string()
                        } else if self
                            .variable_types
                            .contains_key(&format!("{}_length", bare_name))
                        {
                            format!("%{}_length", bare_name)
                        } else {
                            "10".to_string()
                        }
                    }
                    _ => "10".to_string(), // Default fallback
                };

                // Generate labels
                let start_label = self.generate_label();
                let body_label = self.generate_label();
                let end_label = self.generate_label();

                // Allocate loop counter variable (index into array)
                let loop_counter = if for_stmt.variable.chars().any(|c| !c.is_ascii()) {
                    format!("%idx_{}", self.mangle_function_name(&for_stmt.variable))
                } else {
                    format!("%idx_{}", for_stmt.variable)
                };

                // Allocate loop variable (holds current element value)
                let loop_var = if for_stmt.variable.chars().any(|c| !c.is_ascii()) {
                    format!("%{}", self.mangle_function_name(&for_stmt.variable))
                } else {
                    format!("%{}", for_stmt.variable)
                };

                // Initialize counter to 0
                self.add_instruction(IrInstruction::分配 {
                    dest: loop_counter.clone(),
                    type_name: "i64".to_string(),
                });

                self.add_instruction(IrInstruction::存储 {
                    target: loop_counter.clone(),
                    value: "0".to_string(),
                    value_type: Some("i64".to_string()),
                });

                // Allocate loop variable
                self.add_instruction(IrInstruction::分配 {
                    dest: loop_var.clone(),
                    type_name: "i64".to_string(),
                });

                // Jump to condition check
                self.add_instruction(IrInstruction::跳转 {
                    label: start_label.clone(),
                });

                // Start label (condition check)
                self.add_instruction(IrInstruction::标签 {
                    name: start_label.clone(),
                });

                // Load counter
                let counter_val = self.generate_temp();
                self.add_instruction(IrInstruction::加载 {
                    dest: counter_val.clone(),
                    source: loop_counter.clone(),
                    load_type: None,
                });

                // Check: counter < max_iterations
                let cond = self.generate_temp();
                self.add_instruction(IrInstruction::二元操作 {
                    dest: cond.clone(),
                    left: counter_val,
                    operator: BinaryOperator::小于,
                    right: max_iterations.to_string(),
                    operand_type: "i64".to_string(),
                });

                // Conditional jump
                self.add_instruction(IrInstruction::条件跳转 {
                    condition: cond,
                    true_label: body_label.clone(),
                    false_label: end_label.clone(),
                });

                // Body label
                self.add_instruction(IrInstruction::标签 {
                    name: body_label.clone(),
                });

                // Load current counter value
                let curr_idx = self.generate_temp();
                self.add_instruction(IrInstruction::加载 {
                    dest: curr_idx.clone(),
                    source: loop_counter.clone(),
                    load_type: None,
                });

                // Get element from array: array[counter]
                let element = self.generate_temp();
                self.add_instruction(IrInstruction::数组访问 {
                    dest: element.clone(),
                    array: array_val.clone(),
                    index: curr_idx,
                });

                // Load the value at that address
                let element_val = self.generate_temp();
                self.add_instruction(IrInstruction::加载 {
                    dest: element_val.clone(),
                    source: element,
                    load_type: None,
                });

                // Store element value into loop variable
                self.add_instruction(IrInstruction::存储 {
                    target: loop_var.clone(),
                    value: element_val,
                    value_type: Some("i64".to_string()),
                });

                // Execute body statements
                for stmt in &for_stmt.body {
                    self.build_node(stmt)?;
                }

                // Load counter
                let idx_val = self.generate_temp();
                self.add_instruction(IrInstruction::加载 {
                    dest: idx_val.clone(),
                    source: loop_counter.clone(),
                    load_type: None,
                });

                // Increment counter
                let new_idx = self.generate_temp();
                self.add_instruction(IrInstruction::二元操作 {
                    dest: new_idx.clone(),
                    left: idx_val,
                    operator: BinaryOperator::加,
                    right: "1".to_string(),
                    operand_type: "i64".to_string(),
                });

                // Store new counter value
                self.add_instruction(IrInstruction::存储 {
                    target: loop_counter.clone(),
                    value: new_idx,
                    value_type: Some("i64".to_string()),
                });

                // Jump back to condition
                self.add_instruction(IrInstruction::跳转 {
                    label: start_label.clone(),
                });

                // End label
                self.add_instruction(IrInstruction::标签 {
                    name: end_label.clone(),
                });

                Ok("for".to_string())
            }
            AstNode::字面量表达式(literal) => {
                match &literal.value {
                    crate::parser::ast::LiteralValue::整数(n) => Ok(n.to_string()),
                    crate::parser::ast::LiteralValue::浮点数(f) => {
                        // Ensure float literals always have decimal point
                        let s = f.to_string();
                        if s.contains('.') || s.contains('e') || s.contains('E') {
                            Ok(s)
                        } else {
                            Ok(format!("{}.0", s))
                        }
                    }
                    crate::parser::ast::LiteralValue::布尔(b) => {
                        // Generate a temporary variable and boolean constant instruction
                        let temp_val = self.generate_temp();
                        let bool_value = if *b { 1 } else { 0 };
                        self.add_instruction(IrInstruction::布尔常量 {
                            dest: temp_val.clone(),
                            value: bool_value as i8,
                        });
                        // Track the temporary variable type
                        let temp_var_name = temp_val.trim_start_matches('%');
                        self.variable_types
                            .insert(temp_var_name.to_string(), "i1".to_string());
                        Ok(temp_val)
                    }
                    crate::parser::ast::LiteralValue::字符串(s) => {
                        // Create a global string constant matching clang's format
                        let escaped_str = self.escape_string(s);
                        let byte_len = s.as_bytes().len();
                        let total_len = byte_len + 1; // +1 for null terminator

                        // Generate a unique string name by incrementing temp_counter
                        let str_name = format!("@.str{}", self.temp_counter);
                        self.temp_counter += 1;

                        self.add_instruction(IrInstruction::字符串常量 {
                            name: format!(
                                "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1",
                                str_name, total_len, escaped_str
                            ),
                        });

                        // For string literals, return the constant name directly
                        Ok(str_name)
                    }
                    crate::parser::ast::LiteralValue::字符(c) => Ok(format!("{}", *c as i32)),
                }
            }
            AstNode::二元操作表达式(binary_expr) => {
                let left = self.build_node(&binary_expr.left)?;
                let right = self.build_node(&binary_expr.right)?;

                // Check if this is string concatenation (加 operator with string operands)
                if binary_expr.operator == crate::parser::ast::BinaryOperator::加 {
                    // Check if either operand is a string (starts with @ for string constants or is ptr type)
                    let left_is_string = left.starts_with('@')
                        || (left.starts_with('%')
                            && self
                                .variable_types
                                .get(left.trim_start_matches('%'))
                                .map(|t| t == "ptr")
                                .unwrap_or(false));
                    let right_is_string = right.starts_with('@')
                        || (right.starts_with('%')
                            && self
                                .variable_types
                                .get(right.trim_start_matches('%'))
                                .map(|t| t == "ptr")
                                .unwrap_or(false));

                    if left_is_string || right_is_string {
                        // This is string concatenation - convert non-string operands to strings first
                        let left_str = if left_is_string {
                            left
                        } else {
                            // Convert non-string to string
                            let conv_temp = self.generate_temp();
                            let left_type = if left.starts_with('%') {
                                self.variable_types
                                    .get(left.trim_start_matches('%'))
                                    .map(|s| s.as_str())
                                    .unwrap_or("i64")
                            } else if left.contains('.') {
                                "double"
                            } else {
                                "i64"
                            };
                            let conv_func = if left_type == "double" {
                                "qi_runtime_float_to_string"
                            } else {
                                "qi_runtime_int_to_string"
                            };
                            self.variable_types.insert(
                                conv_temp.trim_start_matches('%').to_string(),
                                "ptr".to_string(),
                            );
                            self.add_instruction(IrInstruction::函数调用 {
                                dest: Some(conv_temp.clone()),
                                callee: conv_func.to_string(),
                                arguments: vec![left],
                            });
                            conv_temp
                        };

                        let right_str = if right_is_string {
                            right
                        } else {
                            // Convert non-string to string
                            let conv_temp = self.generate_temp();
                            let right_type = if right.starts_with('%') {
                                self.variable_types
                                    .get(right.trim_start_matches('%'))
                                    .map(|s| s.as_str())
                                    .unwrap_or("i64")
                            } else if right.contains('.') {
                                "double"
                            } else {
                                "i64"
                            };
                            let conv_func = if right_type == "double" {
                                "qi_runtime_float_to_string"
                            } else {
                                "qi_runtime_int_to_string"
                            };
                            self.variable_types.insert(
                                conv_temp.trim_start_matches('%').to_string(),
                                "ptr".to_string(),
                            );
                            self.add_instruction(IrInstruction::函数调用 {
                                dest: Some(conv_temp.clone()),
                                callee: conv_func.to_string(),
                                arguments: vec![right],
                            });
                            conv_temp
                        };

                        // Now concatenate the two strings
                        let temp = self.generate_temp();
                        self.variable_types
                            .insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(temp.clone()),
                            callee: "qi_runtime_string_concat".to_string(),
                            arguments: vec![left_str, right_str],
                        });
                        return Ok(temp);
                    }
                }

                // Check if this is string comparison (== or != with string operands)
                if binary_expr.operator == crate::parser::ast::BinaryOperator::等于
                    || binary_expr.operator == crate::parser::ast::BinaryOperator::不等于
                {
                    // Check if either operand is a string (starts with @ for string constants or is ptr type)
                    let left_is_string = left.starts_with('@')
                        || (left.starts_with('%')
                            && self
                                .variable_types
                                .get(left.trim_start_matches('%'))
                                .map(|t| t == "ptr")
                                .unwrap_or(false));
                    let right_is_string = right.starts_with('@')
                        || (right.starts_with('%')
                            && self
                                .variable_types
                                .get(right.trim_start_matches('%'))
                                .map(|t| t == "ptr")
                                .unwrap_or(false));

                    if left_is_string && right_is_string {
                        // String comparison - use runtime function (qi_runtime_string_compare returns 0 if equal)
                        let compare_temp = self.generate_temp();
                        self.variable_types.insert(
                            compare_temp.trim_start_matches('%').to_string(),
                            "i32".to_string(),
                        );

                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(compare_temp.clone()),
                            callee: "qi_runtime_string_compare".to_string(),
                            arguments: vec![left, right],
                        });

                        // string_compare returns 0 if equal, non-zero if not equal
                        let result_temp = self.generate_temp();
                        self.variable_types.insert(
                            result_temp.trim_start_matches('%').to_string(),
                            "i1".to_string(),
                        );

                        if binary_expr.operator == crate::parser::ast::BinaryOperator::等于 {
                            // For ==: compare_temp == 0 means strings are equal
                            self.add_instruction(IrInstruction::二元操作 {
                                dest: result_temp.clone(),
                                left: compare_temp,
                                operator: crate::parser::ast::BinaryOperator::等于,
                                right: "0".to_string(),
                                operand_type: "i32".to_string(),
                            });
                        } else {
                            // For !=: compare_temp != 0 means strings are not equal
                            self.add_instruction(IrInstruction::二元操作 {
                                dest: result_temp.clone(),
                                left: compare_temp,
                                operator: crate::parser::ast::BinaryOperator::不等于,
                                right: "0".to_string(),
                                operand_type: "i32".to_string(),
                            });
                        }
                        return Ok(result_temp);
                    }
                }

                // Determine the operand type and result type of the binary operation
                // Check if either operand is a float type (either literal or variable)
                let is_float_op = left.contains('.')
                    || right.contains('.')
                    || self.is_float_operand(&left)
                    || self.is_float_operand(&right);

                // Determine the operand type for the operation
                let operand_type = if is_float_op {
                    "double".to_string()
                } else {
                    // Check if left operand has a specific type (i32, i64, etc.)
                    let left_type = if left.starts_with('%') {
                        self.variable_types
                            .get(left.trim_start_matches('%'))
                            .map(|s| s.as_str())
                            .unwrap_or("i64")
                    } else {
                        "i64"
                    };
                    left_type.to_string()
                };

                // For comparison operators, result is i1 (boolean), otherwise same as operand type
                let result_type = match binary_expr.operator {
                    BinaryOperator::等于
                    | BinaryOperator::不等于
                    | BinaryOperator::大于
                    | BinaryOperator::小于
                    | BinaryOperator::大于等于
                    | BinaryOperator::小于等于 => "i1".to_string(),
                    _ => operand_type.clone(),
                };

                let temp = self.generate_temp();

                // Record the type of this temporary variable
                self.variable_types.insert(
                    temp.trim_start_matches('%').to_string(),
                    result_type.clone(),
                );

                self.add_instruction(IrInstruction::二元操作 {
                    dest: temp.clone(),
                    left,
                    operator: binary_expr.operator,
                    right,
                    operand_type: operand_type,
                });
                Ok(temp)
            }
            AstNode::一元操作表达式(unary_expr) => {
                let operand = self.build_node(&unary_expr.operand)?;

                // Check if operand is float type
                let is_float = operand.contains('.') || self.is_float_operand(&operand);
                let operand_type = if is_float { "double" } else { "i64" };

                match unary_expr.operator {
                    crate::parser::ast::UnaryOperator::负 => {
                        // Negation: 0 - operand
                        let temp = self.generate_temp();
                        self.variable_types.insert(
                            temp.trim_start_matches('%').to_string(),
                            operand_type.to_string(),
                        );

                        let zero = if is_float { "0.0" } else { "0" };
                        self.add_instruction(IrInstruction::二元操作 {
                            dest: temp.clone(),
                            left: zero.to_string(),
                            operator: BinaryOperator::减,
                            right: operand,
                            operand_type: operand_type.to_string(),
                        });
                        Ok(temp)
                    }
                    crate::parser::ast::UnaryOperator::正 => {
                        // Unary plus: just return operand as-is
                        Ok(operand)
                    }
                    crate::parser::ast::UnaryOperator::非 => {
                        // Logical not
                        let temp = self.generate_temp();
                        self.variable_types
                            .insert(temp.trim_start_matches('%').to_string(), "i1".to_string());

                        // Convert operand to boolean if needed and XOR with 1
                        self.add_instruction(IrInstruction::二元操作 {
                            dest: temp.clone(),
                            left: operand,
                            operator: BinaryOperator::不等于,
                            right: "0".to_string(),
                            operand_type: "i64".to_string(),
                        });

                        // XOR with true to get negation
                        let final_temp = self.generate_temp();
                        self.variable_types.insert(
                            final_temp.trim_start_matches('%').to_string(),
                            "i1".to_string(),
                        );
                        self.add_instruction(IrInstruction::异或 {
                            dest: final_temp.clone(),
                            left: temp,
                            right: "1".to_string(),
                        });
                        Ok(final_temp)
                    }
                }
            }
            AstNode::类型转换表达式(cast_expr) => {
                let value = self.build_node(&cast_expr.expression)?;
                // Infer source type from value
                let source_type = if value.contains('.') || self.is_float_operand(&value) {
                    "double".to_string()
                } else if value.trim_start_matches('%').starts_with("str_")
                    || value.starts_with("@str_")
                    || value.starts_with("@.str")
                {
                    "ptr".to_string()
                } else {
                    // Default to i64 for variables - check in variable_types
                    self.variable_types
                        .get(value.trim_start_matches('%'))
                        .cloned()
                        .unwrap_or_else(|| "i64".to_string())
                };
                let target_type = self.get_llvm_type(&Some(cast_expr.target_type.clone()));

                // Generate appropriate LLVM cast instruction based on source and target types
                let temp = self.generate_temp();
                self.variable_types.insert(
                    temp.trim_start_matches('%').to_string(),
                    target_type.clone(),
                );

                match (source_type.as_str(), target_type.as_str()) {
                    // Integer to float
                    ("i64", "double")
                    | ("i32", "double")
                    | ("i16", "double")
                    | ("i8", "double") => {
                        self.add_instruction(IrInstruction::类型转换 {
                            dest: temp.clone(),
                            value,
                            from_type: source_type,
                            to_type: target_type,
                            cast_type: "sitofp".to_string(), // signed int to float
                        });
                    }
                    // Float to integer
                    ("double", "i64")
                    | ("double", "i32")
                    | ("double", "i16")
                    | ("double", "i8") => {
                        self.add_instruction(IrInstruction::类型转换 {
                            dest: temp.clone(),
                            value,
                            from_type: source_type,
                            to_type: target_type,
                            cast_type: "fptosi".to_string(), // float to signed int
                        });
                    }
                    // Integer to integer (different sizes)
                    ("i64", "i32")
                    | ("i64", "i16")
                    | ("i64", "i8")
                    | ("i32", "i16")
                    | ("i32", "i8")
                    | ("i16", "i8") => {
                        self.add_instruction(IrInstruction::类型转换 {
                            dest: temp.clone(),
                            value,
                            from_type: source_type,
                            to_type: target_type,
                            cast_type: "trunc".to_string(), // truncate to smaller int
                        });
                    }
                    ("i8", "i16")
                    | ("i8", "i32")
                    | ("i8", "i64")
                    | ("i16", "i32")
                    | ("i16", "i64")
                    | ("i32", "i64") => {
                        self.add_instruction(IrInstruction::类型转换 {
                            dest: temp.clone(),
                            value,
                            from_type: source_type,
                            to_type: target_type,
                            cast_type: "sext".to_string(), // sign extend to larger int
                        });
                    }
                    // Boolean conversions
                    ("i1", "i64") | ("i1", "i32") | ("i1", "i16") | ("i1", "i8") => {
                        self.add_instruction(IrInstruction::类型转换 {
                            dest: temp.clone(),
                            value,
                            from_type: source_type,
                            to_type: target_type,
                            cast_type: "zext".to_string(), // zero extend
                        });
                    }
                    // Same type - no conversion needed
                    (s, t) if s == t => {
                        return Ok(value); // No conversion needed
                    }
                    _ => {
                        return Err(format!(
                            "不支持的类型转换: {} -> {}",
                            source_type, target_type
                        ));
                    }
                }

                Ok(temp)
            }
            AstNode::赋值表达式(assign_expr) => {
                let value = self.build_node(&assign_expr.value)?;

                // Handle different LValue types
                match assign_expr.target.as_ref() {
                    AstNode::标识符表达式(ident) => {
                        // Check if the variable is a constant (cannot be reassigned)
                        if self.constant_variables.contains(&ident.name) {
                            return Err(format!("常量 '{}' 不能重新赋值", ident.name));
                        }

                        // 先经 variable_alias 把用户标识符解析到唯一内部名
                        // （同名局部跨块唯一化 / catch error 变量），再 mangle。
                        let resolved_name = self
                            .variable_alias
                            .get(&ident.name)
                            .cloned()
                            .unwrap_or_else(|| ident.name.clone());
                        let bare_mangled = if resolved_name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&resolved_name)
                        } else {
                            resolved_name.clone()
                        };

                        let is_global = self.global_variables.contains(&ident.name)
                            || self.global_variables.contains(&bare_mangled);

                        let target_name = if is_global {
                            format!("@{}", bare_mangled)
                        } else {
                            format!("%{}", bare_mangled)
                        };
                        let existing_target_type = self
                            .variable_types
                            .get(&ident.name)
                            .cloned()
                            .or_else(|| self.variable_types.get(&bare_mangled).cloned());
                        let inferred_value_type = self.infer_ir_value_type(&value);
                        let value_type = existing_target_type.clone().or(inferred_value_type);

                        // Update variable_types so subsequent uses of this var have the correct type
                        if let Some(ref vt) = value_type {
                            self.variable_types.insert(ident.name.clone(), vt.clone());
                            self.variable_types.insert(bare_mangled.clone(), vt.clone());
                        }
                        // If the assigned value is a struct pointer, propagate struct type too
                        let value_var = value.trim_start_matches('%');
                        if let Some(st) = self.variable_struct_types.get(value_var).cloned() {
                            self.variable_struct_types.insert(bare_mangled.clone(), st);
                        }

                        self.add_instruction(IrInstruction::存储 {
                            target: target_name.clone(),
                            value,
                            value_type,
                        });
                        Ok(target_name)
                    }
                    AstNode::字段访问表达式(field_access) => {
                        // Field assignment: obj.field = value
                        // First get the field address
                        let object = self.build_node(&field_access.object)?;
                        let field_addr = self.generate_temp();
                        let object_var_name = object.trim_start_matches('%');
                        let struct_type = self
                            .variable_struct_types
                            .get(object_var_name)
                            .cloned()
                            .unwrap_or_else(|| "unknown".to_string());
                        self.add_instruction(IrInstruction::字段访问 {
                            dest: field_addr.clone(),
                            object: object.clone(),
                            field: field_access.field.clone(),
                            struct_type,
                        });
                        let value_type = self.infer_ir_value_type(&value);

                        // Then store the value to that address
                        self.add_instruction(IrInstruction::存储 {
                            target: field_addr.clone(),
                            value: value.clone(),
                            value_type,
                        });
                        let value_var = value.trim_start_matches('%');
                        let is_ptr = self
                            .variable_types
                            .get(value_var)
                            .map(|t| t == "ptr")
                            .unwrap_or(false)
                            || value.starts_with('@');
                        // 跳过 .rodata 字面量 — 永不被 GC 回收，追踪是浪费
                        if is_ptr && !self.is_rodata_literal(&value) {
                            let gc_temp = self.generate_temp();
                            self.add_instruction(IrInstruction::函数调用 {
                                dest: Some(gc_temp),
                                callee: "qi_runtime_gc_add_reference".to_string(),
                                arguments: vec![object.clone(), value.clone()],
                            });
                        }
                        Ok(field_addr)
                    }
                    AstNode::数组访问表达式(array_access) => {
                        // Array element assignment: arr[index] = value
                        let array = self.build_node(&array_access.array)?;
                        let index = self.build_node(&array_access.index)?;

                        // Determine element type from value
                        let element_type = if value.contains('.') {
                            "double"
                        } else {
                            "i64" // Default to i64
                        };

                        // Generate instruction to store to array element
                        self.add_instruction(IrInstruction::数组存储 {
                            array: array.clone(),
                            index,
                            value: value.clone(),
                            element_type: element_type.to_string(),
                        });
                        if self.is_pointer_value(&value) && !self.is_rodata_literal(&value) {
                            let gc_temp = self.generate_temp();
                            self.add_instruction(IrInstruction::函数调用 {
                                dest: Some(gc_temp),
                                callee: "qi_runtime_gc_add_reference".to_string(),
                                arguments: vec![array.clone(), value],
                            });
                        }
                        Ok(array)
                    }
                    _ => Err(format!(
                        "Invalid assignment target: {:?}",
                        assign_expr.target
                    )),
                }
            }
            AstNode::函数调用表达式(call_expr) => {
                // Special handling for 打印 and 打印行 functions - map to appropriate runtime function
                let function_name = self.get_full_function_name(call_expr);
                let runtime_function = if function_name == "打印" || function_name == "打印行"
                {
                    if call_expr.arguments.len() == 1 {
                        // Single argument - determine type
                        let first_arg = &call_expr.arguments[0];
                        let expr_type = match first_arg {
                            AstNode::字面量表达式(literal) => match &literal.value {
                                crate::parser::ast::LiteralValue::字符串(_) => "string",
                                crate::parser::ast::LiteralValue::整数(_) => "integer",
                                crate::parser::ast::LiteralValue::浮点数(_) => "float",
                                crate::parser::ast::LiteralValue::布尔(_) => "boolean",
                                crate::parser::ast::LiteralValue::字符(_) => "integer",
                            },
                            AstNode::标识符表达式(ident) => {
                                // Check if this is a semantically boolean variable first
                                let is_bool_var = self.boolean_variables.contains(&ident.name) || {
                                    let mangled = if ident.name.chars().any(|c| !c.is_ascii()) {
                                        format!(
                                            "_Z_{}",
                                            self.mangle_function_name(&ident.name)
                                                .trim_start_matches("_Z_")
                                        )
                                    } else {
                                        ident.name.clone()
                                    };
                                    self.boolean_variables.contains(&mangled)
                                };

                                if is_bool_var {
                                    "boolean"
                                } else {
                                    // Look up variable type from our tracking
                                    let var_type =
                                        self.variable_types.get(&ident.name).or_else(|| {
                                            let mangled =
                                                if ident.name.chars().any(|c| !c.is_ascii()) {
                                                    format!(
                                                        "_Z_{}",
                                                        self.mangle_function_name(&ident.name)
                                                            .trim_start_matches("_Z_")
                                                    )
                                                } else {
                                                    ident.name.clone()
                                                };
                                            self.variable_types.get(&mangled)
                                        });

                                    match var_type {
                                        Some(vtype) if vtype == "double" => "float",
                                        Some(vtype) if vtype == "ptr" => "string",
                                        Some(vtype) if vtype == "i1" => "boolean",
                                        _ => "integer", // Default to integer
                                    }
                                }
                            }
                            AstNode::字符串连接表达式(_) => {
                                "string" // String concatenation returns string
                            }
                            AstNode::二元操作表达式(bin_expr) => {
                                // Check if this is a string concatenation (+ with strings)
                                if bin_expr.operator == crate::parser::ast::BinaryOperator::加 {
                                    // Check if left operand is a string
                                    let left_is_string = match &*bin_expr.left {
                                        AstNode::字面量表达式(lit) => {
                                            let is_str = matches!(
                                                &lit.value,
                                                crate::parser::ast::LiteralValue::字符串(_)
                                            );
                                            is_str
                                        }
                                        AstNode::字符串连接表达式(_) => true,
                                        AstNode::二元操作表达式(_) => {
                                            // Recursively check (for chained concatenations)
                                            // Heuristic: if it's a binary + and involves strings, it's likely string
                                            true // Conservative: assume chained + with string is string
                                        }
                                        AstNode::标识符表达式(ident) => {
                                            let var_type = self
                                                .variable_types
                                                .get(&ident.name)
                                                .or_else(|| {
                                                    let mangled = if ident
                                                        .name
                                                        .chars()
                                                        .any(|c| !c.is_ascii())
                                                    {
                                                        format!(
                                                            "_Z_{}",
                                                            self.mangle_function_name(&ident.name)
                                                                .trim_start_matches("_Z_")
                                                        )
                                                    } else {
                                                        ident.name.clone()
                                                    };
                                                    self.variable_types.get(&mangled)
                                                });
                                            let is_str =
                                                matches!(var_type, Some(vtype) if vtype == "ptr");
                                            is_str
                                        }
                                        AstNode::函数调用表达式(func_call) => {
                                            // Check if function returns string
                                            let func_name = self.get_full_function_name(func_call);
                                            let mangled_func =
                                                self.mangle_function_name(&func_name);
                                            let is_str = matches!(self.function_return_types.get(&mangled_func), Some(ret) if ret == "ptr");
                                            is_str
                                        }
                                        _ => false,
                                    };
                                    if left_is_string {
                                        "string"
                                    } else {
                                        "integer"
                                    }
                                } else {
                                    "integer" // Non-add binary ops default to integer
                                }
                            }
                            AstNode::函数调用表达式(func_call) => {
                                // Check if this function returns a string
                                let func_name = self.get_full_function_name(func_call);
                                if let Some(runtime_func) = self.map_to_runtime_function(&func_name)
                                {
                                    // Check if it's a runtime function that returns a string
                                    if runtime_func.contains("int_to_string")
                                        || runtime_func.contains("float_to_string")
                                        || (runtime_func.contains("string")
                                            && !runtime_func.contains("to_int")
                                            && !runtime_func.contains("to_float")
                                            && !runtime_func.contains("length"))
                                    {
                                        "string"
                                    } else if runtime_func.contains("sqrt")
                                        || runtime_func.contains("sin")
                                        || runtime_func.contains("cos")
                                        || runtime_func.contains("tan")
                                        || runtime_func.contains("floor")
                                        || runtime_func.contains("ceil")
                                        || runtime_func.contains("round")
                                        || runtime_func.contains("abs_float")
                                    {
                                        "float"
                                    } else {
                                        "integer"
                                    }
                                } else {
                                    // User-defined function - check return type
                                    let mangled_func = self.mangle_function_name(&func_name);
                                    if let Some(ret_type) =
                                        self.function_return_types.get(&mangled_func)
                                    {
                                        match ret_type.as_str() {
                                            "ptr" => "string",
                                            "double" => "float",
                                            "i1" => "boolean",
                                            _ => "integer",
                                        }
                                    } else {
                                        "integer" // Default
                                    }
                                }
                            }
                            AstNode::方法调用表达式(method_call) => {
                                // Module function call - check module registry for return type
                                if let AstNode::标识符表达式(ident) = &*method_call.object {
                                    let module_name = &ident.name;
                                    let actual_module =
                                        self.import_aliases.get(module_name).unwrap_or(module_name);

                                    if let Some(module_function) = self
                                        .module_registry
                                        .get_function(actual_module, &method_call.method_name)
                                    {
                                        let return_type_str = &module_function.return_type;
                                        match return_type_str.as_str() {
                                            "字符串" | "ptr" => "string",
                                            "浮点数" | "double" => "float",
                                            "布尔" | "i1" => "boolean",
                                            _ => "integer",
                                        }
                                    } else {
                                        "integer" // Default
                                    }
                                } else {
                                    "integer" // Default
                                }
                            }
                            AstNode::字段访问表达式(field_access) => {
                                if let AstNode::标识符表达式(ident) = &*field_access.object {
                                    let struct_type = self
                                        .variable_struct_types
                                        .get(&ident.name)
                                        .cloned()
                                        .or_else(|| {
                                            let mangled =
                                                if ident.name.chars().any(|c| !c.is_ascii()) {
                                                    format!(
                                                        "_Z_{}",
                                                        self.mangle_function_name(&ident.name)
                                                            .trim_start_matches("_Z_")
                                                    )
                                                } else {
                                                    ident.name.clone()
                                                };
                                            self.variable_struct_types.get(&mangled).cloned()
                                        });
                                    if let Some(struct_name) = struct_type {
                                        match self
                                            .get_struct_field_llvm_type(
                                                &struct_name,
                                                &field_access.field,
                                            )
                                            .as_deref()
                                        {
                                            Some("ptr") => "string",
                                            Some("double") => "float",
                                            Some("i1") => "boolean",
                                            _ => "integer",
                                        }
                                    } else {
                                        "integer"
                                    }
                                } else {
                                    "integer"
                                }
                            }
                            _ => "integer", // Default to integer
                        };

                        // Map to appropriate runtime function
                        let is_println = function_name == "打印行";
                        Some(
                            match expr_type {
                                "string" => {
                                    if is_println {
                                        "qi_runtime_println"
                                    } else {
                                        "qi_runtime_print"
                                    }
                                }
                                "float" => {
                                    if is_println {
                                        "qi_runtime_println_float"
                                    } else {
                                        "qi_runtime_print_float"
                                    }
                                }
                                "boolean" => {
                                    if is_println {
                                        "qi_runtime_println_bool"
                                    } else {
                                        "qi_runtime_print_bool"
                                    }
                                }
                                _ => {
                                    if is_println {
                                        "qi_runtime_println_int"
                                    } else {
                                        "qi_runtime_print_int"
                                    }
                                }
                            }
                            .to_string(),
                        )
                    } else {
                        None
                    }
                } else {
                    // Check if this is a builtin runtime function
                    self.map_to_runtime_function(&function_name)
                };

                // Evaluate arguments
                // For array arguments, also pass their lengths as hidden parameters
                let mut arg_temps = Vec::new();
                for arg in &call_expr.arguments {
                    let temp = self.build_node(arg)?;
                    arg_temps.push(temp.clone());

                    // Check if this argument is an array - if so, add its length as a hidden parameter
                    let arg_is_array = match arg {
                        AstNode::标识符表达式(ident) => {
                            // Check if this identifier is a known array
                            self.array_sizes.contains_key(&ident.name) || {
                                // Check if it's an array parameter (try both original and mangled names)
                                let length_param_name = format!("{}_length", ident.name);
                                let mangled_name = if ident.name.chars().any(|c| !c.is_ascii()) {
                                    self.mangle_function_name(&ident.name)
                                } else {
                                    ident.name.clone()
                                };
                                let mangled_length_param_name = format!("{}_length", mangled_name);
                                self.variable_types.contains_key(&length_param_name)
                                    || self.variable_types.contains_key(&mangled_length_param_name)
                            }
                        }
                        AstNode::数组字面量表达式(_) => true,
                        _ => false,
                    };

                    if arg_is_array {
                        // Get the array length
                        let length_value = match arg {
                            AstNode::标识符表达式(ident) => {
                                // First check if it's a literal array with known size
                                if let Some(size) = self.array_sizes.get(&ident.name) {
                                    size.to_string()
                                } else {
                                    // It's an array parameter - pass through its length parameter
                                    let mangled_name = if ident.name.chars().any(|c| !c.is_ascii())
                                    {
                                        self.mangle_function_name(&ident.name)
                                    } else {
                                        ident.name.clone()
                                    };
                                    format!("%{}_length", mangled_name)
                                }
                            }
                            AstNode::数组字面量表达式(_) => {
                                // The temp variable should have size info
                                let temp_name = temp.trim_start_matches('%');
                                if let Some(size) = self.array_sizes.get(temp_name) {
                                    size.to_string()
                                } else {
                                    "0".to_string() // Fallback
                                }
                            }
                            _ => "0".to_string(),
                        };
                        arg_temps.push(length_value);
                    }
                }

                // 闭包变量调用 — fat call：先 qi_closure_get_fn 拿真函数指针，再 call fn(obj, args)
                let is_closure_call = {
                    let bare = self.mangled_bare_name(&function_name);
                    self.closure_variables.contains(&function_name)
                        || self.closure_variables.contains(&bare)
                };
                if is_closure_call {
                    let bare = self.mangled_bare_name(&function_name);
                    let sig = self
                        .closure_signatures
                        .get(&bare)
                        .or_else(|| self.closure_signatures.get(&function_name))
                        .cloned();
                    let (param_types, return_type) =
                        sig.unwrap_or_else(|| (vec![], "void".to_string()));

                    // 加载闭包对象指针（如果是参数直接用，否则 load alloca）
                    let is_param = self
                        .variable_types
                        .contains_key(&format!("param_{}", function_name))
                        || self.variable_types.contains_key(&format!("param_{}", bare));
                    let obj_ref = if is_param {
                        format!("%{}", bare)
                    } else {
                        let loaded = self.generate_temp();
                        let source = if self.global_variables.contains(&function_name)
                            || self.global_variables.contains(&bare)
                        {
                            format!("@{}", bare)
                        } else {
                            format!("%{}", bare)
                        };
                        self.add_instruction(IrInstruction::加载 {
                            dest: loaded.clone(),
                            source,
                            load_type: Some("ptr".to_string()),
                        });
                        loaded
                    };

                    // 取真函数指针
                    let fn_tmp = self.generate_temp();
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(fn_tmp.clone()),
                        callee: "qi_closure_get_fn".to_string(),
                        arguments: vec![obj_ref.clone()],
                    });
                    self.variable_types.insert(
                        fn_tmp.trim_start_matches('%').to_string(),
                        "ptr".to_string(),
                    );

                    // 调用：fn_tmp(obj, 用户参数...)
                    let mut full_args = vec![obj_ref.clone()];
                    full_args.extend(arg_temps);
                    // 用 ptr + 用户参数类型注册 fn_tmp 签名
                    let mut full_param_types = vec!["ptr".to_string()];
                    full_param_types.extend(param_types);
                    self.function_param_types
                        .insert(fn_tmp.clone(), full_param_types);

                    if return_type == "void" {
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: None,
                            callee: fn_tmp,
                            arguments: full_args,
                        });
                        return Ok(String::new());
                    }
                    let temp = self.generate_temp();
                    self.variable_types.insert(
                        temp.trim_start_matches('%').to_string(),
                        return_type.clone(),
                    );
                    self.function_return_types
                        .insert(fn_tmp.clone(), return_type);
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(temp.clone()),
                        callee: fn_tmp,
                        arguments: full_args,
                    });
                    return Ok(temp);
                }

                if let Some((param_types, return_type)) =
                    self.lookup_function_pointer_signature(&function_name)
                {
                    let mangled_name = self.mangled_bare_name(&function_name);
                    let is_param = self
                        .variable_types
                        .contains_key(&format!("param_{}", function_name))
                        || self
                            .variable_types
                            .contains_key(&format!("param_{}", mangled_name));

                    // For parameters the function pointer value is already in SSA form
                    // (the parameter register itself). For local/global variables, we
                    // first have to load the pointer value out of the alloca/global.
                    let callee_ref = if is_param {
                        format!("%{}", mangled_name)
                    } else {
                        let loaded = self.generate_temp();
                        let source = if self.global_variables.contains(&function_name)
                            || self.global_variables.contains(&mangled_name)
                        {
                            format!("@{}", mangled_name)
                        } else {
                            format!("%{}", mangled_name)
                        };
                        self.add_instruction(IrInstruction::加载 {
                            dest: loaded.clone(),
                            source,
                            load_type: Some("ptr".to_string()),
                        });
                        self.variable_types.insert(
                            loaded.trim_start_matches('%').to_string(),
                            "ptr".to_string(),
                        );
                        loaded
                    };

                    self.function_param_types
                        .insert(callee_ref.clone(), param_types);

                    if return_type == "void" {
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: None,
                            callee: callee_ref,
                            arguments: arg_temps,
                        });
                        return Ok(String::new());
                    }

                    let temp = self.generate_temp();
                    self.variable_types.insert(
                        temp.trim_start_matches('%').to_string(),
                        return_type.clone(),
                    );
                    self.function_return_types
                        .insert(callee_ref.clone(), return_type);
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(temp.clone()),
                        callee: callee_ref,
                        arguments: arg_temps,
                    });
                    return Ok(temp);
                }

                // Determine the callee name (mutable to allow printf override)
                let mut mapped_callee: String = if let Some(runtime_func) = runtime_function {
                    // Use runtime function name directly
                    runtime_func
                } else {
                    // Apply the same name mangling logic for user functions
                    match function_name.as_str() {
                        "主函数" | "主" | "主程序" => "main".to_string(), // Special case for main function
                        name => {
                            // Apply UTF-8 + Hex name mangling for non-ASCII names
                            if name.chars().any(|c| !c.is_ascii()) {
                                self.mangle_function_name(name)
                            } else {
                                name.to_string() // Keep ASCII names as-is
                            }
                        }
                    }
                };

                if !mapped_callee.starts_with("qi_") && mapped_callee != "printf" {
                    arg_temps = self.lower_call_arguments(&mapped_callee, arg_temps)?;
                }

                // Special handling: 打印 or 打印行 with multiple arguments -> map to printf with proper format
                if (function_name == "打印" || function_name == "打印行") && arg_temps.len() >= 2
                {
                    let is_println = function_name == "打印行";
                    let mut fmt_parts = Vec::new();

                    // Build format string based on argument types
                    for (i, arg) in arg_temps.iter().enumerate() {
                        if i > 0 {
                            fmt_parts.push(" ".to_string());
                        }

                        // Infer type of each argument
                        let arg_ty = if arg.starts_with('%') {
                            let var_name = arg.trim_start_matches('%');
                            self.variable_types
                                .get(var_name)
                                .cloned()
                                .unwrap_or_else(|| "i64".to_string())
                        } else if arg.starts_with('@') {
                            "ptr".to_string()
                        } else if arg.contains('.') {
                            "double".to_string()
                        } else {
                            "i64".to_string()
                        };

                        // Add appropriate format specifier
                        let fmt = if arg_ty == "double" {
                            "%f"
                        } else if arg_ty == "ptr" {
                            "%s"
                        } else {
                            "%lld"
                        };
                        fmt_parts.push(fmt.to_string());
                    }

                    // Join all format parts and add newline if needed
                    let mut fmt_spec = fmt_parts.join("");
                    if is_println {
                        fmt_spec.push_str("\\0A");
                    }

                    // Calculate actual byte length:
                    // Each \XX escape sequence (like \0A) is 3 chars in source but 1 byte in binary
                    let escape_count = fmt_spec.matches("\\0A").count();
                    let actual_len = fmt_spec.len() - (escape_count * 2); // Each escape saves 2 chars (3 chars -> 1 byte)

                    // Create a global format string constant
                    let fmt_name = format!("@.fmt{}", self.temp_counter);
                    self.temp_counter += 1;
                    self.add_instruction(IrInstruction::字符串常量 {
                        name: format!(
                            "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1",
                            fmt_name,
                            actual_len + 1, // +1 for null terminator
                            fmt_spec
                        ),
                    });

                    // Prepend format string to arguments and switch callee to printf
                    let mut new_args = Vec::new();
                    new_args.push(fmt_name);
                    new_args.extend(arg_temps);
                    arg_temps = new_args;
                    mapped_callee = "printf".to_string();
                }

                // Check if this is an async function call
                // Use the async_function_types HashMap to determine if a function is async
                let is_async_function = self.async_function_types.contains_key(&mapped_callee);

                // Check if this is an external function (called but not defined in current module)
                // Exclude runtime functions, crypto functions (already declared), and printf
                if !mapped_callee.starts_with("qi_runtime_")
                    && !mapped_callee.starts_with("qi_crypto_")
                    && mapped_callee != "printf"
                    && !self.defined_functions.contains(&mapped_callee)
                    && !self.external_functions.contains_key(&mapped_callee as &str)
                {
                    // This is an external function - record its signature
                    // Determine parameter types from arguments
                    let param_types: Vec<String> = arg_temps
                        .iter()
                        .map(|arg| {
                            if arg.starts_with('%') {
                                let var_name = arg.trim_start_matches('%');
                                self.variable_types
                                    .get(var_name)
                                    .cloned()
                                    .unwrap_or_else(|| "i64".to_string())
                            } else if arg.parse::<i64>().is_ok() {
                                "i64".to_string()
                            } else if arg.parse::<f64>().is_ok() {
                                "double".to_string()
                            } else {
                                "i64".to_string()
                            }
                        })
                        .collect();

                    // Determine return type
                    // For async functions, always use ptr
                    let ret_type = if self.async_function_types.contains_key(&mapped_callee) {
                        "ptr".to_string()
                    } else if let Some(rt) = self.function_return_types.get(&mapped_callee) {
                        rt.clone()
                    } else {
                        "i64".to_string() // Default to i64
                    };

                    self.external_functions
                        .insert(mapped_callee.clone(), (param_types, ret_type));
                }
                // Generate function call
                let temp = self.generate_temp();

                if is_async_function && !self.in_async_context {
                    // This is an async function call from a sync context - create a task
                    let task_temp = self.generate_temp();

                    // Create async task
                    self.add_instruction(IrInstruction::创建异步任务 {
                        dest: task_temp.clone(),
                        function: mapped_callee.clone(),
                        arguments: arg_temps.clone(),
                    });

                    // The task creation returns a future handle (ptr)
                    self.variable_types.insert(
                        task_temp.trim_start_matches('%').to_string(),
                        "ptr".to_string(),
                    );

                    Ok(task_temp)
                } else if is_async_function && self.in_async_context {
                    // This is an async function call from an async context - call it directly
                    // The async function returns ptr directly
                    self.variable_types
                        .insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(temp.clone()),
                        callee: mapped_callee,
                        arguments: arg_temps,
                    });

                    Ok(temp)
                } else {
                    // Regular function call

                    // Determine the return type to decide if we need a dest
                    let has_return_value = if mapped_callee.starts_with("qi_runtime_") {
                        // Runtime functions always have return values (even if void, they return status)
                        true
                    } else if let Some(ret_type) = self.function_return_types.get(&mapped_callee) {
                        // User-defined function - check its declared return type
                        ret_type != "void"
                    } else if let Some(module_qualifier) = &call_expr.module_qualifier {
                        // Check if this is a module function
                        if let Ok(module_func) = self
                            .check_module_function_available(module_qualifier, &call_expr.callee)
                        {
                            module_func.return_type != "void"
                        } else {
                            // Unknown function - assume it has a return value
                            true
                        }
                    } else {
                        // Unknown function - assume it has a return value
                        true
                    };

                    // For printf, resolve argument types NOW while we have access to variable_types
                    // Store them as "type:value" so they persist when emitting IR later
                    let typed_args = if mapped_callee == "printf" {
                        arg_temps.iter().enumerate().map(|(i, arg)| {
                            if i == 0 {
                                // Format string - always ptr
                                format!("ptr:{}", arg)
                            } else if arg.starts_with('@') {
                                // String constant
                                format!("ptr:{}", arg)
                            } else if arg.starts_with('%') {
                                // Variable - look up its type NOW
                                let var_name = arg.trim_start_matches('%');
                                let vty = self.variable_types.get(var_name)
                                    .or_else(|| self.variable_types.get(&format!("param_{}", var_name)))
                                    .map(|s| s.as_str())
                                    .unwrap_or_else(|| {
                                        eprintln!("[WARN] Printf arg {} type not found during instruction creation, defaulting to i64", var_name);
                                        "i64"
                                    });
                                format!("{}:{}", vty, arg)
                            } else {
                                // Literal value
                                if arg.parse::<i64>().is_ok() {
                                    format!("i64:{}", arg)
                                } else if arg.parse::<f64>().is_ok() {
                                    format!("double:{}", arg)
                                } else {
                                    format!("i64:{}", arg)
                                }
                            }
                        }).collect()
                    } else {
                        arg_temps
                    };

                    if has_return_value {
                        // Record the return type of this function call for later use
                        let return_type = if mapped_callee.starts_with("qi_runtime_") {
                            if mapped_callee.contains("string_length")
                                || mapped_callee.contains("string_to_int")
                            {
                                "i64" // string_length and string_to_int return integer, not string
                            } else if mapped_callee.contains("string")
                                || mapped_callee.contains("concat")
                                || mapped_callee.contains("read_string")
                                || mapped_callee.contains("int_to_string")
                                || mapped_callee.contains("float_to_string")
                            {
                                "ptr"
                            } else if mapped_callee.contains("sqrt")
                                || mapped_callee.contains("abs")
                                || mapped_callee.contains("math")
                                || mapped_callee.contains("float")
                            {
                                "double"
                            } else if mapped_callee.contains("get_time_ms")
                                || mapped_callee.contains("array_length")
                                || mapped_callee.contains("file_open")
                                || mapped_callee.contains("file_read")
                                || mapped_callee.contains("file_write")
                                || mapped_callee.contains("tcp_connect")
                                || mapped_callee.contains("float_to_int")
                                || mapped_callee.contains("create_channel")
                                || mapped_callee.contains("create_task")
                                || mapped_callee.contains("create_timer")
                            {
                                "i64" // Functions that explicitly return i64 or pointers treated as i64
                            } else if mapped_callee == "qi_runtime_set_timeout"
                                || mapped_callee == "qi_runtime_timer_expired"
                                || mapped_callee == "qi_runtime_timer_stop"
                            {
                                "i64" // Timer status functions return i64
                            } else if mapped_callee == "qi_runtime_waitgroup_create"
                                || mapped_callee == "qi_runtime_mutex_create"
                                || mapped_callee == "qi_runtime_rwlock_create"
                                || mapped_callee == "qi_runtime_condvar_create"
                                || mapped_callee == "qi_runtime_once_create"
                                || mapped_callee == "qi_runtime_timer_create"
                            {
                                "ptr" // Synchronization primitive create functions return pointers
                            } else {
                                "i32" // Most runtime functions return i32 status codes
                            }
                        } else if mapped_callee == "qi_runtime_string_concat" {
                            "ptr"
                        } else if mapped_callee.starts_with("qi_crypto_")
                            && mapped_callee != "qi_crypto_free_string"
                        {
                            "ptr" // All crypto functions return string (ptr)
                        } else if let Some(ret_type) =
                            self.function_return_types.get(&mapped_callee)
                        {
                            ret_type
                        } else if let Some((_param_types, ret_type)) =
                            self.external_functions.get(&mapped_callee)
                        {
                            ret_type.as_str() // Return type from external (cross-module) function
                        } else {
                            "i64"
                        };
                        let temp_var_name = temp.trim_start_matches('%').to_string();
                        self.variable_types
                            .insert(temp_var_name.clone(), return_type.to_string());
                        // If this function returns a struct pointer, track the struct type
                        if return_type == "ptr" {
                            if let Some(struct_name) = self
                                .function_return_struct_types
                                .get(&mapped_callee)
                                .cloned()
                                .or_else(|| {
                                    self.external_function_return_struct_types
                                        .get(&mapped_callee)
                                        .cloned()
                                })
                            {
                                self.variable_struct_types
                                    .insert(temp_var_name, struct_name);
                            }
                        }

                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(temp.clone()),
                            callee: mapped_callee,
                            arguments: typed_args.clone(),
                        });

                        Ok(temp)
                    } else {
                        // Void function - no return value
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: None,
                            callee: mapped_callee,
                            arguments: typed_args,
                        });

                        Ok(String::new()) // Return empty string since there's no result
                    }
                }
            }
            AstNode::等待表达式(await_expr) => {
                // Extract the original variable name if it's an identifier
                let original_var_name =
                    if let AstNode::标识符表达式(ident) = await_expr.expression.as_ref() {
                        Some(ident.name.clone())
                    } else {
                        None
                    };

                // Build the awaited expression first
                let future_expr = self.build_node(&await_expr.expression)?;

                // Determine if this is a Future<T> type or an async coroutine
                let future_var = future_expr.trim_start_matches('%');
                let is_future_type = self
                    .variable_types
                    .get(future_var)
                    .map(|t| t == "ptr")
                    .unwrap_or(false);

                // Propagate Future inner type to the temp variable
                let mut inner_type_propagated = false;

                // If we have the original variable name and it's a Future, propagate its inner type
                if let Some(orig_name) = &original_var_name {
                    let inner_type_opt =
                        self.future_inner_types.get(orig_name).cloned().or_else(|| {
                            let mangled = if orig_name.chars().any(|c| !c.is_ascii()) {
                                format!(
                                    "_Z_{}",
                                    self.mangle_function_name(orig_name)
                                        .trim_start_matches("_Z_")
                                )
                            } else {
                                orig_name.clone()
                            };
                            self.future_inner_types.get(&mangled).cloned()
                        });

                    if let Some(inner_type) = inner_type_opt {
                        self.future_inner_types
                            .insert(future_var.to_string(), inner_type.clone());
                        inner_type_propagated = true;
                    }
                }

                // If awaiting a function call, infer the inner type from the function's return type annotation
                if !inner_type_propagated && original_var_name.is_none() {
                    if let AstNode::函数调用表达式(call_expr) = await_expr.expression.as_ref()
                    {
                        let function_name = self.get_full_function_name(call_expr);
                        let mangled = if function_name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&function_name)
                        } else {
                            function_name.clone()
                        };

                        // Look up the function's Future inner type
                        if let Some(inner_type) = self.function_future_inner_types.get(&mangled) {
                            self.future_inner_types
                                .insert(future_var.to_string(), inner_type.clone());
                        }
                    }
                }

                // Try to infer the type from the expression
                // If it's a function call, look up the async function's return type
                let (inferred_type, is_async_call) = match await_expr.expression.as_ref() {
                    AstNode::函数调用表达式(call_expr) => {
                        let function_name = self.get_full_function_name(call_expr);
                        let func_name = if function_name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&function_name)
                        } else {
                            function_name
                        };
                        let type_opt = self.async_function_types.get(&func_name).cloned();
                        let is_async = type_opt.is_some();
                        (type_opt, is_async)
                    }
                    _ => (None, false),
                };

                // Record the type of the awaited value
                let result_type = inferred_type.unwrap_or_else(|| "i64".to_string());

                // In async context, if we're awaiting an async function call,
                // it was already called directly and returned the result
                if self.in_async_context && is_async_call {
                    // The future_expr is already the result, no need for await
                    // Just return it directly
                    self.variable_types
                        .insert(future_expr.trim_start_matches('%').to_string(), result_type);
                    Ok(future_expr)
                } else if is_future_type {
                    // This is a Future<T> type - determine the correct await return type
                    let await_temp = self.generate_temp();
                    self.add_instruction(IrInstruction::等待表达式 {
                        dest: await_temp.clone(),
                        future: future_expr.clone(),
                    });

                    // Look up the Future's inner type using the original variable name
                    // Try original name first, then mangled name, then temp var name
                    let inner_type = if let Some(orig_name) = &original_var_name {
                        self.future_inner_types
                            .get(orig_name)
                            .or_else(|| {
                                let mangled = if orig_name.chars().any(|c| !c.is_ascii()) {
                                    format!(
                                        "_Z_{}",
                                        self.mangle_function_name(orig_name)
                                            .trim_start_matches("_Z_")
                                    )
                                } else {
                                    orig_name.clone()
                                };
                                self.future_inner_types.get(&mangled)
                            })
                            .map(|s| s.as_str())
                    } else {
                        self.future_inner_types.get(future_var).map(|s| s.as_str())
                    }
                    .unwrap_or("i64");

                    // Map inner type to the final result type (after any conversions)
                    let return_type = if inner_type.starts_with("struct.") {
                        "ptr" // Struct types return ptr
                    } else {
                        match inner_type {
                            "i64" => "i64",       // qi_future_await_i64 returns i64
                            "double" => "double", // qi_future_await_f64 returns double
                            "i1" => "i1", // qi_future_await_bool returns i32, but we convert to i1
                            "ptr" => "ptr", // qi_future_await_string/ptr returns ptr
                            _ => "i64",   // Default
                        }
                    };
                    // Track the final result type (after any conversions)
                    let temp_key = await_temp.trim_start_matches('%').to_string();
                    self.variable_types
                        .insert(temp_key, return_type.to_string());
                    Ok(await_temp)
                } else {
                    // This is an async coroutine - qi_runtime_await returns pointer to the result
                    let await_temp = self.generate_temp();
                    self.add_instruction(IrInstruction::等待表达式 {
                        dest: await_temp.clone(),
                        future: future_expr,
                    });

                    // The await returns a pointer to the actual result
                    // We need to load the value from that pointer
                    let result_temp = self.generate_temp();
                    self.add_instruction(IrInstruction::加载 {
                        dest: result_temp.clone(),
                        source: await_temp.clone(),
                        load_type: Some(result_type.clone()),
                    });

                    // Record the type of the awaited value
                    self.variable_types.insert(
                        result_temp.trim_start_matches('%').to_string(),
                        result_type.clone(),
                    );
                    // Also record the await_temp as pointing to this type
                    self.variable_types
                        .insert(await_temp.trim_start_matches('%').to_string(), result_type);

                    Ok(result_temp)
                }
            }
            AstNode::标识符表达式(ident) => {
                let temp = self.generate_temp();

                // 用 variable_alias 解析用户标识符到内部唯一名（catch error variable 用的）
                let resolved_name = self
                    .variable_alias
                    .get(&ident.name)
                    .cloned()
                    .unwrap_or_else(|| ident.name.clone());

                // Also compute the bare mangled name without %
                let bare_mangled = if resolved_name.chars().any(|c| !c.is_ascii()) {
                    self.mangle_function_name(&resolved_name)
                } else {
                    resolved_name.clone()
                };

                // Check if it's a global variable
                let is_global = self.global_variables.contains(&ident.name)
                    || self.global_variables.contains(&bare_mangled);

                // Determine the correct LLVM reference name for the variable
                let llvm_var_ref_name = if is_global {
                    format!("@{}", bare_mangled)
                } else {
                    format!("%{}", bare_mangled) // Local or parameter
                };

                if !self.variable_types.contains_key(&ident.name)
                    && !self.variable_types.contains_key(bare_mangled.as_str())
                    && !is_global
                    && (self.function_return_types.contains_key(&bare_mangled)
                        || self.external_functions.contains_key(&bare_mangled))
                {
                    // 函数名作为值（不是直接 callee）— 自动 box 成 closure 对象，
                    // 让它能赋给 closure 变量、存进 ptr 列表、传到接受 函数(...) 的位置。
                    // callee = trampoline 函数（接受 env 第一参数，忽略 env，转调真函数）。
                    // 直接调用 函数(...) 时，函数调用 codegen 走的是字符串 callee，不会进这里。
                    let trampoline = format!("{}__t", bare_mangled);
                    self.pending_trampolines.insert(bare_mangled.clone());

                    let obj_tmp = self.generate_temp();
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(obj_tmp.clone()),
                        callee: "qi_closure_create".to_string(),
                        arguments: vec![format!("@{}", trampoline), "0".to_string()],
                    });
                    let obj_key = obj_tmp.trim_start_matches('%').to_string();
                    self.variable_types
                        .insert(obj_key.clone(), "ptr".to_string());
                    self.closure_variables.insert(obj_key.clone());
                    // 记签名：从 function_param_types / function_return_types 拼
                    let param_types = self
                        .function_param_types
                        .get(&bare_mangled)
                        .cloned()
                        .unwrap_or_default();
                    let ret_type = self
                        .function_return_types
                        .get(&bare_mangled)
                        .cloned()
                        .unwrap_or_else(|| "i64".to_string());
                    self.closure_signatures
                        .insert(obj_key, (param_types, ret_type));
                    return Ok(obj_tmp);
                }

                // Get the variable type and record it for the loaded value
                // Try multiple keys: original name, var_name without %, mangled name, mangled with _Z_ prefix
                let var_type = self
                    .variable_types
                    .get(&ident.name)
                    .or_else(|| self.variable_types.get(bare_mangled.as_str()))
                    .or_else(|| {
                        let with_prefix = format!("_Z_{}", bare_mangled.trim_start_matches("_Z_"));
                        self.variable_types.get(&with_prefix)
                    })
                    .cloned();

                if self.verbose {
                    eprintln!(
                        "[DEBUG] Identifier: {} (mangled: {}), type: {:?}, is_param: {}",
                        ident.name,
                        bare_mangled,
                        var_type,
                        self.variable_types
                            .contains_key(&format!("param_{}", ident.name))
                            || self
                                .variable_types
                                .contains_key(&format!("param_{}", bare_mangled))
                    );
                }

                if let Some(vtype) = var_type.clone() {
                    self.variable_types
                        .insert(temp.trim_start_matches('%').to_string(), vtype);
                }

                // Also propagate struct type if it exists
                if let Some(struct_type) = self.variable_struct_types.get(&ident.name).cloned() {
                    self.variable_struct_types
                        .insert(temp.trim_start_matches('%').to_string(), struct_type);
                }

                // Check if this is a parameter (direct value, not a pointer)
                // Need to check both original name and mangled name
                let param_key1 = format!("param_{}", ident.name);
                let param_key2 = format!("param_{}", bare_mangled);
                let has_param_key1 = self.variable_types.contains_key(&param_key1);
                let has_param_key2 = self.variable_types.contains_key(&param_key2);
                let is_param = has_param_key1 || has_param_key2;

                if self.verbose {
                    eprintln!("[DEBUG] Identifier {} check: param_key1='{}' ({}), param_key2='{}' ({}), is_param={}",
                             ident.name, param_key1, has_param_key1, param_key2, has_param_key2, is_param);
                }

                if is_param {
                    // This is a parameter - use it directly without load
                    // Return the parameter name itself
                    if self.verbose {
                        eprintln!("[DEBUG] Using parameter directly: {}", llvm_var_ref_name);
                    }
                    Ok(llvm_var_ref_name)
                } else {
                    // Local or Global
                    // Load the value
                    if self.verbose {
                        eprintln!(
                            "[DEBUG] Loading variable from source: {}",
                            llvm_var_ref_name
                        );
                    }
                    self.add_instruction(IrInstruction::加载 {
                        dest: temp.clone(),
                        source: llvm_var_ref_name,
                        load_type: var_type,
                    });
                    Ok(temp)
                }
            }
            AstNode::数组访问表达式(array_access) => {
                // Build array expression
                let array_var = self.build_node(&array_access.array)?;

                // Build index expression
                let index_var = self.build_node(&array_access.index)?;

                // Determine element type from array variable
                // Try to get the array name from the array expression
                let elem_type = if let AstNode::标识符表达式(ident) = &*array_access.array {
                    // Get element type from array variable
                    self.array_element_types
                        .get(&ident.name)
                        .or_else(|| {
                            // Try mangled name
                            let mangled = format!(
                                "_Z_{}",
                                self.mangle_function_name(&ident.name)
                                    .trim_start_matches("_Z_")
                            );
                            self.array_element_types.get(&mangled)
                        })
                        .cloned()
                        .unwrap_or_else(|| "i64".to_string())
                } else {
                    "i64".to_string() // Default for complex expressions
                };

                // Generate getelementptr instruction to get element pointer
                let ptr_temp = self.generate_temp();
                self.add_instruction(IrInstruction::数组访问 {
                    dest: ptr_temp.clone(),
                    array: array_var,
                    index: index_var,
                });

                // Load the value from the pointer with correct type
                let value_temp = self.generate_temp();
                self.add_instruction(IrInstruction::加载 {
                    dest: value_temp.clone(),
                    source: ptr_temp,
                    load_type: Some(elem_type),
                });

                Ok(value_temp)
            }
            AstNode::数组字面量表达式(array_literal) => {
                // Determine element type from first element
                let element_type = if !array_literal.elements.is_empty() {
                    match &array_literal.elements[0] {
                        AstNode::字面量表达式(lit_expr) => {
                            use crate::parser::ast::LiteralValue;
                            match &lit_expr.value {
                                LiteralValue::浮点数(_) => "double".to_string(),
                                LiteralValue::整数(_) => "i64".to_string(),
                                LiteralValue::布尔(_) => "i1".to_string(),
                                LiteralValue::字符串(_) => "ptr".to_string(),
                                LiteralValue::字符(_) => "i8".to_string(),
                            }
                        }
                        AstNode::结构体实例化表达式(_) => "ptr".to_string(),
                        AstNode::标识符表达式(ident) => {
                            let bare_name = self.mangled_bare_name(&ident.name);
                            self.variable_types
                                .get(&ident.name)
                                .or_else(|| self.variable_types.get(&bare_name))
                                .cloned()
                                .unwrap_or_else(|| "i64".to_string())
                        }
                        AstNode::函数调用表达式(call_expr) => {
                            let function_name = self.get_full_function_name(call_expr);
                            let mapped = if let Some(runtime_func) =
                                self.map_to_runtime_function(&function_name)
                            {
                                runtime_func
                            } else {
                                self.mangle_function_name(&function_name)
                            };
                            self.function_return_types
                                .get(&mapped)
                                .or_else(|| {
                                    self.external_functions.get(&mapped).map(|(_, ret)| ret)
                                })
                                .cloned()
                                .unwrap_or_else(|| "i64".to_string())
                        }
                        _ => "i64".to_string(),
                    }
                } else {
                    "i64".to_string()
                };

                let temp = self.generate_temp();

                // Record the array element type and size
                let temp_name = temp.trim_start_matches('%');
                self.array_element_types
                    .insert(temp_name.to_string(), element_type.clone());
                let size = array_literal.elements.len();
                self.array_sizes.insert(temp_name.to_string(), size);

                // Create array allocation
                self.add_instruction(IrInstruction::数组分配 {
                    dest: temp.clone(),
                    size: size.to_string(),
                    element_type: element_type.clone(),
                });

                // Store each element
                for (i, element) in array_literal.elements.iter().enumerate() {
                    let element_var = self.build_node(element)?;
                    self.add_instruction(IrInstruction::数组存储 {
                        array: temp.clone(),
                        index: i.to_string(),
                        value: element_var.clone(),
                        element_type: element_type.clone(),
                    });
                    if self.is_pointer_value(&element_var) && !self.is_rodata_literal(&element_var)
                    {
                        let gc_temp = self.generate_temp();
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(gc_temp),
                            callee: "qi_runtime_gc_add_reference".to_string(),
                            arguments: vec![temp.clone(), element_var],
                        });
                    }
                }

                Ok(temp)
            }
            AstNode::字符串连接表达式(string_concat) => {
                // Build left and right expressions
                let left_var = self.build_node(&string_concat.left)?;
                let right_var = self.build_node(&string_concat.right)?;

                // Check if we need to convert left to string
                let left_str = {
                    let is_string = left_var.starts_with('@')
                        || (left_var.starts_with('%')
                            && self
                                .variable_types
                                .get(left_var.trim_start_matches('%'))
                                .map(|t| t == "ptr")
                                .unwrap_or(false));

                    if is_string {
                        left_var
                    } else {
                        // Convert to string
                        let conv_temp = self.generate_temp();
                        let left_type = if left_var.starts_with('%') {
                            self.variable_types
                                .get(left_var.trim_start_matches('%'))
                                .map(|s| s.as_str())
                                .unwrap_or("i64")
                        } else if left_var.contains('.') {
                            "double"
                        } else {
                            "i64"
                        };
                        let conv_func = if left_type == "double" {
                            "qi_runtime_float_to_string"
                        } else {
                            "qi_runtime_int_to_string"
                        };
                        self.variable_types.insert(
                            conv_temp.trim_start_matches('%').to_string(),
                            "ptr".to_string(),
                        );
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(conv_temp.clone()),
                            callee: conv_func.to_string(),
                            arguments: vec![left_var],
                        });
                        conv_temp
                    }
                };

                // Check if we need to convert right to string
                let right_str = {
                    let is_string = right_var.starts_with('@')
                        || (right_var.starts_with('%')
                            && self
                                .variable_types
                                .get(right_var.trim_start_matches('%'))
                                .map(|t| t == "ptr")
                                .unwrap_or(false));

                    if is_string {
                        right_var
                    } else {
                        // Convert to string
                        let conv_temp = self.generate_temp();
                        let right_type = if right_var.starts_with('%') {
                            self.variable_types
                                .get(right_var.trim_start_matches('%'))
                                .map(|s| s.as_str())
                                .unwrap_or("i64")
                        } else if right_var.contains('.') {
                            "double"
                        } else {
                            "i64"
                        };
                        let conv_func = if right_type == "double" {
                            "qi_runtime_float_to_string"
                        } else {
                            "qi_runtime_int_to_string"
                        };
                        self.variable_types.insert(
                            conv_temp.trim_start_matches('%').to_string(),
                            "ptr".to_string(),
                        );
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(conv_temp.clone()),
                            callee: conv_func.to_string(),
                            arguments: vec![right_var],
                        });
                        conv_temp
                    }
                };

                // Generate string concatenation
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::字符串连接 {
                    dest: temp.clone(),
                    left: left_str,
                    right: right_str,
                });

                // Record that this temporary variable is a string type
                self.variable_types
                    .insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

                Ok(temp)
            }
            AstNode::结构体声明(struct_decl) => {
                // Record struct definition for later type generation
                let field_types: Vec<String> = struct_decl
                    .fields
                    .iter()
                    .map(|field| {
                        // Convert Qi types to LLVM types
                        match &field.type_annotation {
                            crate::parser::ast::TypeNode::基础类型(bt) => match bt {
                                crate::parser::ast::BasicType::整数 => "i64".to_string(),
                                crate::parser::ast::BasicType::浮点数 => "double".to_string(),
                                crate::parser::ast::BasicType::布尔 => "i1".to_string(),
                                crate::parser::ast::BasicType::字符串 => "ptr".to_string(),
                                crate::parser::ast::BasicType::长整数 => "i64".to_string(),
                                crate::parser::ast::BasicType::短整数 => "i16".to_string(),
                                crate::parser::ast::BasicType::字节 => "i8".to_string(),
                                crate::parser::ast::BasicType::字符 => "i8".to_string(),
                                crate::parser::ast::BasicType::空 => "void".to_string(),
                                crate::parser::ast::BasicType::数组 => "ptr".to_string(),
                                crate::parser::ast::BasicType::字典 => "ptr".to_string(),
                                crate::parser::ast::BasicType::列表 => "ptr".to_string(),
                                crate::parser::ast::BasicType::集合 => "ptr".to_string(),
                                crate::parser::ast::BasicType::指针 => "ptr".to_string(),
                                crate::parser::ast::BasicType::引用 => "ptr".to_string(),
                                crate::parser::ast::BasicType::可变引用 => "ptr".to_string(),
                            },
                            crate::parser::ast::TypeNode::结构体类型(_) => "ptr".to_string(),
                            crate::parser::ast::TypeNode::自定义类型(_) => "ptr".to_string(),
                            crate::parser::ast::TypeNode::指针类型(_) => "ptr".to_string(),
                            crate::parser::ast::TypeNode::数组类型(_) => "ptr".to_string(),
                            crate::parser::ast::TypeNode::函数类型(_) => "ptr".to_string(),
                            _ => "i64".to_string(),
                        }
                    })
                    .collect();

                // Also collect field names
                let field_names: Vec<String> = struct_decl
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect();

                self.struct_definitions
                    .insert(struct_decl.name.clone(), field_types);
                self.struct_field_names
                    .insert(struct_decl.name.clone(), field_names);
                for field in &struct_decl.fields {
                    if let crate::parser::ast::TypeNode::函数类型(function_type) =
                        &field.type_annotation
                    {
                        self.struct_field_function_signatures.insert(
                            (struct_decl.name.clone(), field.name.clone()),
                            self.function_type_signature(function_type),
                        );
                    }
                }
                Ok("".to_string())
            }
            AstNode::枚举声明(_enum_decl) => {
                // Enum declarations don't generate code directly
                // They just define the type for later use
                Ok("".to_string())
            }
            AstNode::特性声明(trait_decl) => {
                // Trait declarations don't generate code directly
                // They define method signatures that implementations must provide
                // Store trait info for later validation when implementing
                let method_signatures: Vec<(String, Vec<String>, Option<String>)> = trait_decl
                    .methods
                    .iter()
                    .map(|m| {
                        let param_types: Vec<String> = m
                            .parameters
                            .iter()
                            .map(|p| self.get_llvm_type(&p.type_annotation))
                            .collect();
                        let return_type_str = self.get_llvm_type(&m.return_type);
                        let return_type = if return_type_str == "void" {
                            None
                        } else {
                            Some(return_type_str)
                        };
                        (m.name.clone(), param_types, return_type)
                    })
                    .collect();

                self.trait_definitions
                    .insert(trait_decl.name.clone(), method_signatures);
                Ok("".to_string())
            }
            AstNode::实现块(impl_block) => {
                // Implementation blocks generate methods for a type
                // If implementing a trait, validate all required methods are provided
                for method in &impl_block.methods {
                    // Set receiver type from impl block
                    let mut method_copy = method.clone();
                    method_copy.receiver_type = impl_block.target_type.clone();

                    // Generate the method
                    self.build_node(&AstNode::方法声明(method_copy))?;
                }
                Ok("".to_string())
            }
            AstNode::结构体实例化表达式(struct_literal) => {
                // Create a temporary for the struct instance
                let temp = self.generate_temp();

                // Struct instances must always be heap-allocated because they are passed as
                // pointers and outlive the creating function's stack frame.
                let needs_heap_allocation = true;

                // Allocate memory for the struct
                let struct_type = format!("{}.type", struct_literal.struct_name);
                if needs_heap_allocation {
                    // Heap allocation using runtime allocator so GC can track it
                    let field_count = struct_literal.fields.len();
                    let struct_size = field_count * 8; // TODO: use exact field layout
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(temp.clone()),
                        callee: "qi_runtime_alloc".to_string(),
                        arguments: vec![struct_size.to_string()],
                    });
                    // IMPORTANT: Record both pointer type and struct type for this variable
                    // This is needed for getelementptr to work correctly
                    let temp_var = temp.trim_start_matches('%');
                    self.variable_types
                        .insert(temp_var.to_string(), "ptr".to_string());
                    self.variable_struct_types
                        .insert(temp_var.to_string(), struct_literal.struct_name.clone());
                    self.record_allocation(AllocationInfo {
                        ptr: temp.clone(),
                        size: struct_size,
                        type_name: struct_type.clone(),
                        scope_level: self.scope_level,
                        is_heap: true,
                    });
                    let gc_temp = self.generate_temp();
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(gc_temp),
                        callee: "qi_runtime_gc_add_root".to_string(),
                        arguments: vec![temp.clone()],
                    });
                } else {
                    // Stack allocation using alloca (normal case)
                    self.add_instruction(IrInstruction::分配 {
                        dest: temp.clone(),
                        type_name: struct_type.clone(),
                    });
                }

                // Initialize each field
                for field in &struct_literal.fields {
                    let field_value = self.build_node(&field.value)?;
                    let field_ptr = self.generate_temp();

                    // Generate field access instruction (getelementptr)
                    self.add_instruction(IrInstruction::字段访问 {
                        dest: field_ptr.clone(),
                        object: temp.clone(),
                        field: field.name.clone(),
                        struct_type: struct_literal.struct_name.clone(),
                    });
                    let field_type = self
                        .get_struct_field_llvm_type(&struct_literal.struct_name, &field.name)
                        .unwrap_or_else(|| {
                            self.infer_ir_value_type(&field_value)
                                .unwrap_or_else(|| "i64".to_string())
                        });
                    self.variable_types.insert(
                        field_ptr.trim_start_matches('%').to_string(),
                        field_type.clone(),
                    );

                    // Store the field value
                    self.add_instruction(IrInstruction::存储 {
                        target: field_ptr,
                        value: field_value.clone(),
                        value_type: Some(field_type),
                    });
                    if needs_heap_allocation {
                        let field_val_name = field_value.trim_start_matches('%');
                        let is_ptr = self
                            .variable_types
                            .get(field_val_name)
                            .map(|t| t == "ptr")
                            .unwrap_or(false)
                            || field_value.starts_with('@');
                        if is_ptr && !self.is_rodata_literal(&field_value) {
                            let gc_temp = self.generate_temp();
                            self.add_instruction(IrInstruction::函数调用 {
                                dest: Some(gc_temp),
                                callee: "qi_runtime_gc_add_reference".to_string(),
                                arguments: vec![temp.clone(), field_value.clone()],
                            });
                        }
                    }
                }

                // Record that this is a pointer type
                self.variable_types
                    .insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());
                // Record the struct type for field access
                self.variable_struct_types.insert(
                    temp.trim_start_matches('%').to_string(),
                    struct_literal.struct_name.clone(),
                );

                Ok(temp)
            }
            AstNode::字段访问表达式(field_access) => {
                // Check if this is a module access (module.function) or struct field access (obj.field)
                match &*field_access.object {
                    AstNode::标识符表达式(ident) => {
                        // This could be a module access: module.function
                        let module_name = &ident.name;

                        // Check if this is a known import alias or module
                        // Need to check variable_types with both original and mangled names
                        let mangled_module_name = if module_name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(module_name)
                        } else {
                            module_name.to_string()
                        };
                        let is_known_variable = self.variable_types.contains_key(module_name)
                            || self.variable_types.contains_key(&mangled_module_name);

                        if let Some(actual_module) =
                            self.import_aliases.get(module_name).or_else(|| {
                                // If no alias, check if it's a direct module name
                                // Always treat identifier.field as module access if we have an alias
                                // Otherwise, only treat as module access if it's not a known variable
                                if self.import_aliases.contains_key(module_name)
                                    || !is_known_variable
                                {
                                    Some(module_name)
                                } else {
                                    None
                                }
                            })
                        {
                            // This is module access: generate a function call with module qualifier
                            let qualified_function_name =
                                format!("{}_{}", actual_module, field_access.field);

                            // Use the existing function call mechanism by creating a function call expression
                            let func_call = AstNode::函数调用表达式(
                                crate::parser::ast::FunctionCallExpression {
                                    module_qualifier: Some(actual_module.clone()),
                                    callee: field_access.field.clone(),
                                    arguments: vec![],
                                    span: Default::default(), // Use default span
                                },
                            );

                            // Build the function call
                            self.build_node(&func_call)
                        } else {
                            // Check if this is array.长度 access
                            let object_var_name = module_name;
                            if field_access.field == "长度" {
                                // First check if this is a known array literal with size
                                if self.array_sizes.contains_key(object_var_name) {
                                    // This is array.长度 - return the size as a constant
                                    let size = self.array_sizes.get(object_var_name).unwrap();
                                    return Ok(size.to_string());
                                }

                                // Check if this is an array parameter (has a hidden length parameter)
                                // Need to check with both original and mangled names
                                let mangled_var_name =
                                    if object_var_name.chars().any(|c| !c.is_ascii()) {
                                        self.mangle_function_name(object_var_name)
                                    } else {
                                        object_var_name.to_string()
                                    };

                                let length_param_name = format!("{}_length", mangled_var_name);
                                if self.variable_types.contains_key(&length_param_name) {
                                    // This is an array parameter - return the length parameter directly
                                    // The parameter name is already mangled in the function signature
                                    return Ok(format!("%{}", length_param_name));
                                }

                                // Array length not found
                                return Err(format!(
                                    "Cannot access .长度 on '{}': array size not tracked",
                                    object_var_name
                                ));
                            }

                            {
                                // This is struct field access
                                let object_var = self.build_node(&field_access.object)?;

                                // Get the struct type from variable_struct_types
                                let object_var_name = object_var.trim_start_matches('%');
                                let struct_type = self
                                    .variable_struct_types
                                    .get(object_var_name)
                                    .cloned()
                                    .unwrap_or_else(|| "unknown".to_string());

                                // Look up the field type from struct definitions
                                let field_type =
                                    self.get_struct_field_type(&struct_type, &field_access.field);

                                // Generate field access instruction
                                let temp = self.generate_temp();
                                self.add_instruction(IrInstruction::字段访问 {
                                    dest: temp.clone(),
                                    object: object_var,
                                    field: field_access.field.clone(),
                                    struct_type: struct_type.clone(),
                                });

                                // Load the field value
                                let load_temp = self.generate_temp();
                                self.add_instruction(IrInstruction::加载 {
                                    dest: load_temp.clone(),
                                    source: temp,
                                    load_type: field_type.clone(),
                                });

                                // If field is a struct pointer, propagate struct type info
                                if let Some(ref ft) = field_type {
                                    let load_name = load_temp.trim_start_matches('%').to_string();
                                    self.variable_types.insert(load_name.clone(), ft.clone());
                                    // Check if field type is a known struct (ptr but not "ptr" from string)
                                    if ft == "ptr" {
                                        if let Some(field_struct) = self
                                            .get_struct_field_struct_type(
                                                &struct_type,
                                                &field_access.field,
                                            )
                                        {
                                            self.variable_struct_types
                                                .insert(load_name, field_struct);
                                        }
                                    }
                                }

                                Ok(load_temp)
                            }
                        }
                    }
                    _ => {
                        // This is definitely struct field access (complex expression)
                        let object_var = self.build_node(&field_access.object)?;

                        // Get the struct type from variable_struct_types
                        let object_var_name = object_var.trim_start_matches('%');
                        let struct_type = self
                            .variable_struct_types
                            .get(object_var_name)
                            .cloned()
                            .unwrap_or_else(|| "unknown".to_string());

                        // Look up the field type from struct definitions
                        let field_type =
                            self.get_struct_field_type(&struct_type, &field_access.field);

                        // Generate field access instruction
                        let temp = self.generate_temp();
                        self.add_instruction(IrInstruction::字段访问 {
                            dest: temp.clone(),
                            object: object_var,
                            field: field_access.field.clone(),
                            struct_type: struct_type.clone(),
                        });

                        // Load the field value
                        let load_temp = self.generate_temp();
                        self.add_instruction(IrInstruction::加载 {
                            dest: load_temp.clone(),
                            source: temp,
                            load_type: field_type.clone(),
                        });

                        // Propagate field type info
                        if let Some(ref ft) = field_type {
                            let load_name = load_temp.trim_start_matches('%').to_string();
                            self.variable_types.insert(load_name.clone(), ft.clone());
                            if ft == "ptr" {
                                if let Some(field_struct) = self
                                    .get_struct_field_struct_type(&struct_type, &field_access.field)
                                {
                                    self.variable_struct_types
                                        .insert(load_name.clone(), field_struct);
                                }
                            }
                            // 函数指针字段 — 标 closure_variable，让访问后的临时变量调用走 fat call
                            let key = (struct_type.clone(), field_access.field.clone());
                            if let Some(sig) =
                                self.struct_field_function_signatures.get(&key).cloned()
                            {
                                self.closure_variables.insert(load_name.clone());
                                self.closure_signatures.insert(load_name, sig);
                            }
                        }

                        Ok(load_temp)
                    }
                }
            }
            AstNode::块语句(block_stmt) => {
                // Process all statements in the block.
                // 块作用域内同名局部变量的唯一化别名不应泄漏到块外。
                let alias_snap = self.snapshot_alias();
                for stmt in &block_stmt.statements {
                    self.build_node(stmt)?;
                }
                self.restore_alias(alias_snap);
                Ok("block".to_string())
            }
            AstNode::方法声明(method_decl) => {
                // Method is just a function with the receiver as the first parameter
                // Generate method name: TypeName_methodName
                let method_full_name =
                    format!("{}_{}", method_decl.receiver_type, method_decl.method_name);
                let func_name = if method_full_name.chars().any(|c| !c.is_ascii()) {
                    self.mangle_function_name(&method_full_name)
                } else {
                    method_full_name.clone()
                };

                // Record the function for later reference
                self.defined_functions.insert(func_name.clone());

                // Build parameter list - receiver is first parameter
                let receiver_type = "ptr"; // Receiver is always a pointer to the struct
                let mut param_decls = vec![];
                let mut param_names = vec![method_decl.receiver_name.clone()];

                // Add receiver parameter
                let receiver_var = if method_decl.receiver_name.chars().any(|c| !c.is_ascii()) {
                    format!("%{}", self.mangle_function_name(&method_decl.receiver_name))
                } else {
                    format!("%{}", method_decl.receiver_name)
                };
                param_decls.push(format!("{} {}", receiver_type, receiver_var));

                // Add other parameters
                for param in &method_decl.parameters {
                    let param_type = self.get_llvm_type(&param.type_annotation);
                    let param_var = if param.name.chars().any(|c| !c.is_ascii()) {
                        format!("%{}", self.mangle_function_name(&param.name))
                    } else {
                        format!("%{}", param.name)
                    };
                    param_decls.push(format!("{} {}", param_type, param_var));
                    param_names.push(param.name.clone());
                }

                // Get return type
                let return_type = if let Some(_) = method_decl.return_type {
                    self.get_return_type(&method_decl.return_type)
                } else {
                    // Infer from body if there's an explicit return with a value
                    self.infer_return_type_from_body(&method_decl.body)
                        .unwrap_or_else(|| "void".to_string())
                };
                self.function_return_types
                    .insert(func_name.clone(), return_type.clone());

                // Generate function definition start
                let params_str = param_decls.join(", ");
                self.add_instruction(IrInstruction::标签 {
                    name: format!("define {} @{}({}) {{", return_type, func_name, params_str),
                });

                // Add entry label
                self.add_instruction(IrInstruction::标签 {
                    name: "entry:".to_string(),
                });

                // Track parameter types for use in function body
                // Receiver parameter
                let receiver_mangled = if method_decl.receiver_name.chars().any(|c| !c.is_ascii()) {
                    format!(
                        "_Z_{}",
                        self.mangle_function_name(&method_decl.receiver_name)
                            .trim_start_matches("_Z_")
                    )
                } else {
                    method_decl.receiver_name.clone()
                };
                self.variable_types
                    .insert(method_decl.receiver_name.clone(), receiver_type.to_string());
                self.variable_types
                    .insert(receiver_mangled.clone(), receiver_type.to_string());
                self.variable_types.insert(
                    format!("param_{}", method_decl.receiver_name),
                    receiver_type.to_string(),
                );
                self.variable_types.insert(
                    format!("param_{}", receiver_mangled),
                    receiver_type.to_string(),
                );

                // Track receiver struct type for field access
                self.variable_struct_types.insert(
                    method_decl.receiver_name.clone(),
                    method_decl.receiver_type.clone(),
                );
                self.variable_struct_types
                    .insert(receiver_mangled.clone(), method_decl.receiver_type.clone());

                // Other parameters
                for param in &method_decl.parameters {
                    let param_type = self.get_llvm_type(&param.type_annotation);
                    let mangled_param_name = if param.name.chars().any(|c| !c.is_ascii()) {
                        format!(
                            "_Z_{}",
                            self.mangle_function_name(&param.name)
                                .trim_start_matches("_Z_")
                        )
                    } else {
                        param.name.clone()
                    };
                    self.variable_types
                        .insert(param.name.clone(), param_type.clone());
                    self.variable_types
                        .insert(mangled_param_name.clone(), param_type.clone());

                    // Mark as parameter (direct value, not pointer)
                    self.variable_types
                        .insert(format!("param_{}", param.name), param_type.clone());
                    self.variable_types
                        .insert(format!("param_{}", mangled_param_name), param_type.clone());
                }

                // Set current function name so local variable declarations inside
                // the method body are treated as local (not global) variables
                self.current_function_name = Some(func_name.clone());
                self.used_local_names.clear();
                self.current_function_ast_return_type = method_decl.return_type.clone();

                // Process method body
                for (_i, stmt) in method_decl.body.iter().enumerate() {
                    self.build_node(stmt)?;
                }

                // Add default return if no explicit return
                if return_type == "void" {
                    self.add_instruction(IrInstruction::返回 { value: None });
                } else if !method_decl
                    .body
                    .iter()
                    .any(|stmt| matches!(stmt, AstNode::返回语句(_)))
                {
                    // Add default return for non-void functions if missing
                    let default_value = match return_type.as_str() {
                        "i64" => "0",
                        "double" => "0.0",
                        "i1" => "false",
                        "ptr" => "null",
                        _ => "0",
                    };
                    self.add_instruction(IrInstruction::返回 {
                        value: Some(default_value.to_string()),
                    });
                }

                // Close function
                self.add_instruction(IrInstruction::标签 {
                    name: "}".to_string(),
                });

                // Clear current function context
                self.current_function_name = None;
                self.current_function_ast_return_type = None;

                Ok(format!("method_{}", func_name))
            }
            AstNode::方法调用表达式(method_call) => {
                // 优先检查：obj.字段(args) 当字段是函数指针字段时 → 字段读取 + fat call
                // 用 标识符识别 obj，看 variable_struct_types 是否有它的 struct，字段是不是 function 字段
                if !method_call.method_name.starts_with("::") {
                    if let AstNode::标识符表达式(obj_ident) = &*method_call.object {
                        let mangled_obj = self.mangled_bare_name(&obj_ident.name);
                        let struct_type = self
                            .variable_struct_types
                            .get(&obj_ident.name)
                            .cloned()
                            .or_else(|| self.variable_struct_types.get(&mangled_obj).cloned());
                        if let Some(stype) = struct_type {
                            let key = (stype.clone(), method_call.method_name.clone());
                            if let Some(sig) =
                                self.struct_field_function_signatures.get(&key).cloned()
                            {
                                // 这是函数指针字段调用 — 取字段，fat call
                                let field_obj = self.build_node(&AstNode::字段访问表达式(
                                    crate::parser::ast::FieldAccessExpression {
                                        object: method_call.object.clone(),
                                        field: method_call.method_name.clone(),
                                        span: Default::default(),
                                    },
                                ))?;
                                // field_obj 是 closure obj ptr — 取 fn ptr，fat call
                                let fn_tmp = self.generate_temp();
                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(fn_tmp.clone()),
                                    callee: "qi_closure_get_fn".to_string(),
                                    arguments: vec![field_obj.clone()],
                                });
                                self.variable_types.insert(
                                    fn_tmp.trim_start_matches('%').to_string(),
                                    "ptr".to_string(),
                                );

                                let mut full_args = vec![field_obj];
                                for a in &method_call.arguments {
                                    full_args.push(self.build_node(a)?);
                                }
                                let mut full_param_types = vec!["ptr".to_string()];
                                full_param_types.extend(sig.0.iter().cloned());
                                self.function_param_types
                                    .insert(fn_tmp.clone(), full_param_types);

                                if sig.1 == "void" {
                                    self.add_instruction(IrInstruction::函数调用 {
                                        dest: None,
                                        callee: fn_tmp,
                                        arguments: full_args,
                                    });
                                    return Ok(String::new());
                                }
                                let ret_tmp = self.generate_temp();
                                self.variable_types.insert(
                                    ret_tmp.trim_start_matches('%').to_string(),
                                    sig.1.clone(),
                                );
                                self.function_return_types
                                    .insert(fn_tmp.clone(), sig.1.clone());
                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(ret_tmp.clone()),
                                    callee: fn_tmp,
                                    arguments: full_args,
                                });
                                return Ok(ret_tmp);
                            }
                        }
                    }
                }

                // Check if this is a static method call (method_name starts with ::)
                // But first check if it's a module call - modules take precedence
                if method_call.method_name.starts_with("::") {
                    if let AstNode::标识符表达式(type_ident) = &*method_call.object {
                        let type_name = &type_ident.name;
                        let actual_method = &method_call.method_name[2..]; // Remove :: prefix

                        // Check if this is a module call first
                        let is_imported_module = self.import_aliases.contains_key(type_name);
                        let is_stdlib_module = self.module_registry.has_module(type_name);
                        let actual_module_path = if is_imported_module {
                            self.import_aliases.get(type_name).map(|s| s.as_str())
                        } else {
                            None
                        };
                        let is_module_function = if let Some(path) = actual_module_path {
                            self.module_registry
                                .get_function(path, actual_method)
                                .is_some()
                        } else if is_stdlib_module {
                            self.module_registry
                                .get_function(type_name, actual_method)
                                .is_some()
                        } else {
                            false
                        };

                        if is_module_function {
                            // This is a module function call with :: syntax
                            let module_path = actual_module_path.unwrap_or(type_name);
                            let (runtime_func_name, return_type_str) = {
                                let module_function = self
                                    .module_registry
                                    .get_function(module_path, actual_method)
                                    .ok_or_else(|| {
                                        format!(
                                            "模块 '{}' 中不存在函数 '{}'",
                                            module_path, actual_method
                                        )
                                    })?;
                                (
                                    module_function.runtime_name.clone(),
                                    module_function.return_type.clone(),
                                )
                            };

                            // Build arguments
                            let mut args = vec![];
                            for arg in &method_call.arguments {
                                args.push(self.build_node(arg)?);
                            }

                            // Handle void return type specially
                            if return_type_str == "void" || return_type_str == "无" {
                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: None,
                                    callee: runtime_func_name,
                                    arguments: args,
                                });
                                return Ok("".to_string());
                            }

                            // Convert Chinese type to LLVM type
                            let return_type = if return_type_str.starts_with("未来<") {
                                "ptr".to_string()
                            } else if return_type_str == "整数" || return_type_str == "i64" {
                                "i64".to_string()
                            } else if return_type_str == "浮点数"
                                || return_type_str == "double"
                                || return_type_str == "f64"
                            {
                                "double".to_string()
                            } else if return_type_str == "字符串"
                                || return_type_str == "ptr"
                                || return_type_str.contains("字符串")
                            {
                                "ptr".to_string()
                            } else if return_type_str == "布尔"
                                || return_type_str == "i1"
                                || return_type_str == "bool"
                            {
                                "i1".to_string()
                            } else if return_type_str == "i64"
                                || return_type_str == "i32"
                                || return_type_str == "i8"
                            {
                                return_type_str.clone()
                            } else {
                                "ptr".to_string()
                            };

                            // Generate temp and record type
                            let temp = self.generate_temp();
                            let temp_name = temp.trim_start_matches('%').to_string();
                            self.variable_types.insert(temp_name, return_type);

                            self.add_instruction(IrInstruction::函数调用 {
                                dest: Some(temp.clone()),
                                callee: runtime_func_name,
                                arguments: args,
                            });
                            return Ok(temp);
                        }

                        // Not a module function, try static method call
                        // Map known static methods to runtime functions
                        let runtime_func_name = match (type_name.as_str(), actual_method) {
                            ("未来", "就绪") => "qi_future_ready_i64",
                            ("未来", "失败") => "qi_future_failed",
                            _ => {
                                return Err(format!(
                                    "Unknown static method: {}::{}",
                                    type_name, actual_method
                                )
                                .into());
                            }
                        };

                        // Build arguments
                        let mut args = vec![];
                        for arg in &method_call.arguments {
                            args.push(self.build_node(arg)?);
                        }

                        // Generate the call
                        let temp = self.generate_temp();

                        // Record the type (Future methods return ptr)
                        self.variable_types
                            .insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(temp.clone()),
                            callee: runtime_func_name.to_string(),
                            arguments: args,
                        });

                        return Ok(temp);
                    } else {
                        return Err("Static method call must have a type name as the object"
                            .to_string()
                            .into());
                    }
                }

                if let AstNode::标识符表达式(ident) = &*method_call.object {
                    let mangled_name = self.mangled_bare_name(&ident.name);
                    let struct_type = self
                        .variable_struct_types
                        .get(&ident.name)
                        .or_else(|| self.variable_struct_types.get(&mangled_name))
                        .cloned();

                    if let Some(struct_name) = struct_type {
                        if let Some((param_types, return_type)) = self
                            .struct_field_function_signatures
                            .get(&(struct_name.clone(), method_call.method_name.clone()))
                            .cloned()
                        {
                            let object_ptr = self.build_node(&method_call.object)?;
                            let field_ptr = self.generate_temp();
                            self.add_instruction(IrInstruction::字段访问 {
                                dest: field_ptr.clone(),
                                object: object_ptr,
                                field: method_call.method_name.clone(),
                                struct_type: struct_name,
                            });
                            self.variable_types.insert(
                                field_ptr.trim_start_matches('%').to_string(),
                                "ptr".to_string(),
                            );

                            let function_ptr = self.generate_temp();
                            self.add_instruction(IrInstruction::加载 {
                                dest: function_ptr.clone(),
                                source: field_ptr,
                                load_type: Some("ptr".to_string()),
                            });

                            self.function_param_types
                                .insert(function_ptr.clone(), param_types);
                            self.function_return_types
                                .insert(function_ptr.clone(), return_type.clone());

                            let mut args = Vec::new();
                            for arg in &method_call.arguments {
                                args.push(self.build_node(arg)?);
                            }

                            if return_type == "void" {
                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: None,
                                    callee: function_ptr,
                                    arguments: args,
                                });
                                return Ok(String::new());
                            }

                            let temp = self.generate_temp();
                            self.variable_types
                                .insert(temp.trim_start_matches('%').to_string(), return_type);
                            self.add_instruction(IrInstruction::函数调用 {
                                dest: Some(temp.clone()),
                                callee: function_ptr,
                                arguments: args,
                            });
                            return Ok(temp);
                        }
                    }
                }

                // 检查是否为模块前缀调用（object 是标识符且不是变量）
                if let AstNode::标识符表达式(ident) = &*method_call.object {
                    // 检查是否为已知变量（排除模块名）
                    let is_module = self.import_aliases.contains_key(&ident.name)
                        || self.import_aliases.values().any(|v| v == &ident.name);
                    let is_variable = self.variable_types.contains_key(&ident.name)
                        || self
                            .variable_types
                            .contains_key(&self.mangle_function_name(&ident.name));

                    if is_module || !is_variable {
                        // 这是模块前缀调用，如 加密.MD5哈希()
                        let module_name = &ident.name;

                        // 检查是否为导入的模块
                        if self.import_aliases.contains_key(module_name) {
                            let module_path = self.import_aliases.get(module_name).unwrap();

                            // 检查是否为标准库模块（在ModuleRegistry中）
                            let is_stdlib = self.module_registry.has_module(module_path);

                            if is_stdlib {
                                // 打印/打印行 走与裸调用一致的 printf 多参格式逻辑（会按各实参
                                // 类型拼 %lld/%f/%s 等）。否则模块限定写法 IO.打印行("x=", 整数)
                                // 不构建格式串，会丢掉非字符串实参。
                                if method_call.method_name == "打印行"
                                    || method_call.method_name == "打印"
                                {
                                    let synthetic = crate::parser::ast::FunctionCallExpression {
                                        module_qualifier: None,
                                        callee: method_call.method_name.clone(),
                                        arguments: method_call.arguments.clone(),
                                        span: method_call.span.clone(),
                                    };
                                    return self.build_node(
                                        &crate::parser::ast::AstNode::函数调用表达式(synthetic),
                                    );
                                }
                                // 标准库模块：验证函数是否存在并获取运行时函数名和返回类型
                                let (runtime_func_name, return_type_str) = {
                                    let module_function = self.check_module_function_available(
                                        module_name,
                                        &method_call.method_name,
                                    )?;
                                    (
                                        module_function.runtime_name.clone(),
                                        module_function.return_type.clone(),
                                    )
                                };

                                // Convert Chinese type to LLVM type
                                let return_type = if return_type_str.starts_with("未来<") {
                                    "ptr".to_string()
                                } else if return_type_str == "整数" || return_type_str == "i64" {
                                    "i64".to_string()
                                } else if return_type_str == "浮点数"
                                    || return_type_str == "double"
                                    || return_type_str == "f64"
                                {
                                    "double".to_string()
                                } else if return_type_str == "字符串"
                                    || return_type_str == "ptr"
                                    || return_type_str.contains("字符串")
                                {
                                    "ptr".to_string()
                                } else if return_type_str == "布尔"
                                    || return_type_str == "i1"
                                    || return_type_str == "bool"
                                {
                                    "i1".to_string()
                                } else if return_type_str == "i64"
                                    || return_type_str == "i32"
                                    || return_type_str == "i8"
                                {
                                    return_type_str.clone()
                                } else {
                                    "ptr".to_string() // Default to ptr for unknown types
                                };

                                // 构建参数
                                let mut args = vec![];
                                for arg in &method_call.arguments {
                                    args.push(self.build_node(arg)?);
                                }

                                // 生成临时变量并记录其类型
                                let temp = self.generate_temp();
                                // Store type without the % prefix for lookups
                                let temp_name = temp.trim_start_matches('%').to_string();
                                self.variable_types.insert(temp_name, return_type);

                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(temp.clone()),
                                    callee: runtime_func_name,
                                    arguments: args,
                                });
                                return Ok(temp);
                            } else {
                                // 用户包：直接使用mangled函数名，跳过验证
                                let func_name =
                                    if method_call.method_name.chars().any(|c| !c.is_ascii()) {
                                        self.mangle_function_name(&method_call.method_name)
                                    } else {
                                        method_call.method_name.clone()
                                    };

                                // 构建参数
                                let mut args = vec![];
                                for arg in &method_call.arguments {
                                    args.push(self.build_node(arg)?);
                                }

                                // 生成临时变量
                                let temp = self.generate_temp();
                                // 默认返回类型为i64（链接阶段会验证）
                                self.variable_types.insert(temp.clone(), "i64".to_string());

                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(temp.clone()),
                                    callee: func_name,
                                    arguments: args,
                                });
                                return Ok(temp);
                            }
                        } else {
                            // 检查是否为直接使用的标准库模块（无需导入）
                            let is_stdlib = self.module_registry.has_module(module_name);

                            if is_stdlib {
                                // 打印/打印行 走裸调用一致的 printf 多参格式逻辑（同上）。
                                if method_call.method_name == "打印行"
                                    || method_call.method_name == "打印"
                                {
                                    let synthetic = crate::parser::ast::FunctionCallExpression {
                                        module_qualifier: None,
                                        callee: method_call.method_name.clone(),
                                        arguments: method_call.arguments.clone(),
                                        span: method_call.span.clone(),
                                    };
                                    return self.build_node(
                                        &crate::parser::ast::AstNode::函数调用表达式(synthetic),
                                    );
                                }
                                // 标准库模块：直接从模块注册表查找函数
                                let module_function = self
                                    .module_registry
                                    .get_function(module_name, &method_call.method_name)
                                    .ok_or_else(|| {
                                        format!(
                                            "模块 '{}' 中不存在函数 '{}'",
                                            module_name, method_call.method_name
                                        )
                                    })?;

                                let runtime_func_name = module_function.runtime_name.clone();
                                let return_type_str = module_function.return_type.clone();

                                // Convert type string to LLVM type
                                // TODO(PLAN:2025-11-10): 需要统一类型字符串到LLVM类型的转换逻辑
                                let return_type = if return_type_str.starts_with("未来<") {
                                    "ptr".to_string() // Future types are pointers
                                } else if return_type_str == "整数" || return_type_str == "i64" {
                                    "i64".to_string()
                                } else if return_type_str == "浮点数"
                                    || return_type_str == "double"
                                    || return_type_str == "f64"
                                {
                                    "double".to_string()
                                } else if return_type_str == "字符串"
                                    || return_type_str == "ptr"
                                    || return_type_str.contains("字符串")
                                {
                                    "ptr".to_string()
                                } else if return_type_str == "布尔"
                                    || return_type_str == "i1"
                                    || return_type_str == "bool"
                                {
                                    "i1".to_string()
                                } else if return_type_str == "i64"
                                    || return_type_str == "i32"
                                    || return_type_str == "i8"
                                {
                                    return_type_str // Already LLVM types
                                } else {
                                    // Fallback: try to convert unknown Chinese types to ptr
                                    eprintln!(
                                        "Warning: Unknown return type '{}', defaulting to ptr",
                                        return_type_str
                                    );
                                    "ptr".to_string()
                                };

                                // 构建参数
                                let mut args = vec![];
                                for arg in &method_call.arguments {
                                    args.push(self.build_node(arg)?);
                                }

                                // 生成临时变量并记录其类型
                                let temp = self.generate_temp();
                                // Store type without the % prefix for lookups
                                let temp_name = temp.trim_start_matches('%').to_string();
                                self.variable_types.insert(temp_name, return_type);

                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(temp.clone()),
                                    callee: runtime_func_name,
                                    arguments: args,
                                });
                                return Ok(temp);
                            } else {
                                // 这是本地模块，使用模块前缀
                                // 模块前缀调用，如 数学工具.最大值 -> 数学工具_最大值
                                let actual_module =
                                    self.import_aliases.get(module_name).unwrap_or(module_name);
                                let qualified_function_name =
                                    format!("{}_{}", actual_module, method_call.method_name);

                                // 构造函数调用
                                let func_name =
                                    if qualified_function_name.chars().any(|c| !c.is_ascii()) {
                                        self.mangle_function_name(&qualified_function_name)
                                    } else {
                                        qualified_function_name
                                    };

                                // 构建参数
                                let mut args = vec![];
                                for arg in &method_call.arguments {
                                    args.push(self.build_node(arg)?);
                                }

                                // 检查返回值类型
                                let has_return_value = if let Some(ret_type) =
                                    self.function_return_types.get(&func_name)
                                {
                                    ret_type != "void"
                                } else {
                                    true // 默认假设有返回值
                                };

                                if has_return_value {
                                    let temp = self.generate_temp();
                                    self.add_instruction(IrInstruction::函数调用 {
                                        dest: Some(temp.clone()),
                                        callee: func_name,
                                        arguments: args,
                                    });
                                    return Ok(temp);
                                } else {
                                    self.add_instruction(IrInstruction::函数调用 {
                                        dest: None,
                                        callee: func_name,
                                        arguments: args,
                                    });
                                    return Ok(String::new());
                                }
                            }
                        }
                    }
                }

                // 这是真正的方法调用：object.method(args)
                // 1. Get the object
                let object_var = self.build_node(&method_call.object)?;

                // 2. Get the struct type
                let object_var_name = object_var.trim_start_matches('%');
                let struct_type = self
                    .variable_struct_types
                    .get(object_var_name)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());

                // 3. Build method name: TypeName_methodName
                let method_full_name = format!("{}_{}", struct_type, method_call.method_name);
                let func_name = if method_full_name.chars().any(|c| !c.is_ascii()) {
                    self.mangle_function_name(&method_full_name)
                } else {
                    method_full_name
                };

                // 4. Build arguments - object is first argument
                let mut args = vec![object_var];
                for arg in &method_call.arguments {
                    args.push(self.build_node(arg)?);
                }

                // 5. Call the method
                // 解析返回类型：先查本模块函数表，再查外部(跨包/跨文件)函数表。
                // 跨包接收者方法的签名只存在 external_functions 里，漏查会默认成
                // i64，使结构体返回(ptr)被当 i64，链接期 call i64 vs ptr 失败。
                let resolved_ret_type = self
                    .function_return_types
                    .get(&func_name)
                    .cloned()
                    .or_else(|| {
                        self.external_functions
                            .get(&func_name)
                            .map(|(_p, rt)| rt.clone())
                    });
                let has_return_value = resolved_ret_type
                    .as_deref()
                    .map(|rt| rt != "void")
                    .unwrap_or(true);

                if has_return_value {
                    let temp = self.generate_temp();
                    // 记录方法调用结果的类型，否则下游（如 printf 的格式符/实参类型选择、
                    // 结构体字段访问）找不到该临时变量类型会默认成 i64。
                    let ret_type = resolved_ret_type.unwrap_or_else(|| "i64".to_string());
                    let temp_name = temp.trim_start_matches('%').to_string();
                    self.variable_types
                        .insert(temp_name.clone(), ret_type.clone());
                    // 若方法返回结构体指针，记录其结构体类型（本模块或外部）
                    if ret_type == "ptr" {
                        if let Some(struct_name) = self
                            .function_return_struct_types
                            .get(&func_name)
                            .cloned()
                            .or_else(|| {
                                self.external_function_return_struct_types
                                    .get(&func_name)
                                    .cloned()
                            })
                        {
                            self.variable_struct_types.insert(temp_name, struct_name);
                        }
                    }
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(temp.clone()),
                        callee: func_name,
                        arguments: args,
                    });
                    Ok(temp)
                } else {
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: None,
                        callee: func_name,
                        arguments: args,
                    });
                    Ok(String::new()) // Return empty string for void methods
                }
            }
            AstNode::协程启动表达式(goroutine_expr) => {
                // Handle different types of goroutine spawns
                match goroutine_expr.expression.as_ref() {
                    AstNode::函数调用表达式(call_expr) => {
                        // Spawn function call as goroutine
                        let function_name = self.get_full_function_name(call_expr);
                        let mangled_name = if function_name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&function_name)
                        } else {
                            function_name
                        };

                        // Build arguments
                        let mut arg_temps = Vec::new();
                        for arg in &call_expr.arguments {
                            let arg_result = self.build_node(arg)?;
                            if self.verbose {
                                eprintln!(
                                    "[DEBUG] Goroutine argument built: {:?} -> {}",
                                    arg, arg_result
                                );
                            }
                            arg_temps.push(arg_result);
                        }

                        // Resolve argument types NOW while we have access to variable_types
                        // Look up the target function's parameter types
                        let typed_args: Vec<String> = if let Some(param_types) =
                            self.function_param_types.get(&mangled_name)
                        {
                            if self.verbose {
                                eprintln!(
                                    "[DEBUG] Found param types for {}: {:?}",
                                    mangled_name, param_types
                                );
                            }
                            arg_temps
                                .iter()
                                .zip(param_types.iter())
                                .map(|(arg, expected_type)| {
                                    let arg_type = if arg.starts_with('%') {
                                        let var_name = arg.trim_start_matches('%');
                                        let resolved_type = self
                                            .variable_types
                                            .get(var_name)
                                            .or_else(|| {
                                                self.variable_types
                                                    .get(&format!("param_{}", var_name))
                                            })
                                            .map(|s| s.as_str())
                                            .unwrap_or(expected_type.as_str());
                                        if self.verbose {
                                            eprintln!(
                                                "[DEBUG] Resolving arg {} (var {}): resolved as {}",
                                                arg, var_name, resolved_type
                                            );
                                        }
                                        resolved_type
                                    } else if arg.starts_with('@') {
                                        "ptr"
                                    } else if arg.parse::<i64>().is_ok() {
                                        "i64"
                                    } else {
                                        expected_type.as_str()
                                    };
                                    let typed = format!("{}:{}", arg_type, arg);
                                    if self.verbose {
                                        eprintln!("[DEBUG] Typed arg: {}", typed);
                                    }
                                    typed
                                })
                                .collect()
                        } else {
                            if self.verbose {
                                eprintln!(
                                    "[DEBUG] No param types found for {}, using raw args",
                                    mangled_name
                                );
                            }
                            // No type info available, pass through as-is
                            arg_temps
                        };

                        // Generate goroutine spawn call
                        if self.verbose {
                            eprintln!(
                                "[DEBUG] Spawning goroutine {} with {} arguments: {:?}",
                                mangled_name,
                                typed_args.len(),
                                typed_args
                            );
                        }
                        self.add_instruction(IrInstruction::协程启动 {
                            function: mangled_name,
                            arguments: typed_args,
                        });
                    }
                    _ => {
                        // For other expressions, just evaluate them (simplified)
                        self.build_node(&goroutine_expr.expression)?;
                    }
                }

                Ok("goroutine".to_string())
            }

            AstNode::通道创建表达式(channel_expr) => {
                // Generate a temporary variable for the channel
                let channel_temp = self.generate_temp();

                // Get the channel type
                let channel_type = self.get_llvm_type(&Some(channel_expr.element_type.clone()));

                // Convert buffer size if present
                let buffer_size = channel_expr.capacity.as_ref().map(|size_expr| {
                    self.build_node(size_expr)
                        .unwrap_or_else(|_| "0".to_string())
                });

                self.add_instruction(IrInstruction::创建通道 {
                    dest: channel_temp.clone(),
                    channel_type,
                    buffer_size,
                });

                Ok(channel_temp)
            }

            AstNode::通道发送表达式(send_expr) => {
                // Build the channel expression
                let channel = self.build_node(&send_expr.channel)?;

                // Build the value to send, ensuring it's properly converted to pointer
                let value = self.build_node_for_channel(&send_expr.value)?;

                self.add_instruction(IrInstruction::通道发送 { channel, value });

                Ok("send".to_string())
            }

            AstNode::通道接收表达式(recv_expr) => {
                // Build the channel expression
                let channel = self.build_node(&recv_expr.channel)?;

                // Generate a temporary for the received value
                let recv_temp = self.generate_temp();

                self.add_instruction(IrInstruction::通道接收 {
                    dest: recv_temp.clone(),
                    channel,
                });

                Ok(recv_temp)
            }

            AstNode::选择表达式(select_expr) => {
                // 真实 select：非阻塞轮询 + 超时/默认分支。
                //
                // 生成的 LLVM IR 结构（每个 select 用唯一 id 区分基本块名）：
                //   br label %selN.poll
                //   selN.poll:
                //     ; 依次尝试每个 case 的非阻塞操作（try_receive / try_send）
                //     ; 若 ready -> br case body block；否则 -> 下一个 case 的 try block
                //   selN.notready:        ; 所有 case 都没就绪
                //     ; 有默认分支 -> br 默认 body（首轮立即生效）
                //     ; 有超时分支 -> 比较当前时间与 deadline；到点 -> 超时 body
                //     ; 否则 -> backoff 后回到 poll
                //   selN.backoff:
                //     call void @qi_runtime_select_backoff()  ; sleep ~1ms
                //     br label %selN.poll
                //   selN.caseK: <body>  br %selN.end
                //   ...
                //   selN.end:
                let sel_id = {
                    self.label_counter += 1;
                    self.label_counter
                };
                let poll_lbl = format!("sel{}.poll", sel_id);
                let notready_lbl = format!("sel{}.notready", sel_id);
                let backoff_lbl = format!("sel{}.backoff", sel_id);
                let end_lbl = format!("sel{}.end", sel_id);

                // 预先把每个真实 case（recv/send）的 channel/value 求值（在循环外只算一次），
                // 并为 recv case 分配结果 slot。
                struct PreparedCase {
                    is_send: bool,
                    channel: String,
                    // recv: 接收变量名（mangled）+ 原始名；send: 要发的 i64 值
                    recv_var: Option<(String, String)>,
                    recv_slot: Option<String>, // alloca ptr 存接收到的 value 指针
                    send_value: Option<String>,
                    body: Vec<AstNode>,
                    try_lbl: String,
                    body_lbl: String,
                }

                let mut prepared: Vec<PreparedCase> = Vec::new();
                let mut idx = 0usize;
                for case in &select_expr.cases {
                    match &case.kind {
                        crate::parser::ast::SelectCaseKind::通道接收 { channel, variable } => {
                            let channel_expr = self.build_node(channel)?;
                            // 为接收值分配一个 ptr slot（在 select 之前）
                            let slot = self.generate_temp();
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("  {} = alloca ptr, align 8", slot),
                            });
                            let recv_var = variable.as_ref().map(|v| {
                                let mangled = self.mangle_function_name(v);
                                // 注册接收变量类型（i64）并预分配 alloca
                                self.variable_types.insert(v.clone(), "i64".to_string());
                                self.variable_types
                                    .insert(mangled.clone(), "i64".to_string());
                                self.add_instruction(IrInstruction::分配 {
                                    dest: format!("%{}", mangled),
                                    type_name: "i64".to_string(),
                                });
                                (mangled, v.clone())
                            });
                            prepared.push(PreparedCase {
                                is_send: false,
                                channel: channel_expr,
                                recv_var,
                                recv_slot: Some(slot),
                                send_value: None,
                                body: case.body.clone(),
                                try_lbl: format!("sel{}.try{}", sel_id, idx),
                                body_lbl: format!("sel{}.case{}", sel_id, idx),
                            });
                            idx += 1;
                        }
                        crate::parser::ast::SelectCaseKind::通道发送 { channel, value } => {
                            let channel_expr = self.build_node(channel)?;
                            // build_node 对 i64 标识符会发出 load 并返回已加载的 i64 值（非指针），
                            // 对字面量返回立即数 —— 正好是 try_send 需要的 i64 操作数，无需再 load。
                            let send_i64 = self.build_node(value)?;
                            prepared.push(PreparedCase {
                                is_send: true,
                                channel: channel_expr,
                                recv_var: None,
                                recv_slot: None,
                                send_value: Some(send_i64),
                                body: case.body.clone(),
                                try_lbl: format!("sel{}.try{}", sel_id, idx),
                                body_lbl: format!("sel{}.case{}", sel_id, idx),
                            });
                            idx += 1;
                        }
                        crate::parser::ast::SelectCaseKind::默认 => {}
                        crate::parser::ast::SelectCaseKind::超时 { .. } => {}
                    }
                }

                let default_body: Option<&Vec<AstNode>> =
                    select_expr.default_case.as_ref().map(|c| &c.body);

                // 超时分支：计算绝对 deadline（毫秒）。在 select 进入前求值一次。
                let timeout_info: Option<(String, &Vec<AstNode>)> =
                    if let Some(tc) = &select_expr.timeout_case {
                        if let crate::parser::ast::SelectCaseKind::超时 { 毫秒 } = &tc.kind {
                            let ms_val = self.build_node(毫秒)?;
                            // ms 可能是指针变量，需要 load
                            let ms_i64 = if ms_val.starts_with('%')
                                && self
                                    .variable_types
                                    .get(ms_val.trim_start_matches('%'))
                                    .map(|t| t != "i64" || true)
                                    .unwrap_or(false)
                            {
                                let loaded = self.generate_temp();
                                self.add_instruction(IrInstruction::标签 {
                                    name: format!("  {} = load i64, ptr {}", loaded, ms_val),
                                });
                                loaded
                            } else {
                                ms_val
                            };
                            let now0 = self.generate_temp();
                            let deadline = self.generate_temp();
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("  {} = call i64 @qi_runtime_get_time_ms()", now0),
                            });
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("  {} = add i64 {}, {}", deadline, now0, ms_i64),
                            });
                            Some((deadline, &tc.body))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                // 进入轮询循环
                self.add_instruction(IrInstruction::跳转 {
                    label: poll_lbl.clone(),
                });
                self.add_instruction(IrInstruction::标签 {
                    name: poll_lbl.clone(),
                });
                // poll 块需要显式终结符跳到第一个 try 块（LLVM 不允许 fall-through）
                let first_try = if let Some(pc0) = prepared.first() {
                    pc0.try_lbl.clone()
                } else {
                    notready_lbl.clone()
                };
                self.add_instruction(IrInstruction::跳转 { label: first_try });

                // 依次尝试每个 case
                for (i, pc) in prepared.iter().enumerate() {
                    self.add_instruction(IrInstruction::标签 {
                        name: format!("{}:", pc.try_lbl),
                    });
                    let next_lbl = if i + 1 < prepared.len() {
                        prepared[i + 1].try_lbl.clone()
                    } else {
                        notready_lbl.clone()
                    };
                    let status = self.generate_temp();
                    if pc.is_send {
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  {} = call i32 @qi_runtime_channel_try_send(ptr {}, i64 {})",
                                status,
                                pc.channel,
                                pc.send_value.as_ref().unwrap()
                            ),
                        });
                    } else {
                        self.add_instruction(IrInstruction::标签 {
                            name: format!(
                                "  {} = call i32 @qi_runtime_channel_try_receive(ptr {}, ptr {})",
                                status,
                                pc.channel,
                                pc.recv_slot.as_ref().unwrap()
                            ),
                        });
                    }
                    let ready = self.generate_temp();
                    self.add_instruction(IrInstruction::标签 {
                        name: format!("  {} = icmp eq i32 {}, 0", ready, status),
                    });
                    self.add_instruction(IrInstruction::条件跳转 {
                        condition: ready,
                        true_label: pc.body_lbl.clone(),
                        false_label: next_lbl,
                    });
                }
                // 如果没有任何 case（理论上不会），直接跳到 notready
                if prepared.is_empty() {
                    self.add_instruction(IrInstruction::跳转 {
                        label: notready_lbl.clone(),
                    });
                }

                // notready：决定 默认 / 超时 / backoff
                self.add_instruction(IrInstruction::标签 {
                    name: format!("{}:", notready_lbl),
                });
                if let Some(_db) = default_body {
                    // 有默认分支 —— 首轮没就绪立即走默认
                    self.add_instruction(IrInstruction::跳转 {
                        label: format!("sel{}.default", sel_id),
                    });
                } else if let Some((deadline, _tb)) = &timeout_info {
                    let now = self.generate_temp();
                    let expired = self.generate_temp();
                    self.add_instruction(IrInstruction::标签 {
                        name: format!("  {} = call i64 @qi_runtime_get_time_ms()", now),
                    });
                    self.add_instruction(IrInstruction::标签 {
                        name: format!("  {} = icmp sge i64 {}, {}", expired, now, deadline),
                    });
                    self.add_instruction(IrInstruction::条件跳转 {
                        condition: expired,
                        true_label: format!("sel{}.timeout", sel_id),
                        false_label: backoff_lbl.clone(),
                    });
                } else {
                    // 既无默认也无超时 —— 退化为无限轮询（仍非阻塞，靠 backoff 让出 CPU）
                    self.add_instruction(IrInstruction::跳转 {
                        label: backoff_lbl.clone(),
                    });
                }

                // backoff：sleep ~1ms 后回到 poll
                self.add_instruction(IrInstruction::标签 {
                    name: format!("{}:", backoff_lbl),
                });
                self.add_instruction(IrInstruction::标签 {
                    // 末尾加 " ;" —— emit 时若给非 "= " 行补 ':'，会落在注释里，避免被当成 label
                    name: "  call void @qi_runtime_select_backoff() ;".to_string(),
                });
                self.add_instruction(IrInstruction::跳转 {
                    label: poll_lbl.clone(),
                });

                // 各 case body 基本块
                for pc in &prepared {
                    self.add_instruction(IrInstruction::标签 {
                        name: format!("{}:", pc.body_lbl),
                    });
                    // recv case：把接收到的值 load 出来，存进接收变量
                    if !pc.is_send {
                        let slot = pc.recv_slot.as_ref().unwrap();
                        let vptr = self.generate_temp();
                        let val = self.generate_temp();
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("  {} = load ptr, ptr {}", vptr, slot),
                        });
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("  {} = load i64, ptr {}", val, vptr),
                        });
                        if let Some((mangled, _orig)) = &pc.recv_var {
                            self.add_instruction(IrInstruction::标签 {
                                name: format!("  store i64 {}, ptr %{} ;", val, mangled),
                            });
                        }
                    }
                    for stmt in &pc.body {
                        self.build_node(stmt)?;
                    }
                    self.add_instruction(IrInstruction::跳转 {
                        label: end_lbl.clone(),
                    });
                }

                // 默认 body
                if let Some(db) = default_body {
                    self.add_instruction(IrInstruction::标签 {
                        name: format!("sel{}.default:", sel_id),
                    });
                    for stmt in db {
                        self.build_node(stmt)?;
                    }
                    self.add_instruction(IrInstruction::跳转 {
                        label: end_lbl.clone(),
                    });
                }

                // 超时 body
                if let Some((_deadline, tb)) = &timeout_info {
                    self.add_instruction(IrInstruction::标签 {
                        name: format!("sel{}.timeout:", sel_id),
                    });
                    for stmt in *tb {
                        self.build_node(stmt)?;
                    }
                    self.add_instruction(IrInstruction::跳转 {
                        label: end_lbl.clone(),
                    });
                }

                // end
                self.add_instruction(IrInstruction::标签 {
                    name: format!("{}:", end_lbl),
                });

                Ok("select".to_string())
            }

            AstNode::取地址表达式(addr_of_expr) => {
                // Get address of a variable
                // The inner expression should be a variable identifier
                if let AstNode::标识符表达式(ident) = addr_of_expr.expression.as_ref() {
                    // Mangle the variable name if it contains Chinese characters
                    let mangled_name = if ident.name.chars().any(|c| !c.is_ascii()) {
                        self.mangle_function_name(&ident.name)
                    } else {
                        ident.name.clone()
                    };

                    // Return the pointer to the variable (the alloca'd address)
                    let var_name = format!("%{}", mangled_name);
                    // The variable itself is already a pointer (alloca returns ptr)
                    // So we just return it directly
                    Ok(var_name)
                } else {
                    Err("取地址操作只能用于变量标识符".to_string())
                }
            }

            AstNode::解引用表达式(deref_expr) => {
                // Dereference a pointer
                let ptr_value = self.build_node(&deref_expr.expression)?;

                // Generate a temporary to hold the loaded value
                let result_temp = self.generate_temp();

                // Load the value from the pointer
                // Note: We need to determine the type of the pointed-to value
                // For now, we'll assume i64 (can be extended later for typed pointers)
                self.add_instruction(IrInstruction::加载 {
                    dest: result_temp.clone(),
                    source: ptr_value,
                    load_type: Some("i64".to_string()),
                });

                Ok(result_temp)
            }

            // ===== 新增语言特性代码生成 | New Language Features Code Generation =====
            AstNode::联合体声明(union_decl) => {
                // Union declarations create a type with the size of the largest variant
                // Similar to C unions - all variants share the same memory location
                let union_name = &union_decl.name;
                let mangled_name = self.mangle_function_name(union_name);

                // Calculate max size of all variants
                let mut max_size = 0i64;
                let mut variant_types = Vec::new();
                for variant in &union_decl.variants {
                    let llvm_type = self.get_llvm_type(&Some(variant.type_annotation.clone()));
                    let size = match llvm_type.as_str() {
                        "i64" | "double" | "ptr" => 8,
                        "i32" | "float" => 4,
                        "i16" => 2,
                        "i8" | "i1" => 1,
                        _ => 8, // Default to pointer size
                    };
                    if size > max_size {
                        max_size = size;
                    }
                    variant_types.push((variant.name.clone(), llvm_type));
                }

                // Store union definition for later use
                // Format: { tag: i8, padding: [alignment], data: [max_size bytes] }
                let fields = vec!["i8".to_string(), format!("[{} x i8]", max_size)];
                self.struct_definitions.insert(mangled_name.clone(), fields);

                let field_names: Vec<String> = vec!["tag".to_string(), "data".to_string()];
                self.struct_field_names.insert(mangled_name, field_names);

                Ok(String::new())
            }

            AstNode::尝试语句(try_stmt) => {
                // 真正的 try/catch/finally —— setjmp/longjmp 风格
                //
                // 生成的 IR：
                //   %buf = call ptr @qi_exc_alloc_frame()
                //   %r   = call i32 @setjmp(%buf) [returns_twice]
                //   %normal = icmp eq i32 %r, 0
                //   br i1 %normal, label %try_body, label %catch_entry
                // try_body:
                //   ; ... user try statements ...
                //   call void @qi_exc_pop()
                //   br label %finally_or_end
                // catch_entry:
                //   %err = call ptr @qi_exc_message()
                //   ; store %err to user error variable
                //   call void @qi_exc_clear()
                //   ; ... user catch statements ...
                //   br label %finally_or_end
                // finally:
                //   ; ... user finally statements ...
                //   br label %end
                // end:

                let body_label = self.generate_label();
                let catch_label = self.generate_label();
                let after_catch_label = self.generate_label();
                let end_label = self.generate_label();

                // 1. 分配 jmp_buf + push 异常栈
                let buf_tmp = self.generate_temp();
                self.add_instruction(IrInstruction::函数调用 {
                    dest: Some(buf_tmp.clone()),
                    callee: "qi_exc_alloc_frame".to_string(),
                    arguments: vec![],
                });
                self.variable_types.insert(
                    buf_tmp.trim_start_matches('%').to_string(),
                    "ptr".to_string(),
                );

                // 2. 调用 setjmp — 必须在 caller 栈帧执行，所以直接调 libc
                let setjmp_tmp = self.generate_temp();
                self.add_instruction(IrInstruction::函数调用 {
                    dest: Some(setjmp_tmp.clone()),
                    callee: "setjmp".to_string(),
                    arguments: vec![buf_tmp.clone()],
                });
                self.variable_types.insert(
                    setjmp_tmp.trim_start_matches('%').to_string(),
                    "i32".to_string(),
                );

                // 3. 比较 setjmp 结果：0 → 进 try body；非 0 → 进 catch
                let cmp_tmp = self.generate_temp();
                self.add_instruction(IrInstruction::二元操作 {
                    dest: cmp_tmp.clone(),
                    left: setjmp_tmp.clone(),
                    operator: BinaryOperator::等于,
                    right: "0".to_string(),
                    operand_type: "i32".to_string(),
                });
                self.add_instruction(IrInstruction::条件跳转 {
                    condition: cmp_tmp,
                    true_label: body_label.clone(),
                    false_label: catch_label.clone(),
                });

                // 4. try body
                self.add_instruction(IrInstruction::标签 { name: body_label });
                for stmt in &try_stmt.try_body {
                    self.build_node(stmt)?;
                }
                // 正常退出 try：弹栈、跳到 catch 后的 finally/end
                self.add_instruction(IrInstruction::函数调用 {
                    dest: None,
                    callee: "qi_exc_pop".to_string(),
                    arguments: vec![],
                });
                self.add_instruction(IrInstruction::跳转 {
                    label: after_catch_label.clone(),
                });

                // 5. catch entry
                self.add_instruction(IrInstruction::标签 { name: catch_label });

                // 弹栈（catch 内代码不再受这个 frame 保护；嵌套 try 可以重新 push）
                self.add_instruction(IrInstruction::函数调用 {
                    dest: None,
                    callee: "qi_exc_pop".to_string(),
                    arguments: vec![],
                });

                // 没有 catch 子句 — 只有 finally：跑完 finally 后 re-throw
                if try_stmt.catch_clauses.is_empty() {
                    // 取异常消息（在 finally 跑前取，避免 finally 内嵌套 try 改 last_error）
                    let saved_msg_tmp = self.generate_temp();
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(saved_msg_tmp.clone()),
                        callee: "qi_exc_message".to_string(),
                        arguments: vec![],
                    });
                    self.variable_types.insert(
                        saved_msg_tmp.trim_start_matches('%').to_string(),
                        "ptr".to_string(),
                    );

                    if let Some(finally_body) = &try_stmt.finally_body {
                        for stmt in finally_body {
                            self.build_node(stmt)?;
                        }
                    }
                    // re-throw 用保存的消息
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: None,
                        callee: "qi_exc_throw".to_string(),
                        arguments: vec![saved_msg_tmp],
                    });
                    self.add_instruction(IrInstruction::不可达);

                    // try 正常退出路径走的 after_catch_label —— 给它一个"跑 finally → end"的实现
                    self.add_instruction(IrInstruction::标签 {
                        name: after_catch_label,
                    });
                    if let Some(finally_body) = &try_stmt.finally_body {
                        for stmt in finally_body {
                            self.build_node(stmt)?;
                        }
                    }
                    self.add_instruction(IrInstruction::跳转 {
                        label: end_label.clone(),
                    });
                    self.add_instruction(IrInstruction::标签 { name: end_label });
                    return Ok(String::new());
                }

                // 取错误消息（用于绑定到 catch error variable）
                let err_msg_tmp = self.generate_temp();
                self.add_instruction(IrInstruction::函数调用 {
                    dest: Some(err_msg_tmp.clone()),
                    callee: "qi_exc_message".to_string(),
                    arguments: vec![],
                });
                self.variable_types.insert(
                    err_msg_tmp.trim_start_matches('%').to_string(),
                    "ptr".to_string(),
                );

                // 走每个 catch 子句（目前只支持第一个匹配 — 没有类型 dispatch）
                for catch_clause in &try_stmt.catch_clauses {
                    let mut prev_alias_entry: Option<(String, Option<String>)> = None;
                    if let Some(error_var) = &catch_clause.error_var {
                        // 生成唯一的内部 alloca 名 — 避免同函数内多个 try 的 catch
                        // 用同一用户标识符时 LLVM 报 "multiple definition"。
                        // 中文字符在 LLVM IR 中非法，要用 mangle_function_name 编码。
                        let mangled_var = if error_var.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(error_var)
                        } else {
                            error_var.clone()
                        };
                        let unique_name = format!("__catch_{}_{}", mangled_var, self.label_counter);
                        self.label_counter += 1;
                        let alloca_name = format!("%{}", unique_name);

                        self.add_instruction(IrInstruction::分配 {
                            dest: alloca_name.clone(),
                            type_name: "ptr".to_string(),
                        });
                        self.add_instruction(IrInstruction::存储 {
                            target: alloca_name,
                            value: err_msg_tmp.clone(),
                            value_type: Some("ptr".to_string()),
                        });
                        self.variable_types
                            .insert(error_var.clone(), "ptr".to_string());
                        self.variable_types
                            .insert(unique_name.clone(), "ptr".to_string());
                        // 把用户标识符映射到唯一内部名；保存旧映射以便 catch 退出时恢复
                        let old = self.variable_alias.insert(error_var.clone(), unique_name);
                        prev_alias_entry = Some((error_var.clone(), old));
                    }

                    for stmt in &catch_clause.body {
                        self.build_node(stmt)?;
                    }

                    // 还原 alias（避免 catch 之外仍把 error_var 当成内部名）
                    if let Some((key, old)) = prev_alias_entry {
                        match old {
                            Some(v) => {
                                self.variable_alias.insert(key, v);
                            }
                            None => {
                                self.variable_alias.remove(&key);
                            }
                        }
                    }

                    break; // 第一个 catch 子句已经处理；多个子句没有类型分派
                }

                // 清空错误消息，避免外层 catch 看到旧消息
                self.add_instruction(IrInstruction::函数调用 {
                    dest: None,
                    callee: "qi_exc_clear".to_string(),
                    arguments: vec![],
                });

                self.add_instruction(IrInstruction::跳转 {
                    label: after_catch_label.clone(),
                });

                // 6. catch 之后：finally（如果有），否则 end
                self.add_instruction(IrInstruction::标签 {
                    name: after_catch_label,
                });
                if let Some(finally_body) = &try_stmt.finally_body {
                    for stmt in finally_body {
                        self.build_node(stmt)?;
                    }
                }
                self.add_instruction(IrInstruction::跳转 {
                    label: end_label.clone(),
                });

                // 7. end
                self.add_instruction(IrInstruction::标签 { name: end_label });

                Ok(String::new())
            }

            AstNode::抛出语句(throw_stmt) => {
                // 真正的 throw —— 调 qi_exc_throw（内部 longjmp 到栈顶 frame）
                let error_value = self.build_node(&throw_stmt.expression)?;

                self.add_instruction(IrInstruction::函数调用 {
                    dest: None,
                    callee: "qi_exc_throw".to_string(),
                    arguments: vec![error_value],
                });

                // throw 后控制流不可达
                self.add_instruction(IrInstruction::不可达);

                Ok(String::new())
            }

            AstNode::异步块表达式(async_block) => {
                // Async block creates a Future that will execute the block
                // Generate a wrapper function for the async block
                let wrapper_name = format!("__async_block_{}", self.temp_counter);
                self.temp_counter += 1;

                // Build the block statements
                for stmt in &async_block.body {
                    self.build_node(stmt)?;
                }

                // Return a Future<void> for now
                let result_temp = self.generate_temp();
                self.add_instruction(IrInstruction::函数调用 {
                    dest: Some(result_temp.clone()),
                    callee: "qi_future_ready_i64".to_string(),
                    arguments: vec!["0".to_string()],
                });

                Ok(result_temp)
            }

            AstNode::闭包表达式(closure_expr) => {
                // 真闭包：堆分配 env，挂全局函数，闭包值是 fat object 指针
                let closure_name = format!("__closure_{}", self.closure_counter);
                self.closure_counter += 1;

                // 1. 收集 freevar — 闭包体里引用、但不是参数也不是局部声明、也不是已知函数的标识符
                let mut local_names: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                for p in &closure_expr.parameters {
                    local_names.insert(p.name.clone());
                }
                let mut frees: Vec<String> = Vec::new();
                for stmt in &closure_expr.body {
                    self.collect_free_identifiers(stmt, &mut local_names, &mut frees);
                }

                // 过滤：保留实际在外层作用域可见的（变量类型表里有，且不是已注册的全局函数）
                let mut captured: Vec<(String, String)> = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for name in frees {
                    if seen.contains(&name) {
                        continue;
                    }
                    seen.insert(name.clone());
                    // 跳过已知函数（全局函数引用不算捕获）
                    if self.function_return_types.contains_key(&name)
                        || self.defined_functions.contains(&name)
                        || self.external_functions.contains_key(&name)
                    {
                        continue;
                    }
                    // 看 variable_types 里是否记录
                    let ty = self
                        .variable_types
                        .get(&name)
                        .or_else(|| {
                            let mangled = self.mangled_bare_name(&name);
                            self.variable_types.get(&mangled)
                        })
                        .cloned();
                    if let Some(t) = ty {
                        captured.push((name, t));
                    }
                    // 没记录就当全局/未知，不捕获
                }

                // 2. 构造闭包顶层函数 AST：env 参数 + 序言（从 env 读 caps 到本地 var）+ 用户体
                let closure_decl =
                    self.synthesize_closure_function(&closure_name, closure_expr, &captured);
                self.pending_closures.push(closure_decl);

                // 3. 当前位置：malloc env + 设 fn_ptr + 填 caps，返回 obj 指针
                let obj_tmp = self.generate_temp();
                self.add_instruction(IrInstruction::函数调用 {
                    dest: Some(obj_tmp.clone()),
                    callee: "qi_closure_create".to_string(),
                    arguments: vec![format!("@{}", closure_name), captured.len().to_string()],
                });

                for (i, (name, ty)) in captured.iter().enumerate() {
                    // 加载用户当前作用域的变量值
                    let val_tmp = self.build_node(&AstNode::标识符表达式(
                        crate::parser::ast::IdentifierExpression {
                            name: name.clone(),
                            span: Default::default(),
                        },
                    ))?;
                    let setter = if ty == "ptr" {
                        "qi_closure_set_ptr"
                    } else {
                        "qi_closure_set_int"
                    };
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: None,
                        callee: setter.to_string(),
                        arguments: vec![obj_tmp.clone(), i.to_string(), val_tmp],
                    });
                }

                // 4. 标记闭包变量类型 + 记签名（供调用点查）
                let key = obj_tmp.trim_start_matches('%').to_string();
                self.variable_types.insert(key.clone(), "ptr".to_string());
                self.closure_variables.insert(key.clone());

                let param_types: Vec<String> = closure_expr
                    .parameters
                    .iter()
                    .map(|p| self.get_llvm_type(&p.type_annotation))
                    .collect();
                let ret_type = self.get_return_type(&closure_expr.return_type);
                self.closure_signatures.insert(key, (param_types, ret_type));

                Ok(obj_tmp)
            }

            AstNode::匹配表达式(match_expr) => {
                // Match expression - compiles to a series of comparisons and branches
                let value_result = self.build_node(&match_expr.value)?;
                let result_temp = self.generate_temp();
                let end_label = self.generate_label();

                // Use a unique prefix for all labels in this match expression
                let match_id = self.label_counter;
                self.label_counter += 1;
                let match_start_label = format!("match_{}_start", match_id);

                // Allocate storage for the result
                self.add_instruction(IrInstruction::分配 {
                    dest: result_temp.clone(),
                    type_name: "i64".to_string(), // Default result type
                });

                // Jump to the first match arm (required terminator before label)
                self.add_instruction(IrInstruction::跳转 {
                    label: match_start_label.clone(),
                });

                // Start of match arms
                self.add_instruction(IrInstruction::标签 {
                    name: match_start_label,
                });

                // Generate code for each arm
                let num_arms = match_expr.arms.len();
                for (i, arm) in match_expr.arms.iter().enumerate() {
                    // Use unique label names per match expression
                    let arm_label = format!("match_{}_arm_{}", match_id, i);
                    let arm_body_label = format!("match_{}_arm_{}_body", match_id, i);
                    let next_arm_label = if i < num_arms - 1 {
                        format!("match_{}_arm_{}", match_id, i + 1)
                    } else {
                        end_label.clone() // Last arm falls through to end
                    };

                    // For the first arm, we're already at the start label, so just continue
                    // For subsequent arms, we need the label
                    if i > 0 {
                        self.add_instruction(IrInstruction::标签 {
                            name: arm_label.clone(),
                        });
                    }

                    // Generate pattern match condition and branching
                    match &arm.pattern {
                        crate::parser::ast::MatchPattern::字面量(lit_value) => {
                            let cond_temp = self.generate_temp();
                            match lit_value {
                                crate::parser::ast::LiteralValue::整数(n) => {
                                    // Compare value_result with n
                                    self.add_instruction(IrInstruction::二元操作 {
                                        dest: cond_temp.clone(),
                                        left: value_result.clone(),
                                        operator: crate::parser::ast::BinaryOperator::等于,
                                        right: n.to_string(),
                                        operand_type: "i64".to_string(),
                                    });
                                }
                                crate::parser::ast::LiteralValue::布尔(b) => {
                                    let bool_val = if *b { "1" } else { "0" };
                                    self.add_instruction(IrInstruction::二元操作 {
                                        dest: cond_temp.clone(),
                                        left: value_result.clone(),
                                        operator: crate::parser::ast::BinaryOperator::等于,
                                        right: bool_val.to_string(),
                                        operand_type: "i1".to_string(),
                                    });
                                }
                                _ => {
                                    // For other literal types, generate a true condition
                                    self.add_instruction(IrInstruction::布尔常量 {
                                        dest: cond_temp.clone(),
                                        value: 1,
                                    });
                                }
                            }

                            // Branch based on condition
                            self.add_instruction(IrInstruction::条件跳转 {
                                condition: cond_temp,
                                true_label: arm_body_label.clone(),
                                false_label: next_arm_label.clone(),
                            });
                        }
                        crate::parser::ast::MatchPattern::通配符 => {
                            // Wildcard always matches - jump directly to body
                            self.add_instruction(IrInstruction::跳转 {
                                label: arm_body_label.clone(),
                            });
                        }
                        crate::parser::ast::MatchPattern::变量绑定(var_name) => {
                            // Variable binding - always matches, binds the value then jumps to body
                            let mangled_name = self.mangle_function_name(var_name);
                            self.add_instruction(IrInstruction::分配 {
                                dest: format!("%{}", mangled_name),
                                type_name: "i64".to_string(),
                            });
                            self.add_instruction(IrInstruction::存储 {
                                target: format!("%{}", mangled_name),
                                value: value_result.clone(),
                                value_type: Some("i64".to_string()),
                            });
                            self.variable_types.insert(mangled_name, "i64".to_string());
                            // Jump to body after binding
                            self.add_instruction(IrInstruction::跳转 {
                                label: arm_body_label.clone(),
                            });
                        }
                        _ => {
                            // Other patterns - default to always match
                            self.add_instruction(IrInstruction::跳转 {
                                label: arm_body_label.clone(),
                            });
                        }
                    }

                    // Arm body label
                    self.add_instruction(IrInstruction::标签 {
                        name: arm_body_label,
                    });

                    // Build arm body
                    for stmt in &arm.body {
                        self.build_node(stmt)?;
                    }

                    // Jump to end after arm body
                    self.add_instruction(IrInstruction::跳转 {
                        label: end_label.clone(),
                    });
                }

                // End label
                self.add_instruction(IrInstruction::标签 { name: end_label });

                // Load result
                let loaded_result = self.generate_temp();
                self.add_instruction(IrInstruction::加载 {
                    dest: loaded_result.clone(),
                    source: result_temp,
                    load_type: Some("i64".to_string()),
                });

                Ok(loaded_result)
            }

            AstNode::格式字符串表达式(format_str) => {
                // Format string expression - builds a string by concatenating literal parts and evaluated expressions
                // We build the result by starting with the first part and concatenating

                let mut current_result: Option<String> = None;

                for part in &format_str.parts {
                    match part {
                        crate::parser::ast::FormatStringPart::文本(text) => {
                            // Create a string literal for the text using the existing string literal handling
                            let text_node = AstNode::字面量表达式(
                                crate::parser::ast::LiteralExpression {
                                    value: crate::parser::ast::LiteralValue::字符串(
                                        text.clone(),
                                    ),
                                    span: Default::default(),
                                },
                            );
                            let text_result = self.build_node(&text_node)?;

                            if let Some(current) = current_result {
                                // Concatenate current + text
                                let new_result = self.generate_temp();
                                self.variable_types.insert(
                                    new_result.trim_start_matches('%').to_string(),
                                    "ptr".to_string(),
                                );
                                self.add_instruction(IrInstruction::字符串连接 {
                                    dest: new_result.clone(),
                                    left: current,
                                    right: text_result,
                                });
                                current_result = Some(new_result);
                            } else {
                                current_result = Some(text_result);
                            }
                        }
                        crate::parser::ast::FormatStringPart::表达式 { expr, format: _ } => {
                            // Evaluate the expression
                            let expr_result = self.build_node(expr)?;

                            // Check if the result is already a string - extract type info before mutable borrow
                            let expr_type_opt = if expr_result.starts_with('%') {
                                self.variable_types
                                    .get(expr_result.trim_start_matches('%'))
                                    .cloned()
                            } else {
                                None
                            };
                            let is_string = expr_result.starts_with('@')
                                || expr_type_opt.as_ref().map(|t| *t == "ptr").unwrap_or(false);

                            let str_result = if is_string {
                                expr_result
                            } else {
                                // Convert to string based on type
                                let expr_type = expr_type_opt.unwrap_or("i64".to_string());
                                let is_float = expr_type == "double" || expr_type.contains("float");

                                let conv_temp = self.generate_temp();
                                let conv_func = if is_float {
                                    "qi_runtime_float_to_string"
                                } else {
                                    "qi_runtime_int_to_string"
                                };
                                self.variable_types.insert(
                                    conv_temp.trim_start_matches('%').to_string(),
                                    "ptr".to_string(),
                                );
                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(conv_temp.clone()),
                                    callee: conv_func.to_string(),
                                    arguments: vec![expr_result],
                                });
                                conv_temp
                            };

                            if let Some(current) = current_result {
                                // Concatenate current + expression result
                                let new_result = self.generate_temp();
                                self.variable_types.insert(
                                    new_result.trim_start_matches('%').to_string(),
                                    "ptr".to_string(),
                                );
                                self.add_instruction(IrInstruction::字符串连接 {
                                    dest: new_result.clone(),
                                    left: current,
                                    right: str_result,
                                });
                                current_result = Some(new_result);
                            } else {
                                current_result = Some(str_result);
                            }
                        }
                    }
                }

                // Return the result, or empty string if no parts
                Ok(current_result.unwrap_or_else(|| {
                    let temp = self.generate_temp();
                    self.add_instruction(IrInstruction::字符串常量 {
                        name:
                            "@.emptystr = private unnamed_addr constant [1 x i8] c\"\\00\", align 1"
                                .to_string(),
                    });
                    temp
                }))
            }

            _ =>
            {
                #[allow(unreachable_patterns)]
                Err(format!("Unsupported AST node: {:?}", node))
            }
        }
    }

    /// Build a node and ensure it's properly converted for channel operations
    /// Returns a pointer to the value
    fn build_node_for_channel(&mut self, expr: &AstNode) -> Result<String, String> {
        match expr {
            AstNode::字面量表达式(literal) => {
                // For literals, we need to allocate storage and store the value
                let temp = self.generate_temp();
                let (value_type, value_temp) = match &literal.value {
                    crate::parser::ast::LiteralValue::整数(n) => {
                        let temp_val = self.generate_temp();
                        self.add_instruction(IrInstruction::整数常量 {
                            dest: temp_val.clone(),
                            value: *n,
                        });
                        ("i64", temp_val)
                    }
                    crate::parser::ast::LiteralValue::浮点数(f) => {
                        let temp_val = self.generate_temp();
                        self.add_instruction(IrInstruction::浮点数常量 {
                            dest: temp_val.clone(),
                            value: *f,
                        });
                        ("double", temp_val)
                    }
                    crate::parser::ast::LiteralValue::布尔(b) => {
                        let temp_val = self.generate_temp();
                        let bool_value = if *b { 1 } else { 0 };
                        self.add_instruction(IrInstruction::布尔常量 {
                            dest: temp_val.clone(),
                            value: bool_value as i8,
                        });
                        // Track the temporary variable type
                        let temp_var_name = temp_val.trim_start_matches('%');
                        self.variable_types
                            .insert(temp_var_name.to_string(), "i1".to_string());
                        ("i1", temp_val)
                    }
                    crate::parser::ast::LiteralValue::字符(c) => {
                        let temp_val = self.generate_temp();
                        self.add_instruction(IrInstruction::整数常量 {
                            dest: temp_val.clone(),
                            value: *c as i64,
                        });
                        ("i8", temp_val)
                    }
                    crate::parser::ast::LiteralValue::字符串(s) => {
                        // For string literals, use the existing string constant handling
                        let str_name = format!("@.str{}", self.temp_counter);
                        self.temp_counter += 1;
                        let escaped_str = s
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('\n', "\\0A");
                        self.add_instruction(IrInstruction::字符串常量 {
                            name: format!(
                                "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1",
                                str_name,
                                s.len() + 1,
                                escaped_str
                            ),
                        });
                        ("ptr", str_name.clone())
                    }
                };

                // Allocate storage for the value
                self.add_instruction(IrInstruction::分配 {
                    dest: temp.clone(),
                    type_name: value_type.to_string(),
                });

                // Store the value
                self.add_instruction(IrInstruction::存储 {
                    target: temp.clone(),
                    value: value_temp.clone(),
                    value_type: Some(value_type.to_string()),
                });

                Ok(temp)
            }
            AstNode::标识符表达式(_) => {
                // Resolve the identifier through the normal expression path so that
                // PARAMETERS (already an i64 value register, name-mangled — e.g. a
                // 通道<整数> worker's `编号` param) are used directly, while LOCAL
                // variables get a proper load and GLOBALS resolve to @-symbols.
                // The previous code naively emitted `load i64, ptr %<raw-name>`,
                // which produced invalid IR for parameters (unmangled name + an i64
                // value treated as a pointer) and broke any goroutine/channel
                // function that sends a parameter value.
                let value = self.build_node(expr)?;

                // 通道发送 expects a pointer-to-storage it can `load i64` from, so
                // box the resolved i64 value into a fresh alloca and hand that back.
                let var_type = self
                    .infer_ir_value_type(&value)
                    .unwrap_or_else(|| "i64".to_string());

                let temp_copy = self.generate_temp();
                self.add_instruction(IrInstruction::分配 {
                    dest: temp_copy.clone(),
                    type_name: var_type.clone(),
                });
                self.add_instruction(IrInstruction::存储 {
                    target: temp_copy.clone(),
                    value,
                    value_type: Some(var_type),
                });

                Ok(temp_copy)
            }
            _ => {
                // For other expressions, build normally
                self.build_node(expr)
            }
        }
    }

    /// Get LLVM type string from type annotation
    fn get_llvm_type(&self, type_annotation: &Option<crate::parser::ast::TypeNode>) -> String {
        match type_annotation {
            Some(crate::parser::ast::TypeNode::基础类型(basic_type)) => {
                match basic_type {
                    crate::parser::ast::BasicType::整数 => "i64".to_string(),
                    crate::parser::ast::BasicType::长整数 => "i64".to_string(),
                    crate::parser::ast::BasicType::短整数 => "i16".to_string(),
                    crate::parser::ast::BasicType::字节 => "i8".to_string(),
                    crate::parser::ast::BasicType::浮点数 => "double".to_string(),
                    crate::parser::ast::BasicType::布尔 => "i1".to_string(),
                    crate::parser::ast::BasicType::字符 => "i8".to_string(),
                    crate::parser::ast::BasicType::字符串 => "ptr".to_string(),
                    crate::parser::ast::BasicType::空 => "void".to_string(),
                    crate::parser::ast::BasicType::数组 => "ptr".to_string(), // Simplified for now
                    crate::parser::ast::BasicType::字典 => "ptr".to_string(), // Simplified for now
                    crate::parser::ast::BasicType::列表 => "ptr".to_string(), // Simplified for now
                    crate::parser::ast::BasicType::集合 => "ptr".to_string(), // Simplified for now
                    crate::parser::ast::BasicType::指针 => "ptr".to_string(),
                    crate::parser::ast::BasicType::引用 => "ptr".to_string(),
                    crate::parser::ast::BasicType::可变引用 => "ptr".to_string(),
                }
            }
            Some(crate::parser::ast::TypeNode::通道类型(_)) => {
                // Channel creation returns a pointer (handle) to the channel
                "ptr".to_string()
            }
            Some(crate::parser::ast::TypeNode::数组类型(_)) => {
                // Array types (e.g., 数组<整数>) are represented as pointers to array data
                "ptr".to_string()
            }
            Some(crate::parser::ast::TypeNode::结构体类型(_)) => "ptr".to_string(),
            Some(crate::parser::ast::TypeNode::自定义类型(_)) => "ptr".to_string(),
            Some(crate::parser::ast::TypeNode::指针类型(_)) => "ptr".to_string(),
            Some(crate::parser::ast::TypeNode::函数类型(_)) => "ptr".to_string(),
            Some(crate::parser::ast::TypeNode::未来类型(_inner_type)) => {
                // Future types are represented as pointers to Future runtime structs
                // The Future<T> is a heap-allocated structure managed by the runtime
                "ptr".to_string()
            }
            _ => "i64".to_string(), // Default to i64
        }
    }

    /// Get return type for function
    fn get_return_type(&self, return_type: &Option<crate::parser::ast::TypeNode>) -> String {
        self.get_llvm_type(return_type)
    }

    /// Emit LLVM IR from instructions
    fn emit_llvm_ir(&mut self) -> Result<String, String> {
        let mut ir = String::new();
        let mut string_constants = Vec::new();
        let mut other_instructions = Vec::new();
        let _temp_counter = self.temp_counter; // reserved for future use
        let mut current_function_ret_ty: Option<String> = None;
        // Param SSA names injected into self.variable_types per-function during emission,
        // so cross-function instructions don't lose param type info after codegen reset.
        let mut injected_param_keys: Vec<String> = Vec::new();

        // Clone instructions to avoid borrow checker issues
        let instructions = self.instructions.clone();

        // Separate string constants from other instructions
        for instruction in &instructions {
            match instruction {
                IrInstruction::字符串常量 { .. } => {
                    string_constants.push(instruction);
                }
                _ => {
                    other_instructions.push(instruction);
                }
            }
        }

        // Add module header
        ir.push_str("; Generated by Qi Language Compiler\n");
        ir.push_str("; Module ID = 'qi_program'\n");
        // 目标三元组/datalayout 按宿主平台来：qi 编译/运行本机可执行文件。
        // 之前写死 macOS，导致在 Linux 上 clang 编译 .ll 直接报错（每个示例都失败）。
        // macOS 保持原样；其它平台不写死，交给 clang 用宿主默认（Linux/Windows 自动正确）。
        #[cfg(target_os = "macos")]
        {
            ir.push_str("target datalayout = \"e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32\"\n");
            ir.push_str("target triple = \"arm64-apple-macosx26.0.0\"\n\n");
        }
        #[cfg(not(target_os = "macos"))]
        {
            ir.push_str("\n");
        }

        // Add struct type definitions
        if !self.struct_definitions.is_empty() {
            ir.push_str("; Struct type definitions\n");
            for (struct_name, field_types) in &self.struct_definitions {
                let mangled_name = self.mangle_type_name(&format!("{}.type", struct_name));
                let fields_str = field_types.join(", ");
                ir.push_str(&format!("{} = type {{ {} }}\n", mangled_name, fields_str));
            }
            ir.push_str("\n");
        }

        // Add Qi Runtime function declarations
        ir.push_str("; Qi Runtime declarations\n");
        ir.push_str("; Core runtime functions\n");
        ir.push_str("declare i32 @qi_runtime_initialize()\n");
        ir.push_str("declare i32 @qi_runtime_shutdown()\n");
        ir.push_str("declare i32 @qi_runtime_execute(ptr, i64)\n");
        ir.push_str("\n");

        // Async runtime functions
        ir.push_str("; Async runtime functions\n");
        ir.push_str("declare ptr @qi_runtime_create_task(ptr, i64)\n");
        ir.push_str("declare ptr @qi_runtime_await(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_spawn_task(ptr)\n");
        ir.push_str("\n");

        // Future type functions
        ir.push_str("; Future type functions - integer\n");
        ir.push_str("declare ptr @qi_future_ready_i64(i64)\n");
        ir.push_str("declare i64 @qi_future_await_i64(ptr)\n");
        ir.push_str("\n");
        ir.push_str("; Future type functions - float\n");
        ir.push_str("declare ptr @qi_future_ready_f64(double)\n");
        ir.push_str("declare double @qi_future_await_f64(ptr)\n");
        ir.push_str("\n");
        ir.push_str("; Future type functions - boolean\n");
        ir.push_str("declare ptr @qi_future_ready_bool(i32)\n");
        ir.push_str("declare i32 @qi_future_await_bool(ptr)\n");
        ir.push_str("\n");
        ir.push_str("; Future type functions - string\n");
        ir.push_str("declare ptr @qi_future_ready_string(ptr, i64)\n");
        ir.push_str("declare ptr @qi_future_await_string(ptr)\n");
        ir.push_str("\n");
        ir.push_str("; Future type functions - pointer (for structs)\n");
        ir.push_str("declare ptr @qi_future_ready_ptr(ptr)\n");
        ir.push_str("declare ptr @qi_future_await_ptr(ptr)\n");
        ir.push_str("\n");
        ir.push_str("; Future type functions - common\n");
        ir.push_str("declare ptr @qi_future_failed(ptr, i64)\n");
        ir.push_str("declare i32 @qi_future_is_completed(ptr)\n");
        ir.push_str("declare void @qi_future_free(ptr)\n");
        ir.push_str("declare void @qi_string_free(ptr)\n");
        ir.push_str("\n");

        // String utility functions
        ir.push_str("; String utility functions\n");
        ir.push_str("declare i64 @strlen(ptr)\n");
        ir.push_str("\n");

        // Memory allocation functions
        ir.push_str("; Memory allocation functions\n");
        ir.push_str("declare ptr @malloc(i64)\n");
        ir.push_str("declare void @free(ptr)\n");
        ir.push_str("\n");

        // Concurrency functions - Channel operations
        ir.push_str("; Concurrency functions - Channel operations\n");
        ir.push_str("declare ptr @qi_runtime_create_channel(i64)\n");
        ir.push_str("declare i32 @qi_runtime_channel_send(ptr, i64)\n");
        ir.push_str("declare i32 @qi_runtime_channel_receive(ptr, ptr)\n");
        ir.push_str("declare i32 @qi_runtime_channel_try_receive(ptr, ptr)\n");
        ir.push_str("declare i32 @qi_runtime_channel_try_send(ptr, i64)\n");
        ir.push_str("declare void @qi_runtime_select_backoff()\n");
        ir.push_str("declare i32 @qi_runtime_channel_close(ptr)\n");
        ir.push_str("\n");

        // Synchronization functions - WaitGroup operations
        ir.push_str("; Synchronization functions - WaitGroup operations\n");
        ir.push_str("declare ptr @qi_runtime_waitgroup_create()\n");
        ir.push_str("declare i32 @qi_runtime_waitgroup_add(ptr, i32)\n");
        ir.push_str("declare i32 @qi_runtime_waitgroup_wait(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_waitgroup_done(ptr)\n");
        ir.push_str("\n");

        // Synchronization functions - Mutex operations
        ir.push_str("; Synchronization functions - Mutex operations\n");
        ir.push_str("declare ptr @qi_runtime_mutex_create()\n");
        ir.push_str("declare i32 @qi_runtime_mutex_lock(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_mutex_unlock(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_mutex_trylock(ptr)\n");
        ir.push_str("\n");

        // Timeout and error handling functions
        ir.push_str("; Timeout and error handling functions\n");
        ir.push_str("declare i64 @qi_runtime_get_time_ms()\n");
        ir.push_str("declare i64 @qi_runtime_set_timeout(i64)\n");
        ir.push_str("declare i32 @qi_runtime_check_timeout(i64)\n");
        ir.push_str("declare ptr @qi_runtime_timer_create(i64)\n");
        ir.push_str("declare i64 @qi_runtime_timer_expired(ptr)\n");
        ir.push_str("declare i64 @qi_runtime_timer_stop(ptr)\n");
        ir.push_str("\n");

        // Chinese function names (HEX encoded)
        ir.push_str("; Chinese function names (HEX encoded)\n");
        ir.push_str("declare ptr @e5_88_9b_e5_bb_ba_e9_80_9a_e9_81_93(i64)\n"); // 创建通道
        ir.push_str("declare i32 @e5_8f_91_e9_80_81_int(ptr, i64)\n"); // 发送
        ir.push_str("declare i64 @e6_a5_a5_e6_8e_af_int(ptr)\n"); // 接收
        ir.push_str("declare i32 @e5_85_b3_e9_97_ad_e9_80_9a_e9_81_93(ptr)\n"); // 关闭通道
        ir.push_str("\n");

        ir.push_str("declare ptr @e5_88_9b_e5_bb_ba_e7_ad_89_e5_be_85_e7_bb_84()\n"); // 创建等待组
        ir.push_str("declare i32 @e6_8b_89_e5_a0_80_e7_ad_89_e5_be_85(ptr, i32)\n"); // 添加等待
        ir.push_str("declare i32 @e7_ad_89_e5_be_85(ptr)\n"); // 等待
        ir.push_str("declare i32 @e5_ae_8c_e6_88_90(ptr)\n"); // 完成
        ir.push_str("\n");

        ir.push_str("declare ptr @e5_88_9b_e5_bb_ba_e4_ba_92_e6_96_a5_e9_94_81()\n"); // 创建互斥锁
        ir.push_str("declare i32 @e5_8a_a0_e9_94_81(ptr)\n"); // 加锁
        ir.push_str("declare i32 @e8_a3_a3_e9_94_81(ptr)\n"); // 解锁
        ir.push_str("declare i32 @e5_b0_9d_e8_af_95_e5_8a_a0_e9_94_81(ptr)\n"); // 尝试加锁
        ir.push_str("\n");

        ir.push_str("declare i64 @e8_b7_a5_e5_8f_96_e9_97_b4_e9_97_b4()\n"); // 获取时间
        ir.push_str("declare i32 @e8_ae_bd_e7_ba_ae_e8_b6_85_e6_97_b6(i64)\n"); // 设置超时
        ir.push_str("declare i32 @e6_8f_a5_e6_9f_a5_e8_b6_85_e6_97_b6(i64)\n"); // 检查超时
        ir.push_str("declare ptr @e5_88_9b_e5_bb_ba_e5_b0_a8_e6_97_b6_e5_99_a8(i64)\n"); // 创建定时器
        ir.push_str("declare i32 @e9_87_8d_e8_af_95_e6_93_8d_e4_bd_9c(i32, i32, i32)\n"); // 重试操作
        ir.push_str("\n");

        // Goroutine spawn functions
        ir.push_str("; Goroutine spawn functions\n");
        ir.push_str("declare void @qi_runtime_spawn_goroutine(ptr)\n");
        ir.push_str("declare void @qi_runtime_spawn_goroutine_with_args(ptr, ptr, i64)\n");
        // qi_runtime_async_serve 通过 external_functions 自动按需声明（避免重复）
        ir.push_str("declare ptr @qi_runtime_select(ptr)\n");
        ir.push_str("declare void @qi_runtime_timer_cancel(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_retry(ptr, i32)\n");
        ir.push_str("declare i32 @qi_runtime_catch_error(ptr)\n");
        ir.push_str("\n");

        // Crypto functions（除 free_string 外其他都已在 external_functions 注册，
        // 自动 declare 由 emit 路径生成；这里只保留 free_string）
        ir.push_str("; Crypto functions\n");
        ir.push_str("declare void @qi_crypto_free_string(ptr)\n");
        ir.push_str("\n");

        // IO functions
        ir.push_str("; IO functions\n");
        ir.push_str("declare ptr @qi_io_read_file(ptr)\n");
        ir.push_str("declare i64 @qi_io_write_file(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_io_append_file(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_io_delete_file(ptr)\n");
        ir.push_str("declare i64 @qi_io_create_file(ptr)\n");
        ir.push_str("declare i64 @qi_io_file_exists(ptr)\n");
        ir.push_str("declare i64 @qi_io_file_size(ptr)\n");
        ir.push_str("declare i64 @qi_io_create_dir(ptr)\n");
        ir.push_str("declare i64 @qi_io_delete_dir(ptr)\n");
        ir.push_str("declare void @qi_io_free_string(ptr)\n");
        ir.push_str("\n");

        // JSON functions
        ir.push_str("; JSON functions\n");
        ir.push_str("declare ptr @qi_json_encode(i64)\n");
        ir.push_str("declare i64 @qi_json_decode(ptr)\n");
        ir.push_str("declare i64 @qi_json_create_object()\n");
        ir.push_str("declare i64 @qi_json_create_array()\n");
        ir.push_str("declare i64 @qi_json_set_string(i64, ptr, ptr)\n");
        ir.push_str("declare i64 @qi_json_set_int(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_json_set_float(i64, ptr, double)\n");
        ir.push_str("declare i64 @qi_json_set_bool(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_json_set_object(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_json_set_array(i64, ptr, i64)\n");
        ir.push_str("declare ptr @qi_json_get_string(i64, ptr)\n");
        ir.push_str("declare i64 @qi_json_get_int(i64, ptr)\n");
        ir.push_str("declare double @qi_json_get_float(i64, ptr)\n");
        ir.push_str("declare i64 @qi_json_get_bool(i64, ptr)\n");
        ir.push_str("declare i64 @qi_json_get_object(i64, ptr)\n");
        ir.push_str("declare i64 @qi_json_get_array(i64, ptr)\n");
        ir.push_str("declare i64 @qi_json_array_push_string(i64, ptr)\n");
        ir.push_str("declare i64 @qi_json_array_push_int(i64, i64)\n");
        ir.push_str("declare i64 @qi_json_array_push_float(i64, double)\n");
        ir.push_str("declare i64 @qi_json_array_push_bool(i64, i64)\n");
        ir.push_str("declare i64 @qi_json_array_push_object(i64, i64)\n");
        ir.push_str("declare i64 @qi_json_array_push_array(i64, i64)\n");
        ir.push_str("declare ptr @qi_json_array_get_string(i64, i64)\n");
        ir.push_str("declare i64 @qi_json_array_get_int(i64, i64)\n");
        ir.push_str("declare double @qi_json_array_get_float(i64, i64)\n");
        ir.push_str("declare i64 @qi_json_array_get_bool(i64, i64)\n");
        ir.push_str("declare i64 @qi_json_array_get_object(i64, i64)\n");
        ir.push_str("declare i64 @qi_json_array_get_array(i64, i64)\n");
        ir.push_str("declare i64 @qi_json_array_length(i64)\n");
        ir.push_str("declare i64 @qi_json_has_key(i64, ptr)\n");
        ir.push_str("declare i64 @qi_json_free(i64)\n");
        ir.push_str("declare ptr @qi_json_to_string(i64)\n");
        ir.push_str("declare ptr @qi_json_to_string_pretty(i64)\n");
        ir.push_str("declare ptr @qi_json_from_pairs(ptr)\n");
        ir.push_str("declare ptr @qi_json_from_text(ptr)\n");
        ir.push_str("declare void @qi_json_free_string(ptr)\n");
        ir.push_str("\n");

        // Network functions
        ir.push_str("; Network functions\n");
        // TCP Client functions
        ir.push_str("declare i64 @qi_network_tcp_connect(ptr, i16, i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_read(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_write(i64, ptr, i64)\n");
        ir.push_str("declare ptr @qi_network_tcp_read_string(i64, i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_write_string(i64, ptr)\n");
        ir.push_str("declare i64 @qi_network_tcp_close(i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_flush(i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_bytes_read(i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_bytes_written(i64)\n");

        // TCP Server functions
        ir.push_str("declare i64 @qi_network_tcp_listen(ptr, i16, i32)\n");
        ir.push_str("declare i64 @qi_network_tcp_accept(i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_server_close(i64)\n");

        // UDP functions
        ir.push_str("declare i64 @qi_network_udp_bind(ptr, i16)\n");
        ir.push_str("declare i64 @qi_network_udp_send_string(i64, ptr, ptr, i16)\n");
        ir.push_str("declare i64 @qi_network_udp_send_to(i64, ptr, i64, ptr, i16)\n");
        ir.push_str("declare ptr @qi_network_udp_recv_string(i64, i64)\n");
        ir.push_str("declare i64 @qi_network_udp_recv_from(i64, ptr, i64, ptr, ptr)\n");
        ir.push_str("declare i64 @qi_network_udp_close(i64)\n");
        ir.push_str("declare i64 @qi_network_udp_set_timeout(i64, i64)\n");
        ir.push_str("declare i64 @qi_network_udp_set_broadcast(i64, i32)\n");

        // Network utility functions
        ir.push_str("declare ptr @qi_network_resolve_host(ptr)\n");
        ir.push_str("declare i64 @qi_network_port_available(i16)\n");
        ir.push_str("declare ptr @qi_network_get_local_ip()\n");
        ir.push_str("declare void @qi_network_free_string(ptr)\n");
        ir.push_str("\n");

        // HTTP functions
        ir.push_str("; HTTP functions\n");
        ir.push_str("declare i64 @qi_http_init()\n");
        ir.push_str("declare ptr @qi_http_get(ptr)\n");
        ir.push_str("declare ptr @qi_http_post(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_http_request(ptr, ptr, ptr, ptr)\n");
        ir.push_str("declare ptr @qi_http_put(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_http_delete(ptr)\n");
        ir.push_str("declare ptr @qi_http_head(ptr)\n");
        ir.push_str("declare ptr @qi_http_patch(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_http_options(ptr)\n");
        ir.push_str("declare i64 @qi_http_request_create(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_http_request_set_header(i64, ptr, ptr)\n");
        ir.push_str("declare i64 @qi_http_request_set_body(i64, ptr)\n");
        ir.push_str("declare i64 @qi_http_request_set_timeout(i64, i64)\n");
        ir.push_str("declare ptr @qi_http_request_execute(i64)\n");
        ir.push_str("declare i64 @qi_http_get_status(ptr)\n");
        ir.push_str("declare void @qi_http_free_string(ptr)\n");

        // HTTP Server functions
        ir.push_str("declare i64 @qi_http_server_create(ptr, i64)\n");
        ir.push_str("declare ptr @qi_http_server_handle_request(i64, ptr, i64)\n");
        ir.push_str("declare ptr @qi_http_server_accept(i64)\n");
        ir.push_str("declare i64 @qi_http_server_close(i64)\n");
        ir.push_str("\n");

        // WebSocket functions
        ir.push_str("; WebSocket functions\n");
        ir.push_str("declare i64 @qi_websocket_connect(ptr)\n");
        ir.push_str("declare i64 @qi_websocket_accept(ptr, i16)\n");
        ir.push_str("declare i64 @qi_websocket_send_text(i64, ptr)\n");
        ir.push_str("declare ptr @qi_websocket_recv_text(i64)\n");
        ir.push_str("declare i64 @qi_websocket_send_binary(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_websocket_ping(i64)\n");
        ir.push_str("declare i64 @qi_websocket_close(i64, i16, ptr)\n");
        ir.push_str("declare i64 @qi_websocket_is_connected(i64)\n");
        ir.push_str("declare i64 @qi_websocket_is_upgrade_request(ptr)\n");
        ir.push_str("declare ptr @qi_websocket_get_client_key(ptr)\n");
        ir.push_str("declare ptr @qi_websocket_create_upgrade_response(ptr)\n");
        ir.push_str("declare void @qi_websocket_free_string(ptr)\n");
        ir.push_str("declare i64 @qi_websocket_register_tcp(i64, i64)\n");
        ir.push_str("declare i64 @qi_websocket_unregister(i64)\n");
        ir.push_str("\n");

        // LLM functions — 仅 free_string 没在 external_functions（其他 19 个自动 declare）
        ir.push_str("; LLM (Large Language Model) functions\n");
        ir.push_str("declare void @qi_llm_free_string(ptr)\n");
        ir.push_str("\n");

        // OS functions
        ir.push_str("; OS (Operating System) functions\n");
        ir.push_str("declare ptr @qi_os_getenv(ptr)\n");
        ir.push_str("declare i64 @qi_os_setenv(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_os_unsetenv(ptr)\n");
        ir.push_str("declare ptr @qi_os_environ()\n");
        ir.push_str("declare ptr @qi_os_getcwd()\n");
        ir.push_str("declare i64 @qi_os_chdir(ptr)\n");
        ir.push_str("declare ptr @qi_os_homedir()\n");
        ir.push_str("declare ptr @qi_os_tempdir()\n");
        ir.push_str("declare ptr @qi_os_type()\n");
        ir.push_str("declare ptr @qi_os_arch()\n");
        ir.push_str("declare ptr @qi_os_family()\n");
        ir.push_str("declare ptr @qi_os_hostname()\n");
        ir.push_str("declare ptr @qi_os_username()\n");
        ir.push_str("declare i64 @qi_os_cpu_count()\n");
        ir.push_str("declare i64 @qi_os_getpid()\n");
        ir.push_str("declare void @qi_os_exit(i32)\n");
        ir.push_str("declare i64 @qi_os_load_env(ptr)\n");
        ir.push_str("declare ptr @qi_os_list_dir(ptr)\n");
        ir.push_str("declare i64 @qi_os_is_dir(ptr)\n");
        ir.push_str("declare i64 @qi_os_is_file(ptr)\n");
        ir.push_str("declare void @qi_os_free_string(ptr)\n");
        ir.push_str("\n");

        ir.push_str("; CLI argument parsing functions\n");
        ir.push_str("declare i64 @qi_cli_create_app(ptr)\n");
        ir.push_str("declare i64 @qi_cli_set_version(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_set_author(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_set_about(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_set_long_about(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_set_override_usage(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_set_after_help(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_create_arg(ptr)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_short(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_long(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_help(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_required(i64, i64)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_default(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_flag(i64)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_multiple(i64)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_env(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_arg_set_global(i64)\n");
        ir.push_str("declare ptr @qi_web_call_handler_safe(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_web_safe_process_request(ptr, ptr, ptr)\n");
        ir.push_str("declare i64 @qi_web_panic_for_test()\n");
        ir.push_str("declare i64 @qi_tls_create_config(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_tls_free_config(i64)\n");
        ir.push_str("declare i64 @qi_tls_listen(ptr, i64, i64, i64)\n");
        ir.push_str("declare i64 @qi_tls_accept(i64)\n");
        ir.push_str("declare ptr @qi_tls_read_string(i64, i64)\n");
        ir.push_str("declare i64 @qi_tls_write_string(i64, ptr)\n");
        ir.push_str("declare i64 @qi_tls_close(i64)\n");
        ir.push_str("declare i64 @qi_tls_server_close(i64)\n");
        ir.push_str("declare void @qi_tls_free_string(ptr)\n");
        ir.push_str("declare i64 @qi_h2_serve(ptr, ptr, ptr, i64, ptr, ptr)\n");
        ir.push_str("declare i64 @qi_bytes_create()\n");
        ir.push_str("declare i64 @qi_bytes_with_capacity(i64)\n");
        ir.push_str("declare i64 @qi_bytes_from_string(ptr)\n");
        ir.push_str("declare ptr @qi_bytes_to_string(i64)\n");
        ir.push_str("declare i64 @qi_bytes_length(i64)\n");
        ir.push_str("declare i64 @qi_bytes_get(i64, i64)\n");
        ir.push_str("declare i64 @qi_bytes_set(i64, i64, i64)\n");
        ir.push_str("declare i64 @qi_bytes_push(i64, i64)\n");
        ir.push_str("declare i64 @qi_bytes_push_string(i64, ptr)\n");
        ir.push_str("declare i64 @qi_bytes_extend(i64, i64)\n");
        ir.push_str("declare i64 @qi_bytes_slice(i64, i64, i64)\n");
        ir.push_str("declare i64 @qi_bytes_compare(i64, i64)\n");
        ir.push_str("declare i64 @qi_bytes_find(i64, i64)\n");
        ir.push_str("declare ptr @qi_bytes_to_hex(i64)\n");
        ir.push_str("declare i64 @qi_bytes_from_hex(ptr)\n");
        ir.push_str("declare ptr @qi_bytes_to_base64(i64)\n");
        ir.push_str("declare i64 @qi_bytes_from_base64(ptr)\n");
        ir.push_str("declare i64 @qi_bytes_free(i64)\n");
        ir.push_str("declare void @qi_bytes_free_string(ptr)\n");
        // Closure runtime
        ir.push_str("declare ptr @qi_closure_create(ptr, i64)\n");
        ir.push_str("declare ptr @qi_closure_get_fn(ptr)\n");
        ir.push_str("declare i64 @qi_closure_get_int(ptr, i64)\n");
        ir.push_str("declare ptr @qi_closure_get_ptr(ptr, i64)\n");
        ir.push_str("declare void @qi_closure_set_int(ptr, i64, i64)\n");
        ir.push_str("declare void @qi_closure_set_ptr(ptr, i64, ptr)\n");

        // Exception (try/catch/throw) runtime
        ir.push_str("declare ptr @qi_exc_alloc_frame()\n");
        // Direct libc setjmp — must be called from caller's stack frame, not wrapped.
        // 'returns_twice' attribute prevents LLVM from sinking allocas across the call.
        ir.push_str("declare i32 @setjmp(ptr) #2\n");
        ir.push_str("declare void @qi_exc_pop()\n");
        ir.push_str("declare void @qi_exc_throw(ptr) noreturn\n");
        ir.push_str("declare ptr @qi_exc_message()\n");
        ir.push_str("declare void @qi_exc_clear()\n");
        ir.push_str("declare void @qi_exc_free_message(ptr)\n");
        ir.push_str("attributes #2 = { returns_twice }\n");
        ir.push_str("declare i64 @qi_signal_install_shutdown()\n");
        ir.push_str("declare i64 @qi_signal_should_shutdown()\n");
        ir.push_str("declare i64 @qi_signal_reset()\n");
        ir.push_str("declare i64 @qi_network_tcp_listener_set_nonblocking(i64, i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_read_bytes(i64, i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_write_bytes(i64, i64)\n");
        ir.push_str("declare i64 @qi_multipart_parse(i64, ptr)\n");
        ir.push_str("declare ptr @qi_multipart_extract_boundary(ptr)\n");
        ir.push_str("declare i64 @qi_multipart_count(i64)\n");
        ir.push_str("declare ptr @qi_multipart_name(i64, i64)\n");
        ir.push_str("declare ptr @qi_multipart_filename(i64, i64)\n");
        ir.push_str("declare ptr @qi_multipart_content_type(i64, i64)\n");
        ir.push_str("declare i64 @qi_multipart_body(i64, i64)\n");
        ir.push_str("declare i64 @qi_multipart_free(i64)\n");
        ir.push_str("declare i64 @qi_cli_app_add_arg(i64, i64)\n");
        ir.push_str("declare i64 @qi_cli_create_subcommand(ptr)\n");
        ir.push_str("declare i64 @qi_cli_app_add_subcommand(i64, i64)\n");
        ir.push_str("declare i64 @qi_cli_app_add_alias(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_print_help(i64)\n");
        ir.push_str("declare i64 @qi_cli_parse(i64)\n");
        ir.push_str("declare ptr @qi_cli_get_value(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_get_flag(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_has_value(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_has_subcommand(i64, ptr)\n");
        ir.push_str("declare i64 @qi_cli_get_subcommand(i64, ptr)\n");
        ir.push_str("declare void @qi_cli_free_string(ptr)\n");
        ir.push_str("declare i64 @qi_cli_free_app(i64)\n");
        ir.push_str("declare i64 @qi_cli_free_arg(i64)\n");
        ir.push_str("declare i64 @qi_cli_free_matches(i64)\n");
        ir.push_str("\n");

        // MCP Server functions
        ir.push_str("; MCP Server functions\n");
        ir.push_str("declare i64 @qi_mcp_create_server(ptr, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_mcp_start_server(i64)\n");
        ir.push_str("declare i32 @qi_mcp_stop_server(i64)\n");
        ir.push_str("declare i32 @qi_mcp_is_running(i64)\n");
        ir.push_str("declare i32 @qi_mcp_destroy_server(i64)\n");
        ir.push_str("declare ptr @qi_mcp_get_server_info(i64)\n");
        ir.push_str("declare i32 @qi_mcp_register_tool(i64, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_mcp_add_tool_parameter(i64, ptr, ptr, ptr, ptr, i32)\n");
        ir.push_str("declare i32 @qi_mcp_set_tool_callback(i64, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_mcp_set_tool_callback_ptr(i64, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_mcp_serve_stdio(i64)\n");
        ir.push_str("declare i32 @qi_mcp_serve_http(i64, ptr, i64)\n");
        ir.push_str("declare ptr @qi_mcp_list_tools(i64)\n");
        ir.push_str("declare ptr @qi_mcp_call_tool(i64, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_mcp_register_resource(i64, ptr, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_mcp_set_resource_text_content(i64, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_mcp_set_resource_json_content(i64, ptr, ptr)\n");
        ir.push_str("declare ptr @qi_mcp_read_resource_text(i64, ptr)\n");
        ir.push_str("declare ptr @qi_mcp_read_resource_json(i64, ptr)\n");
        ir.push_str("declare ptr @qi_mcp_list_resources(i64)\n");
        ir.push_str("declare i32 @qi_mcp_register_prompt(i64, ptr, ptr)\n");
        ir.push_str("declare ptr @qi_mcp_list_prompts(i64)\n");
        ir.push_str("declare ptr @qi_mcp_get_prompt(i64, ptr)\n");
        ir.push_str("declare void @qi_mcp_free_string(ptr)\n");
        // P2: 服务器→客户端推送通知
        ir.push_str("declare i32 @qi_mcp_notify_tools_changed(i64)\n");
        ir.push_str("declare i32 @qi_mcp_notify_resources_changed(i64)\n");
        ir.push_str("declare i32 @qi_mcp_notify_prompts_changed(i64)\n");
        ir.push_str("declare i32 @qi_mcp_log_message(i64, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_mcp_notify_progress(i64, ptr, i64, i64)\n");
        ir.push_str("\n");

        // Regex functions
        ir.push_str("; Regex functions\n");
        ir.push_str("declare i32 @qi_regex_is_match(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_regex_find(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_regex_find_all(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_regex_replace_all(ptr, ptr, ptr)\n");
        ir.push_str("declare ptr @qi_regex_split(ptr, ptr)\n");
        ir.push_str("declare void @qi_regex_free_string(ptr)\n");
        ir.push_str("\n");

        // Path functions
        ir.push_str("; Path functions\n");
        ir.push_str("declare ptr @qi_path_join(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_path_filename(ptr)\n");
        ir.push_str("declare ptr @qi_path_parent(ptr)\n");
        ir.push_str("declare ptr @qi_path_extension(ptr)\n");
        ir.push_str("declare ptr @qi_path_absolute(ptr)\n");
        ir.push_str("declare i32 @qi_path_exists(ptr)\n");
        ir.push_str("declare i32 @qi_path_is_dir(ptr)\n");
        ir.push_str("declare i32 @qi_path_is_file(ptr)\n");
        ir.push_str("declare void @qi_path_free_string(ptr)\n");
        ir.push_str("\n");

        // Random functions
        ir.push_str("; Random functions\n");
        ir.push_str("declare i64 @qi_random_int(i64, i64)\n");
        ir.push_str("declare double @qi_random_float(double, double)\n");
        ir.push_str("declare i32 @qi_random_bool()\n");
        ir.push_str("declare ptr @qi_random_string(i64)\n");
        ir.push_str("declare ptr @qi_random_uuid()\n");
        ir.push_str("declare void @qi_random_free_string(ptr)\n");
        ir.push_str("\n");

        // Environment functions
        ir.push_str("; Environment functions\n");
        ir.push_str("declare ptr @qi_env_get(ptr)\n");
        ir.push_str("declare i32 @qi_env_set(ptr, ptr)\n");
        ir.push_str("declare i32 @qi_env_remove(ptr)\n");
        ir.push_str("declare ptr @qi_env_current_dir()\n");
        ir.push_str("declare i32 @qi_env_set_current_dir(ptr)\n");
        ir.push_str("declare ptr @qi_env_home_dir()\n");
        ir.push_str("declare ptr @qi_env_all()\n");
        ir.push_str("declare void @qi_env_free_string(ptr)\n");
        ir.push_str("\n");

        // Process functions
        ir.push_str("; Process functions\n");
        ir.push_str("declare ptr @qi_process_execute(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_process_current_pid()\n");
        ir.push_str("declare void @qi_process_exit(i32)\n");
        ir.push_str("declare void @qi_process_free_string(ptr)\n");
        ir.push_str("\n");

        // Subprocess functions
        ir.push_str("; Subprocess functions\n");
        ir.push_str("declare i64 @qi_subprocess_spawn(ptr, ptr)\n");
        ir.push_str("declare i32 @qi_subprocess_write_line(i64, ptr)\n");
        ir.push_str("declare ptr @qi_subprocess_read_line(i64)\n");
        ir.push_str("declare ptr @qi_subprocess_read_line_timeout(i64, i64)\n");
        ir.push_str("declare i32 @qi_subprocess_is_alive(i64)\n");
        ir.push_str("declare i32 @qi_subprocess_terminate(i64)\n");
        ir.push_str("declare void @qi_subprocess_free_string(ptr)\n");
        ir.push_str("\n");

        // Sync primitives (标准库.同步) — mutex + atomic i64
        ir.push_str("; Sync primitives (标准库.同步)\n");
        ir.push_str("declare i64 @qi_sync_mutex_create()\n");
        ir.push_str("declare i32 @qi_sync_mutex_lock(i64)\n");
        ir.push_str("declare i32 @qi_sync_mutex_unlock(i64)\n");
        ir.push_str("declare i32 @qi_sync_mutex_trylock(i64)\n");
        ir.push_str("declare i32 @qi_sync_mutex_destroy(i64)\n");
        ir.push_str("declare i64 @qi_sync_atomic_create(i64)\n");
        ir.push_str("declare i64 @qi_sync_atomic_load(i64)\n");
        ir.push_str("declare i32 @qi_sync_atomic_store(i64, i64)\n");
        ir.push_str("declare i64 @qi_sync_atomic_add(i64, i64)\n");
        ir.push_str("declare i32 @qi_sync_atomic_cas(i64, i64, i64)\n");
        ir.push_str("declare i32 @qi_sync_atomic_destroy(i64)\n");
        ir.push_str("\n");

        // MCP Client core functions (标准库.MCP客户端)
        ir.push_str("; MCP Client core (标准库.MCP客户端)\n");
        ir.push_str("declare i64 @qi_mcpc_connect_stdio(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_mcpc_connect_http(ptr)\n");
        ir.push_str("declare ptr @qi_mcpc_request(i64, ptr, ptr)\n");
        ir.push_str("declare i64 @qi_mcpc_close(i64)\n");
        ir.push_str("declare i32 @qi_mcpc_set_sampling_handler(i64, ptr)\n");
        ir.push_str("declare i32 @qi_mcpc_set_elicitation_handler(i64, ptr)\n");
        ir.push_str("declare i32 @qi_mcpc_set_roots(i64, ptr)\n");
        ir.push_str("declare ptr @qi_mcpc_drain_notifications(i64)\n");
        ir.push_str("declare void @qi_mcpc_free_string(ptr)\n");
        ir.push_str("\n");

        // Config functions
        ir.push_str("; Config functions\n");
        ir.push_str("declare ptr @qi_config_read_toml(ptr)\n");
        ir.push_str("declare i32 @qi_config_write_toml(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_config_read_ini(ptr)\n");
        ir.push_str("declare i32 @qi_config_write_ini(ptr, ptr)\n");
        ir.push_str("declare void @qi_config_free_string(ptr)\n");
        ir.push_str("\n");

        // Compress functions
        ir.push_str("; Compress functions\n");
        ir.push_str("declare i32 @qi_compress_gzip_file(ptr, ptr)\n");
        ir.push_str("declare i32 @qi_compress_gunzip_file(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_compress_gzip_string(ptr)\n");
        ir.push_str("declare ptr @qi_compress_gunzip_string(ptr)\n");
        ir.push_str("declare void @qi_compress_free_string(ptr)\n");
        // 注：qi_compress_gzip_bytes / qi_compress_gunzip_bytes 通过 external_functions
        // 自动按需声明（避免与 module 调用点重复）
        ir.push_str("\n");

        // Test functions
        ir.push_str("; Test functions\n");
        ir.push_str("declare i32 @qi_test_assert_eq_int(i64, i64, ptr)\n");
        ir.push_str("declare i32 @qi_test_assert_eq_float(double, double, ptr)\n");
        ir.push_str("declare i32 @qi_test_assert_eq_string(ptr, ptr, ptr)\n");
        ir.push_str("declare i32 @qi_test_assert_true(i32, ptr)\n");
        ir.push_str("declare i32 @qi_test_assert_false(i32, ptr)\n");
        ir.push_str("declare i32 @qi_test_assert_ne_int(i64, i64, ptr)\n");
        ir.push_str("declare void @qi_test_pass(ptr)\n");
        ir.push_str("declare void @qi_test_fail(ptr)\n");
        ir.push_str("\n");

        // Database functions
        ir.push_str("; Database functions\n");
        ir.push_str("declare i64 @qi_db_connect(ptr)\n");
        ir.push_str("declare i64 @qi_db_execute(i64, ptr)\n");
        ir.push_str("declare ptr @qi_db_query(i64, ptr)\n");
        ir.push_str("declare i32 @qi_db_close(i64)\n");
        ir.push_str("declare i32 @qi_db_begin_transaction(i64)\n");
        ir.push_str("declare i32 @qi_db_commit(i64)\n");
        ir.push_str("declare i32 @qi_db_rollback(i64)\n");
        ir.push_str("declare void @qi_db_free_string(ptr)\n");
        ir.push_str("\n");

        // GUI functions
        ir.push_str("; GUI functions\n");
        ir.push_str("declare i64 @qi_gui_create_window(ptr, i64, i64)\n");
        ir.push_str("declare void @qi_gui_destroy_window(i64)\n");
        ir.push_str("declare void @qi_gui_set_title(i64, ptr)\n");
        ir.push_str("declare ptr @qi_gui_get_title(i64)\n");
        ir.push_str("declare void @qi_gui_show_window(i64)\n");
        ir.push_str("declare void @qi_gui_hide_window(i64)\n");
        ir.push_str("declare i64 @qi_gui_is_visible(i64)\n");
        ir.push_str("declare void @qi_gui_enable_event_printing(i64)\n");
        ir.push_str("declare i64 @qi_gui_get_position_x(i64)\n");
        ir.push_str("declare i64 @qi_gui_get_position_y(i64)\n");
        ir.push_str("declare void @qi_gui_set_position(i64, i64, i64)\n");
        ir.push_str("declare i64 @qi_gui_get_width(i64)\n");
        ir.push_str("declare i64 @qi_gui_get_height(i64)\n");
        ir.push_str("declare void @qi_gui_set_size(i64, i64, i64)\n");
        ir.push_str("declare void @qi_gui_on_event(i64, ptr)\n");
        ir.push_str("declare void @qi_gui_set_timer(i64)\n");
        ir.push_str("declare void @qi_gui_set_fps(i64)\n");
        ir.push_str("declare void @qi_gui_run()\n");
        ir.push_str("declare ptr @qi_gui_version()\n");
        ir.push_str("declare void @qi_gui_free_string(ptr)\n");

        // GUI Audio functions
        ir.push_str("; GUI Audio functions\n");
        ir.push_str("declare i64 @qi_gui_audio_load(ptr)\n");
        ir.push_str("declare void @qi_gui_audio_play(i64)\n");
        ir.push_str("declare void @qi_gui_audio_pause(i64)\n");
        ir.push_str("declare void @qi_gui_audio_stop(i64)\n");
        ir.push_str("declare void @qi_gui_audio_set_volume(i64, double)\n");
        ir.push_str("declare i64 @qi_gui_audio_is_playing(i64)\n");
        ir.push_str("declare i64 @qi_gui_audio_is_finished(i64)\n");
        ir.push_str("declare void @qi_gui_audio_free(i64)\n");
        ir.push_str("\n");

        // GUI Renderer functions
        ir.push_str("; GUI Renderer functions\n");
        ir.push_str("declare i64 @qi_gui_renderer_create(i64)\n");
        ir.push_str("declare void @qi_gui_renderer_begin_frame(i64)\n");
        ir.push_str("declare void @qi_gui_renderer_end_frame(i64)\n");
        ir.push_str("declare void @qi_gui_renderer_clear(i64, i64, i64, i64)\n");
        ir.push_str("declare void @qi_gui_renderer_draw_pixel(i64, i64, i64, i64, i64, i64)\n");
        ir.push_str(
            "declare void @qi_gui_renderer_draw_rect(i64, i64, i64, i64, i64, i64, i64, i64)\n",
        );
        ir.push_str(
            "declare void @qi_gui_renderer_draw_line(i64, i64, i64, i64, i64, i64, i64, i64)\n",
        );
        ir.push_str(
            "declare void @qi_gui_renderer_draw_circle(i64, i64, i64, i64, i64, i64, i64)\n",
        );
        ir.push_str("declare i64 @qi_gui_renderer_draw_image(i64, ptr, i64, i64)\n");
        ir.push_str("declare void @qi_gui_renderer_draw_text(i64, ptr, i64, i64, i64, i64, i64)\n");
        ir.push_str("declare void @qi_gui_renderer_draw_text_scaled(i64, ptr, i64, i64, i64, i64, i64, i64)\n");
        ir.push_str("declare void @qi_gui_renderer_free(i64)\n");
        ir.push_str("\n");

        // Data structure functions (List, HashMap, DateTime)
        ir.push_str("; List functions\n");
        ir.push_str("declare i64 @qi_list_int_create()\n");
        ir.push_str("declare i64 @qi_list_int_push(i64, i64)\n");
        ir.push_str("declare i64 @qi_list_int_get(i64, i64)\n");
        ir.push_str("declare i64 @qi_list_int_set(i64, i64, i64)\n");
        ir.push_str("declare i64 @qi_list_int_size(i64)\n");
        ir.push_str("declare i64 @qi_list_int_pop(i64)\n");
        ir.push_str("declare i64 @qi_list_int_clear(i64)\n");
        ir.push_str("declare i64 @qi_list_int_remove(i64, i64)\n");
        ir.push_str("declare i64 @qi_list_int_insert(i64, i64, i64)\n");
        ir.push_str("declare i64 @qi_list_int_contains(i64, i64)\n");
        ir.push_str("declare i64 @qi_list_int_index_of(i64, i64)\n");
        ir.push_str("declare i64 @qi_list_float_create()\n");
        ir.push_str("declare i64 @qi_list_float_push(i64, double)\n");
        ir.push_str("declare double @qi_list_float_get(i64, i64)\n");
        ir.push_str("declare i64 @qi_list_float_size(i64)\n");
        ir.push_str("declare i64 @qi_list_string_create()\n");
        ir.push_str("declare i64 @qi_list_string_push(i64, ptr)\n");
        ir.push_str("declare ptr @qi_list_string_get(i64, i64)\n");
        ir.push_str("declare i64 @qi_list_string_size(i64)\n");
        ir.push_str("declare i64 @qi_list_ptr_create()\n");
        ir.push_str("declare i64 @qi_list_ptr_push(i64, ptr)\n");
        ir.push_str("declare ptr @qi_list_ptr_get(i64, i64)\n");
        ir.push_str("declare i64 @qi_list_ptr_set(i64, i64, ptr)\n");
        ir.push_str("declare i64 @qi_list_ptr_size(i64)\n");
        ir.push_str("declare i64 @qi_list_free(i64)\n");
        ir.push_str("\n");

        ir.push_str("; HashMap functions\n");
        ir.push_str("declare i64 @qi_hashmap_int_create()\n");
        ir.push_str("declare i64 @qi_hashmap_int_set(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_hashmap_int_get(i64, ptr)\n");
        ir.push_str("declare i64 @qi_hashmap_int_contains(i64, ptr)\n");
        ir.push_str("declare i64 @qi_hashmap_int_remove(i64, ptr)\n");
        ir.push_str("declare i64 @qi_hashmap_int_size(i64)\n");
        ir.push_str("declare i64 @qi_hashmap_int_clear(i64)\n");
        ir.push_str("declare i64 @qi_hashmap_float_create()\n");
        ir.push_str("declare i64 @qi_hashmap_float_set(i64, ptr, double)\n");
        ir.push_str("declare double @qi_hashmap_float_get(i64, ptr)\n");
        ir.push_str("declare i64 @qi_hashmap_float_size(i64)\n");
        ir.push_str("declare i64 @qi_hashmap_string_create()\n");
        ir.push_str("declare i64 @qi_hashmap_string_set(i64, ptr, ptr)\n");
        ir.push_str("declare ptr @qi_hashmap_string_get(i64, ptr)\n");
        ir.push_str("declare i64 @qi_hashmap_string_size(i64)\n");
        ir.push_str("declare i64 @qi_hashmap_free(i64)\n");
        ir.push_str("\n");

        ir.push_str("; DateTime functions\n");
        // 时间获取
        ir.push_str("declare i64 @qi_datetime_now()\n");
        ir.push_str("declare i64 @qi_datetime_now_millis()\n");
        ir.push_str("declare i64 @qi_datetime_now_micros()\n");
        ir.push_str("declare i64 @qi_datetime_now_nanos()\n");
        ir.push_str("declare i64 @qi_datetime_now_local()\n");
        // 格式化
        ir.push_str("declare ptr @qi_datetime_format(i64, ptr)\n");
        ir.push_str("declare ptr @qi_datetime_format_local(i64, ptr)\n");
        ir.push_str("declare i64 @qi_datetime_parse(ptr, ptr)\n");
        // 时间组件提取
        ir.push_str("declare i64 @qi_datetime_year(i64)\n");
        ir.push_str("declare i64 @qi_datetime_month(i64)\n");
        ir.push_str("declare i64 @qi_datetime_day(i64)\n");
        ir.push_str("declare i64 @qi_datetime_hour(i64)\n");
        ir.push_str("declare i64 @qi_datetime_minute(i64)\n");
        ir.push_str("declare i64 @qi_datetime_second(i64)\n");
        ir.push_str("declare i64 @qi_datetime_millisecond(i64)\n");
        ir.push_str("declare i64 @qi_datetime_weekday(i64)\n");
        ir.push_str("declare i64 @qi_datetime_quarter(i64)\n");
        ir.push_str("declare i64 @qi_datetime_day_of_year(i64)\n");
        ir.push_str("declare i64 @qi_datetime_week_of_year(i64)\n");
        // 时间运算
        ir.push_str("declare i64 @qi_datetime_add_seconds(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_add_minutes(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_add_hours(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_add_days(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_add_weeks(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_add_months(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_add_years(i64, i64)\n");
        // 时间差
        ir.push_str("declare i64 @qi_datetime_diff_seconds(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_diff_minutes(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_diff_hours(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_diff_days(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_diff_weeks(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_diff_months(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_diff_years(i64, i64)\n");
        // 时间边界
        ir.push_str("declare i64 @qi_datetime_start_of_day(i64)\n");
        ir.push_str("declare i64 @qi_datetime_end_of_day(i64)\n");
        ir.push_str("declare i64 @qi_datetime_start_of_week(i64)\n");
        ir.push_str("declare i64 @qi_datetime_end_of_week(i64)\n");
        ir.push_str("declare i64 @qi_datetime_start_of_month(i64)\n");
        ir.push_str("declare i64 @qi_datetime_end_of_month(i64)\n");
        ir.push_str("declare i64 @qi_datetime_start_of_year(i64)\n");
        ir.push_str("declare i64 @qi_datetime_end_of_year(i64)\n");
        ir.push_str("declare i64 @qi_datetime_start_of_quarter(i64)\n");
        ir.push_str("declare i64 @qi_datetime_end_of_quarter(i64)\n");
        // 时间比较
        ir.push_str("declare i64 @qi_datetime_is_between(i64, i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_today(i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_this_week(i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_this_month(i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_this_year(i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_same_day(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_same_month(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_same_year(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_before(i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_after(i64, i64)\n");
        // 时间构造
        ir.push_str("declare i64 @qi_datetime_from_ymd(i64, i64, i64)\n");
        ir.push_str("declare i64 @qi_datetime_from_ymdhms(i64, i64, i64, i64, i64, i64)\n");
        // 日期验证
        ir.push_str("declare i64 @qi_datetime_is_leap_year(i64)\n");
        ir.push_str("declare i64 @qi_datetime_days_in_month(i64, i64)\n");
        // 工作日/周末
        ir.push_str("declare i64 @qi_datetime_is_weekend(i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_weekday(i64)\n");
        ir.push_str("declare i64 @qi_datetime_is_business_day(i64)\n");
        ir.push_str("declare i64 @qi_datetime_next_business_day(i64)\n");
        ir.push_str("declare i64 @qi_datetime_prev_business_day(i64)\n");
        // 单位转换
        ir.push_str("declare i64 @qi_datetime_seconds_to_millis(i64)\n");
        ir.push_str("declare i64 @qi_datetime_millis_to_seconds(i64)\n");
        ir.push_str("declare i64 @qi_datetime_seconds_to_micros(i64)\n");
        ir.push_str("declare i64 @qi_datetime_micros_to_seconds(i64)\n");
        // 睡眠函数
        ir.push_str("declare void @qi_datetime_sleep_seconds(i64)\n");
        ir.push_str("declare void @qi_datetime_sleep_millis(i64)\n");
        ir.push_str("declare void @qi_datetime_sleep_micros(i64)\n");
        // 内存管理
        ir.push_str("declare void @qi_datetime_free_string(ptr)\n");
        ir.push_str("\n");

        ir.push_str("; Print functions\n");
        ir.push_str("declare i32 @qi_runtime_print(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_println(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_print_int(i64)\n");
        ir.push_str("declare i32 @qi_runtime_println_int(i64)\n");
        ir.push_str("declare i32 @qi_runtime_print_float(double)\n");
        ir.push_str("declare i32 @qi_runtime_println_float(double)\n");
        ir.push_str("declare i32 @qi_runtime_print_bool(i32)\n");
        ir.push_str("declare i32 @qi_runtime_println_bool(i32)\n");
        ir.push_str("declare i32 @qi_runtime_println_str_int(ptr, i64)\n");
        ir.push_str("declare i32 @qi_runtime_println_str_float(ptr, double)\n");
        ir.push_str("declare i32 @qi_runtime_println_str_str(ptr, ptr)\n");
        ir.push_str("\n");

        ir.push_str("; Memory management\n");
        ir.push_str("declare ptr @qi_runtime_alloc(i64)\n");
        ir.push_str("declare i32 @qi_runtime_dealloc(ptr, i64)\n");
        ir.push_str("declare i64 @qi_runtime_gc_should_collect()\n");
        ir.push_str("declare void @qi_runtime_gc_collect()\n");
        ir.push_str("declare i64 @qi_runtime_gc_add_root(ptr)\n");
        ir.push_str("declare i64 @qi_runtime_gc_remove_root(ptr)\n");
        ir.push_str("declare i64 @qi_runtime_gc_add_reference(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_runtime_gc_clear_references(ptr)\n");
        ir.push_str("\n");

        ir.push_str("; String operations\n");
        ir.push_str("declare i64 @qi_runtime_string_length(ptr)\n");
        ir.push_str("declare ptr @qi_runtime_string_concat(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_runtime_string_slice(ptr, i64, i64)\n");
        ir.push_str("declare i32 @qi_runtime_string_compare(ptr, ptr)\n");
        ir.push_str("declare void @qi_runtime_free_string(ptr)\n");
        ir.push_str("\n");

        ir.push_str("; String module functions (标准库.字符串)\n");
        ir.push_str("declare i64 @qi_string_find(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_string_find_from(ptr, ptr, i64)\n");
        ir.push_str("declare ptr @qi_string_substring(ptr, i64, i64)\n");
        ir.push_str("declare ptr @qi_string_substring_from(ptr, i64)\n");
        ir.push_str("declare i64 @qi_string_byte_length(ptr)\n");
        ir.push_str("declare i64 @qi_string_char_count(ptr)\n");
        ir.push_str("declare i64 @qi_string_split(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_string_replace(ptr, ptr, ptr)\n");
        ir.push_str("declare ptr @qi_string_trim(ptr)\n");
        ir.push_str("declare ptr @qi_string_to_upper(ptr)\n");
        ir.push_str("declare ptr @qi_string_to_lower(ptr)\n");
        ir.push_str("declare i64 @qi_string_contains(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_string_starts_with(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_string_ends_with(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_string_equals(ptr, ptr)\n");
        ir.push_str("; Note: qi_string_free already declared in future operations\n");
        ir.push_str("\n");

        ir.push_str("; Math operations\n");
        ir.push_str("declare double @qi_runtime_math_sqrt(double)\n");
        ir.push_str("declare double @qi_runtime_math_pow(double, double)\n");
        ir.push_str("declare double @qi_runtime_math_sin(double)\n");
        ir.push_str("declare double @qi_runtime_math_cos(double)\n");
        ir.push_str("declare double @qi_runtime_math_tan(double)\n");
        ir.push_str("declare i64 @qi_runtime_math_abs_int(i64)\n");
        ir.push_str("declare double @qi_runtime_math_abs_float(double)\n");
        ir.push_str("declare double @qi_runtime_math_floor(double)\n");
        ir.push_str("declare double @qi_runtime_math_ceil(double)\n");
        ir.push_str("declare double @qi_runtime_math_round(double)\n");
        ir.push_str("\n");

        ir.push_str("; File I/O operations\n");
        ir.push_str("declare i64 @qi_runtime_file_open(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_runtime_file_read(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_runtime_file_write(i64, ptr, i64)\n");
        ir.push_str("declare i32 @qi_runtime_file_close(i64)\n");
        ir.push_str("declare ptr @qi_runtime_file_read_string(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_file_write_string(ptr, ptr)\n");
        ir.push_str("\n");

        ir.push_str("; Array operations\n");
        ir.push_str("declare ptr @qi_runtime_array_create(i64, i64)\n");
        ir.push_str("declare i64 @qi_runtime_array_length(ptr)\n");
        ir.push_str("\n");

        ir.push_str("; Type conversions\n");
        ir.push_str("declare ptr @qi_runtime_int_to_string(i64)\n");
        ir.push_str("declare ptr @qi_runtime_float_to_string(double)\n");
        ir.push_str("declare i64 @qi_runtime_string_to_int(ptr)\n");
        ir.push_str("declare double @qi_runtime_string_to_float(ptr)\n");
        ir.push_str("declare double @qi_runtime_int_to_float(i64)\n");
        ir.push_str("declare i64 @qi_runtime_float_to_int(double)\n");
        ir.push_str("\n");

        // Add external function declarations (for backward compatibility)
        ir.push_str("declare i32 @printf(ptr, ...)\n");
        // qi_runtime_string_concat is already declared above in string operations section\n

        // Add external function declarations from imported modules
        // Only declare functions that haven't been declared above
        let already_declared = std::collections::HashSet::from([
            "qi_channel_create",
            "qi_channel_send_int",
            "qi_channel_receive_int",
            "qi_channel_close",
            "qi_waitgroup_create",
            "qi_waitgroup_add",
            "qi_waitgroup_wait",
            "qi_waitgroup_done",
            "qi_mutex_create",
            "qi_mutex_lock",
            "qi_mutex_unlock",
            "qi_mutex_trylock",
            "qi_get_time_ms",
            "qi_set_timeout",
            "qi_check_timeout",
            "qi_timer_create",
            "qi_timer_expired",
            "qi_timer_stop",
            "e5_88_9b_e5_bb_ba_e9_80_9a_e9_81_93",
            "e5_8f_91_e9_80_81_int",
            "e6_a5_a5_e6_8e_af_int",
            "e5_85_b3_e9_97_ad_e9_80_9a_e9_81_93",
            "e5_88_9b_e5_bb_ba_e7_ad_89_e5_be_85_e7_bb_84",
            "e6_8b_89_e5_a0_80_e7_ad_89_e5_be_85",
            "e7_ad_89_e5_be_85",
            "e5_ae_8c_e6_88_90",
            "e5_88_9b_e5_bb_ba_e4_ba_92_e6_96_a5_e9_94_81",
            "e5_8a_a0_e9_94_81",
            "e8_a3_a3_e9_94_81",
            "e5_b0_9d_e8_af_95_e5_8a_a0_e9_94_81",
            "e8_b7_a5_e5_8f_96_e9_97_b4_e9_97_b4",
            "e8_ae_bd_e7_ba_ae_e8_b6_85_e6_97_b6",
            "e6_8f_a5_e6_9f_a5_e8_b6_85_e6_97_b6",
            "e5_88_9b_e5_bb_ba_e5_b0_a8_e6_97_b6_e5_99_a8",
            "e9_87_8d_e8_af_95_e6_93_8d_e4_bd_9c",
            // Future type functions
            "qi_future_ready_i64",
            "qi_future_await_i64",
            "qi_future_ready_f64",
            "qi_future_await_f64",
            "qi_future_ready_bool",
            "qi_future_await_bool",
            "qi_future_ready_string",
            "qi_future_await_string",
            "qi_future_ready_ptr",
            "qi_future_await_ptr",
            "qi_future_failed",
            "qi_future_is_completed",
            "qi_future_free",
            "qi_string_free",
            // Memory allocation
            "malloc",
            "free",
            "strlen",
            // String module functions (declared in emit_llvm_ir)
            "qi_string_length",
            "qi_string_concat",
            "qi_string_substring",
            "qi_string_contains",
            "qi_string_starts_with",
            "qi_string_ends_with",
            "qi_string_find",
            "qi_string_find_from",
            "qi_string_replace",
            "qi_string_trim",
            "qi_string_to_upper",
            "qi_string_to_lower",
            "qi_string_byte_length",
            "qi_string_char_count",
            "qi_string_split",
            "qi_string_compare",
            "qi_string_equals",
            "qi_string_substring_from",
            "qi_runtime_string_concat",
            "qi_runtime_alloc",
            "qi_runtime_dealloc",
            "qi_runtime_gc_should_collect",
            "qi_runtime_gc_collect",
            "qi_runtime_gc_add_root",
            "qi_runtime_gc_remove_root",
            "qi_runtime_gc_add_reference",
            "qi_runtime_gc_clear_references",
            // Conversion runtime functions (declared in emit_llvm_ir)
            "qi_runtime_int_to_string",
            "qi_runtime_float_to_string",
            "qi_runtime_string_to_int",
            "qi_runtime_string_to_float",
            "qi_runtime_int_to_float",
            "qi_runtime_float_to_int",
            // Print runtime functions (declared in emit_llvm_ir)
            "qi_runtime_print_int",
            "qi_runtime_println_int",
            "qi_runtime_print_float",
            "qi_runtime_println_float",
            "qi_runtime_print_string",
            "qi_runtime_println_string",
            "qi_runtime_print_bool",
            "qi_runtime_println_bool",
            // JSON module functions (declared in emit_llvm_ir)
            "qi_json_encode",
            "qi_json_decode",
            "qi_json_create_object",
            "qi_json_create_array",
            "qi_json_set_string",
            "qi_json_set_int",
            "qi_json_set_float",
            "qi_json_set_bool",
            "qi_json_set_object",
            "qi_json_set_array",
            "qi_json_get_string",
            "qi_json_get_int",
            "qi_json_get_float",
            "qi_json_get_bool",
            "qi_json_get_object",
            "qi_json_get_array",
            "qi_json_array_push_string",
            "qi_json_array_push_int",
            "qi_json_array_push_float",
            "qi_json_array_push_bool",
            "qi_json_array_push_object",
            "qi_json_array_get_string",
            "qi_json_array_get_int",
            "qi_json_array_get_float",
            "qi_json_array_get_bool",
            "qi_json_array_get_object",
            "qi_json_array_size",
            "qi_json_array_remove",
            "qi_json_array_clear",
            "qi_json_object_keys",
            "qi_json_object_size",
            "qi_json_object_has_key",
            "qi_json_object_remove",
            "qi_json_object_clear",
            "qi_json_to_string",
            "qi_json_pretty_string",
            "qi_json_clone",
            "qi_json_equals",
            "qi_json_free",
            "qi_json_release_string",
            // List module functions (declared in emit_llvm_ir)
            "qi_list_int_create",
            "qi_list_int_push",
            "qi_list_int_get",
            "qi_list_int_set",
            "qi_list_int_size",
            "qi_list_int_pop",
            "qi_list_int_clear",
            "qi_list_int_remove",
            "qi_list_int_insert",
            "qi_list_int_contains",
            "qi_list_int_index_of",
            "qi_list_float_create",
            "qi_list_float_push",
            "qi_list_float_get",
            "qi_list_float_size",
            "qi_list_string_create",
            "qi_list_string_push",
            "qi_list_string_get",
            "qi_list_string_size",
            "qi_list_ptr_create",
            "qi_list_ptr_push",
            "qi_list_ptr_get",
            "qi_list_ptr_set",
            "qi_list_ptr_size",
            "qi_list_free",
            // Hashmap module functions (declared in emit_llvm_ir)
            "qi_hashmap_int_create",
            "qi_hashmap_int_set",
            "qi_hashmap_int_get",
            "qi_hashmap_int_contains",
            "qi_hashmap_int_remove",
            "qi_hashmap_int_size",
            "qi_hashmap_int_clear",
            "qi_hashmap_float_create",
            "qi_hashmap_float_set",
            "qi_hashmap_float_get",
            "qi_hashmap_float_size",
            "qi_hashmap_string_create",
            "qi_hashmap_string_set",
            "qi_hashmap_string_get",
            "qi_hashmap_string_size",
            "qi_hashmap_free",
            // Random module functions (declared in emit_llvm_ir)
            "qi_random_int",
            "qi_random_float",
            "qi_random_bool",
            "qi_random_string",
            "qi_random_uuid",
            // DateTime sleep / time functions (declared in emit_llvm_ir)
            "qi_datetime_sleep_millis",
            "qi_datetime_sleep_seconds",
            "qi_datetime_sleep_micros",
            "qi_datetime_now_millis",
            // Web runtime helper (declared in emit_llvm_ir)
            "qi_web_call_handler_safe",
            "qi_web_safe_process_request",
            "qi_web_panic_for_test",
            // TLS module functions (declared in emit_llvm_ir)
            "qi_tls_create_config",
            "qi_tls_free_config",
            "qi_tls_listen",
            "qi_tls_accept",
            "qi_tls_read_string",
            "qi_tls_write_string",
            "qi_tls_close",
            "qi_tls_server_close",
            "qi_tls_free_string",
            // HTTP/2 server (declared in emit_llvm_ir)
            "qi_h2_serve",
            // Bytes module (declared in emit_llvm_ir)
            "qi_bytes_create",
            "qi_bytes_with_capacity",
            "qi_bytes_from_string",
            "qi_bytes_to_string",
            "qi_bytes_length",
            "qi_bytes_get",
            "qi_bytes_set",
            "qi_bytes_push",
            "qi_bytes_push_string",
            "qi_bytes_extend",
            "qi_bytes_slice",
            "qi_bytes_compare",
            "qi_bytes_find",
            "qi_bytes_to_hex",
            "qi_bytes_from_hex",
            "qi_bytes_to_base64",
            "qi_bytes_from_base64",
            "qi_bytes_free",
            "qi_bytes_free_string",
            // Closure runtime (declared in emit_llvm_ir)
            "qi_closure_create",
            "qi_closure_get_fn",
            "qi_closure_get_int",
            "qi_closure_get_ptr",
            "qi_closure_set_int",
            "qi_closure_set_ptr",
            // Exception runtime (declared in emit_llvm_ir)
            "qi_exc_alloc_frame",
            "setjmp",
            "qi_exc_pop",
            "qi_exc_throw",
            "qi_exc_message",
            "qi_exc_clear",
            "qi_exc_free_message",
            // Signal module (declared in emit_llvm_ir)
            "qi_signal_install_shutdown",
            "qi_signal_should_shutdown",
            "qi_signal_reset",
            "qi_network_tcp_listener_set_nonblocking",
            "qi_network_tcp_read_bytes",
            "qi_network_tcp_write_bytes",
            // Multipart module
            "qi_multipart_parse",
            "qi_multipart_extract_boundary",
            "qi_multipart_count",
            "qi_multipart_name",
            "qi_multipart_filename",
            "qi_multipart_content_type",
            "qi_multipart_body",
            "qi_multipart_free",
            // Sync primitives — 互斥锁 + 原子整数 (declared in emit_llvm_ir)
            "qi_sync_mutex_create",
            "qi_sync_mutex_lock",
            "qi_sync_mutex_unlock",
            "qi_sync_mutex_trylock",
            "qi_sync_mutex_destroy",
            "qi_sync_atomic_create",
            "qi_sync_atomic_load",
            "qi_sync_atomic_store",
            "qi_sync_atomic_add",
            "qi_sync_atomic_cas",
            "qi_sync_atomic_destroy",
            // Subprocess functions (declared in emit_llvm_ir)
            "qi_subprocess_spawn",
            "qi_subprocess_write_line",
            "qi_subprocess_read_line",
            "qi_subprocess_read_line_timeout",
            "qi_subprocess_is_alive",
            "qi_subprocess_terminate",
            "qi_subprocess_free_string",
        ]);

        if !self.external_functions.is_empty() {
            ir.push_str("; External function declarations from imported modules\n");
            for (func_name, (param_types, return_type)) in &self.external_functions {
                // Skip functions that are already declared or defined in this module
                if !already_declared.contains(func_name.as_str())
                    && !self.function_param_types.contains_key(func_name)
                {
                    let params_str = param_types
                        .iter()
                        .enumerate()
                        .map(|(i, ty)| format!("{} %{}", ty, i))
                        .collect::<Vec<_>>()
                        .join(", ");
                    ir.push_str(&format!(
                        "declare {} @{}({})\n",
                        return_type, func_name, params_str
                    ));
                }
            }
            ir.push_str("\n");
        }

        // Add string constants first
        for instruction in &string_constants {
            match instruction {
                IrInstruction::字符串常量 { name } => {
                    ir.push_str(&format!("{}\n", name));
                }
                _ => {}
            }
        }

        if !string_constants.is_empty() {
            ir.push('\n');
        }

        // Process all instructions in order
        let all_instructions = &other_instructions;

        // Check if there's already a main function being generated
        let has_main_function = other_instructions
            .iter()
            .any(|instruction| match instruction {
                IrInstruction::标签 { name } => {
                    name.contains("@main") || name.contains("define.*@main")
                }
                _ => false,
            });

        // For now, disable main function wrapper completely
        // All functions should be properly generated by the AST to IR conversion
        let should_create_main = false;

        // Helper to get zero value by type
        fn zero_for_ty(ty: &str) -> &'static str {
            match ty {
                "i1" => "0",
                "i8" => "0",
                "i32" => "0",
                "i64" => "0",
                "double" => "0.0",
                "ptr" => "null",
                _ => "0",
            }
        }

        // Process all instructions in order
        for instruction in all_instructions {
            match instruction {
                IrInstruction::全局变量声明 {
                    name,
                    type_name,
                    initializer,
                    is_constant,
                } => {
                    let linkage = if *is_constant { "constant" } else { "global" };
                    let align = self.get_type_alignment(type_name);
                    let init_val = initializer
                        .as_deref()
                        .unwrap_or_else(|| zero_for_ty(type_name));

                    // For global variables, the name already includes @ prefix
                    ir.push_str(&format!(
                        "{} = {} {} {}, align {}\n",
                        name, linkage, type_name, init_val, align
                    ));
                }
                IrInstruction::分配 { dest, type_name } => {
                    let mangled_type = self.mangle_type_name(type_name);

                    // Smart allocation: use heap for large types, stack for small types
                    if self.is_small_type(type_name) {
                        // Small type: use stack allocation (original behavior)
                        ir.push_str(&format!(
                            "  {} = alloca {}, align {}\n",
                            dest,
                            mangled_type,
                            self.get_type_alignment(type_name)
                        ));
                    } else {
                        // Large or complex type: could use heap allocation
                        // For now, keep stack allocation for compatibility, but this is where
                        // we could switch to heap for structs, arrays, etc.
                        ir.push_str(&format!(
                            "  {} = alloca {}, align {}\n",
                            dest,
                            mangled_type,
                            self.get_type_alignment(type_name)
                        ));

                        // Future enhancement: detect large structs and use heap
                        // let type_size = self.estimate_type_size(type_name);
                        // if type_size > 1024 { /* use heap */ }
                    }
                }
                IrInstruction::存储 {
                    target,
                    value,
                    value_type,
                } => {
                    // Determine target type by looking up the target variable
                    let target_var_name = target.trim_start_matches('%').trim_start_matches('_');
                    let target_type = self
                        .variable_types
                        .get(target_var_name)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "i64".to_string());

                    // Determine the type based on the value_type if provided, otherwise infer
                    let inferred_type = if let Some(vt) = value_type {
                        vt.to_string()
                    } else if value.starts_with('@') || value.contains("getelementptr") {
                        "ptr".to_string()
                    } else if value.contains('.') {
                        "double".to_string()
                    } else if value.starts_with('%') {
                        // Look up the type from variable_types HashMap
                        let var_name = value.trim_start_matches('%');
                        self.variable_types
                            .get(var_name)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "i64".to_string())
                    } else if value == "0" || value == "1" {
                        if target_type == "i1" || target_type == "i32" || target_type == "i64" {
                            target_type.clone()
                        } else {
                            "i64".to_string()
                        }
                    } else if value.parse::<i64>().is_ok() {
                        "i64".to_string()
                    } else {
                        // Default to i64 for unknown values
                        "i64".to_string()
                    };

                    // If types don't match, insert conversion (i32 -> i64)
                    let value_to_store = if inferred_type == "i32" && target_type == "i64" {
                        let ext_temp = self.generate_temp();
                        ir.push_str(&format!("{} = sext i32 {} to i64\n", ext_temp, value));
                        ext_temp
                    } else if inferred_type == "i1" && target_type == "i64" {
                        let ext_temp = self.generate_temp();
                        ir.push_str(&format!("{} = zext i1 {} to i64\n", ext_temp, value));
                        ext_temp
                    } else if inferred_type == "i1" && target_type == "i32" {
                        let ext_temp = self.generate_temp();
                        ir.push_str(&format!("{} = zext i1 {} to i32\n", ext_temp, value));
                        ext_temp
                    } else if inferred_type == "i64" && target_type == "i32" {
                        let ext_temp = self.generate_temp();
                        ir.push_str(&format!("{} = trunc i64 {} to i32\n", ext_temp, value));
                        ext_temp
                    } else {
                        value.to_string()
                    };

                    let final_type = if inferred_type == "i32" && target_type == "i64" {
                        "i64".to_string()
                    } else if inferred_type == "i1" && target_type == "i64" {
                        "i64".to_string()
                    } else if inferred_type == "i1" && target_type == "i32" {
                        "i32".to_string()
                    } else if inferred_type == "i64" && target_type == "i32" {
                        "i32".to_string()
                    } else {
                        inferred_type.clone()
                    };

                    ir.push_str(&format!(
                        "store {} {}, ptr {}\n",
                        final_type, value_to_store, target
                    ));

                    // Update target variable type to match what was stored
                    let tgt_var = target.trim_start_matches('%');
                    self.variable_types
                        .insert(tgt_var.to_string(), final_type.clone());
                    // Propagate struct type info from value to target
                    if final_type == "ptr" {
                        let val_var = value.trim_start_matches('%');
                        if let Some(st) = self.variable_struct_types.get(val_var).cloned() {
                            self.variable_struct_types.insert(tgt_var.to_string(), st);
                        }
                    }
                }
                IrInstruction::整数常量 { dest, value } => {
                    ir.push_str(&format!("{} = add i64 0, {}\n", dest, value));
                }
                IrInstruction::布尔常量 { dest, value } => {
                    // Standard boolean constant generation
                    ir.push_str(&format!("{} = add i1 0, {}\n", dest, value));
                }
                IrInstruction::浮点数常量 { dest, value } => {
                    ir.push_str(&format!("{} = fadd double 0.0, {}\n", dest, value));
                }
                IrInstruction::加载 {
                    dest,
                    source,
                    load_type,
                } => {
                    // Use explicit load type if provided, otherwise infer
                    let inferred_type: String = if let Some(ref lt) = load_type {
                        lt.clone()
                    } else if source.starts_with('@') && source.contains(".str") {
                        "ptr".to_string()
                    } else if source.starts_with('@') {
                        // Other globals: look up the declared type
                        let var_name = source.trim_start_matches('@');
                        self.variable_types
                            .get(var_name)
                            .cloned()
                            .unwrap_or_else(|| "i64".to_string())
                    } else if source.starts_with('%') {
                        let var_name = source.trim_start_matches('%');
                        self.variable_types
                            .get(var_name)
                            .cloned()
                            .unwrap_or_else(|| "i64".to_string())
                    } else {
                        "i64".to_string()
                    };
                    ir.push_str(&format!(
                        "{} = load {}, ptr {}\n",
                        dest, inferred_type, source
                    ));
                    // Update dest type in variable_types so subsequent uses have the correct type
                    let dest_var = dest.trim_start_matches('%');
                    self.variable_types
                        .insert(dest_var.to_string(), inferred_type.clone());
                    // Propagate struct type from source to dest when loading a ptr
                    if inferred_type == "ptr" {
                        let source_var = source.trim_start_matches('%');
                        if let Some(st) = self.variable_struct_types.get(source_var).cloned() {
                            self.variable_struct_types.insert(dest_var.to_string(), st);
                        }
                    }
                }
                IrInstruction::二元操作 {
                    dest,
                    left,
                    operator,
                    right,
                    operand_type,
                } => {
                    // Use the operand_type that was determined when creating the instruction
                    let is_float =
                        operand_type.contains("double") || operand_type.contains("float");
                    let (op_str, return_type) = if is_float {
                        match operator {
                            crate::parser::ast::BinaryOperator::加 => ("fadd", "double"),
                            crate::parser::ast::BinaryOperator::减 => ("fsub", "double"),
                            crate::parser::ast::BinaryOperator::乘 => ("fmul", "double"),
                            crate::parser::ast::BinaryOperator::除 => ("fdiv", "double"),
                            crate::parser::ast::BinaryOperator::取余 => ("frem", "double"),
                            crate::parser::ast::BinaryOperator::等于 => ("fcmp oeq", "i1"),
                            crate::parser::ast::BinaryOperator::不等于 => ("fcmp one", "i1"),
                            crate::parser::ast::BinaryOperator::大于 => ("fcmp ogt", "i1"),
                            crate::parser::ast::BinaryOperator::小于 => ("fcmp olt", "i1"),
                            crate::parser::ast::BinaryOperator::大于等于 => ("fcmp oge", "i1"),
                            crate::parser::ast::BinaryOperator::小于等于 => ("fcmp ole", "i1"),
                            crate::parser::ast::BinaryOperator::与 => ("and", "i1"),
                            crate::parser::ast::BinaryOperator::或 => ("or", "i1"),
                        }
                    } else {
                        match operator {
                            crate::parser::ast::BinaryOperator::加 => {
                                ("add", operand_type.as_str())
                            }
                            crate::parser::ast::BinaryOperator::减 => {
                                ("sub", operand_type.as_str())
                            }
                            crate::parser::ast::BinaryOperator::乘 => {
                                ("mul", operand_type.as_str())
                            }
                            crate::parser::ast::BinaryOperator::除 => {
                                ("sdiv", operand_type.as_str())
                            }
                            crate::parser::ast::BinaryOperator::取余 => {
                                ("srem", operand_type.as_str())
                            }
                            crate::parser::ast::BinaryOperator::等于 => ("icmp eq", "i1"),
                            crate::parser::ast::BinaryOperator::不等于 => ("icmp ne", "i1"),
                            crate::parser::ast::BinaryOperator::大于 => ("icmp sgt", "i1"),
                            crate::parser::ast::BinaryOperator::小于 => ("icmp slt", "i1"),
                            crate::parser::ast::BinaryOperator::大于等于 => ("icmp sge", "i1"),
                            crate::parser::ast::BinaryOperator::小于等于 => ("icmp sle", "i1"),
                            crate::parser::ast::BinaryOperator::与 => ("and", "i1"),
                            crate::parser::ast::BinaryOperator::或 => ("or", "i1"),
                        }
                    };

                    // Helper function to convert integer literals to float format
                    let normalize_operand = |operand: &str, is_float_op: bool| -> String {
                        if is_float_op && !operand.starts_with('%') && !operand.starts_with('@') {
                            // It's a literal value in a float operation
                            if let Ok(_int_val) = operand.parse::<i64>() {
                                // It's an integer literal - convert to float
                                format!("{}.0", operand)
                            } else {
                                // Already a float or variable
                                operand.to_string()
                            }
                        } else {
                            operand.to_string()
                        }
                    };

                    // Normalize operands if this is a float operation
                    let normalized_left = normalize_operand(&left, is_float);
                    let normalized_right = normalize_operand(&right, is_float);

                    // For comparison operations (icmp, fcmp), use the operand_type from the instruction
                    // For arithmetic operations, use return_type
                    let type_for_instruction =
                        if op_str.starts_with("icmp") || op_str.starts_with("fcmp") {
                            operand_type.as_str()
                        } else {
                            return_type
                        };

                    ir.push_str(&format!(
                        "{} = {} {} {}, {}\n",
                        dest, op_str, type_for_instruction, normalized_left, normalized_right
                    ));
                }
                IrInstruction::函数调用 {
                    dest,
                    callee,
                    arguments,
                } => {
                    if callee == "printf" && !arguments.is_empty() {
                        // Handle printf calls - arguments are now in "type:value" format
                        let mut processed_args = Vec::new();

                        for (i, arg) in arguments.iter().enumerate() {
                            // Check if argument has "type:value" format (from new typed_args approach)
                            if arg.contains(':') {
                                let parts: Vec<&str> = arg.splitn(2, ':').collect();
                                if parts.len() == 2 {
                                    let arg_type = parts[0];
                                    let arg_value = parts[1];

                                    if i == 0 {
                                        // Format string
                                        processed_args.push(format!("ptr noundef {}", arg_value));
                                    } else {
                                        // Regular argument with embedded type
                                        processed_args.push(format!("{} {}", arg_type, arg_value));
                                    }
                                    continue;
                                }
                            }

                            // Fall back to old logic for arguments without type prefix
                            if i == 0 {
                                // First argument is always format string
                                processed_args.push(format!("ptr noundef {}", arg));
                            } else if arg.starts_with('@') {
                                // String constant - pass as ptr
                                processed_args.push(format!("ptr {}", arg));
                            } else if arg.starts_with('%') {
                                // Variable or temporary - determine type from tracking
                                let var_name = arg.trim_start_matches('%');
                                let vty = self
                                    .variable_types
                                    .get(var_name)
                                    .or_else(|| {
                                        self.variable_types.get(&format!("param_{}", var_name))
                                    })
                                    .map(|s| s.as_str())
                                    .unwrap_or("i64");
                                let llvm_ty = match vty {
                                    "ptr" => "ptr",
                                    "double" => "double",
                                    _ => "i64",
                                };
                                processed_args.push(format!("{} {}", llvm_ty, arg));
                            } else {
                                // Literal values - pass as-is with appropriate type
                                if arg.parse::<i64>().is_ok() {
                                    processed_args.push(format!("i64 {}", arg));
                                } else if arg.parse::<f64>().is_ok() {
                                    processed_args.push(format!("double {}", arg));
                                } else {
                                    processed_args.push(arg.clone());
                                }
                            }
                        }

                        let args_str = processed_args.join(", ");
                        match dest {
                            Some(dest_var) => {
                                ir.push_str(&format!(
                                    "{} = call i32 (ptr, ...) @{}({})\n",
                                    dest_var, callee, args_str
                                ));
                            }
                            None => {
                                ir.push_str(&format!(
                                    "call i32 (ptr, ...) @{}({})\n",
                                    callee, args_str
                                ));
                            }
                        }
                    } else {
                        // Regular function call - determine argument types and return type
                        let mut typed_args = Vec::new();

                        // Get the expected parameter types from function signature if available
                        let mangled_callee = self.mangle_function_name(&callee);
                        let expected_param_types = if let Some(param_types) =
                            self.function_param_types.get(&callee as &str)
                        {
                            Some(param_types.clone())
                        } else if let Some(param_types) =
                            self.function_param_types.get(&mangled_callee as &str)
                        {
                            Some(param_types.clone())
                        } else if let Some((param_types, _)) =
                            self.external_functions.get(&callee as &str)
                        {
                            Some(param_types.clone())
                        } else {
                            None
                        };

                        for (i, arg) in arguments.iter().enumerate() {
                            if arg.starts_with('@') {
                                // String constant
                                typed_args.push(format!("ptr {}", arg));
                            } else if arg.starts_with('%') {
                                // Variable or temporary - look up in variable_types HashMap
                                let arg_var_name = arg.trim_start_matches('%');
                                // When variable_types doesn't have an entry but variable_struct_types does,
                                // the variable is a struct pointer — use "ptr" instead of i64 default
                                let current_arg_type = self
                                    .variable_types
                                    .get(arg_var_name)
                                    .map(|s| s.as_str())
                                    .unwrap_or_else(|| {
                                        if self.variable_struct_types.contains_key(arg_var_name) {
                                            "ptr"
                                        } else {
                                            "i64"
                                        }
                                    });

                                // Determine expected type for this parameter
                                let expected_type =
                                    if let Some(ref param_types) = expected_param_types {
                                        if i < param_types.len() {
                                            &param_types[i]
                                        } else {
                                            "i64" // Default to i64 if no expected type available
                                        }
                                    } else {
                                        // Fallback: determine type based on specific function signatures
                                        if callee == "qi_future_ready_bool" {
                                            "i32"
                                        } else if callee == "qi_future_ready_f64" {
                                            "double"
                                        } else if callee == "qi_future_ready_ptr"
                                            || callee == "qi_future_await_ptr"
                                        {
                                            "ptr"
                                        } else if callee == "qi_runtime_print_int"
                                            || callee == "qi_runtime_println_int"
                                        {
                                            "i64"
                                        } else if callee == "qi_runtime_print_float"
                                            || callee == "qi_runtime_println_float"
                                            || callee == "qi_runtime_float_to_string"
                                        {
                                            "double"
                                        } else if callee == "qi_runtime_print_bool"
                                            || callee == "qi_runtime_println_bool"
                                        {
                                            "i32"
                                        } else if callee == "qi_list_ptr_push" {
                                            if i == 0 {
                                                "i64"
                                            } else {
                                                "ptr"
                                            }
                                        } else if callee == "qi_list_ptr_set" {
                                            if i == 2 {
                                                "ptr"
                                            } else {
                                                "i64"
                                            }
                                        } else if callee == "qi_list_ptr_get"
                                            || callee == "qi_list_ptr_size"
                                        {
                                            "i64"
                                        } else if callee == "qi_list_string_push" {
                                            if i == 0 {
                                                "i64"
                                            } else {
                                                "ptr"
                                            }
                                        } else if callee == "qi_list_string_get"
                                            || callee == "qi_list_string_size"
                                        {
                                            "i64"
                                        } else if callee.starts_with("qi_cli_") {
                                            match callee.as_str() {
                                                "qi_cli_create_app"
                                                | "qi_cli_create_arg"
                                                | "qi_cli_create_subcommand" => "ptr",
                                                "qi_cli_set_version"
                                                | "qi_cli_set_author"
                                                | "qi_cli_set_about"
                                                | "qi_cli_set_long_about"
                                                | "qi_cli_set_override_usage"
                                                | "qi_cli_set_after_help"
                                                | "qi_cli_arg_set_short"
                                                | "qi_cli_arg_set_long"
                                                | "qi_cli_arg_set_help"
                                                | "qi_cli_arg_set_default"
                                                | "qi_cli_arg_set_env"
                                                | "qi_cli_app_add_alias"
                                                | "qi_cli_get_value"
                                                | "qi_cli_get_flag"
                                                | "qi_cli_has_value"
                                                | "qi_cli_has_subcommand"
                                                | "qi_cli_get_subcommand" => {
                                                    if i == 0 {
                                                        "i64"
                                                    } else {
                                                        "ptr"
                                                    }
                                                }
                                                _ => "i64",
                                            }
                                        // Network TCP string functions - must come before generic read_string check
                                        } else if callee == "qi_network_tcp_read_string" {
                                            // qi_network_tcp_read_string(i64, i64) - handle, buffer_size
                                            "i64"
                                        } else if callee == "qi_network_tcp_write_string" {
                                            // qi_network_tcp_write_string(i64, ptr) - handle, data
                                            if i == 0 {
                                                "i64"
                                            } else {
                                                "ptr"
                                            }
                                        } else if callee.contains("concat")
                                            || callee.contains("read_string")
                                            || callee.contains("file")
                                        {
                                            "ptr"
                                        } else if callee == "qi_runtime_waitgroup_add" {
                                            // waitgroup_add(ptr, i32)
                                            if i == 0 {
                                                "ptr"
                                            } else {
                                                "i32"
                                            }
                                        } else if callee == "qi_runtime_waitgroup_create"
                                            || callee == "qi_runtime_waitgroup_wait"
                                            || callee == "qi_runtime_waitgroup_done"
                                            || callee == "qi_runtime_mutex_create"
                                            || callee == "qi_runtime_mutex_lock"
                                            || callee == "qi_runtime_mutex_unlock"
                                            || callee == "qi_runtime_mutex_trylock"
                                        {
                                            // All these take ptr as first parameter
                                            "ptr"
                                        } else if callee == "qi_runtime_channel_send" {
                                            // channel_send(ptr, i64)
                                            if i == 0 {
                                                "ptr"
                                            } else {
                                                "i64"
                                            }
                                        } else if callee == "qi_runtime_channel_receive"
                                            || callee == "qi_runtime_channel_close"
                                        {
                                            // channel operations take ptr
                                            "ptr"
                                        } else if callee == "qi_runtime_create_channel" {
                                            // create_channel(i64) -> ptr
                                            "i64"
                                        } else if callee == "qi_http_server_create" {
                                            // qi_http_server_create(ptr, i64) - host as string, port as integer
                                            if i == 0 {
                                                "ptr"
                                            } else {
                                                "i64"
                                            }
                                        } else if callee == "qi_http_server_handle_request" {
                                            // qi_http_server_handle_request(i64, ptr, i64) - handle, body, status_code
                                            if i == 0 {
                                                "i64"
                                            } else if i == 1 {
                                                "ptr"
                                            } else {
                                                "i64"
                                            }
                                        } else if callee == "qi_http_server_accept"
                                            || callee == "qi_http_server_close"
                                        {
                                            // Both take i64 server handle
                                            "i64"
                                        // WebSocket functions
                                        } else if callee == "qi_websocket_connect" {
                                            // qi_websocket_connect(ptr) - URL as string
                                            "ptr"
                                        } else if callee == "qi_websocket_accept" {
                                            // qi_websocket_accept(ptr, i16) - host, port
                                            if i == 0 {
                                                "ptr"
                                            } else {
                                                "i64"
                                            }
                                        } else if callee == "qi_websocket_send_text" {
                                            // qi_websocket_send_text(i64, ptr) - handle, message
                                            if i == 0 {
                                                "i64"
                                            } else {
                                                "ptr"
                                            }
                                        } else if callee == "qi_websocket_recv_text"
                                            || callee == "qi_websocket_is_connected"
                                            || callee == "qi_websocket_ping"
                                        {
                                            // These take i64 handle
                                            "i64"
                                        } else if callee == "qi_websocket_send_binary" {
                                            // qi_websocket_send_binary(i64, ptr, i64) - handle, data, length
                                            if i == 0 {
                                                "i64"
                                            } else if i == 1 {
                                                "ptr"
                                            } else {
                                                "i64"
                                            }
                                        } else if callee == "qi_websocket_close" {
                                            // qi_websocket_close(i64, i16, ptr) - handle, code, reason
                                            if i == 0 {
                                                "i64"
                                            } else if i == 2 {
                                                "ptr"
                                            } else {
                                                "i64"
                                            }
                                        } else if callee == "qi_websocket_is_upgrade_request"
                                            || callee == "qi_websocket_get_client_key"
                                            || callee == "qi_websocket_create_upgrade_response"
                                        {
                                            // These take ptr (string)
                                            "ptr"
                                        } else if callee == "qi_websocket_register_tcp"
                                            || callee == "qi_websocket_unregister"
                                        {
                                            // These take i64 parameters
                                            "i64"
                                        } else {
                                            current_arg_type // Use the variable's actual type
                                        }
                                    };

                                // Special handling for boolean arguments: if we expect a boolean but get a variable,
                                // ensure the variable is actually a boolean type
                                let final_expected_type = if expected_type == "i1"
                                    && current_arg_type != "i1"
                                {
                                    // Check if this variable was originally a boolean literal
                                    // Look for any indication that this should be boolean
                                    if let Some(temp_val) = self.variable_types.get(arg_var_name) {
                                        if temp_val == "i1" {
                                            "i1"
                                        } else {
                                            expected_type // Keep original expectation
                                        }
                                    } else {
                                        expected_type // Keep original expectation
                                    }
                                } else {
                                    expected_type
                                };

                                // Special handling for boolean arguments to ensure correct value
                                if current_arg_type == "i1" && final_expected_type == "i1" {
                                    // Boolean argument to boolean parameter - ensure value is preserved
                                    // Check if we have stored boolean value information from literal processing
                                    let mut fixed_arg = None;

                                    // Check for bool_ prefix (stores the actual boolean value)
                                    if let Some(bool_val) =
                                        self.variable_types.get(&format!("bool_{}", arg_var_name))
                                    {
                                        if bool_val == "bool_1" {
                                            // This should be true - fix it
                                            let true_temp = format!("%true_fix_{}", arg_var_name);
                                            ir.push_str(&format!("{} = add i1 0, 1\n", true_temp));
                                            fixed_arg = Some(format!("i1 {}", true_temp));
                                        } else if bool_val == "bool_0" {
                                            // This should be false - ensure it's correct
                                            let false_temp = format!("%false_fix_{}", arg_var_name);
                                            ir.push_str(&format!("{} = add i1 0, 0\n", false_temp));
                                            fixed_arg = Some(format!("i1 {}", false_temp));
                                        }
                                    }

                                    // Check for raw_ prefix (stores raw numeric value)
                                    if fixed_arg.is_none() {
                                        if let Some(raw_val) = self
                                            .variable_types
                                            .get(&format!("raw_{}", arg_var_name))
                                        {
                                            if raw_val == "1" {
                                                // Raw value is 1, should be true
                                                let true_temp =
                                                    format!("%true_raw_{}", arg_var_name);
                                                ir.push_str(&format!(
                                                    "{} = add i1 0, 1\n",
                                                    true_temp
                                                ));
                                                fixed_arg = Some(format!("i1 {}", true_temp));
                                            } else if raw_val == "0" {
                                                // Raw value is 0, should be false
                                                let false_temp =
                                                    format!("%false_raw_{}", arg_var_name);
                                                ir.push_str(&format!(
                                                    "{} = add i1 0, 0\n",
                                                    false_temp
                                                ));
                                                fixed_arg = Some(format!("i1 {}", false_temp));
                                            }
                                        }
                                    }

                                    // Use the fixed argument if we created one, otherwise use original
                                    if let Some(fixed) = fixed_arg {
                                        typed_args.push(fixed);
                                    } else {
                                        // Use original argument as fallback
                                        typed_args.push(format!("i1 {}", arg));
                                    }
                                } else if current_arg_type != final_expected_type {
                                    // Only do conversions if we're confident about the current type
                                    // If current_arg_type was defaulted to i64 (not found in variable_types),
                                    // trust the expected type instead
                                    let was_defaulted =
                                        !self.variable_types.contains_key(arg_var_name);
                                    let conv_temp = format!("%conv{}_{}", arg_var_name, i);

                                    if was_defaulted && current_arg_type == "i64" {
                                        // Type was defaulted, trust expected type directly
                                        typed_args.push(format!("{} {}", final_expected_type, arg));
                                    } else if current_arg_type == "i64"
                                        && final_expected_type == "double"
                                    {
                                        // Convert i64 to double
                                        ir.push_str(&format!(
                                            "{} = sitofp i64 {} to double\n",
                                            conv_temp, arg
                                        ));
                                        typed_args.push(format!("double {}", conv_temp));
                                    } else if current_arg_type == "double"
                                        && final_expected_type == "i64"
                                    {
                                        // Convert double to i64
                                        ir.push_str(&format!(
                                            "{} = fptosi double {} to i64\n",
                                            conv_temp, arg
                                        ));
                                        typed_args.push(format!("i64 {}", conv_temp));
                                    } else if current_arg_type == "i1"
                                        && final_expected_type == "i64"
                                    {
                                        // Convert bool to i64
                                        ir.push_str(&format!(
                                            "{} = zext i1 {} to i64\n",
                                            conv_temp, arg
                                        ));
                                        typed_args.push(format!("i64 {}", conv_temp));
                                    } else if current_arg_type == "i1"
                                        && final_expected_type == "i32"
                                    {
                                        // Convert bool to i32 (for qi_runtime_println_bool and similar functions)
                                        ir.push_str(&format!(
                                            "{} = zext i1 {} to i32\n",
                                            conv_temp, arg
                                        ));
                                        typed_args.push(format!("i32 {}", conv_temp));
                                    } else {
                                        // No conversion needed or unsupported conversion
                                        typed_args.push(format!("{} {}", final_expected_type, arg));
                                    }
                                } else {
                                    // No conversion needed
                                    typed_args.push(format!("{} {}", final_expected_type, arg));
                                }
                            } else {
                                // Literal values - determine expected type
                                let expected_type =
                                    if let Some(ref param_types) = expected_param_types {
                                        if i < param_types.len() {
                                            &param_types[i]
                                        } else {
                                            "i64"
                                        }
                                    } else {
                                        // Fallback: infer from literal content
                                        if arg.contains('.') {
                                            "double"
                                        } else if arg == "真" || arg == "假" {
                                            "i1"
                                        } else {
                                            "i64"
                                        }
                                    };

                                // Format literal according to expected type
                                match expected_type {
                                    "double" => {
                                        if arg.contains('.') {
                                            typed_args.push(format!("double {}", arg));
                                        } else {
                                            typed_args.push(format!("double {}.0", arg));
                                        }
                                    }
                                    "i1" => {
                                        let bool_val = if arg == "真" { "1" } else { "0" };
                                        typed_args.push(format!("i1 {}", bool_val));
                                    }
                                    _ => {
                                        typed_args.push(format!("i64 {}", arg));
                                    }
                                }
                            }
                        }

                        let args_str = typed_args.join(", ");

                        // For print functions, map to typed versions based on argument types
                        let final_callee =
                            if callee == "qi_runtime_print" || callee == "qi_runtime_println" {
                                // Check the first argument type to determine which print function to use
                                if let Some(first_arg) = typed_args.first() {
                                    if first_arg.contains("double") {
                                        format!("{}_float", callee)
                                    } else if first_arg.contains("i1") {
                                        format!("{}_bool", callee)
                                    } else if first_arg.contains("ptr") {
                                        // For string arguments, use the base print function (no suffix)
                                        callee.clone()
                                    } else {
                                        format!("{}_int", callee)
                                    }
                                } else {
                                    callee.clone()
                                }
                            } else {
                                callee.clone()
                            };

                        // Determine return type based on function name
                        let ret_type = if callee.starts_with("qi_future_") {
                            // Future type functions - check external_functions first
                            if let Some((_, ret_ty)) = self.external_functions.get(&callee as &str)
                            {
                                ret_ty.as_str()
                            } else {
                                // Fallback for future functions
                                match callee.as_str() {
                                    "qi_future_ready_i64" | "qi_future_failed" => "ptr",
                                    "qi_future_await_i64" => "i64",
                                    "qi_future_is_completed" => "i32",
                                    "qi_future_free" => "void",
                                    _ => "ptr",
                                }
                            }
                        } else if callee.starts_with("qi_runtime_") {
                            if let Some((_, ret_ty)) = self.external_functions.get(&callee as &str)
                            {
                                ret_ty.as_str()
                            // Create functions return ptr - MUST BE FIRST
                            } else if callee == "qi_runtime_create_channel"
                                || callee == "qi_runtime_waitgroup_create"
                                || callee == "qi_runtime_mutex_create"
                                || callee == "qi_runtime_rwlock_create"
                                || callee == "qi_runtime_condvar_create"
                                || callee == "qi_runtime_once_create"
                                || callee == "qi_runtime_timer_create"
                            {
                                "ptr"
                            // Math functions return double
                            } else if callee.contains("math_sqrt")
                                || callee.contains("math_pow")
                                || callee.contains("math_sin")
                                || callee.contains("math_cos")
                                || callee.contains("math_tan")
                                || callee.contains("math_floor")
                                || callee.contains("math_ceil")
                                || callee.contains("math_round")
                                || callee.contains("math_abs_float")
                                || callee.contains("int_to_float")
                                || callee.contains("string_to_float")
                            {
                                "double"
                            // String length and string_to_int return i64, not ptr
                            } else if callee.contains("string_length")
                                || callee.contains("string_to_int")
                            {
                                "i64"
                            // String comparison returns i32 (strcmp-style: 0 if equal)
                            } else if callee.contains("string_compare") {
                                "i32"
                            // String functions return ptr
                            } else if callee.contains("string")
                                || callee.contains("concat")
                                || callee.contains("read_string")
                                || callee.contains("int_to_string")
                                || callee.contains("float_to_string")
                            {
                                "ptr"
                            // Channel receive returns i64 (the actual value)
                            } else if callee == "qi_runtime_channel_receive" {
                                "i64"
                            // Timer functions that return i64
                            } else if callee == "qi_runtime_set_timeout"
                                || callee == "qi_runtime_timer_expired"
                                || callee == "qi_runtime_timer_stop"
                                || callee == "qi_runtime_get_time_ms"
                            {
                                "i64"
                            } else if callee == "qi_runtime_gc_add_root"
                                || callee == "qi_runtime_gc_remove_root"
                                || callee == "qi_runtime_gc_add_reference"
                                || callee == "qi_runtime_gc_clear_references"
                            {
                                "i64"
                            // Synchronization create functions return ptr
                            } else if callee == "qi_runtime_waitgroup_create"
                                || callee == "qi_runtime_mutex_create"
                                || callee == "qi_runtime_rwlock_create"
                                || callee == "qi_runtime_condvar_create"
                                || callee == "qi_runtime_once_create"
                                || callee == "qi_runtime_timer_create"
                            {
                                "ptr"
                            // All other synchronization functions return i32 (status)
                            } else if callee.contains("waitgroup")
                                || callee.contains("mutex")
                                || callee.contains("rwlock")
                                || callee.contains("condvar")
                                || callee.contains("once")
                                || callee == "qi_runtime_check_timeout"
                            {
                                "i32"
                            // Channel functions - create returns ptr, others return i32 or i64
                            } else if callee == "qi_runtime_create_channel" {
                                "ptr"
                            } else if callee.contains("channel") {
                                "i32"
                            // Integer math functions return i64
                            } else if callee.contains("math_abs_int")
                                || callee.contains("float_to_int")
                                || callee.contains("string_to_int")
                                || callee.contains("array_length")
                            {
                                "i64"
                            } else {
                                "i32"
                            }
                        } else if callee == "qi_runtime_string_concat" {
                            "ptr"
                        // Crypto functions return ptr (string)
                        } else if callee.starts_with("qi_crypto_")
                            && callee != "qi_crypto_free_string"
                        {
                            "ptr"
                        // IO functions - check return type based on function name
                        } else if callee.starts_with("qi_io_") {
                            match callee.as_str() {
                                "qi_io_read_file" => "ptr", // 读取文件 returns string
                                "qi_io_file_size" | "qi_io_write_file" | "qi_io_append_file"
                                | "qi_io_delete_file" | "qi_io_create_file"
                                | "qi_io_file_exists" | "qi_io_create_dir" | "qi_io_delete_dir" => {
                                    "i64"
                                } // These return i64
                                "qi_io_free_string" => "void", // Cleanup function
                                _ => "i64",                 // Default for unknown IO functions
                            }
                        // JSON functions - check return type based on function name
                        } else if callee.starts_with("qi_json_") {
                            match callee.as_str() {
                                "qi_json_encode"
                                | "qi_json_get_string"
                                | "qi_json_array_get_string"
                                | "qi_json_to_string"
                                | "qi_json_to_string_pretty"
                                | "qi_json_from_pairs"
                                | "qi_json_from_text" => "ptr", // Return strings
                                "qi_json_decode"
                                | "qi_json_create_object"
                                | "qi_json_create_array"
                                | "qi_json_get_object"
                                | "qi_json_get_array"
                                | "qi_json_array_get_object"
                                | "qi_json_array_get_array"
                                | "qi_json_array_length"
                                | "qi_json_has_key"
                                | "qi_json_free" => "i64", // Return handles/counts
                                "qi_json_get_float" | "qi_json_array_get_float" => "double", // Return floats
                                "qi_json_set_string"
                                | "qi_json_set_int"
                                | "qi_json_set_float"
                                | "qi_json_set_bool"
                                | "qi_json_set_object"
                                | "qi_json_set_array"
                                | "qi_json_array_push_string"
                                | "qi_json_array_push_int"
                                | "qi_json_array_push_float"
                                | "qi_json_array_push_bool"
                                | "qi_json_array_push_object"
                                | "qi_json_array_push_array"
                                | "qi_json_array_get_bool"
                                | "qi_json_get_bool" => "i64", // Return 0/1 as i64
                                "qi_json_free_string" => "void", // Cleanup function
                                _ => "i64", // Default for unknown JSON functions
                            }
                        // Network functions - check return type based on function name
                        } else if callee.starts_with("qi_network_") {
                            match callee.as_str() {
                                "qi_network_resolve_host"
                                | "qi_network_get_local_ip"
                                | "qi_network_udp_recv_string"
                                | "qi_network_tcp_read_string" => "ptr", // Return strings
                                // Async TCP returns *mut Future
                                "qi_network_async_tcp_connect"
                                | "qi_network_async_tcp_read_bytes"
                                | "qi_network_async_tcp_write_bytes"
                                | "qi_network_async_tcp_listen"
                                | "qi_network_async_tcp_accept" => "ptr",
                                "qi_network_tcp_connect"
                                | "qi_network_tcp_read"
                                | "qi_network_tcp_write"
                                | "qi_network_tcp_write_string"
                                | "qi_network_tcp_close"
                                | "qi_network_tcp_flush"
                                | "qi_network_tcp_bytes_read"
                                | "qi_network_tcp_bytes_written"
                                | "qi_network_port_available"
                                | "qi_network_tcp_listen"
                                | "qi_network_tcp_accept"
                                | "qi_network_tcp_server_close"
                                | "qi_network_async_tcp_close"
                                | "qi_network_async_tcp_listener_close"
                                | "qi_network_udp_bind"
                                | "qi_network_udp_send_string"
                                | "qi_network_udp_send_to"
                                | "qi_network_udp_recv_from"
                                | "qi_network_udp_close"
                                | "qi_network_udp_set_timeout"
                                | "qi_network_udp_set_broadcast" => "i64", // Return i64
                                "qi_network_free_string" => "void", // Cleanup function
                                _ => "i64", // Default for unknown network functions
                            }
                        // HTTP functions - check return type based on function name
                        } else if callee.starts_with("qi_http_") {
                            match callee.as_str() {
                                "qi_http_get"
                                | "qi_http_post"
                                | "qi_http_put"
                                | "qi_http_delete"
                                | "qi_http_head"
                                | "qi_http_patch"
                                | "qi_http_options"
                                | "qi_http_request"
                                | "qi_http_request_execute"
                                | "qi_http_server_handle_request"
                                | "qi_http_server_accept" => "ptr", // Return response strings
                                "qi_http_init"
                                | "qi_http_request_create"
                                | "qi_http_request_set_header"
                                | "qi_http_request_set_body"
                                | "qi_http_request_set_timeout"
                                | "qi_http_get_status"
                                | "qi_http_server_create"
                                | "qi_http_server_close" => "i64", // Return i64
                                "qi_http_free_string" => "void", // Cleanup function
                                _ => "i64", // Default for unknown HTTP functions
                            }
                        // WebSocket functions - check return type based on function name
                        } else if callee.starts_with("qi_websocket_") {
                            match callee.as_str() {
                                "qi_websocket_recv_text"
                                | "qi_websocket_get_client_key"
                                | "qi_websocket_create_upgrade_response" => "ptr", // Return strings
                                "qi_websocket_connect"
                                | "qi_websocket_accept"
                                | "qi_websocket_send_text"
                                | "qi_websocket_send_binary"
                                | "qi_websocket_ping"
                                | "qi_websocket_close"
                                | "qi_websocket_is_connected"
                                | "qi_websocket_is_upgrade_request" => "i64", // Return i64
                                "qi_websocket_free_string" => "void", // Cleanup function
                                _ => "i64", // Default for unknown WebSocket functions
                            }
                        } else if callee.starts_with("qi_llm_") {
                            match callee.as_str() {
                                "qi_llm_chat"
                                | "qi_llm_chat_async"
                                | "qi_llm_stream_next"
                                | "qi_llm_chat_with_tools"
                                | "qi_llm_continue_with_tools"
                                | "qi_llm_get_tool_call_id"
                                | "qi_llm_get_tool_call_name"
                                | "qi_llm_get_tool_call_arguments"
                                | "qi_llm_get_tool_call_id_at"
                                | "qi_llm_get_tool_call_name_at"
                                | "qi_llm_get_tool_call_arguments_at" => "ptr", // Return LLM response string / Future<String>
                                "qi_llm_create_session"
                                | "qi_llm_set_config"
                                | "qi_llm_clear_history"
                                | "qi_llm_get_history_count"
                                | "qi_llm_close_session"
                                | "qi_llm_stream_chat"
                                | "qi_llm_stream_close"
                                | "qi_llm_register_tool"
                                | "qi_llm_clear_tools"
                                | "qi_llm_has_tool_call"
                                | "qi_llm_add_tool_result"
                                | "qi_llm_get_tool_call_count" => "i64", // Return i64
                                "qi_llm_free_string" => "void", // Cleanup function
                                _ => "i64",                     // Default for unknown LLM functions
                            }
                        } else if callee.starts_with("qi_os_") {
                            match callee.as_str() {
                                "qi_os_getenv" | "qi_os_environ" | "qi_os_getcwd"
                                | "qi_os_homedir" | "qi_os_tempdir" | "qi_os_type"
                                | "qi_os_arch" | "qi_os_family" | "qi_os_hostname"
                                | "qi_os_username" | "qi_os_list_dir" => "ptr", // Return strings
                                "qi_os_setenv" | "qi_os_unsetenv" | "qi_os_chdir"
                                | "qi_os_cpu_count" | "qi_os_getpid" | "qi_os_load_env"
                                | "qi_os_is_dir" | "qi_os_is_file" => "i64", // Return i64
                                "qi_os_exit" | "qi_os_free_string" => "void", // No return value
                                _ => "i64", // Default for unknown OS functions
                            }
                        } else if callee.starts_with("qi_cli_") {
                            match callee.as_str() {
                                "qi_cli_get_value" => "ptr",    // Return string
                                "qi_cli_free_string" => "void", // No return value
                                _ => "i64",                     // Most CLI functions return i64
                            }
                        // DateTime functions - check return type based on function name
                        } else if callee.starts_with("qi_datetime_") {
                            match callee.as_str() {
                                "qi_datetime_format" | "qi_datetime_format_local" => "ptr", // Return formatted string
                                "qi_datetime_async_sleep_future" => "ptr", // Returns *mut Future
                                "qi_datetime_free_string" => "void",       // Cleanup function
                                _ => "i64", // Most datetime functions return i64 (timestamps or components, including sleep)
                            }
                        // MCP functions - check return type based on function name
                        } else if callee.starts_with("qi_mcp_") {
                            match callee.as_str() {
                                "qi_mcp_get_server_info"
                                | "qi_mcp_list_tools"
                                | "qi_mcp_list_resources"
                                | "qi_mcp_list_prompts"
                                | "qi_mcp_call_tool"
                                | "qi_mcp_get_prompt"
                                | "qi_mcp_read_resource_text"
                                | "qi_mcp_read_resource_json" => "ptr", // Return string
                                "qi_mcp_create_server" => "i64", // Return server ID
                                "qi_mcp_free_string" => "void",  // Cleanup function
                                // P2 notification functions return i32
                                "qi_mcp_notify_tools_changed"
                                | "qi_mcp_notify_resources_changed"
                                | "qi_mcp_notify_prompts_changed"
                                | "qi_mcp_log_message"
                                | "qi_mcp_notify_progress" => "i32",
                                _ => "i32", // Most MCP functions return i32 status
                            }
                        // MCP Client core functions
                        } else if callee.starts_with("qi_mcpc_") {
                            match callee.as_str() {
                                "qi_mcpc_connect_stdio"
                                | "qi_mcpc_connect_http"
                                | "qi_mcpc_close" => "i64",
                                "qi_mcpc_request" => "ptr", // result JSON string
                                "qi_mcpc_drain_notifications" => "ptr", // notifications JSON array string
                                "qi_mcpc_set_sampling_handler"
                                | "qi_mcpc_set_elicitation_handler"
                                | "qi_mcpc_set_roots" => "i32",
                                "qi_mcpc_free_string" => "void",
                                _ => "i64",
                            }
                        // List functions - check return type based on function name
                        } else if callee.starts_with("qi_list_") {
                            match callee.as_str() {
                                "qi_list_ptr_get" => "ptr",
                                "qi_list_string_get" => "ptr", // Return string
                                "qi_list_free" => "i64",
                                _ => "i64", // Most list functions return i64
                            }
                        // HashMap functions - check return type based on function name
                        } else if callee.starts_with("qi_hashmap_") {
                            match callee.as_str() {
                                "qi_hashmap_string_get" => "ptr", // Return string
                                _ => "i64", // Most hashmap functions return i64 (including qi_hashmap_free)
                            }
                        // Check hex-encoded Chinese function names
                        } else if callee == "e6_b1_82_e5_b9_b3_e6_96_b9_e6_a0_b9" {
                            // 求平方根
                            "double"
                        } else if callee == "e6_b1_82_e7_bb_9d_e5_af_b9_e5_80_bc" {
                            // 求绝对值
                            "i64"
                        } else if callee == "e5ad_97_e7_ac_a6_e9_95_bf" {
                            // 字符串长度
                            "i64"
                        } else {
                            // Check if this is a known async function
                            if self.async_function_types.contains_key(callee) {
                                "ptr"
                            } else if let Some(ret_type) = self.function_return_types.get(callee) {
                                ret_type
                            } else if let Some(ret_type) = self
                                .function_return_types
                                .get(&self.mangle_function_name(&callee) as &str)
                            {
                                ret_type // Use stored return type from function signature
                            } else if let Some((_param_types, ret_type)) =
                                self.external_functions.get(callee)
                            {
                                ret_type.as_str() // Use return type from external function signature
                                                  // If we already know the type from the build_node phase (e.g. module registry lookups),
                                                  // trust that rather than defaulting to i64
                            } else if let Some(dest_var) = dest.as_deref() {
                                let var_name = dest_var.trim_start_matches('%');
                                match self.variable_types.get(var_name).map(|s| s.as_str()) {
                                    Some("ptr") => "ptr",
                                    Some("double") => "double",
                                    Some("i1") => "i1",
                                    Some("i32") => "i32",
                                    Some("i64") | None => "i64",
                                    Some(other) => other,
                                }
                            } else {
                                "i64" // Default to i64
                            }
                        };

                        // Special handling for channel functions
                        if callee == "qi_runtime_channel_receive" {
                            // Channel receive needs special handling to allocate pointer for received value
                            let received_ptr = self.generate_temp();
                            let temp_status = self.generate_temp();
                            let temp_ptr = self.generate_temp();
                            ir.push_str(&format!("{} = alloca ptr, align 8\n", received_ptr));
                            ir.push_str(&format!(
                                "{} = call i32 @{}({}, ptr {})\n",
                                temp_status, callee, typed_args[0], received_ptr
                            ));
                            ir.push_str(&format!(
                                "{} = load ptr, ptr {}\n",
                                temp_ptr, received_ptr
                            ));
                            if let Some(dest_var) = dest {
                                ir.push_str(&format!(
                                    "{} = load i64, ptr {}\n",
                                    dest_var, temp_ptr
                                ));
                            }
                        } else {
                            match dest {
                                Some(dest_var) => {
                                    let callee_ref = if final_callee.starts_with('%') {
                                        final_callee.clone()
                                    } else {
                                        format!("@{}", final_callee)
                                    };
                                    ir.push_str(&format!(
                                        "{} = call {} {}({})\n",
                                        dest_var, ret_type, callee_ref, args_str
                                    ));
                                    // Store the return type for this temporary variable
                                    let var_name = dest_var.trim_start_matches('%');
                                    self.variable_types
                                        .insert(var_name.to_string(), ret_type.to_string());
                                }
                                None => {
                                    let callee_ref = if final_callee.starts_with('%') {
                                        final_callee.clone()
                                    } else {
                                        format!("@{}", final_callee)
                                    };
                                    ir.push_str(&format!(
                                        "call void {}({})\n",
                                        callee_ref, args_str
                                    ));
                                }
                            }
                        }
                    }
                }
                IrInstruction::返回 { value: None } => {
                    // If current function is non-void, emit a typed zero; else ret void
                    if let Some(ref ty) = current_function_ret_ty {
                        if ty != "void" {
                            ir.push_str(&format!("ret {} {}\n", ty, zero_for_ty(ty)));
                        } else {
                            ir.push_str("ret void\n");
                        }
                    } else {
                        ir.push_str("ret void\n");
                    }
                }
                IrInstruction::返回 { value: Some(val) } => {
                    // Use the current function return type if known
                    if let Some(ref ty) = current_function_ret_ty {
                        if ty == "void" {
                            ir.push_str("ret void\n");
                        } else {
                            // Normalize integer literals when the return type is ptr — `ret ptr 0`
                            // is invalid in modern LLVM; use `null` instead.
                            let normalized: String = if ty == "ptr" {
                                if val == "0" || val == "0i64" {
                                    "null".to_string()
                                } else if val.parse::<i64>().is_ok() {
                                    "null".to_string()
                                } else {
                                    val.clone()
                                }
                            } else {
                                val.clone()
                            };
                            if self.verbose {
                                eprintln!("[DEBUG] Generating return: ret {} {}", ty, normalized);
                            }
                            ir.push_str(&format!("ret {} {}\n", ty, normalized));
                        }
                    } else {
                        // Default to i64 if not within a function context
                        if self.verbose {
                            eprintln!("[DEBUG] Generating return (default): ret i64 {}", val);
                        }
                        ir.push_str(&format!("ret i64 {}\n", val));
                    }
                }
                IrInstruction::标签 { name } => {
                    if name.starts_with("define") {
                        // Parse return type from define line, e.g., "define i32 @main(...) {"
                        let tokens: Vec<&str> = name.split_whitespace().collect();
                        if tokens.len() >= 2 {
                            current_function_ret_ty = Some(tokens[1].to_string());
                        } else {
                            current_function_ret_ty = None;
                        }
                        // Parse parameter SSA names + types from the define line and inject
                        // back into variable_types. Codegen clears variable_types between
                        // functions (builder.rs ~3322), but IR emission is a SINGLE pass over
                        // all queued instructions. Without re-injecting params here, typed_args
                        // lookup for a print/etc. arg referencing a param of the CURRENT
                        // function falls back to the i64 default — which misroutes
                        // qi_runtime_print -> qi_runtime_print_int and breaks IR linkage.
                        injected_param_keys.clear();
                        if let (Some(lparen), Some(rparen)) = (name.find('('), name.rfind(')')) {
                            if rparen > lparen {
                                let params_str = &name[lparen + 1..rparen];
                                for part in params_str.split(',') {
                                    let tokens: Vec<&str> = part.split_whitespace().collect();
                                    if tokens.len() >= 2 && tokens[1].starts_with('%') {
                                        let ty = tokens[0].to_string();
                                        let var = tokens[1].trim_start_matches('%').to_string();
                                        self.variable_types.insert(var.clone(), ty);
                                        injected_param_keys.push(var);
                                    }
                                }
                            }
                        }
                        ir.push_str(&format!("{}\n", name));
                    } else if name == "}" {
                        ir.push_str("}\n");
                        // Reset current function return type at function end
                        current_function_ret_ty = None;
                        for k in injected_param_keys.drain(..) {
                            self.variable_types.remove(&k);
                        }
                    } else if name.contains(" = ") {
                        // This is an instruction (like zext, add, etc.), not a label
                        // Output as-is without adding colon, but trim trailing colon if present
                        let clean_name = name.trim_end_matches(':');
                        ir.push_str(&format!("{}\n", clean_name));
                    } else if name.ends_with(':') {
                        ir.push_str(&format!("{}\n", name));
                    } else if name.starts_with('@') {
                        ir.push_str(&format!("{}\n", name));
                    } else {
                        ir.push_str(&format!("{}:\n", name));
                    }
                }
                IrInstruction::跳转 { label } => {
                    ir.push_str(&format!("br label %{}\n", label));
                }
                IrInstruction::条件跳转 {
                    condition,
                    true_label,
                    false_label,
                } => {
                    ir.push_str(&format!(
                        "br i1 {}, label %{}, label %{}\n",
                        condition, true_label, false_label
                    ));
                }
                IrInstruction::数组访问 { dest, array, index } => {
                    if array.starts_with('@') && array.contains(".str") {
                        // String constant access - use bitcast to i8* first, then getelementptr
                        ir.push_str(&format!(
                            "{} = getelementptr i8, i8* {}, i32 {}\n",
                            dest, array, index
                        ));
                    } else {
                        // Regular array access using getelementptr
                        ir.push_str(&format!(
                            "{} = getelementptr [10 x i64], [10 x i64]* {}, i64 0, i64 {}\n",
                            dest, array, index
                        ));
                    }
                }
                IrInstruction::数组分配 {
                    dest,
                    size,
                    element_type,
                } => {
                    // Smart array allocation: small arrays on stack, large arrays on heap
                    let array_size: usize = size.parse().unwrap_or(10);
                    const SMALL_ARRAY_THRESHOLD: usize = 64; // Arrays <= 64 elements use stack

                    // Calculate element size based on type
                    let elem_size = match element_type.as_str() {
                        "double" => 8,
                        "i64" => 8,
                        "i32" => 4,
                        "i16" => 2,
                        "i8" | "i1" => 1,
                        _ => 8, // Default to 8 bytes
                    };

                    if array_size <= SMALL_ARRAY_THRESHOLD {
                        // Small array: stack allocation
                        ir.push_str(&format!(
                            "  {} = alloca [{} x {}], align 8\n",
                            dest, size, element_type
                        ));
                    } else {
                        // Large array: heap allocation with GC check
                        let bytes = array_size * elem_size;
                        let (alloc_ir, ptr) =
                            self.generate_allocation_with_gc_check(bytes, element_type, true);
                        ir.push_str(&alloc_ir);

                        // Record heap allocation for cleanup
                        self.record_allocation(AllocationInfo {
                            ptr: ptr.clone(),
                            size: bytes,
                            type_name: format!("[{} x {}]", size, element_type),
                            scope_level: self.scope_level,
                            is_heap: true,
                        });

                        // Alias the result
                        if ptr != *dest {
                            ir.push_str(&format!(
                                "  {} = bitcast ptr {} to [{} x {}]*\n",
                                dest, ptr, size, element_type
                            ));
                        }
                        ir.push_str(&format!(
                            "  %gc_root_{} = call i64 @qi_runtime_gc_add_root(ptr {})\n",
                            dest.trim_start_matches('%').replace('.', "_"),
                            ptr
                        ));
                    }
                }
                IrInstruction::数组存储 {
                    array,
                    index,
                    value,
                    element_type,
                } => {
                    // Generate unique temp name for address
                    let hash = format!(
                        "{}{}",
                        array.replace("%", "").replace("t", ""),
                        index.replace("%", "")
                    );
                    ir.push_str(&format!(
                        "  %addr_tmp{} = getelementptr [10 x {}], ptr {}, i64 0, i64 {}\n",
                        hash, element_type, array, index
                    ));
                    ir.push_str(&format!(
                        "  store {} {}, ptr %addr_tmp{}\n",
                        element_type, value, hash
                    ));
                }
                IrInstruction::字符串连接 { dest, left, right } => {
                    // Simplified string concatenation using external function
                    ir.push_str(&format!(
                        "{} = call ptr @qi_runtime_string_concat(ptr {}, ptr {})\n",
                        dest, left, right
                    ));
                }
                IrInstruction::异或 { dest, left, right } => {
                    // XOR operation for logical not
                    ir.push_str(&format!("  {} = xor i1 {}, {}\n", dest, left, right));
                }
                IrInstruction::不可达 => {
                    // Unreachable instruction for dead code paths
                    ir.push_str("unreachable\n");
                }
                IrInstruction::类型转换 {
                    dest,
                    value,
                    from_type,
                    to_type,
                    cast_type,
                } => {
                    // Type conversion using appropriate LLVM cast instruction
                    ir.push_str(&format!(
                        "  {} = {} {} {} to {}\n",
                        dest, cast_type, from_type, value, to_type
                    ));
                }
                IrInstruction::字段访问 {
                    dest,
                    object,
                    field,
                    struct_type,
                } => {
                    // Get field index from struct field names
                    let field_index =
                        if let Some(field_names) = self.struct_field_names.get(struct_type) {
                            // Find field index by name
                            field_names.iter().position(|f| f == field).unwrap_or(0)
                        } else {
                            0 // Unknown struct, use 0
                        };

                    let mangled_type = self.mangle_type_name(&format!("{}.type", struct_type));
                    ir.push_str(&format!(
                        "{} = getelementptr {}, ptr {}, i32 0, i32 {}\n",
                        dest, mangled_type, object, field_index
                    ));
                }
                IrInstruction::字符串常量 { .. } => {
                    // String constants are handled separately at the beginning
                }
                IrInstruction::等待表达式 { dest, future } => {
                    // Check if we're awaiting a Future<T> type (ptr) or an async coroutine
                    // For Future<T>, call the appropriate qi_future_await_* based on inner type
                    // For async coroutines, call qi_runtime_await which returns a pointer

                    // Try to determine the type from the future variable
                    let future_var = future.trim_start_matches('%');
                    let is_future_type = self
                        .variable_types
                        .get(future_var)
                        .map(|t| t == "ptr")
                        .unwrap_or(false);

                    if is_future_type {
                        // This is a Future<T> type - determine the inner type and call appropriate await
                        let inner_type = self
                            .future_inner_types
                            .get(future_var)
                            .map(|s| s.as_str())
                            .unwrap_or("i64"); // Default to i64 if not tracked

                        let (await_func, call_return_type, final_type) =
                            if inner_type.starts_with("struct.") {
                                // Struct type - use qi_future_await_ptr
                                ("qi_future_await_ptr", "ptr", "ptr")
                            } else {
                                match inner_type {
                                    "i64" => ("qi_future_await_i64", "i64", "i64"),
                                    "double" => ("qi_future_await_f64", "double", "double"),
                                    "i1" => ("qi_future_await_bool", "i32", "i1"), // bool await returns i32, convert to i1
                                    "ptr" => ("qi_future_await_string", "ptr", "ptr"), // string pointer
                                    _ => ("qi_future_await_i64", "i64", "i64"),        // fallback
                                }
                            };

                        // Call the await function
                        if call_return_type == final_type {
                            // Direct call - no conversion needed
                            ir.push_str(&format!(
                                "{} = call {} @{}(ptr {})\n",
                                dest, call_return_type, await_func, future
                            ));
                        } else {
                            // Need type conversion (bool case: i32 -> i1)
                            let temp_result = self.generate_temp();
                            ir.push_str(&format!(
                                "{} = call {} @{}(ptr {})\n",
                                temp_result, call_return_type, await_func, future
                            ));
                            // Convert i32 to i1 by checking if != 0
                            ir.push_str(&format!(
                                "{} = icmp ne {} {}, 0\n",
                                dest, call_return_type, temp_result
                            ));
                        }

                        // Record the final type of the dest variable for later use
                        let dest_var = dest.trim_start_matches('%');
                        self.variable_types
                            .insert(dest_var.to_string(), final_type.to_string());
                    } else {
                        // This is an async coroutine - call qi_runtime_await
                        ir.push_str(&format!(
                            "{} = call ptr @qi_runtime_await(ptr {})\n",
                            dest, future
                        ));
                    }
                }
                IrInstruction::创建异步任务 {
                    dest,
                    function,
                    arguments,
                } => {
                    // Create async task - pass function pointer and argument count
                    // Note: This is a simplified implementation. In a real async runtime,
                    // we would need to handle argument passing more carefully.
                    ir.push_str(&format!(
                        "{} = call ptr @qi_runtime_create_task(ptr @{}, i64 {})\n",
                        dest,
                        function,
                        arguments.len()
                    ));

                    // Spawn the task to start execution
                    ir.push_str(&format!("call i32 @qi_runtime_spawn_task(ptr {})\n", dest));
                }
                IrInstruction::协程启动 {
                    function,
                    arguments,
                } => {
                    // For functions with arguments, generate a wrapper function and use generic spawn
                    if arguments.is_empty() {
                        // No arguments - use simple spawn
                        let temp1 = self.generate_temp();
                        ir.push_str(&format!("{} = ptrtoint ptr @{} to i64\n", temp1, function));
                        let temp2 = self.generate_temp();
                        ir.push_str(&format!("{} = inttoptr i64 {} to ptr\n", temp2, temp1));
                        ir.push_str(&format!(
                            "call void @qi_runtime_spawn_goroutine(ptr {})\n",
                            temp2
                        ));
                    } else {
                        // Generate wrapper function name
                        let wrapper_name =
                            format!("__goroutine_wrapper_{}_{}", function, self.label_counter);
                        self.label_counter += 1;

                        // Parse argument types and values
                        let mut arg_types = Vec::new();
                        let mut arg_values = Vec::new();
                        for arg in arguments {
                            let (arg_type, arg_value) = if arg.contains(':') {
                                let parts: Vec<&str> = arg.splitn(2, ':').collect();
                                (parts[0].to_string(), parts[1].to_string())
                            } else {
                                ("i64".to_string(), arg.clone())
                            };
                            arg_types.push(arg_type);
                            arg_values.push(arg_value);
                        }

                        // Generate wrapper function definition (to be added at the end)
                        let mut wrapper_def = String::new();
                        wrapper_def
                            .push_str(&format!("define void @{}(ptr %args) {{\n", wrapper_name));

                        // Load each argument from the array and call the target function
                        let mut call_args = Vec::new();
                        for (i, (arg_type, _)) in arg_types.iter().zip(&arg_values).enumerate() {
                            let arg_temp = format!("%arg{}", i);
                            let ptr_temp = format!("%argptr{}", i);

                            // Get pointer to array element: args[i]
                            wrapper_def.push_str(&format!(
                                "  {} = getelementptr i64, ptr %args, i32 {}\n",
                                ptr_temp, i
                            ));

                            // Load the i64 value
                            wrapper_def.push_str(&format!(
                                "  {} = load i64, ptr {}\n",
                                arg_temp, ptr_temp
                            ));

                            // Convert to appropriate type and add to call args
                            if arg_type == "ptr" {
                                let cast_temp = format!("%argcast{}", i);
                                wrapper_def.push_str(&format!(
                                    "  {} = inttoptr i64 {} to ptr\n",
                                    cast_temp, arg_temp
                                ));
                                call_args.push(format!("ptr {}", cast_temp));
                            } else {
                                call_args.push(format!("{} {}", arg_type, arg_temp));
                            }
                        }

                        // Call the actual function
                        wrapper_def.push_str(&format!(
                            "  call void @{}({})\n",
                            function,
                            call_args.join(", ")
                        ));
                        wrapper_def.push_str("  ret void\n");
                        wrapper_def.push_str("}\n");

                        // Store wrapper for later emission
                        self.goroutine_wrappers.push(wrapper_def);

                        // Allocate array for arguments
                        let args_array = format!("%goroutine_args_{}", self.temp_counter);
                        self.temp_counter += 1;
                        ir.push_str(&format!(
                            "{} = alloca [{}  x i64], align 8\n",
                            args_array,
                            arguments.len()
                        ));

                        // Store each argument value into the array
                        for (i, (arg_type, arg_value)) in
                            arg_types.iter().zip(&arg_values).enumerate()
                        {
                            let element_ptr = self.generate_temp();
                            ir.push_str(&format!(
                                "{} = getelementptr [{} x i64], ptr {}, i32 0, i32 {}\n",
                                element_ptr,
                                arguments.len(),
                                args_array,
                                i
                            ));

                            // Convert to i64 if needed
                            if arg_type == "ptr" {
                                let as_int = self.generate_temp();
                                ir.push_str(&format!(
                                    "{} = ptrtoint ptr {} to i64\n",
                                    as_int, arg_value
                                ));
                                ir.push_str(&format!(
                                    "store i64 {}, ptr {}\n",
                                    as_int, element_ptr
                                ));
                            } else {
                                ir.push_str(&format!(
                                    "store i64 {}, ptr {}\n",
                                    arg_value, element_ptr
                                ));
                            }
                        }

                        // Get wrapper function pointer
                        let wrapper_ptr1 = self.generate_temp();
                        ir.push_str(&format!(
                            "{} = ptrtoint ptr @{} to i64\n",
                            wrapper_ptr1, wrapper_name
                        ));
                        let wrapper_ptr2 = self.generate_temp();
                        ir.push_str(&format!(
                            "{} = inttoptr i64 {} to ptr\n",
                            wrapper_ptr2, wrapper_ptr1
                        ));

                        // Call qi_runtime_spawn_goroutine_with_args(wrapper, args, count)
                        ir.push_str(&format!("call void @qi_runtime_spawn_goroutine_with_args(ptr {}, ptr {}, i64 {})\n",
                            wrapper_ptr2, args_array, arguments.len()));
                    }
                }
                IrInstruction::创建通道 {
                    dest,
                    channel_type,
                    buffer_size,
                } => {
                    // Create channel - generate runtime call
                    let size = buffer_size.as_ref().unwrap_or(&"0".to_string()).clone();
                    ir.push_str(&format!(
                        "{} = call ptr @qi_runtime_create_channel(i64 {})\n",
                        dest, size
                    ));
                }
                IrInstruction::通道发送 { channel, value } => {
                    // Send value to channel using runtime
                    // If value is a pointer (from build_node_for_channel), load it first
                    let value_to_send = if value.starts_with('%') {
                        // Get variable type to determine if we need to load
                        let var_name = value.trim_start_matches('%');
                        let var_type = self.variable_types.get(var_name).map(|s| s.as_str());

                        // For now, assume channel values are always i64
                        // TODO: Support other types when type system is enhanced
                        let loaded_temp = self.generate_temp();
                        ir.push_str(&format!("{} = load i64, ptr {}\n", loaded_temp, value));
                        loaded_temp
                    } else {
                        value.clone()
                    };
                    ir.push_str(&format!(
                        "call i32 @qi_runtime_channel_send(ptr {}, i64 {})\n",
                        channel, value_to_send
                    ));
                }
                IrInstruction::通道接收 { dest, channel } => {
                    // Receive value from channel using runtime
                    let received_ptr = self.generate_temp();
                    let status_temp = self.generate_temp();
                    let value_ptr_temp = self.generate_temp();
                    ir.push_str(&format!("{} = alloca ptr, align 8\n", received_ptr));
                    ir.push_str(&format!(
                        "{} = call i32 @qi_runtime_channel_receive(ptr {}, ptr {})\n",
                        status_temp, channel, received_ptr
                    ));
                    ir.push_str(&format!(
                        "{} = load ptr, ptr {}\n",
                        value_ptr_temp, received_ptr
                    ));
                    ir.push_str(&format!("{} = load i64, ptr {}\n", dest, value_ptr_temp));
                }
                IrInstruction::选择语句 {
                    cases,
                    default_case,
                } => {
                    // Generate select statement using runtime
                    ir.push_str("; Select statement - runtime implementation\n");

                    // For now, implement a simple blocking select
                    // TODO: Implement proper non-blocking select with multiple cases
                    ir.push_str("call ptr @qi_runtime_select(ptr null)\n");
                }
            }
        }

        // Emit collected goroutine wrapper functions at the end
        for wrapper in &self.goroutine_wrappers {
            ir.push_str("\n");
            ir.push_str(wrapper);
        }

        Ok(ir)
    }

    /// Check if an operand is a float type parameter or variable
    fn is_float_operand(&self, operand: &str) -> bool {
        // Remove % prefix if present
        let clean_operand = operand.trim_start_matches('%');

        // Check if it's a parameter
        let param_key = format!("param_{}", clean_operand);
        if let Some(param_type) = self.variable_types.get(&param_key) {
            return param_type.contains("double") || param_type.contains("float");
        }

        // Check if it's a regular variable
        if let Some(var_type) = self.variable_types.get(clean_operand) {
            return var_type.contains("double") || var_type.contains("float");
        }

        false
    }

    /// Get the type of an expression for format string interpolation
    fn get_expression_type(&self, expr: &AstNode) -> String {
        match expr {
            AstNode::字面量表达式(literal) => match literal.value {
                crate::parser::ast::LiteralValue::整数(_) => "i64".to_string(),
                crate::parser::ast::LiteralValue::浮点数(_) => "double".to_string(),
                crate::parser::ast::LiteralValue::字符串(_) => "ptr".to_string(),
                crate::parser::ast::LiteralValue::布尔(_) => "i1".to_string(),
                crate::parser::ast::LiteralValue::字符(_) => "i8".to_string(),
            },
            AstNode::标识符表达式(ident) => {
                // Check variable types
                if let Some(var_type) = self.variable_types.get(&ident.name) {
                    var_type.clone()
                } else {
                    "i64".to_string() // Default to integer
                }
            }
            AstNode::二元操作表达式(binary) => {
                // Check if it's a float operation by checking the operands
                let left_is_float = match &*binary.left {
                    AstNode::字面量表达式(lit) => {
                        matches!(&lit.value, crate::parser::ast::LiteralValue::浮点数(_))
                    }
                    AstNode::标识符表达式(ident) => self
                        .variable_types
                        .get(&ident.name)
                        .map(|t| t.contains("double") || t.contains("float"))
                        .unwrap_or(false),
                    _ => false,
                };
                let right_is_float = match &*binary.right {
                    AstNode::字面量表达式(lit) => {
                        matches!(&lit.value, crate::parser::ast::LiteralValue::浮点数(_))
                    }
                    AstNode::标识符表达式(ident) => self
                        .variable_types
                        .get(&ident.name)
                        .map(|t| t.contains("double") || t.contains("float"))
                        .unwrap_or(false),
                    _ => false,
                };
                if left_is_float || right_is_float {
                    "double".to_string()
                } else {
                    "i64".to_string()
                }
            }
            _ => "i64".to_string(), // Default to integer
        }
    }

    /// Check if a statement or block contains a return statement
    fn contains_return(&self, stmts: &[AstNode]) -> bool {
        for stmt in stmts {
            if matches!(stmt, AstNode::返回语句(_)) {
                return true;
            }
            // Check inside blocks
            if let AstNode::块语句(block) = stmt {
                if self.contains_return(&block.statements) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a node contains a return statement
    fn node_contains_return(&self, node: &AstNode) -> bool {
        match node {
            AstNode::返回语句(_) => true,
            AstNode::块语句(block) => self.contains_return(&block.statements),
            _ => false,
        }
    }

    // ===== Memory Management Methods =====

    /// Determine whether to allocate on stack or heap based on the AST node
    fn determine_allocation_target(&self, node: &AstNode) -> AllocationTarget {
        match node {
            // Small basic types -> Stack
            AstNode::变量声明(var) => {
                if let Some(ref type_ann) = var.type_annotation {
                    if self.is_small_type_node(type_ann) {
                        return AllocationTarget::Stack;
                    }
                }
                AllocationTarget::Stack
            }

            // Arrays, strings, structs -> Heap (by default, can be refined)
            AstNode::数组字面量表达式(_) => AllocationTarget::Heap,
            AstNode::字符串连接表达式(_) => AllocationTarget::Heap,
            AstNode::结构体实例化表达式(_) => AllocationTarget::Heap,

            // Default to stack for other cases
            _ => AllocationTarget::Stack,
        }
    }

    /// Check if a TypeNode is considered "small" and suitable for stack allocation
    fn is_small_type_node(&self, type_node: &crate::parser::ast::TypeNode) -> bool {
        use crate::parser::ast::{BasicType, TypeNode};

        match type_node {
            TypeNode::基础类型(basic_type) => matches!(
                basic_type,
                BasicType::整数 | BasicType::长整数 | BasicType::浮点数 | BasicType::布尔
            ),
            _ => false,
        }
    }

    /// Check if a type string is considered "small" and suitable for stack allocation
    fn is_small_type(&self, type_name: &str) -> bool {
        matches!(
            type_name,
            "整数" | "浮点数" | "布尔" | "i64" | "f64" | "i32" | "f32" | "i8" | "i1"
        )
    }

    /// Get size in bytes for a given type
    fn get_type_size(&self, type_name: &str) -> usize {
        match type_name {
            "i64" | "整数" | "f64" | "浮点数" => 8,
            "i32" | "f32" => 4,
            "i8" | "布尔" | "i1" => 1,
            _ => 8, // Default size
        }
    }

    /// Record a memory allocation for lifetime tracking
    fn record_allocation(&mut self, info: AllocationInfo) {
        self.allocations.push(info);
    }

    /// Generate heap allocation IR code
    fn generate_heap_allocation(&mut self, size: usize, type_name: &str) -> String {
        let dest = self.generate_temp();
        let mut ir = String::new();

        // Call qi_runtime_alloc to get raw pointer
        ir.push_str(&format!(
            "  {} = call ptr @qi_runtime_alloc(i64 {})\n",
            dest, size
        ));

        // Optionally bitcast to specific type if needed
        if type_name != "ptr" && type_name != "i8" {
            let typed_ptr = self.generate_temp();
            ir.push_str(&format!(
                "  {} = bitcast ptr {} to {}*\n",
                typed_ptr, dest, type_name
            ));
            ir
        } else {
            ir
        }
    }

    /// Generate heap allocation with GC check for large allocations
    /// Returns tuple of (IR code, result pointer variable name)
    fn generate_allocation_with_gc_check(
        &mut self,
        size: usize,
        type_name: &str,
        check_gc: bool,
    ) -> (String, String) {
        let mut ir = String::new();

        // For large allocations (> 1MB), check if GC should run first
        if check_gc && size > 1024 * 1024 {
            let should_gc = self.generate_temp();
            let need_gc = self.generate_temp();
            let do_gc_label = self.generate_label();
            let skip_gc_label = self.generate_label();

            // Check if GC should collect
            ir.push_str(&format!(
                "  {} = call i64 @qi_runtime_gc_should_collect()\n",
                should_gc
            ));
            ir.push_str(&format!("  {} = icmp ne i64 {}, 0\n", need_gc, should_gc));
            ir.push_str(&format!(
                "  br i1 {}, label %{}, label %{}\n",
                need_gc, do_gc_label, skip_gc_label
            ));

            // GC block
            ir.push_str(&format!("\n{}:\n", do_gc_label));
            ir.push_str("  call void @qi_runtime_gc_collect()\n");
            ir.push_str(&format!("  br label %{}\n", skip_gc_label));

            // Continue with allocation
            ir.push_str(&format!("\n{}:\n", skip_gc_label));
        }

        // Perform allocation
        let alloc_ptr = self.generate_temp();
        ir.push_str(&format!(
            "  {} = call ptr @qi_runtime_alloc(i64 {})\n",
            alloc_ptr, size
        ));

        // Bitcast if needed
        let result_ptr = if type_name != "ptr" && type_name != "i8" {
            let typed_ptr = self.generate_temp();
            ir.push_str(&format!(
                "  {} = bitcast ptr {} to {}*\n",
                typed_ptr, alloc_ptr, type_name
            ));
            typed_ptr
        } else {
            alloc_ptr
        };

        (ir, result_ptr)
    }

    /// Generate stack allocation IR code
    fn generate_stack_allocation(&mut self, type_name: &str) -> String {
        // This generates standard LLVM alloca instruction
        format!("alloca {}, align 8", type_name)
    }

    /// Generate cleanup code for exiting a scope
    fn generate_scope_cleanup(&mut self, scope_level: usize) -> String {
        let mut ir = String::new();

        // Find all heap allocations for this scope
        let allocations_to_free: Vec<_> = self
            .allocations
            .iter()
            .filter(|a| a.scope_level == scope_level && a.is_heap)
            .cloned()
            .collect();

        // Heap objects leave the current root set when scope exits.
        // Actual reclamation is deferred to tracing GC.
        for alloc in &allocations_to_free {
            ir.push_str(&format!(
                "  %gc_unroot_{} = call i64 @qi_runtime_gc_remove_root(ptr {})\n",
                alloc.ptr.trim_start_matches('%').replace('.', "_"),
                alloc.ptr
            ));
        }

        // Remove allocations for this scope
        self.allocations.retain(|a| a.scope_level != scope_level);

        ir
    }

    /// Enter a new scope (increment scope level)
    fn enter_scope(&mut self) {
        self.scope_level += 1;
    }

    /// Exit current scope (decrement scope level and cleanup)
    fn exit_scope(&mut self) -> String {
        let cleanup_ir = self.generate_scope_cleanup(self.scope_level);
        if self.scope_level > 0 {
            self.scope_level -= 1;
        }
        cleanup_ir
    }
}

impl Default for IrBuilder {
    fn default() -> Self {
        Self::new()
    }
}
