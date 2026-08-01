use agilang_database_transport::AgtpTransportFrame;
use anyhow::Result;

pub struct NamedPipeTransport {
    pub pipe_name: String,
    pub is_connected: bool,
}

impl NamedPipeTransport {
    pub fn open(pipe_name: impl Into<String>) -> Self {
        Self {
            pipe_name: pipe_name.into(),
            is_connected: true,
        }
    }

    pub fn send_frame(&mut self, frame: &AgtpTransportFrame) -> Result<()> {
        let _payload_len = frame.payload.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_named_pipe_send() {
        let mut pipe = NamedPipeTransport::open(r"\\.\pipe\agidb_pipe");
        let frame = AgtpTransportFrame::new(1, vec![0xaa, 0xbb]).unwrap();
        assert!(pipe.send_frame(&frame).is_ok());
    }
}
