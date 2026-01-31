//! Attestation Reducer — pure scoring from attestation sets.
//!
//! The reducer is a **pure function**: given the same set of attestations,
//! it always produces the same score and the same inputs hash. This makes
//! scoring deterministic across nodes and cache-safe with version-keyed
//! entries.
//!
//! # Architecture
//!
//! ```text
//! Vec<TrustAttestation>  ───>  AttestationReducer::reduce()  ───>  ReducedScore
//!                                     │
//!                                     ├── verify signatures
//!                                     ├── filter expired + future-dated
//!                                     ├── deduplicate (issuer, subject, graph_type)
//!                                     ├── compute average score
//!                                     └── hash inputs for provenance
//! ```

use icn_trust::TrustAttestation;
use sha2::{Digest, Sha256};

/// Current reducer algorithm version.
///
/// Bump this when the scoring algorithm changes. Cache consumers compare
/// this against cached entries to detect stale results.
pub const REDUCER_VERSION: &str = "1.0.0";

/// Maximum clock skew tolerance for future-dated attestations (5 minutes).
///
/// Attestations with `created_at` more than this far ahead of `now` are
/// rejected. This prevents a malicious node from creating attestations with
/// far-future timestamps that persist longer than intended.
const CLOCK_SKEW_TOLERANCE_SECS: u64 = 300;

/// Result of reducing a set of attestations into a single score.
///
/// This is the app-side counterpart to `TrustScoreResult` in kernel-api.
/// It contains the same provenance fields plus internal details that
/// the kernel does not need to see.
#[derive(Debug, Clone)]
pub struct ReducedScore {
    /// Computed trust score (0.0-1.0)
    pub score: f64,
    /// Number of valid attestation inputs after filtering
    pub input_count: u32,
    /// SHA-256 hash of the canonical input representation.
    /// Deterministic: same inputs (regardless of order) → same hash.
    pub inputs_hash: [u8; 32],
    /// Reducer algorithm version
    pub reducer_version: String,
    /// Per-issuer breakdown (issuer DID string → score they assigned)
    pub issuer_scores: Vec<(String, f64)>,
}

/// Pure attestation reducer.
///
/// Stateless — all configuration is provided at construction time.
/// Call `reduce()` with a set of attestations to get a deterministic score.
pub struct AttestationReducer {
    /// Current time for expiration filtering (Unix seconds).
    /// Injected for determinism in tests.
    now: u64,
}

impl AttestationReducer {
    /// Create a reducer with the given "current time" for expiry checks.
    pub fn new(now: u64) -> Self {
        Self { now }
    }

