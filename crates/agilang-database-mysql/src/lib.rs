use agilang_database_driver::{
    DatabaseCapability, DatabaseConnection, DatabaseDriverKind, DatabaseHealth, DatabaseRow,
    DatabaseValue, ExecutionResult,
};
use anyhow::{bail, Context, Result};
use mysql::prelude::Queryable;
use mysql::{OptsBuilder, Pool, PooledConn, Row, Value};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct MySqlConnection {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub in_transaction: bool,
    pool: Pool,
    conn: Arc<Mutex<PooledConn>>,
}

impl MySqlConnection {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        database: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self> {
        let host = host.into();
        let database = database.into();
        let username = username.into();
        let password = password.into();
        let opts = OptsBuilder::new()
            .ip_or_hostname(Some(host.clone()))
            .tcp_port(port)
            .db_name(Some(database.clone()))
            .user(Some(username.clone()))
            .pass(Some(password.clone()));
        let pool = Pool::new(opts).context("failed to create mysql pool")?;
        let conn = pool
            .get_conn()
            .context("failed to acquire mysql connection")?;
        Ok(Self {
            host,
            port,
            database,
            username,
            password,
            in_transaction: false,
            pool,
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn conn(&self) -> Result<std::sync::MutexGuard<'_, PooledConn>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("mysql connection mutex poisoned"))
    }

    pub fn ping(&self) -> bool {
        self.pool.get_conn().is_ok()
    }
}

impl DatabaseConnection for MySqlConnection {
    fn execute(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<ExecutionResult> {
        let mut conn = self.conn()?;
        conn.exec_drop(sql, to_mysql_params(params))
            .with_context(|| format!("mysql execute failed for `{sql}`"))?;
        Ok(ExecutionResult {
            rows_affected: conn.affected_rows(),
            last_insert_id: Some(conn.last_insert_id() as i64),
        })
    }

    fn query(&mut self, sql: &str, params: &[DatabaseValue]) -> Result<Vec<DatabaseRow>> {
        let mut conn = self.conn()?;
        let rows: Vec<Row> = conn
            .exec(sql, to_mysql_params(params))
            .with_context(|| format!("mysql query failed for `{sql}`"))?;
        Ok(rows.into_iter().map(from_mysql_row).collect())
    }

    fn begin_transaction(&mut self) -> Result<()> {
        {
            let mut conn = self.conn()?;
            conn.query_drop("START TRANSACTION")
                .context("failed to begin mysql transaction")?;
        }
        self.in_transaction = true;
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        {
            let mut conn = self.conn()?;
            conn.query_drop("COMMIT")
                .context("failed to commit mysql transaction")?;
        }
        self.in_transaction = false;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        {
            let mut conn = self.conn()?;
            conn.query_drop("ROLLBACK")
                .context("failed to rollback mysql transaction")?;
        }
        self.in_transaction = false;
        Ok(())
    }

    fn health_check(&mut self) -> Result<DatabaseHealth> {
        let mut conn = self.conn()?;
        let result: Option<u8> = conn.query_first("SELECT 1").context("mysql ping failed")?;
        Ok(DatabaseHealth {
            healthy: result == Some(1),
            message: "ok".to_string(),
        })
    }

    fn inspect_tables(&mut self) -> Result<Vec<String>> {
        let mut conn = self.conn()?;
        let tables: Vec<String> = conn
            .query("SHOW TABLES")
            .context("failed to inspect mysql tables")?;
        Ok(tables)
    }

    fn acquire_migration_lock(&mut self, lock_name: &str) -> Result<()> {
        let mut conn = self.conn()?;
        let result: Option<u8> = conn
            .exec_first("SELECT GET_LOCK(?, 0)", (lock_name,))
            .context("failed to acquire mysql migration lock")?;
        if result != Some(1) {
            bail!("E6110 Another migration process is currently active");
        }
        Ok(())
    }

    fn release_migration_lock(&mut self, lock_name: &str) -> Result<()> {
        let mut conn = self.conn()?;
        let _ = conn.exec_first::<u8, _, _>("SELECT RELEASE_LOCK(?)", (lock_name,));
        Ok(())
    }

    fn capabilities(&self) -> Vec<DatabaseCapability> {
        vec![
            DatabaseCapability::Transactions,
            DatabaseCapability::PreparedStatements,
            DatabaseCapability::SchemaInspection,
            DatabaseCapability::MigrationLocking,
            DatabaseCapability::ForeignKeys,
        ]
    }

    fn driver_kind(&self) -> DatabaseDriverKind {
        DatabaseDriverKind::MySql
    }
}

fn to_mysql_params(params: &[DatabaseValue]) -> Vec<Value> {
    params
        .iter()
        .map(|value| match value {
            DatabaseValue::Null => Value::NULL,
            DatabaseValue::Boolean(value) => Value::Int(i64::from(*value)),
            DatabaseValue::Integer(value) => Value::Int(*value),
            DatabaseValue::Float(value) => Value::Double(*value),
            DatabaseValue::Decimal(value)
            | DatabaseValue::Text(value)
            | DatabaseValue::DateTime(value)
            | DatabaseValue::Json(value) => Value::Bytes(value.as_bytes().to_vec()),
            DatabaseValue::Binary(value) => Value::Bytes(value.clone()),
        })
        .collect()
}

fn from_mysql_row(row: Row) -> DatabaseRow {
    let columns = row
        .columns_ref()
        .iter()
        .map(|column| column.name_str().to_string())
        .collect::<Vec<_>>();
    let values = row.unwrap();
    let mut out = DatabaseRow::new();
    for (name, value) in columns.into_iter().zip(values.into_iter()) {
        let mapped = match value {
            Value::NULL => DatabaseValue::Null,
            Value::Bytes(bytes) => DatabaseValue::Text(String::from_utf8_lossy(&bytes).to_string()),
            Value::Int(value) => DatabaseValue::Integer(value),
            Value::UInt(value) => DatabaseValue::Integer(value as i64),
            Value::Float(value) => DatabaseValue::Float(value.into()),
            Value::Double(value) => DatabaseValue::Float(value),
            Value::Date(year, month, day, hour, minute, second, micros) => {
                DatabaseValue::DateTime(format!(
                    "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{:06}",
                    micros
                ))
            }
            Value::Time(_, days, hours, minutes, seconds, micros) => DatabaseValue::Text(format!(
                "{days}:{hours:02}:{minutes:02}:{seconds:02}.{:06}",
                micros
            )),
        };
        out.insert(name, mapped);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_connection_instantiation() {
        let err =
            MySqlConnection::new("127.0.0.1", 3306, "app_db", "root", "bad-password").unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}
