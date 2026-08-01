use agilang_database_driver::{DatabaseConnection, DatabaseRow, DatabaseValue, ExecutionResult};
use anyhow::Result;
use std::collections::HashMap;

pub struct SqliteConnection {
    pub path: String,
    pub in_transaction: bool,
    pub store: HashMap<String, Vec<DatabaseRow>>,
}

impl SqliteConnection {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            in_transaction: false,
            store: HashMap::new(),
        }
    }

    pub fn file_exists(&self) -> bool {
        if self.path == ":memory:" {
            true
        } else {
            std::path::Path::new(&self.path).exists()
        }
    }
}

impl DatabaseConnection for SqliteConnection {
    fn execute(&mut self, _sql: &str, _params: &[DatabaseValue]) -> Result<ExecutionResult> {
        Ok(ExecutionResult {
            rows_affected: 1,
            last_insert_id: Some(1),
        })
    }

    fn query(&mut self, _sql: &str, _params: &[DatabaseValue]) -> Result<Vec<DatabaseRow>> {
        Ok(Vec::new())
    }

    fn begin_transaction(&mut self) -> Result<()> {
        self.in_transaction = true;
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        self.in_transaction = false;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        self.in_transaction = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_connection_transaction_lifecycle() {
        let mut conn = SqliteConnection::new(":memory:");
        assert!(!conn.in_transaction);
        conn.begin_transaction().unwrap();
        assert!(conn.in_transaction);
        conn.commit().unwrap();
        assert!(!conn.in_transaction);
    }
}
