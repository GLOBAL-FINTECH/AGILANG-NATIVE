//! Bounded network-facing JSON-RPC server for the Native AGILANG blockchain.

use agilang_blockchain_node::BlockchainNode;
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, Semaphore},
    time::{timeout, Duration},
};

#[derive(Debug, Clone)]
pub struct RpcServerConfig {
    pub listen: SocketAddr,
    pub max_connections: usize,
    pub max_request_bytes: usize,
    pub read_timeout_ms: u64,
    pub write_timeout_ms: u64,
}

impl Default for RpcServerConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:8545".parse().expect("static socket address"),
            max_connections: 256,
            max_request_bytes: 1_048_576,
            read_timeout_ms: 15_000,
            write_timeout_ms: 15_000,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    #[serde(default)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i64,
    message: String,
}

pub struct RpcServer {
    config: RpcServerConfig,
    node: Arc<Mutex<BlockchainNode>>,
}

impl RpcServer {
    pub fn new(config: RpcServerConfig, node: Arc<Mutex<BlockchainNode>>) -> RuntimeResult<Self> {
        if config.max_connections == 0 || config.max_request_bytes == 0 {
            return invalid("RPC connection and request limits must be greater than zero");
        }
        Ok(Self { config, node })
    }

    pub async fn serve(self) -> RuntimeResult<()> {
        let listener = TcpListener::bind(self.config.listen).await.map_err(io_error)?;
        let limiter = Arc::new(Semaphore::new(self.config.max_connections));
        loop {
            let (stream, peer) = listener.accept().await.map_err(io_error)?;
            let permit = limiter
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| invalid_error("RPC connection limiter closed"))?;
            let node = self.node.clone();
            let config = self.config.clone();
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(error) = handle_connection(stream, node, config).await {
                    tracing::warn!(%peer, error = %error, "blockchain RPC connection failed");
                }
            });
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    node: Arc<Mutex<BlockchainNode>>,
    config: RpcServerConfig,
) -> RuntimeResult<()> {
    let request = timeout(
        Duration::from_millis(config.read_timeout_ms),
        read_http_request(&mut stream, config.max_request_bytes),
    )
    .await
    .map_err(|_| invalid_error("RPC request read timed out"))??;

    let response = match request.method.as_str() {
        "POST" => dispatch_body(&request.body, node).await,
        "GET" if request.path == "/health" => json!({"status":"ok"}),
        _ => json!({"error":"method not allowed"}),
    };
    let status = if request.method == "POST" || request.path == "/health" {
        "200 OK"
    } else {
        "405 Method Not Allowed"
    };
    let encoded = serde_json::to_vec(&response).map_err(json_error)?;
    let header = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\naccess-control-allow-origin: *\r\n\r\n",
        encoded.len()
    );
    timeout(Duration::from_millis(config.write_timeout_ms), async {
        stream.write_all(header.as_bytes()).await?;
        stream.write_all(&encoded).await?;
        stream.shutdown().await
    })
    .await
    .map_err(|_| invalid_error("RPC response write timed out"))?
    .map_err(io_error)
}

async fn dispatch_body(body: &[u8], node: Arc<Mutex<BlockchainNode>>) -> Value {
    let value: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(error) => return response_error(Value::Null, -32700, format!("parse error: {error}")),
    };
    if let Some(batch) = value.as_array() {
        if batch.is_empty() {
            return response_error(Value::Null, -32600, "empty batch".to_string());
        }
        let mut responses = Vec::with_capacity(batch.len());
        for item in batch {
            if let Some(response) = dispatch_one(item.clone(), node.clone()).await {
                responses.push(response);
            }
        }
        Value::Array(responses)
    } else {
        dispatch_one(value, node).await.unwrap_or(Value::Null)
    }
}

async fn dispatch_one(value: Value, node: Arc<Mutex<BlockchainNode>>) -> Option<Value> {
    let request: RpcRequest = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(error) => return Some(response_error(Value::Null, -32600, error.to_string())),
    };
    let notification = request.id.is_none();
    let id = request.id.unwrap_or(Value::Null);
    if request.jsonrpc != "2.0" || request.method.trim().is_empty() {
        return Some(response_error(id, -32600, "invalid JSON-RPC request".to_string()));
    }
    let result = node.lock().await.rpc(&request.method, request.params);
    if notification {
        return None;
    }
    Some(match result {
        Ok(result) => serde_json::to_value(RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        })
        .unwrap_or_else(|error| response_error(Value::Null, -32603, error.to_string())),
        Err(error) => response_error(id, -32601, error.to_string()),
    })
}

fn response_error(id: Value, code: i64, message: String) -> Value {
    serde_json::to_value(RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(RpcError { code, message }),
    })
    .unwrap_or_else(|_| json!({"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"internal error"}}))
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

async fn read_http_request(stream: &mut TcpStream, max_bytes: usize) -> RuntimeResult<HttpRequest> {
    let mut buffer = Vec::with_capacity(4096);
    let header_end;
    loop {
        if buffer.len() >= max_bytes {
            return invalid("RPC request exceeds configured size limit");
        }
        let mut chunk = [0_u8; 4096];
        let count = stream.read(&mut chunk).await.map_err(io_error)?;
        if count == 0 {
            return invalid("RPC client closed before sending a complete request");
        }
        buffer.extend_from_slice(&chunk[..count]);
        if let Some(index) = find_header_end(&buffer) {
            header_end = index;
            break;
        }
    }
    let header = std::str::from_utf8(&buffer[..header_end]).map_err(|_| invalid_error("invalid HTTP header encoding"))?;
    let mut lines = header.split("\r\n");
    let request_line = lines.next().ok_or_else(|| invalid_error("missing HTTP request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut content_length = 0_usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().map_err(|_| invalid_error("invalid content-length"))?;
            }
        }
    }
    if header_end + 4 + content_length > max_bytes {
        return invalid("RPC request exceeds configured size limit");
    }
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let mut chunk = [0_u8; 4096];
        let count = stream.read(&mut chunk).await.map_err(io_error)?;
        if count == 0 {
            return invalid("RPC request body ended early");
        }
        buffer.extend_from_slice(&chunk[..count]);
        if buffer.len() > max_bytes {
            return invalid("RPC request exceeds configured size limit");
        }
    }
    Ok(HttpRequest {
        method,
        path,
        body: buffer[body_start..body_start + content_length].to_vec(),
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn invalid<T>(message: impl Into<String>) -> RuntimeResult<T> {
    Err(invalid_error(message))
}
fn invalid_error(message: impl Into<String>) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidArgument, message.into())
}
fn io_error(error: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::Io, error.to_string())
}
fn json_error(error: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidState, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_http_header_boundary() {
        assert_eq!(find_header_end(b"POST / HTTP/1.1\r\ncontent-length: 0\r\n\r\n"), Some(38));
    }

    #[test]
    fn default_limits_are_bounded() {
        let config = RpcServerConfig::default();
        assert!(config.max_connections > 0);
        assert!(config.max_request_bytes <= 2 * 1024 * 1024);
    }
}
