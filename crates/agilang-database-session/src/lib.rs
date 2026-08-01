use agilang_database_security_kernel::Capability;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgtpSession {
    pub session_id: [u8; 16],
    pub application_id: [u8; 16],
    pub database_id: [u8; 16],
    pub capabilities: Vec<Capability>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub next_sequence: u64,
    pub seen_nonces: HashSet<[u8; 32]>,
}

impl AgtpSession {
    pub fn new(app_id: [u8; 16], db_id: [u8; 16], capabilities: Vec<Capability>) -> Self {
        let issued_at = unix_seconds();
        let session_seed = issued_at
            .to_le_bytes()
            .into_iter()
            .cycle()
            .take(16)
            .collect::<Vec<_>>();
        let mut session_id = [0u8; 16];
        session_id.copy_from_slice(&session_seed[..16]);
        Self {
            session_id,
            application_id: app_id,
            database_id: db_id,
            capabilities,
            issued_at,
            expires_at: issued_at + 3600,
            next_sequence: 1,
            seen_nonces: HashSet::new(),
        }
    }

    pub fn validate_request(&mut self, seq: u64, nonce: [u8; 32]) -> Result<()> {
        if seq != self.next_sequence {
            bail!(
                "E6601 Sequence mismatch: expected sequence {}, got {}",
                self.next_sequence,
                seq
            );
        }

        if self.seen_nonces.contains(&nonce) {
            bail!("E6602 Nonce reused: request replay detected");
        }

        self.seen_nonces.insert(nonce);
        self.next_sequence += 1;
        Ok(())
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_tracking_and_replay_nonce_rejection() {
        let mut session = AgtpSession::new([1u8; 16], [2u8; 16], vec![Capability::TableRead]);
        let nonce1 = [0x11u8; 32];

        assert!(session.validate_request(1, nonce1).is_ok());

        let err_seq = session.validate_request(5, [0x22u8; 32]);
        assert!(err_seq.is_err());
        assert!(err_seq.unwrap_err().to_string().contains("E6601"));

        let err_replay = session.validate_request(2, nonce1);
        assert!(err_replay.is_err());
        assert!(err_replay.unwrap_err().to_string().contains("E6602"));
    }
}
