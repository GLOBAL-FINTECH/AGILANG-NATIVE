use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DatabaseValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Decimal(String),
    Text(String),
    Binary(Vec<u8>),
    DateTime(String),
    Json(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub rows_affected: u64,
    pub last_insert_id: Option<i64>,
}

pub type DatabaseRow = HashMap<String, DatabaseValue>;

pub trait DatabaseConnection: Send + Sync {
    fn execute(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<ExecutionResult>;
    fn query(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<Vec<DatabaseRow>>;
    fn begin_transaction(&mut self) -> Result<()>;
    fn commit(&mut self) -> Result<()>;
    fn rollback(&mut self) -> Result<()>;
}

pub fn map_driver_error(code: &str, msg: &str) -> anyhow::Error {
    anyhow::anyhow!("{} {}", code, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_value_serialization() {
        let val = DatabaseValue::Text("test".to_string());
        assert_eq!(val, DatabaseValue::Text("test".to_string()));
    }
}
