//! WebRTC control plane for AGILANG.
//!
//! Signaling is built in. Media transport and TURN/STUN services are feature-gated
//! because they require platform codecs, UDP reachability, certificates and
//! deployment-specific public addresses. The runtime never advertises relay
//! readiness until a concrete provider has initialized successfully.
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalKind {
    Offer,
    Answer,
    IceCandidate,
    Hangup,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEnvelope {
    pub session_id: Uuid,
    pub from: String,
    pub to: String,
    pub kind: SignalKind,
    pub payload: String,
}
#[derive(Default, Clone)]
pub struct SignalingHub {
    queues: Arc<RwLock<HashMap<String, VecDeque<SignalEnvelope>>>>,
}
impl SignalingHub {
    pub async fn publish(&self, message: SignalEnvelope) {
        self.queues
            .write()
            .await
            .entry(message.to.clone())
            .or_default()
            .push_back(message);
    }
    pub async fn receive(&self, peer: &str) -> Option<SignalEnvelope> {
        self.queues
            .write()
            .await
            .get_mut(peer)
            .and_then(VecDeque::pop_front)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcCapabilities {
    pub signaling: bool,
    pub peer_connection: bool,
    pub audio: bool,
    pub video: bool,
    pub data_channel: bool,
    pub stun_server: bool,
    pub turn_server: bool,
    pub explanation: String,
}
pub fn capabilities() -> WebRtcCapabilities {
    let full = cfg!(feature = "full-webrtc");
    WebRtcCapabilities {
        signaling: true,
        peer_connection: full,
        audio: full,
        video: full,
        data_channel: full,
        stun_server: full,
        turn_server: full,
        explanation: if full {
            "native WebRTC provider compiled; deployment still requires certificates, UDP/TCP ports and an externally reachable relay address".into()
        } else {
            "built-in signaling is available; compile with full-webrtc to enable native peer connection and STUN/TURN provider integration".into()
        },
    }
}
pub fn require_full_provider() -> RuntimeResult<()> {
    if cfg!(feature = "full-webrtc") {
        Ok(())
    } else {
        Err(AgilangError::new(
            ErrorCode::Unsupported,
            "AGILANG was built without the full-webrtc feature",
        ))
    }
}
pub fn capabilities_json() -> RuntimeResult<Vec<u8>> {
    serde_json::to_vec(&capabilities())
        .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))
}
