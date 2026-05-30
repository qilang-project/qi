//! Scope management for Qi language

use crate::lexer::Span;
use crate::semantic::symbol_table::Symbol;

/// Scope manager
pub struct ScopeManager {
    scopes: Vec<Scope>,
    current_scope: usize,
}

/// Scope information
#[derive(Debug, Clone)]
pub struct Scope {
    pub level: usize,
    pub symbols: Vec<Symbol>,
    pub parent: Option<usize>,
    pub span: Span,
}

impl ScopeManager {
    pub fn new() -> Self {
        Self { scopes: vec![Scope::new_global()], current_scope: 0 }
    }

    pub fn enter_scope(&mut self, span: Span) {
        let scope = Scope::new(self.current_scope, span);
        let scope_id = self.scopes.len();
        self.scopes.push(scope);
        self.current_scope = scope_id;
    }

    pub fn exit_scope(&mut self) -> Option<usize> {
        if self.current_scope == 0 {
            return None; // Cannot exit global scope
        }

        let exiting_scope = self.current_scope;
        if let Some(scope) = self.scopes.get(self.current_scope) {
            self.current_scope = scope.parent.unwrap_or(0);
        }

        Some(exiting_scope)
    }

    pub fn current_scope_level(&self) -> usize {
        self.current_scope
    }

    pub fn add_symbol(&mut self, symbol: Symbol) {
        if let Some(scope) = self.scopes.get_mut(self.current_scope) {
            scope.symbols.push(symbol);
        }
    }

    pub fn find_symbol(&self, name: &str) -> Option<&Symbol> {
        let mut current_scope = self.current_scope;

        loop {
            if let Some(scope) = self.scopes.get(current_scope) {
                if let Some(symbol) = scope.symbols.iter().find(|sym| sym.name == name) {
                    return Some(symbol);
                }
            }

            // Move to parent scope
            if let Some(scope) = self.scopes.get(current_scope) {
                match scope.parent {
                    Some(parent) => current_scope = parent,
                    None => break, // Reached global scope
                }
            } else {
                break;
            }
        }

        None
    }

    pub fn get_scope(&self, scope_id: usize) -> Option<&Scope> {
        self.scopes.get(scope_id)
    }
}

impl Scope {
    pub fn new_global() -> Self {
        Self {
            level: 0,
            symbols: Vec::new(),
            parent: None,
            span: Span::new(0, 0),
        }
    }

    pub fn new(parent: usize, span: Span) -> Self {
        Self {
            level: parent + 1,
            symbols: Vec::new(),
            parent: Some(parent),
            span,
        }
    }
}

impl Default for ScopeManager {
    fn default() -> Self {
        Self::new()
    }
}
