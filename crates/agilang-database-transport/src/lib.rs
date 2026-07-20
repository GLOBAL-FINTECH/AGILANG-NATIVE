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
