use agilang_database_identity::{ApplicationIdentity, DatabaseIdentity};
use agilang_database_security_kernel::Capability;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const MAXIMUM_FRAME_BYTES: usize = 16 * 1024 * 1024; // 16 MB

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgtpTransportFrame {
    pub frame_type: u8,
    pub payload: Vec<u8>,
}

impl AgtpTransportFrame {
    pub fn new(frame_type: u8, payload: Vec<u8>) -> Result<Self> {
        if payload.len() > MAXIMUM_FRAME_BYTES {
            bail!(
                "E6603 Oversized transport frame: payload size {} > {}",
                payload.len(),
                MAXIMUM_FRAME_BYTES
            );
        }
        Ok(Self {
            frame_type,
            payload,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeChallenge {
    pub challenge_nonce: [u8; 32],
    pub database_identity: DatabaseIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportSessionInfo {
    pub session_id: [u8; 16],
    pub application_name: String,
    pub database_name: String,
    pub capabilities: Vec<Capability>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub peer_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportStatusSnapshot {
    pub listener_addr: String,
    pub air_gapped: bool,
    pub active_sessions: Vec<TransportSessionInfo>,
    pub known_peers: Vec<String>,
    pub started_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportRequest {
    Status,
    HandshakeInit {
        client_identity: ApplicationIdentity,
        requested_capabilities: Vec<Capability>,
    },
    HandshakeFinish {
        client_identity: ApplicationIdentity,
        requested_capabilities: Vec<Capability>,
        challenge_nonce: [u8; 32],
        client_signature: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportResponse {
    Status {
        snapshot: TransportStatusSnapshot,
    },
    HandshakeChallenge {
        challenge: HandshakeChallenge,
    },
    HandshakeComplete {
        session: TransportSessionInfo,
        server_signature: Vec<u8>,
    },
    Error {
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_frame_size_limit() {
        let valid = vec![0u8; 100];
        assert!(AgtpTransportFrame::new(1, valid).is_ok());

        let invalid = vec![0u8; MAXIMUM_FRAME_BYTES + 10];
        let err = AgtpTransportFrame::new(1, invalid);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6603"));
    }
}
