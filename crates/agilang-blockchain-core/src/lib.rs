//! Native AGILANG blockchain primitives derived from the canonical Genesis
//! behavior in `GLOBAL-FINTECH/agilang/agilang/blockchain.py`.
//!
//! This crate intentionally starts with deterministic, consensus-visible data
//! structures. Networking, persistent storage, execution and consensus engines
//! build on these types rather than redefining transaction or block hashing.

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const ZERO_HASH: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000000";

/// Canonical Genesis-compatible chain configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockchainConfig {
    pub chain_id: u64,
    pub name: String,
    pub consensus_mode: ConsensusMode,
    pub slot_seconds: u64,
    pub epoch_length: u64,
    pub block_gas_limit: u64,
    pub max_block_txs: usize,
    pub mempool_max_txs: usize,
    pub mempool_min_gas_price: u128,
    pub finality_depth: u64,
    pub validators: BTreeMap<String, u64>,
    pub delegates: Vec<String>,
    pub delegations: BTreeMap<String, Value>,
    pub validator_signing_keys: BTreeMap<String, String>,
    pub genesis_state: BTreeMap<String, Value>,
    pub fork_choice: String,
    pub allow_empty_blocks: bool,
    pub evm_enabled: bool,
    pub strict_accounting: bool,
    pub enforce_nonce_order: bool,
    pub require_block_signatures: bool,
    pub mainnet_profile: bool,
    pub min_validator_stake: u64,
    pub max_clock_drift_ms: u64,
    pub dev_allow_any_proposer: bool,
}

impl Default for BlockchainConfig {
    fn default() -> Self {
        let mut validators = BTreeMap::new();
        validators.insert("validator-1".to_string(), 100);
        Self {
            chain_id: 7777,
            name: "agilang-chain".to_string(),
            consensus_mode: ConsensusMode::ProofOfStake,
            slot_seconds: 6,
            epoch_length: 32,
            block_gas_limit: 30_000_000,
            max_block_txs: 1_024,
            mempool_max_txs: 100_000,
            mempool_min_gas_price: 0,
            finality_depth: 8,
            validators,
            delegates: Vec::new(),
            delegations: BTreeMap::new(),
            validator_signing_keys: BTreeMap::new(),
            genesis_state: BTreeMap::new(),
            fork_choice: "stake_weighted_height".to_string(),
            allow_empty_blocks: true,
            evm_enabled: true,
            strict_accounting: false,
            enforce_nonce_order: true,
            require_block_signatures: false,
            mainnet_profile: false,
            min_validator_stake: 1,
            max_clock_drift_ms: 120_000,
            dev_allow_any_proposer: false,
        }
    }
}

impl BlockchainConfig {
    /// Apply the stricter profile used by Genesis `blockchain_mainnet_config`.
    pub fn hardened_mainnet(mut self) -> Self {
        self.mainnet_profile = true;
        self.strict_accounting = true;
        self.enforce_nonce_order = true;
        self.require_block_signatures = true;
        self.finality_depth = self.finality_depth.max(32);
        self.mempool_min_gas_price = self.mempool_min_gas_price.max(1);
        if self.fork_choice.trim().is_empty() {
            self.fork_choice = "stake_weighted_height".to_string();
        }
        self
    }

