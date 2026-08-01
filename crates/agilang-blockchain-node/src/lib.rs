//! Native AGILANG blockchain node orchestration.
//!
//! This crate connects the consensus-visible core types, persistent chain
//! database and deterministic account state into a bounded mempool, block
//! producer, fork-choice engine and JSON-RPC dispatch surface.

use agilang_blockchain_core::{stable_hash, Block, BlockchainConfig, ConsensusMode, Transaction};
use agilang_blockchain_state::{ChainState, Receipt};
use agilang_blockchain_storage::ChainDatabase;
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MempoolEntry {
    pub transaction: Transaction,
    pub received_at_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Mempool {
    by_hash: BTreeMap<String, MempoolEntry>,
    by_sender_nonce: BTreeMap<(String, u64), String>,
}

impl Mempool {
    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    pub fn contains(&self, hash: &str) -> bool {
        self.by_hash.contains_key(hash)
    }

    pub fn transaction(&self, hash: &str) -> Option<&Transaction> {
        self.by_hash.get(hash).map(|entry| &entry.transaction)
    }

    pub fn add(
        &mut self,
        config: &BlockchainConfig,
        state: &ChainState,
        transaction: Transaction,
        received_at_ms: u64,
    ) -> RuntimeResult<Option<Transaction>> {
        transaction.validate()?;
        if self.by_hash.contains_key(&transaction.hash) {
            return invalid("duplicate transaction hash");
        }
        if self.by_hash.len() >= config.mempool_max_txs {
            return invalid("mempool capacity reached");
        }
        if transaction.gas_price < config.mempool_min_gas_price {
            return invalid("transaction gas price is below chain minimum");
        }
        let confirmed_nonce = state.nonce(&transaction.sender);
        if config.enforce_nonce_order && transaction.nonce < confirmed_nonce {
            return invalid(format!(
                "transaction nonce {} is below confirmed nonce {confirmed_nonce}",
                transaction.nonce
            ));
        }

        let key = (transaction.sender.clone(), transaction.nonce);
        let replaced = if let Some(existing_hash) = self.by_sender_nonce.get(&key).cloned() {
            let existing = self
                .by_hash
                .get(&existing_hash)
                .ok_or_else(|| invalid_error("mempool sender index is inconsistent"))?;
            if transaction.gas_price <= existing.transaction.gas_price {
                return invalid("replacement transaction gas price must be higher");
            }
            self.by_hash.remove(&existing_hash).map(|entry| entry.transaction)
        } else {
            None
        };

        self.by_sender_nonce.insert(key, transaction.hash.clone());
        self.by_hash.insert(
            transaction.hash.clone(),
            MempoolEntry {
                transaction,
                received_at_ms,
            },
        );
        Ok(replaced)
    }

    pub fn remove(&mut self, hash: &str) -> Option<Transaction> {
        let entry = self.by_hash.remove(hash)?;
        self.by_sender_nonce
            .remove(&(entry.transaction.sender.clone(), entry.transaction.nonce));
        Some(entry.transaction)
    }

    pub fn remove_many<'a>(&mut self, hashes: impl IntoIterator<Item = &'a str>) {
        for hash in hashes {
            self.remove(hash);
        }
    }

    pub fn pending_nonce(&self, state: &ChainState, sender: &str) -> u64 {
        let mut next = state.nonce(sender);
        while self
            .by_sender_nonce
            .contains_key(&(sender.to_string(), next))
        {
            next = next.saturating_add(1);
        }
        next
    }

    pub fn select(
        &self,
        config: &BlockchainConfig,
        state: &ChainState,
    ) -> Vec<Transaction> {
        let mut candidates: Vec<&MempoolEntry> = self.by_hash.values().collect();
        candidates.sort_by(|left, right| {
            right
                .transaction
                .gas_price
                .cmp(&left.transaction.gas_price)
                .then_with(|| left.received_at_ms.cmp(&right.received_at_ms))
                .then_with(|| left.transaction.hash.cmp(&right.transaction.hash))
        });

        let mut expected_nonce: BTreeMap<String, u64> = BTreeMap::new();
        let mut gas_used = 0_u64;
        let mut selected = Vec::new();
        let mut made_progress = true;
        let mut consumed = BTreeSet::new();

        while made_progress && selected.len() < config.max_block_txs {
            made_progress = false;
            for entry in &candidates {
                let tx = &entry.transaction;
                if consumed.contains(&tx.hash) {
                    continue;
                }
                let expected = *expected_nonce
                    .entry(tx.sender.clone())
                    .or_insert_with(|| state.nonce(&tx.sender));
                if config.enforce_nonce_order && tx.nonce != expected {
                    continue;
                }
                let Some(next_gas) = gas_used.checked_add(tx.gas_limit) else {
                    continue;
                };
                if next_gas > config.block_gas_limit {
                    continue;
                }
                selected.push(tx.clone());
                consumed.insert(tx.hash.clone());
                gas_used = next_gas;
                expected_nonce.insert(tx.sender.clone(), expected.saturating_add(1));
                made_progress = true;
                if selected.len() >= config.max_block_txs {
                    break;
                }
            }
        }
        selected
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducedBlock {
    pub block: Block,
    pub receipts: Vec<Receipt>,
}

pub struct BlockchainNode {
    pub config: BlockchainConfig,
    pub database: ChainDatabase,
    pub state: ChainState,
    pub mempool: Mempool,
}

impl BlockchainNode {
    pub fn memory(config: BlockchainConfig, genesis_timestamp_ms: u64) -> RuntimeResult<Self> {
        config.validate()?;
        let mut database = ChainDatabase::memory()?;
        let state = ChainState::from_genesis(&config.genesis_state)?;
        let genesis = Block::genesis(&config, genesis_timestamp_ms)?;
        database.put_block(&genesis, true, true)?;
        database.put_state("chain_state", &state)?;
        database.put_metadata("chain_config", &config)?;
        database.put_metadata("genesis_hash", &genesis.hash)?;
        Ok(Self {
            config,
            database,
            state,
            mempool: Mempool::default(),
        })
    }

    pub fn submit_transaction(
        &mut self,
        transaction: Transaction,
        received_at_ms: u64,
    ) -> RuntimeResult<Option<Transaction>> {
        self.mempool
            .add(&self.config, &self.state, transaction, received_at_ms)
    }

    pub fn canonical_head(&self) -> RuntimeResult<Block> {
        self.database
            .canonical_head()?
            .map(|stored| stored.block)
            .ok_or_else(|| invalid_error("canonical chain has no head"))
    }

    pub fn produce_block(
        &mut self,
        proposer: &str,
        timestamp_ms: u64,
        slot: u64,
        validator_signature: &str,
    ) -> RuntimeResult<ProducedBlock> {
        let parent = self.canonical_head()?;
        let expected_proposer = select_proposer(&self.config, slot)?;
        if self.config.consensus_mode != ConsensusMode::Development
            && !self.config.dev_allow_any_proposer
            && proposer != expected_proposer
        {
            return invalid(format!(
                "unexpected proposer {proposer}; expected {expected_proposer} for slot {slot}"
            ));
        }
        let transactions = self.mempool.select(&self.config, &self.state);
        if transactions.is_empty() && !self.config.allow_empty_blocks {
            return invalid("empty block production is disabled");
        }

        let mut candidate_state = self.state.clone();
        let receipts = candidate_state.apply_batch(&self.config, &transactions)?;
        let state_root = candidate_state.state_root()?;
        let proposer_stake = self.config.validators.get(proposer).copied().unwrap_or(1);
        let score = parent
            .header
            .score
            .checked_add(u128::from(proposer_stake))
            .ok_or_else(|| invalid_error("block score overflow"))?;
        let block = Block::child(
            &self.config,
            &parent,
            proposer,
            timestamp_ms,
            slot,
            transactions,
            state_root,
            score,
            validator_signature,
        )?;
        block.validate(&self.config, Some(&parent))?;

        self.database.put_block(&block, false, false)?;
        let best = choose_best_head(self.database.candidate_heads()?)?;
        self.database
            .set_canonical_chain(&best.hash, self.config.finality_depth)?;
        self.database.put_state("chain_state", &candidate_state)?;
        self.database.put_state(
            &format!("receipts:{}", block.hash),
            &receipts,
        )?;
        self.state = candidate_state;
        self.mempool
            .remove_many(block.transactions.iter().map(|tx| tx.hash.as_str()));
        Ok(ProducedBlock { block, receipts })
    }

    pub fn rpc(&self, method: &str, params: Value) -> RuntimeResult<Value> {
        match method {
            "web3_clientVersion" => Ok(json!("AGILANG-NATIVE/0.7.0")),
            "net_version" => Ok(json!(self.config.chain_id.to_string())),
            "eth_chainId" => Ok(json!(quantity(self.config.chain_id))),
            "eth_blockNumber" => Ok(json!(quantity(self.canonical_head()?.header.height))),
            "eth_getBalance" => {
                let address = param_string(&params, 0)?;
                Ok(json!(quantity_u128(self.state.balance(address))))
            }
            "eth_getTransactionCount" => {
                let address = param_string(&params, 0)?;
                Ok(json!(quantity(self.state.nonce(address))))
            }
            "eth_getBlockByHash" => {
                let hash = param_string(&params, 0)?;
                Ok(self
                    .database
                    .block(hash)?
                    .map(|stored| block_json(&stored.block, stored.canonical, stored.finalized))
                    .unwrap_or(Value::Null))
            }
            "eth_getBlockByNumber" => {
                let number = param_string(&params, 0)?;
                let height = if number == "latest" {
                    self.canonical_head()?.header.height
                } else {
                    parse_quantity(number)?
                };
                Ok(self
                    .database
                    .block_by_height(height, true)?
                    .map(|stored| block_json(&stored.block, stored.canonical, stored.finalized))
                    .unwrap_or(Value::Null))
            }
            "eth_getTransactionByHash" => {
                let hash = param_string(&params, 0)?;
                if let Some(stored) = self.database.transaction(hash)? {
                    Ok(transaction_json(&stored.transaction, stored.block_hash.as_deref()))
                } else if let Some(transaction) = self.mempool.transaction(hash) {
                    Ok(transaction_json(transaction, None))
                } else {
                    Ok(Value::Null)
                }
            }
            "eth_getTransactionReceipt" => {
                let hash = param_string(&params, 0)?;
                let Some(stored) = self.database.transaction(hash)? else {
                    return Ok(Value::Null);
                };
                let Some(block_hash) = stored.block_hash else {
                    return Ok(Value::Null);
                };
                let receipts: Option<Vec<Receipt>> = self
                    .database
                    .state(&format!("receipts:{block_hash}"))?;
                let receipt = receipts
                    .unwrap_or_default()
                    .into_iter()
                    .find(|receipt| receipt.transaction_hash == hash);
                Ok(receipt
                    .map(|receipt| receipt_json(&receipt, &block_hash))
                    .unwrap_or(Value::Null))
            }
            _ => Err(AgilangError::new(
                ErrorCode::InvalidArgument,
                format!("unsupported JSON-RPC method: {method}"),
            )),
        }
    }
}

pub fn select_proposer(config: &BlockchainConfig, slot: u64) -> RuntimeResult<String> {
    if config.consensus_mode == ConsensusMode::Development && config.dev_allow_any_proposer {
        return Ok(config
            .validators
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "developer".to_string()));
    }
    let validators: Vec<(&String, &u64)> = match config.consensus_mode {
        ConsensusMode::DelegatedProofOfStake if !config.delegates.is_empty() => config
            .delegates
            .iter()
            .filter_map(|name| config.validators.get_key_value(name))
            .collect(),
        _ => config.validators.iter().collect(),
    };
    if validators.is_empty() {
        return invalid("no eligible validators are configured");
    }
    let total = validators.iter().try_fold(0_u64, |sum, (_, stake)| {
        sum.checked_add(**stake)
            .ok_or_else(|| invalid_error("validator stake overflow"))
    })?;
    if total == 0 {
        return invalid("eligible validator stake is zero");
    }
    let mut position = slot % total;
    for (validator, stake) in validators {
        if position < *stake {
            return Ok(validator.clone());
        }
        position -= *stake;
    }
    invalid("failed to select validator")
}

pub fn choose_best_head(mut candidates: Vec<Block>) -> RuntimeResult<Block> {
    candidates.sort_by(|left, right| {
        right
            .header
            .score
            .cmp(&left.header.score)
            .then_with(|| right.header.height.cmp(&left.header.height))
            .then_with(|| left.hash.cmp(&right.hash))
    });
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| invalid_error("no candidate chain heads are available"))
}

