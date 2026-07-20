use agilang_database_driver::{DatabaseConnection, DatabaseRow, DatabaseValue, ExecutionResult};
use agilang_database_storage::PageManager;
use agilang_database_wal::WriteAheadLog;
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use std::sync::Mutex;

static DB_PROCESS_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

#[derive(Debug)]
pub struct AgiDbEngine {
    pub path: String,
    pub storage: PageManager,
    pub wal: WriteAheadLog,
    pub air_gapped: bool,
}

impl AgiDbEngine {
    pub fn open(path: impl Into<String>, air_gapped: bool) -> Result<Self> {
        let mut guard = DB_PROCESS_LOCK.lock().unwrap();
        if *guard {
            bail!("E6301 Database already opened for writing: single writer lock active");
        }
        *guard = true;

        Ok(Self {
            path: path.into(),
            storage: PageManager::new(),
            wal: WriteAheadLog::new(),
            air_gapped,
        })
    }

    pub fn close(&mut self) {
        let mut guard = DB_PROCESS_LOCK.lock().unwrap();
        *guard = false;
    }
}

pub struct AgiDbConnection {
    pub engine_path: String,
    pub in_transaction: bool,
}

impl AgiDbConnection {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            engine_path: path.into(),
            in_transaction: false,
        }
    }
}

impl DatabaseConnection for AgiDbConnection {
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
    fn test_single_writer_process_lock() {
        let mut eng1 = AgiDbEngine::open("storage/test.agidb", true).unwrap();
        let eng2_err = AgiDbEngine::open("storage/test.agidb", true);

        assert!(eng2_err.is_err());
        assert!(eng2_err.unwrap_err().to_string().contains("E6301"));

        eng1.close();
        let eng3 = AgiDbEngine::open("storage/test.agidb", true);
        assert!(eng3.is_ok());
    }
}
