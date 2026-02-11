//! Canonical receipt traits for cross-domain provenance.
//!
//! All receipt-like objects (governance decisions, allocations, settlements)
//! implement `CanonicalReceipt` to enable:
//! - Cross-node equality via deterministic hashing
//! - Provenance chain verification
//! - Unified audit trail

use serde::{Deserialize, Serialize};

/// 32-byte hash for canonical equality anchors
pub type Hash = [u8; 32];

/// Node-local receipt identifier (may differ across nodes)
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiptId(pub String);

impl ReceiptId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ReceiptId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Upstream provenance anchors linking receipts in a chain
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceAnchors {
    /// Hashes of upstream receipts that authorized this one
    pub upstream_hashes: Vec<Hash>,
}

impl ProvenanceAnchors {
    /// Create empty provenance (root receipt)
    pub fn root() -> Self {
        Self {
            upstream_hashes: Vec::new(),
        }
    }

    /// Create provenance with a single parent hash
    pub fn from_parent(parent_hash: Hash) -> Self {
        Self {
            upstream_hashes: vec![parent_hash],
        }
    }

    /// Create provenance with multiple parent hashes
    pub fn from_parents(hashes: Vec<Hash>) -> Self {
        Self {
            upstream_hashes: hashes,
        }
    }

    /// Add a parent hash
    pub fn add_parent(&mut self, hash: Hash) {
        self.upstream_hashes.push(hash);
    }
}

/// Trait for receipt-like objects with canonical equality semantics.
///
/// # Invariants
///
/// 1. `canonical_hash()` MUST NOT depend on `receipt_id()` (node-local)
/// 2. `canonical_hash()` MUST be deterministic (same content → same hash)
/// 3. Serialization must be canonicalized before hashing
/// 4. Two nodes with identical inputs MUST compute identical hashes
pub trait CanonicalReceipt {
    /// Canonical cross-node equality anchor.
    ///
    /// This hash is computed from the deterministic serialization of
    /// all semantically-meaningful fields (excluding node-local IDs).
    fn canonical_hash(&self) -> Hash;

    /// Node-local identifier (may differ across nodes).
    ///
    /// This is for local indexing/lookup only. Do not use for
    /// cross-node equality checks.
    fn receipt_id(&self) -> &ReceiptId;

    /// Upstream provenance anchors.
    ///
    /// Links this receipt to the chain of decisions/receipts that
    /// authorized it. Used for audit trail verification.
    fn provenance(&self) -> &ProvenanceAnchors;
}

/// Compute canonical hash using Blake3 over deterministic serialization.
///
/// Use this helper to implement `canonical_hash()` consistently.
pub fn compute_canonical_hash<T: Serialize>(value: &T) -> Hash {
    // Use bincode for deterministic serialization
    let bytes =
        bincode::serialize(value).expect("serialization should not fail for valid receipts");
    blake3::hash(&bytes).into()
}

// ============================================================================
// AllocationReceipt (B3: Decision → Intents bridge)
// ============================================================================

use crate::economics::SettlementIntent;
use crate::ScopeLevel;

/// Receipt for resource allocation decisions.
///
/// Links governance decisions to economic intents. A governance decision
/// produces an AllocationReceipt which contains one or more SettlementIntents.
///
/// The chain is: DecisionReceipt → AllocationReceipt → SettlementIntent(s) → LedgerEntry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationReceipt {
    /// Node-local receipt identifier
    #[serde(skip)]
    pub receipt_id: ReceiptId,

    /// Canonical hash of the governance decision that authorized this allocation
    pub decision_hash: Hash,

    /// Settlement intents (economic actions) resulting from this allocation
    pub intents: Vec<SettlementIntent>,

    /// Scope at which this allocation occurs
    pub scope: ScopeLevel,

    /// Creation timestamp (Unix seconds)
    pub created_at: u64,

    /// Provenance anchors (links to decision receipt)
    pub provenance: ProvenanceAnchors,

    /// Optional signature over the receipt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Vec<u8>>,
}

/// Canonical content for AllocationReceipt hashing
#[derive(Serialize)]
struct AllocationReceiptCanonical<'a> {
    decision_hash: &'a Hash,
    // For intents, we include their canonical hashes, not full content
    intent_hashes: Vec<Hash>,
    scope: ScopeLevel,
    created_at: u64,
    provenance: &'a ProvenanceAnchors,
    // NOTE: receipt_id and signature are excluded
}