pub fn quantity(value: u64) -> String {
    format!("0x{value:x}")
}

pub fn quantity_u128(value: u128) -> String {
    format!("0x{value:x}")
}

pub fn parse_quantity(value: &str) -> RuntimeResult<u64> {
    let raw = value
        .strip_prefix("0x")
        .ok_or_else(|| invalid_error("quantity must start with 0x"))?;
    u64::from_str_radix(raw, 16).map_err(|error| invalid_error(error.to_string()))
}

fn param_string<'a>(params: &'a Value, index: usize) -> RuntimeResult<&'a str> {
    params
        .as_array()
        .and_then(|values| values.get(index))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_error(format!("JSON-RPC parameter {index} must be a string")))
}

fn block_json(block: &Block, canonical: bool, finalized: bool) -> Value {
    json!({
        "number": quantity(block.header.height),
        "hash": block.hash,
        "parentHash": block.header.parent_hash,
        "timestamp": quantity(block.header.timestamp_ms / 1000),
        "gasLimit": quantity(block.header.gas_limit),
        "gasUsed": quantity(block.header.gas_used),
        "miner": block.header.proposer,
        "transactionsRoot": block.header.transaction_root,
        "stateRoot": block.header.state_root,
        "transactions": block.transactions.iter().map(|tx| tx.hash.clone()).collect::<Vec<_>>(),
        "canonical": canonical,
        "finalized": finalized,
        "slot": quantity(block.header.slot),
        "epoch": quantity(block.header.epoch),
    })
}

