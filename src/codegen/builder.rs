//! IR builder for Qi language

use crate::parser::ast::{AstNode, BinaryOperator};
use super::module_registry::{ModuleRegistry, ModuleFunction};

/// IR instruction
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum IrInstruction {
    /// Allocate a variable
    分配 {
        dest: String,
        type_name: String,
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
        load_type: Option<String>,  // Explicit type to load
    },

    /// Binary operation
    二元操作 {
        dest: String,
        left: String,
        operator: BinaryOperator,
        right: String,
        operand_type: String,  // "i64", "double", "i1", etc. - the type of left and right operands
    },

    /// Function call
    函数调用 {
        dest: Option<String>,
        callee: String,
        arguments: Vec<String>,
    },

    /// Return from function
    返回 {
        value: Option<String>,
    },

    /// Jump to label
    跳转 {
        label: String,
    },

    /// Conditional jump
    条件跳转 {
        condition: String,
        true_label: String,
        false_label: String,
    },

    /// String constant
    字符串常量 {
        name: String,
    },

    /// Integer constant
    整数常量 {
        dest: String,
        value: i64,
    },

    /// Boolean constant
    布尔常量 {
        dest: String,
        value: i8,  // Use i8 to represent 0 or 1
    },

    /// Float constant
    浮点数常量 {
        dest: String,
        value: f64,
    },

    /// Label
    标签 {
        name: String,
    },

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
    },

    /// Array store
    数组存储 {
        array: String,
        index: String,
        value: String,
    },

    /// String concatenation
    字符串连接 {
        dest: String,
        left: String,
        right: String,
    },

    /// Field access (getelementptr for struct fields)
    字段访问 {
        dest: String,
        object: String,
        field: String,
        struct_type: String,  // The struct type name (e.g., "点")
    },

    /// Await expression
    等待表达式 {
        dest: String,
        future: String,
    },

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
    通道发送 {
        channel: String,
        value: String,
    },

    /// Receive from channel
    通道接收 {
        dest: String,
        channel: String,
    },

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
    接收,  // Receive
    发送,  // Send
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
    /// Track defined function parameter types (name -> params)
    function_param_types: std::collections::HashMap<String, Vec<String>>,
    /// Track if we're currently inside an async function
    in_async_context: bool,
    /// Track defined functions in current module
    defined_functions: std::collections::HashSet<String>,
    /// Track external function signatures (name -> (params, return_type))
    external_functions: std::collections::HashMap<String, (Vec<String>, String)>,
    /// Track struct definitions (name -> field_types)
    struct_definitions: std::collections::HashMap<String, Vec<String>>,
    /// Track struct field names (struct_name -> field_names)
    struct_field_names: std::collections::HashMap<String, Vec<String>>,
    /// Track variable struct types (variable_name -> struct_type_name)
    variable_struct_types: std::collections::HashMap<String, String>,
    /// Track import aliases (alias -> actual_module_name)
    import_aliases: std::collections::HashMap<String, String>,
    /// Track current module/package name
    current_package_name: Option<String>,
    /// Track loop labels for break/continue (continue_label, break_label)
    loop_stack: Vec<(String, String)>,
    /// Wrapper functions for goroutine spawn (generated at the end)
    goroutine_wrappers: Vec<String>,
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
}