impl CanonicalReceipt for AllocationReceipt {
    fn canonical_hash(&self) -> Hash {
        // Sort intent hashes to ensure order-independent determinism
        // This prevents canonical hash divergence when intents are added in different orders
        let mut intent_hashes: Vec<Hash> =
            self.intents.iter().map(|i| i.canonical_hash()).collect();
        intent_hashes.sort();

        let canonical = AllocationReceiptCanonical {
            decision_hash: &self.decision_hash,
            intent_hashes,
            scope: self.scope,
            created_at: self.created_at,
            provenance: &self.provenance,
        };
        compute_canonical_hash(&canonical)
    }

    fn receipt_id(&self) -> &ReceiptId {
        &self.receipt_id
    }

    fn provenance(&self) -> &ProvenanceAnchors {
        &self.provenance
    }
}

impl AllocationReceipt {
    /// Create a new allocation receipt
    pub fn new(decision_hash: Hash, scope: ScopeLevel) -> Self {
        Self {
            receipt_id: ReceiptId::default(),
            decision_hash,
            intents: Vec::new(),
            scope,
            created_at: 0,
            provenance: ProvenanceAnchors::from_parent(decision_hash),
            signature: None,
        }
    }

    /// Add a settlement intent to this allocation
    pub fn add_intent(mut self, intent: SettlementIntent) -> Self {
        self.intents.push(intent);
        self
    }

    /// Add multiple settlement intents
    pub fn with_intents(mut self, intents: Vec<SettlementIntent>) -> Self {
        self.intents = intents;
        self
    }

