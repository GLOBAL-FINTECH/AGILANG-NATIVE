use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryBudgetLimits {
    pub maximum_query_bytes: usize,
    pub maximum_parameters: usize,
    pub maximum_result_rows: usize,
}

impl QueryBudgetLimits {
    pub fn default_limits() -> Self {
        Self {
            maximum_query_bytes: 1_048_576,
            maximum_parameters: 4_096,
            maximum_result_rows: 100_000,
        }
    }

    pub fn validate_query_size(&self, sql: &str) -> Result<()> {
        if sql.len() > self.maximum_query_bytes {
            bail!(
                "E6508 Query size limit exceeded: {} > {}",
                sql.len(),
                self.maximum_query_bytes
            );
        }
        Ok(())
    }
}

impl Default for QueryBudgetLimits {
    fn default() -> Self {
        Self::default_limits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_size_budget_validation() {
        let limits = QueryBudgetLimits::default_limits();
        assert!(limits.validate_query_size("SELECT 1").is_ok());

        let huge_query = "A".repeat(2_000_000);
        let err = limits.validate_query_size(&huge_query);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6508"));
    }
}
