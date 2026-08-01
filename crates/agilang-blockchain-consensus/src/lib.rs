//! Validator signing, attestations and weighted checkpoint finality for Native AGILANG.

use agilang_blockchain_core::{stable_json, Block, BlockchainConfig};
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};

type HmacSha256 = Hmac<Sha256>;

const BLOCK_DOMAIN: &[u8] = b"AGILANG_BLOCK_V1";
const ATTESTATION_DOMAIN: &[u8] = b"AGILANG_ATTESTATION_V1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    pub validator: String,
    pub source_epoch: u64,
    pub source_hash: String,
    pub target_epoch: u64,
    pub target_hash: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub epoch: u64,
    pub block_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalityState {
    pub justified: Option<Checkpoint>,
    pub finalized: Option<Checkpoint>,
    pub equivocators: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct ConsensusEngine {
    validators: BTreeMap<String, u64>,
    signing_keys: BTreeMap<String, String>,
    votes: BTreeMap<(u64, String), BTreeSet<String>>,
    validator_targets: BTreeMap<(String, u64), String>,
    state: FinalityState,
}

impl ConsensusEngine {
    pub fn new(config: &BlockchainConfig) -> RuntimeResult<Self> {
        config.validate()?;
        Ok(Self {
            validators: config.validators.clone(),
            signing_keys: config.validator_signing_keys.clone(),
            votes: BTreeMap::new(),
            validator_targets: BTreeMap::new(),
            state: FinalityState::default(),
        })
    }

    pub fn state(&self) -> &FinalityState {
        &self.state
    }

    pub fn sign_block(&self, validator: &str, block: &Block) -> RuntimeResult<String> {
        let key = self.key_for(validator)?;
        let payload = block_signing_payload(block)?;
        Ok(sign(BLOCK_DOMAIN, key.as_bytes(), payload.as_bytes())?)
    }

    pub fn verify_block_signature(&self, validator: &str, block: &Block) -> RuntimeResult<bool> {
        if block.header.proposer != validator {
            return Ok(false);
        }
        let expected = self.sign_block(validator, block)?;
        Ok(constant_time_eq(expected.as_bytes(), block.validator_signature.as_bytes()))
    }

    pub fn create_attestation(
        &self,
        validator: &str,
        source: Checkpoint,
        target: Checkpoint,
    ) -> RuntimeResult<Attestation> {
        if target.epoch <= source.epoch {
            return invalid("attestation target epoch must be greater than source epoch");
        }
        let key = self.key_for(validator)?;
        let payload = attestation_payload(validator, &source, &target)?;
        let signature = sign(ATTESTATION_DOMAIN, key.as_bytes(), payload.as_bytes())?;
        Ok(Attestation {
            validator: validator.to_string(),
            source_epoch: source.epoch,
            source_hash: source.block_hash,
            target_epoch: target.epoch,
            target_hash: target.block_hash,
            signature,
        })
    }

    pub fn verify_attestation(&self, attestation: &Attestation) -> RuntimeResult<bool> {
        let key = self.key_for(&attestation.validator)?;
        let source = Checkpoint {
            epoch: attestation.source_epoch,
            block_hash: attestation.source_hash.clone(),
        };
        let target = Checkpoint {
            epoch: attestation.target_epoch,
            block_hash: attestation.target_hash.clone(),
        };
        let payload = attestation_payload(&attestation.validator, &source, &target)?;
        let expected = sign(ATTESTATION_DOMAIN, key.as_bytes(), payload.as_bytes())?;
        Ok(constant_time_eq(expected.as_bytes(), attestation.signature.as_bytes()))
    }

    pub fn submit_attestation(&mut self, attestation: Attestation) -> RuntimeResult<FinalityState> {
        if !self.verify_attestation(&attestation)? {
            return invalid("invalid validator attestation signature");
        }
        if attestation.target_epoch <= attestation.source_epoch {
            return invalid("invalid attestation epoch ordering");
        }

        let validator_epoch = (attestation.validator.clone(), attestation.target_epoch);
        if let Some(existing_hash) = self.validator_targets.get(&validator_epoch) {
            if existing_hash != &attestation.target_hash {
                self.state.equivocators.insert(attestation.validator.clone());
                return invalid("validator equivocation detected for target epoch");
            }
            return Ok(self.state.clone());
        }
        self.validator_targets
            .insert(validator_epoch, attestation.target_hash.clone());

        let vote_key = (attestation.target_epoch, attestation.target_hash.clone());
        self.votes
            .entry(vote_key.clone())
            .or_default()
            .insert(attestation.validator.clone());

        if self.has_supermajority(&vote_key) {
            let target = Checkpoint {
                epoch: attestation.target_epoch,
                block_hash: attestation.target_hash,
            };
            let previous_justified = self.state.justified.clone();
            self.state.justified = Some(target.clone());

            if let Some(previous) = previous_justified {
                if previous.epoch + 1 == target.epoch
                    && previous.epoch == attestation.source_epoch
                    && previous.block_hash == attestation.source_hash
                {
                    self.state.finalized = Some(previous);
                }
            }
        }
        Ok(self.state.clone())
    }

    fn has_supermajority(&self, key: &(u64, String)) -> bool {
        let total: u128 = self.validators.values().map(|stake| u128::from(*stake)).sum();
        if total == 0 {
            return false;
        }
        let voted: u128 = self
            .votes
            .get(key)
            .into_iter()
            .flat_map(|validators| validators.iter())
            .filter_map(|validator| self.validators.get(validator))
            .map(|stake| u128::from(*stake))
            .sum();
        voted.saturating_mul(3) >= total.saturating_mul(2)
    }

    fn key_for(&self, validator: &str) -> RuntimeResult<&str> {
        if !self.validators.contains_key(validator) {
            return invalid("validator is not active");
        }
        self.signing_keys
            .get(validator)
            .map(String::as_str)
            .filter(|key| !key.is_empty())
            .ok_or_else(|| invalid_error("validator signing key is unavailable"))
    }
}

pub fn block_signing_payload(block: &Block) -> RuntimeResult<String> {
    let mut unsigned = block.clone();
    unsigned.hash.clear();
    unsigned.validator_signature.clear();
    stable_json(&unsigned)
}

fn attestation_payload(
    validator: &str,
    source: &Checkpoint,
    target: &Checkpoint,
) -> RuntimeResult<String> {
    stable_json(&serde_json::json!({
        "source_epoch": source.epoch,
        "source_hash": source.block_hash,
        "target_epoch": target.epoch,
        "target_hash": target.block_hash,
        "validator": validator,
    }))
}

fn sign(domain: &[u8], key: &[u8], payload: &[u8]) -> RuntimeResult<String> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|error| invalid_error(format!("invalid signing key: {error}")))?;
    mac.update(domain);
    mac.update(payload);
    Ok(format!("0x{}", encode_hex(&mac.finalize().into_bytes())))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn invalid<T>(message: impl Into<String>) -> RuntimeResult<T> {
    Err(invalid_error(message))
}

