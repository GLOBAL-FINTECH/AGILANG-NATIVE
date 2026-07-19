use agilang_source::Span;
use agilang_types::Type;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Parameter,
    Local,
    Constant,
    Builtin,
    Module,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub ty: Type,
    pub span: Span,
    pub mutable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolTable {
    pub symbols: HashMap<String, Symbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    pub fn insert(&mut self, symbol: Symbol) -> Result<(), Symbol> {
        if self.symbols.contains_key(&symbol.name) {
            return Err(self.symbols.get(&symbol.name).unwrap().clone());
        }
        self.symbols.insert(symbol.name.clone(), symbol);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}
