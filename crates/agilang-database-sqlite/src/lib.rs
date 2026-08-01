use agilang_database_driver::{
    DatabaseCapability, DatabaseConnection, DatabaseDriverKind, DatabaseHealth, DatabaseRow,
    DatabaseValue, ExecutionResult,
};
use anyhow::{bail, Context, Result};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params_from_iter, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct SqliteConnection {
    pub path: String,
    conn: Arc<Mutex<Connection>>,
    pub in_transaction: bool,
}

impl SqliteConnection {
    pub fn new(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        let conn = initialize_connection(&path)?;
        Ok(Self {
            path,
            conn: Arc::new(Mutex::new(conn)),
            in_transaction: false,
        })
    }

    pub fn file_exists(&self) -> bool {
        self.path == ":memory:" || Path::new(&self.path).exists()
    }

    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("sqlite connection mutex poisoned"))
    }
}

impl DatabaseConnection for SqliteConnection {
    fn execute(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<ExecutionResult> {
        let conn = self.conn()?;
        let changed = conn
            .execute(sql, params_from_iter(params.iter().map(to_sqlite_value)))
            .with_context(|| format!("sqlite execute failed for `{sql}`"))?;
        Ok(ExecutionResult {
            rows_affected: changed as u64,
            last_insert_id: Some(conn.last_insert_rowid()),
        })
    }

    fn query(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<Vec<DatabaseRow>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(sql)
            .with_context(|| format!("sqlite prepare failed for `{sql}`"))?;
        let column_names = stmt
            .column_names()
            .into_iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let mut rows = stmt
            .query(params_from_iter(params.iter().map(to_sqlite_value)))
            .with_context(|| format!("sqlite query failed for `{sql}`"))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut mapped = DatabaseRow::new();
            for (index, name) in column_names.iter().enumerate() {
                mapped.insert(name.clone(), from_sqlite_value(row.get_ref(index)?));
            }
            out.push(mapped);
        }
        Ok(out)
    }

    fn begin_transaction(&mut self) -> Result<()> {
        if self.in_transaction {
            bail!("E6330 sqlite transaction already active");
        }
        self.conn()?
            .execute_batch("BEGIN IMMEDIATE TRANSACTION")
            .context("failed to begin sqlite transaction")?;
        self.in_transaction = true;
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if self.in_transaction {
            self.conn()?
                .execute_batch("COMMIT")
                .context("failed to commit sqlite transaction")?;
        }
        self.in_transaction = false;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        if self.in_transaction {
            self.conn()?
                .execute_batch("ROLLBACK")
                .context("failed to rollback sqlite transaction")?;
        }
        self.in_transaction = false;
        Ok(())
    }

    fn health_check(&mut self) -> Result<DatabaseHealth> {
        let conn = self.conn()?;
        let value: i64 = conn
            .query_row("SELECT 1", [], |row| row.get(0))
            .context("sqlite health check failed")?;
        Ok(DatabaseHealth {
            healthy: value == 1,
            message: "ok".to_string(),
        })
    }

    fn inspect_tables(&mut self) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn acquire_migration_lock(&mut self, lock_name: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agilang_migration_locks (name TEXT PRIMARY KEY, acquired_at TEXT NOT NULL)",
        )?;
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO agilang_migration_locks (name, acquired_at) VALUES (?, datetime('now'))",
            [lock_name],
        )?;
        if inserted == 0 {
            bail!("E6110 Another migration process is currently active");
        }
        Ok(())
    }

    fn release_migration_lock(&mut self, lock_name: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM agilang_migration_locks WHERE name = ?",
            [lock_name],
        )?;
        Ok(())
    }

    fn capabilities(&self) -> Vec<DatabaseCapability> {
        vec![
            DatabaseCapability::Transactions,
            DatabaseCapability::TransactionalDdl,
            DatabaseCapability::PreparedStatements,
            DatabaseCapability::SchemaInspection,
            DatabaseCapability::MigrationLocking,
            DatabaseCapability::ForeignKeys,
        ]
    }

    fn driver_kind(&self) -> DatabaseDriverKind {
        DatabaseDriverKind::Sqlite
    }
}

fn initialize_connection(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open sqlite database at {}", path))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
        .context("failed to initialize sqlite pragmas")?;
    Ok(conn)
}

fn to_sqlite_value(value: &DatabaseValue) -> SqlValue {
    match value {
        DatabaseValue::Null => SqlValue::Null,
        DatabaseValue::Boolean(value) => SqlValue::Integer(i64::from(*value)),
        DatabaseValue::Integer(value) => SqlValue::Integer(*value),
        DatabaseValue::Float(value) => SqlValue::Real(*value),
        DatabaseValue::Decimal(value)
        | DatabaseValue::Text(value)
        | DatabaseValue::DateTime(value)
        | DatabaseValue::Json(value) => SqlValue::Text(value.clone()),
        DatabaseValue::Binary(value) => SqlValue::Blob(value.clone()),
    }
}

fn from_sqlite_value(value: ValueRef<'_>) -> DatabaseValue {
    match value {
        ValueRef::Null => DatabaseValue::Null,
        ValueRef::Integer(value) => DatabaseValue::Integer(value),
        ValueRef::Real(value) => DatabaseValue::Float(value),
        ValueRef::Text(value) => DatabaseValue::Text(String::from_utf8_lossy(value).to_string()),
        ValueRef::Blob(value) => DatabaseValue::Binary(value.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_sqlite_path(name: &str) -> String {
        std::env::temp_dir()
            .join(format!(
                "agilang-sqlite-{name}-{}-{}.db",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn test_sqlite_connection_transaction_lifecycle() {
        let mut conn = SqliteConnection::new(":memory:").unwrap();
        assert!(!conn.in_transaction);
        conn.begin_transaction().unwrap();
        assert!(conn.in_transaction);
        conn.commit().unwrap();
        assert!(!conn.in_transaction);
    }

    #[test]
    fn sqlite_migration_lock_is_exclusive() {
        let path = temp_sqlite_path("lock");
        let mut conn = SqliteConnection::new(path.clone()).unwrap();
        conn.acquire_migration_lock("default").unwrap();
        let mut second = SqliteConnection::new(path).unwrap();
        let err = second.acquire_migration_lock("default").unwrap_err();
        assert!(err.to_string().contains("E6110"));
        conn.release_migration_lock("default").unwrap();
        second.acquire_migration_lock("default").unwrap();
    }
}
