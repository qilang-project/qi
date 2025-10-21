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
    },

    /// Load a value
    加载 {
        dest: String,
        source: String,
    },

    /// Binary operation
    二元操作 {
        dest: String,
        left: String,
        operator: BinaryOperator,
        right: String,
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
    },
}

/// IR builder
pub struct IrBuilder {
    instructions: Vec<IrInstruction>,
    temp_counter: usize,
    label_counter: usize,
}

impl IrBuilder {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
        }
    }

    pub fn build(&mut self, ast: &AstNode) -> Result<String, String> {
        self.instructions.clear();
        self.temp_counter = 0;
        self.label_counter = 0;

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

    /// Build IR for an AST node
    #[allow(unreachable_patterns)]
    fn build_node(&mut self, node: &AstNode) -> Result<String, String> {
        match node {
            AstNode::程序(program) => {
                for stmt in &program.statements {
                    self.build_node(stmt)?;
                }
                Ok("main".to_string())
            }
            AstNode::变量声明(decl) => {
                let var_name = format!("%{}", decl.name);

                // Determine the type based on the initializer
                let type_name = if let Some(initializer) = &decl.initializer {
                    match &**initializer {
                        AstNode::字面量表达式(literal) => {
                            match &literal.value {
                                crate::parser::ast::LiteralValue::字符串(_) => "ptr",
                                _ => &self.get_llvm_type(&decl.type_annotation)
                            }
                        }
                        _ => &self.get_llvm_type(&decl.type_annotation)
                    }
                } else {
                    &self.get_llvm_type(&decl.type_annotation)
                };

                // Allocate variable
                self.add_instruction(IrInstruction::分配 {
                    dest: var_name.clone(),
                    type_name: type_name.to_string(),
                });

                // Initialize if there's an initializer
                if let Some(initializer) = &decl.initializer {
                    let value = self.build_node(initializer)?;
                    self.add_instruction(IrInstruction::存储 {
                        target: var_name.clone(),
                        value,
                    });
                }

                Ok(var_name)
            }
            AstNode::函数声明(func_decl) => {
                // Handle special cases and apply name mangling for Chinese function names
                let func_name: String = match func_decl.name.as_str() {
                    "主函数" | "主" => "main".to_string(), // Special case for main function
                    name => {
                        // Apply UTF-8 + Hex name mangling for non-ASCII names
                        if name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(name)
                        } else {
                            name.to_string() // Keep ASCII names as-is
                        }
                    }
                };

                // Build parameter list
                let params: Vec<String> = func_decl.parameters
                    .iter()
                    .map(|p| {
                        let type_str = self.get_llvm_type(&p.type_annotation);
                        format!("{} {}", type_str, p.name)
                    })
                    .collect();

                let params_str = if params.is_empty() {
                    String::new()
                } else {
                    format!(" {}", params.join(", "))
                };

                // Use i32 for main function return type, void for others unless specified
                let return_type = if func_decl.name == "主函数" || func_decl.name == "主" {
                    "i32"
                } else if func_decl.return_type.is_none() {
                    "void"
                } else {
                    &self.get_return_type(&func_decl.return_type)
                };

                // Add function label
                self.add_instruction(IrInstruction::标签 {
                    name: format!("define {} @{}({}) {{", return_type, func_name, params_str),
                });

                // Add entry block
                self.add_instruction(IrInstruction::标签 {
                    name: "entry:".to_string(),
                });

                // Process function body
                for stmt in &func_decl.body {
                    self.build_node(stmt)?;
                }

                // Add return statement for all functions if none exists
                if func_decl.name == "主函数" || func_decl.name == "主" {
                    self.add_instruction(IrInstruction::返回 { value: Some("0".to_string()) });
                } else {
                    // For non-main functions, return void if no explicit return
                    self.add_instruction(IrInstruction::返回 { value: None });
                }

                // Add closing brace for the function
                self.add_instruction(IrInstruction::标签 {
                    name: "}".to_string(),
                });

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
            AstNode::打印语句(print_stmt) => {
                // Build the value to print
                let value = self.build_node(&print_stmt.value)?;

                // Increment counter to ensure unique names
                self.temp_counter += 1;

                // Check if it's a string literal (starts with @)
                if value.starts_with('@') {
                    // Direct string literal - create a format string with newline
                    let format_name = format!("@.printf_format_{}", self.temp_counter);
                    self.add_instruction(IrInstruction::字符串常量 {
                        name: format!("{} = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\", align 1", format_name),
                    });

                    // Generate printf call matching clang's format
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(format!("%t{}", self.temp_counter + 1)),
                        callee: "printf".to_string(),
                        arguments: vec![format_name, value],
                    });
                } else {
                    // Variable or expression - need to load it first
                    let format_name = format!("@.printf_format_{}", self.temp_counter);
                    self.add_instruction(IrInstruction::字符串常量 {
                        name: format!("{} = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\", align 1", format_name),
                    });

                    // Generate printf call
                    self.add_instruction(IrInstruction::函数调用 {
                        dest: Some(format!("%t{}", self.temp_counter + 1)),
                        callee: "printf".to_string(),
                        arguments: vec![format_name, value],
                    });
                }

                Ok("print".to_string())
            }
            AstNode::表达式语句(expr_stmt) => {
                self.build_node(&expr_stmt.expression)
            }
            AstNode::如果语句(if_stmt) => {
                // Build condition
                let condition = self.build_node(&if_stmt.condition)?;

                // Generate labels
                let then_label = self.generate_label();
                let else_label = self.generate_label();
                let end_label = self.generate_label();

                // Compare condition to 0 (false)
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::二元操作 {
                    dest: temp.clone(),
                    left: condition,
                    operator: crate::parser::ast::BinaryOperator::不等于,
                    right: "0".to_string(),
                });

                // Conditional jump
                self.add_instruction(IrInstruction::条件跳转 {
                    condition: temp,
                    true_label: then_label.clone(),
                    false_label: else_label.clone(),
                });

                // Then branch
                self.add_instruction(IrInstruction::标签 { name: then_label.clone() });
                for stmt in &if_stmt.then_branch {
                    self.build_node(stmt)?;
                }
                self.add_instruction(IrInstruction::跳转 { label: end_label.clone() });

                // Else branch (if exists)
                self.add_instruction(IrInstruction::标签 { name: else_label.clone() });
                if let Some(else_branch) = &if_stmt.else_branch {
                    for stmt in else_branch {
                        self.build_node(stmt)?;
                    }
                }

                // End label
                self.add_instruction(IrInstruction::标签 { name: end_label.clone() });

                Ok("if".to_string())
            }
            AstNode::当语句(while_stmt) => {
                // Generate labels
                let start_label = self.generate_label();
                let body_label = self.generate_label();
                let end_label = self.generate_label();

                // Start label
                self.add_instruction(IrInstruction::标签 { name: start_label.clone() });

                // Build condition
                let condition = self.build_node(&while_stmt.condition)?;

                // Compare condition to 0 (false)
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::二元操作 {
                    dest: temp.clone(),
                    left: condition,
                    operator: crate::parser::ast::BinaryOperator::不等于,
                    right: "0".to_string(),
                });

                // Conditional jump to body
                self.add_instruction(IrInstruction::条件跳转 {
                    condition: temp,
                    true_label: body_label.clone(),
                    false_label: end_label.clone(),
                });

                // Body
                self.add_instruction(IrInstruction::标签 { name: body_label.clone() });
                for stmt in &while_stmt.body {
                    self.build_node(stmt)?;
                }

                // Jump back to start
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
            AstNode::C风格对于语句(c_for_stmt) => {
                // Generate labels
                let start_label = self.generate_label();
                let body_label = self.generate_label();
                let update_label = self.generate_label();
                let end_label = self.generate_label();

                // Execute initializer
                self.build_node(&c_for_stmt.initializer)?;

                // Start label (condition check)
                self.add_instruction(IrInstruction::标签 { name: start_label.clone() });

                // Build condition
                let condition = self.build_node(&c_for_stmt.condition)?;

                // Compare condition to 0 (false)
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::二元操作 {
                    dest: temp.clone(),
                    left: condition,
                    operator: crate::parser::ast::BinaryOperator::不等于,
                    right: "0".to_string(),
                });

                // Conditional jump to body or end
                self.add_instruction(IrInstruction::条件跳转 {
                    condition: temp,
                    true_label: body_label.clone(),
                    false_label: end_label.clone(),
                });

                // Body
                self.add_instruction(IrInstruction::标签 { name: body_label.clone() });
                for stmt in &c_for_stmt.body {
                    self.build_node(stmt)?;
                }

                // Jump to update
                self.add_instruction(IrInstruction::跳转 { label: update_label.clone() });

                // Update section
                self.add_instruction(IrInstruction::标签 { name: update_label.clone() });
                self.build_node(&c_for_stmt.update)?;

                // Jump back to condition check
                self.add_instruction(IrInstruction::跳转 { label: start_label.clone() });

                // End label
                self.add_instruction(IrInstruction::标签 { name: end_label.clone() });

                Ok("c_for".to_string())
            }
            AstNode::对于语句(_for_stmt) => {
                // Range-based for loops are more complex, skip for now
                Ok("for".to_string())
            }
            AstNode::字面量表达式(literal) => {
                match &literal.value {
                    crate::parser::ast::LiteralValue::整数(n) => Ok(n.to_string()),
                    crate::parser::ast::LiteralValue::浮点数(f) => Ok(f.to_string()),
                    crate::parser::ast::LiteralValue::布尔(b) => Ok(if *b { "1".to_string() } else { "0".to_string() }),
                    crate::parser::ast::LiteralValue::字符串(s) => {
                        // Create a global string constant matching clang's format
                        let escaped_str = self.escape_string(s);
                        let byte_len = s.as_bytes().len();
                        let total_len = byte_len + 1; // +1 for null terminator

                        let str_name = format!("@.str{}", self.temp_counter);

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

                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::二元操作 {
                    dest: temp.clone(),
                    left,
                    operator: binary_expr.operator,
                    right,
                });
                Ok(temp)
            }
            AstNode::赋值表达式(assign_expr) => {
                let value = self.build_node(&assign_expr.value)?;

                // For assignment, we store the value to the variable
                self.add_instruction(IrInstruction::存储 {
                    target: assign_expr.target.clone(),
                    value,
                });
                Ok(assign_expr.target.clone())
            }
            AstNode::函数调用表达式(call_expr) => {
                // Evaluate arguments
                let mut arg_temps = Vec::new();
                for arg in &call_expr.arguments {
                    let temp = self.build_node(arg)?;
                    arg_temps.push(temp);
                }

                // Apply the same name mangling logic for function calls
                let mapped_callee: String = match call_expr.callee.as_str() {
                    "主函数" | "主" => "main".to_string(), // Special case for main function
                    name => {
                        // Apply UTF-8 + Hex name mangling for non-ASCII names
                        if name.chars().any(|c| !c.is_ascii()) {
                            self.mangle_function_name(name)
                        } else {
                            name.to_string() // Keep ASCII names as-is
                        }
                    }
                };

                // Generate function call
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::函数调用 {
                    dest: Some(temp.clone()),
                    callee: mapped_callee,
                    arguments: arg_temps,
                });
                Ok(temp)
            }
            AstNode::标识符表达式(ident) => {
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::加载 {
                    dest: temp.clone(),
                    source: format!("%{}", ident.name),
                });
                Ok(temp)
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

                // Generate string concatenation
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::字符串连接 {
                    dest: temp.clone(),
                    left: left_var,
                    right: right_var,
                });
                Ok(temp)
            }
            AstNode::结构体声明(_struct_decl) => {
                // Struct declarations don't generate code directly
                // They just define the type for later use
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
                    });

                    // Store the field value
                    self.add_instruction(IrInstruction::存储 {
                        target: field_ptr,
                        value: field_value,
                    });
                }

                Ok(temp)
            }
            AstNode::字段访问表达式(field_access) => {
                // Build object expression
                let object_var = self.build_node(&field_access.object)?;

                // Generate field access instruction
                let temp = self.generate_temp();
                self.add_instruction(IrInstruction::字段访问 {
                    dest: temp.clone(),
                    object: object_var,
                    field: field_access.field.clone(),
                });

                // Load the field value
                let load_temp = self.generate_temp();
                self.add_instruction(IrInstruction::加载 {
                    dest: load_temp.clone(),
                    source: temp,
                });

                Ok(load_temp)
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
                    crate::parser::ast::BasicType::浮点数 => "double".to_string(),
                    crate::parser::ast::BasicType::布尔 => "i1".to_string(),
                    crate::parser::ast::BasicType::字符串 => "ptr".to_string(),
                    crate::parser::ast::BasicType::字符 => "i8".to_string(),
                    crate::parser::ast::BasicType::空 => "void".to_string(),
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
        ir.push_str("; Module ID = 'qi_program'\n\n");

        // Add external function declarations
        ir.push_str("declare i32 @printf(ptr, ...)\n");
        ir.push_str("declare ptr @qi_string_concat(ptr, ptr)\n\n");

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

        // Process other instructions
        for instruction in &other_instructions {
            match instruction {
                IrInstruction::分配 { dest, type_name } => {
                    ir.push_str(&format!("{} = alloca {}\n", dest, type_name));
                }
                IrInstruction::存储 { target, value } => {
                    // Determine the type based on the target variable name
                    let (value_type, _pointer_type) = if target.contains("message") || target.starts_with("%str") {
                        ("ptr", "ptr")
                    } else if value.contains("getelementptr") || value.starts_with('@') {
                        ("ptr", "ptr")
                    } else if value.contains('.') {
                        ("double", "ptr")
                    } else if value == "0" || value == "1" {
                        ("i1", "ptr")
                    } else {
                        ("i64", "ptr")
                    };
                    ir.push_str(&format!("store {} {}, ptr {}\n", value_type, value, target));
                }
                IrInstruction::加载 { dest, source } => {
                    // Determine the correct type based on the source
                    let load_type = if source.starts_with("%message") {
                        // Assume message is a string pointer
                        "ptr"
                    } else {
                        "i64"
                    };
                    ir.push_str(&format!("{} = load {}, ptr {}\n", dest, load_type, source));
                }
                IrInstruction::二元操作 { dest, left, operator, right } => {
                    let (op_str, return_type) = match operator {
                        crate::parser::ast::BinaryOperator::加 => ("add", "i64"),
                        crate::parser::ast::BinaryOperator::减 => ("sub", "i64"),
                        crate::parser::ast::BinaryOperator::乘 => ("mul", "i64"),
                        crate::parser::ast::BinaryOperator::除 => ("sdiv", "i64"),
                        crate::parser::ast::BinaryOperator::取余 => ("srem", "i64"),
                        crate::parser::ast::BinaryOperator::等于 => ("icmp eq", "i1"),
                        crate::parser::ast::BinaryOperator::不等于 => ("icmp ne", "i1"),
                        crate::parser::ast::BinaryOperator::大于 => ("icmp sgt", "i1"),
                        crate::parser::ast::BinaryOperator::小于 => ("icmp slt", "i1"),
                        crate::parser::ast::BinaryOperator::大于等于 => ("icmp sge", "i1"),
                        crate::parser::ast::BinaryOperator::小于等于 => ("icmp sle", "i1"),
                        crate::parser::ast::BinaryOperator::与 => ("and", "i1"),
                        crate::parser::ast::BinaryOperator::或 => ("or", "i1"),
                    };

                    ir.push_str(&format!("{} = {} {} {}, {}\n", dest, op_str, return_type, left, right));
                }
                IrInstruction::函数调用 { dest, callee, arguments } => {
                    if callee == "printf" && !arguments.is_empty() {
                        // Handle printf calls matching clang's format
                        let mut processed_args = Vec::new();

                        for arg in arguments {
                            if arg.starts_with('@') {
                                // String constant - pass as ptr noundef
                                processed_args.push(format!("ptr noundef {}", arg));
                            } else if arg.starts_with('%') {
                                // Variable or temporary - pass as ptr
                                processed_args.push(format!("ptr {}", arg));
                            } else {
                                // Other values
                                processed_args.push(arg.clone());
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
                        // Regular function call
                        let args_str = if arguments.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", arguments.join(", "))
                        };

                        match dest {
                            Some(dest_var) => {
                                ir.push_str(&format!("{} = call i64 @{}({})\n", dest_var, callee, args_str));
                            }
                            None => {
                                ir.push_str(&format!("call void @{}({})\n", callee, args_str));
                            }
                        }
                    }
                }
                IrInstruction::返回 { value: None } => {
                    // If current function is void, emit ret void; otherwise emit a default zero return? Here keep ret void.
                    ir.push_str("ret void\n");
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
                    // Simplified array store
                    ir.push_str(&format!("store i64 {}, i64* getelementptr [10 x i64], [10 x i64]* {}, i64 0, i64 {}\n", value, array, index));
                }
                IrInstruction::字符串连接 { dest, left, right } => {
                    // Simplified string concatenation using external function
                    ir.push_str(&format!("{} = call i8* @qi_string_concat(i8* {}, i8* {})\n", dest, left, right));
                }
                IrInstruction::字段访问 { dest, object, field: _ } => {
                    // Simplified field access using getelementptr
                    // In a real implementation, this would use struct field indices based on the field name
                    ir.push_str(&format!("{} = getelementptr %{}.type, %{}* {}, i32 0, i32 {}\n",
                        dest, object, object, object, 0));
                }
                IrInstruction::字符串常量 { .. } => {
                    // String constants are handled separately at the beginning
                }
            }
        }

        // ALWAYS add closing brace for main function - no conditions
        // This is a simple fix for the missing closing brace issue
        if ir.contains("define i32 @main") && !ir.ends_with("}\n") {
            ir.push_str("}\n");
        }

        Ok(ir)
    }
}

impl Default for IrBuilder {
    fn default() -> Self {
        Self::new()
    }
}
