use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseIdentity {
    pub database_id: [u8; 16],
    pub public_key: Vec<u8>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationIdentity {
    pub application_id: [u8; 16],
    pub public_key: Vec<u8>,
    pub name: String,
}

impl DatabaseIdentity {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            database_id: [0xdb; 16],
            public_key: vec![0x10, 0x20, 0x30, 0x40],
            name: name.into(),
        }
    }
}

impl ApplicationIdentity {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            application_id: [0xaa; 16],
            public_key: vec![0x50, 0x60, 0x70, 0x80],
            name: name.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_creation() {
        let db_id = DatabaseIdentity::new("main_agidb");
        let app_id = ApplicationIdentity::new("web_app");

        assert_eq!(db_id.name, "main_agidb");
        assert_eq!(app_id.name, "web_app");
    }
}
