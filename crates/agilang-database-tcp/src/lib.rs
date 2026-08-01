use agilang_database_auth::AuthHandshake;
use agilang_database_identity::{ApplicationIdentity, DatabaseIdentity};
use agilang_database_security_kernel::Capability;
use agilang_database_session::AgtpSession;
use agilang_database_transport::{
    HandshakeChallenge, TransportRequest, TransportResponse, TransportSessionInfo,
    TransportStatusSnapshot,
};
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct TcpListenerGuard {
    pub local_addr: SocketAddr,
    pub air_gapped: bool,
}

impl TcpListenerGuard {
    pub fn bind(addr: &str, air_gapped: bool) -> Result<(Self, TcpListener)> {
        if air_gapped {
            bail!(
                "E6604 Network listener blocked in air-gapped mode: TCP socket creation rejected"
            );
        }
        let listener = TcpListener::bind(addr)
            .with_context(|| format!("failed to bind transport listener at {addr}"))?;
        let local_addr = listener.local_addr()?;
        Ok((
            Self {
                local_addr,
                air_gapped,
            },
            listener,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct TransportServerConfig {
    pub bind_addr: String,
    pub air_gapped: bool,
    pub database_identity: DatabaseIdentity,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug)]
pub struct LiveTransportServer {
    local_addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LiveTransportServer {
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl Drop for LiveTransportServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.local_addr);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Debug, Clone)]
pub struct HandshakeResult {
    pub database_identity: DatabaseIdentity,
    pub session: TransportSessionInfo,
    pub server_signature: Vec<u8>,
}

#[derive(Debug)]
struct ServerState {
    listener_addr: String,
    air_gapped: bool,
    started_at: u64,
    active_sessions: HashMap<[u8; 16], TransportSessionInfo>,
    known_peers: HashSet<String>,
    pending_challenges: HashMap<String, HandshakeChallenge>,
}

impl ServerState {
    fn snapshot(&self) -> TransportStatusSnapshot {
        let mut known_peers = self.known_peers.iter().cloned().collect::<Vec<_>>();
        known_peers.sort();
        TransportStatusSnapshot {
            listener_addr: self.listener_addr.clone(),
            air_gapped: self.air_gapped,
            active_sessions: self.active_sessions.values().cloned().collect(),
            known_peers,
            started_at: self.started_at,
        }
    }
}

pub fn start_server(config: TransportServerConfig) -> Result<LiveTransportServer> {
    let (guard, listener) = TcpListenerGuard::bind(&config.bind_addr, config.air_gapped)?;
    listener.set_nonblocking(false)?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let state = Arc::new(Mutex::new(ServerState {
        listener_addr: guard.local_addr.to_string(),
        air_gapped: config.air_gapped,
        started_at: unix_seconds(),
        active_sessions: HashMap::new(),
        known_peers: HashSet::new(),
        pending_challenges: HashMap::new(),
    }));
    let shutdown_flag = shutdown.clone();
    let db_identity = config.database_identity.clone();
    let capabilities = config.capabilities.clone();
    let thread_state = state.clone();
    let local_addr = guard.local_addr;
    let handle = thread::spawn(move || {
        while !shutdown_flag.load(Ordering::SeqCst) {
            let Ok((stream, peer)) = listener.accept() else {
                continue;
            };
            let _ = handle_connection(stream, peer, &db_identity, &capabilities, &thread_state);
        }
    });
    Ok(LiveTransportServer {
        local_addr,
        shutdown,
        thread: Some(handle),
    })
}

pub fn fetch_status(addr: &str, timeout: Duration) -> Result<TransportStatusSnapshot> {
    let response = send_request(addr, timeout, &TransportRequest::Status)?;
    match response {
        TransportResponse::Status { snapshot } => Ok(snapshot),
        TransportResponse::Error { code, message } => bail!("{code} {message}"),
        other => bail!("unexpected transport response: {:?}", other),
    }
}

pub fn perform_handshake(
    addr: &str,
    timeout: Duration,
    client_identity: ApplicationIdentity,
    requested_capabilities: Vec<Capability>,
) -> Result<HandshakeResult> {
    let init = send_request(
        addr,
        timeout,
        &TransportRequest::HandshakeInit {
            client_identity: client_identity.clone(),
            requested_capabilities: requested_capabilities.clone(),
        },
    )?;
    let challenge = match init {
        TransportResponse::HandshakeChallenge { challenge } => challenge,
        TransportResponse::Error { code, message } => bail!("{code} {message}"),
        other => bail!("unexpected transport response: {:?}", other),
    };
    let client_signature = AuthHandshake::sign_client_challenge(
        &client_identity,
        &challenge.database_identity,
        &challenge.challenge_nonce,
    );
    let finish = send_request(
        addr,
        timeout,
        &TransportRequest::HandshakeFinish {
            client_identity,
            requested_capabilities,
            challenge_nonce: challenge.challenge_nonce,
            client_signature,
        },
    )?;
    match finish {
        TransportResponse::HandshakeComplete {
            session,
            server_signature,
        } => Ok(HandshakeResult {
            database_identity: challenge.database_identity,
            session,
            server_signature,
        }),
        TransportResponse::Error { code, message } => bail!("{code} {message}"),
        other => bail!("unexpected transport response: {:?}", other),
    }
}

fn send_request(addr: &str, timeout: Duration, request: &TransportRequest) -> Result<TransportResponse> {
    let mut stream = TcpStream::connect(addr)
        .with_context(|| format!("failed to connect to transport peer at {addr}"))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let payload = serde_json::to_vec(request)?;
    stream.write_all(&payload)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 {
        bail!("transport peer closed connection before sending a response");
    }
    serde_json::from_str(line.trim_end()).context("failed to decode transport response")
}

fn handle_connection(
    mut stream: TcpStream,
    peer: SocketAddr,
    db_identity: &DatabaseIdentity,
    capabilities: &[Capability],
    state: &Arc<Mutex<ServerState>>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }
    let request: TransportRequest =
        serde_json::from_str(line.trim_end()).context("failed to decode transport request")?;
    {
        let mut locked = state.lock().unwrap();
        locked.known_peers.insert(peer.to_string());
    }
    let response = match request {
        TransportRequest::Status => {
            let snapshot = state.lock().unwrap().snapshot();
            TransportResponse::Status { snapshot }
        }
        TransportRequest::HandshakeInit { client_identity, .. } => {
            let challenge = AuthHandshake::issue_challenge(db_identity)?;
            state.lock().unwrap().pending_challenges.insert(
                client_identity.name.clone(),
                challenge.clone(),
            );
            TransportResponse::HandshakeChallenge { challenge }
        }
        TransportRequest::HandshakeFinish {
            client_identity,
            requested_capabilities,
            challenge_nonce,
            client_signature,
        } => {
            let challenge = state
                .lock()
                .unwrap()
                .pending_challenges
                .remove(&client_identity.name);
            match challenge {
                Some(stored) if stored.challenge_nonce == challenge_nonce => {
                    let handshake = AuthHandshake::perform_handshake(
                        &client_identity,
                        db_identity,
                        &challenge_nonce,
                        &client_signature,
                    )?;
                    let session = AgtpSession::new(
                        client_identity.application_id,
                        db_identity.database_id,
                        requested_capabilities.clone(),
                    );
                    let info = TransportSessionInfo {
                        session_id: session.session_id,
                        application_name: client_identity.name.clone(),
                        database_name: db_identity.name.clone(),
                        capabilities: requested_capabilities,
                        issued_at: session.issued_at,
                        expires_at: session.expires_at,
                        peer_addr: peer.to_string(),
                    };
                    state
                        .lock()
                        .unwrap()
                        .active_sessions
                        .insert(info.session_id, info.clone());
                    TransportResponse::HandshakeComplete {
                        session: info,
                        server_signature: handshake.server_signature,
                    }
                }
                Some(_) => TransportResponse::Error {
                    code: "E6606".to_string(),
                    message: "challenge nonce mismatch".to_string(),
                },
                None => TransportResponse::Error {
                    code: "E6607".to_string(),
                    message: "no pending challenge for client".to_string(),
                },
            }
        }
    };
    write_response(&mut stream, &response)?;
    let _ = capabilities;
    Ok(())
}

fn write_response(stream: &mut TcpStream, response: &TransportResponse) -> Result<()> {
    let payload = serde_json::to_vec(response)?;
    stream.write_all(&payload)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
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
    fn test_air_gapped_tcp_listener_blocking() {
        let err = TcpListenerGuard::bind("127.0.0.1:0", true);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("E6604"));

        let ok = TcpListenerGuard::bind("127.0.0.1:0", false);
        assert!(ok.is_ok());
    }

