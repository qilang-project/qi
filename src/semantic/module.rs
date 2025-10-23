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
                        exports.insert(func.name.clone(), Symbol {
                            name: func.name.clone(),
                            visibility: func.visibility,
                            kind: SymbolKind::Function,
                        });
                    }
                }
                AstNode::结构体声明(struct_decl) => {
                    if struct_decl.visibility == Visibility::公开 {
                        exports.insert(struct_decl.name.clone(), Symbol {
                            name: struct_decl.name.clone(),
                            visibility: struct_decl.visibility,
                            kind: SymbolKind::Struct,
                        });
                    }
                }
                AstNode::枚举声明(enum_decl) => {
                    if enum_decl.visibility == Visibility::公开 {
                        exports.insert(enum_decl.name.clone(), Symbol {
                            name: enum_decl.name.clone(),
                            visibility: enum_decl.visibility,
                            kind: SymbolKind::Enum,
                        });
                    }
                }
                _ => {}
            }
        }

        exports
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
        });
        exports.insert("私有函数".to_string(), Symbol {
            name: "私有函数".to_string(),
            visibility: Visibility::私有,
            kind: SymbolKind::Function,
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
