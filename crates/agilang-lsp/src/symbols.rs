use crate::diagnostics::offset_to_position;
use crate::documents::uri_to_path;
use agilang_source::SourceFile;
use serde_json::{json, Value};

pub fn get_document_symbols(uri: &str, text: &str) -> Value {
    let path = match uri_to_path(uri) {
        Some(p) => p,
        None => return Value::Array(Vec::new()),
    };

    let source = SourceFile::new(path, text);
    let mut symbols_list = Vec::new();

    if let Ok(scopes) = agilang_compiler::symbols(&source) {
        for table in scopes {
            for (name, sym) in table.symbols {
                if sym.kind == agilang_symbols::SymbolKind::Local
                    || sym.kind == agilang_symbols::SymbolKind::Parameter
                {
                    continue;
                }
                let lsp_kind = match sym.kind {
                    agilang_symbols::SymbolKind::Function => 12, // Function
                    agilang_symbols::SymbolKind::Constant => 14, // Constant
                    agilang_symbols::SymbolKind::Builtin => 12,
                    agilang_symbols::SymbolKind::Module => 2, // Module
                    _ => 13,                                  // Variable
                };

                let (start_line, start_char) = offset_to_position(text, sym.span.start);
                let (end_line, end_char) = offset_to_position(text, sym.span.end);

                symbols_list.push(json!({
                    "name": name,
                    "kind": lsp_kind,
                    "range": {
                        "start": {
                            "line": start_line,
                            "character": start_char,
                        },
                        "end": {
                            "line": end_line,
                            "character": end_char,
                        }
                    },
                    "selectionRange": {
                        "start": {
                            "line": start_line,
                            "character": start_char,
                        },
                        "end": {
                            "line": end_line,
                            "character": end_char,
                        }
                    }
                }));
            }
        }
    }

    Value::Array(symbols_list)
}
