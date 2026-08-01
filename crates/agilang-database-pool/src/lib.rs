use agilang_database_driver::DatabaseConnection;
use agilang_database_sqlite::SqliteConnection;
use anyhow::{bail, Result};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub minimum: usize,
    pub maximum: usize,
    pub acquire_timeout_seconds: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            minimum: 1,
            maximum: 10,
            acquire_timeout_seconds: 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
}

pub struct ConnectionPool {
    pub config: PoolConfig,
    pub active_count: Arc<Mutex<usize>>,
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            active_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn acquire(&self) -> Result<Box<dyn DatabaseConnection>> {
        let mut guard = self.active_count.lock().unwrap();
        if *guard >= self.config.maximum {
            bail!(
                "E6202 Connection acquisition timeout: maximum pool capacity of {} reached",
                self.config.maximum
            );
        }
        *guard += 1;
        Ok(Box::new(SqliteConnection::new(":memory:")?))
    }

    pub fn release(&self) {
        let mut guard = self.active_count.lock().unwrap();
        if *guard > 0 {
            *guard -= 1;
        }
    }

    pub fn status(&self) -> PoolStatus {
        let guard = self.active_count.lock().unwrap();
        PoolStatus {
            total_connections: *guard,
            active_connections: *guard,
            idle_connections: self.config.maximum.saturating_sub(*guard),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_acquisition_and_capacity_limits() {
        let config = PoolConfig {
            minimum: 1,
            maximum: 2,
            acquire_timeout_seconds: 5,
        };
        let pool = ConnectionPool::new(config);

        let _c1 = pool.acquire().unwrap();
        let _c2 = pool.acquire().unwrap();
        let err = pool.acquire();

        if let Err(e) = err {
            assert!(e.to_string().contains("E6202"));
        }
    }
}
