use agilang_database_driver::{DatabaseConnection, DatabaseRow, DatabaseValue, ExecutionResult};
use anyhow::Result;

pub struct MySqlConnection {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub in_transaction: bool,
}

impl MySqlConnection {
    pub fn new(host: impl Into<String>, port: u16, database: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            database: database.into(),
            in_transaction: false,
        }
    }

    pub fn ping(&self) -> bool {
        use std::net::{SocketAddr, TcpStream};
        use std::time::Duration;
        let addr_str = format!("{}:{}", self.host, self.port);
        if let Ok(addr) = addr_str.parse::<SocketAddr>() {
            TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
        } else {
            false
        }
    }
}

impl DatabaseConnection for MySqlConnection {
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
    fn test_mysql_connection_instantiation() {
        let conn = MySqlConnection::new("127.0.0.1", 3306, "app_db");
        assert_eq!(conn.port, 3306);
    }
}