fn invalid_error(message: impl Into<String>) -> AgilangError {
    AgilangError::new(ErrorCode::InvalidArgument, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_blockchain_core::Block;

    fn config() -> BlockchainConfig {
        let mut config = BlockchainConfig::default();
        config.validators = BTreeMap::from([
            ("validator-a".to_string(), 40),
            ("validator-b".to_string(), 35),
            ("validator-c".to_string(), 25),
        ]);
        config.validator_signing_keys = BTreeMap::from([
            ("validator-a".to_string(), "key-a".to_string()),
            ("validator-b".to_string(), "key-b".to_string()),
            ("validator-c".to_string(), "key-c".to_string()),
        ]);
        config
    }

    #[test]
    fn signs_and_verifies_blocks() {
        let config = config();
        let engine = ConsensusEngine::new(&config).unwrap();
        let genesis = Block::genesis(&config, 1).unwrap();
        let mut block = Block::child(
            &config,
            &genesis,
            "validator-a",
            2,
            1,
            vec![],
            genesis.header.state_root.clone(),
            40,
            "",
        )
        .unwrap();
        block.validator_signature = engine.sign_block("validator-a", &block).unwrap();
        assert!(engine.verify_block_signature("validator-a", &block).unwrap());
    }

    #[test]
    fn supermajority_justifies_and_next_link_finalizes() {
        let config = config();
        let mut engine = ConsensusEngine::new(&config).unwrap();
        let genesis = Checkpoint { epoch: 0, block_hash: "g".into() };
        let one = Checkpoint { epoch: 1, block_hash: "one".into() };
        for validator in ["validator-a", "validator-b"] {
            let vote = engine.create_attestation(validator, genesis.clone(), one.clone()).unwrap();
            engine.submit_attestation(vote).unwrap();
        }
        assert_eq!(engine.state().justified.as_ref().unwrap().epoch, 1);

        let two = Checkpoint { epoch: 2, block_hash: "two".into() };
        for validator in ["validator-a", "validator-b"] {
            let vote = engine.create_attestation(validator, one.clone(), two.clone()).unwrap();
            engine.submit_attestation(vote).unwrap();
        }
        assert_eq!(engine.state().finalized.as_ref().unwrap().epoch, 1);
    }

    #[test]
    fn detects_equivocation() {
        let config = config();
        let mut engine = ConsensusEngine::new(&config).unwrap();
        let source = Checkpoint { epoch: 0, block_hash: "g".into() };
        let first = Checkpoint { epoch: 1, block_hash: "a".into() };
        let second = Checkpoint { epoch: 1, block_hash: "b".into() };
        let vote = engine.create_attestation("validator-a", source.clone(), first).unwrap();
        engine.submit_attestation(vote).unwrap();
        let conflict = engine.create_attestation("validator-a", source, second).unwrap();
        assert!(engine.submit_attestation(conflict).is_err());
        assert!(engine.state().equivocators.contains("validator-a"));
    }
}
