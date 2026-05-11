//! Sealed state resolution capsule (host-side); hashed into the execution envelope.

use blake3::Hasher;
use icn_boundary::Hash;
use serde::{Deserialize, Serialize};

use crate::constants::{PROCESS_ID, RULE_REF, TARGET_REF};

/// Deterministic capsule pinned before WASM execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateResolutionCapsule {
    pub process_id: String,
    pub target_ref: String,
    pub rule_ref: String,
    /// Sorted receipt hashes (causal chain materialization).
    pub receipt_hashes_sorted: Vec<Hash>,
    pub tip_receipt_hash: Hash,
    pub standing_context_hash: Hash,
    pub notice_set_hash: Hash,
    pub vote_set_root: Hash,
}

impl StateResolutionCapsule {
    pub const DOMAIN: &[u8] = b"icn:baseline:state_resolution_capsule:v1";

    pub fn compute_hash(&self) -> Hash {
        let bytes =
            postcard::to_allocvec(self).expect("capsule serialization must succeed for hashing");
        let mut h = Hasher::new();
        h.update(Self::DOMAIN);
        h.update(&bytes);
        Hash(*h.finalize().as_bytes())
    }
}

/// Build capsule from resolved frontier (canonical path).
pub fn build_capsule(
    receipt_hashes: Vec<Hash>,
    tip: Hash,
    standing_context_hash: Hash,
    notice_set_hash: Hash,
    vote_set_root: Hash,
) -> StateResolutionCapsule {
    let mut sorted = receipt_hashes;
    sorted.sort();
    StateResolutionCapsule {
        process_id: PROCESS_ID.into(),
        target_ref: TARGET_REF.into(),
        rule_ref: RULE_REF.into(),
        receipt_hashes_sorted: sorted,
        tip_receipt_hash: tip,
        standing_context_hash,
        notice_set_hash,
        vote_set_root,
    }
}
