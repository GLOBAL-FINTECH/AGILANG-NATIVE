//! Production-oriented block orchestration for the Native AGILANG blockchain.
//!
//! This crate gives locally produced and remotely imported blocks one execution
//! path. Consensus-visible state roots, gas usage and fee totals are recomputed
//! rather than trusted from peers.

use agilang_blockchain_core::{merkle_root, Block, BlockHeader, Transaction};
use agilang_blockchain_execution::{execute_block, BlockExecution};
use agilang_blockchain_fee::{next_base_fee, DEFAULT_INITIAL_BASE_FEE};
use agilang_blockchain_node::{choose_best_head, select_proposer, BlockchainNode};
use agilang_blockchain_state::intrinsic_gas;
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};

const BASE_FEE_KEY_PREFIX: &str = "base_fee:";
const EXECUTION_KEY_PREFIX: &str = "execution:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutedBlock {
    pub block: Block,
    pub execution: BlockExecution,
}

pub struct ProductionRuntime {
    pub node: BlockchainNode,
}

impl ProductionRuntime {
    pub fn new(mut node: BlockchainNode) -> RuntimeResult<Self> {
        let genesis = node.canonical_head()?;
        let key = base_fee_key(&genesis.hash);
        if node.database.state::<u128>(&key)?.is_none() {
            node.database.put_state(&key, &DEFAULT_INITIAL_BASE_FEE)?;
        }
        Ok(Self { node })
    }

    pub fn produce_block(
        &mut self,
        proposer: &str,
        timestamp_ms: u64,
        slot: u64,
        validator_signature: &str,
    ) -> RuntimeResult<ExecutedBlock> {
        let parent = self.node.canonical_head()?;
        let expected = select_proposer(&self.node.config, slot)?;
        if !self.node.config.dev_allow_any_proposer && proposer != expected {
            return invalid(format!(
                "unexpected proposer {proposer}; expected {expected} for slot {slot}"
            ));
        }
        let transactions = self.node.mempool.select(&self.node.config, &self.node.state);
        if transactions.is_empty() && !self.node.config.allow_empty_blocks {
            return invalid("empty block production is disabled");
        }
        let base_fee = self.child_base_fee(&parent)?;
        let mut candidate = self.node.state.clone();
        let execution = execute_block(
            &mut candidate,
            &self.node.config,
            &transactions,
            base_fee,
            proposer,
        )?;
        let block = build_executed_block(
            &self.node,
            &parent,
            transactions,
            &candidate,
            proposer,
            timestamp_ms,
            slot,
            validator_signature,
            &execution,
        )?;
        self.persist_executed_block(&block, &candidate, &execution)?;
        self.node.state = candidate;
        self.node
            .mempool
            .remove_many(block.transactions.iter().map(|tx| tx.hash.as_str()));
        Ok(ExecutedBlock { block, execution })
    }

    /// Validate, re-execute and import a peer block.
    ///
    /// The peer-provided state root and gas usage are accepted only when they
    /// equal locally recomputed values.
    pub fn import_block(&mut self, block: Block) -> RuntimeResult<ExecutedBlock> {
        if self.node.database.has_block(&block.hash)? {
            return invalid("block is already known");
        }
        let parent = self
            .node
            .database
            .block(&block.header.parent_hash)?
            .map(|stored| stored.block)
            .ok_or_else(|| invalid_error("block parent is unknown"))?;
        block.validate(&self.node.config, Some(&parent))?;
        let expected = select_proposer(&self.node.config, block.header.slot)?;
        if !self.node.config.dev_allow_any_proposer && block.header.proposer != expected {
            return invalid("imported block proposer is not eligible for its slot");
        }

        self.protect_finalized_ancestry(&block)?;
        let base_fee = self.child_base_fee(&parent)?;
        let mut candidate = self.node.state.clone();
        let execution = execute_block(
            &mut candidate,
            &self.node.config,
            &block.transactions,
            base_fee,
            &block.header.proposer,
        )?;
        let expected_state_root = candidate.state_root()?;
        if block.header.state_root != expected_state_root {
            return invalid("imported block state root mismatch");
        }
        let gas_used = execution_gas_used(&execution)?;
        if block.header.gas_used != gas_used {
            return invalid("imported block gas usage mismatch");
        }
        self.persist_executed_block(&block, &candidate, &execution)?;
        self.node.state = candidate;
        self.node
            .mempool
            .remove_many(block.transactions.iter().map(|tx| tx.hash.as_str()));
        Ok(ExecutedBlock { block, execution })
    }

    pub fn base_fee(&self) -> RuntimeResult<u128> {
        let head = self.node.canonical_head()?;
        self.node
            .database
            .state(&base_fee_key(&head.hash))?
            .ok_or_else(|| invalid_error("canonical head base fee is missing"))
    }

    fn child_base_fee(&self, parent: &Block) -> RuntimeResult<u128> {
        let parent_base_fee = self
            .node
            .database
            .state::<u128>(&base_fee_key(&parent.hash))?
            .unwrap_or(DEFAULT_INITIAL_BASE_FEE);
        next_base_fee(
            parent_base_fee,
            parent.header.gas_used,
            parent.header.gas_limit,
        )
    }

