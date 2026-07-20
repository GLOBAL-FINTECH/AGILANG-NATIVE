use std::collections::HashMap;
use std::net::SocketAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Options,
    Head,
    Patch,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub query: HashMap<String, Vec<String>>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub remote_addr: Option<SocketAddr>,
}

impl Request {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query.get(key).and_then(|v| v.first().map(|s| s.as_str()))
    }
    pub fn query_all(&self, key: &str) -> Vec<&str> {
        self.query
            .get(key)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
    pub fn has_query(&self, key: &str) -> bool {
        self.query.contains_key(key)
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn html(body: &str) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
        Response {
            status: 200,
            headers,
            body: body.as_bytes().to_vec(),
        }
    }
    pub fn json(body: &str) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        Response {
            status: 200,
            headers,
            body: body.as_bytes().to_vec(),
        }
    }
}