    #[test]
    fn transport_status_and_handshake_use_live_runtime_state() {
        let server = start_server(TransportServerConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            air_gapped: false,
            database_identity: DatabaseIdentity::new("main_agidb"),
            capabilities: vec![Capability::TableRead, Capability::TableWrite],
        })
        .unwrap();
        let addr = server.local_addr().to_string();

        let before = fetch_status(&addr, Duration::from_secs(2)).unwrap();
        assert_eq!(before.listener_addr, addr);
        assert_eq!(before.active_sessions.len(), 0);

        let handshake = perform_handshake(
            &addr,
            Duration::from_secs(2),
            ApplicationIdentity::new("app_client"),
            vec![Capability::TableRead],
        )
        .unwrap();
        assert_eq!(handshake.database_identity.name, "main_agidb");
        assert!(!handshake.server_signature.is_empty());

        let after = fetch_status(&addr, Duration::from_secs(2)).unwrap();
        assert_eq!(after.active_sessions.len(), 1);
        assert_eq!(after.active_sessions[0].application_name, "app_client");
        assert!(!after.known_peers.is_empty());
    }

    #[test]
    fn unreachable_peer_returns_error() {
        let err = fetch_status("127.0.0.1:9", Duration::from_millis(200));
        assert!(err.is_err());
    }
}
