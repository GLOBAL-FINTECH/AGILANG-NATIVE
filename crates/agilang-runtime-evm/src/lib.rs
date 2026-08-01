//! EVM JSON-RPC runtime module suitable for SIBAQ and other EVM-compatible chains.
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use agilang_runtime_http::{HttpClient, HttpRequest};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub result: serde_json::Value,
}
#[derive(Clone)]
pub struct EvmClient {
    http: HttpClient,
    endpoint: String,
    next_id: std::sync::Arc<AtomicU64>,
}
impl EvmClient {
    pub fn new(http: HttpClient, endpoint: impl Into<String>) -> Self {
        Self {
            http,
            endpoint: endpoint.into(),
            next_id: std::sync::Arc::new(AtomicU64::new(1)),
        }
    }
    pub async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> RuntimeResult<RpcResponse> {
        if method.trim().is_empty() {
            return Err(AgilangError::new(
                ErrorCode::InvalidArgument,
                "JSON-RPC method is required",
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        let response = self
            .http
            .execute(HttpRequest {
                method: "POST".into(),
                url: self.endpoint.clone(),
                query: std::collections::BTreeMap::new(), headers: std::collections::BTreeMap::from([("content-type".into(), "application/json".into())]),
                body: serde_json::to_vec(&body).map_err(json_error)?,
                timeout_ms: Some(30_000),
            })
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(AgilangError::new(
                ErrorCode::Io,
                format!("EVM RPC returned HTTP {}", response.status),
            ));
        }
        let value: serde_json::Value =
            serde_json::from_slice(&response.body).map_err(json_error)?;
        if let Some(error) = value.get("error") {
            return Err(AgilangError::new(
                ErrorCode::InvalidState,
                format!("EVM RPC error: {error}"),
            ));
        }
        Ok(RpcResponse {
            result: value
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        })
    }
    pub async fn chain_id(&self) -> RuntimeResult<u64> {
        let value = self
            .call("eth_chainId", serde_json::json!([]))
            .await?
            .result;
        parse_quantity(value.as_str().ok_or_else(|| {
            AgilangError::new(
                ErrorCode::InvalidState,
                "eth_chainId returned a non-string value",
            )
        })?)
    }
    pub async fn block_number(&self) -> RuntimeResult<u64> {
        let value = self
            .call("eth_blockNumber", serde_json::json!([]))
            .await?
            .result;
        parse_quantity(value.as_str().ok_or_else(|| {
            AgilangError::new(
                ErrorCode::InvalidState,
                "eth_blockNumber returned a non-string value",
            )
        })?)
    }
}
pub fn parse_quantity(value: &str) -> RuntimeResult<u64> {
    let hex = value.strip_prefix("0x").ok_or_else(|| {
        AgilangError::new(
            ErrorCode::InvalidArgument,
            "EVM quantity must start with 0x",
        )
    })?;
    u64::from_str_radix(hex, 16)
        .map_err(|e| AgilangError::new(ErrorCode::InvalidArgument, e.to_string()))
}
fn json_error(error: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidArgument, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_chain_id_1990() {
        assert_eq!(parse_quantity("0x7c6").unwrap(), 1990);
    }
}