    /// Create a reducer using the actual wall clock.
    pub fn now() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self { now }
    }

    /// Reduce a set of attestations into a single score.
    ///
    /// # Algorithm
    ///
    /// 1. **Filter**: Remove attestations with invalid signatures, expired, or future-dated
    ///    (beyond clock skew tolerance).
    /// 2. **Deduplicate**: For each `(issuer, subject, graph_type)` triple, keep only
    ///    the "winning" attestation using `should_supersede()` ordering.
    /// 3. **Score**: Compute the arithmetic mean of the surviving attestation scores.
    /// 4. **Hash**: Produce a deterministic hash of the surviving attestation set
    ///    (sorted by issuer DID for ordering stability).
    ///
    /// # Determinism guarantee
    ///
    /// Given the same set of attestations and the same `now` timestamp,
    /// `reduce()` ALWAYS returns the same `ReducedScore`, regardless of
    /// input ordering.
    ///
    /// **Platform caveat**: Floating-point division (`sum / count`) may produce
    /// slightly different results across CPU architectures (x86 vs ARM) or
    /// compiler optimization levels. The determinism guarantee holds within
    /// the same platform and compiler version. Cross-platform determinism
    /// would require fixed-point arithmetic (tracked as future work).
    pub fn reduce(&self, attestations: &[TrustAttestation]) -> ReducedScore {
        // Step 1: Filter — only verified, non-expired, non-future attestations
        let valid: Vec<&TrustAttestation> = attestations
            .iter()
            .filter(|a| {
                a.verify().is_ok()
                    && !a.is_expired(self.now)
                    && a.created_at <= self.now + CLOCK_SKEW_TOLERANCE_SECS
            })
            .collect();

        if valid.is_empty() {
            return ReducedScore {
                score: 0.0,
                input_count: 0,
                inputs_hash: [0u8; 32],
                reducer_version: REDUCER_VERSION.to_string(),
                issuer_scores: Vec::new(),
            };
        }

        // Step 2: Deduplicate by (issuer, subject, graph_type)
        // Keep the "winning" attestation per triple.
        let mut best: std::collections::HashMap<(String, String, String), &TrustAttestation> =
            std::collections::HashMap::new();

        for att in &valid {
            let key = (
                att.issuer.to_string(),
                att.subject.to_string(),
                att.graph_type.as_str().to_string(),
            );
            let entry = best.entry(key);
            entry
                .and_modify(|existing| {
                    if att.should_supersede(existing.created_at, existing.score) {
                        *existing = att;
                    }
                })
                .or_insert(att);
        }

        // Step 3: Collect winners sorted by issuer for determinism
        let mut winners: Vec<&&TrustAttestation> = best.values().collect();
        winners.sort_by(|a, b| {
            a.issuer
                .to_string()
                .cmp(&b.issuer.to_string())
                .then_with(|| a.subject.to_string().cmp(&b.subject.to_string()))
                .then_with(|| a.graph_type.as_str().cmp(b.graph_type.as_str()))
        });

        // Step 4: Compute score — arithmetic mean of surviving attestation scores
        let sum: f64 = winners.iter().map(|a| a.score).sum();
        let count = winners.len() as f64;
        let score = (sum / count).clamp(0.0, 1.0);

        // Step 5: Compute deterministic inputs hash
        let inputs_hash = self.hash_inputs(&winners);

        // Step 6: Collect issuer breakdown
        let issuer_scores: Vec<(String, f64)> = winners
            .iter()
            .map(|a| (a.issuer.to_string(), a.score))
            .collect();

        ReducedScore {
            score,
            input_count: winners.len() as u32,
            inputs_hash,
            reducer_version: REDUCER_VERSION.to_string(),
            issuer_scores,
        }
    }

    /// Compute a deterministic SHA-256 hash over the canonical input set.
    ///
    /// The hash covers: for each winning attestation (sorted by issuer),
    /// `issuer || subject || score_le_bytes || created_at_le_bytes || graph_type`.
    fn hash_inputs(&self, winners: &[&&TrustAttestation]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        // Prefix with reducer version so algorithm changes produce different hashes
        hasher.update(REDUCER_VERSION.as_bytes());
        for att in winners {
            hasher.update(att.issuer.as_str().as_bytes());
            hasher.update(att.subject.as_str().as_bytes());
            hasher.update(att.score.to_le_bytes());
            hasher.update(att.created_at.to_le_bytes());
            hasher.update(att.graph_type.as_str().as_bytes());
        }
        hasher.finalize().into()
    }
}

impl ReducedScore {
    /// Convert to the kernel-api `TrustScoreResult` type.
    pub fn to_kernel_result(&self, epoch: u64) -> icn_kernel_api::services::TrustScoreResult {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        icn_kernel_api::services::TrustScoreResult {
            score: self.score,
            epoch,
            computed_at: now,
            input_count: self.input_count,
            inputs_hash: self.inputs_hash,
            reducer_version: self.reducer_version.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_trust::types::TrustGraphType;

    /// Helper to create a signed attestation.
    fn make_signed(
        issuer: &KeyPair,
        subject: &icn_identity::Did,
        score: f64,
        created_at: u64,
    ) -> TrustAttestation {
        let mut att = TrustAttestation::new_typed(
            issuer.did().clone(),
            subject.clone(),
            score,
            TrustGraphType::Social,
        );
        att.created_at = created_at;
        att.sign(issuer).expect("signing failed");
        att
    }

    // ====================================================================
    // DETERMINISM TESTS — the core guarantee of the reducer
    // ====================================================================

    #[test]
    fn deterministic_same_inputs_same_output() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let carol = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        let atts = vec![
            make_signed(&alice, target.did(), 0.8, now - 100),
            make_signed(&bob, target.did(), 0.6, now - 200),
            make_signed(&carol, target.did(), 0.9, now - 50),
        ];

        let reducer = AttestationReducer::new(now);
        let r1 = reducer.reduce(&atts);
        let r2 = reducer.reduce(&atts);

        assert_eq!(r1.score, r2.score);
        assert_eq!(r1.inputs_hash, r2.inputs_hash);
        assert_eq!(r1.input_count, r2.input_count);
    }

    #[test]
    fn deterministic_regardless_of_input_order() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        let a = make_signed(&alice, target.did(), 0.7, now - 100);
        let b = make_signed(&bob, target.did(), 0.3, now - 100);

        let reducer = AttestationReducer::new(now);
        let r1 = reducer.reduce(&[a.clone(), b.clone()]);
        let r2 = reducer.reduce(&[b, a]);

        assert_eq!(r1.score, r2.score);
        assert_eq!(r1.inputs_hash, r2.inputs_hash);
    }

