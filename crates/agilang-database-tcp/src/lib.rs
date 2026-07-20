use anyhow::{bail, Result};

#[derive(Debug)]
pub struct TcpListenerGuard {
    pub port: u16,
    pub air_gapped: bool,
}

impl TcpListenerGuard {
    pub fn bind(port: u16, air_gapped: bool) -> Result<Self> {
        if air_gapped {
            bail!(
                "E6604 Network listener blocked in air-gapped mode: TCP socket creation rejected"
            );
        }
        Ok(Self { port, air_gapped })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_air_gapped_tcp_listener_blocking() {
        let err = TcpListenerGuard::bind(5432, true);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6604"));

        let ok = TcpListenerGuard::bind(5432, false);
        assert!(ok.is_ok());
    }
}