fn transaction_json(transaction: &Transaction, block_hash: Option<&str>) -> Value {
    json!({
        "hash": transaction.hash,
        "from": transaction.sender,
        "to": if transaction.to.is_empty() { Value::Null } else { json!(transaction.to) },
        "value": quantity_u128(transaction.value),
        "nonce": quantity(transaction.nonce),
        "gas": quantity(transaction.gas_limit),
        "gasPrice": quantity_u128(transaction.gas_price),
        "input": transaction.data,
        "blockHash": block_hash,
    })
}

fn receipt_json(receipt: &Receipt, block_hash: &str) -> Value {
    json!({
        "transactionHash": receipt.transaction_hash,
        "blockHash": block_hash,
        "status": if receipt.success { "0x1" } else { "0x0" },
        "gasUsed": quantity(receipt.gas_used),
        "effectiveGasPrice": quantity_u128(receipt.fee_charged / u128::from(receipt.gas_used.max(1))),
        "from": receipt.sender,
        "to": receipt.recipient,
        "logs": receipt.logs,
    })
}

pub fn compatibility_fingerprint(
    config: &BlockchainConfig,
    state: &ChainState,
    head: &Block,
) -> RuntimeResult<String> {
    stable_hash(&json!({
        "chain_id": config.chain_id,
        "config": config,
        "state_root": state.state_root()?,
        "head": head,
    }))
}

