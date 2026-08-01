use agilang_database_driver::DatabaseConnection;
use anyhow::Result;

pub struct TransactionManager;

impl TransactionManager {
    pub fn transaction<T, F>(conn: &mut dyn DatabaseConnection, action: F) -> Result<T>
    where
        F: FnOnce(&mut dyn DatabaseConnection) -> Result<T>,
    {
        conn.begin_transaction()?;
        match action(conn) {
            Ok(result) => {
                conn.commit()?;
                Ok(result)
            }
            Err(e) => {
                let _ = conn.rollback();
                Err(e)
            }
        }
    }

    pub fn compile_savepoint(name: &str) -> String {
        format!("SAVEPOINT {}", name)
    }

    pub fn compile_release_savepoint(name: &str) -> String {
        format!("RELEASE SAVEPOINT {}", name)
    }

    pub fn compile_rollback_savepoint(name: &str) -> String {
        format!("ROLLBACK TO SAVEPOINT {}", name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_savepoint_sql_compilation() {
        let sp = TransactionManager::compile_savepoint("agilang_sp_1");
        assert_eq!(sp, "SAVEPOINT agilang_sp_1");

        let rel = TransactionManager::compile_release_savepoint("agilang_sp_1");
        assert_eq!(rel, "RELEASE SAVEPOINT agilang_sp_1");
    }
}
