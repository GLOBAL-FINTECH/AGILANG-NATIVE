use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalRecordType {
    BeginTransaction,
    PageBeforeImage,
    PageAfterImage,
    CommitTransaction,
    RollbackTransaction,
    Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalRecord {
    pub lsn: u64,
    pub tx_id: u64,
    pub record_type: WalRecordType,
    pub page_id: Option<u64>,
    pub checksum: u32,
}

#[derive(Debug)]
pub struct WriteAheadLog {
    pub current_lsn: u64,
    pub records: Vec<WalRecord>,
}

impl WriteAheadLog {
    pub fn new() -> Self {
        Self {
            current_lsn: 1,
            records: Vec::new(),
        }
    }

    pub fn append(&mut self, tx_id: u64, record_type: WalRecordType, page_id: Option<u64>) -> u64 {
        let lsn = self.current_lsn;
        self.current_lsn += 1;

        let record = WalRecord {
            lsn,
            tx_id,
            record_type,
            page_id,
            checksum: (lsn as u32) ^ 0xabcdef,
        };

        self.records.push(record);
        lsn
    }
}

impl Default for WriteAheadLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_append_sequence() {
        let mut wal = WriteAheadLog::new();
        let lsn1 = wal.append(1, WalRecordType::BeginTransaction, None);
        let lsn2 = wal.append(1, WalRecordType::CommitTransaction, None);

        assert_eq!(lsn1, 1);
        assert_eq!(lsn2, 2);
        assert_eq!(wal.records.len(), 2);
    }
}
