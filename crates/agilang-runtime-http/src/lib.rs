//! Native HTTP/1.1, HTTP/2 and TLS client services for AGILANG standard modules.

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, time::Duration};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub query: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default, deserialize_with = "deserialize_body")]
    pub body: Vec<u8>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub final_url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TlsPolicy {
    pub require_https: bool,
    pub allow_invalid_certificates: bool,
}

impl Default for TlsPolicy {
    fn default() -> Self {
        Self {
            require_https: true,
            allow_invalid_certificates: false,
        }
    }
}

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    tls: TlsPolicy,
}

impl HttpClient {
    pub fn new(tls: TlsPolicy) -> RuntimeResult<Self> {
        let client = reqwest::Client::builder()
            .http2_adaptive_window(true)
            .danger_accept_invalid_certs(tls.allow_invalid_certificates)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(network_error)?;
        Ok(Self { client, tls })
    }

    pub async fn execute(&self, request: HttpRequest) -> RuntimeResult<HttpResponse> {
        let url = build_request_url(&request.url, &request.query)?;
        if self.tls.require_https && url.scheme() != "https" {
            return Err(AgilangError::new(
                ErrorCode::PermissionDenied,
                "TLS policy requires an https URL",
            ));
        }
        if !matches!(url.scheme(), "http" | "https") {
            return Err(invalid_input("only http and https URLs are supported"));
        }
        let method = Method::from_bytes(request.method.as_bytes())
            .map_err(|e| invalid_input(e.to_string()))?;
        let mut headers = HeaderMap::new();
        for (name, value) in request.headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| invalid_input(e.to_string()))?;
            let value = HeaderValue::from_str(&value).map_err(|e| invalid_input(e.to_string()))?;
            headers.insert(name, value);
        }
        let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(30_000).clamp(1, 300_000));
        let response = self
            .client
            .request(method, url)
            .headers(headers)
            .body(request.body)
            .timeout(timeout)
            .send()
            .await
            .map_err(network_error)?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_owned()))
            .collect();
        let body = response.bytes().await.map_err(network_error)?.to_vec();
        Ok(HttpResponse {
            status,
            final_url,
            headers,
            body,
        })
    }
}

pub fn build_request_url(base_url: &str, query: &BTreeMap<String, Vec<String>>) -> RuntimeResult<Url> {
    let mut url = Url::parse(base_url).map_err(|e| invalid_input(e.to_string()))?;
    if !query.is_empty() {
        let encoded = encode_query_pairs(query);
        match url.query() {
            Some(existing) if !existing.is_empty() => {
                url.set_query(Some(&format!("{existing}&{encoded}")));
            }
            _ => url.set_query(Some(&encoded)),
        }
    }
    Ok(url)
}

fn encode_query_pairs(query: &BTreeMap<String, Vec<String>>) -> String {
    let mut pairs = Vec::new();
    for (key, values) in query {
        let encoded_key = encode_query_component(key);
        if values.is_empty() {
            pairs.push(encoded_key.clone());
            continue;
        }
        for value in values {
            pairs.push(format!("{encoded_key}={}", encode_query_component(value)));
        }
    }
    pairs.join("&")
}

fn encode_query_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        if is_unreserved(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0F) as usize] as char);
        }
    }
    encoded
}

fn is_unreserved(byte: u8) -> bool {
    matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~')
}

fn invalid_input(message: impl Into<String>) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidArgument, message)
}
fn network_error(error: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::Io, error.to_string())
}

fn deserialize_body<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BodyValue {
        Text(String),
        Bytes(Vec<u8>),
    }

    match Option::<BodyValue>::deserialize(deserializer)? {
        Some(BodyValue::Text(text)) => Ok(text.into_bytes()),
        Some(BodyValue::Bytes(bytes)) => Ok(bytes),
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_plain_http_when_tls_is_required() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = HttpClient::new(TlsPolicy {
            require_https: true,
            allow_invalid_certificates: false,
        })
        .unwrap();
        let err = rt
            .block_on(client.execute(HttpRequest {
                method: "GET".into(),
                url: "http://example.com".into(),
                query: BTreeMap::new(),
                headers: BTreeMap::new(),
                body: vec![],
                timeout_ms: Some(100),
            }))
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::PermissionDenied);
    }

    #[test]
    fn appends_rfc3986_encoded_query_pairs() {
        let query = BTreeMap::from([
            ("a b".to_string(), vec!["c+d".to_string()]),
            ("emoji".to_string(), vec!["🙂".to_string()]),
            ("list".to_string(), vec!["1".to_string(), "2".to_string()]),
        ]);
        let url = build_request_url("https://example.com/path?existing=1", &query).unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/path?existing=1&a%20b=c%2Bd&emoji=%F0%9F%99%82&list=1&list=2"
        );
    }

    #[test]
    fn encodes_empty_query_value_as_bare_key() {
        let query = BTreeMap::from([("flag".to_string(), Vec::new())]);
        let url = build_request_url("https://example.com/path", &query).unwrap();
        assert_eq!(url.as_str(), "https://example.com/path?flag");
    }
}
