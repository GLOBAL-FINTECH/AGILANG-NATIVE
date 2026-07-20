use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyPurpose {
    DatabasePages,
    WriteAheadLog,
    Backup,
    VaultRecord,
    AuditLog,
    SessionSecret,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEncryptionKey {
    pub id: String,
    pub version: u32,
    pub wrapped_key: Vec<u8>,
    pub purpose: KeyPurpose,
    pub is_active: bool,
}

pub struct Keyring {
    pub active_version: u32,
    pub keys: HashMap<u32, DataEncryptionKey>,
}

impl Keyring {
    pub fn new() -> Self {
        let mut keys = HashMap::new();
        keys.insert(
            1,
            DataEncryptionKey {
                id: "dek-v1".to_string(),
                version: 1,
                wrapped_key: vec![0x11, 0x22, 0x33, 0x44],
                purpose: KeyPurpose::VaultRecord,
                is_active: true,
            },
        );

        Self {
            active_version: 1,
            keys,
        }
    }

    pub fn rotate_key(&mut self) -> u32 {
        let new_version = self.active_version + 1;
        let new_key = DataEncryptionKey {
            id: format!("dek-v{}", new_version),
            version: new_version,
            wrapped_key: vec![0xaa, 0xbb, 0xcc, 0xdd],
            purpose: KeyPurpose::VaultRecord,
            is_active: true,
        };

        if let Some(old) = self.keys.get_mut(&self.active_version) {
            old.is_active = false;
        }

        self.keys.insert(new_version, new_key);
        self.active_version = new_version;
        new_version
    }

    pub fn get_key(&self, version: u32) -> Result<&DataEncryptionKey> {
        if let Some(key) = self.keys.get(&version) {
            Ok(key)
        } else {
            bail!("E6408 Key version {} unavailable", version);
        }
    }
}

impl Default for Keyring {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_rotation_creates_new_version_and_preserves_old() {
        let mut keyring = Keyring::new();
        assert_eq!(keyring.active_version, 1);

        let v2 = keyring.rotate_key();
        assert_eq!(v2, 2);
        assert_eq!(keyring.active_version, 2);

        assert!(keyring.get_key(1).is_ok());
        assert!(keyring.get_key(2).is_ok());
        assert!(keyring.get_key(99).is_err());
    }
}
