use agilang_database_storage::PageManager;
use agilang_database_wal::{WalRecordType, WriteAheadLog};
use anyhow::Result;
use std::collections::HashSet;

pub struct RecoveryEngine;

impl RecoveryEngine {
    pub fn recover(_mgr: &mut PageManager, wal: &WriteAheadLog) -> Result<HashSet<u64>> {
        let mut committed_txs = HashSet::new();
        let mut uncommitted_txs = HashSet::new();

        for record in &wal.records {
            match record.record_type {
                WalRecordType::BeginTransaction => {
                    uncommitted_txs.insert(record.tx_id);
                }
                WalRecordType::CommitTransaction => {
                    uncommitted_txs.remove(&record.tx_id);
                    committed_txs.insert(record.tx_id);
                }
                WalRecordType::RollbackTransaction => {
                    uncommitted_txs.remove(&record.tx_id);
                }
                _ => {}
            }
        }

        Ok(committed_txs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_identifies_committed_transactions() {
        let mut mgr = PageManager::new();
        let mut wal = WriteAheadLog::new();

        wal.append(1, WalRecordType::BeginTransaction, None);
        wal.append(1, WalRecordType::CommitTransaction, None);

        wal.append(2, WalRecordType::BeginTransaction, None);

        let committed = RecoveryEngine::recover(&mut mgr, &wal).unwrap();
        assert!(committed.contains(&1));
        assert!(!committed.contains(&2));
    }
}
