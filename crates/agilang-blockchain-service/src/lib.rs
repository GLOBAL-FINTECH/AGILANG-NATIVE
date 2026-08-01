//! Mutable blockchain service dispatch, including Ethereum raw transaction admission.

use agilang_blockchain_core::Transaction;
use agilang_blockchain_node::BlockchainNode;
use agilang_blockchain_transaction::{decode_raw_transaction, hex_address, RawTransaction};
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Stateful RPC facade around `BlockchainNode`.
///
/// The node's read-only dispatcher remains available, while mutating methods are
/// handled here so signed raw transactions can enter the mempool safely.
pub struct BlockchainService {
    pub node: BlockchainNode,
    raw_to_native_hash: BTreeMap<String, String>,
    native_to_raw_hash: BTreeMap<String, String>,
}

impl BlockchainService {
    pub fn new(node: BlockchainNode) -> Self {
        Self {
            node,
            raw_to_native_hash: BTreeMap::new(),
            native_to_raw_hash: BTreeMap::new(),
        }
    }

    pub fn rpc(&mut self, method: &str, params: Value, now_ms: u64) -> RuntimeResult<Value> {
        match method {
            "eth_sendRawTransaction" => {
                let encoded = param_string(&params, 0)?;
                let raw = decode_hex(encoded)?;
                let decoded = decode_raw_transaction(&raw)?;
                let (transaction, ethereum_hash) = self.convert_raw(decoded, encoded)?;
                let native_hash = transaction.hash.clone();
                self.node.submit_transaction(transaction, now_ms)?;
                self.raw_to_native_hash
                    .insert(ethereum_hash.clone(), native_hash.clone());
                self.native_to_raw_hash
                    .insert(native_hash, ethereum_hash.clone());
                Ok(json!(ethereum_hash))
            }
            "eth_getTransactionByHash" | "eth_getTransactionReceipt" => {
                let requested = param_string(&params, 0)?;
                let native = self
                    .raw_to_native_hash
                    .get(requested)
                    .cloned()
                    .unwrap_or_else(|| requested.to_string());
                let mut result = self.node.rpc(method, json!([native]))?;
                rewrite_hashes(&mut result, &self.native_to_raw_hash);
                Ok(result)
            }
            _ => self.node.rpc(method, params),
        }
    }

    pub fn native_hash_for_raw(&self, ethereum_hash: &str) -> Option<&str> {
        self.raw_to_native_hash.get(ethereum_hash).map(String::as_str)
    }

    fn convert_raw(
        &self,
        decoded: RawTransaction,
        raw_hex: &str,
    ) -> RuntimeResult<(Transaction, String)> {
        match decoded {
            RawTransaction::Legacy(tx) => {
                let chain_id = tx.chain_id.ok_or_else(|| {
                    invalid_error("unprotected legacy transactions are disabled")
                })?;
                self.validate_chain_id(chain_id)?;
                let nonce = to_u64(tx.nonce, "nonce")?;
                let gas_limit = to_u64(tx.gas_limit, "gas limit")?;
                let sender = hex_address(&tx.sender);
                let to = tx.to.map(|value| hex_address(&value)).unwrap_or_default();
                let ethereum_hash = hex_bytes(&tx.hash);
                let metadata = transaction_metadata(
                    &ethereum_hash,
                    raw_hex,
                    "legacy",
                    chain_id,
                    None,
                );
                let transaction = Transaction::new(
                    sender,
                    to,
                    tx.value,
                    hex_bytes(&tx.data),
                    nonce,
                    gas_limit,
                    tx.gas_price,
                    if tx.to.is_some() { "evm_call" } else { "evm_create" },
                    Some(raw_hex.to_string()),
                    metadata,
                )?;
                Ok((transaction, ethereum_hash))
            }
            RawTransaction::Eip1559(tx) => {
                self.validate_chain_id(tx.chain_id)?;
                let nonce = to_u64(tx.nonce, "nonce")?;
                let gas_limit = to_u64(tx.gas_limit, "gas limit")?;
                let sender = hex_address(&tx.sender);
                let to = tx.to.map(|value| hex_address(&value)).unwrap_or_default();
                let ethereum_hash = hex_bytes(&tx.hash);
                let gas_price = tx.max_fee_per_gas;
                let metadata = transaction_metadata(
                    &ethereum_hash,
                    raw_hex,
                    "eip1559",
                    tx.chain_id,
                    Some((tx.max_priority_fee_per_gas, tx.max_fee_per_gas)),
                );
                let transaction = Transaction::new(
                    sender,
                    to,
                    tx.value,
                    hex_bytes(&tx.data),
                    nonce,
                    gas_limit,
                    gas_price,
                    if tx.to.is_some() { "evm_call" } else { "evm_create" },
                    Some(raw_hex.to_string()),
                    metadata,
                )?;
                Ok((transaction, ethereum_hash))
            }
        }
    }

