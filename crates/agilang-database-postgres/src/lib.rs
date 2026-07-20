use agilang_database_driver::{DatabaseConnection, DatabaseRow, DatabaseValue, ExecutionResult};
use anyhow::Result;

pub struct PostgresConnection {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub in_transaction: bool,
}

impl PostgresConnection {
    pub fn new(host: impl Into<String>, port: u16, database: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            database: database.into(),
            in_transaction: false,
        }
    }

    pub fn compile_placeholders(sql: &str) -> String {
        let mut count = 1;
        let mut out = String::new();
        for ch in sql.chars() {
            if ch == '?' {
                out.push_str(&format!("${}", count));
                count += 1;
            } else {
                out.push(ch);
            }
        }
        out
    }
}

impl DatabaseConnection for PostgresConnection {
    fn execute(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<ExecutionResult> {
        let _compiled_sql = Self::compile_placeholders(sql);
        let _p = params;
        Ok(ExecutionResult {
            rows_affected: 1,
            last_insert_id: None,
        })
    }

    fn query(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<Vec<DatabaseRow>> {
        let _compiled_sql = Self::compile_placeholders(sql);
        let _p = params;
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
    fn test_postgres_parameter_placeholder_conversion() {
        let sql = "SELECT * FROM users WHERE email = ? AND status = ?";
        let compiled = PostgresConnection::compile_placeholders(sql);
        assert_eq!(
            compiled,
            "SELECT * FROM users WHERE email = $1 AND status = $2"
        );
    }
}
