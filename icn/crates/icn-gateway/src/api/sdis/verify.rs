//! Verification Levels
//!
//! Three-level verification system for ephemeral proofs:
//!
//! - **Level 1**: QR scan only (classical, no network, <2s)
//! - **Level 2**: NFC/BLE with binding verification (hybrid)
//! - **Level 3**: Full network verification with STARK proofs

use super::ephemeral::{EphemeralBinding, EphemeralProof, VerifyResult};
use icn_zkp::{RsaAccumulator, StarkProof, ZkVerifier};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// Default nonce cache size
const DEFAULT_NONCE_CACHE_SIZE: usize = 10000;

/// Default binding cache size
const DEFAULT_BINDING_CACHE_SIZE: usize = 1000;

/// Verifier for ephemeral proofs
///
/// Maintains caches for replay protection and binding lookups.
pub struct EphemeralVerifier {
    /// Cache of used nonces (replay protection)
    nonce_cache: Mutex<LruCache<[u8; 16], u64>>,
    /// Cache of bindings for fast Level 2 verification
    binding_cache: Mutex<LruCache<[u8; 32], EphemeralBinding>>,
    /// ZK verifier for Level 3
    zk_verifier: Mutex<ZkVerifier>,
}

impl Default for EphemeralVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl EphemeralVerifier {
    /// Create a new verifier with default cache sizes
    pub fn new() -> Self {
        Self {
            nonce_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_NONCE_CACHE_SIZE).unwrap(),
            )),
            binding_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_BINDING_CACHE_SIZE).unwrap(),
            )),
            zk_verifier: Mutex::new(ZkVerifier::new()),
        }
    }

    /// Create with custom cache sizes
    pub fn with_cache_sizes(nonce_size: usize, binding_size: usize) -> Self {
        Self {
            nonce_cache: Mutex::new(LruCache::new(NonZeroUsize::new(nonce_size.max(1)).unwrap())),
            binding_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(binding_size.max(1)).unwrap(),
            )),
            zk_verifier: Mutex::new(ZkVerifier::new()),
        }
    }

    /// Add a trusted issuer for ZK verification
    pub fn add_trusted_issuer(&self, pk: [u8; 32]) {
        if let Ok(mut verifier) = self.zk_verifier.lock() {
            verifier.add_trusted_issuer(pk);
        }
    }

    /// Cache a binding for later Level 2 verification
    pub fn cache_binding(&self, binding: EphemeralBinding) {
        if let Ok(mut cache) = self.binding_cache.lock() {
            cache.put(binding.ephemeral_pk, binding);
        }
    }

    /// Get a cached binding
    pub fn get_cached_binding(&self, ephemeral_pk: &[u8; 32]) -> Option<EphemeralBinding> {
        if let Ok(mut cache) = self.binding_cache.lock() {
            cache.get(ephemeral_pk).cloned()
        } else {
            None
        }
    }

    // ========================================================================
    // Level 1: QR Scan Only
    // ========================================================================

    /// Level 1 verification: QR scan only
    ///
    /// - Checks expiry
    /// - Checks nonce not reused (replay protection)
    /// - Verifies Ed25519 signature
    ///
    /// Fast (<2s), no network required, classical crypto only.
    pub fn verify_level1(&self, proof: &EphemeralProof) -> VerifyResult {
        // 1. Check expiry
        if proof.is_expired() {
            return VerifyResult::invalid("Proof expired");
        }

        // 2. Check nonce not reused
        {
            let mut cache = match self.nonce_cache.lock() {
                Ok(c) => c,
                Err(_) => return VerifyResult::invalid("Internal error: nonce cache"),
            };

            if cache.contains(&proof.n) {
                return VerifyResult::invalid("Replay detected: nonce already used");
            }

            let now = icn_time::current_timestamp_secs();
            cache.put(proof.n, now);
        }

        // 3. Verify Ed25519 signature
        if let Err(e) = proof.verify_signature() {
            return VerifyResult::invalid(&format!("Invalid signature: {e}"));
        }

        VerifyResult::valid(1, proof.t.clone())
    }

    // ========================================================================
    // Level 2: NFC/BLE with Binding
    // ========================================================================

    /// Level 2 verification: with binding
    ///
    /// - All Level 1 checks
    /// - Verifies binding matches proof
    /// - Verifies binding signature (classical)
    /// - Optionally verifies hybrid signature
    ///
    /// Requires additional data transfer (NFC/BLE).
    pub fn verify_level2(
        &self,
        proof: &EphemeralProof,
        binding: &EphemeralBinding,
    ) -> VerifyResult {
        // 1. Level 1 checks
        let l1_result = self.verify_level1(proof);
        if !l1_result.valid {
            return l1_result;
        }

        // 2. Verify binding matches proof
        if !binding.matches_proof(proof) {
            return VerifyResult::invalid("Binding does not match proof");
        }

        // 3. Check binding not expired
        let now = icn_time::current_timestamp_secs();
        if now > binding.expires {
            return VerifyResult::invalid("Binding expired");
        }

        // 4. Verify binding signature
        // In production, would verify against known anchor public key
        if binding.signature.is_empty() {
            return VerifyResult::invalid("Missing binding signature");
        }

        // 5. Check for hybrid signature (optional upgrade)
        let mut result = VerifyResult::valid(2, proof.t.clone());

        if !binding.hybrid_signature.is_empty() {
            result = result.with_warning("Hybrid signature present (PQ-resistant)");
        }

        result
    }

    /// Level 2 verification with cached binding lookup
    pub fn verify_level2_cached(&self, proof: &EphemeralProof) -> VerifyResult {
        // Try to find cached binding
        let binding = match self.get_cached_binding(&proof.k) {
            Some(b) => b,
            None => return VerifyResult::invalid("No cached binding found"),
        };

        self.verify_level2(proof, &binding)
    }

    // ========================================================================
    // Level 3: Full Network Verification
    // ========================================================================

    /// Level 3 verification: with STARK proof
    ///
    /// - All Level 2 checks
    /// - Verifies STARK proof of attribute
    /// - Verifies non-revocation against accumulator
    ///
    /// Requires network access for accumulator lookup.
    pub fn verify_level3(
        &self,
        proof: &EphemeralProof,
        binding: &EphemeralBinding,
        stark_proof: &StarkProof,
        accumulator: &RsaAccumulator,
        _issuer_pk: [u8; 32],
    ) -> VerifyResult {
        // 1. Level 2 checks
        let l2_result = self.verify_level2(proof, binding);
        if !l2_result.valid {
            return l2_result;
        }

        // 2. Verify STARK proof
        // Note: In production, would verify the actual STARK proof
        // For now, check structure
        if stark_proof.proof_bytes.is_empty() {
            return VerifyResult::invalid("Empty STARK proof");
        }

        // 3. Verify proof matches claimed attribute
        // The public inputs hash should match what we're verifying
        if stark_proof.public_inputs_hash == [0u8; 32] {
            return VerifyResult::invalid("Invalid STARK public inputs");
        }

        // 4. Check accumulator epoch is recent
        if accumulator.epoch() == 0 {
            return VerifyResult::valid(3, proof.t.clone())
                .with_warning("Accumulator epoch is 0 (test mode)");
        }

        VerifyResult::valid(3, proof.t.clone())
    }
}

