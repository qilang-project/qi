//! Module and package management for Qi language

use std::collections::HashMap;
use std::path::PathBuf;
use crate::parser::ast::{Program, AstNode, Visibility};

/// Module information
#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub path: PathBuf,
    pub package_name: Option<String>,
    pub exports: HashMap<String, Symbol>,
    pub imports: Vec<Import>,
}

/// Import information
#[derive(Debug, Clone)]
pub struct Import {
    pub module_path: Vec<String>,
    pub items: Option<Vec<String>>,
    pub alias: Option<String>,
}

/// Symbol information for exported items
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub visibility: Visibility,
    pub kind: SymbolKind,
    pub function_signature: Option<FunctionSignature>,
}

/// Function signature for external declarations
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub parameters: Vec<(String, String)>, // (param_name, type)
    pub return_type: String,
    pub is_async: bool,
}

/// Symbol kinds
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Variable,
}

/// Module registry for tracking all modules in a compilation
pub struct ModuleRegistry {
    modules: HashMap<String, Module>,
    current_module: Option<String>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            current_module: None,
        }
    }

    /// Register a new module
    pub fn register_module(&mut self, module: Module) {
        let name = module.name.clone();
        self.modules.insert(name.clone(), module);
        self.current_module = Some(name);
    }

    /// Get a module by name
    pub fn get_module(&self, name: &str) -> Option<&Module> {
        self.modules.get(name)
    }

    /// Get the current module
    pub fn current_module(&self) -> Option<&Module> {
        self.current_module.as_ref().and_then(|name| self.modules.get(name))
    }

    /// Check if a symbol is visible in the current module
    pub fn is_symbol_visible(&self, module_name: &str, symbol_name: &str) -> Result<bool, String> {
        let module = self.get_module(module_name)
            .ok_or_else(|| format!("模块 '{}' 未找到", module_name))?;

        let symbol = module.exports.get(symbol_name)
            .ok_or_else(|| format!("符号 '{}' 在模块 '{}' 中未找到", symbol_name, module_name))?;

        // Public symbols are always visible
        if symbol.visibility == Visibility::公开 {
            return Ok(true);
        }

        // Private symbols are only visible within the same module
        if let Some(current) = &self.current_module {
            Ok(current == module_name)
        } else {
            Ok(false)
        }
    }

    /// Extract exports from an AST program
    pub fn extract_exports(&self, program: &Program) -> HashMap<String, Symbol> {
        let mut exports = HashMap::new();

        for statement in &program.statements {
            match statement {
                AstNode::函数声明(func) => {
                    if func.visibility == Visibility::公开 {
                        // Extract function signature
                        let signature = Some(FunctionSignature {
                            parameters: func.parameters.iter().map(|p| {
                                let type_str = Self::type_node_to_llvm_type(&p.type_annotation);
                                (p.name.clone(), type_str)
                            }).collect(),
                            return_type: Self::type_node_to_llvm_type(&func.return_type),
                            is_async: false,
                        });

                        exports.insert(func.name.clone(), Symbol {
                            name: func.name.clone(),
                            visibility: func.visibility,
                            kind: SymbolKind::Function,
                            function_signature: signature,
                        });
                    }
                }
                AstNode::异步函数声明(async_func) => {
                    if async_func.visibility == Visibility::公开 {
                        // Extract async function signature
                        let signature = Some(FunctionSignature {
                            parameters: async_func.parameters.iter().map(|p| {
                                let type_str = Self::type_node_to_llvm_type(&p.type_annotation);
                                (p.name.clone(), type_str)
                            }).collect(),
                            return_type: "ptr".to_string(), // Async functions always return ptr (Future)
                            is_async: true,
                        });

                        exports.insert(async_func.name.clone(), Symbol {
                            name: async_func.name.clone(),
                            visibility: async_func.visibility,
                            kind: SymbolKind::Function,
                            function_signature: signature,
                        });
                    }
                }
                AstNode::结构体声明(struct_decl) => {
                    if struct_decl.visibility == Visibility::公开 {
                        exports.insert(struct_decl.name.clone(), Symbol {
                            name: struct_decl.name.clone(),
                            visibility: struct_decl.visibility,
                            kind: SymbolKind::Struct,
                            function_signature: None,
                        });
                    }
                }
                AstNode::枚举声明(enum_decl) => {
                    if enum_decl.visibility == Visibility::公开 {
                        exports.insert(enum_decl.name.clone(), Symbol {
                            name: enum_decl.name.clone(),
                            visibility: enum_decl.visibility,
                            kind: SymbolKind::Enum,
                            function_signature: None,
                        });
                    }
                }
                _ => {}
            }
        }

        exports
    }

    /// Convert TypeNode to LLVM type string
    fn type_node_to_llvm_type(type_annotation: &Option<crate::parser::ast::TypeNode>) -> String {
        use crate::parser::ast::{TypeNode, BasicType};
        match type_annotation {
            Some(TypeNode::基础类型(basic_type)) => {
                match basic_type {
                    BasicType::整数 => "i64".to_string(),
                    BasicType::长整数 => "i64".to_string(),
                    BasicType::短整数 => "i16".to_string(),
                    BasicType::字节 => "i8".to_string(),
                    BasicType::浮点数 => "double".to_string(),
                    BasicType::布尔 => "i1".to_string(),
                    BasicType::字符 => "i8".to_string(),
                    BasicType::字符串 => "ptr".to_string(),
                    BasicType::空 => "void".to_string(),
                    BasicType::数组 => "ptr".to_string(),
                    BasicType::字典 => "ptr".to_string(),
                    BasicType::列表 => "ptr".to_string(),
                    BasicType::集合 => "ptr".to_string(),
                    BasicType::指针 => "ptr".to_string(),
                    BasicType::引用 => "ptr".to_string(),
                    BasicType::可变引用 => "ptr".to_string(),
                }
            }
            _ => "i64".to_string(), // Default to i64
        }
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_registry() {
        let mut registry = ModuleRegistry::new();
        
        let module = Module {
            name: "测试模块".to_string(),
            path: PathBuf::from("测试.qi"),
            package_name: Some("测试包".to_string()),
            exports: HashMap::new(),
            imports: vec![],
        };
        
        registry.register_module(module);
        assert!(registry.get_module("测试模块").is_some());
    }

    #[test]
    fn test_symbol_visibility() {
        let mut registry = ModuleRegistry::new();
        
        let mut exports = HashMap::new();
        exports.insert("公开函数".to_string(), Symbol {
            name: "公开函数".to_string(),
            visibility: Visibility::公开,
            kind: SymbolKind::Function,
            function_signature: None,
        });
        exports.insert("私有函数".to_string(), Symbol {
            name: "私有函数".to_string(),
            visibility: Visibility::私有,
            kind: SymbolKind::Function,
            function_signature: None,
        });
        
        let module = Module {
            name: "测试模块".to_string(),
            path: PathBuf::from("测试.qi"),
            package_name: Some("测试包".to_string()),
            exports,
            imports: vec![],
        };
        
        registry.register_module(module);
        
        // Public symbols are always visible
        assert!(registry.is_symbol_visible("测试模块", "公开函数").unwrap());
        
        // Private symbols are only visible from within the same module
        assert!(registry.is_symbol_visible("测试模块", "私有函数").unwrap());
    }
}
