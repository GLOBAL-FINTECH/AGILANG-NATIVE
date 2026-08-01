//! Namespace-isolated, authenticated envelope encryption for AGILANG Vault services.

use aes_gcm::{
    aead::{AeadInPlace, KeyInit},
    Aes256Gcm, Nonce, Tag,
};
use anyhow::{bail, Context, Result};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zeroize::Zeroizing;

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedSecret {
    pub application_id: String,
    pub namespace: String,
    pub key_version: u32,
    pub algorithm: EncryptionAlgorithm,
    pub nonce: Vec<u8>,
    pub wrapped_key_nonce: Vec<u8>,
    pub wrapped_data_key: Vec<u8>,
    pub wrapped_data_key_tag: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub authentication_tag: Vec<u8>,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationIdentity {
    pub application_id: String,
    pub namespace: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub sequence: u64,
    pub application_id: String,
    pub namespace: String,
    pub action: &'static str,
    pub allowed: bool,
    pub timestamp: u64,
}

pub struct VaultCore {
    active_version: u32,
    master_keys: HashMap<u32, Zeroizing<[u8; KEY_BYTES]>>,
    records: HashMap<String, SealedSecret>,
    audit: Vec<AuditEvent>,
}

impl VaultCore {
    pub fn new(master_key: [u8; KEY_BYTES]) -> Self {
        Self {
            active_version: 1,
            master_keys: HashMap::from([(1, Zeroizing::new(master_key))]),
            records: HashMap::new(),
            audit: Vec::new(),
        }
    }

    pub fn active_key_version(&self) -> u32 {
        self.active_version
    }

    pub fn audit_events(&self) -> &[AuditEvent] {
        &self.audit
    }

    pub fn rotate_master_key(&mut self, new_key: [u8; KEY_BYTES], now: u64) -> u32 {
        self.active_version += 1;
        self.master_keys
            .insert(self.active_version, Zeroizing::new(new_key));
        self.audit.push(AuditEvent {
            sequence: self.audit.len() as u64 + 1,
            application_id: "vault-admin".into(),
            namespace: "system".into(),
            action: "key.rotate",
            allowed: true,
            timestamp: now,
        });
        self.active_version
    }

    pub fn seal(
        &mut self,
        identity: &ApplicationIdentity,
        record_name: &str,
        plaintext: &[u8],
        now: u64,
        expires_at: Option<u64>,
    ) -> Result<()> {
        self.validate_record(identity, record_name, now, "secret.seal")?;
        let master_key = self
            .master_keys
            .get(&self.active_version)
            .context("active Vault master key is unavailable")?;
        let mut data_key = Zeroizing::new([0u8; KEY_BYTES]);
        OsRng.fill_bytes(data_key.as_mut());
        let aad = associated_data(identity, record_name, self.active_version);
        let (wrapped_data_key, wrapped_key_nonce, wrapped_data_key_tag) =
            encrypt(master_key.as_ref(), data_key.as_ref(), &aad)?;
        let (ciphertext, nonce, authentication_tag) = encrypt(data_key.as_ref(), plaintext, &aad)?;
        self.records.insert(
            record_key(identity, record_name),
            SealedSecret {
                application_id: identity.application_id.clone(),
                namespace: identity.namespace.clone(),
                key_version: self.active_version,
                algorithm: EncryptionAlgorithm::Aes256Gcm,
                nonce,
                wrapped_key_nonce,
                wrapped_data_key,
                wrapped_data_key_tag,
                ciphertext,
                authentication_tag,
                created_at: now,
                expires_at,
                revoked: false,
            },
        );
        self.audit(identity, "secret.seal", true, now);
        Ok(())
    }

    pub fn open(
        &mut self,
        identity: &ApplicationIdentity,
        record_name: &str,
        now: u64,
    ) -> Result<Zeroizing<Vec<u8>>> {
        self.validate_record(identity, record_name, now, "secret.open")?;
        let key = record_key(identity, record_name);
        let record = self.records.get(&key).context("secret not found")?;
        if record.revoked {
            self.audit(identity, "secret.open", false, now);
            bail!("secret lease is revoked");
        }
        if record.expires_at.is_some_and(|expiry| now >= expiry) {
            self.audit(identity, "secret.open", false, now);
            bail!("secret lease is expired");
        }
        let master_key = self
            .master_keys
            .get(&record.key_version)
            .context("Vault master key version is unavailable")?;
        let aad = associated_data(identity, record_name, record.key_version);
        let data_key = Zeroizing::new(decrypt(
            master_key.as_ref(),
            &record.wrapped_data_key,
            &record.wrapped_key_nonce,
            &record.wrapped_data_key_tag,
            &aad,
        )?);
        if data_key.len() != KEY_BYTES {
            bail!("invalid wrapped data-key length");
        }
        let plaintext = decrypt(
            data_key.as_slice(),
            &record.ciphertext,
            &record.nonce,
            &record.authentication_tag,
            &aad,
        )?;
        self.audit(identity, "secret.open", true, now);
        Ok(Zeroizing::new(plaintext))
    }

    pub fn revoke(
        &mut self,
        identity: &ApplicationIdentity,
        record_name: &str,
        now: u64,
    ) -> Result<()> {
        self.validate_record(identity, record_name, now, "secret.revoke")?;
        self.records
            .get_mut(&record_key(identity, record_name))
            .context("secret not found")?
            .revoked = true;
        self.audit(identity, "secret.revoke", true, now);
        Ok(())
    }

    fn validate_record(
        &mut self,
        identity: &ApplicationIdentity,
        record_name: &str,
        now: u64,
        action: &'static str,
    ) -> Result<()> {
        let valid = !identity.application_id.trim().is_empty()
            && !identity.namespace.trim().is_empty()
            && !record_name.trim().is_empty()
            && !record_name.contains("..")
            && !record_name.starts_with('/');
        if !valid {
            self.audit(identity, action, false, now);
            bail!("invalid or unauthorized Vault namespace record");
        }
        Ok(())
    }

    fn audit(
        &mut self,
        identity: &ApplicationIdentity,
        action: &'static str,
        allowed: bool,
        now: u64,
    ) {
        self.audit.push(AuditEvent {
            sequence: self.audit.len() as u64 + 1,
            application_id: identity.application_id.clone(),
            namespace: identity.namespace.clone(),
            action,
            allowed,
            timestamp: now,
        });
    }

    #[cfg(test)]
    fn record_mut(&mut self, identity: &ApplicationIdentity, name: &str) -> &mut SealedSecret {
        self.records.get_mut(&record_key(identity, name)).unwrap()
    }
}

fn record_key(identity: &ApplicationIdentity, name: &str) -> String {
    format!(
        "{}\0{}\0{}",
        identity.application_id, identity.namespace, name
    )
}

fn associated_data(identity: &ApplicationIdentity, name: &str, version: u32) -> Vec<u8> {
    format!(
        "AGIVLT\0{}\0{}\0{}\0{}",
        identity.application_id, identity.namespace, name, version
    )
    .into_bytes()
}

fn encrypt(key: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| anyhow::anyhow!("invalid AES-256-GCM key"))?;
    let mut nonce = [0u8; NONCE_BYTES];
    OsRng.fill_bytes(&mut nonce);
    let mut ciphertext = plaintext.to_vec();
    let tag = cipher
        .encrypt_in_place_detached(Nonce::from_slice(&nonce), aad, &mut ciphertext)
        .map_err(|_| anyhow::anyhow!("Vault encryption failed"))?;
    Ok((ciphertext, nonce.to_vec(), tag.to_vec()))
}

