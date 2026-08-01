//! Deterministic account state and transaction application for Native AGILANG.

use agilang_blockchain_core::{stable_hash, BlockchainConfig, Transaction};
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub balance: u128,
    pub nonce: u64,
    pub code: String,
    pub storage: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainState {
    pub accounts: BTreeMap<String, Account>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub transaction_hash: String,
    pub success: bool,
    pub gas_used: u64,
    pub fee_charged: u128,
    pub sender: String,
    pub recipient: String,
    pub sender_nonce: u64,
    pub error: Option<String>,
    pub logs: Vec<Value>,
}

impl ChainState {
    pub fn from_genesis(genesis: &BTreeMap<String, Value>) -> RuntimeResult<Self> {
        let mut state = Self::default();
        for (address, value) in genesis {
            let account = match value {
                Value::Number(number) => Account {
                    balance: number
                        .as_u64()
                        .map(u128::from)
                        .ok_or_else(|| invalid_error("genesis balance must be a non-negative integer"))?,
                    ..Account::default()
                },
                Value::Object(_) => serde_json::from_value(value.clone())
                    .map_err(|error| invalid_error(error.to_string()))?,
                _ => return invalid("genesis account must be a balance or account object"),
            };
            state.accounts.insert(address.clone(), account);
        }
        Ok(state)
    }

    pub fn account(&self, address: &str) -> Account {
        self.accounts.get(address).cloned().unwrap_or_default()
    }

    pub fn balance(&self, address: &str) -> u128 {
        self.accounts.get(address).map(|account| account.balance).unwrap_or(0)
    }

    pub fn nonce(&self, address: &str) -> u64 {
        self.accounts.get(address).map(|account| account.nonce).unwrap_or(0)
    }

    pub fn set_account(&mut self, address: impl Into<String>, account: Account) {
        self.accounts.insert(address.into(), account);
    }

    pub fn state_root(&self) -> RuntimeResult<String> {
        stable_hash(&self.accounts)
    }

    /// Apply a transaction atomically. Any failure leaves the original state unchanged.
    pub fn apply(&mut self, config: &BlockchainConfig, transaction: &Transaction) -> RuntimeResult<Receipt> {
        transaction.validate()?;
        let mut candidate = self.clone();
        let receipt = candidate.apply_inner(config, transaction)?;
        *self = candidate;
        Ok(receipt)
    }

    pub fn apply_batch(
        &mut self,
        config: &BlockchainConfig,
        transactions: &[Transaction],
    ) -> RuntimeResult<Vec<Receipt>> {
        let mut candidate = self.clone();
        let mut receipts = Vec::with_capacity(transactions.len());
        for transaction in transactions {
            receipts.push(candidate.apply_inner(config, transaction)?);
        }
        *self = candidate;
        Ok(receipts)
    }

    fn apply_inner(&mut self, config: &BlockchainConfig, transaction: &Transaction) -> RuntimeResult<Receipt> {
        if transaction.gas_price < config.mempool_min_gas_price {
            return invalid("transaction gas price is below chain minimum");
        }
        if config.enforce_nonce_order && transaction.nonce != self.nonce(&transaction.sender) {
            return invalid(format!(
                "invalid nonce for {}: expected {}, received {}",
                transaction.sender,
                self.nonce(&transaction.sender),
                transaction.nonce
            ));
        }

        let gas_used = intrinsic_gas(transaction)?;
        if gas_used > transaction.gas_limit {
            return invalid("transaction gas limit is below intrinsic gas");
        }
        let fee = u128::from(gas_used)
            .checked_mul(transaction.gas_price)
            .ok_or_else(|| invalid_error("transaction fee overflow"))?;
        let total_debit = transaction
            .value
            .checked_add(fee)
            .ok_or_else(|| invalid_error("transaction debit overflow"))?;

        let sender = self.accounts.entry(transaction.sender.clone()).or_default();
        if config.strict_accounting && sender.balance < total_debit {
            return invalid("insufficient sender balance");
        }
        sender.balance = sender.balance.saturating_sub(total_debit);
        sender.nonce = sender
            .nonce
            .checked_add(1)
            .ok_or_else(|| invalid_error("sender nonce overflow"))?;

        if !transaction.to.is_empty() {
            let recipient = self.accounts.entry(transaction.to.clone()).or_default();
            recipient.balance = recipient
                .balance
                .checked_add(transaction.value)
                .ok_or_else(|| invalid_error("recipient balance overflow"))?;
        }

        Ok(Receipt {
            transaction_hash: transaction.hash.clone(),
            success: true,
            gas_used,
            fee_charged: fee,
            sender: transaction.sender.clone(),
            recipient: transaction.to.clone(),
            sender_nonce: transaction.nonce,
            error: None,
            logs: Vec::new(),
        })
    }
}

pub fn intrinsic_gas(transaction: &Transaction) -> RuntimeResult<u64> {
    let data = transaction.data.strip_prefix("0x").unwrap_or(&transaction.data);
    if data.len() % 2 != 0 {
        return invalid("transaction data must contain complete hexadecimal bytes");
    }
    let mut gas = 21_000_u64;
    for pair in data.as_bytes().chunks_exact(2) {
        let byte = std::str::from_utf8(pair)
            .ok()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            .ok_or_else(|| invalid_error("transaction data contains invalid hexadecimal"))?;
        gas = gas
            .checked_add(if byte == 0 { 4 } else { 16 })
            .ok_or_else(|| invalid_error("intrinsic gas overflow"))?;
    }
    Ok(gas)
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

    fn funded_state() -> (BlockchainConfig, ChainState) {
        let mut config = BlockchainConfig::default();
        config.strict_accounting = true;
        let mut state = ChainState::default();
        state.set_account(
            "alice",
            Account {
                balance: 1_000_000,
                ..Account::default()
            },
        );
        (config, state)
    }

    #[test]
    fn applies_transfer_and_increments_nonce() {
        let (config, mut state) = funded_state();
        let transaction = Transaction::transfer("alice", "bob", 100, 0).unwrap();
        let receipt = state.apply(&config, &transaction).unwrap();
        assert!(receipt.success);
        assert_eq!(state.balance("bob"), 100);
        assert_eq!(state.nonce("alice"), 1);
    }

    #[test]
    fn failed_transition_rolls_back() {
        let (config, mut state) = funded_state();
        let before = state.clone();
        let transaction = Transaction::transfer("alice", "bob", 2_000_000, 0).unwrap();
        assert!(state.apply(&config, &transaction).is_err());
        assert_eq!(state, before);
    }

    #[test]
    fn rejects_nonce_gaps() {
        let (config, mut state) = funded_state();
        let transaction = Transaction::transfer("alice", "bob", 1, 3).unwrap();
        assert!(state.apply(&config, &transaction).is_err());
    }

    #[test]
    fn calculates_data_intrinsic_gas() {
        let transaction = Transaction::new(
            "alice",
            "bob",
            0,
            "0x0001",
            0,
            30_000,
            0,
            "call",
            None,
            BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(intrinsic_gas(&transaction).unwrap(), 21_020);
    }
}
