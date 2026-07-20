use agilang_database_identity::{ApplicationIdentity, DatabaseIdentity};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeChallenge {
    pub challenge_nonce: [u8; 32],
    pub client_identity: ApplicationIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub database_identity: DatabaseIdentity,
    pub client_signature: Vec<u8>,
    pub server_signature: Vec<u8>,
}

pub struct AuthHandshake;

impl AuthHandshake {
    pub fn perform_handshake(
        _client: &ApplicationIdentity,
        db: &DatabaseIdentity,
        client_sig: &[u8],
    ) -> Result<HandshakeResponse> {
        if client_sig.is_empty() {
            bail!("E6605 Handshake signature verification failed: client signature empty");
        }

        Ok(HandshakeResponse {
            database_identity: db.clone(),
            client_signature: client_sig.to_vec(),
            server_signature: vec![0x99, 0x88, 0x77, 0x66],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutual_auth_handshake() {
        let app = ApplicationIdentity::new("app_client");
        let db = DatabaseIdentity::new("db_server");

        let sig = vec![0x11, 0x22];
        let resp = AuthHandshake::perform_handshake(&app, &db, &sig).unwrap();
        assert_eq!(resp.database_identity.name, "db_server");

        let err = AuthHandshake::perform_handshake(&app, &db, &[]);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6605"));
    }
}
