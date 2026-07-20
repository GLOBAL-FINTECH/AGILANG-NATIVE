use crate::diagnostics::{offset_to_position, position_to_offset};
use crate::documents::uri_to_path;
use agilang_source::SourceFile;
use serde_json::{json, Value};

pub fn get_hover(uri: &str, line: u32, character: u32, text: &str) -> Value {
    let path = match uri_to_path(uri) {
        Some(p) => p,
        None => return Value::Null,
    };

    let source = SourceFile::new(path, text);
    let offset = position_to_offset(text, line, character);

    if let Ok(scopes) = agilang_compiler::symbols(&source) {
        for table in scopes {
            for (_, sym) in table.symbols {
                if offset >= sym.span.start && offset <= sym.span.end {
                    let kind_str = match sym.kind {
                        agilang_symbols::SymbolKind::Function => "fn",
                        agilang_symbols::SymbolKind::Parameter => "parameter",
                        agilang_symbols::SymbolKind::Local => "local",
                        agilang_symbols::SymbolKind::Constant => "const",
                        agilang_symbols::SymbolKind::Builtin => "builtin",
                        agilang_symbols::SymbolKind::Module => "module",
                    };

                    let hover_text =
                        format!("```agilang\n{} {}: {:?}\n```", kind_str, sym.name, sym.ty);
                    let (start_line, start_char) = offset_to_position(text, sym.span.start);
                    let (end_line, end_char) = offset_to_position(text, sym.span.end);

                    return json!({
                        "contents": {
                            "kind": "markdown",
                            "value": hover_text
                        },
                        "range": {
                            "start": {
                                "line": start_line,
                                "character": start_char,
                            },
                            "end": {
                                "line": end_line,
                                "character": end_char,
                            }
                        }
                    });
                }
            }
        }
    }

    Value::Null
}
