use anyhow::{bail, Context, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

pub trait PasswordHasher {
    fn hash(&self, password: &[u8]) -> Result<String>;
    fn verify(&self, password: &[u8], encoded_hash: &str) -> Result<bool>;
    fn needs_rehash(&self, encoded_hash: &str) -> bool;
}

/// Argon2id password hashing using OWASP-aligned memory-hard defaults.
#[derive(Debug, Clone)]
pub struct Argon2idPasswordHasher {
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
}

impl Default for Argon2idPasswordHasher {
    fn default() -> Self {
        Self {
            memory_kib: 19_456,
            iterations: 2,
            parallelism: 1,
        }
    }
}

impl Argon2idPasswordHasher {
    pub fn new(memory_kib: u32, iterations: u32, parallelism: u32) -> Result<Self> {
        Params::new(memory_kib, iterations, parallelism, None)
            .context("invalid Argon2id parameters")?;
        Ok(Self {
            memory_kib,
            iterations,
            parallelism,
        })
    }

    fn engine(&self) -> Result<Argon2<'static>> {
        let params = Params::new(self.memory_kib, self.iterations, self.parallelism, None)
            .context("invalid Argon2id parameters")?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

impl PasswordHasher for Argon2idPasswordHasher {
    fn hash(&self, password: &[u8]) -> Result<String> {
        if password.is_empty() {
            bail!("password must not be empty");
        }
        let password = Zeroizing::new(password.to_vec());
        let salt = SaltString::generate(&mut OsRng);
        Ok(self.engine()?.hash_password(&password, &salt)?.to_string())
    }

    fn verify(&self, password: &[u8], encoded_hash: &str) -> Result<bool> {
        let parsed = match PasswordHash::new(encoded_hash) {
            Ok(value) => value,
            Err(_) => return Ok(false),
        };
        let password = Zeroizing::new(password.to_vec());
        Ok(self.engine()?.verify_password(&password, &parsed).is_ok())
    }

    fn needs_rehash(&self, encoded_hash: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(encoded_hash) else {
            return true;
        };
        parsed.algorithm.as_str() != "argon2id"
            || parsed.version != Some(0x13)
            || parsed.params.get_decimal("m") != Some(self.memory_kib)
            || parsed.params.get_decimal("t") != Some(self.iterations)
            || parsed.params.get_decimal("p") != Some(self.parallelism)
    }
}

/// Backward-compatible name retained for source compatibility. It now uses Argon2id.
pub type HmacSha256PasswordHasher = Argon2idPasswordHasher;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionToken {
    pub token: String,
    pub expires_at: u64,
}

impl SessionToken {
    pub fn generate(ttl: Duration) -> Result<Self> {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        Ok(Self {
            token: URL_SAFE_NO_PAD.encode(bytes),
            expires_at: now.saturating_add(ttl.as_secs()),
        })
    }

    pub fn is_expired(&self, now_unix: u64) -> bool {
        now_unix >= self.expires_at
    }
}

pub struct AuditLogger;
impl AuditLogger {
    pub fn log(event_type: &str, details: serde_json::Value) {
        let record = serde_json::json!({
            "event": event_type,
            "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            "details": details
        });
        eprintln!("[AUDIT] {}", record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_passwords() {
        let hasher = Argon2idPasswordHasher::default();
        let hash = hasher.hash(b"secret_password_123").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(hasher.verify(b"secret_password_123", &hash).unwrap());
        assert!(!hasher.verify(b"wrong_password", &hash).unwrap());
        assert!(!hasher.needs_rehash(&hash));
    }

    #[test]
    fn salts_are_unique() {
        let hasher = Argon2idPasswordHasher::default();
        assert_ne!(hasher.hash(b"same").unwrap(), hasher.hash(b"same").unwrap());
    }

    #[test]
    fn creates_256_bit_session_tokens() {
        let token = SessionToken::generate(Duration::from_secs(60)).unwrap();
        assert!(token.token.len() >= 43);
        assert!(!token.is_expired(token.expires_at - 1));
        assert!(token.is_expired(token.expires_at));
    }
}
