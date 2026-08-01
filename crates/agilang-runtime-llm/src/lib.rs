//! Provider-neutral OpenAI-compatible LLM runtime module.
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use agilang_runtime_http::{HttpClient, HttpRequest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResult {
    pub id: Option<String>,
    pub model: Option<String>,
    pub content: String,
    pub raw: serde_json::Value,
}

#[derive(Clone)]
pub struct LlmClient {
    http: HttpClient,
    endpoint: String,
    api_key: Option<String>,
}
impl LlmClient {
    pub fn new(http: HttpClient, endpoint: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            http,
            endpoint: endpoint.into().trim_end_matches('/').to_owned(),
            api_key,
        }
    }
    pub async fn chat(&self, request: ChatRequest) -> RuntimeResult<ChatResult> {
        if request.model.trim().is_empty() || request.messages.is_empty() {
            return Err(AgilangError::new(
                ErrorCode::InvalidArgument,
                "model and messages are required",
            ));
        }
        let mut headers = BTreeMap::from([("content-type".into(), "application/json".into())]);
        if let Some(key) = &self.api_key {
            headers.insert("authorization".into(), format!("Bearer {key}"));
        }
        let response = self
            .http
            .execute(HttpRequest {
                method: "POST".into(),
                url: format!("{}/chat/completions", self.endpoint),
                query: BTreeMap::new(),
                headers,
                body: serde_json::to_vec(&request).map_err(json_error)?,
                timeout_ms: Some(120_000),
            })
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(AgilangError::new(
                ErrorCode::Io,
                format!("LLM provider returned HTTP {}", response.status),
            ));
        }
        let raw: serde_json::Value = serde_json::from_slice(&response.body).map_err(json_error)?;
        let content = raw
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AgilangError::new(
                    ErrorCode::InvalidState,
                    "LLM response did not contain choices[0].message.content",
                )
            })?
            .to_owned();
        Ok(ChatResult {
            id: raw.get("id").and_then(|v| v.as_str()).map(str::to_owned),
            model: raw.get("model").and_then(|v| v.as_str()).map(str::to_owned),
            content,
            raw,
        })
    }
}
fn json_error(error: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidArgument, error.to_string())
}
