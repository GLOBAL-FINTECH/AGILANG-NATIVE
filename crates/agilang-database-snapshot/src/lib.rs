use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub database_id: [u8; 16],
    pub snapshot_id: [u8; 16],
    pub block_height: u64,
    pub checkpoint_lsn: u64,
    pub state_root: [u8; 32],
    pub page_count: u64,
    pub created_at: u64,
    pub key_version: u32,
    pub manifest_signature: Vec<u8>,
}

impl SnapshotManifest {
    pub fn verify_state_root(&self, expected_root: &[u8; 32]) -> Result<()> {
        if &self.state_root != expected_root {
            bail!("E6704 State root mismatch: snapshot state root does not match committed state root");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_manifest_state_root_verification() {
        let manifest = SnapshotManifest {
            database_id: [1u8; 16],
            snapshot_id: [2u8; 16],
            block_height: 100,
            checkpoint_lsn: 500,
            state_root: [0xaa; 32],
            page_count: 50,
            created_at: 1774000000,
            key_version: 1,
            manifest_signature: vec![0x11, 0x22],
        };

        assert!(manifest.verify_state_root(&[0xaa; 32]).is_ok());

        let err = manifest.verify_state_root(&[0xbb; 32]);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6704"));
    }
}
