//! IR builder for Qi language

use crate::parser::ast::{AstNode, BinaryOperator};

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

    /// Async function declaration
    异步函数声明 {
        name: String,
        params: Vec<String>,
        return_type: String,
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
}

/// IR builder
pub struct IrBuilder {
    instructions: Vec<IrInstruction>,
    temp_counter: usize,
    label_counter: usize,
    /// Track variable types for better code generation
    variable_types: std::collections::HashMap<String, String>,
    /// Track async function return types
    async_function_types: std::collections::HashMap<String, String>,
    /// Track all function return types (including sync functions)
    function_return_types: std::collections::HashMap<String, String>,
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
}

impl IrBuilder {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
            variable_types: std::collections::HashMap::new(),
            async_function_types: std::collections::HashMap::new(),
            function_return_types: std::collections::HashMap::new(),
            in_async_context: false,
            defined_functions: std::collections::HashSet::new(),
            external_functions: std::collections::HashMap::new(),
            struct_definitions: std::collections::HashMap::new(),
            struct_field_names: std::collections::HashMap::new(),
            variable_struct_types: std::collections::HashMap::new(),
            import_aliases: std::collections::HashMap::new(),
            current_package_name: None,
        }
    }

    pub fn build(&mut self, ast: &AstNode) -> Result<String, String> {
        self.instructions.clear();
        self.temp_counter = 0;
        self.label_counter = 0;
        self.variable_types.clear();
        self.async_function_types.clear();
        // Note: We don't clear defined_functions and external_functions here
        // so they can be set before calling build()

        self.build_node(ast)?;
        self.emit_llvm_ir()
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
        self.external_functions = funcs;
    }

    /// Set defined functions in the current module
    pub fn set_defined_functions(&mut self, funcs: std::collections::HashSet<String>) {
        self.defined_functions = funcs;
    }

    /// Set import aliases for namespace resolution
    pub fn set_import_aliases(&mut self, aliases: std::collections::HashMap<String, String>) {
        self.import_aliases = aliases;
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
                
                // Process all statements in the program (functions, variables, etc.)
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
                                } else if runtime_func.contains("read_string") ||
                                          runtime_func.contains("int_to_string") ||
                                          runtime_func.contains("float_to_string") ||
                                          runtime_func.contains("string") ||
                                          runtime_func == "qi_string_concat" {
                                    "ptr"
                                } else if runtime_func.contains("math_abs_int") || runtime_func.contains("float_to_int") ||
                                          runtime_func.contains("string_to_int") || runtime_func.contains("array_length") {
                                    "i64"
                                } else {
                                    "i64"
                                }
                            } else {
                                "i64"
                            };
                            (ty.to_string(), None)
                        }
                        AstNode::等待表达式(_) => {
                            // Build the await expression first to determine its type
                            let init_value = self.build_node(&**initializer)?;
                            let init_var_name = init_value.trim_start_matches('%');
                            let ty = self.variable_types.get(init_var_name)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "i64".to_string());
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
                self.variable_types.insert(mangled_name, type_name.to_string());

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
                    self.variable_types.insert(param_name, type_str);
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

                // Record the function's return type for later function calls
                self.function_return_types.insert(func_name.clone(), return_type.clone());
                
                // Record this function as defined in current module
                self.defined_functions.insert(func_name.clone());

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
            AstNode::异步函数声明(async_func_decl) => {
                // Handle async function declaration similar to regular functions
                let func_name: String = match async_func_decl.name.as_str() {
                    name => {
                        if name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(name)
                        } else {
                            name.to_string()
                        }
                    }
                };

                // Mark that we're entering an async context
                let was_async = self.in_async_context;
                self.in_async_context = true;

                // Record the async function's return type
                let return_type_str = if let Some(ref ret_type) = async_func_decl.return_type {
                    self.get_llvm_type(&Some(ret_type.clone()))
                } else {
                    "void".to_string()
                };
                self.async_function_types.insert(func_name.clone(), return_type_str);
                
                // Record this function as defined in current module
                self.defined_functions.insert(func_name.clone());

                // Build parameter list
                let params: Vec<String> = async_func_decl.parameters
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

                let params_str = if params.is_empty() {
                    String::new()
                } else {
                    format!(" {}", params.join(", "))
                };

                // Async functions always return ptr (Future handle)
                let return_type = "ptr".to_string();

                // Add async function header label
                self.add_instruction(IrInstruction::标签 {
                    name: format!("define {} @{}({}) {{", return_type, func_name, params_str),
                });

                // Add entry block
                self.add_instruction(IrInstruction::标签 {
                    name: "entry:".to_string(),
                });

                // Process async function body
                for stmt in &async_func_decl.body {
                    self.build_node(stmt)?;
                }

                // Async functions should always return a Future handle
                // For now, we'll return a null Future handle as a placeholder
                // In a real implementation, we would create a proper Future object
                self.add_instruction(IrInstruction::返回 {
                    value: Some("null".to_string()) // Return null Future handle
                });

                // // Function is already properly closed by function body processing
                self.add_instruction(IrInstruction::标签 { name: "}".to_string() });

                // Add module-qualified alias if we have a package name
                if let Some(package_name) = &self.current_package_name {
                    // Create an alias: @数学_异步函数 = alias ptr (), ptr @异步函数
                    let alias_name = format!("{}_{}", package_name, &async_func_decl.name);
                    let alias_mangled = self.mangle_function_name(&alias_name);
                    
                    // Build parameter types list (without parameter names)
                    let param_types: Vec<String> = async_func_decl.parameters
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

                // Restore async context flag
                self.in_async_context = was_async;

                Ok(func_name.to_string())
            }
            AstNode::返回语句(return_stmt) => {
                let value = if let Some(expr) = &return_stmt.value {
                    Some(self.build_node(expr)?)
                } else {
                    None
                };

                self.add_instruction(IrInstruction::返回 { value });
                Ok("ret".to_string())
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
                    crate::parser::ast::LiteralValue::布尔(b) => Ok(if *b { "1".to_string() } else { "0".to_string() }),
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
                            callee: "qi_string_concat".to_string(),
                            arguments: vec![left_str, right_str],
                        });
                        return Ok(temp);
                    }
                }

                // Determine the result type of the binary operation
                // Check if either operand is a float type (either literal or variable)
                let is_float_op = left.contains('.') || right.contains('.') ||
                                  self.is_float_operand(&left) || self.is_float_operand(&right);
                let result_type = if is_float_op {
                    "double"
                } else {
                    "i64"
                };

                let temp = self.generate_temp();
                
                // Record the type of this temporary variable
                self.variable_types.insert(temp.trim_start_matches('%').to_string(), result_type.to_string());
                
                self.add_instruction(IrInstruction::二元操作 {
                    dest: temp.clone(),
                    left,
                    operator: binary_expr.operator,
                    right,
                    operand_type: result_type.to_string(),
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
                                    crate::parser::ast::LiteralValue::布尔(_) => "integer",
                                    crate::parser::ast::LiteralValue::字符(_) => "integer",
                                }
                            }
                            AstNode::标识符表达式(ident) => {
                                // Look up variable type from our tracking
                                match self.variable_types.get(&ident.name) {
                                    Some(vtype) if vtype == "double" => "float",
                                    Some(vtype) if vtype == "ptr" => "string",
                                    _ => "integer", // Default to integer
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

                // Special handling: 打印 or 打印行 with two arguments -> map to printf with proper format
                if (function_name == "打印" || function_name == "打印行") && arg_temps.len() == 2 {
                    // Infer type of second argument
                    let second = &arg_temps[1];
                    let second_ty = if second.starts_with('%') {
                        let var_name = second.trim_start_matches('%');
                        self.variable_types.get(var_name)
                            .cloned()
                            .unwrap_or_else(|| "i64".to_string())
                    } else if second.starts_with('@') {
                        "ptr".to_string()
                    } else if second.contains('.') {
                        "double".to_string()
                    } else {
                        "i64".to_string()
                    };

                    // Choose printf format based on type and whether to add newline
                    let is_println = function_name == "打印行";
                    let (fmt_spec, fmt_len) = if second_ty == "double" {
                        if is_println { ("%s %f\\0A", 6) } else { ("%s %f", 5) }
                    } else if second_ty == "ptr" {
                        if is_println { ("%s %s\\0A", 6) } else { ("%s %s", 5) }
                    } else {
                        if is_println { ("%s %lld\\0A", 8) } else { ("%s %lld", 7) }
                    };

                    // Create a global format string constant
                    let fmt_name = format!("@.fmt{}", self.temp_counter);
                    self.temp_counter += 1;
                    self.add_instruction(IrInstruction::字符串常量 {
                        name: format!(
                            "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1",
                            fmt_name,
                            fmt_len + 1,
                            fmt_spec
                        ),
                    });

                    // Prepend format string to arguments and switch callee to printf
                    let mut new_args = Vec::new();
                    new_args.push(fmt_name);
                    new_args.push(arg_temps[0].clone());
                    new_args.push(arg_temps[1].clone());
                    arg_temps = new_args;
                    mapped_callee = "printf".to_string();
                }

                // Check if this is an async function call
                // Use the async_function_types HashMap to determine if a function is async
                let is_async_function = self.async_function_types.contains_key(&mapped_callee);

                // Check if this is an external function (called but not defined in current module)
                if !mapped_callee.starts_with("qi_runtime_") && 
                   mapped_callee != "printf" &&
                   !self.defined_functions.contains(&mapped_callee) &&
                   !self.external_functions.contains_key(&mapped_callee) {
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
                            } else {
                                "i64"
                            }
                        } else if mapped_callee == "qi_string_concat" {
                            "ptr"
                        } else if let Some(ret_type) = self.function_return_types.get(&mapped_callee) {
                            ret_type.as_str()
                        } else {
                            "i64"
                        };
                        self.variable_types.insert(temp.trim_start_matches('%').to_string(), return_type.to_string());

                        self.add_instruction(IrInstruction::函数调用 {
                            dest: Some(temp.clone()),
                            callee: mapped_callee,
                            arguments: arg_temps,
                        });

                        Ok(temp)
                    } else {
                        // Void function - no return value
                        self.add_instruction(IrInstruction::函数调用 {
                            dest: None,
                            callee: mapped_callee,
                            arguments: arg_temps,
                        });

                        Ok(String::new()) // Return empty string since there's no result
                    }
                }
            }
            AstNode::等待表达式(await_expr) => {
                // Build the awaited expression first
                let future_expr = self.build_node(&await_expr.expression)?;

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
                } else {
                    // Generate await instruction - returns pointer to the result
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

                if let Some(vtype) = var_type.clone() {
                    self.variable_types.insert(temp.trim_start_matches('%').to_string(), vtype);
                }
                
                // Also propagate struct type if it exists
                if let Some(struct_type) = self.variable_struct_types.get(&ident.name).cloned() {
                    self.variable_struct_types.insert(temp.trim_start_matches('%').to_string(), struct_type);
                }

                // Check if this is a parameter (direct value, not a pointer)
                // Need to check both original name and mangled name
                let is_param = self.variable_types.contains_key(&format!("param_{}", ident.name)) ||
                               self.variable_types.contains_key(&format!("param_{}", bare_mangled));
                
                if is_param {
                    // This is a parameter - use it directly without load
                    // Return the parameter name instead of generating a temp
                    Ok(var_name)
                } else {
                    // This is a regular variable or unknown identifier - load it
                    // Even if var_type is None, we'll try to load it
                    // (it might be an error, but let LLVM catch it)
                    self.add_instruction(IrInstruction::加载 {
                        dest: temp.clone(),
                        source: var_name,
                        load_type: None,
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

                // Allocate memory for the struct
                let struct_type = format!("{}.type", struct_literal.struct_name);
                self.add_instruction(IrInstruction::分配 {
                    dest: temp.clone(),
                    type_name: struct_type,
                });

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
                // 检查是否为模块前缀调用（object 是标识符且不是变量）
                if let AstNode::标识符表达式(ident) = &*method_call.object {
                    // 检查是否为已知变量（排除模块名）
                    let is_module = self.import_aliases.contains_key(&ident.name) || 
                                   self.import_aliases.values().any(|v| v == &ident.name);
                    let is_variable = self.variable_types.contains_key(&ident.name) ||
                                     self.variable_types.contains_key(&self.mangle_function_name(&ident.name));
                    
                    if is_module || !is_variable {
                        // 这是模块前缀调用，如 数学.最大值()
                        let module_name = &ident.name;

                        // 检查是否为导入的模块
                        if self.import_aliases.contains_key(module_name) {
                            // 这是导入的函数，直接使用函数名，不加模块前缀
                            // 例如：数学.最大值 -> 最大值（直接使用导入的函数名）
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
            _ => {
                #[allow(unreachable_patterns)]
                Err(format!("Unsupported AST node: {:?}", node))
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
            _ => "i64".to_string(), // Default to i64
        }
    }

    /// Get return type for function
    fn get_return_type(&self, return_type: &Option<crate::parser::ast::TypeNode>) -> String {
        self.get_llvm_type(return_type)
    }

    /// Emit LLVM IR from instructions
    fn emit_llvm_ir(&self) -> Result<String, String> {
        let mut ir = String::new();
        let mut string_constants = Vec::new();
        let mut other_instructions = Vec::new();
        let _temp_counter = self.temp_counter; // reserved for future use
        let mut current_function_ret_ty: Option<String> = None;

        // Separate string constants from other instructions
        for instruction in &self.instructions {
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
        
        ir.push_str("; Print functions\n");
        ir.push_str("declare i32 @qi_runtime_print(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_println(ptr)\n");
        ir.push_str("declare i32 @qi_runtime_print_int(i64)\n");
        ir.push_str("declare i32 @qi_runtime_println_int(i64)\n");
        ir.push_str("declare i32 @qi_runtime_print_float(double)\n");
        ir.push_str("declare i32 @qi_runtime_println_float(double)\n");
        ir.push_str("declare i32 @qi_runtime_println_str_int(ptr, i64)\n");
        ir.push_str("declare i32 @qi_runtime_println_str_float(ptr, double)\n");
        ir.push_str("declare i32 @qi_runtime_println_str_str(ptr, ptr)\n");
        ir.push_str("\n");
        
        ir.push_str("; Memory management\n");
        ir.push_str("declare ptr @qi_runtime_alloc(i64)\n");
        ir.push_str("declare i32 @qi_runtime_dealloc(ptr, i64)\n");
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
        ir.push_str("declare ptr @qi_string_concat(ptr, ptr)\n\n");

        // Add external function declarations from imported modules
        if !self.external_functions.is_empty() {
            ir.push_str("; External function declarations from imported modules\n");
            for (func_name, (param_types, return_type)) in &self.external_functions {
                let params_str = param_types.iter()
                    .enumerate()
                    .map(|(i, ty)| format!("{} %{}", ty, i))
                    .collect::<Vec<_>>()
                    .join(", ");
                ir.push_str(&format!("declare {} @{}({})\n", return_type, func_name, params_str));
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
                    // Add explicit type annotation for the destination and alignment
                    ir.push_str(&format!("{} = alloca {}, align {}\n", dest, mangled_type, self.get_type_alignment(type_name)));
                }
                IrInstruction::存储 { target, value, value_type } => {
                    // Determine the type based on the value_type if provided, otherwise infer
                    let inferred_type = if let Some(vt) = value_type {
                        vt.to_string()
                    } else if value.starts_with('@') || value.contains("getelementptr") {
                        "ptr".to_string()
                    } else if value.contains('.') {
                        "double".to_string()
                    } else if value.parse::<i64>().is_ok() {
                        "i64".to_string()
                    } else if value.starts_with('%') {
                        // Look up the type from variable_types HashMap
                        let var_name = value.trim_start_matches('%');
                        self.variable_types.get(var_name)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "i64".to_string())
                    } else {
                        // Default to i64 for unknown values
                        "i64".to_string()
                    };
                    ir.push_str(&format!("store {} {}, ptr {}\n", inferred_type, value, target));
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
                    let (op_str, _instr_operand_type, return_type) = if is_float {
                        match operator {
                            crate::parser::ast::BinaryOperator::加 => ("fadd", "double", "double"),
                            crate::parser::ast::BinaryOperator::减 => ("fsub", "double", "double"),
                            crate::parser::ast::BinaryOperator::乘 => ("fmul", "double", "double"),
                            crate::parser::ast::BinaryOperator::除 => ("fdiv", "double", "double"),
                            crate::parser::ast::BinaryOperator::取余 => ("frem", "double", "double"),
                            crate::parser::ast::BinaryOperator::等于 => ("fcmp oeq", "double", "i1"),
                            crate::parser::ast::BinaryOperator::不等于 => ("fcmp one", "double", "i1"),
                            crate::parser::ast::BinaryOperator::大于 => ("fcmp ogt", "double", "i1"),
                            crate::parser::ast::BinaryOperator::小于 => ("fcmp olt", "double", "i1"),
                            crate::parser::ast::BinaryOperator::大于等于 => ("fcmp oge", "double", "i1"),
                            crate::parser::ast::BinaryOperator::小于等于 => ("fcmp ole", "double", "i1"),
                            crate::parser::ast::BinaryOperator::与 => ("and", "i1", "i1"),
                            crate::parser::ast::BinaryOperator::或 => ("or", "i1", "i1"),
                        }
                    } else {
                        match operator {
                            crate::parser::ast::BinaryOperator::加 => ("add", "i64", "i64"),
                            crate::parser::ast::BinaryOperator::减 => ("sub", "i64", "i64"),
                            crate::parser::ast::BinaryOperator::乘 => ("mul", "i64", "i64"),
                            crate::parser::ast::BinaryOperator::除 => ("sdiv", "i64", "i64"),
                            crate::parser::ast::BinaryOperator::取余 => ("srem", "i64", "i64"),
                            crate::parser::ast::BinaryOperator::等于 => ("icmp eq", "i64", "i1"),
                            crate::parser::ast::BinaryOperator::不等于 => ("icmp ne", "i64", "i1"),
                            crate::parser::ast::BinaryOperator::大于 => ("icmp sgt", "i64", "i1"),
                            crate::parser::ast::BinaryOperator::小于 => ("icmp slt", "i64", "i1"),
                            crate::parser::ast::BinaryOperator::大于等于 => ("icmp sge", "i64", "i1"),
                            crate::parser::ast::BinaryOperator::小于等于 => ("icmp sle", "i64", "i1"),
                            crate::parser::ast::BinaryOperator::与 => ("and", "i1", "i1"),
                            crate::parser::ast::BinaryOperator::或 => ("or", "i1", "i1"),
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

                    // For comparison operations (icmp, fcmp), use _instr_operand_type
                    // For arithmetic operations, use return_type
                    let type_for_instruction = if op_str.starts_with("icmp") || op_str.starts_with("fcmp") {
                        _instr_operand_type
                    } else {
                        return_type
                    };

                    ir.push_str(&format!("{} = {} {} {}, {}\n", dest, op_str, type_for_instruction, normalized_left, normalized_right));
                }
                IrInstruction::函数调用 { dest, callee, arguments } => {
                    if callee == "printf" && !arguments.is_empty() {
                        // Handle printf calls matching clang's format
                        let mut processed_args = Vec::new();

                        for (i, arg) in arguments.iter().enumerate() {
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
                        
                        // Check if this is a math function that requires double arguments
                        let needs_double_args = callee.contains("math_sqrt") || 
                                               callee.contains("math_pow") ||
                                               callee.contains("math_sin") ||
                                               callee.contains("math_cos") ||
                                               callee.contains("math_tan") ||
                                               callee.contains("math_floor") ||
                                               callee.contains("math_ceil") ||
                                               callee.contains("math_round") ||
                                               callee == "e6_b1_82_e5_b9_b3_e6_96_b9_e6_a0_b9"; // 求平方根
                        
                        for arg in arguments {
                            if arg.starts_with('@') {
                                // String constant
                                typed_args.push(format!("ptr {}", arg));
                            } else if arg.starts_with('%') {
                                // Variable or temporary - look up in variable_types HashMap
                                let arg_var_name = arg.trim_start_matches('%');
                                let arg_type = self.variable_types.get(arg_var_name)
                                    .map(|s| s.as_str())
                                    .unwrap_or_else(|| {
                                        // Fallback: determine type based on specific function signatures
                                        if callee == "qi_runtime_print_int" || callee == "qi_runtime_println_int" {
                                            "i64"
                                        } else if callee == "qi_runtime_print_float" || callee == "qi_runtime_println_float" ||
                                                  callee == "qi_runtime_float_to_string" {
                                            "double"
                                        } else if callee.contains("concat") || callee.contains("read_string") || callee.contains("file") {
                                            "ptr"
                                        } else {
                                            "i64"
                                        }
                                    });
                                
                                // Convert i64 to double if needed for math functions
                                if needs_double_args && arg_type == "i64" {
                                    // Generate sitofp conversion instruction
                                    let conv_temp = format!("%conv{}", arg_var_name);
                                    ir.push_str(&format!("{} = sitofp i64 {} to double\n", conv_temp, arg));
                                    typed_args.push(format!("double {}", conv_temp));
                                } else {
                                    typed_args.push(format!("{} {}", arg_type, arg));
                                }
                            } else {
                                // Literal values
                                if needs_double_args && !arg.contains('.') {
                                    // Convert integer literal to double for math functions
                                    typed_args.push(format!("double {}.0", arg));
                                } else if arg.contains('.') {
                                    typed_args.push(format!("double {}", arg));
                                } else {
                                    typed_args.push(format!("i64 {}", arg));
                                }
                            }
                        }
                        
                        let args_str = typed_args.join(", ");
                        
                        // Determine return type based on function name
                        let ret_type = if callee.starts_with("qi_runtime_") {
                            // Math functions return double
                            if callee.contains("math_sqrt") || callee.contains("math_pow") || 
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
                            // Integer math functions return i64
                            } else if callee.contains("math_abs_int") || callee.contains("float_to_int") ||
                                      callee.contains("string_to_int") || callee.contains("array_length") {
                                "i64"
                            } else {
                                "i32"
                            }
                        } else if callee == "qi_string_concat" {
                            "ptr"
                        // Check hex-encoded Chinese function names
                        } else if callee == "e6_b1_82_e5_b9_b3_e6_96_b9_e6_a0_b9" { // 求平方根
                            "double"
                        } else if callee == "e6_b1_82_e7_bb_9d_e5_af_b9_e5_80_bc" { // 求绝对值
                            "i64"
                        } else if callee == "e5_ad_97_e7_ac_a6_e9_95_bf" { // 字符串长度
                            "i64"
                        } else {
                            // Check if this is a known async function
                            if self.async_function_types.contains_key(callee) {
                                "ptr"
                            } else {
                                "i64" // Default to i64
                            }
                        };

                        match dest {
                            Some(dest_var) => {
                                ir.push_str(&format!("{} = call {} @{}({})\n", dest_var, ret_type, callee, args_str));
                            }
                            None => {
                                ir.push_str(&format!("call void @{}({})\n", callee, args_str));
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
                            ir.push_str(&format!("ret {} {}\n", ty, val));
                        }
                    } else {
                        // Default to i64 if not within a function context
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
                    // Simplified array allocation
                    ir.push_str(&format!("{} = alloca [{} x i64]\n", dest, size));
                }
                IrInstruction::数组存储 { array, index, value } => {
                    // Simplified array store - generate unique temp names using a hash of array and index
                    let hash = format!("{}{}", array.replace("%", "").replace("t", ""), index.replace("%", ""));
                    ir.push_str(&format!("%addr_tmp{} = getelementptr [10 x i64], [10 x i64]* {}, i64 0, i64 {}\n", hash, array, index));
                    ir.push_str(&format!("store i64 {}, i64* %addr_tmp{}\n", value, hash));
                }
                IrInstruction::字符串连接 { dest, left, right } => {
                    // Simplified string concatenation using external function
                    ir.push_str(&format!("{} = call i8* @qi_string_concat(i8* {}, i8* {})\n", dest, left, right));
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
                IrInstruction::异步函数声明 { name, params, return_type } => {
                    // Async function declarations are already handled in the label processing
                    // This is just a placeholder for completeness
                }
                IrInstruction::等待表达式 { dest, future } => {
                    // Generate await call - this blocks until the future completes
                    ir.push_str(&format!("{} = call ptr @qi_runtime_await(ptr {})\n", dest, future));
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
            }
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
}

impl Default for IrBuilder {
    fn default() -> Self {
        Self::new()
    }
}
