#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::string::String;
#[cfg(feature = "std")]
use std::vec::Vec;

use serde::{Deserialize, Serialize};

/// Content-addressable 32-byte hash at the WASM boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hash(pub [u8; 32]);

impl From<[u8; 32]> for Hash {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeterminismClass {
    Strict,
    Advisory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyClass {
    Public,
    EncryptedOverlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinalityClass {
    InstitutionLocal,
    FederationPending,
    BridgeExecuted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultKind {
    GateValidation,
    MutationProposal,
}

/// Bounded facts visible to the WASM guest (no institutional semantics).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalFacts {
    pub eligible_voters: u32,
    pub votes_cast: u32,
    pub approvals: u32,
    pub rejections: u32,
    pub abstentions: u32,
    pub required_approvals: u32,
    pub notice_delivered: bool,
    pub allocation_requested: u64,
    pub allocation_limit: u64,
    pub reservation_not_consumed: bool,
}

/// Host → guest execution envelope (version 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionInputEnvelopeV1 {
    pub abi_version: u32,
    pub schema_id: String,
    pub workload_id: String,
    pub module_hash: Hash,
    pub process_id: String,
    pub target_ref: String,
    pub state_resolution_capsule_hash: Hash,
    pub canonical_fact_snapshot_hash: Hash,
    pub authority_context_hash: Hash,
    pub standing_context_hash: Hash,
    pub agreement_context_hash: Hash,
    pub mandate_ref: Option<String>,
    pub rule_ref: String,
    pub determinism_class: DeterminismClass,
    pub privacy_class: PrivacyClass,
    pub finality_class: FinalityClass,
    pub fuel_limit: u64,
    pub input_artifact_refs: Vec<Hash>,
    pub canonical_facts: CanonicalFacts,
    pub expected_output_schema: String,
}

/// Guest → host execution envelope (version 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionOutputEnvelopeV1 {
    pub abi_version: u32,
    pub schema_id: String,
    pub workload_id: String,
    pub process_id: String,
    /// Must match `ExecutionInputEnvelopeV1::module_hash` for the same evaluation.
    pub module_hash: Hash,
    pub result_kind: ResultKind,
    pub passed: bool,
    pub output_artifact_refs: Vec<Hash>,
    pub diagnostics: Vec<String>,
    pub proposed_transition_ref: Option<Hash>,
    pub consumed_fuel: u64,
}
