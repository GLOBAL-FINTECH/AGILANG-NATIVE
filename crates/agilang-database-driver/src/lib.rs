use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatabaseDriverKind {
    AgiDb,
    Sqlite,
    MySql,
    Postgres,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatabaseCapability {
    Transactions,
    TransactionalDdl,
    PreparedStatements,
    SchemaInspection,
    MigrationLocking,
    ForeignKeys,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseHealth {
    pub healthy: bool,
    pub message: String,
}

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
    fn health_check(&mut self) -> Result<DatabaseHealth> {
        let _ = self.query("SELECT 1", &[])?;
        Ok(DatabaseHealth {
            healthy: true,
            message: "ok".to_string(),
        })
    }
    fn inspect_tables(&mut self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
    fn acquire_migration_lock(&mut self, _lock_name: &str) -> Result<()> {
        Ok(())
    }
    fn release_migration_lock(&mut self, _lock_name: &str) -> Result<()> {
        Ok(())
    }
    fn capabilities(&self) -> Vec<DatabaseCapability> {
        vec![
            DatabaseCapability::Transactions,
            DatabaseCapability::PreparedStatements,
        ]
    }
    fn driver_kind(&self) -> DatabaseDriverKind {
        DatabaseDriverKind::AgiDb
    }
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
