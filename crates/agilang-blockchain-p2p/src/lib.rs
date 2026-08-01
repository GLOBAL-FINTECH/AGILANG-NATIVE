//! Native AGILANG blockchain P2P protocol types and synchronization planning.

use agilang_blockchain_core::{stable_hash, Block, Transaction};
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handshake {
    pub protocol_version: u32,
    pub node_id: String,
    pub chain_id: u64,
    pub genesis_hash: String,
    pub head_hash: String,
    pub head_height: u64,
    pub finalized_hash: String,
    pub finalized_height: u64,
    pub capabilities: BTreeSet<String>,
    pub timestamp_ms: u64,
}

impl Handshake {
    pub fn validate_against(&self, local: &Handshake, max_clock_drift_ms: u64) -> RuntimeResult<()> {
        if self.protocol_version != PROTOCOL_VERSION {
            return invalid("unsupported blockchain P2P protocol version");
        }
        if self.node_id.trim().is_empty() || self.node_id == local.node_id {
            return invalid("invalid or duplicate peer node identity");
        }
        if self.chain_id != local.chain_id {
            return invalid("peer chain ID does not match local chain");
        }
        if self.genesis_hash != local.genesis_hash {
            return invalid("peer genesis hash does not match local chain");
        }
        if self.timestamp_ms.abs_diff(local.timestamp_ms) > max_clock_drift_ms {
            return invalid("peer clock drift exceeds configured maximum");
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> RuntimeResult<String> {
        stable_hash(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Message {
    Handshake(Handshake),
    HeadAnnouncement(HeadAnnouncement),
    GetHeaders(GetHeaders),
    Headers(Vec<BlockHeaderSummary>),
    GetBlocks(Vec<String>),
    Blocks(Vec<Block>),
    Transactions(Vec<Transaction>),
    GetSnapshot(SnapshotRequest),
    SnapshotChunk(SnapshotChunk),
    Ping(u64),
    Pong(u64),
    Disconnect { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadAnnouncement {
    pub head_hash: String,
    pub head_height: u64,
    pub finalized_hash: String,
    pub finalized_height: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetHeaders {
    pub locator: Vec<String>,
    pub stop_hash: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockHeaderSummary {
    pub hash: String,
    pub parent_hash: String,
    pub height: u64,
    pub slot: u64,
    pub score: u128,
    pub state_root: String,
}

impl From<&Block> for BlockHeaderSummary {
    fn from(block: &Block) -> Self {
        Self {
            hash: block.hash.clone(),
            parent_hash: block.header.parent_hash.clone(),
            height: block.header.height,
            slot: block.header.slot,
            score: block.header.score,
            state_root: block.header.state_root.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRequest {
    pub checkpoint_hash: String,
    pub start_key: Option<String>,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotChunk {
    pub checkpoint_hash: String,
    pub sequence: u64,
    pub entries: BTreeMap<String, serde_json::Value>,
    pub next_key: Option<String>,
    pub checksum: String,
}

impl SnapshotChunk {
    pub fn calculate_checksum(&self) -> RuntimeResult<String> {
        stable_hash(&serde_json::json!({
            "checkpoint_hash": self.checkpoint_hash,
            "sequence": self.sequence,
            "entries": self.entries,
            "next_key": self.next_key,
        }))
    }

    pub fn verify(&self) -> RuntimeResult<()> {
        if self.checksum != self.calculate_checksum()? {
            return invalid("snapshot chunk checksum mismatch");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncMode {
    UpToDate,
    HeaderAndBlockSync { from_height: u64, to_height: u64 },
    CheckpointSnapshot { checkpoint_hash: String, checkpoint_height: u64 },
    RejectFinalityConflict,
}

pub fn plan_sync(local: &Handshake, remote: &Handshake, snapshot_threshold: u64) -> SyncMode {
    if remote.finalized_height < local.finalized_height
        && remote.finalized_hash != local.finalized_hash
    {
        return SyncMode::RejectFinalityConflict;
    }
    if remote.head_height <= local.head_height {
        return SyncMode::UpToDate;
    }
    let distance = remote.head_height - local.head_height;
    if distance >= snapshot_threshold && remote.finalized_height > local.finalized_height {
        return SyncMode::CheckpointSnapshot {
            checkpoint_hash: remote.finalized_hash.clone(),
            checkpoint_height: remote.finalized_height,
        };
    }
    SyncMode::HeaderAndBlockSync {
        from_height: local.head_height.saturating_add(1),
        to_height: remote.head_height,
    }
}

#[derive(Debug, Clone)]
pub struct PeerState {
    pub handshake: Handshake,
    pub score: i64,
    pub last_seen_ms: u64,
    pub in_flight: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct PeerBook {
    peers: BTreeMap<String, PeerState>,
    recent_messages: BTreeSet<String>,
    message_order: VecDeque<String>,
    max_recent_messages: usize,
}

impl PeerBook {
    pub fn new(max_recent_messages: usize) -> RuntimeResult<Self> {
        if max_recent_messages == 0 {
            return invalid("recent-message cache must be bounded above zero");
        }
        Ok(Self {
            peers: BTreeMap::new(),
            recent_messages: BTreeSet::new(),
            message_order: VecDeque::new(),
            max_recent_messages,
        })
    }

    pub fn register(&mut self, peer: Handshake, now_ms: u64) {
        self.peers.insert(
            peer.node_id.clone(),
            PeerState {
                handshake: peer,
                score: 0,
                last_seen_ms: now_ms,
                in_flight: BTreeSet::new(),
            },
        );
    }

    pub fn penalize(&mut self, node_id: &str, amount: i64) -> bool {
        let Some(peer) = self.peers.get_mut(node_id) else { return false; };
        peer.score = peer.score.saturating_sub(amount.abs());
        peer.score <= -100
    }

    pub fn reward(&mut self, node_id: &str, amount: i64) {
        if let Some(peer) = self.peers.get_mut(node_id) {
            peer.score = peer.score.saturating_add(amount.abs()).min(100);
        }
    }

    pub fn record_message<T: Serialize>(&mut self, message: &T) -> RuntimeResult<bool> {
        let id = stable_hash(message)?;
        if !self.recent_messages.insert(id.clone()) {
            return Ok(false);
        }
        self.message_order.push_back(id);
        while self.message_order.len() > self.max_recent_messages {
            if let Some(oldest) = self.message_order.pop_front() {
                self.recent_messages.remove(&oldest);
            }
        }
        Ok(true)
    }

    pub fn peers(&self) -> impl Iterator<Item = (&String, &PeerState)> {
        self.peers.iter()
    }
}

pub fn encode_message(message: &Message, max_bytes: usize) -> RuntimeResult<Vec<u8>> {
    let encoded = serde_json::to_vec(message).map_err(json_error)?;
    if encoded.len() > max_bytes {
        return invalid("P2P message exceeds configured size limit");
    }
    let length = u32::try_from(encoded.len()).map_err(|_| invalid_error("P2P message is too large"))?;
    let mut frame = Vec::with_capacity(4 + encoded.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&encoded);
    Ok(frame)
}

pub fn decode_message(frame: &[u8], max_bytes: usize) -> RuntimeResult<Message> {
    if frame.len() < 4 {
        return invalid("P2P frame is truncated");
    }
    let length = u32::from_be_bytes(frame[..4].try_into().expect("four-byte slice")) as usize;
    if length > max_bytes || frame.len() != length + 4 {
        return invalid("P2P frame length is invalid");
    }
    serde_json::from_slice(&frame[4..]).map_err(json_error)
}

fn invalid<T>(message: impl Into<String>) -> RuntimeResult<T> {
    Err(invalid_error(message))
}
fn invalid_error(message: impl Into<String>) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidArgument, message.into())
}
fn json_error(error: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidState, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handshake(node: &str, height: u64) -> Handshake {
        Handshake {
            protocol_version: PROTOCOL_VERSION,
            node_id: node.to_string(),
            chain_id: 1990,
            genesis_hash: "0xgenesis".to_string(),
            head_hash: format!("0x{height:x}"),
            head_height: height,
            finalized_hash: "0xfinal".to_string(),
            finalized_height: height.saturating_sub(8),
            capabilities: BTreeSet::from(["blocks".to_string(), "snapshots".to_string()]),
            timestamp_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn rejects_wrong_chain_handshake() {
        let local = handshake("local", 10);
        let mut remote = handshake("remote", 20);
        remote.chain_id = 1;
        assert!(remote.validate_against(&local, 120_000).is_err());
    }

    #[test]
    fn selects_snapshot_for_large_gap() {
        let local = handshake("local", 10);
        let remote = handshake("remote", 10_000);
        assert!(matches!(plan_sync(&local, &remote, 1_000), SyncMode::CheckpointSnapshot { .. }));
    }

    #[test]
    fn frames_round_trip() {
        let message = Message::Ping(42);
        let frame = encode_message(&message, 1024).unwrap();
        assert_eq!(decode_message(&frame, 1024).unwrap(), message);
    }

    #[test]
    fn suppresses_duplicate_messages() {
        let mut peers = PeerBook::new(2).unwrap();
        assert!(peers.record_message(&Message::Ping(1)).unwrap());
        assert!(!peers.record_message(&Message::Ping(1)).unwrap());
    }
}