    // ====================================================================
    // FILTERING TESTS
    // ====================================================================

    #[test]
    fn expired_attestations_are_filtered() {
        let alice = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        // Default TTL is 30 days = 2,592,000 seconds
        // Created at 1000, so expires at 2,593,000
        let att = make_signed(&alice, target.did(), 0.8, 1000);
        let now = 2_593_001; // 1 second after expiry

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[att]);

        assert_eq!(result.score, 0.0);
        assert_eq!(result.input_count, 0);
    }

    #[test]
    fn unsigned_attestations_are_filtered() {
        let alice = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        // Unsigned (no sign() call)
        let mut att = TrustAttestation::new(alice.did().clone(), target.did().clone(), 0.5);
        att.created_at = now - 100;

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[att]);

        assert_eq!(result.score, 0.0);
        assert_eq!(result.input_count, 0);
    }

    #[test]
    fn tampered_attestations_are_filtered() {
        let alice = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        let mut att = make_signed(&alice, target.did(), 0.5, now - 100);
        att.score = 0.99; // tamper

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[att]);

        assert_eq!(result.score, 0.0);
        assert_eq!(result.input_count, 0);
    }

    #[test]
    fn all_invalid_attestations_return_zero() {
        let alice = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        // One unsigned, one expired, one tampered — all invalid
        let now = 3_000_000u64; // far enough for 30-day TTL to expire created_at=1

        let mut unsigned = TrustAttestation::new(alice.did().clone(), target.did().clone(), 0.5);
        unsigned.created_at = now - 100;

        let expired = make_signed(&alice, target.did(), 0.8, 1); // created_at=1, expired long ago

        let mut tampered = make_signed(&alice, target.did(), 0.7, now - 100);
        tampered.score = 0.99; // invalidate signature

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[unsigned, expired, tampered]);

        assert_eq!(result.score, 0.0);
        assert_eq!(result.input_count, 0);
        assert_eq!(result.inputs_hash, [0u8; 32]);
    }

    #[test]
    fn future_dated_attestations_are_filtered() {
        let alice = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        // Created 1 hour in the future (well beyond 5-minute tolerance)
        let att = make_signed(&alice, target.did(), 0.8, now + 3600);

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[att]);

        assert_eq!(result.score, 0.0);
        assert_eq!(result.input_count, 0);
    }

    #[test]
    fn within_clock_skew_tolerance_accepted() {
        let alice = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        // Created 2 minutes in the future (within 5-minute tolerance)
        let att = make_signed(&alice, target.did(), 0.8, now + 120);

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[att]);

        assert_eq!(result.input_count, 1);
        assert!((result.score - 0.8).abs() < 0.001);
    }

    // ====================================================================
    // DEDUPLICATION TESTS
    // ====================================================================

    #[test]
    fn dedup_keeps_superseding_attestation() {
        let alice = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        let old = make_signed(&alice, target.did(), 0.3, now - 500);
        let new = make_signed(&alice, target.did(), 0.9, now - 100);

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[old, new]);

        // Only one attestation should survive (same issuer)
        assert_eq!(result.input_count, 1);
        // The newer one (0.9) should win
        assert!((result.score - 0.9).abs() < 0.001);
    }

    // ====================================================================
    // SCORING TESTS
    // ====================================================================

    #[test]
    fn score_is_mean_of_surviving_attestations() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        let a = make_signed(&alice, target.did(), 0.8, now - 100);
        let b = make_signed(&bob, target.did(), 0.4, now - 100);

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[a, b]);

        assert_eq!(result.input_count, 2);
        assert!((result.score - 0.6).abs() < 0.001); // mean of 0.8 and 0.4
    }

    #[test]
    fn empty_input_returns_zero() {
        let reducer = AttestationReducer::new(10_000);
        let result = reducer.reduce(&[]);

        assert_eq!(result.score, 0.0);
        assert_eq!(result.input_count, 0);
        assert_eq!(result.inputs_hash, [0u8; 32]);
    }

    // ====================================================================
    // PROVENANCE TESTS
    // ====================================================================

    #[test]
    fn inputs_hash_changes_when_inputs_differ() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        let a = make_signed(&alice, target.did(), 0.8, now - 100);
        let b = make_signed(&bob, target.did(), 0.4, now - 100);

        let reducer = AttestationReducer::new(now);
        let r1 = reducer.reduce(&[a.clone()]);
        let r2 = reducer.reduce(&[a, b]);

        // Different input sets → different hashes
        assert_ne!(r1.inputs_hash, r2.inputs_hash);
    }

    #[test]
    fn reducer_version_is_set() {
        let alice = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        let a = make_signed(&alice, target.did(), 0.5, now - 100);

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[a]);

        assert_eq!(result.reducer_version, REDUCER_VERSION);
    }

    #[test]
    fn issuer_breakdown_is_populated() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();
        let target = KeyPair::generate().unwrap();

        let now = 10_000u64;
        let a = make_signed(&alice, target.did(), 0.8, now - 100);
        let b = make_signed(&bob, target.did(), 0.4, now - 100);

        let reducer = AttestationReducer::new(now);
        let result = reducer.reduce(&[a, b]);

        assert_eq!(result.issuer_scores.len(), 2);
    }

    // ====================================================================
    // SCHEMA CONTRACT TEST
    // ====================================================================

    #[test]
    fn trust_attestation_schema_contract() {
        // This test catches silent breakage in the TrustAttestation wire format.
        // If serialization changes (field renames, type changes, new required fields),
        // this test will fail and force a conscious decision.

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let mut att = TrustAttestation::new_typed(
            alice.did().clone(),
            bob.did().clone(),
            0.75,
            TrustGraphType::Social,
        );
        att.created_at = 1700000000;
        att.labels = vec!["partner".to_string()];
        att.evidence = vec!["evidence_ref_1".to_string()];
        att.sign(&alice).unwrap();

        // Round-trip through JSON
        let json = serde_json::to_value(&att).unwrap();

        // Assert required fields exist with correct types
        assert!(json["issuer"].is_string(), "issuer must be string");
        assert!(json["subject"].is_string(), "subject must be string");
        assert!(json["score"].is_f64(), "score must be f64");
        assert!(json["ttl_seconds"].is_u64(), "ttl_seconds must be u64");
        assert!(json["created_at"].is_u64(), "created_at must be u64");
        assert!(json["signature"].is_array(), "signature must be array");
        assert!(json["graph_type"].is_string(), "graph_type must be string");
        assert!(json["labels"].is_array(), "labels must be array");
        assert!(json["evidence"].is_array(), "evidence must be array");

        // Verify round-trip deserialization preserves values
        let bytes = serde_json::to_vec(&att).unwrap();
        let deser: TrustAttestation = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(deser.issuer, att.issuer);
        assert_eq!(deser.subject, att.subject);
        assert_eq!(deser.score, att.score);
        assert_eq!(deser.created_at, att.created_at);
        assert_eq!(deser.labels, att.labels);
        assert_eq!(deser.evidence, att.evidence);
        assert_eq!(deser.graph_type, att.graph_type);
        assert!(!deser.signature.is_empty());

        // Verify deserialized attestation still passes signature verification
        assert!(
            deser.verify().is_ok(),
            "round-tripped attestation must verify"
        );
    }
}
