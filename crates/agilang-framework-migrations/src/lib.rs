use agilang_database_driver::{DatabaseConnection, DatabaseValue};
use agilang_framework_database::{DatabaseConfig, FrameworkConnection};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const MIGRATIONS_TABLE: &str = "agilang_migrations";
const MIGRATION_LOCK_NAME: &str = "agilang_migrations";
const FRAMEWORK_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub id: i64,
    pub migration: String,
    pub batch: i32,
    pub checksum: String,
    pub applied_at: String,
    pub execution_duration_ms: i64,
    pub framework_version: String,
}

#[derive(Debug, Clone)]
pub struct MigrationFile {
    pub name: String,
    pub up_sql_statements: Vec<String>,
    pub down_sql_statements: Vec<String>,
    pub checksum: String,
}

impl MigrationFile {
    pub fn new(
        name: impl Into<String>,
        up_sql_statements: Vec<String>,
        down_sql_statements: Vec<String>,
        source: &str,
    ) -> Self {
        Self {
            name: name.into(),
            up_sql_statements,
            down_sql_statements,
            checksum: compute_checksum(source),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationAction {
    Applied,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct MigrationOutcome {
    pub migration: String,
    pub action: MigrationAction,
    pub batch: Option<i32>,
}

pub fn compute_checksum(content: &str) -> String {
    let mut state = 0x811c9dc5u32;
    for b in content.as_bytes() {
        state ^= *b as u32;
        state = state.wrapping_mul(0x01000193);
    }
    format!("{:08x}", state)
}

pub struct MigrationExecutor {
    conn: FrameworkConnection,
}

impl MigrationExecutor {
    pub fn connect(config: &DatabaseConfig) -> Result<Self> {
        Ok(Self {
            conn: FrameworkConnection::connect(config)?,
        })
    }

    pub fn run_migrations(
        &mut self,
        files: &[MigrationFile],
        pretend: bool,
    ) -> Result<Vec<MigrationOutcome>> {
        self.ensure_repository()?;
        self.conn.acquire_migration_lock(MIGRATION_LOCK_NAME)?;
        let result = self.run_migrations_locked(files, pretend);
        let _ = self.conn.release_migration_lock(MIGRATION_LOCK_NAME);
        result
    }

    pub fn rollback_latest(
        &mut self,
        files: &[MigrationFile],
        pretend: bool,
    ) -> Result<Vec<String>> {
        self.ensure_repository()?;
        self.conn.acquire_migration_lock(MIGRATION_LOCK_NAME)?;
        let result = self.rollback_latest_locked(files, pretend);
        let _ = self.conn.release_migration_lock(MIGRATION_LOCK_NAME);
        result
    }

    fn rollback_latest_locked(
        &mut self,
        files: &[MigrationFile],
        pretend: bool,
    ) -> Result<Vec<String>> {
        let latest = self.latest_batch()?;
        if latest == 0 {
            return Ok(Vec::new());
        }
        let records = self.records_for_batch(latest)?;
        let file_map = files
            .iter()
            .map(|file| (file.name.clone(), file.clone()))
            .collect::<HashMap<_, _>>();
        let mut rolled = Vec::new();
        for record in records.into_iter().rev() {
            let Some(file) = file_map.get(&record.migration) else {
                bail!(
                    "E6108 Cannot rollback `{}` because its migration definition is unavailable",
                    record.migration
                );
            };
            if record.checksum != file.checksum {
                bail!(
                    "E6104 Migration checksum mismatch for `{}`: applied checksum {}, current {}",
                    record.migration,
                    record.checksum,
                    file.checksum
                );
            }
            if pretend {
                rolled.push(record.migration);
                continue;
            }
            self.conn.begin_transaction()?;
            let execution = (|| -> Result<()> {
                for sql in file.down_sql_statements.iter().rev() {
                    self.conn.execute(sql, &[])?;
                }
                self.conn.execute(
                    &format!("DELETE FROM {MIGRATIONS_TABLE} WHERE migration = ?"),
                    &[DatabaseValue::Text(record.migration.clone())],
                )?;
                Ok(())
            })();
            match execution {
                Ok(()) => {
                    self.conn.commit()?;
                    rolled.push(record.migration);
                }
                Err(err) => {
                    let _ = self.conn.rollback();
                    return Err(err);
                }
            }
        }
        Ok(rolled)
    }

    fn run_migrations_locked(
        &mut self,
        files: &[MigrationFile],
        pretend: bool,
    ) -> Result<Vec<MigrationOutcome>> {
        let applied = self.applied_index()?;
        let next_batch = self.latest_batch()? + 1;
        let mut ordered = files.to_vec();
        ordered.sort_by(|left, right| left.name.cmp(&right.name));
        let mut outcomes = Vec::new();
        for file in ordered {
            if let Some(record) = applied.get(&file.name) {
                if record.checksum != file.checksum {
                    bail!(
                        "E6104 Migration checksum mismatch for `{}`: applied checksum {}, current {}",
                        file.name,
                        record.checksum,
                        file.checksum
                    );
                }
                outcomes.push(MigrationOutcome {
                    migration: file.name,
                    action: MigrationAction::Skipped,
                    batch: Some(record.batch),
                });
                continue;
            }
            if pretend {
                outcomes.push(MigrationOutcome {
                    migration: file.name,
                    action: MigrationAction::Applied,
                    batch: Some(next_batch),
                });
                continue;
            }
            let started = Instant::now();
            self.conn.begin_transaction()?;
            let execution = (|| -> Result<()> {
                for sql in &file.up_sql_statements {
                    self.conn.execute(sql, &[])?;
                }
                self.insert_record(&file, next_batch, started.elapsed().as_millis() as i64)?;
                Ok(())
            })();
            match execution {
                Ok(()) => {
                    self.conn.commit()?;
                    outcomes.push(MigrationOutcome {
                        migration: file.name,
                        action: MigrationAction::Applied,
                        batch: Some(next_batch),
                    });
                }
                Err(err) => {
                    let _ = self.conn.rollback();
                    return Err(err);
                }
            }
        }
        Ok(outcomes)
    }

    pub fn status(&mut self) -> Result<Vec<MigrationRecord>> {
        self.ensure_repository()?;
        self.all_records()
    }

    pub fn reset(&mut self, files: &[MigrationFile], pretend: bool) -> Result<Vec<String>> {
        let mut rolled = Vec::new();
        while self.latest_batch()? > 0 {
            let batch = self.rollback_latest(files, pretend)?;
            if batch.is_empty() {
                break;
            }
            rolled.extend(batch);
            if pretend {
                break;
            }
        }
        Ok(rolled)
    }

    pub fn refresh(
        &mut self,
        files: &[MigrationFile],
        pretend: bool,
    ) -> Result<Vec<MigrationOutcome>> {
        let _ = self.reset(files, pretend)?;
        self.run_migrations(files, pretend)
    }

    pub fn fresh(
        &mut self,
        files: &[MigrationFile],
        pretend: bool,
        environment: &str,
        allow_destructive: bool,
    ) -> Result<Vec<MigrationOutcome>> {
        if environment.eq_ignore_ascii_case("production") && !allow_destructive {
            bail!("E6112 migrate:fresh is refused in production without explicit authorization");
        }
        self.ensure_repository()?;
        self.conn.acquire_migration_lock(MIGRATION_LOCK_NAME)?;
        let result = (|| -> Result<Vec<MigrationOutcome>> {
            if !pretend {
                let tables = self.conn.inspect_tables()?;
                for table in tables {
                    if table != "sqlite_sequence" {
                        self.conn
                            .execute(&format!("DROP TABLE IF EXISTS {table}"), &[])
                            .with_context(|| {
                                format!("failed to drop table `{table}` during fresh")
                            })?;
                    }
                }
            }
            self.run_migrations_locked(files, pretend)
        })();
        let _ = self.conn.release_migration_lock(MIGRATION_LOCK_NAME);
        result
    }

    fn ensure_repository(&mut self) -> Result<()> {
        self.conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {MIGRATIONS_TABLE} (id INTEGER, migration TEXT, batch INTEGER, checksum TEXT, applied_at TEXT, execution_duration_ms INTEGER, framework_version TEXT)"
            ),
            &[],
        )?;
        Ok(())
    }

    fn insert_record(
        &mut self,
        file: &MigrationFile,
        batch: i32,
        execution_duration_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO {MIGRATIONS_TABLE} (migration, batch, checksum, applied_at, execution_duration_ms, framework_version) VALUES (?, ?, ?, ?, ?, ?)"
            ),
            &[
                DatabaseValue::Text(file.name.clone()),
                DatabaseValue::Integer(batch as i64),
                DatabaseValue::Text(file.checksum.clone()),
                DatabaseValue::Text(now_timestamp()),
                DatabaseValue::Integer(execution_duration_ms),
                DatabaseValue::Text(FRAMEWORK_VERSION.to_string()),
            ],
        )?;
        Ok(())
    }

