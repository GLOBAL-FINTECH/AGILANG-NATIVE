use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRecord {
    pub height: u64,
    pub block_hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub is_finalized: bool,
}

pub struct BlockStore {
    pub blocks: HashMap<u64, BlockRecord>,
}

impl BlockStore {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }

    pub fn append_block(&mut self, block: BlockRecord) -> Result<()> {
        if self.blocks.contains_key(&block.height) {
            bail!(
                "E6701 Immutable block state: block at height {} already exists",
                block.height
            );
        }
        self.blocks.insert(block.height, block);
        Ok(())
    }

    pub fn finalize_block(&mut self, height: u64) -> Result<()> {
        if let Some(block) = self.blocks.get_mut(&height) {
            block.is_finalized = true;
            Ok(())
        } else {
            bail!(
                "E6701 Immutable block state: block height {} not found",
                height
            );
        }
    }
}

impl Default for BlockStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blockchain_store_block_append_and_finalization() {
        let mut store = BlockStore::new();
        let block = BlockRecord {
            height: 1,
            block_hash: [0x11; 32],
            parent_hash: [0x00; 32],
            state_root: [0xaa; 32],
            is_finalized: false,
        };

        assert!(store.append_block(block.clone()).is_ok());

        // Duplicate append fails
        let err = store.append_block(block);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6701"));

        // Finalize succeeds
        assert!(store.finalize_block(1).is_ok());
    }
}