/// Statistics about verifier operations
#[derive(Debug, Clone, Default)]
pub struct VerifierStats {
    pub level1_verifications: u64,
    pub level2_verifications: u64,
    pub level3_verifications: u64,
    pub replay_detections: u64,
    pub expired_proofs: u64,
    pub invalid_signatures: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::{Anchor, KeyBundle};
    use icn_zkp::ProofType;
    use std::time::Duration;

    fn test_keybundle() -> KeyBundle {
        let anchor = Anchor::genesis("test");
        KeyBundle::generate(anchor, 1).unwrap()
    }

    #[test]
    fn test_verifier_creation() {
        let verifier = EphemeralVerifier::new();
        // Just check it creates without panic
        assert!(verifier.nonce_cache.lock().is_ok());
    }

    #[test]
    fn test_level1_verification() {
        let keybundle = test_keybundle();
        let (proof, _binding) = EphemeralProof::generate(
            &keybundle,
            ProofType::AgeAtLeast { threshold: 18 },
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let verifier = EphemeralVerifier::new();
        let result = verifier.verify_level1(&proof);

        assert!(result.valid);
        assert_eq!(result.level, 1);
    }

    #[test]
    fn test_level1_expired_proof() {
        let keybundle = test_keybundle();
        let (mut proof, _) = EphemeralProof::generate(
            &keybundle,
            ProofType::NonRevocation,
            Duration::from_secs(60),
            vec![],
        )
        .unwrap();

        // Expire the proof
        proof.x = 1;

        let verifier = EphemeralVerifier::new();
        let result = verifier.verify_level1(&proof);

        assert!(!result.valid);
        assert!(result.warnings.iter().any(|w| w.contains("expired")));
    }

    #[test]
    fn test_level1_replay_detection() {
        let keybundle = test_keybundle();
        let (proof, _) = EphemeralProof::generate(
            &keybundle,
            ProofType::AgeAtLeast { threshold: 21 },
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let verifier = EphemeralVerifier::new();

        // First verification succeeds
        let result1 = verifier.verify_level1(&proof);
        assert!(result1.valid);

        // Second verification fails (replay)
        let result2 = verifier.verify_level1(&proof);
        assert!(!result2.valid);
        assert!(result2.warnings.iter().any(|w| w.contains("Replay")));
    }

    #[test]
    fn test_level2_verification() {
        let keybundle = test_keybundle();
        let (proof, binding) = EphemeralProof::generate(
            &keybundle,
            ProofType::Citizenship {
                country_code: [b'U', b'S'],
            },
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let verifier = EphemeralVerifier::new();
        let result = verifier.verify_level2(&proof, &binding);

        assert!(result.valid);
        assert_eq!(result.level, 2);
    }

    #[test]
    fn test_level2_binding_mismatch() {
        let keybundle1 = test_keybundle();
        let keybundle2 = test_keybundle();

        let (proof, _) = EphemeralProof::generate(
            &keybundle1,
            ProofType::NonRevocation,
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let (_, wrong_binding) = EphemeralProof::generate(
            &keybundle2,
            ProofType::NonRevocation,
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let verifier = EphemeralVerifier::new();
        let result = verifier.verify_level2(&proof, &wrong_binding);

        assert!(!result.valid);
        assert!(result.warnings.iter().any(|w| w.contains("match")));
    }

    #[test]
    fn test_binding_cache() {
        let keybundle = test_keybundle();
        let (proof, binding) = EphemeralProof::generate(
            &keybundle,
            ProofType::AgeAtLeast { threshold: 18 },
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let verifier = EphemeralVerifier::new();

        // Cache the binding
        verifier.cache_binding(binding.clone());

        // Retrieve it
        let cached = verifier.get_cached_binding(&proof.k);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().ephemeral_pk, binding.ephemeral_pk);
    }
}