    fn all_records(&mut self) -> Result<Vec<MigrationRecord>> {
        let mut rows = self
            .conn
            .query(&format!("SELECT * FROM {MIGRATIONS_TABLE}"), &[])?
            .into_iter()
            .map(row_to_record)
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            left.batch
                .cmp(&right.batch)
                .then_with(|| left.migration.cmp(&right.migration))
        });
        Ok(rows)
    }

    fn records_for_batch(&mut self, batch: i32) -> Result<Vec<MigrationRecord>> {
        let mut rows = self
            .conn
            .query(
                &format!("SELECT * FROM {MIGRATIONS_TABLE} WHERE batch = ?"),
                &[DatabaseValue::Integer(batch as i64)],
            )?
            .into_iter()
            .map(row_to_record)
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.migration.cmp(&right.migration));
        Ok(rows)
    }

    fn applied_index(&mut self) -> Result<HashMap<String, MigrationRecord>> {
        Ok(self
            .all_records()?
            .into_iter()
            .map(|record| (record.migration.clone(), record))
            .collect())
    }

    fn latest_batch(&mut self) -> Result<i32> {
        Ok(self
            .all_records()?
            .iter()
            .map(|record| record.batch)
            .max()
            .unwrap_or(0))
    }
}

fn row_to_record(row: agilang_database_driver::DatabaseRow) -> MigrationRecord {
    MigrationRecord {
        id: extract_i64(&row, "id"),
        migration: extract_text(&row, "migration"),
        batch: extract_i64(&row, "batch") as i32,
        checksum: extract_text(&row, "checksum"),
        applied_at: extract_text(&row, "applied_at"),
        execution_duration_ms: extract_i64(&row, "execution_duration_ms"),
        framework_version: extract_text(&row, "framework_version"),
    }
}

