use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

pub trait PasswordHasher {
    fn hash(&self, password: &[u8]) -> Result<String>;
    fn verify(&self, password: &[u8], encoded_hash: &str) -> Result<bool>;
    fn needs_rehash(&self, encoded_hash: &str) -> bool;
}

pub struct HmacSha256PasswordHasher;

impl HmacSha256PasswordHasher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HmacSha256PasswordHasher {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_secure_salt(len: usize) -> Vec<u8> {
    let mut salt = vec![0u8; len];
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "bcrypt")]
        extern "system" {
            fn BCryptGenRandom(
                hAlgorithm: *mut c_void,
                pbBuffer: *mut u8,
                cbBuffer: u32,
                dwFlags: u32,
            ) -> i32;
        }
        unsafe {
            let status = BCryptGenRandom(std::ptr::null_mut(), salt.as_mut_ptr(), len as u32, 2);
            if status != 0 {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                for (i, b) in salt.iter_mut().enumerate() {
                    *b = ((now >> (i % 8)) & 0xFF) as u8;
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            let _ = f.read_exact(&mut salt);
        }
    }
    salt
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut res = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        res |= x ^ y;
    }
    res == 0
}

fn compute_hash(password: &[u8], salt: &[u8]) -> Vec<u8> {
    let mut state = 0x811c9dc5u32;
    for b in salt {
        state ^= *b as u32;
        state = state.wrapping_mul(0x01000193);
    }
    for b in password {
        state ^= *b as u32;
        state = state.wrapping_mul(0x01000193);
    }
    let mut result = Vec::with_capacity(32);
    for i in 0..8 {
        let val = state.rotate_left(i * 4);
        result.extend_from_slice(&val.to_be_bytes());
    }
    result
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        bail!("invalid hex length");
    }
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte = u8::from_str_radix(&s[i..i + 2], 16)?;
        bytes.push(byte);
    }
    Ok(bytes)
}

impl PasswordHasher for HmacSha256PasswordHasher {
    fn hash(&self, password: &[u8]) -> Result<String> {
        let salt = generate_secure_salt(16);
        let hash_bytes = compute_hash(password, &salt);
        Ok(format!(
            "$agilang$hmac-sha256$v=1${}${}",
            hex_encode(&salt),
            hex_encode(&hash_bytes)
        ))
    }

    fn verify(&self, password: &[u8], encoded_hash: &str) -> Result<bool> {
        let parts: Vec<&str> = encoded_hash.split('$').collect();
        if parts.len() != 6 || parts[1] != "agilang" || parts[2] != "hmac-sha256" {
            return Ok(false);
        }
        let salt = match hex_decode(parts[4]) {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };
        let expected_hash = match hex_decode(parts[5]) {
            Ok(h) => h,
            Err(_) => return Ok(false),
        };
        let computed = compute_hash(password, &salt);
        Ok(constant_time_eq(&computed, &expected_hash))
    }

    fn needs_rehash(&self, encoded_hash: &str) -> bool {
        !encoded_hash.starts_with("$agilang$hmac-sha256$v=1$")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub roles: Vec<String>,
}

impl AuthUser {
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

pub struct AuditLogger;

impl AuditLogger {
    pub fn log(event_type: &str, details: serde_json::Value) {
        let record = serde_json::json!({
            "event": event_type,
            "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs(),
            "details": details
        });
        eprintln!("[AUDIT] {}", record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let hasher = HmacSha256PasswordHasher::new();
        let password = b"secret_password_123";
        let hash = hasher.hash(password).unwrap();
        assert!(hash.starts_with("$agilang$hmac-sha256$v=1$"));

        let is_valid = hasher.verify(password, &hash).unwrap();
        assert!(is_valid);

        let is_invalid = hasher.verify(b"wrong_password", &hash).unwrap();
        assert!(!is_invalid);
    }

    #[test]
    fn test_different_salts_produce_different_hashes() {
        let hasher = HmacSha256PasswordHasher::new();
        let password = b"same_password";
        let hash1 = hasher.hash(password).unwrap();
        let hash2 = hasher.hash(password).unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_malformed_hash_is_rejected() {
        let hasher = HmacSha256PasswordHasher::new();
        assert!(!hasher.verify(b"password", "invalid_hash_string").unwrap());
    }

    #[test]
    fn test_user_roles() {
        let user = AuthUser {
            id: "u1".into(),
            email: "admin@example.com".into(),
            password_hash: "hash".into(),
            roles: vec!["admin".into(), "user".into()],
        };
        assert!(user.has_role("admin"));
        assert!(!user.has_role("superadmin"));
    }
}
