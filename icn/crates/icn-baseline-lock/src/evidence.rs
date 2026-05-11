//! Evidence packet and member-legible Action Card projection (fixture).

use icn_boundary::Hash;
use serde::Serialize;

/// Bundle of hashes for audit / export (fixture-level).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EvidencePacket {
    pub input_envelope_hash: Hash,
    pub output_envelope_hash: Hash,
    pub module_hash: Hash,
    pub canonical_fact_snapshot_hash: Hash,
    pub state_resolution_capsule_hash: Hash,
    pub receipt_ref_hashes: Vec<Hash>,
    pub gate_result_receipt_hash: Hash,
    pub allocation_receipt_canonical_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionCardView {
    pub title: String,
    pub status: String,
    pub summary: String,
}

pub fn action_card_allocation_authorized() -> ActionCardView {
    ActionCardView {
        title: "Allocation authorized".into(),
        status: "Completed".into(),
        summary: "The cooperative process gate passed and allocation was recorded.".into(),
    }
}