fn decrypt(key: &[u8], ciphertext: &[u8], nonce: &[u8], tag: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    if nonce.len() != NONCE_BYTES || tag.len() != 16 {
        bail!("invalid AES-256-GCM envelope");
    }
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| anyhow::anyhow!("invalid AES-256-GCM key"))?;
    let mut plaintext = ciphertext.to_vec();
    cipher
        .decrypt_in_place_detached(
            Nonce::from_slice(nonce),
            aad,
            &mut plaintext,
            Tag::from_slice(tag),
        )
        .map_err(|_| anyhow::anyhow!("Vault authentication failed"))?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> ApplicationIdentity {
        ApplicationIdentity {
            application_id: name.into(),
            namespace: format!("apps/{name}/production"),
        }
    }

    #[test]
    fn seals_opens_and_detects_tampering() {
        let identity = app("commodity");
        let mut vault = VaultCore::new([7; 32]);
        vault
            .seal(&identity, "database/password", b"correct horse", 10, None)
            .unwrap();
        assert_eq!(
            vault
                .open(&identity, "database/password", 11)
                .unwrap()
                .as_slice(),
            b"correct horse"
        );
        vault.record_mut(&identity, "database/password").ciphertext[0] ^= 1;
        assert!(vault
            .open(&identity, "database/password", 12)
            .unwrap_err()
            .to_string()
            .contains("authentication"));
    }

    #[test]
    fn isolates_applications_and_namespaces() {
        let owner = app("commodity");
        let attacker = app("other");
        let mut vault = VaultCore::new([8; 32]);
        vault
            .seal(&owner, "signing/key", b"secret", 10, None)
            .unwrap();
        assert!(vault.open(&attacker, "signing/key", 11).is_err());
    }

    #[test]
    fn rotation_preserves_old_records_and_uses_new_version() {
        let identity = app("commodity");
        let mut vault = VaultCore::new([1; 32]);
        vault.seal(&identity, "old", b"before", 10, None).unwrap();
        assert_eq!(vault.rotate_master_key([2; 32], 11), 2);
        vault.seal(&identity, "new", b"after", 12, None).unwrap();
        assert_eq!(
            vault.open(&identity, "old", 13).unwrap().as_slice(),
            b"before"
        );
        assert_eq!(
            vault.open(&identity, "new", 13).unwrap().as_slice(),
            b"after"
        );
    }

    #[test]
    fn enforces_expiry_and_revocation() {
        let identity = app("commodity");
        let mut vault = VaultCore::new([3; 32]);
        vault
            .seal(&identity, "lease", b"short", 10, Some(20))
            .unwrap();
        assert!(vault.open(&identity, "lease", 20).is_err());
        vault.seal(&identity, "revoked", b"gone", 10, None).unwrap();
        vault.revoke(&identity, "revoked", 11).unwrap();
        assert!(vault.open(&identity, "revoked", 12).is_err());
    }
}