    /// Set the creation timestamp
    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.created_at = timestamp;
        self
    }

    /// Set the node-local receipt ID
    pub fn with_receipt_id(mut self, id: impl Into<String>) -> Self {
        self.receipt_id = ReceiptId::new(id);
        self
    }

    /// Sign this receipt (stores raw signature bytes)
    pub fn with_signature(mut self, sig: Vec<u8>) -> Self {
        self.signature = Some(sig);
        self
    }

    /// Get all intent canonical hashes (sorted for determinism)
    ///
    /// Returns hashes in sorted order to match canonical_hash() behavior.
    /// This ensures callers don't reintroduce order-dependent logic.
    pub fn intent_hashes(&self) -> Vec<Hash> {
        let mut hashes: Vec<Hash> = self.intents.iter().map(|i| i.canonical_hash()).collect();
        hashes.sort();
        hashes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test receipt for verifying trait invariants
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestReceipt {
        // Excluded from canonical hash (node-local)
        #[serde(skip)]
        id: ReceiptId,

        // Included in canonical hash
        decision_hash: Hash,
        amount: u64,
        currency: String,
        provenance: ProvenanceAnchors,
    }

    /// Canonical content for hashing (excludes receipt_id)
    #[derive(Serialize)]
    struct TestReceiptCanonical<'a> {
        decision_hash: &'a Hash,
        amount: u64,
        currency: &'a str,
        provenance: &'a ProvenanceAnchors,
    }

    impl CanonicalReceipt for TestReceipt {
        fn canonical_hash(&self) -> Hash {
            let canonical = TestReceiptCanonical {
                decision_hash: &self.decision_hash,
                amount: self.amount,
                currency: &self.currency,
                provenance: &self.provenance,
            };
            compute_canonical_hash(&canonical)
        }

        fn receipt_id(&self) -> &ReceiptId {
            &self.id
        }

        fn provenance(&self) -> &ProvenanceAnchors {
            &self.provenance
        }
    }

    impl Default for TestReceipt {
        fn default() -> Self {
            Self {
                id: ReceiptId::new("default"),
                decision_hash: [0u8; 32],
                amount: 0,
                currency: String::new(),
                provenance: ProvenanceAnchors::root(),
            }
        }
    }

    #[test]
    fn test_canonical_hash_stability() {
        let receipt = TestReceipt {
            id: ReceiptId::new("node-a-local-id-001"),
            decision_hash: [1u8; 32],
            amount: 1000,
            currency: "hours".to_string(),
            provenance: ProvenanceAnchors::root(),
        };

        let hash1 = receipt.canonical_hash();
        let hash2 = receipt.canonical_hash();

        assert_eq!(hash1, hash2, "canonical hash must be stable");
    }

    #[test]
    fn test_receipt_id_does_not_affect_canonical_hash() {
        let receipt_a = TestReceipt {
            id: ReceiptId::new("node-a-local-id-001"),
            decision_hash: [1u8; 32],
            amount: 1000,
            currency: "hours".to_string(),
            provenance: ProvenanceAnchors::root(),
        };

        let receipt_b = TestReceipt {
            id: ReceiptId::new("node-b-completely-different-id"),
            decision_hash: [1u8; 32],
            amount: 1000,
            currency: "hours".to_string(),
            provenance: ProvenanceAnchors::root(),
        };

        assert_eq!(
            receipt_a.canonical_hash(),
            receipt_b.canonical_hash(),
            "different receipt_ids must produce same canonical_hash"
        );
    }

    #[test]
    fn test_content_change_changes_hash() {
        let receipt_a = TestReceipt {
            id: ReceiptId::new("id"),
            decision_hash: [1u8; 32],
            amount: 1000,
            currency: "hours".to_string(),
            provenance: ProvenanceAnchors::root(),
        };

        let receipt_b = TestReceipt {
            id: ReceiptId::new("id"),
            decision_hash: [1u8; 32],
            amount: 2000, // Different amount
            currency: "hours".to_string(),
            provenance: ProvenanceAnchors::root(),
        };

        assert_ne!(
            receipt_a.canonical_hash(),
            receipt_b.canonical_hash(),
            "different content must produce different canonical_hash"
        );
    }

    #[test]
    fn test_provenance_included_in_hash() {
        let receipt_a = TestReceipt {
            id: ReceiptId::new("id"),
            decision_hash: [1u8; 32],
            amount: 1000,
            currency: "hours".to_string(),
            provenance: ProvenanceAnchors::root(),
        };

        let receipt_b = TestReceipt {
            id: ReceiptId::new("id"),
            decision_hash: [1u8; 32],
            amount: 1000,
            currency: "hours".to_string(),
            provenance: ProvenanceAnchors::from_parent([2u8; 32]),
        };

        assert_ne!(
            receipt_a.canonical_hash(),
            receipt_b.canonical_hash(),
            "different provenance must produce different canonical_hash"
        );
    }

    #[test]
    fn test_serde_roundtrip_preserves_hash() {
        let original = TestReceipt {
            id: ReceiptId::new("test-id"),
            decision_hash: [42u8; 32],
            amount: 5000,
            currency: "credits".to_string(),
            provenance: ProvenanceAnchors::from_parents(vec![[1u8; 32], [2u8; 32]]),
        };

        let original_hash = original.canonical_hash();

        // Serialize and deserialize
        let json = serde_json::to_string(&original).unwrap();
        let mut restored: TestReceipt = serde_json::from_str(&json).unwrap();

        // Receipt ID is skipped in serde, so restore it manually for comparison
        restored.id = ReceiptId::new("different-id-after-restore");

        assert_eq!(
            original_hash,
            restored.canonical_hash(),
            "serde roundtrip must not change canonical_hash"
        );
    }

    #[test]
    fn test_provenance_anchors_builder() {
        let mut prov = ProvenanceAnchors::root();
        assert!(prov.upstream_hashes.is_empty());

        prov.add_parent([1u8; 32]);
        assert_eq!(prov.upstream_hashes.len(), 1);

        let prov2 = ProvenanceAnchors::from_parents(vec![[1u8; 32], [2u8; 32]]);
        assert_eq!(prov2.upstream_hashes.len(), 2);
    }

    // ========================================================================
    // AllocationReceipt Tests
    // ========================================================================

    use crate::economics::SettlementIntent;

    fn make_test_intent(amount: u64) -> SettlementIntent {
        SettlementIntent::new(
            "dec-123", [1u8; 32], "treasury", "member-1", amount, "hours",
        )
        .with_timestamp(1000000)
    }

    #[test]
    fn test_allocation_receipt_canonical_hash_stability() {
        let receipt = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(make_test_intent(100));

        let hash1 = receipt.canonical_hash();
        let hash2 = receipt.canonical_hash();

        assert_eq!(hash1, hash2, "canonical hash must be stable across calls");
    }

    #[test]
    fn test_allocation_receipt_id_does_not_affect_hash() {
        let receipt_a = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .with_receipt_id("receipt-A")
            .add_intent(make_test_intent(100));

        let receipt_b = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .with_receipt_id("receipt-B-different")
            .add_intent(make_test_intent(100));

        assert_eq!(
            receipt_a.canonical_hash(),
            receipt_b.canonical_hash(),
            "receipt_id must not affect canonical_hash"
        );
    }

    #[test]
    fn test_allocation_receipt_signature_does_not_affect_hash() {
        let receipt_a = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(make_test_intent(100));

        let receipt_b = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(make_test_intent(100))
            .with_signature(vec![1, 2, 3, 4, 5]);

        assert_eq!(
            receipt_a.canonical_hash(),
            receipt_b.canonical_hash(),
            "signature must not affect canonical_hash"
        );
    }

    #[test]
    fn test_allocation_receipt_decision_hash_affects_hash() {
        let receipt_a = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(make_test_intent(100));

        let receipt_b = AllocationReceipt::new([99u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(make_test_intent(100));

        assert_ne!(
            receipt_a.canonical_hash(),
            receipt_b.canonical_hash(),
            "different decision_hash must produce different canonical_hash"
        );
    }

    #[test]
    fn test_allocation_receipt_intents_affect_hash() {
        let receipt_a = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(make_test_intent(100));

        let receipt_b = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(make_test_intent(200));

        assert_ne!(
            receipt_a.canonical_hash(),
            receipt_b.canonical_hash(),
            "different intents must produce different canonical_hash"
        );
    }

    #[test]
    fn test_allocation_receipt_scope_affects_hash() {
        let receipt_a = AllocationReceipt::new([42u8; 32], ScopeLevel::Org).with_timestamp(1000);

        let receipt_b =
            AllocationReceipt::new([42u8; 32], ScopeLevel::Federation).with_timestamp(1000);

        assert_ne!(
            receipt_a.canonical_hash(),
            receipt_b.canonical_hash(),
            "different scope must produce different canonical_hash"
        );
    }

    #[test]
    fn test_allocation_receipt_intent_hashes() {
        let receipt = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .add_intent(make_test_intent(100))
            .add_intent(make_test_intent(200));

        let hashes = receipt.intent_hashes();
        assert_eq!(hashes.len(), 2);
        assert_ne!(
            hashes[0], hashes[1],
            "different intents should have different hashes"
        );
    }

    #[test]
    fn test_allocation_receipt_serde_roundtrip() {
        let original = AllocationReceipt::new([42u8; 32], ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(make_test_intent(100));

        let original_hash = original.canonical_hash();

        let json = serde_json::to_string(&original).unwrap();
        let restored: AllocationReceipt = serde_json::from_str(&json).unwrap();

        assert_eq!(
            original_hash,
            restored.canonical_hash(),
            "serde roundtrip must preserve canonical_hash"
        );
    }

    #[test]
    fn test_allocation_receipt_provenance_chain() {
        let decision_hash = [42u8; 32];
        let receipt = AllocationReceipt::new(decision_hash, ScopeLevel::Org);

        // Provenance should link to decision
        assert!(
            receipt
                .provenance()
                .upstream_hashes
                .contains(&decision_hash),
            "provenance must include decision_hash"
        );
    }

    #[test]
    fn test_allocation_receipt_canonical_hash_order_independent() {
        // D0 determinism hardening: intent order must not affect canonical hash
        // Two nodes adding the same intents in different order must produce the same hash

        let decision_hash = [42u8; 32];

        // Create intents with different amounts so they have different hashes
        let intent_100 = make_test_intent(100);
        let intent_200 = make_test_intent(200);
        let intent_300 = make_test_intent(300);

        // Node A adds intents in order: 100, 200, 300
        let receipt_a = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(intent_100.clone())
            .add_intent(intent_200.clone())
            .add_intent(intent_300.clone());

        // Node B adds intents in reverse order: 300, 200, 100
        let receipt_b = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(intent_300.clone())
            .add_intent(intent_200.clone())
            .add_intent(intent_100.clone());

        // Node C adds intents in different order: 200, 100, 300
        let receipt_c = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(intent_200)
            .add_intent(intent_100)
            .add_intent(intent_300);

        let hash_a = receipt_a.canonical_hash();
        let hash_b = receipt_b.canonical_hash();
        let hash_c = receipt_c.canonical_hash();

        assert_eq!(
            hash_a, hash_b,
            "canonical_hash must be order-independent (A vs B)"
        );
        assert_eq!(
            hash_b, hash_c,
            "canonical_hash must be order-independent (B vs C)"
        );
        assert_eq!(
            hash_a, hash_c,
            "canonical_hash must be order-independent (A vs C)"
        );

        println!("✅ AllocationReceipt canonical_hash is order-independent");
        println!("   Hash A (100,200,300): {}", hex::encode(&hash_a[..16]));
        println!("   Hash B (300,200,100): {}", hex::encode(&hash_b[..16]));
        println!("   Hash C (200,100,300): {}", hex::encode(&hash_c[..16]));

        // Also verify intent_hashes() returns sorted output
        // This ensures callers don't reintroduce order-dependent logic
        let hashes_a = receipt_a.intent_hashes();
        let hashes_b = receipt_b.intent_hashes();
        let hashes_c = receipt_c.intent_hashes();

        assert_eq!(
            hashes_a, hashes_b,
            "intent_hashes() must return sorted output (A vs B)"
        );
        assert_eq!(
            hashes_b, hashes_c,
            "intent_hashes() must return sorted output (B vs C)"
        );

        // Verify the output is actually sorted
        let mut sorted = hashes_a.clone();
        sorted.sort();
        assert_eq!(hashes_a, sorted, "intent_hashes() must be sorted");

        println!("✅ intent_hashes() returns sorted output");
    }
}
