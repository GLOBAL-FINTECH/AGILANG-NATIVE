use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgtpSessionToken {
    pub app_id: String,
    pub database_id: String,
    pub capabilities: Vec<String>,
    pub expires_at: u64,
    pub nonce: String,
    pub signature: String,
}

impl AgtpSessionToken {
    pub fn is_valid(&self, current_time: u64) -> bool {
        current_time < self.expires_at && !self.signature.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agtp_token_validation() {
        let token = AgtpSessionToken {
            app_id: "app-1".into(),
            database_id: "db-1".into(),
            capabilities: vec!["SELECT".into(), "INSERT".into()],
            expires_at: 1000,
            nonce: "nonce123".into(),
            signature: "sig123".into(),
        };

        assert!(token.is_valid(500));
        assert!(!token.is_valid(1500));
    }
}
