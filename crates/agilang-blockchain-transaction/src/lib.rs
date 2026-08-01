//! Ethereum-compatible raw transaction decoding for Native AGILANG.
//!
//! Supports legacy/EIP-155 and EIP-1559 envelopes, strict RLP decoding,
//! Keccak-256 transaction hashes and secp256k1 public-key recovery.

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawTransaction {
    Legacy(LegacyTransaction),
    Eip1559(Eip1559Transaction),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyTransaction {
    pub nonce: u128,
    pub gas_price: u128,
    pub gas_limit: u128,
    pub to: Option<[u8; 20]>,
    pub value: u128,
    pub data: Vec<u8>,
    pub chain_id: Option<u64>,
    pub recovery_id: u8,
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub hash: [u8; 32],
    pub sender: [u8; 20],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eip1559Transaction {
    pub chain_id: u64,
    pub nonce: u128,
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
    pub gas_limit: u128,
    pub to: Option<[u8; 20]>,
    pub value: u128,
    pub data: Vec<u8>,
    pub access_list: Vec<AccessListEntry>,
    pub recovery_id: u8,
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub hash: [u8; 32],
    pub sender: [u8; 20],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessListEntry {
    pub address: [u8; 20],
    pub storage_keys: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Rlp<'a> {
    Bytes(&'a [u8]),
    List(Vec<Rlp<'a>>),
}

pub fn decode_raw_transaction(input: &[u8]) -> RuntimeResult<RawTransaction> {
    if input.is_empty() {
        return invalid("raw transaction is empty");
    }
    if input[0] == 0x02 {
        decode_eip1559(&input[1..]).map(RawTransaction::Eip1559)
    } else if input[0] >= 0xc0 {
        decode_legacy(input).map(RawTransaction::Legacy)
    } else {
        invalid(format!("unsupported transaction envelope type 0x{:02x}", input[0]))
    }
}

fn decode_legacy(raw: &[u8]) -> RuntimeResult<LegacyTransaction> {
    let root = decode_complete_rlp(raw)?;
    let fields = expect_list(&root, 9, "legacy transaction")?;
    let nonce = uint(&fields[0])?;
    let gas_price = uint(&fields[1])?;
    let gas_limit = uint(&fields[2])?;
    let to = address(&fields[3])?;
    let value = uint(&fields[4])?;
    let data = bytes(&fields[5])?.to_vec();
    let v = uint(&fields[6])?;
    let r = scalar(&fields[7], "r")?;
    let s = scalar(&fields[8], "s")?;

    let (chain_id, recovery_id) = match v {
        27 | 28 => (None, (v - 27) as u8),
        value if value >= 35 => {
            let adjusted = value - 35;
            let chain_id = u64::try_from(adjusted / 2)
                .map_err(|_| invalid_error("legacy chain ID exceeds u64"))?;
            (Some(chain_id), (adjusted % 2) as u8)
        }
        _ => return invalid("invalid legacy transaction v value"),
    };

    let signing_fields = if let Some(chain_id) = chain_id {
        vec![
            rlp_uint(nonce),
            rlp_uint(gas_price),
            rlp_uint(gas_limit),
            rlp_optional_address(to),
            rlp_uint(value),
            rlp_bytes(&data),
            rlp_uint(u128::from(chain_id)),
            rlp_bytes(&[]),
            rlp_bytes(&[]),
        ]
    } else {
        vec![
            rlp_uint(nonce),
            rlp_uint(gas_price),
            rlp_uint(gas_limit),
            rlp_optional_address(to),
            rlp_uint(value),
            rlp_bytes(&data),
        ]
    };
    let signing_hash = keccak(&rlp_list(&signing_fields));
    let sender = recover_sender(signing_hash, recovery_id, r, s)?;

    Ok(LegacyTransaction {
        nonce,
        gas_price,
        gas_limit,
        to,
        value,
        data,
        chain_id,
        recovery_id,
        r,
        s,
        hash: keccak(raw),
        sender,
    })
}

fn decode_eip1559(payload: &[u8]) -> RuntimeResult<Eip1559Transaction> {
    let root = decode_complete_rlp(payload)?;
    let fields = expect_list(&root, 12, "EIP-1559 transaction")?;
    let chain_id = u64::try_from(uint(&fields[0])?)
        .map_err(|_| invalid_error("EIP-1559 chain ID exceeds u64"))?;
    let nonce = uint(&fields[1])?;
    let max_priority_fee_per_gas = uint(&fields[2])?;
    let max_fee_per_gas = uint(&fields[3])?;
    if max_priority_fee_per_gas > max_fee_per_gas {
        return invalid("max priority fee exceeds max fee");
    }
    let gas_limit = uint(&fields[4])?;
    let to = address(&fields[5])?;
    let value = uint(&fields[6])?;
    let data = bytes(&fields[7])?.to_vec();
    let access_list = decode_access_list(&fields[8])?;
    let recovery_id = u8::try_from(uint(&fields[9])?)
        .map_err(|_| invalid_error("invalid EIP-1559 y parity"))?;
    if recovery_id > 1 {
        return invalid("EIP-1559 y parity must be 0 or 1");
    }
    let r = scalar(&fields[10], "r")?;
    let s = scalar(&fields[11], "s")?;

    let unsigned = rlp_list(&[
        rlp_uint(u128::from(chain_id)),
        rlp_uint(nonce),
        rlp_uint(max_priority_fee_per_gas),
        rlp_uint(max_fee_per_gas),
        rlp_uint(gas_limit),
        rlp_optional_address(to),
        rlp_uint(value),
        rlp_bytes(&data),
        encode_access_list(&access_list),
    ]);
    let mut signing_payload = Vec::with_capacity(unsigned.len() + 1);
    signing_payload.push(0x02);
    signing_payload.extend_from_slice(&unsigned);
    let signing_hash = keccak(&signing_payload);
    let sender = recover_sender(signing_hash, recovery_id, r, s)?;

    let mut envelope = Vec::with_capacity(payload.len() + 1);
    envelope.push(0x02);
    envelope.extend_from_slice(payload);
    Ok(Eip1559Transaction {
        chain_id,
        nonce,
        max_priority_fee_per_gas,
        max_fee_per_gas,
        gas_limit,
        to,
        value,
        data,
        access_list,
        recovery_id,
        r,
        s,
        hash: keccak(&envelope),
        sender,
    })
}

fn decode_access_list(value: &Rlp<'_>) -> RuntimeResult<Vec<AccessListEntry>> {
    let entries = match value {
        Rlp::List(entries) => entries,
        Rlp::Bytes(_) => return invalid("access list must be an RLP list"),
    };
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let pair = expect_list(entry, 2, "access-list entry")?;
        let address_bytes = bytes(&pair[0])?;
        if address_bytes.len() != 20 {
            return invalid("access-list address must contain 20 bytes");
        }
        let mut address = [0_u8; 20];
        address.copy_from_slice(address_bytes);
        let keys = match &pair[1] {
            Rlp::List(keys) => keys,
            Rlp::Bytes(_) => return invalid("access-list storage keys must be a list"),
        };
        let mut storage_keys = Vec::with_capacity(keys.len());
        for key in keys {
            let key_bytes = bytes(key)?;
            if key_bytes.len() != 32 {
                return invalid("access-list storage key must contain 32 bytes");
            }
            let mut storage_key = [0_u8; 32];
            storage_key.copy_from_slice(key_bytes);
            storage_keys.push(storage_key);
        }
        result.push(AccessListEntry { address, storage_keys });
    }
    Ok(result)
}

fn encode_access_list(entries: &[AccessListEntry]) -> Vec<u8> {
    let entries = entries
        .iter()
        .map(|entry| {
            let keys = entry
                .storage_keys
                .iter()
                .map(|key| rlp_bytes(key))
                .collect::<Vec<_>>();
            rlp_list(&[rlp_bytes(&entry.address), rlp_list(&keys)])
        })
        .collect::<Vec<_>>();
    rlp_list(&entries)
}

fn recover_sender(
    digest: [u8; 32],
    recovery_id: u8,
    r: [u8; 32],
    s: [u8; 32],
) -> RuntimeResult<[u8; 20]> {
    let signature = Signature::from_scalars(r, s)
        .map_err(|error| invalid_error(format!("invalid transaction signature: {error}")))?;
    let recovery_id = RecoveryId::try_from(recovery_id)
        .map_err(|error| invalid_error(format!("invalid recovery ID: {error}")))?;
    let key = VerifyingKey::recover_from_prehash(&digest, &signature, recovery_id)
        .map_err(|error| invalid_error(format!("signature recovery failed: {error}")))?;
    let encoded = key.to_encoded_point(false);
    let public_key = encoded.as_bytes();
    let hash = keccak(&public_key[1..]);
    let mut address = [0_u8; 20];
    address.copy_from_slice(&hash[12..]);
    Ok(address)
}

fn decode_complete_rlp(input: &[u8]) -> RuntimeResult<Rlp<'_>> {
    let (value, consumed) = decode_rlp(input, 0)?;
    if consumed != input.len() {
        return invalid("trailing bytes after RLP value");
    }
    Ok(value)
}

fn decode_rlp(input: &[u8], depth: usize) -> RuntimeResult<(Rlp<'_>, usize)> {
    if depth > 64 {
        return invalid("RLP nesting exceeds 64 levels");
    }
    let first = *input.first().ok_or_else(|| invalid_error("truncated RLP value"))?;
    match first {
        0x00..=0x7f => Ok((Rlp::Bytes(&input[..1]), 1)),
        0x80..=0xb7 => {
            let len = usize::from(first - 0x80);
            let end = 1_usize.checked_add(len).ok_or_else(|| invalid_error("RLP length overflow"))?;
            if input.len() < end {
                return invalid("truncated RLP byte string");
            }
            if len == 1 && input[1] < 0x80 {
                return invalid("non-canonical RLP byte string");
            }
            Ok((Rlp::Bytes(&input[1..end]), end))
        }
        0xb8..=0xbf => {
            let length_of_length = usize::from(first - 0xb7);
            let len = decode_length(input, 1, length_of_length)?;
            if len < 56 {
                return invalid("non-canonical long RLP byte string");
            }
            let start = 1 + length_of_length;
            let end = start.checked_add(len).ok_or_else(|| invalid_error("RLP length overflow"))?;
            if input.len() < end {
                return invalid("truncated long RLP byte string");
            }
            Ok((Rlp::Bytes(&input[start..end]), end))
        }
        0xc0..=0xf7 => decode_list(input, 1, usize::from(first - 0xc0), depth),
        0xf8..=0xff => {
            let length_of_length = usize::from(first - 0xf7);
            let len = decode_length(input, 1, length_of_length)?;
            if len < 56 {
                return invalid("non-canonical long RLP list");
            }
            decode_list(input, 1 + length_of_length, len, depth)
        }
    }
}

fn decode_list(input: &[u8], start: usize, len: usize, depth: usize) -> RuntimeResult<(Rlp<'_>, usize)> {
    let end = start.checked_add(len).ok_or_else(|| invalid_error("RLP list length overflow"))?;
    if input.len() < end {
        return invalid("truncated RLP list");
    }
    let mut cursor = start;
    let mut values = Vec::new();
    while cursor < end {
        let (value, consumed) = decode_rlp(&input[cursor..end], depth + 1)?;
        if consumed == 0 {
            return invalid("RLP decoder made no progress");
        }
        cursor += consumed;
        values.push(value);
    }
    Ok((Rlp::List(values), end))
}

fn decode_length(input: &[u8], offset: usize, count: usize) -> RuntimeResult<usize> {
    if count == 0 || count > std::mem::size_of::<usize>() || input.len() < offset + count {
        return invalid("invalid RLP length prefix");
    }
    if input[offset] == 0 {
        return invalid("RLP length contains a leading zero");
    }
    let mut value = 0_usize;
    for byte in &input[offset..offset + count] {
        value = value
            .checked_mul(256)
            .and_then(|current| current.checked_add(usize::from(*byte)))
            .ok_or_else(|| invalid_error("RLP length overflow"))?;
    }
    Ok(value)
}

fn expect_list<'a>(value: &'a Rlp<'a>, expected: usize, name: &str) -> RuntimeResult<&'a [Rlp<'a>]> {
    match value {
        Rlp::List(values) if values.len() == expected => Ok(values),
        Rlp::List(values) => invalid(format!("{name} must contain {expected} fields, received {}", values.len())),
        Rlp::Bytes(_) => invalid(format!("{name} must be an RLP list")),
    }
}

fn bytes<'a>(value: &'a Rlp<'a>) -> RuntimeResult<&'a [u8]> {
    match value {
        Rlp::Bytes(value) => Ok(value),
        Rlp::List(_) => invalid("expected RLP byte string"),
    }
}

fn uint(value: &Rlp<'_>) -> RuntimeResult<u128> {
    let value = bytes(value)?;
    if value.len() > 16 {
        return invalid("integer exceeds u128");
    }
    if value.first() == Some(&0) {
        return invalid("RLP integer contains a leading zero");
    }
    Ok(value.iter().fold(0_u128, |result, byte| (result << 8) | u128::from(*byte)))
}

fn scalar(value: &Rlp<'_>, name: &str) -> RuntimeResult<[u8; 32]> {
    let value = bytes(value)?;
    if value.is_empty() || value.len() > 32 {
        return invalid(format!("signature scalar {name} has invalid length"));
    }
    if value[0] == 0 {
        return invalid(format!("signature scalar {name} contains a leading zero"));
    }
    let mut scalar = [0_u8; 32];
    scalar[32 - value.len()..].copy_from_slice(value);
    Ok(scalar)
}

fn address(value: &Rlp<'_>) -> RuntimeResult<Option<[u8; 20]>> {
    let value = bytes(value)?;
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() != 20 {
        return invalid("transaction recipient must be empty or 20 bytes");
    }
    let mut address = [0_u8; 20];
    address.copy_from_slice(value);
    Ok(Some(address))
}

fn rlp_uint(value: u128) -> Vec<u8> {
    if value == 0 {
        return rlp_bytes(&[]);
    }
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(bytes.len() - 1);
    rlp_bytes(&bytes[first..])
}

fn rlp_optional_address(value: Option<[u8; 20]>) -> Vec<u8> {
    value.map_or_else(|| rlp_bytes(&[]), |address| rlp_bytes(&address))
}

fn rlp_bytes(value: &[u8]) -> Vec<u8> {
    if value.len() == 1 && value[0] < 0x80 {
        return value.to_vec();
    }
    encode_length(0x80, 0xb7, value)
}

fn rlp_list(values: &[Vec<u8>]) -> Vec<u8> {
    let payload = values.concat();
    encode_length(0xc0, 0xf7, &payload)
}

fn encode_length(short_base: u8, long_base: u8, payload: &[u8]) -> Vec<u8> {
    if payload.len() <= 55 {
        let mut encoded = Vec::with_capacity(payload.len() + 1);
        encoded.push(short_base + payload.len() as u8);
        encoded.extend_from_slice(payload);
        return encoded;
    }
    let length = payload.len().to_be_bytes();
    let first = length.iter().position(|byte| *byte != 0).unwrap_or(length.len() - 1);
    let length = &length[first..];
    let mut encoded = Vec::with_capacity(payload.len() + length.len() + 1);
    encoded.push(long_base + length.len() as u8);
    encoded.extend_from_slice(length);
    encoded.extend_from_slice(payload);
    encoded
}

fn keccak(value: &[u8]) -> [u8; 32] {
    let digest = Keccak256::digest(value);
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    output
}

pub fn hex_address(address: &[u8; 20]) -> String {
    let mut result = String::with_capacity(42);
    result.push_str("0x");
    for byte in address {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
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
    fn rejects_trailing_rlp_bytes() {
        assert!(decode_complete_rlp(&[0x80, 0x80]).is_err());
    }

    #[test]
    fn rejects_noncanonical_single_byte_string() {
        assert!(decode_complete_rlp(&[0x81, 0x01]).is_err());
    }

    #[test]
    fn roundtrips_rlp_list_encoding() {
        let encoded = rlp_list(&[rlp_uint(1), rlp_bytes(b"hello")]);
        let decoded = decode_complete_rlp(&encoded).unwrap();
        let values = expect_list(&decoded, 2, "test").unwrap();
        assert_eq!(uint(&values[0]).unwrap(), 1);
        assert_eq!(bytes(&values[1]).unwrap(), b"hello");
    }

    #[test]
    fn rejects_unknown_typed_transaction() {
        assert!(decode_raw_transaction(&[0x03, 0xc0]).is_err());
    }
}
