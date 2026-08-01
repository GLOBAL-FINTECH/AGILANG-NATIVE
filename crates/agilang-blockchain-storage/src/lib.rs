//! Persistent Genesis-compatible blockchain storage.

use agilang_blockchain_core::{stable_json, Block, Transaction};
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use rusqlite::{params, Connection, OptionalExtension, Transaction as SqlTransaction};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

pub struct ChainDatabase {
    connection: Connection,
}

impl ChainDatabase {
    pub fn open(path: impl AsRef<Path>) -> RuntimeResult<Self> {
        let connection = Connection::open(path).map_err(storage_error)?;
        let database = Self { connection };
        database.initialize()?;
        Ok(database)
    }

    pub fn memory() -> RuntimeResult<Self> {
        let connection = Connection::open_in_memory().map_err(storage_error)?;
        let database = Self { connection };
        database.initialize()?;
        Ok(database)
    }

    fn initialize(&self) -> RuntimeResult<()> {
        self.connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS blocks (
                    hash TEXT PRIMARY KEY,
                    height INTEGER NOT NULL,
                    parent_hash TEXT NOT NULL,
                    slot INTEGER NOT NULL,
                    proposer TEXT NOT NULL,
                    score TEXT NOT NULL,
                    canonical INTEGER NOT NULL DEFAULT 0,
                    finalized INTEGER NOT NULL DEFAULT 0,
                    json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_blocks_height ON blocks(height);
                 CREATE INDEX IF NOT EXISTS idx_blocks_parent ON blocks(parent_hash);
                 CREATE INDEX IF NOT EXISTS idx_blocks_canonical ON blocks(canonical, height);
                 CREATE TABLE IF NOT EXISTS transactions (
                    hash TEXT PRIMARY KEY,
                    block_hash TEXT,
                    json TEXT NOT NULL,
                    FOREIGN KEY(block_hash) REFERENCES blocks(hash) ON DELETE SET NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_transactions_block ON transactions(block_hash);
                 CREATE TABLE IF NOT EXISTS metadata (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS state (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                 );",
            )
            .map_err(storage_error)
    }

    pub fn put_metadata<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> RuntimeResult<()> {
        let encoded = stable_json(value)?;
        self.connection
            .execute(
                "INSERT INTO metadata(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, encoded],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn metadata<T: DeserializeOwned>(&self, key: &str) -> RuntimeResult<Option<T>> {
        let encoded: Option<String> = self
            .connection
            .query_row("SELECT value FROM metadata WHERE key=?1", params![key], |row| row.get(0))
            .optional()
            .map_err(storage_error)?;
        encoded.map(|value| decode(&value)).transpose()
    }

    pub fn put_state<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> RuntimeResult<()> {
        let encoded = stable_json(value)?;
        self.connection
            .execute(
                "INSERT INTO state(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, encoded],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn state<T: DeserializeOwned>(&self, key: &str) -> RuntimeResult<Option<T>> {
        let encoded: Option<String> = self
            .connection
            .query_row("SELECT value FROM state WHERE key=?1", params![key], |row| row.get(0))
            .optional()
            .map_err(storage_error)?;
        encoded.map(|value| decode(&value)).transpose()
    }

    pub fn all_state(&self) -> RuntimeResult<BTreeMap<String, Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT key, value FROM state ORDER BY key")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(storage_error)?;
        let mut state = BTreeMap::new();
        for row in rows {
            let (key, encoded) = row.map_err(storage_error)?;
            state.insert(key, decode(&encoded)?);
        }
        Ok(state)
    }

    pub fn put_block(&mut self, block: &Block, canonical: bool, finalized: bool) -> RuntimeResult<()> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
        put_block_transaction(&transaction, block, canonical, finalized)?;
        transaction.commit().map_err(storage_error)
    }

    pub fn block(&self, hash: &str) -> RuntimeResult<Option<StoredBlock>> {
        self.connection
            .query_row(
                "SELECT json, canonical, finalized FROM blocks WHERE hash=?1",
                params![hash],
                stored_block_from_row,
            )
            .optional()
            .map_err(storage_error)?
            .map(|result| result)
            .transpose()
    }

    pub fn block_by_height(&self, height: u64, canonical_only: bool) -> RuntimeResult<Option<StoredBlock>> {
        let sql = if canonical_only {
            "SELECT json, canonical, finalized FROM blocks WHERE height=?1 AND canonical=1 ORDER BY score DESC LIMIT 1"
        } else {
            "SELECT json, canonical, finalized FROM blocks WHERE height=?1 ORDER BY canonical DESC, score DESC LIMIT 1"
        };
        self.connection
            .query_row(sql, params![height], stored_block_from_row)
            .optional()
            .map_err(storage_error)?
            .map(|result| result)
            .transpose()
    }

    pub fn transaction(&self, hash: &str) -> RuntimeResult<Option<StoredTransaction>> {
        let row: Option<(String, Option<String>)> = self
            .connection
            .query_row(
                "SELECT json, block_hash FROM transactions WHERE hash=?1",
                params![hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?;
        row.map(|(encoded, block_hash)| {
            Ok(StoredTransaction {
                transaction: decode(&encoded)?,
                block_hash,
            })
        })
        .transpose()
    }

    pub fn has_block(&self, hash: &str) -> RuntimeResult<bool> {
        let found: Option<i64> = self
            .connection
            .query_row("SELECT 1 FROM blocks WHERE hash=?1", params![hash], |row| row.get(0))
            .optional()
            .map_err(storage_error)?;
        Ok(found.is_some())
    }

    pub fn canonical_head(&self) -> RuntimeResult<Option<StoredBlock>> {
        self.connection
            .query_row(
                "SELECT json, canonical, finalized FROM blocks
                 WHERE canonical=1 ORDER BY height DESC, CAST(score AS INTEGER) DESC LIMIT 1",
                [],
                stored_block_from_row,
            )
            .optional()
            .map_err(storage_error)?
            .map(|result| result)
            .transpose()
    }

    pub fn candidate_heads(&self) -> RuntimeResult<Vec<Block>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT parent.json FROM blocks parent
                 LEFT JOIN blocks child ON child.parent_hash = parent.hash
                 WHERE child.hash IS NULL
                 ORDER BY CAST(parent.score AS INTEGER) DESC, parent.height DESC",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        let mut blocks = Vec::new();
        for row in rows {
            blocks.push(decode(&row.map_err(storage_error)?)?);
        }
        Ok(blocks)
    }

    pub fn set_canonical_chain(&mut self, head_hash: &str, finality_depth: u64) -> RuntimeResult<()> {
        let path = self.ancestor_path(head_hash)?;
        if path.is_empty() {
            return Err(AgilangError::new(ErrorCode::InvalidArgument, "canonical head was not found"));
        }
        let head_height = path[0].header.height;
        let finalize_height = head_height.saturating_sub(finality_depth);
        let transaction = self.connection.transaction().map_err(storage_error)?;
        transaction.execute("UPDATE blocks SET canonical=0", []).map_err(storage_error)?;
        for block in &path {
            transaction
                .execute(
                    "UPDATE blocks SET canonical=1, finalized=CASE WHEN height<=?2 THEN 1 ELSE finalized END WHERE hash=?1",
                    params![block.hash, finalize_height],
                )
                .map_err(storage_error)?;
        }
        transaction
            .execute(
                "INSERT INTO metadata(key, value) VALUES('canonical_head', ?1)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![stable_json(head_hash)?],
            )
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)
    }

    pub fn ancestor_path(&self, head_hash: &str) -> RuntimeResult<Vec<Block>> {
        let mut path = Vec::new();
        let mut current = self.block(head_hash)?.map(|stored| stored.block);
        while let Some(block) = current {
            let is_genesis = block.header.height == 0;
            let parent_hash = block.header.parent_hash.clone();
            path.push(block);
            if is_genesis {
                break;
            }
            current = self.block(&parent_hash)?.map(|stored| stored.block);
            if current.is_none() {
                return Err(AgilangError::new(
                    ErrorCode::InvalidState,
                    format!("missing ancestor block {parent_hash}"),
                ));
            }
        }
        Ok(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBlock {
    pub block: Block,
    pub canonical: bool,
    pub finalized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTransaction {
    pub transaction: Transaction,
    pub block_hash: Option<String>,
}

fn put_block_transaction(
    transaction: &SqlTransaction<'_>,
    block: &Block,
    canonical: bool,
    finalized: bool,
) -> RuntimeResult<()> {
    transaction
        .execute(
            "INSERT INTO blocks(hash, height, parent_hash, slot, proposer, score, canonical, finalized, json)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(hash) DO UPDATE SET
                height=excluded.height,
                parent_hash=excluded.parent_hash,
                slot=excluded.slot,
                proposer=excluded.proposer,
                score=excluded.score,
                canonical=excluded.canonical,
                finalized=excluded.finalized,
                json=excluded.json",
            params![
                block.hash,
                block.header.height,
                block.header.parent_hash,
                block.header.slot,
                block.header.proposer,
                block.header.score.to_string(),
                canonical as i64,
                finalized as i64,
                stable_json(block)?,
            ],
        )
        .map_err(storage_error)?;
    for chain_transaction in &block.transactions {
        transaction
            .execute(
                "INSERT INTO transactions(hash, block_hash, json) VALUES(?1, ?2, ?3)
                 ON CONFLICT(hash) DO UPDATE SET block_hash=excluded.block_hash, json=excluded.json",
                params![chain_transaction.hash, block.hash, stable_json(chain_transaction)?],
            )
            .map_err(storage_error)?;
    }
    Ok(())
}

fn stored_block_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeResult<StoredBlock>> {
    let encoded: String = row.get(0)?;
    let canonical: i64 = row.get(1)?;
    let finalized: i64 = row.get(2)?;
    Ok(decode(&encoded).map(|block| StoredBlock {
        block,
        canonical: canonical != 0,
        finalized: finalized != 0,
    }))
}

fn decode<T: DeserializeOwned>(encoded: &str) -> RuntimeResult<T> {
    serde_json::from_str(encoded)
        .map_err(|error| AgilangError::new(ErrorCode::InvalidState, error.to_string()))
}

fn storage_error(error: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::Io, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_blockchain_core::{stable_hash, BlockchainConfig};

    fn chain() -> (BlockchainConfig, Block, Block, Block) {
        let config = BlockchainConfig::default();
        let genesis = Block::genesis(&config, 1_700_000_000_000).unwrap();
        let state_root = stable_hash(&BTreeMap::<String, Value>::new()).unwrap();
        let first = Block::child(
            &config,
            &genesis,
            "validator-1",
            1_700_000_006_000,
            1,
            vec![Transaction::transfer("alice", "bob", 10, 0).unwrap()],
            state_root.clone(),
            100,
            "",
        )
        .unwrap();
        let second = Block::child(
            &config,
            &first,
            "validator-1",
            1_700_000_012_000,
            2,
            vec![],
            state_root,
            200,
            "",
        )
        .unwrap();
        (config, genesis, first, second)
    }

    #[test]
    fn persists_blocks_transactions_and_state() {
        let (_, genesis, first, _) = chain();
        let mut database = ChainDatabase::memory().unwrap();
        database.put_block(&genesis, true, true).unwrap();
        database.put_block(&first, true, false).unwrap();
        database.put_state("balance:alice", &90_u64).unwrap();
        assert_eq!(database.state::<u64>("balance:alice").unwrap(), Some(90));
        assert_eq!(database.canonical_head().unwrap().unwrap().block.hash, first.hash);
        let transaction = database.transaction(&first.transactions[0].hash).unwrap().unwrap();
        assert_eq!(transaction.block_hash.as_deref(), Some(first.hash.as_str()));
    }

    #[test]
    fn switches_canonical_chain_and_finalizes_by_depth() {
        let (_, genesis, first, second) = chain();
        let mut database = ChainDatabase::memory().unwrap();
        database.put_block(&genesis, false, false).unwrap();
        database.put_block(&first, false, false).unwrap();
        database.put_block(&second, false, false).unwrap();
        database.set_canonical_chain(&second.hash, 1).unwrap();
        assert!(database.block(&genesis.hash).unwrap().unwrap().finalized);
        assert!(database.block(&first.hash).unwrap().unwrap().finalized);
        assert!(!database.block(&second.hash).unwrap().unwrap().finalized);
        assert_eq!(database.canonical_head().unwrap().unwrap().block.hash, second.hash);
    }
}
