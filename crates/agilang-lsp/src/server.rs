use serde_json::Value;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::completion::get_completions;
use crate::definition::get_definition;
use crate::diagnostics::{clear_diagnostics, publish_diagnostics};
use crate::documents::{OpenDocument, DOCUMENTS};
use crate::formatting::format_document;
use crate::hover::get_hover;
use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::symbols::get_document_symbols;
use crate::transport::{read_framed_message, write_framed_message};

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn run_lsp_server() -> io::Result<()> {
    eprintln!("AGILANG LSP: Server process started over stdio");
    let mut stdin = io::stdin();

    while let Ok(msg) = read_framed_message(&mut stdin) {
        let req: JsonRpcRequest = match serde_json::from_str(&msg) {
            Ok(r) => r,
            Err(_) => {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: Value::Null,
                    result: None,
                    error: Some(
                        serde_json::to_value(JsonRpcError::new(-32700, "Parse error")).unwrap(),
                    ),
                };
                if let Ok(out) = serde_json::to_string(&resp) {
                    let _ = write_framed_message(&out);
                }
                continue;
            }
        };

        if req.method == "exit" {
            eprintln!("AGILANG LSP: Exit signal received. Terminating process.");
            if SHUTDOWN.load(Ordering::SeqCst) {
                std::process::exit(0);
            } else {
                std::process::exit(1);
            }
        }

        if let Some(resp) = dispatch_request(&req) {
            let _ = write_framed_message(&resp);
        }
    }
    Ok(())
}

fn dispatch_request(req: &JsonRpcRequest) -> Option<String> {
    match req.method.as_str() {
        "initialize" => {
            INITIALIZED.store(true, Ordering::SeqCst);
            eprintln!("AGILANG LSP: Handshake initialize completed");
            let res = serde_json::json!({
                "capabilities": {
                    "textDocumentSync": {
                        "openClose": true,
                        "change": 1, // Full document sync
                        "save": {
                            "includeText": false
                        }
                    },
                    "completionProvider": {
                        "resolveProvider": false,
                        "triggerCharacters": [".", ":"]
                    },
                    "hoverProvider": true,
                    "definitionProvider": true,
                    "documentSymbolProvider": true,
                    "documentFormattingProvider": true
                },
                "serverInfo": {
                    "name": "AGILANG Language Server",
                    "version": "0.5.0"
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
        "initialized" => {
            eprintln!("AGILANG LSP: Client sent initialized notification");
            None
        }
        "shutdown" => {
            SHUTDOWN.store(true, Ordering::SeqCst);
            eprintln!("AGILANG LSP: Shutdown requested");
            let resp = JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id.clone().unwrap_or(Value::Null),
                result: Some(Value::Null),
                error: None,
            };
            serde_json::to_string(&resp).ok()
        }
        "$/cancelRequest" => None,
        "textDocument/didOpen" => {
            if let Some(params) = &req.params {
                if let Some(doc) = params.get("textDocument") {
                    if let (Some(uri), Some(text)) = (
                        doc.get("uri").and_then(|v| v.as_str()),
                        doc.get("text").and_then(|v| v.as_str()),
                    ) {
                        let version = doc.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
                        let lang = doc
                            .get("languageId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("agilang")
                            .to_string();
                        DOCUMENTS.lock().unwrap().insert(
                            uri.to_string(),
                            OpenDocument {
                                uri: uri.to_string(),
                                language_id: lang,
                                version,
                                text: text.to_string(),
                            },
                        );
                        publish_diagnostics(uri, text);
                    }
                }
            }
            None
        }
        "textDocument/didChange" => {
            if let Some(params) = &req.params {
                if let Some(doc) = params.get("textDocument") {
                    if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                        let version = doc.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
                        if let Some(content_changes) =
                            params.get("contentChanges").and_then(|v| v.as_array())
                        {
                            if let Some(change) = content_changes.first() {
                                if let Some(text) = change.get("text").and_then(|v| v.as_str()) {
                                    if let Some(existing) = DOCUMENTS.lock().unwrap().get_mut(uri) {
                                        existing.version = version;
                                        existing.text = text.to_string();
                                    }
                                    publish_diagnostics(uri, text);
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        "textDocument/didSave" => None,
        "textDocument/didClose" => {
            if let Some(params) = &req.params {
                if let Some(doc) = params.get("textDocument") {
                    if let Some(uri) = doc.get("uri").and_then(|v| v.as_str()) {
                        DOCUMENTS.lock().unwrap().remove(uri);
                        clear_diagnostics(uri);
                    }
                }
            }
            None
        }
        "textDocument/completion" => {
            if let Some(id) = &req.id {
                let mut result = Value::Array(Vec::new());
                if let Some(params) = &req.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|v| v.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let docs = DOCUMENTS.lock().unwrap();
                    let text = docs.get(uri).map(|d| d.text.as_str()).unwrap_or("");
                    result = get_completions(uri, text);
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
                    let text = docs.get(uri).map(|d| d.text.as_str()).unwrap_or("");
                    result = get_hover(uri, line, character, text);
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
                    let text = docs.get(uri).map(|d| d.text.as_str()).unwrap_or("");
                    result = get_definition(uri, line, character, text);
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
                let mut result = Value::Array(Vec::new());
                if let Some(params) = &req.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|v| v.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let docs = DOCUMENTS.lock().unwrap();
                    let text = docs.get(uri).map(|d| d.text.as_str()).unwrap_or("");
                    result = get_document_symbols(uri, text);
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
                    let text = docs.get(uri).map(|d| d.text.as_str()).unwrap_or("");
                    result = format_document(text);
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
                    result: None,
                    error: Some(
                        serde_json::to_value(JsonRpcError::new(-32601, "Method not found"))
                            .unwrap(),
                    ),
                };
                serde_json::to_string(&resp).ok()
            } else {
                None
            }
        }
    }
}
