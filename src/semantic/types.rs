//! Type system definitions for Qi language

use crate::parser::ast::{TypeNode, BasicType};

/// Type system manager
pub struct TypeSystem {
    builtin_types: BuiltinTypes,
    custom_types: std::collections::HashMap<String, TypeNode>,
}

/// Built-in type definitions
#[derive(Debug, Clone)]
pub struct BuiltinTypes {
    pub integer_type: TypeNode,
    pub float_type: TypeNode,
    pub boolean_type: TypeNode,
    pub string_type: TypeNode,
    pub char_type: TypeNode,
    pub void_type: TypeNode,
}

impl TypeSystem {
    pub fn new() -> Self {
        Self {
            builtin_types: BuiltinTypes::new(),
            custom_types: std::collections::HashMap::new(),
        }
    }

    pub fn builtin_types(&self) -> &BuiltinTypes {
        &self.builtin_types
    }

    /// Register a custom type in the type system
    pub fn register_custom_type(&mut self, name: String, type_def: TypeNode) -> Result<(), String> {
        if self.custom_types.contains_key(&name) {
            return Err(format!("类型 '{}' 已经定义", name));
        }

        // Validate that custom types are only struct or enum types
        match &type_def {
            TypeNode::结构体类型(_) | TypeNode::枚举类型(_) => {
                self.custom_types.insert(name, type_def);
                Ok(())
            }
            _ => Err(format!("自定义类型 '{}' 必须是结构体或枚举", name)),
        }
    }

    /// Get a custom type by name
    pub fn get_custom_type(&self, name: &str) -> Option<&TypeNode> {
        self.custom_types.get(name)
    }

    /// Get all custom types
    pub fn get_custom_types(&self) -> &std::collections::HashMap<String, TypeNode> {
        &self.custom_types
    }

    /// Check if a type name is a registered custom type
    pub fn is_custom_type(&self, name: &str) -> bool {
        self.custom_types.contains_key(name)
    }

    /// Resolve type name (check builtin first, then custom)
    pub fn resolve_type(&self, name: &str) -> Option<TypeNode> {
        // First check if it's a builtin type
        if let Some(builtin_type) = self.parse_chinese_type(name) {
            return Some(builtin_type);
        }

        // Then check custom types
        self.custom_types.get(name).cloned()
    }

    /// Check if two types are compatible
    pub fn is_compatible(&self, expected: &TypeNode, actual: &TypeNode) -> bool {
        match (expected, actual) {
            (TypeNode::基础类型(expected_basic), TypeNode::基础类型(actual_basic)) => {
                self.is_basic_type_compatible(expected_basic, actual_basic)
            }
            _ => false,
        }
    }

    /// Check if two basic types are compatible
    fn is_basic_type_compatible(&self, expected: &BasicType, actual: &BasicType) -> bool {
        match (expected, actual) {
            // Exact match
            (e, a) if e == a => true,

            // Integer and float compatibility
            (BasicType::整数, BasicType::浮点数) => true,
            (BasicType::浮点数, BasicType::整数) => true,

            // No other compatibility
            _ => false,
        }
    }

    /// Get the common type for two types
    pub fn get_common_type(&self, type1: &TypeNode, type2: &TypeNode) -> Option<TypeNode> {
        match (type1, type2) {
            (TypeNode::基础类型(basic1), TypeNode::基础类型(basic2)) => {
                self.get_common_basic_type(basic1, basic2).map(TypeNode::基础类型)
            }
            _ => None,
        }
    }

    /// Get the common basic type for two basic types
    fn get_common_basic_type(&self, type1: &BasicType, type2: &BasicType) -> Option<BasicType> {
        match (type1, type2) {
            // Same type
            (t1, t2) if t1 == t2 => Some(t1.clone()),

            // Integer + Float = Float
            (BasicType::整数, BasicType::浮点数) => Some(BasicType::浮点数),
            (BasicType::浮点数, BasicType::整数) => Some(BasicType::浮点数),

            // No common type
            _ => None,
        }
    }

    /// Convert Chinese type names to TypeNode
    pub fn parse_chinese_type(&self, name: &str) -> Option<TypeNode> {
        match name {
            "整数" => Some(TypeNode::基础类型(BasicType::整数)),
            "浮点数" => Some(TypeNode::基础类型(BasicType::浮点数)),
            "布尔" => Some(TypeNode::基础类型(BasicType::布尔)),
            "字符串" => Some(TypeNode::基础类型(BasicType::字符串)),
            "字符" => Some(TypeNode::基础类型(BasicType::字符)),
            "空" => Some(TypeNode::基础类型(BasicType::空)),
            "长整数" => Some(TypeNode::基础类型(BasicType::长整数)),
            "短整数" => Some(TypeNode::基础类型(BasicType::短整数)),
            "字节" => Some(TypeNode::基础类型(BasicType::字节)),
            _ => None,
        }
    }

    /// Convert TypeNode to Chinese name
    pub fn type_to_chinese_name(&self, type_node: &TypeNode) -> String {
        match type_node {
            TypeNode::基础类型(basic_type) => match basic_type {
                BasicType::整数 => "整数".to_string(),
                BasicType::浮点数 => "浮点数".to_string(),
                BasicType::布尔 => "布尔".to_string(),
                BasicType::字符串 => "字符串".to_string(),
                BasicType::字符 => "字符".to_string(),
                BasicType::空 => "空".to_string(),
                BasicType::长整数 => "长整数".to_string(),
                BasicType::短整数 => "短整数".to_string(),
                BasicType::字节 => "字节".to_string(),
                BasicType::数组 => "数组".to_string(),
                BasicType::字典 => "字典".to_string(),
                BasicType::列表 => "列表".to_string(),
                BasicType::集合 => "集合".to_string(),
                BasicType::指针 => "指针".to_string(),
                BasicType::引用 => "引用".to_string(),
                BasicType::可变引用 => "可变引用".to_string(),
            },
            TypeNode::函数Type(_) => "函数".to_string(),
            TypeNode::数组类型(_) => "数组".to_string(),
            TypeNode::结构体类型(_) => "类型".to_string(),
            TypeNode::枚举类型(_) => "枚举".to_string(),
            TypeNode::字典类型(_) => "字典".to_string(),
            TypeNode::列表类型(_) => "列表".to_string(),
            TypeNode::集合类型(_) => "集合".to_string(),
            TypeNode::指针类型(_) => "指针".to_string(),
            TypeNode::引用类型(_) => "引用".to_string(),
            TypeNode::自定义类型(_) => "类型".to_string(),
        }
    }
}

impl BuiltinTypes {
    pub fn new() -> Self {
        Self {
            integer_type: TypeNode::基础类型(BasicType::整数),
            float_type: TypeNode::基础类型(BasicType::浮点数),
            boolean_type: TypeNode::基础类型(BasicType::布尔),
            string_type: TypeNode::基础类型(BasicType::字符串),
            char_type: TypeNode::基础类型(BasicType::字符),
            void_type: TypeNode::基础类型(BasicType::空),
        }
    }
}

impl Default for TypeSystem {
    fn default() -> Self {
        Self::new()
    }
}