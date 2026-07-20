use crate::documents::uri_to_path;
use crate::transport::write_framed_message;
use agilang_compiler::check;
use agilang_source::SourceFile;
use serde_json::json;

pub fn offset_to_position(text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let mut line = 0;
    let mut line_start_byte_offset = 0;
    for (i, c) in text.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            line_start_byte_offset = i + 1;
        }
    }

    let segment = &text[line_start_byte_offset..offset];
    let character = segment.encode_utf16().count() as u32;
    (line, character)
}

pub fn position_to_offset(text: &str, line: u32, character: u32) -> usize {
    let mut current_line = 0;
    let mut line_start_byte = 0;
    for (i, c) in text.char_indices() {
        if current_line == line {
            let segment = &text[line_start_byte..];
            let mut utf16_count = 0;
            for (byte_offset, chr) in segment.char_indices() {
                if utf16_count >= character as usize {
                    return line_start_byte + byte_offset;
                }
                utf16_count += chr.len_utf16();
            }
            return text.len();
        }
        if c == '\n' {
            current_line += 1;
            line_start_byte = i + 1;
        }
    }
    text.len()
}

pub fn publish_diagnostics(uri: &str, text: &str) {
    let path = match uri_to_path(uri) {
        Some(p) => p,
        None => return,
    };

    let source = SourceFile::new(path, text);
    let mut lsp_diagnostics = Vec::new();

    if let Err(diagnostics) = check(&source) {
        for diag in diagnostics {
            let (start_line, start_char) = offset_to_position(text, diag.span.start);
            let (end_line, end_char) = offset_to_position(text, diag.span.end);

            let severity = match diag.severity {
                agilang_diagnostics::Severity::Error => 1,
                agilang_diagnostics::Severity::Warning => 2,
                agilang_diagnostics::Severity::Note => 3,
            };

            lsp_diagnostics.push(json!({
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
                "severity": severity,
                "code": diag.code,
                "source": "agilang",
                "message": diag.message,
            }));
        }
    }

    let notification = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": lsp_diagnostics
        }
    });

    if let Ok(msg) = serde_json::to_string(&notification) {
        let _ = write_framed_message(&msg);
    }
}

pub fn clear_diagnostics(uri: &str) {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": []
        }
    });

    if let Ok(msg) = serde_json::to_string(&notification) {
        let _ = write_framed_message(&msg);
    }
}