fn extract_text(row: &agilang_database_driver::DatabaseRow, key: &str) -> String {
    match row.get(key) {
        Some(DatabaseValue::Text(value))
        | Some(DatabaseValue::DateTime(value))
        | Some(DatabaseValue::Decimal(value))
        | Some(DatabaseValue::Json(value)) => value.clone(),
        Some(DatabaseValue::Integer(value)) => value.to_string(),
        Some(DatabaseValue::Float(value)) => value.to_string(),
        Some(DatabaseValue::Boolean(value)) => value.to_string(),
        Some(DatabaseValue::Binary(value)) => String::from_utf8_lossy(value).to_string(),
        _ => String::new(),
    }
}

fn extract_i64(row: &agilang_database_driver::DatabaseRow, key: &str) -> i64 {
    match row.get(key) {
        Some(DatabaseValue::Integer(value)) => *value,
        Some(DatabaseValue::Text(value)) => value.parse().unwrap_or_default(),
        _ => 0,
    }
}

fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_database_driver::DatabaseConnection;
    use agilang_framework_database::FrameworkConnection;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_suffix(name: &str) -> String {
        format!(
            "{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    fn agidb_config(name: &str) -> DatabaseConfig {
        DatabaseConfig::agidb(
            std::env::temp_dir()
                .join(format!("agilang-migrations-{}.agidb", unique_suffix(name)))
                .to_string_lossy()
                .to_string(),
        )
    }

    fn sqlite_config(name: &str) -> DatabaseConfig {
        DatabaseConfig::sqlite(
            std::env::temp_dir()
                .join(format!("agilang-migrations-{}.sqlite", unique_suffix(name)))
                .to_string_lossy()
                .to_string(),
        )
    }

    fn sample_files() -> Vec<MigrationFile> {
        vec![MigrationFile::new(
            "20260801_create_users_table",
            vec!["CREATE TABLE users (id INTEGER, name TEXT)".to_string()],
            vec!["DROP TABLE IF EXISTS users".to_string()],
            "create users",
        )]
    }

    fn mysql_config() -> Option<DatabaseConfig> {
        let host = std::env::var("AGILANG_TEST_MYSQL_HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("AGILANG_TEST_MYSQL_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3306);
        let database = std::env::var("AGILANG_TEST_MYSQL_DATABASE")
            .unwrap_or_else(|_| "agilang_tranche42_test".to_string());
        let username = std::env::var("AGILANG_TEST_MYSQL_USER")
            .unwrap_or_else(|_| "root".to_string());
        let password = std::env::var("AGILANG_TEST_MYSQL_PASSWORD").unwrap_or_default();
        let config = DatabaseConfig::mysql(host, port, database, username, password);

        let mut conn = match FrameworkConnection::connect(&config) {
            Ok(conn) => conn,
            Err(_) => return None,
        };
        match conn.health_check() {
            Ok(health) if health.healthy => Some(config),
            _ => None,
        }
    }

    fn cleanup_mysql_tables(config: &DatabaseConfig) {
        let Ok(mut conn) = FrameworkConnection::connect(config) else {
            return;
        };
        let _ = conn.execute("DROP TABLE IF EXISTS users", &[]);
        let _ = conn.execute("DROP TABLE IF EXISTS agilang_migrations", &[]);
        let _ = conn.execute("DROP TABLE IF EXISTS agilang_migration_locks", &[]);
    }

    #[test]
    fn agidb_persists_status_and_rollback() {
        let config = agidb_config("agidb-status");
        let files = sample_files();
        let mut executor = MigrationExecutor::connect(&config).unwrap();
        let applied = executor.run_migrations(&files, false).unwrap();
        assert_eq!(applied.len(), 1);
        let status = executor.status().unwrap();
        assert_eq!(status.len(), 1);

        let mut reopened = MigrationExecutor::connect(&config).unwrap();
        assert_eq!(reopened.status().unwrap().len(), 1);
        let rolled = reopened.rollback_latest(&files, false).unwrap();
        assert_eq!(rolled, vec!["20260801_create_users_table".to_string()]);
        assert!(reopened.status().unwrap().is_empty());
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let config = agidb_config("checksum");
        let files = sample_files();
        let mut executor = MigrationExecutor::connect(&config).unwrap();
        executor.run_migrations(&files, false).unwrap();
        let drifted = vec![MigrationFile {
            checksum: "deadbeef".to_string(),
            ..files[0].clone()
        }];
        let err = executor.run_migrations(&drifted, false).unwrap_err();
        assert!(err.to_string().contains("E6104"));
    }

    #[test]
    fn concurrent_attempts_are_locked() {
        let config = sqlite_config("lock");
        let files = sample_files();
        let config2 = config.clone();
        let files2 = files.clone();
        let handle = thread::spawn(move || {
            let mut executor = MigrationExecutor::connect(&config2).unwrap();
            executor.conn.acquire_migration_lock(MIGRATION_LOCK_NAME).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(300));
            executor
                .conn
                .release_migration_lock(MIGRATION_LOCK_NAME)
                .unwrap();
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut executor = MigrationExecutor::connect(&config).unwrap();
        let err = executor.run_migrations(&files2, false).unwrap_err();
        assert!(err.to_string().contains("E6110"));
        handle.join().unwrap();
    }

    #[test]
    fn failed_migration_does_not_record_success() {
        let config = sqlite_config("failed");
        let files = vec![MigrationFile::new(
            "20260801_broken",
            vec![
                "CREATE TABLE posts (id INTEGER, name TEXT)".to_string(),
                "THIS IS INVALID SQL".to_string(),
            ],
            vec!["DROP TABLE IF EXISTS posts".to_string()],
            "broken migration",
        )];
        let mut executor = MigrationExecutor::connect(&config).unwrap();
        assert!(executor.run_migrations(&files, false).is_err());
        assert!(executor.status().unwrap().is_empty());
    }

    #[test]
    fn sqlite_suite_passes() {
        let config = sqlite_config("sqlite-suite");
        let files = sample_files();
        let mut executor = MigrationExecutor::connect(&config).unwrap();
        executor.run_migrations(&files, false).unwrap();
        let status = executor.status().unwrap();
        assert_eq!(status.len(), 1);
        let mut reopened = MigrationExecutor::connect(&config).unwrap();
        assert_eq!(reopened.status().unwrap().len(), 1);
    }

    #[test]
    fn mysql_live_suite_passes_against_real_server() {
        let Some(config) = mysql_config() else {
            eprintln!("skipping mysql live suite: real mysql server unavailable");
            return;
        };
        cleanup_mysql_tables(&config);

        let files = sample_files();
        let mut executor = MigrationExecutor::connect(&config).unwrap();
        let applied = executor.run_migrations(&files, false).unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(executor.status().unwrap().len(), 1);

        let drifted = vec![MigrationFile {
            checksum: "deadbeef".to_string(),
            ..files[0].clone()
        }];
        let err = executor.run_migrations(&drifted, false).unwrap_err();
        assert!(err.to_string().contains("E6104"));

        let mut reopened = MigrationExecutor::connect(&config).unwrap();
        assert_eq!(reopened.status().unwrap().len(), 1);
        let rolled = reopened.rollback_latest(&files, false).unwrap();
        assert_eq!(rolled, vec!["20260801_create_users_table".to_string()]);
        assert!(reopened.status().unwrap().is_empty());

        cleanup_mysql_tables(&config);
    }
}
