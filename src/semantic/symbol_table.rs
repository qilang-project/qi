//! Symbol table management for Qi language

use crate::lexer::Span;
use crate::parser::ast::TypeNode;
use std::collections::HashMap;

/// Symbol kinds
#[derive(Debug, Clone)]
pub enum SymbolKind {
    变量,
    函数(FunctionInfo),
    常量,
    类型(TypeInfo),
}

/// Function information
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub parameters: Vec<crate::parser::ast::Parameter>,
    pub return_type: TypeNode,
    pub is_defined: bool,
}

/// Type information
#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub definition: TypeDefinition,
    pub is_builtin: bool,
}

/// Type definition placeholder
#[derive(Debug, Clone)]
pub struct TypeDefinition {
    // TODO: Implement type definition
}

/// Symbol entry
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub type_node: TypeNode,
    pub scope_level: usize,
    pub span: Span,
    pub is_mutable: bool,
}

/// Symbol table
#[derive(Clone)]
pub struct SymbolTable {
    symbols: HashMap<String, Vec<Symbol>>,
    current_scope: usize,
    errors: Vec<ScopeError>,
}

/// Scope errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ScopeError {
    /// Name conflict
    #[error("名称冲突: {name} 已在作用域中定义")]
    NameConflict { name: String, existing_span: Span, new_span: Span },

    /// Undefined symbol
    #[error("未定义的符号: {name}")]
    UndefinedSymbol { name: String, span: Span },
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            current_scope: 0,
            errors: Vec::new(),
        }
    }

    pub fn enter_scope(&mut self) {
        self.current_scope += 1;
    }

    pub fn exit_scope(&mut self) {
        self.symbols.retain(|_, symbols| {
            symbols
                .last()
                .map_or(true, |sym| sym.scope_level <= self.current_scope - 1)
        });
        self.current_scope = self.current_scope.saturating_sub(1);
    }

    pub fn define_symbol(&mut self, symbol: Symbol) -> Result<(), ScopeError> {
        if let Some(existing_symbols) = self.symbols.get(&symbol.name) {
            if let Some(existing) = existing_symbols
                .iter()
                .find(|sym| sym.scope_level == self.current_scope)
            {
                return Err(ScopeError::NameConflict {
                    name: symbol.name.clone(),
                    existing_span: existing.span,
                    new_span: symbol.span,
                });
            }
        }

        self.symbols
            .entry(symbol.name.clone())
            .or_insert_with(Vec::new)
            .push(symbol);

        Ok(())
    }

    pub fn lookup_symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name).and_then(|symbols| {
            symbols
                .iter()
                .filter(|sym| sym.scope_level <= self.current_scope)
                .max_by_key(|sym| sym.scope_level)
        })
    }

    pub fn get_errors(&self) -> &[ScopeError] {
        &self.errors
    }

    pub fn current_scope(&self) -> usize {
        self.current_scope
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
