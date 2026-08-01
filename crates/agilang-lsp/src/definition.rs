use crate::diagnostics::{offset_to_position, position_to_offset};
use crate::documents::uri_to_path;
use agilang_source::{SourceFile, Span};
use serde_json::{json, Value};

fn get_word_at_offset(text: &str, offset: usize) -> Option<(String, Span)> {
    if offset >= text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut start = offset;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    let mut end = offset;
    while end < text.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if start < end {
        let word = String::from_utf8_lossy(&bytes[start..end]).to_string();
        Some((word, Span::new(start, end)))
    } else {
        None
    }
}

pub fn get_definition(uri: &str, line: u32, character: u32, text: &str) -> Value {
    let path = match uri_to_path(uri) {
        Some(p) => p,
        None => return Value::Null,
    };

    let source = SourceFile::new(path, text);
    let offset = position_to_offset(text, line, character);
    let (word, _) = match get_word_at_offset(text, offset) {
        Some(w) => w,
        None => return Value::Null,
    };

    if let Ok(scopes) = agilang_compiler::symbols(&source) {
        for table in scopes {
            if let Some(sym) = table.get(&word) {
                let (start_line, start_char) = offset_to_position(text, sym.span.start);
                let (end_line, end_char) = offset_to_position(text, sym.span.end);
                return json!({
                    "uri": uri,
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

    Value::Null
}
