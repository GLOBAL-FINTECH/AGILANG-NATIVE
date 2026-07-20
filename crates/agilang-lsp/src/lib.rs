use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use agilang_compiler::check;
use agilang_source::{SourceFile, Span};

static DOCUMENTS: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

const KEYWORDS: &[&str] = &[
    "fn", "class", "use", "return", "let", "if", "else", "while", "void", "int", "string", "bool",
    "true", "false",
];

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

pub fn run_lsp_server() -> io::Result<()> {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        match read_message(&mut stdin) {
            Ok(msg) => {
                if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&msg) {
                    if req.method == "exit" {
                        break;
                    }

                    let response_content = handle_request(&req);
                    if let Some(resp) = response_content {
                        let _ = write_message(&mut stdout, &resp);
                    }
                }
            }
            Err(_) => {
                break;
            }
        }
    }
    Ok(())
}

fn read_message<R: Read>(reader: &mut R) -> Result<String, Box<dyn std::error::Error>> {
    let mut headers = String::new();
    let mut buf = [0u8; 1];
    loop {
        reader.read_exact(&mut buf)?;
        headers.push(buf[0] as char);
        if headers.ends_with("\r\n\r\n") {
            break;
        }
    }

    let mut content_length = 0;
    for line in headers.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() == 2 {
                content_length = parts[1].trim().parse::<usize>()?;
            }
        }
    }

    if content_length == 0 {
        return Err("Content-Length missing".into());
    }

    let mut payload = vec![0u8; content_length];
    reader.read_exact(&mut payload)?;
    Ok(String::from_utf8(payload)?)
}

fn write_message<W: Write>(writer: &mut W, content: &str) -> io::Result<()> {
    write!(
        writer,
        "Content-Length: {}\r\n\r\n{}",
        content.len(),
        content
    )?;
    writer.flush()
}

