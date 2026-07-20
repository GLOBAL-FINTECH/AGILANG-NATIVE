use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsolationLevel {
    ReadCommitted,
    Snapshot,
    Serializable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHeader {
    pub created_by: u64,
    pub committed_at: Option<u64>,
    pub deleted_at: Option<u64>,
    pub previous_version: Option<u64>,
    pub payload_length: u32,
}

pub struct MvccEngine {
    pub current_timestamp: u64,
    pub isolation: IsolationLevel,
}

impl MvccEngine {
    pub fn new(isolation: IsolationLevel) -> Self {
        Self {
            current_timestamp: 100,
            isolation,
        }
    }

    pub fn validate_write(&self, version: &VersionHeader, tx_timestamp: u64) -> Result<()> {
        if let Some(committed) = version.committed_at {
            if committed > tx_timestamp {
                bail!(
                    "E6702 Write conflict: version updated by concurrent transaction at ts {}",
                    committed
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mvcc_write_conflict_detection() {
        let engine = MvccEngine::new(IsolationLevel::Snapshot);
        let version = VersionHeader {
            created_by: 1,
            committed_at: Some(150),
            deleted_at: None,
            previous_version: None,
            payload_length: 32,
        };

        // Tx with read timestamp 100 attempting write on version committed at 150
        let err = engine.validate_write(&version, 100);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6702"));
    }
}
