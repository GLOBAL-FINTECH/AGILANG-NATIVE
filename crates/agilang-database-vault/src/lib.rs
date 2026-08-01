use agilang_database_audit::AuditChain;
use agilang_database_crypto::EncryptedEnvelope;
use agilang_database_keyring::Keyring;
use agilang_database_security_kernel::{Capability, SecurityKernel, SecurityProfile};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultRecord {
    pub key: String,
    pub envelope: EncryptedEnvelope,
    pub is_one_time: bool,
    pub expires_at: Option<u64>,
}

pub struct Vault {
    pub locked: bool,
    pub keyring: Keyring,
    pub audit: AuditChain,
    pub records: HashMap<String, VaultRecord>,
    pub kernel: SecurityKernel,
}

impl Vault {
    pub fn new() -> Self {
        Self {
            locked: true,
            keyring: Keyring::new(),
            audit: AuditChain::new(),
            records: HashMap::new(),
            kernel: SecurityKernel::new(
                SecurityProfile::AirGapped,
                vec![
                    Capability::VaultRead,
                    Capability::VaultWrite,
                    Capability::TableRead,
                ],
            ),
        }
    }

    pub fn unlock(&mut self, passphrase: &str) -> Result<()> {
        if passphrase.is_empty() {
            bail!("E6402 Vault initialization failed: invalid passphrase");
        }
        self.locked = false;
        self.audit.append("operator", "vault.unlocked");
        Ok(())
    }

    pub fn lock(&mut self) {
        self.locked = true;
        self.audit.append("operator", "vault.locked");
    }

    pub fn put(&mut self, key: &str, secret: &[u8]) -> Result<()> {
        if self.locked {
            bail!("E6401 Vault locked: operation denied");
        }
        self.kernel.authorize(&Capability::VaultWrite)?;

        let envelope = EncryptedEnvelope::encrypt(secret, "dek-v1", key);
        self.records.insert(
            key.to_string(),
            VaultRecord {
                key: key.to_string(),
                envelope,
                is_one_time: false,
                expires_at: None,
            },
        );

        self.audit.append("app", &format!("vault.put:{}", key));
        Ok(())
    }

    pub fn put_once(&mut self, key: &str, secret: &[u8]) -> Result<()> {
        if self.locked {
            bail!("E6401 Vault locked: operation denied");
        }
        self.kernel.authorize(&Capability::VaultWrite)?;

        let envelope = EncryptedEnvelope::encrypt(secret, "dek-v1", key);
        self.records.insert(
            key.to_string(),
            VaultRecord {
                key: key.to_string(),
                envelope,
                is_one_time: true,
                expires_at: None,
            },
        );

        self.audit.append("app", &format!("vault.put_once:{}", key));
        Ok(())
    }

    pub fn get(&mut self, key: &str) -> Result<Vec<u8>> {
        if self.locked {
            bail!("E6401 Vault locked: operation denied");
        }
        self.kernel.authorize(&Capability::VaultRead)?;

        if let Some(record) = self.records.get(key) {
            let decrypted = record.envelope.decrypt(key)?;
            let is_one_time = record.is_one_time;

            if is_one_time {
                self.records.remove(key);
            }

            self.audit.append("app", &format!("vault.get:{}", key));
            Ok(decrypted)
        } else {
            bail!("E6406 Secret not found: `{}`", key);
        }
    }
}

impl Default for Vault {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_lock_unlock_and_one_time_secret_consumption() {
        let mut vault = Vault::new();
        assert!(vault.locked);

        // Put while locked fails
        assert!(vault.put("stripe_key", b"sk_live_123").is_err());

        // Unlock
        vault.unlock("secret-passphrase").unwrap();
        assert!(!vault.locked);

        // Put once
        vault.put_once("one_time_code", b"code_999").unwrap();

        // Read 1 succeeds and consumes secret
        let val1 = vault.get("one_time_code").unwrap();
        assert_eq!(val1, b"code_999");

        // Read 2 fails (already consumed)
        let val2_err = vault.get("one_time_code");
        assert!(val2_err.is_err());
        assert!(val2_err.unwrap_err().to_string().contains("E6406"));
    }
}
