use agilang_database_wal::{WalRecordType, WriteAheadLog};
use anyhow::Result;

pub struct GroupCommitEngine {
    pub wal: WriteAheadLog,
    pub pending_txs: Vec<u64>,
    pub max_batch_size: usize,
}

impl GroupCommitEngine {
    pub fn new(max_batch_size: usize) -> Self {
        Self {
            wal: WriteAheadLog::new(),
            pending_txs: Vec::new(),
            max_batch_size,
        }
    }

    pub fn queue_commit(&mut self, tx_id: u64) -> Result<Option<u64>> {
        self.pending_txs.push(tx_id);

        if self.pending_txs.len() >= self.max_batch_size {
            let flushed_lsn = self.flush_batch()?;
            Ok(Some(flushed_lsn))
        } else {
            Ok(None)
        }
    }

    pub fn flush_batch(&mut self) -> Result<u64> {
        let mut last_lsn = 0;
        for tx_id in self.pending_txs.drain(..) {
            last_lsn = self
                .wal
                .append(tx_id, WalRecordType::CommitTransaction, None);
        }
        Ok(last_lsn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_commit_batch_flushing() {
        let mut engine = GroupCommitEngine::new(2);

        // First queue doesn't trigger batch flush
        assert_eq!(engine.queue_commit(1).unwrap(), None);

        // Second queue hits batch size 2, triggering flush
        let flushed = engine.queue_commit(2).unwrap();
        assert!(flushed.is_some());
        assert_eq!(engine.pending_txs.len(), 0);
    }
}