fn handle_request(req: &JsonRpcRequest) -> Option<String> {
    match req.method.as_str() {
        "initialize" => {
            let res = serde_json::json!({
                "capabilities": {
                    "textDocumentSync": 1, // Full sync
                    "completionProvider": {
                        "resolveProvider": false,
                        "triggerCharacters": [".", ":"]
                    },
                    "hoverProvider": true,
                    "definitionProvider": true,
                    "documentSymbolProvider": true,
                    "documentFormattingProvider": true
                }
            });
            let resp = JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id.clone().unwrap_or(Value::Null),
                result: Some(res),
                error: None,
            };
            serde_json::to_string(&resp).ok()
        }
        "textDocument/didOpen" => {
            if let Some(params) = &req.params {
                if let Some(doc) = params.get("textDocument") {
                    if let (Some(uri), Some(text)) = (
                        doc.get("uri").and_then(|v| v.as_str()),
                        doc.get("text").and_then(|v| v.as_str()),
                    ) {
                        DOCUMENTS
                            .lock()
                            .unwrap()
                            .insert(uri.to_string(), text.to_string());
                        if let Some(diag_msg) = handle_diagnostics(uri, text) {
                            let _ = write_message(&mut io::stdout(), &diag_msg);
                        }
                    }
                }
            }
            None
        }
        "textDocument/didChange" => {
            if let Some(params) = &req.params {
                if let Some(doc) = params.get("textDocument") {
                    if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                        if let Some(content_changes) =
                            params.get("contentChanges").and_then(|v| v.as_array())
                        {
                            if let Some(change) = content_changes.first() {
                                if let Some(text) = change.get("text").and_then(|v| v.as_str()) {
                                    DOCUMENTS
                                        .lock()
                                        .unwrap()
                                        .insert(uri.to_string(), text.to_string());
                                    if let Some(diag_msg) = handle_diagnostics(uri, text) {
                                        let _ = write_message(&mut io::stdout(), &diag_msg);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        "textDocument/completion" => {
            if let Some(id) = &req.id {
                let mut items = Vec::new();
                if let Some(params) = &req.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|v| v.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let _line = params
                        .get("position")
                        .and_then(|v| v.get("line"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    let _character = params
                        .get("position")
                        .and_then(|v| v.get("character"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;

                    let docs = DOCUMENTS.lock().unwrap();
                    let content = docs.get(uri).map(|s| s.as_str()).unwrap_or("");

                    for kw in KEYWORDS {
                        items.push(serde_json::json!({
                            "label": kw,
                            "kind": 14,
                            "detail": "Keyword"
                        }));
                    }

                    if let Some(path) = uri_to_path(uri) {
                        let source = SourceFile::new(path, content);
                        if let Ok(scopes) = agilang_compiler::symbols(&source) {
                            for table in scopes {
                                for (name, sym) in table.symbols {
                                    let lsp_kind = match sym.kind {
                                        agilang_symbols::SymbolKind::Function => 3,
                                        agilang_symbols::SymbolKind::Parameter => 6,
                                        agilang_symbols::SymbolKind::Local => 6,
                                        agilang_symbols::SymbolKind::Constant => 21,
                                        agilang_symbols::SymbolKind::Builtin => 3,
                                        agilang_symbols::SymbolKind::Module => 9,
                                    };
                                    items.push(serde_json::json!({
                                        "label": name,
                                        "kind": lsp_kind,
                                        "detail": format!("{:?}", sym.ty)
                                    }));
                                }
                            }
                        }
                    }
                }
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(serde_json::json!(items)),
                    error: None,
                };
                serde_json::to_string(&resp).ok()
            } else {
                None
            }
        }
        "textDocument/hover" => {
            if let Some(id) = &req.id {
                let mut result = Value::Null;
                if let Some(params) = &req.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|v| v.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let line = params
                        .get("position")
                        .and_then(|v| v.get("line"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    let character = params
                        .get("position")
                        .and_then(|v| v.get("character"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;

                    let docs = DOCUMENTS.lock().unwrap();
                    let content = docs.get(uri).map(|s| s.as_str()).unwrap_or("");
                    if let Some(hover) = handle_hover(uri, line, character, content) {
                        result = hover;
                    }
                }
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(result),
                    error: None,
                };
                serde_json::to_string(&resp).ok()
            } else {
                None
            }
        }
        "textDocument/definition" => {
            if let Some(id) = &req.id {
                let mut result = Value::Null;
                if let Some(params) = &req.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|v| v.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let line = params
                        .get("position")
                        .and_then(|v| v.get("line"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    let character = params
                        .get("position")
                        .and_then(|v| v.get("character"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;

                    let docs = DOCUMENTS.lock().unwrap();
                    let content = docs.get(uri).map(|s| s.as_str()).unwrap_or("");
                    if let Some(definition) = handle_definition(uri, line, character, content) {
                        result = definition;
                    }
                }
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(result),
                    error: None,
                };
                serde_json::to_string(&resp).ok()
            } else {
                None
            }
        }
        "textDocument/documentSymbol" => {
            if let Some(id) = &req.id {
                let mut result = Value::Null;
                if let Some(params) = &req.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|v| v.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let docs = DOCUMENTS.lock().unwrap();
                    let content = docs.get(uri).map(|s| s.as_str()).unwrap_or("");
                    if let Some(symbols) = handle_document_symbols(uri, content) {
                        result = symbols;
                    }
                }
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(result),
                    error: None,
                };
                serde_json::to_string(&resp).ok()
            } else {
                None
            }
        }
        "textDocument/formatting" => {
            if let Some(id) = &req.id {
                let mut result = Value::Null;
                if let Some(params) = &req.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|v| v.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let docs = DOCUMENTS.lock().unwrap();
                    let content = docs.get(uri).map(|s| s.as_str()).unwrap_or("");
                    if let Some(formatting) = handle_formatting(uri, content) {
                        result = formatting;
                    }
                }
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(result),
                    error: None,
                };
                serde_json::to_string(&resp).ok()
            } else {
                None
            }
        }
        _ => {
            if let Some(id) = &req.id {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(Value::Null),
                    error: None,
                };
                serde_json::to_string(&resp).ok()
            } else {
                None
            }
        }
    }
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    if uri.starts_with("file://") {
        let mut path_str = &uri[7..];
        if path_str.starts_with('/') && path_str.chars().nth(2) == Some(':') {
            path_str = &path_str[1..];
        }
        let decoded = percent_decode(path_str);
        Some(PathBuf::from(decoded))
    } else {
        None
    }
}

fn percent_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let mut hex = String::new();
            if let Some(h1) = chars.next() {
                hex.push(h1);
            }
            if let Some(h2) = chars.next() {
                hex.push(h2);
            }
            if let Ok(val) = u8::from_str_radix(&hex, 16) {
                result.push(val as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn handle_diagnostics(uri: &str, content: &str) -> Option<String> {
    let path = uri_to_path(uri)?;
    let source = SourceFile::new(path, content);
    let mut lsp_diagnostics = Vec::new();

    if let Err(diagnostics) = check(&source) {
        for diag in diagnostics {
            let start_pos = source.position(diag.span.start);
            let end_pos = source.position(diag.span.end);
            let severity = match diag.severity {
                agilang_diagnostics::Severity::Error => 1,
                agilang_diagnostics::Severity::Warning => 2,
                agilang_diagnostics::Severity::Note => 3,
            };

            lsp_diagnostics.push(serde_json::json!({
                "range": {
                    "start": {
                        "line": start_pos.line.saturating_sub(1) as u32,
                        "character": start_pos.column.saturating_sub(1) as u32,
                    },
                    "end": {
                        "line": end_pos.line.saturating_sub(1) as u32,
                        "character": end_pos.column.saturating_sub(1) as u32,
                    }
                },
                "severity": severity,
                "code": diag.code,
                "source": "agilang",
                "message": diag.message,
            }));
        }
    }

    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": lsp_diagnostics
        }
    });
    serde_json::to_string(&notification).ok()
}

fn get_byte_offset(text: &str, line: u32, character: u32) -> usize {
    let mut current_line = 0;
    let mut current_char = 0;
    for (offset, c) in text.char_indices() {
        if current_line == line {
            if current_char == character {
                return offset;
            }
            current_char += 1;
        }
        if c == '\n' {
            current_line += 1;
            current_char = 0;
        }
    }
    text.len()
}

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

fn handle_hover(uri: &str, line: u32, character: u32, content: &str) -> Option<Value> {
    let path = uri_to_path(uri)?;
    let source = SourceFile::new(path, content);
    let offset = get_byte_offset(content, line, character);

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
                    return Some(serde_json::json!({
                        "contents": {
                            "kind": "markdown",
                            "value": hover_text
                        },
                        "range": {
                            "start": {
                                "line": source.position(sym.span.start).line.saturating_sub(1) as u32,
                                "character": source.position(sym.span.start).column.saturating_sub(1) as u32,
                            },
                            "end": {
                                "line": source.position(sym.span.end).line.saturating_sub(1) as u32,
                                "character": source.position(sym.span.end).column.saturating_sub(1) as u32,
                            }
                        }
                    }));
                }
            }
        }
    }
    None
}

fn handle_definition(uri: &str, line: u32, character: u32, content: &str) -> Option<Value> {
    let path = uri_to_path(uri)?;
    let source = SourceFile::new(path, content);
    let offset = get_byte_offset(content, line, character);
    let (word, _) = get_word_at_offset(content, offset)?;

    if let Ok(scopes) = agilang_compiler::symbols(&source) {
        for table in scopes {
            if let Some(sym) = table.get(&word) {
                let start_pos = source.position(sym.span.start);
                let end_pos = source.position(sym.span.end);
                return Some(serde_json::json!({
                    "uri": uri,
                    "range": {
                        "start": {
                            "line": start_pos.line.saturating_sub(1) as u32,
                            "character": start_pos.column.saturating_sub(1) as u32,
                        },
                        "end": {
                            "line": end_pos.line.saturating_sub(1) as u32,
                            "character": end_pos.column.saturating_sub(1) as u32,
                        }
                    }
                }));
            }
        }
    }
    None
}

fn handle_document_symbols(uri: &str, content: &str) -> Option<Value> {
    let path = uri_to_path(uri)?;
    let source = SourceFile::new(path, content);
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
                    agilang_symbols::SymbolKind::Function => 12,
                    agilang_symbols::SymbolKind::Constant => 14,
                    agilang_symbols::SymbolKind::Builtin => 12,
                    agilang_symbols::SymbolKind::Module => 2,
                    _ => 13,
                };
                let start_pos = source.position(sym.span.start);
                let end_pos = source.position(sym.span.end);

                symbols_list.push(serde_json::json!({
                    "name": name,
                    "kind": lsp_kind,
                    "range": {
                        "start": {
                            "line": start_pos.line.saturating_sub(1) as u32,
                            "character": start_pos.column.saturating_sub(1) as u32,
                        },
                        "end": {
                            "line": end_pos.line.saturating_sub(1) as u32,
                            "character": end_pos.column.saturating_sub(1) as u32,
                        }
                    },
                    "selectionRange": {
                        "start": {
                            "line": start_pos.line.saturating_sub(1) as u32,
                            "character": start_pos.column.saturating_sub(1) as u32,
                        },
                        "end": {
                            "line": end_pos.line.saturating_sub(1) as u32,
                            "character": end_pos.column.saturating_sub(1) as u32,
                        }
                    }
                }));
            }
        }
    }
    Some(Value::Array(symbols_list))
}

fn format_code(content: &str) -> String {
    let mut formatted = String::new();
    for line in content.lines() {
        formatted.push_str(line.trim_end());
        formatted.push('\n');
    }
    formatted
}

fn handle_formatting(uri: &str, content: &str) -> Option<Value> {
    let path = uri_to_path(uri)?;
    let source = SourceFile::new(path, content);
    let formatted = format_code(content);
    let end_pos = source.position(content.len());

    Some(serde_json::json!([
        {
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": {
                    "line": end_pos.line.saturating_sub(1) as u32,
                    "character": end_pos.column.saturating_sub(1) as u32
                }
            },
            "newText": formatted
        }
    ]))
}
