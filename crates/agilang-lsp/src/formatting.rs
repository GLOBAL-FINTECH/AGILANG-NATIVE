use crate::diagnostics::offset_to_position;
use serde_json::{json, Value};

pub fn format_document(text: &str) -> Value {
    let mut formatted = String::new();
    for line in text.lines() {
        formatted.push_str(line.trim_end());
        formatted.push('\n');
    }

    let (end_line, end_char) = offset_to_position(text, text.len());

    json!([
        {
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": {
                    "line": end_line,
                    "character": end_char
                }
            },
            "newText": formatted
        }
    ])
}
