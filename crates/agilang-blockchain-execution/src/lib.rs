//! Fee-aware atomic block execution for Native AGILANG.

use agilang_blockchain_core::{BlockchainConfig, Transaction};
use agilang_blockchain_fee::{settle_eip1559, settle_legacy, FeeSettlement};
use agilang_blockchain_state::{intrinsic_gas, ChainState, Receipt};
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub receipt: Receipt,
    pub settlement: FeeSettlement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockExecution {
    pub receipts: Vec<ExecutionReceipt>,
    pub base_fee_per_gas: u128,
    pub burned_fee: u128,
    pub proposer_tip: u128,
    pub total_fees: u128,
}

/// Execute a complete block atomically.
///
/// The underlying state engine initially reserves `transaction.gas_price`.
/// Dynamic-fee transactions use `maxFeePerGas` as that reservation, after
/// which this layer refunds the difference between the reservation and the
/// effective EIP-1559 charge. Base fees remain uncredited (burned), while the
/// priority portion is credited to the block proposer.
pub fn execute_block(
    state: &mut ChainState,
    config: &BlockchainConfig,
    transactions: &[Transaction],
    base_fee_per_gas: u128,
    proposer: &str,
) -> RuntimeResult<BlockExecution> {
    if proposer.trim().is_empty() {
        return invalid("block proposer is required");
    }

    let mut candidate = state.clone();
    let mut receipts = Vec::with_capacity(transactions.len());
    let mut burned_fee = 0_u128;
    let mut proposer_tip = 0_u128;
    let mut total_fees = 0_u128;

    for transaction in transactions {
        let gas_used = intrinsic_gas(transaction)?;
        let settlement = settlement_for(transaction, gas_used, base_fee_per_gas)?;
        let reserved_fee = u128::from(gas_used)
            .checked_mul(transaction.gas_price)
            .ok_or_else(|| invalid_error("reserved transaction fee overflow"))?;
        if settlement.total_fee > reserved_fee {
            return invalid("effective transaction fee exceeds reserved fee");
        }

        let mut receipt = candidate.apply(config, transaction)?;
        let refund = reserved_fee - settlement.total_fee;
        if refund > 0 {
            let mut sender = candidate.account(&transaction.sender);
            sender.balance = sender
                .balance
                .checked_add(refund)
                .ok_or_else(|| invalid_error("sender fee refund overflow"))?;
            candidate.set_account(transaction.sender.clone(), sender);
        }

        if settlement.validator_tip > 0 {
            let mut proposer_account = candidate.account(proposer);
            proposer_account.balance = proposer_account
                .balance
                .checked_add(settlement.validator_tip)
                .ok_or_else(|| invalid_error("proposer tip overflow"))?;
            candidate.set_account(proposer.to_string(), proposer_account);
        }

        receipt.fee_charged = settlement.total_fee;
        receipt.logs.push(serde_json::json!({
            "type": "fee_settlement",
            "base_fee_per_gas": settlement.base_fee_per_gas.to_string(),
            "priority_fee_per_gas": settlement.priority_fee_per_gas.to_string(),
            "effective_gas_price": settlement.effective_gas_price.to_string(),
            "burned_fee": settlement.burned_fee.to_string(),
            "validator_tip": settlement.validator_tip.to_string()
        }));

        burned_fee = burned_fee
            .checked_add(settlement.burned_fee)
            .ok_or_else(|| invalid_error("block burned-fee overflow"))?;
        proposer_tip = proposer_tip
            .checked_add(settlement.validator_tip)
            .ok_or_else(|| invalid_error("block proposer-tip overflow"))?;
        total_fees = total_fees
            .checked_add(settlement.total_fee)
            .ok_or_else(|| invalid_error("block total-fee overflow"))?;
        receipts.push(ExecutionReceipt { receipt, settlement });
    }

    *state = candidate;
    Ok(BlockExecution {
        receipts,
        base_fee_per_gas,
        burned_fee,
        proposer_tip,
        total_fees,
    })
}