    fn protect_finalized_ancestry(&self, candidate: &Block) -> RuntimeResult<()> {
        let mut current = self.node.database.canonical_head()?;
        while let Some(stored) = current {
            if stored.finalized {
                let candidate_path = self.node.database.ancestor_path(&candidate.header.parent_hash)?;
                if !candidate_path.iter().any(|block| block.hash == stored.block.hash) {
                    return invalid("candidate chain conflicts with finalized checkpoint");
                }
                break;
            }
            if stored.block.header.height == 0 {
                break;
            }
            current = self.node.database.block(&stored.block.header.parent_hash)?;
        }
        Ok(())
    }

    fn persist_executed_block(
        &mut self,
        block: &Block,
        state: &agilang_blockchain_state::ChainState,
        execution: &BlockExecution,
    ) -> RuntimeResult<()> {
        // These calls intentionally remain grouped here. The storage crate must
        // expose a single SQL transaction bundle before this method is declared
        // crash-atomic across every record.
        self.node.database.put_block(block, false, false)?;
        self.node
            .database
            .put_state("chain_state", state)?;
        self.node
            .database
            .put_state(&execution_key(&block.hash), execution)?;
        self.node
            .database
            .put_state(&base_fee_key(&block.hash), &execution.base_fee_per_gas)?;
        let best = choose_best_head(self.node.database.candidate_heads()?)?;
        self.node
            .database
            .set_canonical_chain(&best.hash, self.node.config.finality_depth)?;
        Ok(())
    }
}

fn build_executed_block(
    node: &BlockchainNode,
    parent: &Block,
    transactions: Vec<Transaction>,
    state: &agilang_blockchain_state::ChainState,
    proposer: &str,
    timestamp_ms: u64,
    slot: u64,
    validator_signature: &str,
    execution: &BlockExecution,
) -> RuntimeResult<Block> {
    let gas_used = execution_gas_used(execution)?;
    if gas_used > node.config.block_gas_limit {
        return invalid("executed block exceeds gas limit");
    }
    let score = parent
        .header
        .score
        .checked_add(u128::from(
            node.config.validators.get(proposer).copied().unwrap_or(1),
        ))
        .ok_or_else(|| invalid_error("block score overflow"))?;
    let header = BlockHeader {
        chain_id: node.config.chain_id,
        height: parent.header.height + 1,
        parent_hash: parent.hash.clone(),
        timestamp_ms,
        slot,
        epoch: slot / node.config.epoch_length,
        proposer: proposer.to_string(),
        transaction_root: merkle_root(&transactions)?,
        state_root: state.state_root()?,
        gas_limit: node.config.block_gas_limit,
        gas_used,
        score,
    };
    let block = Block::new(header, transactions, validator_signature.to_string())?;
    block.validate(&node.config, Some(parent))?;
    Ok(block)
}

fn execution_gas_used(execution: &BlockExecution) -> RuntimeResult<u64> {
    execution.receipts.iter().try_fold(0_u64, |total, receipt| {
        total
            .checked_add(receipt.receipt.gas_used)
            .ok_or_else(|| invalid_error("block gas-used overflow"))
    })
}

pub fn transaction_intrinsic_gas(transactions: &[Transaction]) -> RuntimeResult<u64> {
    transactions.iter().try_fold(0_u64, |total, transaction| {
        total
            .checked_add(intrinsic_gas(transaction)?)
            .ok_or_else(|| invalid_error("transaction intrinsic-gas overflow"))
    })
}

fn base_fee_key(hash: &str) -> String {
    format!("{BASE_FEE_KEY_PREFIX}{hash}")
}

fn execution_key(hash: &str) -> String {
    format!("{EXECUTION_KEY_PREFIX}{hash}")
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
    use agilang_blockchain_core::{BlockchainConfig, Transaction};
    use agilang_blockchain_state::Account;
    use std::collections::BTreeMap;

    fn runtime() -> ProductionRuntime {
        let mut config = BlockchainConfig::default();
        config.strict_accounting = true;
        config.genesis_state.insert(
            "alice".into(),
            serde_json::to_value(Account {
                balance: 100_000_000_000_000,
                ..Account::default()
            })
            .unwrap(),
        );
        ProductionRuntime::new(BlockchainNode::memory(config, 1_700_000_000_000).unwrap())
            .unwrap()
    }

    #[test]
    fn produces_fee_aware_block_with_actual_gas() {
        let mut runtime = runtime();
        let transaction = Transaction::new(
            "alice",
            "bob",
            100,
            "0x",
            0,
            21_000,
            DEFAULT_INITIAL_BASE_FEE + 10,
            "transfer",
            None,
            BTreeMap::new(),
        )
        .unwrap();
        runtime.node.submit_transaction(transaction, 1).unwrap();
        let produced = runtime
            .produce_block("validator-1", 1_700_000_006_000, 1, "")
            .unwrap();
        assert_eq!(produced.block.header.gas_used, 21_000);
        assert_eq!(produced.execution.receipts.len(), 1);
        assert_eq!(runtime.node.state.balance("bob"), 100);
    }

    #[test]
    fn duplicate_import_is_rejected() {
        let mut runtime = runtime();
        let produced = runtime
            .produce_block("validator-1", 1_700_000_006_000, 1, "")
            .unwrap();
        assert!(runtime.import_block(produced.block).is_err());
    }
}
