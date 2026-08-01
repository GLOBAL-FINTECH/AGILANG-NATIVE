use agilang_database_identity::{ApplicationIdentity, DatabaseIdentity};
use agilang_database_transport::HandshakeChallenge;
use agilang_runtime_crypto::{constant_time_eq, hmac_sha256, random_bytes};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub database_identity: DatabaseIdentity,
    pub client_signature: Vec<u8>,
    pub server_signature: Vec<u8>,
}

pub struct AuthHandshake;

impl AuthHandshake {
    pub fn issue_challenge(db: &DatabaseIdentity) -> Result<HandshakeChallenge> {
        let nonce = random_bytes(32).map_err(|err| anyhow::anyhow!(err.to_string()))?;
        let mut challenge_nonce = [0u8; 32];
        challenge_nonce.copy_from_slice(&nonce[..32]);
        Ok(HandshakeChallenge {
            challenge_nonce,
            database_identity: db.clone(),
        })
    }

    pub fn sign_client_challenge(
        client: &ApplicationIdentity,
        db: &DatabaseIdentity,
        challenge_nonce: &[u8; 32],
    ) -> Vec<u8> {
        let payload = handshake_payload(client, db, challenge_nonce);
        hmac_sha256(&client.public_key, &payload).to_vec()
    }

    pub fn perform_handshake(
        client: &ApplicationIdentity,
        db: &DatabaseIdentity,
        challenge_nonce: &[u8; 32],
        client_sig: &[u8],
    ) -> Result<HandshakeResponse> {
        let expected = Self::sign_client_challenge(client, db, challenge_nonce);
        if client_sig.is_empty() || !constant_time_eq(client_sig, &expected) {
            bail!("E6605 Handshake signature verification failed");
        }

        let server_signature = hmac_sha256(&db.public_key, client_sig).to_vec();

        Ok(HandshakeResponse {
            database_identity: db.clone(),
            client_signature: client_sig.to_vec(),
            server_signature,
        })
    }
}

fn handshake_payload(
    client: &ApplicationIdentity,
    db: &DatabaseIdentity,
    challenge_nonce: &[u8; 32],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        client.application_id.len()
            + db.database_id.len()
            + client.name.len()
            + db.name.len()
            + challenge_nonce.len(),
    );
    payload.extend_from_slice(&client.application_id);
    payload.extend_from_slice(&db.database_id);
    payload.extend_from_slice(client.name.as_bytes());
    payload.extend_from_slice(db.name.as_bytes());
    payload.extend_from_slice(challenge_nonce);
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutual_auth_handshake() {
        let app = ApplicationIdentity::new("app_client");
        let db = DatabaseIdentity::new("db_server");
        let challenge = AuthHandshake::issue_challenge(&db).unwrap();

        let sig = AuthHandshake::sign_client_challenge(&app, &db, &challenge.challenge_nonce);
        let resp = AuthHandshake::perform_handshake(&app, &db, &challenge.challenge_nonce, &sig)
            .unwrap();
        assert_eq!(resp.database_identity.name, "db_server");
        assert!(!resp.server_signature.is_empty());

        let err = AuthHandshake::perform_handshake(&app, &db, &challenge.challenge_nonce, &[]);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6605"));
    }
}
