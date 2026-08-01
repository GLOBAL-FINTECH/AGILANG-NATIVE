use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub id: i64,
    pub migration: String,
    pub batch: i32,
    pub checksum: String,
    pub executed_at: String,
}

static MIGRATION_REPO: Lazy<Mutex<Vec<MigrationRecord>>> = Lazy::new(|| Mutex::new(Vec::new()));
static MIGRATION_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

pub fn compute_checksum(content: &str) -> String {
    let mut state = 0x811c9dc5u32;
    for b in content.as_bytes() {
        state ^= *b as u32;
        state = state.wrapping_mul(0x01000193);
    }
    format!("{:08x}", state)
}

pub struct MigrationRepository;

impl MigrationRepository {
    pub fn get_applied() -> Vec<MigrationRecord> {
        let guard = MIGRATION_REPO.lock().unwrap();
        guard.clone()
    }

    pub fn is_applied(name: &str) -> bool {
        let guard = MIGRATION_REPO.lock().unwrap();
        guard.iter().any(|r| r.migration == name)
    }

    pub fn latest_batch() -> i32 {
        let guard = MIGRATION_REPO.lock().unwrap();
        guard.iter().map(|r| r.batch).max().unwrap_or(0)
    }

    pub fn record_applied(name: &str, batch: i32, checksum: &str) {
        let mut guard = MIGRATION_REPO.lock().unwrap();
        let next_id = (guard.len() + 1) as i64;
        guard.push(MigrationRecord {
            id: next_id,
            migration: name.to_string(),
            batch,
            checksum: checksum.to_string(),
            executed_at: format!("{:?}", SystemTime::now()),
        });
    }

    pub fn validate_checksum(name: &str, current_checksum: &str) -> Result<()> {
        let guard = MIGRATION_REPO.lock().unwrap();
        if let Some(record) = guard.iter().find(|r| r.migration == name) {
            if record.checksum != current_checksum {
                bail!(
                    "E6104 Migration checksum mismatch for `{}`: applied checksum {}, current {}",
                    name,
                    record.checksum,
                    current_checksum
                );
            }
        }
        Ok(())
    }

    pub fn remove_batch(batch: i32) -> Vec<MigrationRecord> {
        let mut guard = MIGRATION_REPO.lock().unwrap();
        let mut removed = Vec::new();
        guard.retain(|r| {
            if r.batch == batch {
                removed.push(r.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    pub fn clear() {
        let mut guard = MIGRATION_REPO.lock().unwrap();
        guard.clear();
    }
}

pub struct MigrationLock;

impl MigrationLock {
    pub fn acquire() -> Result<()> {
        let mut guard = MIGRATION_LOCK.lock().unwrap();
        if *guard {
            bail!("E6110 Another migration process is currently active");
        }
        *guard = true;
        Ok(())
    }

    pub fn release() {
        let mut guard = MIGRATION_LOCK.lock().unwrap();
        *guard = false;
    }
}

#[derive(Debug, Clone)]
pub struct MigrationFile {
    pub name: String,
    pub sql_statements: Vec<String>,
    pub checksum: String,
}

pub struct MigrationExecutor;

impl MigrationExecutor {
    pub fn run_migrations(files: &[MigrationFile], pretend: bool) -> Result<Vec<String>> {
        MigrationLock::acquire()?;
        let mut executed_logs = Vec::new();
        let next_batch = MigrationRepository::latest_batch() + 1;

        if pretend {
            println!("Migration plan\n");
            for file in files {
                if !MigrationRepository::is_applied(&file.name) {
                    println!("{}", file.name);
                    for stmt in &file.sql_statements {
                        println!("  {}", stmt);
                    }
                    println!();
                    executed_logs.push(file.name.clone());
                }
            }
            println!("No database changes were applied.");
            MigrationLock::release();
            return Ok(executed_logs);
        }

        for file in files {
            if let Err(e) = MigrationRepository::validate_checksum(&file.name, &file.checksum) {
                MigrationLock::release();
                return Err(e);
            }

            if !MigrationRepository::is_applied(&file.name) {
                MigrationRepository::record_applied(&file.name, next_batch, &file.checksum);
                executed_logs.push(file.name.clone());
            }
        }

        MigrationLock::release();
        Ok(executed_logs)
    }

    pub fn rollback_latest(pretend: bool) -> Result<Vec<String>> {
        MigrationLock::acquire()?;
        let latest_batch = MigrationRepository::latest_batch();
        if latest_batch == 0 {
            MigrationLock::release();
            return Ok(Vec::new());
        }

        if pretend {
            let guard = MIGRATION_REPO.lock().unwrap();
            let rolled: Vec<String> = guard
                .iter()
                .filter(|r| r.batch == latest_batch)
                .map(|r| r.migration.clone())
                .collect();
            println!("Rollback plan (Batch {})\n", latest_batch);
            for m in &rolled {
                println!("  Rollback {}", m);
            }
            println!("\nNo database changes were applied.");
            MigrationLock::release();
            return Ok(rolled);
        }

        let removed = MigrationRepository::remove_batch(latest_batch);
        let rolled_names = removed.into_iter().map(|r| r.migration).collect();
        MigrationLock::release();
        Ok(rolled_names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_checksum_computation_and_drift_detection() {
        let _guard = TEST_MUTEX.lock().unwrap();
        MigrationRepository::clear();
        let content = "schema.create('users')";
        let checksum = compute_checksum(content);
        assert_eq!(checksum.len(), 8);

        MigrationRepository::record_applied("20260720_001_users", 1, &checksum);
        assert!(MigrationRepository::validate_checksum("20260720_001_users", &checksum).is_ok());

        let drifted_err =
            MigrationRepository::validate_checksum("20260720_001_users", "drifted123");
        assert!(drifted_err.is_err());
        assert!(drifted_err.unwrap_err().to_string().contains("E6104"));
    }

    #[test]
    fn test_pretend_mode_output() {
        let _guard = TEST_MUTEX.lock().unwrap();
        MigrationRepository::clear();
        let file = MigrationFile {
            name: "20260720_002_posts".to_string(),
            sql_statements: vec!["CREATE TABLE \"posts\" (id INT);".to_string()],
            checksum: "abc12345".to_string(),
        };

        let logs = MigrationExecutor::run_migrations(&[file], true).unwrap();
        assert_eq!(logs.len(), 1);
        assert!(!MigrationRepository::is_applied("20260720_002_posts"));
    }
}
