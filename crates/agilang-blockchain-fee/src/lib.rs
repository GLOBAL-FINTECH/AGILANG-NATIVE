use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeeQuote {
    pub base_fee: u64,
    pub priority_fee: u64,
    pub max_fee: u64,
}

impl FeeQuote {
    pub fn total_fee(&self) -> u64 {
        self.base_fee.saturating_add(self.priority_fee)
    }
}

pub fn quote_fee(base_fee: u64, priority_fee: u64) -> RuntimeResult<FeeQuote> {
    let max_fee = base_fee
        .checked_add(priority_fee)
        .ok_or_else(|| AgilangError::new(ErrorCode::InvalidArgument, "fee overflow"))?;

    Ok(FeeQuote {
        base_fee,
        priority_fee,
        max_fee,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_fee_without_overflow() {
        let quote = quote_fee(10, 3).unwrap();
        assert_eq!(
            quote,
            FeeQuote {
                base_fee: 10,
                priority_fee: 3,
                max_fee: 13,
            }
        );
        assert_eq!(quote.total_fee(), 13);
    }

    #[test]
    fn rejects_fee_overflow() {
        let error = quote_fee(u64::MAX, 1).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }
}
