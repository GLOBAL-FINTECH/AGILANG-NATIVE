use crate::documents::uri_to_path;
use agilang_source::SourceFile;
use serde_json::{json, Value};

const KEYWORDS: &[&str] = &[
    "fn", "class", "use", "return", "let", "if", "else", "while", "void", "int", "string", "bool",
    "true", "false",
];

pub fn get_completions(uri: &str, text: &str) -> Value {
    let mut items = Vec::new();

    // 1. Keywords
    for kw in KEYWORDS {
        items.push(json!({
            "label": kw,
            "kind": 14, // Keyword
            "detail": "Keyword"
        }));
    }

    // 2. Add Snippets
    items.push(json!({
        "label": "fn snippet",
        "kind": 15, // Snippet
        "insertText": "fn ${1:name}(${2}) -> ${3:void}:\n    ${0}",
        "insertTextFormat": 2, // Snippet format
        "detail": "Function Snippet"
    }));

    items.push(json!({
        "label": "Route get snippet",
        "kind": 15,
        "insertText": "Route.get(\"${1:/path}\", ${2:Controller.action})",
        "insertTextFormat": 2,
        "detail": "Route GET Snippet"
    }));

    items.push(json!({
        "label": "@section snippet",
        "kind": 15,
        "insertText": "@section(\"${1:content}\")\n    ${0}\n@endsection",
        "insertTextFormat": 2,
        "detail": "AGS Section Snippet"
    }));

    // 3. Current File symbols
    if let Some(path) = uri_to_path(uri) {
        let source = SourceFile::new(path, text);
        if let Ok(scopes) = agilang_compiler::symbols(&source) {
            for table in scopes {
                for (name, sym) in table.symbols {
                    let lsp_kind = match sym.kind {
                        agilang_symbols::SymbolKind::Function => 3,  // Function
                        agilang_symbols::SymbolKind::Parameter => 6, // Variable
                        agilang_symbols::SymbolKind::Local => 6,     // Variable
                        agilang_symbols::SymbolKind::Constant => 21, // Constant
                        agilang_symbols::SymbolKind::Builtin => 3,
                        agilang_symbols::SymbolKind::Module => 9, // Module
                    };
                    items.push(json!({
                        "label": name,
                        "kind": lsp_kind,
                        "detail": format!("{:?}", sym.ty)
                    }));
                }
            }
        }
    }

    Value::Array(items)
}
