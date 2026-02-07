//! Proof artifacts for the ICN legitimacy architecture.
//!
//! Every significant state change in ICN produces a content-addressed,
//! signed proof artifact. These proofs enable anyone to verify outcomes
//! without trusting any party.
//!
//! # Artifact Types (v0 Set)
//!
//! - `ArtifactReceipt` — proves a blob transfer completed and verified (PR2c)
//!
//! # Self-Authenticating Design
//!
//! Each receipt contains a `receipt_hash` computed at construction time via
//! blake3 binding of all significant fields. `verify_binding()` recomputes
//! from fields and compares, enabling tamper detection without external context.

use serde::{Deserialize, Serialize};

use crate::types::{Did, Hash, Signature};

/// Proof that a blob transfer completed and the content was verified.
///
/// Produced by the requester after all chunks are received, reassembled,
/// and the final blake3 hash matches the declared `blob_hash`.
///
/// The `receipt_hash` is self-authenticating: it is computed from the
/// binding fields at construction and can be re-verified at any time
/// via `verify_binding()`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactReceipt {
    /// blake3 hash of the complete blob
    pub blob_hash: Hash,
    /// DID of the node that served the blob
    pub provider_did: Did,
    /// DID of the node that requested and verified the blob
    pub requester_did: Did,
    /// Nonce binding this receipt to the originating request
    pub request_id: [u8; 32],
    /// Scope identifier for this transfer.
    /// TODO: Converge with canonical ScopeId type once available in kernel-api.
    /// Currently ScopeId lives only in icn-trust (domain crate, cannot import here).
    pub scope_id: String,
    /// Unix timestamp (seconds) when verification completed
    pub verified_at: u64,
    /// blake3 binding hash of all significant fields, computed at construction
    pub receipt_hash: Hash,
    /// Signature by the requester (empty until signed)
    pub signature: Signature,
}

impl ArtifactReceipt {
    /// Create a new receipt with computed binding hash and empty signature.
    pub fn new(
        blob_hash: Hash,
        provider_did: Did,
        requester_did: Did,
        request_id: [u8; 32],
        scope_id: String,
        verified_at: u64,
    ) -> Self {
        let receipt_hash = Self::compute_receipt_hash(
            &request_id,
            &blob_hash,
            &requester_did,
            &provider_did,
            &scope_id,
        );
        Self {
            blob_hash,
            provider_did,
            requester_did,
            request_id,
            scope_id,
            verified_at,
            receipt_hash,
            signature: Signature::new(Vec::new()),
        }
    }

    /// Domain separation tag for receipt hashes.
    ///
    /// Prevents cross-protocol hash collisions if the same field layout is
    /// reused in another proof type.
    pub const DOMAIN_TAG: &[u8] = b"icn:artifact-receipt:v1";

