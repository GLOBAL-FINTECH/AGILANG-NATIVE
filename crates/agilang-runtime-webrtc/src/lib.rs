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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_signal(kind: SignalKind, to: &str, payload: &str) -> SignalEnvelope {
        SignalEnvelope {
            session_id: Uuid::new_v4(),
            from: "alice".to_string(),
            to: to.to_string(),
            kind,
            payload: payload.to_string(),
        }
    }

    #[tokio::test]
    async fn signaling_hub_preserves_fifo_delivery_per_peer() {
        let hub = SignalingHub::default();
        let offer = sample_signal(SignalKind::Offer, "bob", "offer-sdp");
        let candidate = sample_signal(SignalKind::IceCandidate, "bob", "candidate-1");
        hub.publish(offer.clone()).await;
        hub.publish(candidate.clone()).await;

        assert_eq!(hub.receive("bob").await.unwrap().payload, offer.payload);
        assert_eq!(hub.receive("bob").await.unwrap().payload, candidate.payload);
        assert!(hub.receive("bob").await.is_none());
    }

    #[tokio::test]
    async fn signaling_hub_isolates_peer_queues() {
        let hub = SignalingHub::default();
        hub.publish(sample_signal(SignalKind::Offer, "bob", "offer")).await;
        hub.publish(sample_signal(SignalKind::Answer, "carol", "answer")).await;

        assert_eq!(hub.receive("carol").await.unwrap().payload, "answer");
        assert_eq!(hub.receive("bob").await.unwrap().payload, "offer");
        assert!(hub.receive("dave").await.is_none());
    }

    #[test]
    fn capabilities_report_signaling_and_explain_provider_boundary() {
        let caps = capabilities();
        assert!(caps.signaling);
        assert!(!caps.peer_connection);
        assert!(!caps.turn_server);
        assert!(caps.explanation.contains("signaling"));
    }

    #[test]
    fn full_provider_guard_fails_closed_without_feature() {
        let error = require_full_provider().unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert!(error.message.contains("full-webrtc"));
    }

    #[test]
    fn capabilities_json_serializes_current_state() {
        let json = capabilities_json().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(value["signaling"], true);
        assert_eq!(value["peer_connection"], false);
        assert!(value["explanation"].as_str().unwrap().contains("signaling"));
    }
}