impl IrBuilder {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
            variable_types: std::collections::HashMap::new(),
            future_inner_types: std::collections::HashMap::new(),
            function_future_inner_types: std::collections::HashMap::new(),
            boolean_variables: std::collections::HashSet::new(),
            async_function_types: std::collections::HashMap::new(),
            function_return_types: std::collections::HashMap::new(),
            function_param_types: std::collections::HashMap::new(),
            in_async_context: false,
            defined_functions: std::collections::HashSet::new(),
            external_functions: std::collections::HashMap::new(),
            struct_definitions: std::collections::HashMap::new(),
            struct_field_names: std::collections::HashMap::new(),
            variable_struct_types: std::collections::HashMap::new(),
            import_aliases: std::collections::HashMap::new(),
            current_package_name: None,
            loop_stack: Vec::new(),
            goroutine_wrappers: Vec::new(),
            module_registry: ModuleRegistry::new(),
            imported_modules: std::collections::HashMap::new(),
            allocations: Vec::new(),
            scope_level: 0,
            current_function_name: None,
            current_function_ast_return_type: None,
        }.register_runtime_functions()
    }

    /// Register runtime function signatures
    fn register_runtime_functions(mut self) -> Self {
        // Future type functions - integer
        self.external_functions.insert("qi_future_ready_i64".to_string(), (vec!["i64".to_string()], "ptr".to_string()));
        self.external_functions.insert("qi_future_await_i64".to_string(), (vec!["ptr".to_string()], "i64".to_string()));

        // Future type functions - float
        self.external_functions.insert("qi_future_ready_f64".to_string(), (vec!["double".to_string()], "ptr".to_string()));
        self.external_functions.insert("qi_future_await_f64".to_string(), (vec!["ptr".to_string()], "double".to_string()));

        // Future type functions - boolean
        self.external_functions.insert("qi_future_ready_bool".to_string(), (vec!["i32".to_string()], "ptr".to_string()));
        self.external_functions.insert("qi_future_await_bool".to_string(), (vec!["ptr".to_string()], "i32".to_string()));

        // Future type functions - string
        self.external_functions.insert("qi_future_ready_string".to_string(), (vec!["ptr".to_string(), "i64".to_string()], "ptr".to_string()));
        self.external_functions.insert("qi_future_await_string".to_string(), (vec!["ptr".to_string()], "ptr".to_string()));

        // Future type functions - pointer (for structs)
        self.external_functions.insert("qi_future_ready_ptr".to_string(), (vec!["ptr".to_string()], "ptr".to_string()));
        self.external_functions.insert("qi_future_await_ptr".to_string(), (vec!["ptr".to_string()], "ptr".to_string()));

        // Future type functions - common
        self.external_functions.insert("qi_future_failed".to_string(), (vec!["ptr".to_string(), "i64".to_string()], "ptr".to_string()));
        self.external_functions.insert("qi_future_is_completed".to_string(), (vec!["ptr".to_string()], "i32".to_string()));
        self.external_functions.insert("qi_future_free".to_string(), (vec!["ptr".to_string()], "void".to_string()));
        self.external_functions.insert("qi_string_free".to_string(), (vec!["ptr".to_string()], "void".to_string()));

        // String utility functions
        self.external_functions.insert("strlen".to_string(), (vec!["ptr".to_string()], "i64".to_string()));

        // Memory allocation functions
        self.external_functions.insert("malloc".to_string(), (vec!["i64".to_string()], "ptr".to_string()));
        self.external_functions.insert("free".to_string(), (vec!["ptr".to_string()], "void".to_string()));

        // Other runtime functions can be added here if needed
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

        // First pass: collect all function signatures
        self.collect_function_signatures(ast)?;

        // Second pass: generate code
        self.build_node(ast)?;
        self.emit_llvm_ir()
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
                    "入口" => "main".to_string(),
                    name => {
                        if name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(name)
                        } else {
                            name.to_string()
                        }
                    }
                };

                // Collect parameter types
                let param_types: Vec<String> = func_decl.parameters
                    .iter()
                    .map(|p| self.get_llvm_type(&p.type_annotation))
                    .collect();

                // Determine return type
                let return_type = if func_decl.name == "入口" || func_name == "main" {
                    "i32".to_string()
                } else if let Some(_) = func_decl.return_type {
                    self.get_return_type(&func_decl.return_type)
                } else {
                    // For now, default to void if no return type specified
                    "void".to_string()
                };

                // Store in function_param_types and function_return_types
                self.function_param_types.insert(func_name.clone(), param_types);
                self.function_return_types.insert(func_name.clone(), return_type);

                eprintln!("[DEBUG] Collected signature for {}: {:?} -> {:?}",
                    func_name,
                    self.function_param_types.get(&func_name),
                    self.function_return_types.get(&func_name));
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

    #[allow(dead_code)]
    fn generate_label(&mut self) -> String {
        self.label_counter += 1;
        format!("L{}", self.label_counter)
    }

    /// Get the full function name from a function call expression, including module prefix
    fn get_full_function_name(&self, call_expr: &crate::parser::ast::FunctionCallExpression) -> String {
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
    pub fn set_external_functions(&mut self, funcs: std::collections::HashMap<String, (Vec<String>, String)>) {
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

    /// Process an import statement and register the imported module
    fn process_import(&mut self, import_stmt: &crate::parser::ast::ImportStatement) -> Result<(), String> {
        // Check if this is a relative path (starts with . or ..)
        let is_relative_path = !import_stmt.module_path.is_empty() &&
            (import_stmt.module_path[0] == "." || import_stmt.module_path[0] == "..");

        // For relative paths, we don't use the ModuleRegistry
        // They will be resolved by the compiler's resolve_import_path function
        if is_relative_path {
            // For relative imports, use the last component as the module name
            let module_name = import_stmt.module_path.last()
                .ok_or_else(|| "导入路径为空".to_string())?
                .clone();

            // Use alias if provided, otherwise use the last path component
            let import_key = import_stmt.alias.clone().unwrap_or(module_name.clone());

            // For relative imports, register them with a special marker
            // We'll use the full path joined with / as the "module path"
            let relative_path_key = import_stmt.module_path.join("/");
            self.import_aliases.insert(import_key.clone(), relative_path_key.clone());

            return Ok(());
        }

        // For single-component imports in a package context, treat as intra-package import
        // e.g., when in "数学" package: "导入 最大值;" means importing from the same package
        if import_stmt.module_path.len() == 1 && self.current_package_name.is_some() {
            let submodule_name = &import_stmt.module_path[0];

            // For intra-package imports, we don't need to resolve or register them
            // The functions from the same package are already available
            // Just record the alias if provided
            let import_key = import_stmt.alias.clone().unwrap_or_else(|| submodule_name.clone());

            // For intra-package imports, we use the package name as the module path
            // This allows functions from different files in the same package to be accessible
            if let Some(package_name) = &self.current_package_name {
                self.import_aliases.insert(import_key, package_name.clone());
            }

            return Ok(());
        }

        // Resolve module path from import statement (for standard library and global modules)
        let module_path = self.module_registry.resolve_module_path(&import_stmt.module_path)
            .ok_or_else(|| {
                format!(
                    "模块 '{}' 不存在。可用的模块: {:?}",
                    import_stmt.module_path.join("."),
                    self.module_registry.module_paths()
                )
            })?;

        // Get the last component as the module name
        let module_name = import_stmt.module_path.last()
            .ok_or_else(|| "导入路径为空".to_string())?
            .clone();

        // Use alias if provided, otherwise use the module name
        let import_key = import_stmt.alias.clone().unwrap_or(module_name);

        // Register the import
        self.imported_modules.insert(module_path.clone(), import_key.clone());
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
        let module_path = self.import_aliases.get(module_name)
            .ok_or_else(|| {
                format!(
                    "模块 '{}' 未导入。请先使用 '导入 标准库.{};'",
                    module_name,
                    module_name
                )
            })?;

        // Get the function from the module
        self.module_registry.get_function(module_path, function_name)
            .ok_or_else(|| {
                format!(
                    "模块 '{}' 中不存在函数 '{}'",
                    module_name,
                    function_name
                )
            })
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
    fn mangle_function_name(&self, name: &str) -> String {
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
        use crate::parser::ast::{TypeNode, BasicType};
        match inner_type {
            TypeNode::基础类型(BasicType::整数 | BasicType::长整数 | BasicType::短整数 | BasicType::字节) => {
                "qi_future_ready_i64"
            }
            TypeNode::基础类型(BasicType::浮点数) => {
                "qi_future_ready_f64"
            }
            TypeNode::基础类型(BasicType::布尔) => {
                "qi_future_ready_bool"
            }
            TypeNode::基础类型(BasicType::字符串) => {
                "qi_future_ready_string"
            }
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
        use crate::parser::ast::{TypeNode, BasicType};
        match inner_type {
            TypeNode::基础类型(BasicType::整数 | BasicType::长整数 | BasicType::短整数 | BasicType::字节) => {
                "qi_future_await_i64"
            }
            TypeNode::基础类型(BasicType::浮点数) => {
                "qi_future_await_f64"
            }
            TypeNode::基础类型(BasicType::布尔) => {
                "qi_future_await_bool"
            }
            TypeNode::基础类型(BasicType::字符串) => {
                "qi_future_await_string"
            }
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
        use crate::parser::ast::{TypeNode, BasicType};
        match type_node {
            TypeNode::基础类型(BasicType::整数 | BasicType::长整数 | BasicType::短整数 | BasicType::字节) => {
                "i64".to_string()
            }
            TypeNode::基础类型(BasicType::浮点数) => {
                "double".to_string()
            }
            TypeNode::基础类型(BasicType::布尔) => {
                "i1".to_string()
            }
            TypeNode::基础类型(BasicType::字符串) => {
                "ptr".to_string()
            }
            TypeNode::结构体类型(_) | TypeNode::自定义类型(_) | TypeNode::指针类型(_) => {
                "ptr".to_string()
            }
            _ => {
                "i64".to_string()
            }
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
    
    /// Map Chinese function names to runtime function names
    /// This bridges Qi language function names (Chinese/English aliases) to actual runtime C function names
    fn map_to_runtime_function(&self, name: &str) -> Option<String> {
        let runtime_func = match name {
            // String operations
            "字符串长度" | "长度" | "len" => Some("qi_runtime_string_length"),
            "字符串连接" | "连接" | "concat" => Some("qi_runtime_string_concat"),
            "字符串切片" | "切片" | "slice" => Some("qi_runtime_string_slice"),
            "字符串比较" | "比较" | "compare" => Some("qi_runtime_string_compare"),

            // Math operations
            "平方根" | "根号" | "求平方根" | "sqrt" => Some("qi_runtime_math_sqrt"),
            "幂" | "次方" | "pow" => Some("qi_runtime_math_pow"),
            "正弦" | "sin" => Some("qi_runtime_math_sin"),
            "余弦" | "cos" => Some("qi_runtime_math_cos"),
            "正切" | "tan" => Some("qi_runtime_math_tan"),
            "绝对值" | "求绝对值" | "abs" => Some("qi_runtime_math_abs_int"), // Default to int, could be smarter
            "向下取整" | "floor" => Some("qi_runtime_math_floor"),
            "向上取整" | "ceil" => Some("qi_runtime_math_ceil"),
            "四舍五入" | "round" => Some("qi_runtime_math_round"),

            // File I/O operations
            "打开文件" | "打开" | "open" => Some("qi_runtime_file_open"),
            "读取文件" | "读取" | "read" => Some("qi_runtime_file_read_string"),
            "写入文件" | "写入" | "write" => Some("qi_runtime_file_write_string"),
            "关闭文件" | "关闭" | "close" => Some("qi_runtime_file_close"),
            "读取文本" => Some("qi_runtime_file_read_string"),
            "写入文本" => Some("qi_runtime_file_write_string"),

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
            "创建等待组" | "新建等待组" | "new_waitgroup" => Some("qi_runtime_waitgroup_create"),
            "等待组增加" | "等待组添加" | "waitgroup_add" | "添加等待" => Some("qi_runtime_waitgroup_add"),
            "等待组完成" | "waitgroup_done" | "完成" => Some("qi_runtime_waitgroup_done"),
            "等待组等待" | "waitgroup_wait" | "等待" => Some("qi_runtime_waitgroup_wait"),

            "创建互斥锁" | "新建互斥锁" | "new_mutex" => Some("qi_runtime_mutex_create"),
            "互斥锁加锁" | "互斥锁锁定" | "mutex_lock" | "加锁" => Some("qi_runtime_mutex_lock"),
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
                        if let Some(t) = infer_from_node(s) { return Some(t); }
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
                        if let Some(t) = infer_from_node(s) { return Some(t); }
                    }
                    None
                }
                AstNode::循环语句(loop_stmt) => {
                    for s in &loop_stmt.body {
                        if let Some(t) = infer_from_node(s) { return Some(t); }
                    }
                    None
                }
                // Program and other containers
                AstNode::程序(p) => {
                    for s in &p.statements {
                        if let Some(t) = infer_from_node(s) { return Some(t); }
                    }
                    None
                }
                _ => None,
            }
        }

        for stmt in body {
            if let Some(t) = infer_from_node(stmt) { return Some(t); }
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
                // Mangle variable names for Chinese characters
                let var_name = if decl.name.chars().any(|c| !c.is_ascii()) {
                    format!("%{}", self.mangle_function_name(&decl.name))
                } else {
                    format!("%{}", decl.name)
                };

                // Determine the type based on the initializer or type annotation
                // For binary expressions, we need to evaluate them first to get their type
                let (type_name, pre_evaluated_init) = if let Some(initializer) = &decl.initializer {
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
                            ("ptr".to_string(), None)
                        }
                        AstNode::二元操作表达式(_) => {
                            // Build the initializer first to determine its type
                            let init_value = self.build_node(&**initializer)?;
                            let init_var_name = init_value.trim_start_matches('%');
                            let ty = self.variable_types.get(init_var_name)
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
                            let ty = if let Some(runtime_func) = self.map_to_runtime_function(&function_name) {
                                if runtime_func.contains("math_sqrt") || runtime_func.contains("math_pow") ||
                                   runtime_func.contains("math_sin") || runtime_func.contains("math_cos") ||
                                   runtime_func.contains("math_tan") || runtime_func.contains("math_floor") ||
                                   runtime_func.contains("math_ceil") || runtime_func.contains("math_round") ||
                                   runtime_func.contains("math_abs_float") || runtime_func.contains("int_to_float") ||
                                   runtime_func.contains("string_to_float") {
                                    "double"
                                } else if runtime_func.contains("string_length") {
                                    "i64"  // string_length returns integer, not string
                                } else if runtime_func.starts_with("qi_crypto_") && runtime_func != "qi_crypto_free_string" {
                                    "ptr"  // All crypto functions return string (ptr)
                                } else if runtime_func.contains("read_string") ||
                                          runtime_func.contains("int_to_string") ||
                                          runtime_func.contains("float_to_string") ||
                                          runtime_func.contains("string") ||
                                          runtime_func == "qi_runtime_string_concat" {
                                    "ptr"
                                } else if runtime_func.contains("math_abs_int") || runtime_func.contains("float_to_int") ||
                                          runtime_func.contains("string_to_int") || runtime_func.contains("array_length") ||
                                          runtime_func.contains("get_time_ms") || runtime_func.contains("file_open") ||
                                          runtime_func.contains("file_read") || runtime_func.contains("file_write") ||
                                          runtime_func.contains("tcp_connect") {
                                    "i64"  // Functions that return i64
                                } else if runtime_func.contains("waitgroup_create") ||
                                          runtime_func.contains("mutex_create") ||
                                          runtime_func.contains("rwlock_create") ||
                                          runtime_func.contains("timer_create") ||
                                          runtime_func.contains("create_channel") ||
                                          runtime_func.contains("create_task") {
                                    "ptr"  // Synchronization primitives and async constructs return pointers
                                } else if runtime_func == "qi_runtime_set_timeout" || runtime_func == "qi_runtime_timer_expired" ||
                                          runtime_func == "qi_runtime_timer_stop" {
                                    "i64"  // Timer status functions return i64
                                } else if runtime_func.contains("trylock") || runtime_func.contains("timeout") ||
                                          runtime_func.contains("retry") || runtime_func.contains("catch_error") ||
                                          runtime_func.contains("mutex") || runtime_func.contains("waitgroup") ||
                                          runtime_func.contains("channel") {
                                    "i32"  // Most synchronization and status functions return i32 status codes
                                } else {
                                    "i32"  // Default to i32 for unknown runtime functions (most return status codes)
                                }
                            } else if let Some(ret_type) = self.function_return_types.get(&self.mangle_function_name(&function_name) as &str) {
                                ret_type  // Use stored return type from function signature
                            } else {
                                "i64"
                            };
                            (ty.to_string(), None)
                        }
                        AstNode::取地址表达式(_) => {
                            // Address-of expressions always return pointers
                            ("ptr".to_string(), None)
                        }
                        AstNode::等待表达式(await_expr) => {
                            // Determine type from Future inner type BEFORE building the await expression
                            // This is necessary because variable allocation needs to know the type
                            let (ty, struct_name) = if let AstNode::标识符表达式(ident) = await_expr.expression.as_ref() {
                                let future_var = &ident.name;
                                if let Some(inner_type_info) = self.future_inner_types.get(future_var) {
                                    if inner_type_info.starts_with("struct.") {
                                        // Extract struct type name
                                        let struct_name = inner_type_info.strip_prefix("struct.").unwrap();
                                        eprintln!("[AWAIT-VAR-DECL] Future inner type is struct: {}", struct_name);
                                        ("ptr".to_string(), Some(struct_name.to_string()))
                                    } else {
                                        // Basic type from Future inner type
                                        eprintln!("[AWAIT-VAR-DECL] Future inner type: {}", inner_type_info);
                                        (inner_type_info.to_string(), None)
                                    }
                                } else {
                                    eprintln!("[AWAIT-VAR-DECL] No Future inner type found for {}, defaulting to i64", future_var);
                                    ("i64".to_string(), None)
                                }
                            } else if let AstNode::函数调用表达式(call_expr) = await_expr.expression.as_ref() {
                                // Awaiting a function call - infer from function's Future<T> return type
                                let function_name = self.get_full_function_name(call_expr);
                                let mangled = if function_name.chars().any(|c| !c.is_ascii()) {
                                    self.mangle_function_name(&function_name)
                                } else {
                                    function_name.clone()
                                };

                                if let Some(inner_type_info) = self.function_future_inner_types.get(&mangled) {
                                    if inner_type_info.starts_with("struct.") {
                                        let struct_name = inner_type_info.strip_prefix("struct.").unwrap();
                                        eprintln!("[AWAIT-VAR-DECL] Function {} returns Future<struct {}>, allocating ptr", function_name, struct_name);
                                        ("ptr".to_string(), Some(struct_name.to_string()))
                                    } else {
                                        eprintln!("[AWAIT-VAR-DECL] Function {} returns Future<{}>, using that type", function_name, inner_type_info);
                                        (inner_type_info.to_string(), None)
                                    }
                                } else {
                                    eprintln!("[AWAIT-VAR-DECL] No Future inner type for function {}, defaulting to i64", function_name);
                                    ("i64".to_string(), None)
                                }
                            } else {
                                eprintln!("[AWAIT-VAR-DECL] await expression not from identifier or function call, defaulting to i64");
                                ("i64".to_string(), None)
                            };

                            // Preserve struct type information if present
                            if let Some(struct_name) = struct_name {
                                eprintln!("[AWAIT-VAR-DECL] Preserving struct type {} for variable {}", struct_name, decl.name);
                                self.variable_struct_types.insert(decl.name.clone(), struct_name);
                            }

                            // Now build the await expression
                            let init_value = self.build_node(&**initializer)?;
                            eprintln!("[AWAIT-VAR-DECL] Final type for variable {}: {}", &decl.name, ty);

                            (ty, Some(init_value))
                        }
                        AstNode::结构体实例化表达式(struct_lit) => {
                            // Struct literals return pointers
                            let init_value = self.build_node(&**initializer)?;
                            // Also propagate the struct type info
                            let init_var_name = init_value.trim_start_matches('%');
                            if let Some(struct_type_name) = self.variable_struct_types.get(init_var_name) {
                                self.variable_struct_types.insert(decl.name.clone(), struct_type_name.clone());
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
                    (ty.to_string(), None)
                };

                // Record the variable type for later use (both original and mangled names)
                let mangled_name = if decl.name.chars().any(|c| !c.is_ascii()) {
                    format!("_Z_{}", self.mangle_function_name(&decl.name).trim_start_matches("_Z_"))
                } else {
                    decl.name.clone()
                };
                self.variable_types.insert(decl.name.clone(), type_name.to_string());
                self.variable_types.insert(mangled_name.clone(), type_name.to_string());

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
                        eprintln!("[FUTURE-TRACK] Variable {} (mangled: {}) has Future inner type: {}", decl.name, mangled_name, inner_type_info);
                        self.future_inner_types.insert(decl.name.clone(), inner_type_info.clone());
                        self.future_inner_types.insert(mangled_name.clone(), inner_type_info);
                    } else if let crate::parser::ast::TypeNode::基础类型(crate::parser::ast::BasicType::布尔) = type_ann {
                        // Track boolean variables (even if they end up stored as i32)
                        self.boolean_variables.insert(decl.name.clone());
                        self.boolean_variables.insert(mangled_name.clone());
                    }
                }

                // Allocate variable
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
                    self.add_instruction(IrInstruction::存储 {
                        target: var_name.clone(),
                        value,
                        value_type: Some(type_name.to_string()),
                    });
                }

                Ok(var_name)
            }
            AstNode::函数声明(func_decl) => {
                // Handle special cases and apply name mangling for Chinese function names
                let func_name: String = match func_decl.name.as_str() {
                    "入口" => "main".to_string(), // Special case for main function
                    name => {
                        if name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(name)
                        } else {
                            name.to_string()
                        }
                    }
                };

                let is_main = func_decl.name == "入口" || func_name == "main";

                // Clear variable types for this new function scope
                // (but keep function_param_types, function_return_types, etc.)
                self.variable_types.clear();
                self.variable_struct_types.clear();

                // Build parameter list with mangled names for Chinese identifiers
                let params: Vec<String> = func_decl.parameters
                    .iter()
                    .map(|p| {
                        let type_str = self.get_llvm_type(&p.type_annotation);
                        let param_name = if p.name.chars().any(|c| !c.is_ascii()) {
                            format!("%{}", self.mangle_function_name(&p.name))
                        } else {
                            format!("%{}", p.name)
                        };
                        format!("{} {}", type_str, param_name)
                    })
                    .collect();

                // Mark parameters as direct values (not pointers) in variable_types
                for param in &func_decl.parameters {
                    let param_name = if param.name.chars().any(|c| !c.is_ascii()) {
                        self.mangle_function_name(&param.name)
                    } else {
                        param.name.clone()
                    };
                    let type_str = self.get_llvm_type(&param.type_annotation);
                    // Store with a special prefix to indicate this is a parameter (direct value)
                    self.variable_types.insert(format!("param_{}", param_name), type_str.clone());
                    self.variable_types.insert(param_name.clone(), type_str.clone());
                    eprintln!("[DEBUG] Function {} parameter: {} -> type {}", func_name, param_name, type_str);
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
                    self.infer_return_type_from_body(&func_decl.body).unwrap_or_else(|| "void".to_string())
                };

                // Record the function's parameter types for later function calls
                let param_types: Vec<String> = func_decl.parameters
                    .iter()
                    .map(|p| self.get_llvm_type(&p.type_annotation))
                    .collect();
                self.function_param_types.insert(func_name.clone(), param_types);

                // Record the function's return type for later function calls
                self.function_return_types.insert(func_name.clone(), return_type.clone());

                // If function returns Future<T>, track the inner type
                if let Some(ref ret_type_node) = func_decl.return_type {
                    if let crate::parser::ast::TypeNode::未来类型(inner_type) = ret_type_node {
                        let inner_llvm_type = self.get_llvm_type_from_ast(inner_type);
                        self.function_future_inner_types.insert(func_name.clone(), inner_llvm_type);
                    }
                }

                // Record this function as defined in current module
                self.defined_functions.insert(func_name.clone());

                // Set current function name for return statement processing
                self.current_function_name = Some(func_name.clone());

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

                // Detect whether an explicit return was emitted in this function
                let mut has_explicit_return = false;
                for instr in &self.instructions[start_len..] {
                    if let IrInstruction::返回 { .. } = instr {
                        has_explicit_return = true;
                        break;
                    }
                }

                // Add implicit return if needed
                if !has_explicit_return {
                    if is_main {
                        // Call runtime shutdown before returning from main
                        let shutdown_result = self.generate_temp();
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(shutdown_result),
                            callee: "qi_runtime_shutdown".to_string(),
                            arguments: vec![],
                        });
                        // main returns i32 0 by default
                        self.add_instruction(IrInstruction::返回 { value: Some("0".to_string()) });
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
                        self.add_instruction(IrInstruction::返回 { value: Some(zero_val.to_string()) });
                    }
                }

                // // Function is already properly closed by function body processing
                self.add_instruction(IrInstruction::标签 { name: "}".to_string() });

                // Clear current function name and return type after function ends
                self.current_function_name = None;
                self.current_function_ast_return_type = None;

                // Add module-qualified alias if we have a package name and this is not main
                if let Some(package_name) = &self.current_package_name {
                    if !is_main {
                        // Create an alias: @数学_最大值 = alias i64 (i64, i64), ptr @最大值
                        let alias_name = format!("{}_{}", package_name, &func_decl.name);
                        let alias_mangled = self.mangle_function_name(&alias_name);
                        
                        // Build parameter types list (without parameter names)
                        let param_types: Vec<String> = func_decl.parameters
                            .iter()
                            .map(|p| self.get_llvm_type(&p.type_annotation))
                            .collect();
                        let param_types_str = param_types.join(", ");
                        
                        // Generate function signature for alias
                        self.add_instruction(IrInstruction::标签 {
                            name: format!("@{} = alias {} ({}), ptr @{}", 
                                alias_mangled, return_type, param_types_str, func_name)
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
                let final_value = if let Some(ref ast_return_type) = self.current_function_ast_return_type {
                    if let crate::parser::ast::TypeNode::未来类型(inner_type) = ast_return_type {
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
                                self.variable_types.insert(bool_var.to_string(), "i32".to_string());
                                vec![bool_i32]
                            } else if ready_func == "qi_future_ready_ptr" {
                                // Pointer (struct/custom types): determine the actual type of the value
                                // The value might be a variable or a temporary
                                let val_var = val.trim_start_matches('%');
                                let val_type = self.variable_types.get(val_var)
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
                    self.add_instruction(IrInstruction::跳转 { label: end_label.clone() });
                    Ok("break".to_string())
                } else {
                    Err("跳出语句必须在循环内使用".to_string())
                }
            }
            AstNode::继续语句(_) => {
                // Continue: jump to the start label of the innermost loop
                if let Some((start_label, _)) = self.loop_stack.last() {
                    self.add_instruction(IrInstruction::跳转 { label: start_label.clone() });
                    Ok("continue".to_string())
                } else {
                    Err("继续语句必须在循环内使用".to_string())
                }
            }
            AstNode::表达式语句(expr_stmt) => {
                self.build_node(&expr_stmt.expression)
            }
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
                self.add_instruction(IrInstruction::标签 { name: then_label.clone() });
                let then_has_return = self.contains_return(&if_stmt.then_branch);
                for stmt in &if_stmt.then_branch {
                    self.build_node(stmt)?;
                }
                // Only add jump if there's no return
                if !then_has_return {
                    self.add_instruction(IrInstruction::跳转 { label: end_label.clone() });
                }

                // Else branch (if exists)
                self.add_instruction(IrInstruction::标签 { name: else_label.clone() });
                let else_has_return = if let Some(else_branch) = &if_stmt.else_branch {
                    let has_ret = self.node_contains_return(else_branch);
                    self.build_node(else_branch)?;
                    has_ret
                } else {
                    false
                };
                
                // Only add jump if there's no return
                if !else_has_return {
                    self.add_instruction(IrInstruction::跳转 { label: end_label.clone() });
                }

                // Only add end label if at least one branch doesn't return
                if !then_has_return || !else_has_return {
                    self.add_instruction(IrInstruction::标签 { name: end_label.clone() });
                }

                Ok("if".to_string())
            }
            AstNode::当语句(while_stmt) => {
                // Generate labels
                let start_label = self.generate_label();
                let body_label = self.generate_label();
                let end_label = self.generate_label();

                // Push loop labels onto stack for break/continue
                self.loop_stack.push((start_label.clone(), end_label.clone()));

                // Jump to start label (condition check)
                self.add_instruction(IrInstruction::跳转 { label: start_label.clone() });

                // Start label (condition check)
                self.add_instruction(IrInstruction::标签 { name: start_label.clone() });

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
                self.add_instruction(IrInstruction::标签 { name: body_label.clone() });

                // Body
                for stmt in &while_stmt.body {
                    self.build_node(stmt)?;
                }

                // Jump back to start to check condition again
                self.add_instruction(IrInstruction::跳转 { label: start_label.clone() });

                // End label
                self.add_instruction(IrInstruction::标签 { name: end_label.clone() });

                // Pop loop labels from stack
                self.loop_stack.pop();

                Ok("while".to_string())
            }
            AstNode::循环语句(loop_stmt) => {
                // Generate labels
                let start_label = self.generate_label();
                let end_label = self.generate_label();

                // Start label
                self.add_instruction(IrInstruction::标签 { name: start_label.clone() });

                // Body
                for stmt in &loop_stmt.body {
                    self.build_node(stmt)?;
                }

                // Jump back to start (infinite loop)
                self.add_instruction(IrInstruction::跳转 { label: start_label.clone() });

                // End label (unreachable in current implementation)
                self.add_instruction(IrInstruction::标签 { name: end_label.clone() });

                Ok("loop".to_string())
            }
            AstNode::对于语句(for_stmt) => {
                // Handle: for var in [1, 2, 3] { ... }
                // For now, support array literals only
                
                // First, evaluate the range expression to get the array
                let array_val = self.build_node(&for_stmt.range)?;
                
                // Check if range is an array literal - if so, we know the size
                let max_iterations = match &*for_stmt.range {
                    AstNode::数组字面量表达式(arr_lit) => arr_lit.elements.len().to_string(),
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
                self.add_instruction(IrInstruction::跳转 { label: start_label.clone() });
                
                // Start label (condition check)
                self.add_instruction(IrInstruction::标签 { name: start_label.clone() });
                
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
                self.add_instruction(IrInstruction::标签 { name: body_label.clone() });
                
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
                self.add_instruction(IrInstruction::跳转 { label: start_label.clone() });
                
                // End label
                self.add_instruction(IrInstruction::标签 { name: end_label.clone() });
                
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
                    },
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
                        self.variable_types.insert(temp_var_name.to_string(), "i1".to_string());
                        Ok(temp_val)
                    },
                    crate::parser::ast::LiteralValue::字符串(s) => {
                        // Create a global string constant matching clang's format
                        let escaped_str = self.escape_string(s);
                        let byte_len = s.as_bytes().len();
                        let total_len = byte_len + 1; // +1 for null terminator

                        // Generate a unique string name by incrementing temp_counter
                        let str_name = format!("@.str{}", self.temp_counter);
                        self.temp_counter += 1;

                        self.add_instruction(IrInstruction::字符串常量 {
                            name: format!("{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1",
                                str_name, total_len, escaped_str),
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
                    let left_is_string = left.starts_with('@') ||
                        (left.starts_with('%') &&
                         self.variable_types.get(left.trim_start_matches('%'))
                             .map(|t| t == "ptr").unwrap_or(false));
                    let right_is_string = right.starts_with('@') ||
                        (right.starts_with('%') &&
                         self.variable_types.get(right.trim_start_matches('%'))
                             .map(|t| t == "ptr").unwrap_or(false));

                    if left_is_string || right_is_string {
                        // This is string concatenation - convert non-string operands to strings first
                        let left_str = if left_is_string {
                            left
                        } else {
                            // Convert non-string to string
                            let conv_temp = self.generate_temp();
                            let left_type = if left.starts_with('%') {
                                self.variable_types.get(left.trim_start_matches('%'))
                                    .map(|s| s.as_str()).unwrap_or("i64")
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
                            self.variable_types.insert(conv_temp.trim_start_matches('%').to_string(), "ptr".to_string());
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
                                self.variable_types.get(right.trim_start_matches('%'))
                                    .map(|s| s.as_str()).unwrap_or("i64")
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
                            self.variable_types.insert(conv_temp.trim_start_matches('%').to_string(), "ptr".to_string());
                            self.add_instruction(IrInstruction::函数调用 {
                                dest: Some(conv_temp.clone()),
                                callee: conv_func.to_string(),
                                arguments: vec![right],
                            });
                            conv_temp
                        };

                        // Now concatenate the two strings
                        let temp = self.generate_temp();
                        self.variable_types.insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(temp.clone()),
                            callee: "qi_runtime_string_concat".to_string(),
                            arguments: vec![left_str, right_str],
                        });
                        return Ok(temp);
                    }
                }

                // Determine the operand type and result type of the binary operation
                // Check if either operand is a float type (either literal or variable)
                let is_float_op = left.contains('.') || right.contains('.') ||
                                  self.is_float_operand(&left) || self.is_float_operand(&right);

                // Determine the operand type for the operation
                let operand_type = if is_float_op {
                    "double".to_string()
                } else {
                    // Check if left operand has a specific type (i32, i64, etc.)
                    let left_type = if left.starts_with('%') {
                        self.variable_types.get(left.trim_start_matches('%'))
                            .map(|s| s.as_str()).unwrap_or("i64")
                    } else {
                        "i64"
                    };
                    left_type.to_string()
                };

                // For comparison operators, result is i1 (boolean), otherwise same as operand type
                let result_type = match binary_expr.operator {
                    BinaryOperator::等于 | BinaryOperator::不等于 |
                    BinaryOperator::大于 | BinaryOperator::小于 |
                    BinaryOperator::大于等于 | BinaryOperator::小于等于 => "i1".to_string(),
                    _ => operand_type.clone(),
                };

                let temp = self.generate_temp();

                // Record the type of this temporary variable
                self.variable_types.insert(temp.trim_start_matches('%').to_string(), result_type.clone());

                self.add_instruction(IrInstruction::二元操作 {
                    dest: temp.clone(),
                    left,
                    operator: binary_expr.operator,
                    right,
                    operand_type: operand_type,
                });
                Ok(temp)
            }
            AstNode::赋值表达式(assign_expr) => {
                let value = self.build_node(&assign_expr.value)?;

                // Handle different LValue types
                match assign_expr.target.as_ref() {
                    AstNode::标识符表达式(ident) => {
                        // Simple variable assignment: x = value
                        let target_name = if ident.name.chars().any(|c| !c.is_ascii()) {
                            format!("%{}", self.mangle_function_name(&ident.name))
                        } else {
                            format!("%{}", ident.name)
                        };

                        self.add_instruction(IrInstruction::存储 {
                            target: target_name.clone(),
                            value,
                            value_type: None,
                        });
                        Ok(target_name)
                    }
                    AstNode::字段访问表达式(field_access) => {
                        // Field assignment: obj.field = value
                        // First get the field address
                        let object = self.build_node(&field_access.object)?;
                        let field_addr = format!("%t{}", self.generate_temp());
                        self.add_instruction(IrInstruction::字段访问 {
                            dest: field_addr.clone(),
                            object,
                            field: field_access.field.clone(),
                            struct_type: "unknown".to_string(), // TODO: track struct types
                        });
                        
                        // Then store the value to that address
                        self.add_instruction(IrInstruction::存储 {
                            target: field_addr.clone(),
                            value,
                            value_type: None,
                        });
                        Ok(field_addr)
                    }
                    AstNode::数组访问表达式(array_access) => {
                        // Array element assignment: arr[index] = value
                        let array = self.build_node(&array_access.array)?;
                        let index = self.build_node(&array_access.index)?;
                        
                        // Generate instruction to store to array element
                        self.add_instruction(IrInstruction::数组存储 {
                            array: array.clone(),
                            index,
                            value,
                        });
                        Ok(array)
                    }
                    _ => Err(format!("Invalid assignment target: {:?}", assign_expr.target)),
                }
            }
            AstNode::函数调用表达式(call_expr) => {
                // Special handling for 打印 and 打印行 functions - map to appropriate runtime function
                let function_name = self.get_full_function_name(call_expr);
                let runtime_function = if function_name == "打印" || function_name == "打印行" {
                    if call_expr.arguments.len() == 1 {
                        // Single argument - determine type
                        let first_arg = &call_expr.arguments[0];
                        let expr_type = match first_arg {
                            AstNode::字面量表达式(literal) => {
                                match &literal.value {
                                    crate::parser::ast::LiteralValue::字符串(_) => "string",
                                    crate::parser::ast::LiteralValue::整数(_) => "integer",
                                    crate::parser::ast::LiteralValue::浮点数(_) => "float",
                                    crate::parser::ast::LiteralValue::布尔(_) => "boolean",
                                    crate::parser::ast::LiteralValue::字符(_) => "integer",
                                }
                            }
                            AstNode::标识符表达式(ident) => {
                                // Check if this is a semantically boolean variable first
                                let is_bool_var = self.boolean_variables.contains(&ident.name) || {
                                    let mangled = if ident.name.chars().any(|c| !c.is_ascii()) {
                                        format!("_Z_{}", self.mangle_function_name(&ident.name).trim_start_matches("_Z_"))
                                    } else {
                                        ident.name.clone()
                                    };
                                    self.boolean_variables.contains(&mangled)
                                };

                                if is_bool_var {
                                    "boolean"
                                } else {
                                    // Look up variable type from our tracking
                                    let var_type = self.variable_types.get(&ident.name).or_else(|| {
                                        let mangled = if ident.name.chars().any(|c| !c.is_ascii()) {
                                            format!("_Z_{}", self.mangle_function_name(&ident.name).trim_start_matches("_Z_"))
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
                            AstNode::字符串连接表达式(_) => "string", // String concatenation returns string
                            AstNode::二元操作表达式(_) => "integer", // Binary ops default to integer for now
                            _ => "integer", // Default to integer
                        };

                        // Map to appropriate runtime function
                        let is_println = function_name == "打印行";
                        Some(match expr_type {
                            "string" => if is_println { "qi_runtime_println" } else { "qi_runtime_print" },
                            "float" => if is_println { "qi_runtime_println_float" } else { "qi_runtime_print_float" },
                            "boolean" => if is_println { "qi_runtime_println_bool" } else { "qi_runtime_print_bool" },
                            _ => if is_println { "qi_runtime_println_int" } else { "qi_runtime_print_int" },
                        }.to_string())
                    } else {
                        None
                    }
                } else {
                    // Check if this is a builtin runtime function
                    self.map_to_runtime_function(&function_name)
                };
                
                // Evaluate arguments
                let mut arg_temps = Vec::new();
                for arg in &call_expr.arguments {
                    let temp = self.build_node(arg)?;
                    arg_temps.push(temp);
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

                // Special handling: 打印 or 打印行 with multiple arguments -> map to printf with proper format
                if (function_name == "打印" || function_name == "打印行") && arg_temps.len() >= 2 {
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
                            self.variable_types.get(var_name)
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
                            actual_len + 1,  // +1 for null terminator
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
                if !mapped_callee.starts_with("qi_runtime_") &&
                   !mapped_callee.starts_with("qi_crypto_") &&
                   mapped_callee != "printf" &&
                   !self.defined_functions.contains(&mapped_callee) &&
                   !self.external_functions.contains_key(&mapped_callee as &str) {
                    // This is an external function - record its signature
                    // Determine parameter types from arguments
                    let param_types: Vec<String> = arg_temps.iter().map(|arg| {
                        if arg.starts_with('%') {
                            let var_name = arg.trim_start_matches('%');
                            self.variable_types.get(var_name)
                                .cloned()
                                .unwrap_or_else(|| "i64".to_string())
                        } else if arg.parse::<i64>().is_ok() {
                            "i64".to_string()
                        } else if arg.parse::<f64>().is_ok() {
                            "double".to_string()
                        } else {
                            "i64".to_string()
                        }
                    }).collect();
                    
                    // Determine return type
                    // For async functions, always use ptr
                    let ret_type = if self.async_function_types.contains_key(&mapped_callee) {
                        "ptr".to_string()
                    } else if let Some(rt) = self.function_return_types.get(&mapped_callee) {
                        rt.clone()
                    } else {
                        "i64".to_string() // Default to i64
                    };
                    
                    self.external_functions.insert(mapped_callee.clone(), (param_types, ret_type));
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
                    self.variable_types.insert(task_temp.trim_start_matches('%').to_string(), "ptr".to_string());

                    Ok(task_temp)
                } else if is_async_function && self.in_async_context {
                    // This is an async function call from an async context - call it directly
                    // The async function returns ptr directly
                    self.variable_types.insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

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
                            if mapped_callee.contains("string_length") {
                                "i64"  // string_length returns integer, not string
                            } else if mapped_callee.contains("string") || mapped_callee.contains("concat") ||
                               mapped_callee.contains("read_string") || mapped_callee.contains("int_to_string") ||
                               mapped_callee.contains("float_to_string") {
                                "ptr"
                            } else if mapped_callee.contains("sqrt") || mapped_callee.contains("abs") ||
                                      mapped_callee.contains("math") || mapped_callee.contains("float") {
                                "double"
                            } else if mapped_callee.contains("get_time_ms") || mapped_callee.contains("array_length") ||
                                      mapped_callee.contains("file_open") || mapped_callee.contains("file_read") ||
                                      mapped_callee.contains("file_write") || mapped_callee.contains("tcp_connect") ||
                                      mapped_callee.contains("string_to_int") || mapped_callee.contains("float_to_int") ||
                                      mapped_callee.contains("create_channel") || mapped_callee.contains("create_task") ||
                                      mapped_callee.contains("create_timer") {
                                "i64"  // Functions that explicitly return i64 or pointers treated as i64
                            } else if mapped_callee == "qi_runtime_set_timeout" || mapped_callee == "qi_runtime_timer_expired" ||
                                      mapped_callee == "qi_runtime_timer_stop" {
                                "i64"  // Timer status functions return i64
                            } else {
                                "i32"  // Most runtime functions return i32 status codes
                            }
                        } else if mapped_callee == "qi_runtime_string_concat" {
                            "ptr"
                        } else if let Some(ret_type) = self.function_return_types.get(&mapped_callee) {
                            ret_type
                        } else {
                            "i64"
                        };
                        self.variable_types.insert(temp.trim_start_matches('%').to_string(), return_type.to_string());

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
                let original_var_name = if let AstNode::标识符表达式(ident) = await_expr.expression.as_ref() {
                    Some(ident.name.clone())
                } else {
                    None
                };

                // Build the awaited expression first
                let future_expr = self.build_node(&await_expr.expression)?;

                // Determine if this is a Future<T> type or an async coroutine
                let future_var = future_expr.trim_start_matches('%');
                let is_future_type = self.variable_types.get(future_var)
                    .map(|t| t == "ptr")
                    .unwrap_or(false);

                // Propagate Future inner type to the temp variable
                let mut inner_type_propagated = false;

                // If we have the original variable name and it's a Future, propagate its inner type
                if let Some(orig_name) = &original_var_name {
                    let inner_type_opt = self.future_inner_types.get(orig_name).cloned().or_else(|| {
                        let mangled = if orig_name.chars().any(|c| !c.is_ascii()) {
                            format!("_Z_{}", self.mangle_function_name(orig_name).trim_start_matches("_Z_"))
                        } else {
                            orig_name.clone()
                        };
                        self.future_inner_types.get(&mangled).cloned()
                    });

                    if let Some(inner_type) = inner_type_opt {
                        eprintln!("[AWAIT-EXPR-PROPAGATE] Propagating inner type {} from variable {} to temp {}", inner_type, orig_name, future_var);
                        self.future_inner_types.insert(future_var.to_string(), inner_type.clone());
                        inner_type_propagated = true;
                    }
                }

                // If awaiting a function call, infer the inner type from the function's return type annotation
                if !inner_type_propagated && original_var_name.is_none() {
                    if let AstNode::函数调用表达式(call_expr) = await_expr.expression.as_ref() {
                        let function_name = self.get_full_function_name(call_expr);
                        let mangled = if function_name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(&function_name)
                        } else {
                            function_name.clone()
                        };

                        // Look up the function's Future inner type
                        if let Some(inner_type) = self.function_future_inner_types.get(&mangled) {
                            eprintln!("[AWAIT-EXPR-FUNC-CALL] Awaiting function {} -> temp {}, inner_type={}", function_name, future_var, inner_type);
                            self.future_inner_types.insert(future_var.to_string(), inner_type.clone());
                        } else {
                            eprintln!("[AWAIT-EXPR-FUNC-CALL] Awaiting function {} -> temp {}, no inner type found", function_name, future_var);
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
                    _ => (None, false)
                };

                // Record the type of the awaited value
                let result_type = inferred_type.unwrap_or_else(|| "i64".to_string());

                // In async context, if we're awaiting an async function call,
                // it was already called directly and returned the result
                if self.in_async_context && is_async_call {
                    // The future_expr is already the result, no need for await
                    // Just return it directly
                    self.variable_types.insert(future_expr.trim_start_matches('%').to_string(), result_type);
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
                        self.future_inner_types.get(orig_name)
                            .or_else(|| {
                                let mangled = if orig_name.chars().any(|c| !c.is_ascii()) {
                                    format!("_Z_{}", self.mangle_function_name(orig_name).trim_start_matches("_Z_"))
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
                    eprintln!("[AWAIT-EXPR] original_var_name={:?}, future_var={}, inner_type={}", original_var_name, future_var, inner_type);

                    // Map inner type to the final result type (after any conversions)
                    let return_type = if inner_type.starts_with("struct.") {
                        "ptr"  // Struct types return ptr
                    } else {
                        match inner_type {
                            "i64" => "i64",      // qi_future_await_i64 returns i64
                            "double" => "double", // qi_future_await_f64 returns double
                            "i1" => "i1",        // qi_future_await_bool returns i32, but we convert to i1
                            "ptr" => "ptr",      // qi_future_await_string/ptr returns ptr
                            _ => "i64",          // Default
                        }
                    };
                    eprintln!("[AWAIT-EXPR] return_type={}, await_temp={}", return_type, await_temp);

                    // Track the final result type (after any conversions)
                    let temp_key = await_temp.trim_start_matches('%').to_string();
                    eprintln!("[AWAIT-EXPR] Inserting variable_types[{}] = {}", temp_key, return_type);
                    self.variable_types.insert(temp_key, return_type.to_string());
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
                    self.variable_types.insert(result_temp.trim_start_matches('%').to_string(), result_type.clone());
                    // Also record the await_temp as pointing to this type
                    self.variable_types.insert(await_temp.trim_start_matches('%').to_string(), result_type);

                    Ok(result_temp)
                }
            }
            AstNode::标识符表达式(ident) => {
                let temp = self.generate_temp();
                let var_name = if ident.name.chars().any(|c| !c.is_ascii()) {
                    format!("%{}", self.mangle_function_name(&ident.name))
                } else {
                    format!("%{}", ident.name)
                };
                
                // Also compute the bare mangled name without %
                let bare_mangled = if ident.name.chars().any(|c| !c.is_ascii()) {
                    self.mangle_function_name(&ident.name)
                } else {
                    ident.name.clone()
                };

                // Get the variable type and record it for the loaded value
                // Try multiple keys: original name, var_name without %, mangled name, mangled with _Z_ prefix
                let var_type = self.variable_types.get(&ident.name)
                    .or_else(|| self.variable_types.get(var_name.trim_start_matches('%')))
                    .or_else(|| self.variable_types.get(&bare_mangled))
                    .or_else(|| {
                        let with_prefix = format!("_Z_{}", bare_mangled.trim_start_matches("_Z_"));
                        self.variable_types.get(&with_prefix)
                    })
                    .cloned();

                eprintln!("[DEBUG] Identifier: {} (mangled: {}), type: {:?}, is_param: {}", 
                         ident.name, bare_mangled, var_type,
                         self.variable_types.contains_key(&format!("param_{}", ident.name)) ||
                         self.variable_types.contains_key(&format!("param_{}", bare_mangled)));

                if let Some(vtype) = var_type.clone() {
                    self.variable_types.insert(temp.trim_start_matches('%').to_string(), vtype);
                }
                
                // Also propagate struct type if it exists
                if let Some(struct_type) = self.variable_struct_types.get(&ident.name).cloned() {
                    self.variable_struct_types.insert(temp.trim_start_matches('%').to_string(), struct_type);
                }

                // Check if this is a parameter (direct value, not a pointer)
                // Need to check both original name and mangled name
                let param_key1 = format!("param_{}", ident.name);
                let param_key2 = format!("param_{}", bare_mangled);
                let has_param_key1 = self.variable_types.contains_key(&param_key1);
                let has_param_key2 = self.variable_types.contains_key(&param_key2);
                let is_param = has_param_key1 || has_param_key2;

                eprintln!("[DEBUG] Identifier {} check: param_key1='{}' ({}), param_key2='{}' ({}), is_param={}",
                         ident.name, param_key1, has_param_key1, param_key2, has_param_key2, is_param);

                if is_param {
                    // This is a parameter - use it directly without load
                    // Return the parameter name instead of generating a temp
                    eprintln!("[DEBUG] Using parameter directly: {}", var_name);
                    Ok(var_name)
                } else {
                    // This is a regular variable or unknown identifier - load it
                    // Even if var_type is None, we'll try to load it
                    // (it might be an error, but let LLVM catch it)
                    eprintln!("[DEBUG] Loading variable: {}", var_name);
                    self.add_instruction(IrInstruction::加载 {
                        dest: temp.clone(),
                        source: var_name,
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

                // Generate getelementptr instruction
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::数组访问 {
                    dest: temp.clone(),
                    array: array_var,
                    index: index_var,
                });
                Ok(temp)
            }
            AstNode::数组字面量表达式(array_literal) => {
                // For now, create a simple array literal
                // In a real implementation, this would allocate memory and store elements
                let temp = self.generate_temp();

                // Create array allocation
                let size = array_literal.elements.len();
                self.add_instruction(IrInstruction::数组分配 {
                    dest: temp.clone(),
                    size: size.to_string(),
                });

                // Store each element (simplified)
                for (i, element) in array_literal.elements.iter().enumerate() {
                    let element_var = self.build_node(element)?;
                    self.add_instruction(IrInstruction::数组存储 {
                        array: temp.clone(),
                        index: i.to_string(),
                        value: element_var,
                    });
                }

                Ok(temp)
            }
            AstNode::字符串连接表达式(string_concat) => {
                // Build left and right expressions
                let left_var = self.build_node(&string_concat.left)?;
                let right_var = self.build_node(&string_concat.right)?;

                // Check if we need to convert left to string
                let left_str = {
                    let is_string = left_var.starts_with('@') || 
                        (left_var.starts_with('%') && 
                         self.variable_types.get(left_var.trim_start_matches('%'))
                             .map(|t| t == "ptr").unwrap_or(false));
                    
                    if is_string {
                        left_var
                    } else {
                        // Convert to string
                        let conv_temp = self.generate_temp();
                        let left_type = if left_var.starts_with('%') {
                            self.variable_types.get(left_var.trim_start_matches('%'))
                                .map(|s| s.as_str()).unwrap_or("i64")
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
                        self.variable_types.insert(conv_temp.trim_start_matches('%').to_string(), "ptr".to_string());
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
                    let is_string = right_var.starts_with('@') || 
                        (right_var.starts_with('%') && 
                         self.variable_types.get(right_var.trim_start_matches('%'))
                             .map(|t| t == "ptr").unwrap_or(false));
                    
                    if is_string {
                        right_var
                    } else {
                        // Convert to string
                        let conv_temp = self.generate_temp();
                        let right_type = if right_var.starts_with('%') {
                            self.variable_types.get(right_var.trim_start_matches('%'))
                                .map(|s| s.as_str()).unwrap_or("i64")
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
                        self.variable_types.insert(conv_temp.trim_start_matches('%').to_string(), "ptr".to_string());
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
                self.variable_types.insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

                Ok(temp)
            }
            AstNode::结构体声明(struct_decl) => {
                // Record struct definition for later type generation
                let field_types: Vec<String> = struct_decl.fields.iter()
                    .map(|field| {
                        // Convert Qi types to LLVM types
                        match &field.type_annotation {
                            crate::parser::ast::TypeNode::基础类型(bt) => {
                                match bt {
                                    crate::parser::ast::BasicType::整数 => "i64".to_string(),
                                    crate::parser::ast::BasicType::浮点数 => "double".to_string(),
                                    crate::parser::ast::BasicType::布尔 => "i1".to_string(),
                                    crate::parser::ast::BasicType::字符串 => "ptr".to_string(),
                                    _ => "i64".to_string(),
                                }
                            }
                            _ => "i64".to_string(),
                        }
                    })
                    .collect();
                
                // Also collect field names
                let field_names: Vec<String> = struct_decl.fields.iter()
                    .map(|field| field.name.clone())
                    .collect();
                
                self.struct_definitions.insert(struct_decl.name.clone(), field_types);
                self.struct_field_names.insert(struct_decl.name.clone(), field_names);
                Ok("".to_string())
            }
            AstNode::枚举声明(_enum_decl) => {
                // Enum declarations don't generate code directly
                // They just define the type for later use
                Ok("".to_string())
            }
            AstNode::结构体实例化表达式(struct_literal) => {
                // Create a temporary for the struct instance
                let temp = self.generate_temp();

                // Check if we're in a Future-returning function that returns this struct type
                // If so, we need to heap-allocate the struct instead of stack-allocating it
                let needs_heap_allocation = if let Some(ref ast_return_type) = self.current_function_ast_return_type {
                    match ast_return_type {
                        crate::parser::ast::TypeNode::未来类型(inner_type) => {
                            // Check if inner type matches this struct
                            match inner_type.as_ref() {
                                crate::parser::ast::TypeNode::自定义类型(type_name) => {
                                    type_name == &struct_literal.struct_name
                                }
                                crate::parser::ast::TypeNode::结构体类型(struct_type) => {
                                    &struct_type.name == &struct_literal.struct_name
                                }
                                _ => false,
                            }
                        }
                        _ => false,
                    }
                } else {
                    false
                };

                // Allocate memory for the struct
                let struct_type = format!("{}.type", struct_literal.struct_name);
                if needs_heap_allocation {
                    // Heap allocation using malloc
                    eprintln!("[HEAP-ALLOC] Heap-allocating struct {} in Future-returning function", struct_literal.struct_name);
                    // Call malloc with the size of the struct
                    // Get struct size (assuming each field is i64 for now, which is 8 bytes)
                    let field_count = struct_literal.fields.len();
                    let struct_size = field_count * 8;  // i64 = 8 bytes
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(temp.clone()),
                        callee: "malloc".to_string(),
                        arguments: vec![struct_size.to_string()],
                    });
                    // IMPORTANT: Record both pointer type and struct type for this variable
                    // This is needed for getelementptr to work correctly
                    let temp_var = temp.trim_start_matches('%');
                    self.variable_types.insert(temp_var.to_string(), "ptr".to_string());
                    self.variable_struct_types.insert(temp_var.to_string(), struct_literal.struct_name.clone());
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

                    // Store the field value
                    self.add_instruction(IrInstruction::存储 {
                        target: field_ptr,
                        value: field_value,
                        value_type: None, // Type will be inferred
                    });
                }

                // Record that this is a pointer type
                self.variable_types.insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());
                // Record the struct type for field access
                self.variable_struct_types.insert(temp.trim_start_matches('%').to_string(), struct_literal.struct_name.clone());

                Ok(temp)
            }
            AstNode::字段访问表达式(field_access) => {
                // Check if this is a module access (module.function) or struct field access (obj.field)
                match &*field_access.object {
                    AstNode::标识符表达式(ident) => {
                        // This could be a module access: module.function
                        let module_name = &ident.name;

                        // Check if this is a known import alias or module
                        if let Some(actual_module) = self.import_aliases.get(module_name).or_else(|| {
                            // If no alias, check if it's a direct module name
                            // Always treat identifier.field as module access if we have an alias
                            // Otherwise, only treat as module access if it's not a known variable
                            if self.import_aliases.contains_key(module_name) || !self.variable_types.contains_key(module_name) {
                                Some(module_name)
                            } else {
                                None
                            }
                        }) {
                            // This is module access: generate a function call with module qualifier
                            let qualified_function_name = format!("{}_{}", actual_module, field_access.field);

                            // Use the existing function call mechanism by creating a function call expression
                            let func_call = AstNode::函数调用表达式(crate::parser::ast::FunctionCallExpression {
                                module_qualifier: Some(actual_module.clone()),
                                callee: field_access.field.clone(),
                                arguments: vec![],
                                span: Default::default(),  // Use default span
                            });

                            // Build the function call
                            self.build_node(&func_call)
                        } else {
                            // This is struct field access
                            let object_var = self.build_node(&field_access.object)?;

                            // Get the struct type from variable_struct_types
                            let object_var_name = object_var.trim_start_matches('%');
                            let struct_type = self.variable_struct_types.get(object_var_name)
                                .cloned()
                                .unwrap_or_else(|| "unknown".to_string());

                            // Generate field access instruction
                            let temp = self.generate_temp();
                            self.add_instruction(IrInstruction::字段访问 {
                                dest: temp.clone(),
                                object: object_var,
                                field: field_access.field.clone(),
                                struct_type,
                            });

                            // Load the field value
                            let load_temp = self.generate_temp();
                            self.add_instruction(IrInstruction::加载 {
                                dest: load_temp.clone(),
                                source: temp,
                                load_type: None,
                            });

                            Ok(load_temp)
                        }
                    }
                    _ => {
                        // This is definitely struct field access (complex expression)
                        let object_var = self.build_node(&field_access.object)?;

                        // Get the struct type from variable_struct_types
                        let object_var_name = object_var.trim_start_matches('%');
                        let struct_type = self.variable_struct_types.get(object_var_name)
                            .cloned()
                            .unwrap_or_else(|| "unknown".to_string());

                        // Generate field access instruction
                        let temp = self.generate_temp();
                        self.add_instruction(IrInstruction::字段访问 {
                            dest: temp.clone(),
                            object: object_var,
                            field: field_access.field.clone(),
                            struct_type,
                        });

                        // Load the field value
                        let load_temp = self.generate_temp();
                        self.add_instruction(IrInstruction::加载 {
                            dest: load_temp.clone(),
                            source: temp,
                            load_type: None,
                        });

                        Ok(load_temp)
                    }
                }
            }
            AstNode::块语句(block_stmt) => {
                // Process all statements in the block
                for stmt in &block_stmt.statements {
                    self.build_node(stmt)?;
                }
                Ok("block".to_string())
            }
            AstNode::方法声明(method_decl) => {
                // Method is just a function with the receiver as the first parameter
                // Generate method name: TypeName_methodName
                let method_full_name = format!("{}_{}", method_decl.receiver_type, method_decl.method_name);
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
                    self.infer_return_type_from_body(&method_decl.body).unwrap_or_else(|| "void".to_string())
                };
                self.function_return_types.insert(func_name.clone(), return_type.clone());

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
                    format!("_Z_{}", self.mangle_function_name(&method_decl.receiver_name).trim_start_matches("_Z_"))
                } else {
                    method_decl.receiver_name.clone()
                };
                self.variable_types.insert(method_decl.receiver_name.clone(), receiver_type.to_string());
                self.variable_types.insert(receiver_mangled.clone(), receiver_type.to_string());
                self.variable_types.insert(format!("param_{}", method_decl.receiver_name), receiver_type.to_string());
                self.variable_types.insert(format!("param_{}", receiver_mangled), receiver_type.to_string());
                
                // Track receiver struct type for field access
                self.variable_struct_types.insert(method_decl.receiver_name.clone(), method_decl.receiver_type.clone());
                self.variable_struct_types.insert(receiver_mangled.clone(), method_decl.receiver_type.clone());
                
                // Other parameters
                for param in &method_decl.parameters {
                    let param_type = self.get_llvm_type(&param.type_annotation);
                    let mangled_param_name = if param.name.chars().any(|c| !c.is_ascii()) {
                        format!("_Z_{}", self.mangle_function_name(&param.name).trim_start_matches("_Z_"))
                    } else {
                        param.name.clone()
                    };
                    self.variable_types.insert(param.name.clone(), param_type.clone());
                    self.variable_types.insert(mangled_param_name.clone(), param_type.clone());
                    
                    // Mark as parameter (direct value, not pointer)
                    self.variable_types.insert(format!("param_{}", param.name), param_type.clone());
                    self.variable_types.insert(format!("param_{}", mangled_param_name), param_type.clone());
                }

                // Process method body
                for (i, stmt) in method_decl.body.iter().enumerate() {
                    self.build_node(stmt)?;
                }

                // Add default return if no explicit return
                if return_type == "void" {
                    self.add_instruction(IrInstruction::返回 { value: None });
                } else if !method_decl.body.iter().any(|stmt| matches!(stmt, AstNode::返回语句(_))) {
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

                Ok(format!("method_{}", func_name))
            }
            AstNode::方法调用表达式(method_call) => {
                // Check if this is a static method call (method_name starts with ::)
                if method_call.method_name.starts_with("::") {
                    // Static method call: Type::method(args)
                    // Extract type name and method name
                    if let AstNode::标识符表达式(type_ident) = &*method_call.object {
                        let type_name = &type_ident.name;
                        let method_name = &method_call.method_name[2..]; // Remove :: prefix

                        // Map known static methods to runtime functions
                        let runtime_func_name = match (type_name.as_str(), method_name) {
                            ("未来", "就绪") => "qi_future_ready_i64",
                            ("未来", "失败") => "qi_future_failed",
                            _ => {
                                return Err(format!("Unknown static method: {}::{}", type_name, method_name).into());
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
                        self.variable_types.insert(temp.trim_start_matches('%').to_string(), "ptr".to_string());

                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(temp.clone()),
                            callee: runtime_func_name.to_string(),
                            arguments: args,
                        });

                        return Ok(temp);
                    } else {
                        return Err("Static method call must have a type name as the object".to_string().into());
                    }
                }

                // 检查是否为模块前缀调用（object 是标识符且不是变量）
                if let AstNode::标识符表达式(ident) = &*method_call.object {
                    // 检查是否为已知变量（排除模块名）
                    let is_module = self.import_aliases.contains_key(&ident.name) || 
                                   self.import_aliases.values().any(|v| v == &ident.name);
                    let is_variable = self.variable_types.contains_key(&ident.name) ||
                                     self.variable_types.contains_key(&self.mangle_function_name(&ident.name));
                    
                    if is_module || !is_variable {
                        // 这是模块前缀调用，如 加密.MD5哈希()
                        let module_name = &ident.name;

                        // 检查是否为导入的模块
                        if self.import_aliases.contains_key(module_name) {
                            let module_path = self.import_aliases.get(module_name).unwrap();

                            // 检查是否为标准库模块（在ModuleRegistry中）
                            let is_stdlib = self.module_registry.has_module(module_path);

                            if is_stdlib {
                                // 标准库模块：验证函数是否存在并获取运行时函数名和返回类型
                                let (runtime_func_name, return_type) = {
                                    let module_function = self.check_module_function_available(
                                        module_name,
                                        &method_call.method_name
                                    )?;
                                    (module_function.runtime_name.clone(), module_function.return_type.clone())
                                };

                                // 构建参数
                                let mut args = vec![];
                                for arg in &method_call.arguments {
                                    args.push(self.build_node(arg)?);
                                }

                                // 生成临时变量并记录其类型
                                let temp = self.generate_temp();
                                self.variable_types.insert(temp.clone(), return_type);

                                self.add_instruction(IrInstruction::函数调用 {
                                    dest: Some(temp.clone()),
                                    callee: runtime_func_name,
                                    arguments: args,
                                });
                                return Ok(temp);
                            } else {
                                // 用户包：直接使用mangled函数名，跳过验证
                                let func_name = if method_call.method_name.chars().any(|c| !c.is_ascii()) {
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
                            // 这是本地模块，使用模块前缀
                            // 模块前缀调用，如 数学工具.最大值 -> 数学工具_最大值
                            let actual_module = self.import_aliases.get(module_name).unwrap_or(module_name);
                            let qualified_function_name = format!("{}_{}", actual_module, method_call.method_name);

                            // 构造函数调用
                            let func_name = if qualified_function_name.chars().any(|c| !c.is_ascii()) {
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
                            let has_return_value = if let Some(ret_type) = self.function_return_types.get(&func_name) {
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
                
                // 这是真正的方法调用：object.method(args)
                // 1. Get the object
                let object_var = self.build_node(&method_call.object)?;
                
                // 2. Get the struct type
                let object_var_name = object_var.trim_start_matches('%');
                let struct_type = self.variable_struct_types.get(object_var_name)
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
                // Check if the method has a return value
                let has_return_value = if let Some(ret_type) = self.function_return_types.get(&func_name) {
                    ret_type != "void"
                } else {
                    // Unknown - assume it has a return value
                    true
                };
                
                if has_return_value {
                    let temp = self.generate_temp();
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
                            eprintln!("[DEBUG] Goroutine argument built: {:?} -> {}", arg, arg_result);
                            arg_temps.push(arg_result);
                        }

                        // Resolve argument types NOW while we have access to variable_types
                        // Look up the target function's parameter types
                        let typed_args: Vec<String> = if let Some(param_types) = self.function_param_types.get(&mangled_name) {
                            eprintln!("[DEBUG] Found param types for {}: {:?}", mangled_name, param_types);
                            arg_temps.iter().zip(param_types.iter()).map(|(arg, expected_type)| {
                                let arg_type = if arg.starts_with('%') {
                                    let var_name = arg.trim_start_matches('%');
                                    let resolved_type = self.variable_types.get(var_name)
                                        .or_else(|| self.variable_types.get(&format!("param_{}", var_name)))
                                        .map(|s| s.as_str())
                                        .unwrap_or(expected_type.as_str());
                                    eprintln!("[DEBUG] Resolving arg {} (var {}): resolved as {}", arg, var_name, resolved_type);
                                    resolved_type
                                } else if arg.starts_with('@') {
                                    "ptr"
                                } else if arg.parse::<i64>().is_ok() {
                                    "i64"
                                } else {
                                    expected_type.as_str()
                                };
                                let typed = format!("{}:{}", arg_type, arg);
                                eprintln!("[DEBUG] Typed arg: {}", typed);
                                typed
                            }).collect()
                        } else {
                            eprintln!("[DEBUG] No param types found for {}, using raw args", mangled_name);
                            // No type info available, pass through as-is
                            arg_temps
                        };

                        // Generate goroutine spawn call
                        eprintln!("[DEBUG] Spawning goroutine {} with {} arguments: {:?}", mangled_name, typed_args.len(), typed_args);
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
                    self.build_node(size_expr).unwrap_or_else(|_| "0".to_string())
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

                self.add_instruction(IrInstruction::通道发送 {
                    channel,
                    value,
                });

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
                // Build each select case
                let mut cases = Vec::new();

                for case in &select_expr.cases {
                    let case_label = self.generate_label();

                    match &case.kind {
                        crate::parser::ast::SelectCaseKind::通道接收 { channel, variable } => {
                            let channel_expr = self.build_node(channel)?;
                            let dest_temp = self.generate_temp();
                            cases.push(SelectCase {
                                operation_type: SelectOperationType::接收,
                                channel: channel_expr,
                                value: None,
                                dest: Some(dest_temp),
                                label: case_label.clone(),
                            });
                        }
                        crate::parser::ast::SelectCaseKind::通道发送 { channel, value } => {
                            let channel_expr = self.build_node(channel)?;
                            let value_expr = self.build_node(value)?;
                            cases.push(SelectCase {
                                operation_type: SelectOperationType::发送,
                                channel: channel_expr,
                                value: Some(value_expr),
                                dest: None,
                                label: case_label.clone(),
                            });
                        }
                        crate::parser::ast::SelectCaseKind::默认 => {
                            // Default case handled separately
                        }
                    }
                }

                // Generate select instruction
                self.add_instruction(IrInstruction::选择语句 {
                    cases: cases.clone(),
                    default_case: None, // TODO: Handle default case
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

            _ => {
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
                        self.variable_types.insert(temp_var_name.to_string(), "i1".to_string());
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
                        let escaped_str = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\0A");
                        self.add_instruction(IrInstruction::字符串常量 {
                            name: format!("{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1",
                                str_name, s.len() + 1, escaped_str),
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
            AstNode::标识符表达式(ident_expr) => {
                // For identifiers, we need to load the value and ensure it's a pointer
                let var_name = format!("%{}", ident_expr.name);
                let temp = self.generate_temp();

                // Load the value from the variable
                self.add_instruction(IrInstruction::加载 {
                    dest: temp.clone(),
                    source: var_name.clone(),
                    load_type: None,
                });

                // Allocate storage for the value copy
                let temp_copy = self.generate_temp();
                let var_type = self.variable_types.get(&ident_expr.name)
                    .unwrap_or(&"i64".to_string())
                    .clone();

                self.add_instruction(IrInstruction::分配 {
                    dest: temp_copy.clone(),
                    type_name: var_type,
                });

                // Store the loaded value
                self.add_instruction(IrInstruction::存储 {
                    target: temp_copy.clone(),
                    value: temp,
                    value_type: None,
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
                    crate::parser::ast::BasicType::数组 => "ptr".to_string(),  // Simplified for now
                    crate::parser::ast::BasicType::字典 => "ptr".to_string(), // Simplified for now
                    crate::parser::ast::BasicType::列表 => "ptr".to_string(),  // Simplified for now
                    crate::parser::ast::BasicType::集合 => "ptr".to_string(),  // Simplified for now
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
        ir.push_str("target datalayout = \"e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32\"\n");
        ir.push_str("target triple = \"arm64-apple-macosx26.0.0\"\n\n");

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
        ir.push_str("declare ptr @e5_88_9b_e5_bb_ba_e9_80_9a_e9_81_93(i64)\n");  // 创建通道
        ir.push_str("declare i32 @e5_8f_91_e9_80_81_int(ptr, i64)\n");       // 发送
        ir.push_str("declare i64 @e6_a5_a5_e6_8e_af_int(ptr)\n");           // 接收
        ir.push_str("declare i32 @e5_85_b3_e9_97_ad_e9_80_9a_e9_81_93(ptr)\n"); // 关闭通道
        ir.push_str("\n");

        ir.push_str("declare ptr @e5_88_9b_e5_bb_ba_e7_ad_89_e5_be_85_e7_bb_84()\n"); // 创建等待组
        ir.push_str("declare i32 @e6_8b_89_e5_a0_80_e7_ad_89_e5_be_85(ptr, i32)\n"); // 添加等待
        ir.push_str("declare i32 @e7_ad_89_e5_be_85(ptr)\n");                     // 等待
        ir.push_str("declare i32 @e5_ae_8c_e6_88_90(ptr)\n");                     // 完成
        ir.push_str("\n");

        ir.push_str("declare ptr @e5_88_9b_e5_bb_ba_e4_ba_92_e6_96_a5_e9_94_81()\n"); // 创建互斥锁
        ir.push_str("declare i32 @e5_8a_a0_e9_94_81(ptr)\n");                      // 加锁
        ir.push_str("declare i32 @e8_a3_a3_e9_94_81(ptr)\n");                      // 解锁
        ir.push_str("declare i32 @e5_b0_9d_e8_af_95_e5_8a_a0_e9_94_81(ptr)\n");   // 尝试加锁
        ir.push_str("\n");

        ir.push_str("declare i64 @e8_b7_a5_e5_8f_96_e9_97_b4_e9_97_b4()\n");       // 获取时间
        ir.push_str("declare i32 @e8_ae_bd_e7_ba_ae_e8_b6_85_e6_97_b6(i64)\n");   // 设置超时
        ir.push_str("declare i32 @e6_8f_a5_e6_9f_a5_e8_b6_85_e6_97_b6(i64)\n");   // 检查超时
        ir.push_str("declare ptr @e5_88_9b_e5_bb_ba_e5_b0_a8_e6_97_b6_e5_99_a8(i64)\n"); // 创建定时器
        ir.push_str("declare i32 @e9_87_8d_e8_af_95_e6_93_8d_e4_bd_9c(i32, i32, i32)\n"); // 重试操作
        ir.push_str("\n");

        // Goroutine spawn functions
        ir.push_str("; Goroutine spawn functions\n");
        ir.push_str("declare void @qi_runtime_spawn_goroutine(ptr)\n");
        ir.push_str("declare void @qi_runtime_spawn_goroutine_with_args(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_runtime_select(ptr)\n");
        ir.push_str("declare void @qi_runtime_timer_cancel(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_retry(ptr, i32)\n");
        ir.push_str("declare i32 @qi_runtime_catch_error(ptr)\n");
        ir.push_str("\n");

        // Crypto functions
        ir.push_str("; Crypto functions\n");
        ir.push_str("declare ptr @qi_crypto_md5(ptr)\n");
        ir.push_str("declare ptr @qi_crypto_sha256(ptr)\n");
        ir.push_str("declare ptr @qi_crypto_sha512(ptr)\n");
        ir.push_str("declare ptr @qi_crypto_base64_encode(ptr)\n");
        ir.push_str("declare ptr @qi_crypto_base64_decode(ptr)\n");
        ir.push_str("declare ptr @qi_crypto_hmac_sha256(ptr, ptr)\n");
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

        // Network functions
        ir.push_str("; Network functions\n");
        ir.push_str("declare i64 @qi_network_tcp_connect(ptr, i16, i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_read(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_write(i64, ptr, i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_close(i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_flush(i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_bytes_read(i64)\n");
        ir.push_str("declare i64 @qi_network_tcp_bytes_written(i64)\n");
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
        ir.push_str("declare ptr @qi_http_put(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_http_delete(ptr)\n");
        ir.push_str("declare i64 @qi_http_request_create(ptr, ptr)\n");
        ir.push_str("declare i64 @qi_http_request_set_header(i64, ptr, ptr)\n");
        ir.push_str("declare i64 @qi_http_request_set_body(i64, ptr)\n");
        ir.push_str("declare i64 @qi_http_request_set_timeout(i64, i64)\n");
        ir.push_str("declare ptr @qi_http_request_execute(i64)\n");
        ir.push_str("declare i64 @qi_http_get_status(ptr)\n");
        ir.push_str("declare void @qi_http_free_string(ptr)\n");
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
        ir.push_str("\n");
        
        ir.push_str("; String operations\n");
        ir.push_str("declare i64 @qi_runtime_string_length(ptr)\n");
        ir.push_str("declare ptr @qi_runtime_string_concat(ptr, ptr)\n");
        ir.push_str("declare ptr @qi_runtime_string_slice(ptr, i64, i64)\n");
        ir.push_str("declare i32 @qi_runtime_string_compare(ptr, ptr)\n");
        ir.push_str("declare void @qi_runtime_free_string(ptr)\n");
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
            "qi_channel_create", "qi_channel_send_int", "qi_channel_receive_int", "qi_channel_close",
            "qi_waitgroup_create", "qi_waitgroup_add", "qi_waitgroup_wait", "qi_waitgroup_done",
            "qi_mutex_create", "qi_mutex_lock", "qi_mutex_unlock", "qi_mutex_trylock",
            "qi_get_time_ms", "qi_set_timeout", "qi_check_timeout", "qi_timer_create",
            "qi_timer_expired", "qi_timer_stop",
            "e5_88_9b_e5_bb_ba_e9_80_9a_e9_81_93", "e5_8f_91_e9_80_81_int", "e6_a5_a5_e6_8e_af_int", "e5_85_b3_e9_97_ad_e9_80_9a_e9_81_93",
            "e5_88_9b_e5_bb_ba_e7_ad_89_e5_be_85_e7_bb_84", "e6_8b_89_e5_a0_80_e7_ad_89_e5_be_85", "e7_ad_89_e5_be_85", "e5_ae_8c_e6_88_90",
            "e5_88_9b_e5_bb_ba_e4_ba_92_e6_96_a5_e9_94_81", "e5_8a_a0_e9_94_81", "e8_a3_a3_e9_94_81", "e5_b0_9d_e8_af_95_e5_8a_a0_e9_94_81",
            "e8_b7_a5_e5_8f_96_e9_97_b4_e9_97_b4", "e8_ae_bd_e7_ba_ae_e8_b6_85_e6_97_b6", "e6_8f_a5_e6_9f_a5_e8_b6_85_e6_97_b6",
            "e5_88_9b_e5_bb_ba_e5_b0_a8_e6_97_b6_e5_99_a8", "e9_87_8d_e8_af_95_e6_93_8d_e4_bd_9c",
            // Future type functions
            "qi_future_ready_i64", "qi_future_await_i64",
            "qi_future_ready_f64", "qi_future_await_f64",
            "qi_future_ready_bool", "qi_future_await_bool",
            "qi_future_ready_string", "qi_future_await_string",
            "qi_future_ready_ptr", "qi_future_await_ptr",
            "qi_future_failed", "qi_future_is_completed", "qi_future_free", "qi_string_free",
            // Memory allocation
            "malloc", "free", "strlen"
        ]);

        if !self.external_functions.is_empty() {
            ir.push_str("; External function declarations from imported modules\n");
            for (func_name, (param_types, return_type)) in &self.external_functions {
                if !already_declared.contains(func_name.as_str()) {
                    let params_str = param_types.iter()
                        .enumerate()
                        .map(|(i, ty)| format!("{} %{}", ty, i))
                        .collect::<Vec<_>>()
                        .join(", ");
                    ir.push_str(&format!("declare {} @{}({})\n", return_type, func_name, params_str));
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
        let has_main_function = other_instructions.iter().any(|instruction| {
            match instruction {
                IrInstruction::标签 { name } => {
                    name.contains("@main") || name.contains("define.*@main")
                }
                _ => false,
            }
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
                IrInstruction::分配 { dest, type_name } => {
                    let mangled_type = self.mangle_type_name(type_name);

                    // Smart allocation: use heap for large types, stack for small types
                    if self.is_small_type(type_name) {
                        // Small type: use stack allocation (original behavior)
                        ir.push_str(&format!("  {} = alloca {}, align {}\n", dest, mangled_type, self.get_type_alignment(type_name)));
                    } else {
                        // Large or complex type: could use heap allocation
                        // For now, keep stack allocation for compatibility, but this is where
                        // we could switch to heap for structs, arrays, etc.
                        ir.push_str(&format!("  {} = alloca {}, align {}\n", dest, mangled_type, self.get_type_alignment(type_name)));

                        // Future enhancement: detect large structs and use heap
                        // let type_size = self.estimate_type_size(type_name);
                        // if type_size > 1024 { /* use heap */ }
                    }
                }
                IrInstruction::存储 { target, value, value_type } => {
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
                        self.variable_types.get(var_name)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "i64".to_string())
                    } else if value == "0" || value == "1" {
                        // These could be boolean values - prefer i1 for boolean constants
                        "i1".to_string()
                    } else if value.parse::<i64>().is_ok() {
                        "i64".to_string()
                    } else {
                        // Default to i64 for unknown values
                        "i64".to_string()
                    };
                    ir.push_str(&format!("store {} {}, ptr {}\n", inferred_type, value, target));
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
                IrInstruction::加载 { dest, source, load_type } => {
                    // Use explicit load type if provided, otherwise infer
                    let inferred_type = if let Some(ref lt) = load_type {
                        lt.as_str()
                    } else if source.starts_with('@') && source.contains(".str") {
                        // Check if we're loading a string constant (starts with @ and contains .str)
                        "ptr"
                    } else if source.starts_with('%') {
                        // Look up variable type from our tracking for variables
                        // Remove the % prefix to get the original variable name
                        let var_name = source.trim_start_matches('%');

                        // NOTE: We used to check for param_ here and skip the load, but this caused issues
                        // because variable_types accumulates state across multiple functions.
                        // If a load instruction was generated, we should trust it and emit the load.
                        // The decision of whether to load or not should be made earlier, in build_node.

                        // Look up the variable type from our tracking (we store both original and mangled names)
                        self.variable_types.get(var_name).map(|s| s.as_str()).unwrap_or("i64")
                    } else {
                        // Default to i64 for most variables
                        "i64"
                    };
                    ir.push_str(&format!("{} = load {}, ptr {}\n", dest, inferred_type, source));
                }
                IrInstruction::二元操作 { dest, left, operator, right, operand_type } => {
                    // Use the operand_type that was determined when creating the instruction
                    let is_float = operand_type.contains("double") || operand_type.contains("float");
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
                            crate::parser::ast::BinaryOperator::加 => ("add", operand_type.as_str()),
                            crate::parser::ast::BinaryOperator::减 => ("sub", operand_type.as_str()),
                            crate::parser::ast::BinaryOperator::乘 => ("mul", operand_type.as_str()),
                            crate::parser::ast::BinaryOperator::除 => ("sdiv", operand_type.as_str()),
                            crate::parser::ast::BinaryOperator::取余 => ("srem", operand_type.as_str()),
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
                    let type_for_instruction = if op_str.starts_with("icmp") || op_str.starts_with("fcmp") {
                        operand_type.as_str()
                    } else {
                        return_type
                    };

                    ir.push_str(&format!("{} = {} {} {}, {}\n", dest, op_str, type_for_instruction, normalized_left, normalized_right));
                }
                IrInstruction::函数调用 { dest, callee, arguments } => {
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
                                let vty = self.variable_types.get(var_name)
                                    .or_else(|| self.variable_types.get(&format!("param_{}", var_name)))
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
                                ir.push_str(&format!("{} = call i32 (ptr, ...) @{}({})\n", dest_var, callee, args_str));
                            }
                            None => {
                                ir.push_str(&format!("call i32 (ptr, ...) @{}({})\n", callee, args_str));
                            }
                        }
                    } else {
                        // Regular function call - determine argument types and return type
                        let mut typed_args = Vec::new();

                        // Get the expected parameter types from function signature if available
                        let mangled_callee = self.mangle_function_name(&callee);
                        let expected_param_types = if let Some(param_types) = self.function_param_types.get(&mangled_callee as &str) {
                            Some(param_types.clone())
                        } else if let Some((param_types, _)) = self.external_functions.get(&callee as &str) {
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
                                let current_arg_type = self.variable_types.get(arg_var_name)
                                    .map(|s| s.as_str())
                                    .unwrap_or("i64");

                                // Determine expected type for this parameter
                                let expected_type = if let Some(ref param_types) = expected_param_types {
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
                                    } else if callee == "qi_future_ready_ptr" || callee == "qi_future_await_ptr" {
                                        "ptr"
                                    } else if callee == "qi_runtime_print_int" || callee == "qi_runtime_println_int" {
                                        "i64"
                                    } else if callee == "qi_runtime_print_float" || callee == "qi_runtime_println_float" ||
                                              callee == "qi_runtime_float_to_string" {
                                        "double"
                                    } else if callee == "qi_runtime_print_bool" || callee == "qi_runtime_println_bool" {
                                        "i32"
                                    } else if callee.contains("concat") || callee.contains("read_string") || callee.contains("file") {
                                        "ptr"
                                    } else if callee == "qi_runtime_waitgroup_add" {
                                        // waitgroup_add(ptr, i32)
                                        if i == 0 { "ptr" } else { "i32" }
                                    } else if callee == "qi_runtime_waitgroup_create" ||
                                              callee == "qi_runtime_waitgroup_wait" ||
                                              callee == "qi_runtime_waitgroup_done" ||
                                              callee == "qi_runtime_mutex_create" ||
                                              callee == "qi_runtime_mutex_lock" ||
                                              callee == "qi_runtime_mutex_unlock" ||
                                              callee == "qi_runtime_mutex_trylock" {
                                        // All these take ptr as first parameter
                                        "ptr"
                                    } else if callee == "qi_runtime_channel_send" {
                                        // channel_send(ptr, i64)
                                        if i == 0 { "ptr" } else { "i64" }
                                    } else if callee == "qi_runtime_channel_receive" ||
                                              callee == "qi_runtime_channel_close" {
                                        // channel operations take ptr
                                        "ptr"
                                    } else if callee == "qi_runtime_create_channel" {
                                        // create_channel(i64) -> ptr
                                        "i64"
                                    } else {
                                        current_arg_type // Use the variable's actual type
                                    }
                                };

                                // Special handling for boolean arguments: if we expect a boolean but get a variable,
                                // ensure the variable is actually a boolean type
                                let final_expected_type = if expected_type == "i1" && current_arg_type != "i1" {
                                    // Check if this variable was originally a boolean literal
                                    // Look for any indication that this should be boolean
                                    if let Some(temp_val) = self.variable_types.get(arg_var_name) {
                                        if temp_val == "i1" {
                                            "i1"
                                        } else {
                                            expected_type  // Keep original expectation
                                        }
                                    } else {
                                        expected_type  // Keep original expectation
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
                                    if let Some(bool_val) = self.variable_types.get(&format!("bool_{}", arg_var_name)) {
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
                                        if let Some(raw_val) = self.variable_types.get(&format!("raw_{}", arg_var_name)) {
                                            if raw_val == "1" {
                                                // Raw value is 1, should be true
                                                let true_temp = format!("%true_raw_{}", arg_var_name);
                                                ir.push_str(&format!("{} = add i1 0, 1\n", true_temp));
                                                fixed_arg = Some(format!("i1 {}", true_temp));
                                            } else if raw_val == "0" {
                                                // Raw value is 0, should be false
                                                let false_temp = format!("%false_raw_{}", arg_var_name);
                                                ir.push_str(&format!("{} = add i1 0, 0\n", false_temp));
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
                                    let was_defaulted = !self.variable_types.contains_key(arg_var_name);
                                    let conv_temp = format!("%conv{}_{}", arg_var_name, i);

                                    if was_defaulted && current_arg_type == "i64" {
                                        // Type was defaulted, trust expected type directly
                                        typed_args.push(format!("{} {}", final_expected_type, arg));
                                    } else if current_arg_type == "i64" && final_expected_type == "double" {
                                        // Convert i64 to double
                                        ir.push_str(&format!("{} = sitofp i64 {} to double\n", conv_temp, arg));
                                        typed_args.push(format!("double {}", conv_temp));
                                    } else if current_arg_type == "double" && final_expected_type == "i64" {
                                        // Convert double to i64
                                        ir.push_str(&format!("{} = fptosi double {} to i64\n", conv_temp, arg));
                                        typed_args.push(format!("i64 {}", conv_temp));
                                    } else if current_arg_type == "i1" && final_expected_type == "i64" {
                                        // Convert bool to i64
                                        ir.push_str(&format!("{} = zext i1 {} to i64\n", conv_temp, arg));
                                        typed_args.push(format!("i64 {}", conv_temp));
                                    } else if current_arg_type == "i1" && final_expected_type == "i32" {
                                        // Convert bool to i32 (for qi_runtime_println_bool and similar functions)
                                        ir.push_str(&format!("{} = zext i1 {} to i32\n", conv_temp, arg));
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
                                let expected_type = if let Some(ref param_types) = expected_param_types {
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
                        let final_callee = if callee == "qi_runtime_print" || callee == "qi_runtime_println" {
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
                            if let Some((_, ret_ty)) = self.external_functions.get(&callee as &str) {
                                ret_ty.as_str()
                            } else {
                                // Fallback for future functions
                                match callee.as_str() {
                                    "qi_future_ready_i64" | "qi_future_failed" => "ptr",
                                    "qi_future_await_i64" => "i64",
                                    "qi_future_is_completed" => "i32",
                                    "qi_future_free" => "void",
                                    _ => "ptr"
                                }
                            }
                        } else if callee.starts_with("qi_runtime_") {
                            // Create functions return ptr - MUST BE FIRST
                            if callee == "qi_runtime_create_channel" || callee == "qi_runtime_waitgroup_create" ||
                               callee == "qi_runtime_mutex_create" || callee == "qi_runtime_rwlock_create" ||
                               callee == "qi_runtime_condvar_create" || callee == "qi_runtime_once_create" ||
                               callee == "qi_runtime_timer_create" {
                                "ptr"
                            // Math functions return double
                            } else if callee.contains("math_sqrt") || callee.contains("math_pow") ||
                               callee.contains("math_sin") || callee.contains("math_cos") ||
                               callee.contains("math_tan") || callee.contains("math_floor") ||
                               callee.contains("math_ceil") || callee.contains("math_round") ||
                               callee.contains("math_abs_float") || callee.contains("int_to_float") ||
                               callee.contains("string_to_float") {
                                "double"
                            // String length returns i64, not ptr
                            } else if callee.contains("string_length") {
                                "i64"
                            // String functions return ptr
                            } else if callee.contains("string") || callee.contains("concat") ||
                                      callee.contains("read_string") || callee.contains("int_to_string") ||
                                      callee.contains("float_to_string") {
                                "ptr"
                            // Channel receive returns i64 (the actual value)
                            } else if callee == "qi_runtime_channel_receive" {
                                "i64"
                            // Timer functions that return i64
                            } else if callee == "qi_runtime_set_timeout" || callee == "qi_runtime_timer_expired" ||
                                      callee == "qi_runtime_timer_stop" || callee == "qi_runtime_get_time_ms" {
                                "i64"
                            // All other synchronization functions return i32 (status)
                            } else if callee.contains("waitgroup") || callee.contains("mutex") || callee.contains("timer") ||
                                      callee.contains("rwlock") || callee.contains("condvar") || callee.contains("once") ||
                                      callee.contains("channel") || callee == "qi_runtime_check_timeout" {
                                "i32"
                            // Integer math functions return i64
                            } else if callee.contains("math_abs_int") || callee.contains("float_to_int") ||
                                      callee.contains("string_to_int") || callee.contains("array_length") {
                                "i64"
                            } else {
                                "i32"
                            }
                        } else if callee == "qi_runtime_string_concat" {
                            "ptr"
                        // Crypto functions return ptr (string)
                        } else if callee.starts_with("qi_crypto_") && callee != "qi_crypto_free_string" {
                            "ptr"
                        // IO functions - check return type based on function name
                        } else if callee.starts_with("qi_io_") {
                            match callee.as_str() {
                                "qi_io_read_file" => "ptr",  // 读取文件 returns string
                                "qi_io_file_size" | "qi_io_write_file" | "qi_io_append_file" |
                                "qi_io_delete_file" | "qi_io_create_file" | "qi_io_file_exists" |
                                "qi_io_create_dir" | "qi_io_delete_dir" => "i64",  // These return i64
                                "qi_io_free_string" => "void",  // Cleanup function
                                _ => "i64"  // Default for unknown IO functions
                            }
                        // Network functions - check return type based on function name
                        } else if callee.starts_with("qi_network_") {
                            match callee.as_str() {
                                "qi_network_resolve_host" | "qi_network_get_local_ip" => "ptr",  // Return strings
                                "qi_network_tcp_connect" | "qi_network_tcp_read" | "qi_network_tcp_write" |
                                "qi_network_tcp_close" | "qi_network_tcp_flush" | "qi_network_tcp_bytes_read" |
                                "qi_network_tcp_bytes_written" | "qi_network_port_available" => "i64",  // Return i64
                                "qi_network_free_string" => "void",  // Cleanup function
                                _ => "i64"  // Default for unknown network functions
                            }
                        // HTTP functions - check return type based on function name
                        } else if callee.starts_with("qi_http_") {
                            match callee.as_str() {
                                "qi_http_get" | "qi_http_post" | "qi_http_put" | "qi_http_delete" |
                                "qi_http_request_execute" => "ptr",  // Return response strings
                                "qi_http_init" | "qi_http_request_create" | "qi_http_request_set_header" |
                                "qi_http_request_set_body" | "qi_http_request_set_timeout" |
                                "qi_http_get_status" => "i64",  // Return i64
                                "qi_http_free_string" => "void",  // Cleanup function
                                _ => "i64"  // Default for unknown HTTP functions
                            }
                        // Check hex-encoded Chinese function names
                        } else if callee == "e6_b1_82_e5_b9_b3_e6_96_b9_e6_a0_b9" { // 求平方根
                            "double"
                        } else if callee == "e6_b1_82_e7_bb_9d_e5_af_b9_e5_80_bc" { // 求绝对值
                            "i64"
                        } else if callee == "e5ad_97_e7_ac_a6_e9_95_bf" { // 字符串长度
                            "i64"
                        } else {
                            // Check if this is a known async function
                            if self.async_function_types.contains_key(callee) {
                                "ptr"
                            } else if let Some(ret_type) = self.function_return_types.get(&self.mangle_function_name(&callee) as &str) {
                                ret_type  // Use stored return type from function signature
                            } else if let Some((_param_types, ret_type)) = self.external_functions.get(callee) {
                                ret_type.as_str()  // Use return type from external function signature
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
                            ir.push_str(&format!("{} = call i32 @{}({}, ptr {})\n", temp_status, callee, typed_args[0], received_ptr));
                            ir.push_str(&format!("{} = load ptr, ptr {}\n", temp_ptr, received_ptr));
                            if let Some(dest_var) = dest {
                                ir.push_str(&format!("{} = load i64, ptr {}\n", dest_var, temp_ptr));
                            }
                        } else {
                            match dest {
                                Some(dest_var) => {
                                    ir.push_str(&format!("{} = call {} @{}({})\n", dest_var, ret_type, final_callee, args_str));
                                }
                                None => {
                                    ir.push_str(&format!("call void @{}({})\n", final_callee, args_str));
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
                            eprintln!("[DEBUG] Generating return: ret {} {}", ty, val);
                            ir.push_str(&format!("ret {} {}\n", ty, val));
                        }
                    } else {
                        // Default to i64 if not within a function context
                        eprintln!("[DEBUG] Generating return (default): ret i64 {}", val);
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
                        ir.push_str(&format!("{}\n", name));
                    } else if name == "}" {
                        ir.push_str("}\n");
                        // Reset current function return type at function end
                        current_function_ret_ty = None;
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
                IrInstruction::条件跳转 { condition, true_label, false_label } => {
                    ir.push_str(&format!("br i1 {}, label %{}, label %{}\n", condition, true_label, false_label));
                }
                IrInstruction::数组访问 { dest, array, index } => {
                    if array.starts_with('@') && array.contains(".str") {
                        // String constant access - use bitcast to i8* first, then getelementptr
                        ir.push_str(&format!("{} = getelementptr i8, i8* {}, i32 {}\n", dest, array, index));
                    } else {
                        // Regular array access using getelementptr
                        ir.push_str(&format!("{} = getelementptr [10 x i64], [10 x i64]* {}, i64 0, i64 {}\n", dest, array, index));
                    }
                }
                IrInstruction::数组分配 { dest, size } => {
                    // Smart array allocation: small arrays on stack, large arrays on heap
                    let array_size: usize = size.parse().unwrap_or(10);
                    const SMALL_ARRAY_THRESHOLD: usize = 64; // Arrays <= 64 elements use stack

                    if array_size <= SMALL_ARRAY_THRESHOLD {
                        // Small array: stack allocation
                        ir.push_str(&format!("  {} = alloca [{} x i64], align 8\n", dest, size));
                    } else {
                        // Large array: heap allocation with GC check
                        let bytes = array_size * 8; // i64 = 8 bytes
                        let (alloc_ir, ptr) = self.generate_allocation_with_gc_check(bytes, "i64", true);
                        ir.push_str(&alloc_ir);

                        // Record heap allocation for cleanup
                        self.record_allocation(AllocationInfo {
                            ptr: ptr.clone(),
                            size: bytes,
                            type_name: format!("[{} x i64]", size),
                            scope_level: self.scope_level,
                            is_heap: true,
                        });

                        // Alias the result
                        if ptr != *dest {
                            ir.push_str(&format!("  {} = bitcast ptr {} to [{} x i64]*\n", dest, ptr, size));
                        }
                    }
                }
                IrInstruction::数组存储 { array, index, value } => {
                    // Simplified array store - generate unique temp names using a hash of array and index
                    let hash = format!("{}{}", array.replace("%", "").replace("t", ""), index.replace("%", ""));
                    ir.push_str(&format!("%addr_tmp{} = getelementptr [10 x i64], [10 x i64]* {}, i64 0, i64 {}\n", hash, array, index));
                    ir.push_str(&format!("store i64 {}, i64* %addr_tmp{}\n", value, hash));
                }
                IrInstruction::字符串连接 { dest, left, right } => {
                    // Simplified string concatenation using external function
                    ir.push_str(&format!("{} = call ptr @qi_runtime_string_concat(ptr {}, ptr {})\n", dest, left, right));
                }
                IrInstruction::字段访问 { dest, object, field, struct_type } => {
                    // Get field index from struct field names
                    let field_index = if let Some(field_names) = self.struct_field_names.get(struct_type) {
                        // Find field index by name
                        field_names.iter().position(|f| f == field).unwrap_or(0)
                    } else {
                        0 // Unknown struct, use 0
                    };
                    
                    let mangled_type = self.mangle_type_name(&format!("{}.type", struct_type));
                    ir.push_str(&format!("{} = getelementptr {}, ptr {}, i32 0, i32 {}\n",
                        dest, mangled_type, object, field_index));
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
                    let is_future_type = self.variable_types.get(future_var)
                        .map(|t| t == "ptr")
                        .unwrap_or(false);

                    if is_future_type {
                        // This is a Future<T> type - determine the inner type and call appropriate await
                        let inner_type = self.future_inner_types.get(future_var)
                            .map(|s| s.as_str())
                            .unwrap_or("i64"); // Default to i64 if not tracked

                        let (await_func, call_return_type, final_type) = if inner_type.starts_with("struct.") {
                            // Struct type - use qi_future_await_ptr
                            ("qi_future_await_ptr", "ptr", "ptr")
                        } else {
                            match inner_type {
                                "i64" => ("qi_future_await_i64", "i64", "i64"),
                                "double" => ("qi_future_await_f64", "double", "double"),
                                "i1" => ("qi_future_await_bool", "i32", "i1"),  // bool await returns i32, convert to i1
                                "ptr" => ("qi_future_await_string", "ptr", "ptr"),  // string pointer
                                _ => ("qi_future_await_i64", "i64", "i64"),  // fallback
                            }
                        };

                        // Call the await function
                        if call_return_type == final_type {
                            // Direct call - no conversion needed
                            ir.push_str(&format!("{} = call {} @{}(ptr {})\n", dest, call_return_type, await_func, future));
                        } else {
                            // Need type conversion (bool case: i32 -> i1)
                            let temp_result = self.generate_temp();
                            ir.push_str(&format!("{} = call {} @{}(ptr {})\n", temp_result, call_return_type, await_func, future));
                            // Convert i32 to i1 by checking if != 0
                            ir.push_str(&format!("{} = icmp ne {} {}, 0\n", dest, call_return_type, temp_result));
                        }

                        // Record the final type of the dest variable for later use
                        let dest_var = dest.trim_start_matches('%');
                        self.variable_types.insert(dest_var.to_string(), final_type.to_string());
                        eprintln!("[AWAIT-EXPR] Recorded type for {}: {}", dest_var, final_type);
                    } else {
                        // This is an async coroutine - call qi_runtime_await
                        ir.push_str(&format!("{} = call ptr @qi_runtime_await(ptr {})\n", dest, future));
                    }
                }
                IrInstruction::创建异步任务 { dest, function, arguments } => {
                    // Create async task - pass function pointer and argument count
                    // Note: This is a simplified implementation. In a real async runtime,
                    // we would need to handle argument passing more carefully.
                    ir.push_str(&format!("{} = call ptr @qi_runtime_create_task(ptr @{}, i64 {})\n",
                        dest, function, arguments.len()));

                    // Spawn the task to start execution
                    ir.push_str(&format!("call i32 @qi_runtime_spawn_task(ptr {})\n", dest));
                }
                IrInstruction::协程启动 { function, arguments } => {
                    // For functions with arguments, generate a wrapper function and use generic spawn
                    if arguments.is_empty() {
                        // No arguments - use simple spawn
                        let temp1 = self.generate_temp();
                        ir.push_str(&format!("{} = ptrtoint ptr @{} to i64\n", temp1, function));
                        let temp2 = self.generate_temp();
                        ir.push_str(&format!("{} = inttoptr i64 {} to ptr\n", temp2, temp1));
                        ir.push_str(&format!("call void @qi_runtime_spawn_goroutine(ptr {})\n", temp2));
                    } else {
                        // Generate wrapper function name
                        let wrapper_name = format!("__goroutine_wrapper_{}_{}", function, self.label_counter);
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
                        wrapper_def.push_str(&format!("define void @{}(ptr %args) {{\n", wrapper_name));

                        // Load each argument from the array and call the target function
                        let mut call_args = Vec::new();
                        for (i, (arg_type, _)) in arg_types.iter().zip(&arg_values).enumerate() {
                            let arg_temp = format!("%arg{}", i);
                            let ptr_temp = format!("%argptr{}", i);

                            // Get pointer to array element: args[i]
                            wrapper_def.push_str(&format!("  {} = getelementptr i64, ptr %args, i32 {}\n", ptr_temp, i));

                            // Load the i64 value
                            wrapper_def.push_str(&format!("  {} = load i64, ptr {}\n", arg_temp, ptr_temp));

                            // Convert to appropriate type and add to call args
                            if arg_type == "ptr" {
                                let cast_temp = format!("%argcast{}", i);
                                wrapper_def.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", cast_temp, arg_temp));
                                call_args.push(format!("ptr {}", cast_temp));
                            } else {
                                call_args.push(format!("{} {}", arg_type, arg_temp));
                            }
                        }

                        // Call the actual function
                        wrapper_def.push_str(&format!("  call void @{}({})\n", function, call_args.join(", ")));
                        wrapper_def.push_str("  ret void\n");
                        wrapper_def.push_str("}\n");

                        // Store wrapper for later emission
                        self.goroutine_wrappers.push(wrapper_def);

                        // Allocate array for arguments
                        let args_array = format!("%goroutine_args_{}", self.temp_counter);
                        self.temp_counter += 1;
                        ir.push_str(&format!("{} = alloca [{}  x i64], align 8\n", args_array, arguments.len()));

                        // Store each argument value into the array
                        for (i, (arg_type, arg_value)) in arg_types.iter().zip(&arg_values).enumerate() {
                            let element_ptr = self.generate_temp();
                            ir.push_str(&format!("{} = getelementptr [{} x i64], ptr {}, i32 0, i32 {}\n",
                                element_ptr, arguments.len(), args_array, i));

                            // Convert to i64 if needed
                            if arg_type == "ptr" {
                                let as_int = self.generate_temp();
                                ir.push_str(&format!("{} = ptrtoint ptr {} to i64\n", as_int, arg_value));
                                ir.push_str(&format!("store i64 {}, ptr {}\n", as_int, element_ptr));
                            } else {
                                ir.push_str(&format!("store i64 {}, ptr {}\n", arg_value, element_ptr));
                            }
                        }

                        // Get wrapper function pointer
                        let wrapper_ptr1 = self.generate_temp();
                        ir.push_str(&format!("{} = ptrtoint ptr @{} to i64\n", wrapper_ptr1, wrapper_name));
                        let wrapper_ptr2 = self.generate_temp();
                        ir.push_str(&format!("{} = inttoptr i64 {} to ptr\n", wrapper_ptr2, wrapper_ptr1));

                        // Call qi_runtime_spawn_goroutine_with_args(wrapper, args)
                        ir.push_str(&format!("call void @qi_runtime_spawn_goroutine_with_args(ptr {}, ptr {})\n",
                            wrapper_ptr2, args_array));
                    }
                }
                IrInstruction::创建通道 { dest, channel_type, buffer_size } => {
                    // Create channel - generate runtime call
                    let size = buffer_size.as_ref().unwrap_or(&"0".to_string()).clone();
                    ir.push_str(&format!("{} = call ptr @qi_runtime_create_channel(i64 {})\n", dest, size));
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
                    ir.push_str(&format!("call i32 @qi_runtime_channel_send(ptr {}, i64 {})\n", channel, value_to_send));
                }
                IrInstruction::通道接收 { dest, channel } => {
                    // Receive value from channel using runtime
                    let received_ptr = self.generate_temp();
                    let status_temp = self.generate_temp();
                    let value_ptr_temp = self.generate_temp();
                    ir.push_str(&format!("{} = alloca ptr, align 8\n", received_ptr));
                    ir.push_str(&format!("{} = call i32 @qi_runtime_channel_receive(ptr {}, ptr {})\n", status_temp, channel, received_ptr));
                    ir.push_str(&format!("{} = load ptr, ptr {}\n", value_ptr_temp, received_ptr));
                    ir.push_str(&format!("{} = load i64, ptr {}\n", dest, value_ptr_temp));
                }
                IrInstruction::选择语句 { cases, default_case } => {
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
        use crate::parser::ast::{TypeNode, BasicType};

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
        matches!(type_name, "整数" | "浮点数" | "布尔" | "i64" | "f64" | "i32" | "f32" | "i8" | "i1")
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
        ir.push_str(&format!("  {} = call ptr @qi_runtime_alloc(i64 {})\n", dest, size));

        // Optionally bitcast to specific type if needed
        if type_name != "ptr" && type_name != "i8" {
            let typed_ptr = self.generate_temp();
            ir.push_str(&format!("  {} = bitcast ptr {} to {}*\n", typed_ptr, dest, type_name));
            ir
        } else {
            ir
        }
    }

    /// Generate heap allocation with GC check for large allocations
    /// Returns tuple of (IR code, result pointer variable name)
    fn generate_allocation_with_gc_check(&mut self, size: usize, type_name: &str, check_gc: bool) -> (String, String) {
        let mut ir = String::new();

        // For large allocations (> 1MB), check if GC should run first
        if check_gc && size > 1024 * 1024 {
            let should_gc = self.generate_temp();
            let need_gc = self.generate_temp();
            let do_gc_label = self.generate_label();
            let skip_gc_label = self.generate_label();

            // Check if GC should collect
            ir.push_str(&format!("  {} = call i64 @qi_runtime_gc_should_collect()\n", should_gc));
            ir.push_str(&format!("  {} = icmp ne i64 {}, 0\n", need_gc, should_gc));
            ir.push_str(&format!("  br i1 {}, label %{}, label %{}\n", need_gc, do_gc_label, skip_gc_label));

            // GC block
            ir.push_str(&format!("\n{}:\n", do_gc_label));
            ir.push_str("  call void @qi_runtime_gc_collect()\n");
            ir.push_str(&format!("  br label %{}\n", skip_gc_label));

            // Continue with allocation
            ir.push_str(&format!("\n{}:\n", skip_gc_label));
        }

        // Perform allocation
        let alloc_ptr = self.generate_temp();
        ir.push_str(&format!("  {} = call ptr @qi_runtime_alloc(i64 {})\n", alloc_ptr, size));

        // Bitcast if needed
        let result_ptr = if type_name != "ptr" && type_name != "i8" {
            let typed_ptr = self.generate_temp();
            ir.push_str(&format!("  {} = bitcast ptr {} to {}*\n", typed_ptr, alloc_ptr, type_name));
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
        let allocations_to_free: Vec<_> = self.allocations
            .iter()
            .filter(|a| a.scope_level == scope_level && a.is_heap)
            .cloned()
            .collect();

        // Generate deallocation calls
        for alloc in &allocations_to_free {
            ir.push_str(&format!(
                "  call i32 @qi_runtime_dealloc(ptr {}, i64 {})\n",
                alloc.ptr, alloc.size
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
