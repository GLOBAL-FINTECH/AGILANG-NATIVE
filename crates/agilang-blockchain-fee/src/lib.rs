//! Deterministic EIP-1559-style fee calculations for Native AGILANG.

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};

pub const DEFAULT_INITIAL_BASE_FEE: u128 = 1_000_000_000;
pub const DEFAULT_PRIORITY_FEE: u128 = 1_000_000_000;
pub const BASE_FEE_MAX_CHANGE_DENOMINATOR: u128 = 8;
pub const ELASTICITY_MULTIPLIER: u64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeSettlement {
    pub effective_gas_price: u128,
    pub base_fee_per_gas: u128,
    pub priority_fee_per_gas: u128,
    pub burned_fee: u128,
    pub validator_tip: u128,
    pub total_fee: u128,
}

pub fn target_gas(gas_limit: u64) -> RuntimeResult<u64> {
    if gas_limit == 0 {
        return invalid("gas limit must be greater than zero");
    }
    Ok((gas_limit / ELASTICITY_MULTIPLIER).max(1))
}

/// Calculate the child block base fee using the EIP-1559 adjustment rule.
pub fn next_base_fee(
    parent_base_fee: u128,
    parent_gas_used: u64,
    parent_gas_limit: u64,
) -> RuntimeResult<u128> {
    let target = target_gas(parent_gas_limit)?;
    if parent_gas_used == target {
        return Ok(parent_base_fee);
    }
    if parent_gas_used > target {
        let excess = u128::from(parent_gas_used - target);
        let change = parent_base_fee
            .checked_mul(excess)
            .and_then(|value| value.checked_div(u128::from(target)))
            .and_then(|value| value.checked_div(BASE_FEE_MAX_CHANGE_DENOMINATOR))
            .ok_or_else(|| invalid_error("base-fee increase overflow"))?
            .max(1);
        return parent_base_fee
            .checked_add(change)
            .ok_or_else(|| invalid_error("base-fee overflow"));
    }
    let deficit = u128::from(target - parent_gas_used);
    let change = parent_base_fee
        .checked_mul(deficit)
        .and_then(|value| value.checked_div(u128::from(target)))
        .and_then(|value| value.checked_div(BASE_FEE_MAX_CHANGE_DENOMINATOR))
        .ok_or_else(|| invalid_error("base-fee decrease overflow"))?;
    Ok(parent_base_fee.saturating_sub(change))
}

pub fn settle_legacy(gas_used: u64, gas_price: u128, base_fee: u128) -> RuntimeResult<FeeSettlement> {
    if gas_price < base_fee {
        return invalid("legacy gas price is below the block base fee");
    }
    settle(gas_used, gas_price, base_fee)
}

pub fn settle_eip1559(
    gas_used: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
    base_fee: u128,
) -> RuntimeResult<FeeSettlement> {
    if max_priority_fee_per_gas > max_fee_per_gas {
        return invalid("max priority fee exceeds max fee");
    }
    if max_fee_per_gas < base_fee {
        return invalid("max fee per gas is below the block base fee");
    }
    let priority_cap = max_fee_per_gas - base_fee;
    let priority = max_priority_fee_per_gas.min(priority_cap);
    let effective = base_fee
        .checked_add(priority)
        .ok_or_else(|| invalid_error("effective gas price overflow"))?;
    settle(gas_used, effective, base_fee)
}

fn settle(gas_used: u64, effective_gas_price: u128, base_fee: u128) -> RuntimeResult<FeeSettlement> {
    let priority_fee_per_gas = effective_gas_price
        .checked_sub(base_fee)
        .ok_or_else(|| invalid_error("effective gas price is below base fee"))?;
    let gas = u128::from(gas_used);
    let burned_fee = gas
        .checked_mul(base_fee)
        .ok_or_else(|| invalid_error("burned fee overflow"))?;
    let validator_tip = gas
        .checked_mul(priority_fee_per_gas)
        .ok_or_else(|| invalid_error("validator tip overflow"))?;
    let total_fee = burned_fee
        .checked_add(validator_tip)
        .ok_or_else(|| invalid_error("total fee overflow"))?;
    Ok(FeeSettlement {
        effective_gas_price,
        base_fee_per_gas: base_fee,
        priority_fee_per_gas,
        burned_fee,
        validator_tip,
        total_fee,
    })
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
    fn base_fee_increases_above_target() {
        let next = next_base_fee(1_000, 15_000_000, 20_000_000).unwrap();
        assert!(next > 1_000);
    }

    #[test]
    fn base_fee_decreases_below_target() {
        let next = next_base_fee(1_000, 5_000_000, 20_000_000).unwrap();
        assert!(next < 1_000);
    }

    #[test]
    fn dynamic_fee_caps_priority() {
        let settlement = settle_eip1559(21_000, 120, 50, 100).unwrap();
        assert_eq!(settlement.effective_gas_price, 120);
        assert_eq!(settlement.priority_fee_per_gas, 20);
        assert_eq!(settlement.burned_fee, 2_100_000);
        assert_eq!(settlement.validator_tip, 420_000);
    }
}