fn settlement_for(
    transaction: &Transaction,
    gas_used: u64,
    base_fee_per_gas: u128,
) -> RuntimeResult<FeeSettlement> {
    match transaction
        .metadata
        .get("envelope")
        .and_then(Value::as_str)
    {
        Some("eip1559") => {
            let max_fee = metadata_u128(transaction, "max_fee_per_gas")?;
            let priority = metadata_u128(transaction, "max_priority_fee_per_gas")?;
            settle_eip1559(gas_used, max_fee, priority, base_fee_per_gas)
        }
        _ => settle_legacy(gas_used, transaction.gas_price, base_fee_per_gas),
    }
}

fn metadata_u128(transaction: &Transaction, key: &str) -> RuntimeResult<u128> {
    let value = transaction
        .metadata
        .get(key)
        .ok_or_else(|| invalid_error(format!("missing transaction metadata: {key}")))?;
    match value {
        Value::String(value) => value
            .parse::<u128>()
            .map_err(|_| invalid_error(format!("invalid transaction metadata: {key}"))),
        Value::Number(value) => value
            .as_u64()
            .map(u128::from)
            .ok_or_else(|| invalid_error(format!("invalid transaction metadata: {key}"))),
        _ => invalid(format!("invalid transaction metadata: {key}")),
    }
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
    use serde_json::json;
    use std::collections::BTreeMap;

    fn funded_state() -> (BlockchainConfig, ChainState) {
        let mut config = BlockchainConfig::default();
        config.strict_accounting = true;
        let mut state = ChainState::default();
        state.set_account(
            "alice",
            Account {
                balance: 10_000_000,
                ..Account::default()
            },
        );
        (config, state)
    }

    #[test]
    fn credits_tip_and_burns_base_fee() {
        let (config, mut state) = funded_state();
        let tx = Transaction::new(
            "alice",
            "bob",
            100,
            "0x",
            0,
            21_000,
            3,
            "transfer",
            None,
            BTreeMap::new(),
        )
        .unwrap();
        let result = execute_block(&mut state, &config, &[tx], 2, "validator-1").unwrap();
        assert_eq!(result.burned_fee, 42_000);
        assert_eq!(result.proposer_tip, 21_000);
        assert_eq!(state.balance("validator-1"), 21_000);
        assert_eq!(state.balance("bob"), 100);
    }

    #[test]
    fn refunds_unused_dynamic_fee_cap() {
        let (config, mut state) = funded_state();
        let mut metadata = BTreeMap::new();
        metadata.insert("envelope".into(), json!("eip1559"));
        metadata.insert("max_fee_per_gas".into(), json!("10"));
        metadata.insert("max_priority_fee_per_gas".into(), json!("2"));
        let tx = Transaction::new(
            "alice",
            "bob",
            0,
            "0x",
            0,
            21_000,
            10,
            "evm_call",
            None,
            metadata,
        )
        .unwrap();
        let before = state.balance("alice");
        let result = execute_block(&mut state, &config, &[tx], 5, "validator-1").unwrap();
        assert_eq!(result.receipts[0].settlement.effective_gas_price, 7);
        assert_eq!(before - state.balance("alice"), 147_000);
        assert_eq!(state.balance("validator-1"), 42_000);
    }

    #[test]
    fn block_failure_rolls_back_all_transactions() {
        let (config, mut state) = funded_state();
        let good = Transaction::new(
            "alice",
            "bob",
            1,
            "0x",
            0,
            21_000,
            2,
            "transfer",
            None,
            BTreeMap::new(),
        )
        .unwrap();
        let bad = Transaction::new(
            "alice",
            "bob",
            20_000_000,
            "0x",
            1,
            21_000,
            2,
            "transfer",
            None,
            BTreeMap::new(),
        )
        .unwrap();
        let before = state.clone();
        assert!(execute_block(&mut state, &config, &[good, bad], 1, "validator-1").is_err());
        assert_eq!(state, before);
    }
}
