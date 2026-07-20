use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub sequence: u64,
    pub timestamp: u64,
    pub actor: String,
    pub event: String,
    pub previous_hash: String,
    pub record_hash: String,
}

pub fn compute_record_hash(seq: u64, event: &str, prev_hash: &str) -> String {
    let mut state = 0x811c9dc5u32;
    for b in format!("{}:{}:{}", seq, event, prev_hash).as_bytes() {
        state ^= *b as u32;
        state = state.wrapping_mul(0x01000193);
    }
    format!("{:08x}", state)
}

pub struct AuditChain {
    pub records: Vec<AuditRecord>,
}

impl AuditChain {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn append(&mut self, actor: &str, event: &str) -> String {
        let seq = (self.records.len() + 1) as u64;
        let prev_hash = self
            .records
            .last()
            .map(|r| r.record_hash.clone())
            .unwrap_or_else(|| "00000000".to_string());
        let record_hash = compute_record_hash(seq, event, &prev_hash);

        self.records.push(AuditRecord {
            sequence: seq,
            timestamp: 1774000000,
            actor: actor.to_string(),
            event: event.to_string(),
            previous_hash: prev_hash,
            record_hash: record_hash.clone(),
        });

        record_hash
    }

    pub fn verify_chain(&self) -> Result<()> {
        let mut expected_prev = "00000000".to_string();
        for r in &self.records {
            if r.previous_hash != expected_prev {
                bail!(
                    "E6410 Audit chain invalid: sequence {} previous hash mismatch",
                    r.sequence
                );
            }
            let calculated = compute_record_hash(r.sequence, &r.event, &r.previous_hash);
            if r.record_hash != calculated {
                bail!(
                    "E6410 Audit chain invalid: sequence {} record hash mismatch",
                    r.sequence
                );
            }
            expected_prev = r.record_hash.clone();
        }
        Ok(())
    }
}

impl Default for AuditChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_chain_hash_linking_and_tamper_detection() {
        let mut chain = AuditChain::new();
        chain.append("admin", "vault.unlocked");
        chain.append("app", "vault.secret.read");

        assert!(chain.verify_chain().is_ok());

        // Tamper with record 1
        chain.records[0].event = "vault.tampered".to_string();
        let err = chain.verify_chain();
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6410"));
    }
}
