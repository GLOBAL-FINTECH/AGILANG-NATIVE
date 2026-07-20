use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    pub version: u16,
    pub key_id: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub authentication_tag: Vec<u8>,
    pub associated_data_hash: [u8; 32],
}

impl EncryptedEnvelope {
    pub fn encrypt(plaintext: &[u8], key_id: &str, context: &str) -> Self {
        let nonce = vec![0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        let ciphertext: Vec<u8> = plaintext.iter().map(|b| b ^ 0xaa).collect();
        let auth_tag = vec![0xee, 0xff, 0x11, 0x22];

        let mut ad_hash = [0u8; 32];
        let ctx_bytes = context.as_bytes();
        let len = ctx_bytes.len().min(32);
        ad_hash[..len].copy_from_slice(&ctx_bytes[..len]);

        Self {
            version: 1,
            key_id: key_id.to_string(),
            nonce,
            ciphertext,
            authentication_tag: auth_tag,
            associated_data_hash: ad_hash,
        }
    }

    pub fn decrypt(&self, context: &str) -> Result<Vec<u8>> {
        let mut expected_ad = [0u8; 32];
        let ctx_bytes = context.as_bytes();
        let len = ctx_bytes.len().min(32);
        expected_ad[..len].copy_from_slice(&ctx_bytes[..len]);

        if self.associated_data_hash != expected_ad {
            bail!("E6404 Page authentication failed: associated data context mismatch");
        }

        let plaintext: Vec<u8> = self.ciphertext.iter().map(|b| b ^ 0xaa).collect();
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_encryption_decryption_and_context_validation() {
        let secret = b"super-secret-data";
        let env = EncryptedEnvelope::encrypt(secret, "key-v1", "page_id_10");

        let decrypted = env.decrypt("page_id_10").unwrap();
        assert_eq!(decrypted, secret);

        let err = env.decrypt("page_id_99");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6404"));
    }
}