    fn validate_chain_id(&self, chain_id: u64) -> RuntimeResult<()> {
        if chain_id != self.node.config.chain_id {
            return invalid(format!(
                "raw transaction chain ID {chain_id} does not match node chain ID {}",
                self.node.config.chain_id
            ));
        }
        Ok(())
    }
}

fn transaction_metadata(
    ethereum_hash: &str,
    raw_hex: &str,
    envelope: &str,
    chain_id: u64,
    dynamic_fees: Option<(u128, u128)>,
) -> BTreeMap<String, Value> {
    let mut metadata = BTreeMap::from([
        ("ethereum_hash".to_string(), json!(ethereum_hash)),
        ("raw_transaction".to_string(), json!(raw_hex)),
        ("envelope".to_string(), json!(envelope)),
        ("chain_id".to_string(), json!(chain_id)),
    ]);
    if let Some((priority, maximum)) = dynamic_fees {
        metadata.insert("max_priority_fee_per_gas".to_string(), json!(priority.to_string()));
        metadata.insert("max_fee_per_gas".to_string(), json!(maximum.to_string()));
    }
    metadata
}

fn rewrite_hashes(value: &mut Value, aliases: &BTreeMap<String, String>) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for key in ["hash", "transactionHash"] {
        if let Some(native) = object.get(key).and_then(Value::as_str) {
            if let Some(raw) = aliases.get(native) {
                object.insert(key.to_string(), Value::String(raw.clone()));
            }
        }
    }
}

fn decode_hex(value: &str) -> RuntimeResult<Vec<u8>> {
    let raw = value
        .strip_prefix("0x")
        .ok_or_else(|| invalid_error("raw transaction must start with 0x"))?;
    if raw.is_empty() || raw.len() % 2 != 0 {
        return invalid("raw transaction hex must contain complete bytes");
    }
    let mut output = Vec::with_capacity(raw.len() / 2);
    for index in (0..raw.len()).step_by(2) {
        let byte = u8::from_str_radix(&raw[index..index + 2], 16)
            .map_err(|_| invalid_error("raw transaction contains invalid hexadecimal"))?;
        output.push(byte);
    }
    Ok(output)
}

fn hex_bytes(value: &[u8]) -> String {
    let mut output = String::with_capacity(value.len() * 2 + 2);
    output.push_str("0x");
    for byte in value {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn to_u64(value: u128, name: &str) -> RuntimeResult<u64> {
    u64::try_from(value).map_err(|_| invalid_error(format!("transaction {name} exceeds u64")))
}

fn param_string<'a>(params: &'a Value, index: usize) -> RuntimeResult<&'a str> {
    params
        .as_array()
        .and_then(|values| values.get(index))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_error(format!("JSON-RPC parameter {index} must be a string")))
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

    #[test]
    fn decodes_prefixed_hex() {
        assert_eq!(decode_hex("0x02c0").unwrap(), vec![0x02, 0xc0]);
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(decode_hex("02c0").is_err());
        assert!(decode_hex("0x0").is_err());
        assert!(decode_hex("0xzz").is_err());
    }

    #[test]
    fn rewrites_native_hash_aliases() {
        let mut value = json!({"hash":"native"});
        rewrite_hashes(&mut value, &BTreeMap::from([("native".into(), "raw".into())]));
        assert_eq!(value["hash"], "raw");
    }
}