fn invalid<T>(message: impl Into<String>) -> RuntimeResult<T> {
    Err(invalid_error(message))
}

fn invalid_error(message: impl Into<String>) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidArgument, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_blockchain_state::Account;

    fn node() -> BlockchainNode {
        let mut config = BlockchainConfig::default();
        config.strict_accounting = true;
        config.genesis_state.insert(
            "alice".into(),
            serde_json::to_value(Account {
                balance: 10_000_000,
                ..Account::default()
            })
            .unwrap(),
        );
        BlockchainNode::memory(config, 1_700_000_000_000).unwrap()
    }

    #[test]
    fn mempool_replaces_same_sender_nonce_with_higher_price() {
        let mut node = node();
        let first = Transaction::new(
            "alice", "bob", 1, "0x", 0, 21_000, 1, "transfer", None, BTreeMap::new(),
        )
        .unwrap();
        let second = Transaction::new(
            "alice", "bob", 1, "0x", 0, 21_000, 2, "transfer", None, BTreeMap::new(),
        )
        .unwrap();
        node.submit_transaction(first.clone(), 1).unwrap();
        let replaced = node.submit_transaction(second.clone(), 2).unwrap().unwrap();
        assert_eq!(replaced.hash, first.hash);
        assert!(node.mempool.contains(&second.hash));
    }

    #[test]
    fn produces_persists_and_removes_transactions() {
        let mut node = node();
        let transaction = Transaction::new(
            "alice", "bob", 100, "0x", 0, 21_000, 1, "transfer", None, BTreeMap::new(),
        )
        .unwrap();
        let hash = transaction.hash.clone();
        node.submit_transaction(transaction, 1).unwrap();
        let produced = node
            .produce_block("validator-1", 1_700_000_006_000, 0, "")
            .unwrap();
        assert_eq!(produced.block.transactions.len(), 1);
        assert!(!node.mempool.contains(&hash));
        assert_eq!(node.state.balance("bob"), 100);
        assert!(node.database.transaction(&hash).unwrap().is_some());
    }

    #[test]
    fn exposes_basic_json_rpc() {
        let node = node();
        assert_eq!(node.rpc("eth_chainId", json!([])).unwrap(), json!("0x1e61"));
        assert_eq!(node.rpc("eth_blockNumber", json!([])).unwrap(), json!("0x0"));
        assert_eq!(
            node.rpc("eth_getTransactionCount", json!(["alice", "latest"]))
                .unwrap(),
            json!("0x0")
        );
    }

    #[test]
    fn stake_weighted_selection_is_deterministic() {
        let mut config = BlockchainConfig::default();
        config.validators.insert("validator-2".into(), 50);
        assert_eq!(select_proposer(&config, 0).unwrap(), "validator-1");
        assert_eq!(select_proposer(&config, 100).unwrap(), "validator-2");
    }
}
