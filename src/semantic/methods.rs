//! Method system for Qi language
//!
//! Supports methods associated with custom types (structs/enums)

use crate::parser::ast::{TypeNode, MethodDeclaration, Parameter};
use std::collections::HashMap;

/// Method information for a type
#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub receiver_type: TypeNode,
    pub receiver_name: String,
    pub is_receiver_mutable: bool,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeNode>,
    pub body: Vec<crate::parser::ast::AstNode>,
    pub visibility: crate::parser::ast::Visibility,
}

/// Method system manager
pub struct MethodSystem {
    methods: HashMap<String, Vec<MethodInfo>>, // type_name -> methods
}

impl MethodSystem {
    /// Create a new method system
    pub fn new() -> Self {
        Self {
            methods: HashMap::new(),
        }
    }

    /// Register a method for a type
    pub fn register_method(&mut self, type_name: String, method: MethodDeclaration) -> Result<(), String> {
        let method_name = method.method_name.clone();

        // Check for duplicate method names first
        if let Some(existing_methods) = self.methods.get(&type_name) {
            if existing_methods.iter().any(|m| m.name == method_name) {
                return Err(format!("类型 '{}' 已有方法 '{}'",
                    type_name, method_name));
            }
        }

        let method_info = MethodInfo {
            name: method_name,
            receiver_type: TypeNode::自定义类型(type_name.clone()),
            receiver_name: method.receiver_name.clone(),
            is_receiver_mutable: method.is_receiver_mutable,
            parameters: method.parameters.clone(),
            return_type: method.return_type.clone(),
            body: method.body.clone(),
            visibility: method.visibility,
        };

        // Add new method
        let type_methods = self.methods.entry(type_name).or_insert_with(Vec::new);
        type_methods.push(method_info);
        Ok(())
    }

    /// Get all methods for a type
    pub fn get_methods(&self, type_name: &str) -> Option<&Vec<MethodInfo>> {
        self.methods.get(type_name)
    }

    /// Get a specific method for a type
    pub fn get_method(&self, type_name: &str, method_name: &str) -> Option<&MethodInfo> {
        self.methods.get(type_name)
            .and_then(|methods| methods.iter().find(|m| m.name == method_name))
    }

    /// Check if a type has a specific method
    pub fn has_method(&self, type_name: &str, method_name: &str) -> bool {
        self.get_method(type_name, method_name).is_some()
    }

    /// Get all method names for a type
    pub fn get_method_names(&self, type_name: &str) -> Vec<String> {
        self.methods.get(type_name)
            .map(|methods| methods.iter().map(|m| m.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Validate method call compatibility
    pub fn validate_method_call(&self,
        type_name: &str,
        method_name: &str,
        arg_count: usize
    ) -> Result<(), String> {
        let methods = self.get_methods(type_name)
            .ok_or_else(|| format!("类型 '{}' 不存在", type_name))?;

        let method = methods.iter()
            .find(|m| m.name == method_name)
            .ok_or_else(|| format!("类型 '{}' 没有方法 '{}'", type_name, method_name))?;

        if method.parameters.len() != arg_count {
            return Err(format!(
                "方法 '{}' 需要 {} 个参数，但提供了 {} 个",
                method_name, method.parameters.len(), arg_count
            ));
        }

        Ok(())
    }

    /// Get all types with methods
    pub fn get_types_with_methods(&self) -> Vec<String> {
        self.methods.keys().cloned().collect()
    }

    /// Remove all methods for a type (useful when type is redefined)
    pub fn remove_type_methods(&mut self, type_name: &str) {
        self.methods.remove(type_name);
    }
}