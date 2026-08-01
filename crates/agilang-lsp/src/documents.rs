use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct OpenDocument {
    pub uri: String,
    pub language_id: String,
    pub version: i64,
    pub text: String,
}

pub static DOCUMENTS: Lazy<Mutex<HashMap<String, OpenDocument>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    if !uri.starts_with("file://") {
        return None;
    }
    let mut path_str = &uri[7..];
    if path_str.starts_with('/') && path_str.chars().nth(2) == Some(':') {
        path_str = &path_str[1..];
    }
    let decoded = percent_decode(path_str);
    Some(PathBuf::from(decoded))
}

pub fn path_to_uri(path: &Path) -> String {
    let path_str = path.to_string_lossy().replace('\\', "/");
    let mut encoded = String::new();
    for b in path_str.bytes() {
        if b.is_ascii_alphanumeric()
            || b == b'/'
            || b == b':'
            || b == b'.'
            || b == b'-'
            || b == b'_'
        {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    if encoded.starts_with('/') {
        format!("file://{}", encoded)
    } else {
        format!("file:///{}", encoded)
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