    pub fn validate(&self) -> RuntimeResult<()> {
        if self.chain_id == 0 {
            return invalid("chain_id must be greater than zero");
        }
        if self.name.trim().is_empty() {
            return invalid("chain name is required");
        }
        if self.slot_seconds == 0 || self.epoch_length == 0 {
            return invalid("slot_seconds and epoch_length must be greater than zero");
        }
        if self.block_gas_limit == 0 || self.max_block_txs == 0 || self.mempool_max_txs == 0 {
            return invalid("block and mempool limits must be greater than zero");
        }
        if self.validators.is_empty() && self.consensus_mode != ConsensusMode::Development {
            return invalid("at least one validator is required outside development consensus");
        }
        if self
            .validators
            .values()
            .any(|stake| *stake < self.min_validator_stake)
        {
            return invalid("validator stake is below min_validator_stake");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusMode {
    #[serde(rename = "pos")]
    ProofOfStake,
    #[serde(rename = "dpos")]
    DelegatedProofOfStake,
    #[serde(rename = "dev")]
    Development,
}

impl ConsensusMode {
    pub fn parse(value: &str) -> RuntimeResult<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "pos" | "proof_of_stake" | "proof_of_stake_engine" => Ok(Self::ProofOfStake),
            "dpos" | "dpo" | "delegated_pos" | "delegated_proof_of_stake" => {
                Ok(Self::DelegatedProofOfStake)
            }
            "dev" | "developer" | "dev_consensus" => Ok(Self::Development),
            _ => invalid(format!(
                "unsupported consensus mode: {value:?}; expected pos, dpos/dpo or dev"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    #[serde(rename = "from")]
    pub sender: String,
    pub to: String,
    pub value: u128,
    pub data: String,
    pub nonce: u64,
    pub gas_limit: u64,
    pub gas_price: u128,
    #[serde(rename = "type")]
    pub tx_type: String,
    pub signature: String,
    pub metadata: BTreeMap<String, Value>,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TransactionHashPayload<'a> {
    #[serde(rename = "from")]
    sender: &'a str,
    to: &'a str,
    value: u128,
    data: &'a str,
    nonce: u64,
    gas_limit: u64,
    gas_price: u128,
    #[serde(rename = "type")]
    tx_type: &'a str,
    signature: &'a str,
    metadata: &'a BTreeMap<String, Value>,
}

impl Transaction {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sender: impl Into<String>,
        to: impl Into<String>,
        value: u128,
        data: impl Into<String>,
        nonce: u64,
        gas_limit: u64,
        gas_price: u128,
        tx_type: impl Into<String>,
        signature: Option<String>,
        metadata: BTreeMap<String, Value>,
    ) -> RuntimeResult<Self> {
        let mut tx = Self {
            sender: sender.into(),
            to: to.into(),
            value,
            data: normalize_hex_data(data.into()),
            nonce,
            gas_limit,
            gas_price,
            tx_type: tx_type.into(),
            signature: signature.unwrap_or_default(),
            metadata,
            hash: String::new(),
        };
        tx.validate_shape()?;
        tx.hash = tx.calculate_hash()?;
        Ok(tx)
    }

    pub fn transfer(
        sender: impl Into<String>,
        to: impl Into<String>,
        value: u128,
        nonce: u64,
    ) -> RuntimeResult<Self> {
        Self::new(
            sender,
            to,
            value,
            "0x",
            nonce,
            21_000,
            0,
            "transfer",
            None,
            BTreeMap::new(),
        )
    }

    pub fn calculate_hash(&self) -> RuntimeResult<String> {
        stable_hash(&TransactionHashPayload {
            sender: &self.sender,
            to: &self.to,
            value: self.value,
            data: &self.data,
            nonce: self.nonce,
            gas_limit: self.gas_limit,
            gas_price: self.gas_price,
            tx_type: &self.tx_type,
            signature: &self.signature,
            metadata: &self.metadata,
        })
    }

    pub fn validate(&self) -> RuntimeResult<()> {
        self.validate_shape()?;
        let expected = self.calculate_hash()?;
        if self.hash != expected {
            return invalid(format!(
                "transaction hash mismatch: expected {expected}, received {}",
                self.hash
            ));
        }
        Ok(())
    }

    fn validate_shape(&self) -> RuntimeResult<()> {
        if self.sender.trim().is_empty() {
            return invalid("transaction sender is required");
        }
        if self.gas_limit == 0 {
            return invalid("transaction gas_limit must be greater than zero");
        }
        if self.tx_type.trim().is_empty() {
            return invalid("transaction type is required");
        }
        if !self.data.starts_with("0x") {
            return invalid("transaction data must start with 0x");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockHeader {
    pub chain_id: u64,
    pub height: u64,
    pub parent_hash: String,
    pub timestamp_ms: u64,
    pub slot: u64,
    pub epoch: u64,
    pub proposer: String,
    pub transaction_root: String,
    pub state_root: String,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub score: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub validator_signature: String,
    pub hash: String,
}

#[derive(Debug, Serialize)]
struct BlockHashPayload<'a> {
    header: &'a BlockHeader,
    transactions: &'a [Transaction],
    validator_signature: &'a str,
}

impl Block {
    pub fn genesis(config: &BlockchainConfig, timestamp_ms: u64) -> RuntimeResult<Self> {
        config.validate()?;
        let state_root = stable_hash(&config.genesis_state)?;
        Self::new(
            BlockHeader {
                chain_id: config.chain_id,
                height: 0,
                parent_hash: ZERO_HASH.to_string(),
                timestamp_ms,
                slot: 0,
                epoch: 0,
                proposer: "genesis".to_string(),
                transaction_root: merkle_root::<Transaction>(&[])?,
                state_root,
                gas_limit: config.block_gas_limit,
                gas_used: 0,
                score: 0,
            },
            Vec::new(),
            String::new(),
        )
    }

    pub fn child(
        config: &BlockchainConfig,
        parent: &Block,
        proposer: impl Into<String>,
        timestamp_ms: u64,
        slot: u64,
        transactions: Vec<Transaction>,
        state_root: impl Into<String>,
        score: u128,
        validator_signature: impl Into<String>,
    ) -> RuntimeResult<Self> {
        if transactions.len() > config.max_block_txs {
            return invalid("block exceeds max_block_txs");
        }
        let gas_used = transactions
            .iter()
            .try_fold(0_u64, |total, tx| total.checked_add(tx.gas_limit))
            .ok_or_else(|| AgilangError::new(ErrorCode::InvalidArgument, "block gas overflow"))?;
        if gas_used > config.block_gas_limit {
            return invalid("block exceeds block_gas_limit");
        }
        for tx in &transactions {
            tx.validate()?;
            if tx.gas_price < config.mempool_min_gas_price {
                return invalid("transaction gas price is below chain minimum");
            }
        }
        let proposer = proposer.into();
        if !config.dev_allow_any_proposer
            && config.consensus_mode != ConsensusMode::Development
            && !config.validators.contains_key(&proposer)
        {
            return invalid("block proposer is not an active validator");
        }
        let signature = validator_signature.into();
        if config.require_block_signatures && signature.is_empty() {
            return invalid("validator signature is required");
        }
        let epoch = slot / config.epoch_length;
        let transaction_root = merkle_root(&transactions)?;
        Self::new(
            BlockHeader {
                chain_id: config.chain_id,
                height: parent.header.height + 1,
                parent_hash: parent.hash.clone(),
                timestamp_ms,
                slot,
                epoch,
                proposer,
                transaction_root,
                state_root: state_root.into(),
                gas_limit: config.block_gas_limit,
                gas_used,
                score,
            },
            transactions,
            signature,
        )
    }

    pub fn new(
        header: BlockHeader,
        transactions: Vec<Transaction>,
        validator_signature: String,
    ) -> RuntimeResult<Self> {
        let mut block = Self {
            header,
            transactions,
            validator_signature,
            hash: String::new(),
        };
        block.hash = block.calculate_hash()?;
        Ok(block)
    }

    pub fn calculate_hash(&self) -> RuntimeResult<String> {
        stable_hash(&BlockHashPayload {
            header: &self.header,
            transactions: &self.transactions,
            validator_signature: &self.validator_signature,
        })
    }

    pub fn validate(&self, config: &BlockchainConfig, parent: Option<&Block>) -> RuntimeResult<()> {
        config.validate()?;
        if self.header.chain_id != config.chain_id {
            return invalid("block chain_id does not match chain configuration");
        }
        if self.transactions.len() > config.max_block_txs {
            return invalid("block exceeds max_block_txs");
        }
        for tx in &self.transactions {
            tx.validate()?;
        }
        let expected_root = merkle_root(&self.transactions)?;
        if self.header.transaction_root != expected_root {
            return invalid("block transaction_root mismatch");
        }
        if self.header.gas_used > self.header.gas_limit
            || self.header.gas_limit != config.block_gas_limit
        {
            return invalid("invalid block gas accounting");
        }
        if config.require_block_signatures && self.validator_signature.is_empty() {
            return invalid("validator signature is required");
        }
        match parent {
            Some(parent) => {
                if self.header.height != parent.header.height + 1 {
                    return invalid("block height does not follow parent");
                }
                if self.header.parent_hash != parent.hash {
                    return invalid("block parent_hash does not match parent");
                }
                if self.header.timestamp_ms < parent.header.timestamp_ms {
                    return invalid("block timestamp is earlier than parent");
                }
            }
            None if self.header.height != 0 || self.header.parent_hash != ZERO_HASH => {
                return invalid("non-genesis block requires a parent");
            }
            None => {}
        }
        let expected_hash = self.calculate_hash()?;
        if self.hash != expected_hash {
            return invalid("block hash mismatch");
        }
        Ok(())
    }
}

/// Serialize with the same consensus-visible rules as Genesis
/// `json.dumps(..., sort_keys=True, separators=(",", ":"))`.
pub fn stable_json<T: Serialize + ?Sized>(value: &T) -> RuntimeResult<String> {
    let value = serde_json::to_value(value)
        .map_err(|error| AgilangError::new(ErrorCode::InvalidArgument, error.to_string()))?;
    let canonical = canonicalize_json(value);
    serde_json::to_string(&canonical)
        .map_err(|error| AgilangError::new(ErrorCode::InvalidArgument, error.to_string()))
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(canonicalize_json).collect())
        }
        Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut canonical = Map::new();
            for (key, value) in entries {
                canonical.insert(key, canonicalize_json(value));
            }
            Value::Object(canonical)
        }
        scalar => scalar,
    }
}

pub fn stable_hash<T: Serialize + ?Sized>(value: &T) -> RuntimeResult<String> {
    let json = stable_json(value)?;
    Ok(sha256_hex(json.as_bytes()))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("0x{digest:x}")
}

pub fn merkle_root<T: Serialize>(items: &[T]) -> RuntimeResult<String> {
    let mut level = items
        .iter()
        .map(stable_hash)
        .collect::<RuntimeResult<Vec<_>>>()?;
    if level.is_empty() {
        return Ok(sha256_hex(b""));
    }
    while level.len() > 1 {
        if level.len() % 2 == 1 {
            let last = level.last().cloned().expect("non-empty level");
            level.push(last);
        }
        level = level
            .chunks_exact(2)
            .map(|pair| sha256_hex(format!("{}{}", pair[0], pair[1]).as_bytes()))
            .collect();
    }
    Ok(level.remove(0))
}

fn normalize_hex_data(value: String) -> String {
    if value.trim().is_empty() {
        "0x".to_string()
    } else {
        value
    }
}

fn invalid<T>(message: impl Into<String>) -> RuntimeResult<T> {
    Err(AgilangError::new(ErrorCode::InvalidArgument, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn consensus_aliases_match_genesis() {
        assert_eq!(ConsensusMode::parse("pos").unwrap(), ConsensusMode::ProofOfStake);
        assert_eq!(
            ConsensusMode::parse("delegated-proof-of-stake").unwrap(),
            ConsensusMode::DelegatedProofOfStake
        );
        assert_eq!(ConsensusMode::parse("developer").unwrap(), ConsensusMode::Development);
    }

    #[test]
    fn canonical_json_sorts_nested_keys_recursively() {
        let value = json!({
            "z": {"b": 2, "a": 1},
            "a": [{"d": 4, "c": 3}]
        });
        assert_eq!(
            stable_json(&value).unwrap(),
            r#"{"a":[{"c":3,"d":4}],"z":{"a":1,"b":2}}"#
        );
    }

    #[test]
    fn transaction_hash_matches_genesis_vector() {
        let transaction = Transaction::transfer("alice", "bob", 25, 0).unwrap();
        assert_eq!(
            transaction.hash,
            "0xbe0840258e1cafa5d4c2f30d5e601effaee66ea017e21469086a595db290931a"
        );
        transaction.validate().unwrap();
    }

    #[test]
    fn transaction_hash_is_deterministic() {
        let left = Transaction::transfer("alice", "bob", 25, 0).unwrap();
        let right = Transaction::transfer("alice", "bob", 25, 0).unwrap();
        assert_eq!(left.hash, right.hash);
        left.validate().unwrap();
    }

    #[test]
    fn empty_merkle_root_matches_genesis_algorithm() {
        assert_eq!(
            merkle_root::<Transaction>(&[]).unwrap(),
            sha256_hex(b"")
        );
    }

    #[test]
    fn odd_merkle_level_duplicates_last_leaf() {
        let transactions = vec![
            Transaction::transfer("alice", "bob", 1, 0).unwrap(),
            Transaction::transfer("alice", "carol", 2, 1).unwrap(),
            Transaction::transfer("alice", "dave", 3, 2).unwrap(),
        ];
        let first = merkle_root(&transactions).unwrap();
        let second = merkle_root(&transactions).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn creates_and_validates_child_block() {
        let config = BlockchainConfig::default();
        let genesis = Block::genesis(&config, 1_700_000_000_000).unwrap();
        genesis.validate(&config, None).unwrap();
        let child = Block::child(
            &config,
            &genesis,
            "validator-1",
            1_700_000_006_000,
            1,
            vec![Transaction::transfer("alice", "bob", 10, 0).unwrap()],
            stable_hash(&BTreeMap::<String, Value>::new()).unwrap(),
            100,
            "",
        )
        .unwrap();
        child.validate(&config, Some(&genesis)).unwrap();
    }

    #[test]
    fn hardened_profile_enforces_signatures_and_finality() {
        let config = BlockchainConfig::default().hardened_mainnet();
        assert!(config.require_block_signatures);
        assert!(config.strict_accounting);
        assert!(config.finality_depth >= 32);
        assert!(config.mempool_min_gas_price >= 1);
    }
}
