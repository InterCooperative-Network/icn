//! Test-equivalent receipts emitted after successful hostile validation.

use blake3::Hasher;
use ed25519_dalek::{Signer, SigningKey};
use icn_boundary::Hash;
use serde::Serialize;

/// Test stand-in for `ProcessGateResultReceipt` (not the production governance type).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BaselineProcessGateResultReceipt {
    pub session_id: String,
    pub passed: bool,
    pub input_envelope_hash: Hash,
    pub output_envelope_hash: Hash,
    pub module_hash: Hash,
    pub recorded_at: u64,
}

impl BaselineProcessGateResultReceipt {
    pub const DOMAIN: &[u8] = b"icn:baseline:process_gate_result:v1";

    pub fn record_hash(&self) -> Hash {
        let mut h = Hasher::new();
        h.update(Self::DOMAIN);
        h.update(&(self.session_id.len() as u64).to_le_bytes());
        h.update(self.session_id.as_bytes());
        h.update(&[self.passed as u8]);
        h.update(&self.input_envelope_hash.0);
        h.update(&self.output_envelope_hash.0);
        h.update(&self.module_hash.0);
        h.update(&self.recorded_at.to_le_bytes());
        Hash(*h.finalize().as_bytes())
    }

    pub fn sign(&self, host: &SigningKey) -> Vec<u8> {
        let rh = self.record_hash();
        host.sign(&rh.0).to_bytes().to_vec()
    }
}
