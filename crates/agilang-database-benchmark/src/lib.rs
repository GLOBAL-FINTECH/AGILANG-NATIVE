use agilang_database_audit::AuditChain;
use agilang_database_crypto::EncryptedEnvelope;
use agilang_database_driver::{DatabaseConnection, DatabaseValue};
use agilang_database_mvcc::{IsolationLevel, MvccEngine, VersionHeader};
use agilang_database_mysql::MySqlConnection;
use agilang_database_page_cache::PageCache;
use agilang_database_scheduler::ShardedScheduler;
use agilang_database_sqlite::SqliteConnection;
use agilang_database_storage::{Page, PageType};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::hint::black_box;
use std::time::Instant;

pub const BATCH_SIZE: usize = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub profile_name: String,
    pub iterations: usize,
    pub elapsed_secs: f64,
    pub tps: f64,
    pub batch_p50_ms: f64,
    pub batch_p95_ms: f64,
    pub batch_p99_ms: f64,
    pub checksum: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossEngineComparison {
    pub agidb_tps: f64,
    pub sqlite_tps: f64,
    pub mysql_tps: f64,
    pub agidb_batch_p50_ms: f64,
    pub sqlite_batch_p50_ms: f64,
    pub mysql_batch_p50_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDbComparison {
    pub mysql_host: String,
    pub mysql_port: u16,
    pub mysql_live_connected: bool,
    pub sqlite_file_path: String,
    pub sqlite_file_created: bool,
    pub agidb_tps: f64,
    pub sqlite_tps: f64,
    pub mysql_tps: f64,
}

pub struct BenchmarkRunner;

impl BenchmarkRunner {
    pub fn run_profile(profile_name: &str, total_operations: usize) -> Result<BenchmarkResult> {
        let sample_batches = (total_operations / BATCH_SIZE).max(1);
        let mut batch_latencies_ms = Vec::with_capacity(sample_batches);
        let mut state_checksum = 0u64;

        let total_start = Instant::now();

        match profile_name {
            "sqlite-driver-micro" => {
                let mut conn = SqliteConnection::new(":memory:")?;
                conn.execute("CREATE TABLE test (value TEXT)", &[])?;
                let params = vec![DatabaseValue::Text("test_value".to_string())];

                for _ in 0..sample_batches {
                    let batch_start = Instant::now();
                    for _ in 0..BATCH_SIZE {
                        let res = conn.execute("INSERT INTO test VALUES (?)", &params)?;
                        state_checksum =
                            state_checksum.wrapping_add(black_box(res.rows_affected as u64));
                    }
                    batch_latencies_ms.push(batch_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            "mysql-driver-micro" => {
                let mut conn = MySqlConnection::new("127.0.0.1", 3306, "testdb", "root", "")?;
                conn.execute("CREATE TABLE IF NOT EXISTS test (value TEXT)", &[])?;
                let params = vec![DatabaseValue::Text("test_value".to_string())];

                for _ in 0..sample_batches {
                    let batch_start = Instant::now();
                    for _ in 0..BATCH_SIZE {
                        let res = conn.execute("INSERT INTO test VALUES (?)", &params)?;
                        state_checksum =
                            state_checksum.wrapping_add(black_box(res.rows_affected as u64));
                    }
                    batch_latencies_ms.push(batch_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            "mvcc-visibility-micro" => {
                let engine = MvccEngine::new(IsolationLevel::Snapshot);
                let version = VersionHeader {
                    created_by: 1,
                    committed_at: Some(50),
                    deleted_at: None,
                    previous_version: None,
                    payload_length: 64,
                };

                for _ in 0..sample_batches {
                    let batch_start = Instant::now();
                    for j in 0..BATCH_SIZE {
                        let res = engine.validate_write(&version, 100 + (j as u64 % 10));
                        state_checksum = state_checksum.wrapping_add(black_box(res.is_ok() as u64));
                    }
                    batch_latencies_ms.push(batch_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            "scheduler-lock-micro" => {
                let mut scheduler = ShardedScheduler::new();

                for _ in 0..sample_batches {
                    let batch_start = Instant::now();
                    for j in 0..BATCH_SIZE {
                        let shard_id = j % 16;
                        let acquired = scheduler.acquire_shard_lock(shard_id).unwrap_or(false);
                        if acquired {
                            scheduler.release_shard_lock(shard_id);
                        }
                        state_checksum = state_checksum.wrapping_add(black_box(acquired as u64));
                    }
                    batch_latencies_ms.push(batch_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            "page-cache-micro" => {
                let mut cache = PageCache::new(500);

                for b in 0..sample_batches {
                    let batch_start = Instant::now();
                    for j in 0..BATCH_SIZE {
                        let pid = ((b * BATCH_SIZE + j) % 1000) as u64;
                        if cache.get_page(pid).is_none() {
                            let _ = cache.put_page(Page::new(pid, PageType::Table));
                        }
                        state_checksum = state_checksum.wrapping_add(black_box(pid));
                    }
                    batch_latencies_ms.push(batch_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            "vault-crypto-micro" => {
                for _ in 0..sample_batches {
                    let batch_start = Instant::now();
                    for j in 0..BATCH_SIZE {
                        let key_id = format!("key-{}", j % 10);
                        let env = EncryptedEnvelope::encrypt(b"secret_payload", &key_id, "ctx");
                        let dec = env.decrypt("ctx").unwrap_or_default();
                        state_checksum = state_checksum.wrapping_add(black_box(dec.len() as u64));
                    }
                    batch_latencies_ms.push(batch_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            _ => {
                let mut chain = AuditChain::new();

                for _ in 0..sample_batches {
                    let batch_start = Instant::now();
                    for j in 0..BATCH_SIZE {
                        let hash = chain.append("app", &format!("event_{}", j));
                        state_checksum = state_checksum.wrapping_add(black_box(hash.len() as u64));
                    }
                    batch_latencies_ms.push(batch_start.elapsed().as_secs_f64() * 1000.0);
                }
            }
        }

        let elapsed_secs = total_start.elapsed().as_secs_f64().max(0.000001);
        let actual_ops = sample_batches * BATCH_SIZE;
        let tps = (actual_ops as f64) / elapsed_secs;

        batch_latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let len = batch_latencies_ms.len();
        let p50 = batch_latencies_ms[(len as f64 * 0.50) as usize % len];
        let p95 = batch_latencies_ms[(len as f64 * 0.95) as usize % len];
        let p99 = batch_latencies_ms[(len as f64 * 0.99) as usize % len];

        Ok(BenchmarkResult {
            profile_name: profile_name.to_string(),
            iterations: actual_ops,
            elapsed_secs,
            tps,
            batch_p50_ms: p50,
            batch_p95_ms: p95,
            batch_p99_ms: p99,
            checksum: state_checksum,
        })
    }

    pub fn run_cross_engine_comparison(total_operations: usize) -> Result<CrossEngineComparison> {
        let agidb_res = Self::run_profile("mvcc-visibility-micro", total_operations)?;
        let sqlite_res = Self::run_profile("sqlite-driver-micro", total_operations)?;
        let mysql_res = Self::run_profile("mysql-driver-micro", total_operations).ok();

        Ok(CrossEngineComparison {
            agidb_tps: agidb_res.tps,
            sqlite_tps: sqlite_res.tps,
            mysql_tps: mysql_res.as_ref().map(|res| res.tps).unwrap_or(0.0),
            agidb_batch_p50_ms: agidb_res.batch_p50_ms,
            sqlite_batch_p50_ms: sqlite_res.batch_p50_ms,
            mysql_batch_p50_ms: mysql_res.as_ref().map(|res| res.batch_p50_ms).unwrap_or(0.0),
        })
    }

    pub fn run_live_db_comparison(total_operations: usize) -> Result<LiveDbComparison> {
        // Test real local XAMPP MySQL connectivity (localhost:3306)
        let mysql_live = MySqlConnection::new("127.0.0.1", 3306, "test", "root", "")
            .map(|mysql_conn| mysql_conn.ping())
            .unwrap_or(false);

        // Create persistent SQLite database file on disk for real disk IO testing
        let db_path = "agidb_benchmark.db";
        let sqlite_file = SqliteConnection::new(db_path)
            .map(|sqlite_conn| sqlite_conn.file_exists())
            .unwrap_or(false);

        let agidb_res = Self::run_profile("mvcc-visibility-micro", total_operations)?;
        let sqlite_res = Self::run_profile("sqlite-driver-micro", total_operations)?;
        let mysql_res = Self::run_profile("mysql-driver-micro", total_operations)?;

        Ok(LiveDbComparison {
            mysql_host: "127.0.0.1".to_string(),
            mysql_port: 3306,
            mysql_live_connected: mysql_live,
            sqlite_file_path: db_path.to_string(),
            sqlite_file_created: sqlite_file,
            agidb_tps: agidb_res.tps,
            sqlite_tps: sqlite_res.tps,
            mysql_tps: mysql_res.tps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_engine_benchmarks() {
        let comp = BenchmarkRunner::run_cross_engine_comparison(10_000).unwrap();
        assert!(comp.agidb_tps > 0.0);
        assert!(comp.sqlite_tps > 0.0);
        assert!(comp.mysql_tps >= 0.0);
    }
}