    /// Compute the binding hash from the significant fields.
    ///
    /// This is a pure function used by `new()` and `verify_binding()`.
    /// Variable-length fields are length-prefixed (u64 LE) to prevent
    /// collision attacks from redistributing bytes between adjacent fields.
    /// The domain separation tag is hashed first to prevent cross-protocol
    /// collisions.
    pub fn compute_receipt_hash(
        request_id: &[u8; 32],
        blob_hash: &Hash,
        requester_did: &Did,
        provider_did: &Did,
        scope_id: &str,
    ) -> Hash {
        let mut hasher = blake3::Hasher::new();
        // Domain separation: prevents hash collisions with other proof types
        hasher.update(Self::DOMAIN_TAG);
        // Fixed-length fields: no prefix needed
        hasher.update(request_id);
        hasher.update(blob_hash);
        // Variable-length fields: length-prefix each one
        hasher.update(&(requester_did.len() as u64).to_le_bytes());
        hasher.update(requester_did.as_bytes());
        hasher.update(&(provider_did.len() as u64).to_le_bytes());
        hasher.update(provider_did.as_bytes());
        hasher.update(&(scope_id.len() as u64).to_le_bytes());
        hasher.update(scope_id.as_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Verify that the stored `receipt_hash` matches a fresh computation.
    ///
    /// Returns `true` if the receipt has not been tampered with.
    pub fn verify_binding(&self) -> bool {
        let recomputed = Self::compute_receipt_hash(
            &self.request_id,
            &self.blob_hash,
            &self.requester_did,
            &self.provider_did,
            &self.scope_id,
        );
        self.receipt_hash == recomputed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_receipt() -> ArtifactReceipt {
        ArtifactReceipt::new(
            [0xAA; 32],
            "did:icn:provider123".to_string(),
            "did:icn:requester456".to_string(),
            [0xBB; 32],
            "coop:test-scope".to_string(),
            1700000000,
        )
    }

    #[test]
    fn receipt_hash_determinism() {
        let r1 = make_receipt();
        let r2 = make_receipt();
        assert_eq!(r1.receipt_hash, r2.receipt_hash);
        assert_ne!(r1.receipt_hash, [0u8; 32]);
    }

    #[test]
    fn verify_binding_succeeds_for_fresh_receipt() {
        let receipt = make_receipt();
        assert!(receipt.verify_binding());
    }

    #[test]
    fn tamper_blob_hash_detected() {
        let mut receipt = make_receipt();
        receipt.blob_hash = [0xFF; 32];
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn tamper_provider_did_detected() {
        let mut receipt = make_receipt();
        receipt.provider_did = "did:icn:attacker".to_string();
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn tamper_requester_did_detected() {
        let mut receipt = make_receipt();
        receipt.requester_did = "did:icn:attacker".to_string();
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn tamper_request_id_detected() {
        let mut receipt = make_receipt();
        receipt.request_id = [0xFF; 32];
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn tamper_scope_id_detected() {
        let mut receipt = make_receipt();
        receipt.scope_id = "evil-scope".to_string();
        assert!(!receipt.verify_binding());
    }

    #[test]
    fn signature_starts_empty() {
        let receipt = make_receipt();
        assert!(receipt.signature.as_bytes().is_empty());
    }

    #[test]
    fn domain_tag_is_part_of_hash() {
        // Compute the receipt hash the normal way (with domain tag)
        let receipt = make_receipt();
        let with_tag = receipt.receipt_hash;

        // Compute manually without domain tag — must differ
        let mut hasher = blake3::Hasher::new();
        // Deliberately omit: hasher.update(ArtifactReceipt::DOMAIN_TAG);
        hasher.update(&receipt.request_id);
        hasher.update(&receipt.blob_hash);
        hasher.update(&(receipt.requester_did.len() as u64).to_le_bytes());
        hasher.update(receipt.requester_did.as_bytes());
        hasher.update(&(receipt.provider_did.len() as u64).to_le_bytes());
        hasher.update(receipt.provider_did.as_bytes());
        hasher.update(&(receipt.scope_id.len() as u64).to_le_bytes());
        hasher.update(receipt.scope_id.as_bytes());
        let without_tag: Hash = *hasher.finalize().as_bytes();

        assert_ne!(with_tag, without_tag, "domain tag must affect hash output");
    }

    #[test]
    fn length_prefix_prevents_field_collision() {
        // Without length prefixes, these two would hash identically because
        // the concatenation of provider_did || scope_id is the same bytes.
        let r1 = ArtifactReceipt::new(
            [0xAA; 32],
            "did:icn:ABC".to_string(),
            "did:icn:requester".to_string(),
            [0xBB; 32],
            "XYZ".to_string(),
            1700000000,
        );
        let r2 = ArtifactReceipt::new(
            [0xAA; 32],
            "did:icn:ABCXYZ".to_string(),
            "did:icn:requester".to_string(),
            [0xBB; 32],
            "".to_string(),
            1700000000,
        );
        assert_ne!(r1.receipt_hash, r2.receipt_hash);
        assert!(r1.verify_binding());
        assert!(r2.verify_binding());
    }
}
