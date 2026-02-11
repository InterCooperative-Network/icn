//! Economic settlement types for declarative economic intents.
//!
//! A `SettlementIntent` represents a desired economic action that the
//! ledger service will execute. It is declarative (what should happen)
//! rather than imperative (how to do it).

use crate::receipts::{
    compute_canonical_hash, CanonicalReceipt, Hash, ProvenanceAnchors, ReceiptId,
};
use crate::ScopeLevel;
use serde::{Deserialize, Serialize};

/// Asset classification for economic semantics.
///
/// This enum encodes what kind of "claim on value" is being transferred.
/// ICN's thesis: money is not value, it's a claim on value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetType {
    /// Standard fungible asset (mutual credit, currency)
    Fungible,

    /// Single-use asset (compute hours, one-time vouchers)
    Consumable {
        /// Optional expiration timestamp (Unix seconds)
        expires_at: Option<u64>,
    },

    /// Asset that loses value over time (equipment, licenses)
    Depreciating {
        /// Depreciation schedule
        schedule: DepreciationSchedule,
    },

    /// Asset that transforms into another (raw materials → products)
    Transformable {
        /// Output asset types and ratios
        outputs: Vec<(String, u64)>, // (asset_name, ratio)
    },

    /// Non-storable value (labor hours, services rendered)
    Service {
        /// Duration in seconds, if applicable
        duration_secs: Option<u64>,
    },

    /// Claim on future performance (IOUs, bonds, receivables)
    Claim {
        /// Due date (Unix timestamp)
        due_by: Option<u64>,
        /// Conditions for claim fulfillment
        conditions: Option<String>,
    },
}

impl Default for AssetType {
    fn default() -> Self {
        Self::Fungible
    }
}

/// Depreciation schedule for depreciating assets
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DepreciationSchedule {
    /// Linear depreciation over time
    Linear {
        /// Total lifespan in seconds
        lifespan_secs: u64,
    },
    /// Fixed percentage per period
    Percentage {
        /// Rate per period (basis points, 100 = 1%)
        rate_bps: u32,
        /// Period in seconds
        period_secs: u64,
    },
    /// Custom schedule (opaque, for future extensibility)
    Custom(String),
}

/// Declarative settlement intent - what economic action should happen.
///
/// The kernel consumes this; the LedgerService executes it.
/// This is declarative (describes outcome) not imperative (describes steps).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementIntent {
    /// Node-local intent identifier
    #[serde(skip)]
    pub intent_id: ReceiptId,

    /// Decision receipt that authorized this intent
    pub decision_receipt_id: String,

    /// Canonical decision hash (cross-node anchor)
    pub decision_hash: Hash,

    /// Asset type being settled
    pub asset: AssetType,

    /// Source account (DID or account identifier)
    pub from: String,

    /// Destination account (DID or account identifier)
    pub to: String,

    /// Amount in smallest unit
    pub amount: u64,

    /// Currency/unit symbol
    pub unit: String,

    /// Scope at which this settlement occurs
    pub scope: ScopeLevel,

    /// Optional memo (excluded from canonical hash)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,

    /// Provenance anchors
    pub provenance: ProvenanceAnchors,

    /// Creation timestamp
    pub created_at: u64,
}

/// Canonical content for hashing (excludes node-local fields)
#[derive(Serialize)]
struct SettlementIntentCanonical<'a> {
    decision_receipt_id: &'a str,
    decision_hash: &'a Hash,
    asset: &'a AssetType,
    from: &'a str,
    to: &'a str,
    amount: u64,
    unit: &'a str,
    scope: ScopeLevel,
    provenance: &'a ProvenanceAnchors,
    created_at: u64,
    // NOTE: memo is intentionally excluded
}

impl CanonicalReceipt for SettlementIntent {
    fn canonical_hash(&self) -> Hash {
        let canonical = SettlementIntentCanonical {
            decision_receipt_id: &self.decision_receipt_id,
            decision_hash: &self.decision_hash,
            asset: &self.asset,
            from: &self.from,
            to: &self.to,
            amount: self.amount,
            unit: &self.unit,
            scope: self.scope,
            provenance: &self.provenance,
            created_at: self.created_at,
        };
        compute_canonical_hash(&canonical)
    }

    fn receipt_id(&self) -> &ReceiptId {
        &self.intent_id
    }

    fn provenance(&self) -> &ProvenanceAnchors {
        &self.provenance
    }
}

impl SettlementIntent {
    /// Create a new settlement intent
    pub fn new(
        decision_receipt_id: impl Into<String>,
        decision_hash: Hash,
        from: impl Into<String>,
        to: impl Into<String>,
        amount: u64,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            intent_id: ReceiptId::default(),
            decision_receipt_id: decision_receipt_id.into(),
            decision_hash,
            asset: AssetType::Fungible,
            from: from.into(),
            to: to.into(),
            amount,
            unit: unit.into(),
            scope: ScopeLevel::Org,
            memo: None,
            provenance: ProvenanceAnchors::from_parent(decision_hash),
            created_at: 0,
        }
    }

    /// Set the asset type
    pub fn with_asset(mut self, asset: AssetType) -> Self {
        self.asset = asset;
        self
    }

    /// Set the scope
    pub fn with_scope(mut self, scope: ScopeLevel) -> Self {
        self.scope = scope;
        self
    }

    /// Set a memo (not included in canonical hash)
    pub fn with_memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = Some(memo.into());
        self
    }

    /// Set the creation timestamp
    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.created_at = timestamp;
        self
    }

    /// Set the node-local intent ID
    pub fn with_intent_id(mut self, id: impl Into<String>) -> Self {
        self.intent_id = ReceiptId::new(id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settlement_intent_canonical_hash_stability() {
        let intent = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            1000,
            "hours",
        )
        .with_timestamp(1234567890);

        let hash1 = intent.canonical_hash();
        let hash2 = intent.canonical_hash();

        assert_eq!(hash1, hash2, "canonical hash must be stable");
    }

    #[test]
    fn test_intent_id_does_not_affect_hash() {
        let intent_a = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            1000,
            "hours",
        )
        .with_intent_id("node-a-intent-001")
        .with_timestamp(1234567890);

        let intent_b = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            1000,
            "hours",
        )
        .with_intent_id("node-b-completely-different-id")
        .with_timestamp(1234567890);

        assert_eq!(
            intent_a.canonical_hash(),
            intent_b.canonical_hash(),
            "different intent_ids must produce same canonical_hash"
        );
    }

    #[test]
    fn test_memo_does_not_affect_hash() {
        let intent_a = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            1000,
            "hours",
        )
        .with_timestamp(1234567890);

        let intent_b = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            1000,
            "hours",
        )
        .with_memo("This is a memo that should not affect the hash")
        .with_timestamp(1234567890);

        assert_eq!(
            intent_a.canonical_hash(),
            intent_b.canonical_hash(),
            "memo must not affect canonical_hash"
        );
    }

    #[test]
    fn test_content_change_changes_hash() {
        let intent_a = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            1000,
            "hours",
        )
        .with_timestamp(1234567890);

        let intent_b = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            2000, // Different amount
            "hours",
        )
        .with_timestamp(1234567890);

        assert_ne!(
            intent_a.canonical_hash(),
            intent_b.canonical_hash(),
            "different content must produce different canonical_hash"
        );
    }

    #[test]
    fn test_asset_type_affects_hash() {
        let intent_a = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            1000,
            "hours",
        )
        .with_asset(AssetType::Fungible)
        .with_timestamp(1234567890);

        let intent_b = SettlementIntent::new(
            "decision-001",
            [1u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            1000,
            "hours",
        )
        .with_asset(AssetType::Consumable {
            expires_at: Some(9999999999),
        })
        .with_timestamp(1234567890);

        assert_ne!(
            intent_a.canonical_hash(),
            intent_b.canonical_hash(),
            "different asset types must produce different canonical_hash"
        );
    }

    #[test]
    fn test_serde_roundtrip_preserves_hash() {
        let original = SettlementIntent::new(
            "decision-001",
            [42u8; 32],
            "did:icn:alice",
            "did:icn:bob",
            5000,
            "credits",
        )
        .with_asset(AssetType::Service {
            duration_secs: Some(3600),
        })
        .with_timestamp(1234567890);

        let original_hash = original.canonical_hash();

        // Serialize and deserialize
        let json = serde_json::to_string(&original).unwrap();
        let mut restored: SettlementIntent = serde_json::from_str(&json).unwrap();

        // intent_id is skipped, so set a different one
        restored.intent_id = ReceiptId::new("restored-id");

        assert_eq!(
            original_hash,
            restored.canonical_hash(),
            "serde roundtrip must not change canonical_hash"
        );
    }

    #[test]
    fn test_asset_type_variants() {
        // Just verify all variants are constructible and serializable
        let variants = vec![
            AssetType::Fungible,
            AssetType::Consumable {
                expires_at: Some(1234567890),
            },
            AssetType::Depreciating {
                schedule: DepreciationSchedule::Linear {
                    lifespan_secs: 31536000,
                },
            },
            AssetType::Transformable {
                outputs: vec![("product".to_string(), 2)],
            },
            AssetType::Service {
                duration_secs: Some(3600),
            },
            AssetType::Claim {
                due_by: Some(1234567890),
                conditions: Some("Payment on delivery".to_string()),
            },
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let restored: AssetType = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, restored);
        }
    }

    // ========================================================================
    // AssetType Hash Stability Tests (INT-3)
    // ========================================================================

    #[test]
    fn test_asset_type_fungible_hash_stability() {
        let asset1 = AssetType::Fungible;
        let asset2 = AssetType::Fungible;

        // Hash via bincode serialization (same as canonical_hash in SettlementIntent)
        let hash1 = compute_canonical_hash(&asset1);
        let hash2 = compute_canonical_hash(&asset2);

        assert_eq!(
            hash1, hash2,
            "identical Fungible assets must have same hash"
        );
        assert_ne!(hash1, [0u8; 32], "hash must be non-zero");
    }

    #[test]
    fn test_asset_type_consumable_hash_stability() {
        let asset1 = AssetType::Consumable {
            expires_at: Some(1735689600),
        };
        let asset2 = AssetType::Consumable {
            expires_at: Some(1735689600),
        };

        assert_eq!(
            compute_canonical_hash(&asset1),
            compute_canonical_hash(&asset2),
            "identical Consumable assets must have same hash"
        );
    }

    #[test]
    fn test_asset_type_consumable_none_vs_some_differ() {
        let with_expiry = AssetType::Consumable {
            expires_at: Some(1735689600),
        };
        let without_expiry = AssetType::Consumable { expires_at: None };

        assert_ne!(
            compute_canonical_hash(&with_expiry),
            compute_canonical_hash(&without_expiry),
            "Consumable with/without expiry must have different hashes"
        );
    }

    #[test]
    fn test_asset_type_depreciating_hash_stability() {
        let schedule = DepreciationSchedule::Linear {
            lifespan_secs: 31536000,
        };
        let asset1 = AssetType::Depreciating {
            schedule: schedule.clone(),
        };
        let asset2 = AssetType::Depreciating { schedule };

        assert_eq!(
            compute_canonical_hash(&asset1),
            compute_canonical_hash(&asset2),
            "identical Depreciating assets must have same hash"
        );
    }

    #[test]
    fn test_asset_type_transformable_hash_stability() {
        let asset1 = AssetType::Transformable {
            outputs: vec![("product".to_string(), 2), ("waste".to_string(), 1)],
        };
        let asset2 = AssetType::Transformable {
            outputs: vec![("product".to_string(), 2), ("waste".to_string(), 1)],
        };

        assert_eq!(
            compute_canonical_hash(&asset1),
            compute_canonical_hash(&asset2),
            "identical Transformable assets must have same hash"
        );
    }

    #[test]
    fn test_asset_type_transformable_output_order_matters() {
        let asset1 = AssetType::Transformable {
            outputs: vec![("product".to_string(), 2), ("waste".to_string(), 1)],
        };
        let asset2 = AssetType::Transformable {
            outputs: vec![("waste".to_string(), 1), ("product".to_string(), 2)],
        };

        assert_ne!(
            compute_canonical_hash(&asset1),
            compute_canonical_hash(&asset2),
            "Transformable output order must affect hash (sort outputs for order-independence)"
        );
    }

    #[test]
    fn test_asset_type_service_hash_stability() {
        let asset1 = AssetType::Service {
            duration_secs: Some(3600),
        };
        let asset2 = AssetType::Service {
            duration_secs: Some(3600),
        };

        assert_eq!(
            compute_canonical_hash(&asset1),
            compute_canonical_hash(&asset2),
            "identical Service assets must have same hash"
        );
    }

    #[test]
    fn test_asset_type_claim_hash_stability() {
        let asset1 = AssetType::Claim {
            due_by: Some(1735689600),
            conditions: Some("NET30".to_string()),
        };
        let asset2 = AssetType::Claim {
            due_by: Some(1735689600),
            conditions: Some("NET30".to_string()),
        };

        assert_eq!(
            compute_canonical_hash(&asset1),
            compute_canonical_hash(&asset2),
            "identical Claim assets must have same hash"
        );
    }

    #[test]
    fn test_asset_type_claim_conditions_affect_hash() {
        let asset1 = AssetType::Claim {
            due_by: Some(1735689600),
            conditions: Some("NET30".to_string()),
        };
        let asset2 = AssetType::Claim {
            due_by: Some(1735689600),
            conditions: Some("NET60".to_string()),
        };

        assert_ne!(
            compute_canonical_hash(&asset1),
            compute_canonical_hash(&asset2),
            "different Claim conditions must produce different hash"
        );
    }

    #[test]
    fn test_asset_type_all_variants_have_different_hashes() {
        let variants = vec![
            AssetType::Fungible,
            AssetType::Consumable { expires_at: None },
            AssetType::Depreciating {
                schedule: DepreciationSchedule::Linear {
                    lifespan_secs: 31536000,
                },
            },
            AssetType::Transformable {
                outputs: vec![("x".to_string(), 1)],
            },
            AssetType::Service {
                duration_secs: None,
            },
            AssetType::Claim {
                due_by: None,
                conditions: None,
            },
        ];

        let hashes: Vec<Hash> = variants.iter().map(|v| compute_canonical_hash(v)).collect();

        // Verify all hashes are unique
        for i in 0..hashes.len() {
            for j in (i + 1)..hashes.len() {
                assert_ne!(
                    hashes[i], hashes[j],
                    "variants {:?} and {:?} must have different hashes",
                    variants[i], variants[j]
                );
            }
        }
    }

    #[test]
    fn test_depreciation_schedule_hash_stability() {
        let schedule1 = DepreciationSchedule::Linear {
            lifespan_secs: 31536000,
        };
        let schedule2 = DepreciationSchedule::Linear {
            lifespan_secs: 31536000,
        };

        assert_eq!(
            compute_canonical_hash(&schedule1),
            compute_canonical_hash(&schedule2),
            "identical schedules must have same hash"
        );
    }

    #[test]
    fn test_depreciation_schedule_variants_differ() {
        let linear = DepreciationSchedule::Linear {
            lifespan_secs: 31536000,
        };
        let percentage = DepreciationSchedule::Percentage {
            rate_bps: 100,
            period_secs: 86400,
        };
        let custom = DepreciationSchedule::Custom("custom_schedule".to_string());

        assert_ne!(
            compute_canonical_hash(&linear),
            compute_canonical_hash(&percentage),
            "Linear vs Percentage must have different hashes"
        );
        assert_ne!(
            compute_canonical_hash(&linear),
            compute_canonical_hash(&custom),
            "Linear vs Custom must have different hashes"
        );
        assert_ne!(
            compute_canonical_hash(&percentage),
            compute_canonical_hash(&custom),
            "Percentage vs Custom must have different hashes"
        );
    }

    #[test]
    fn test_asset_type_serde_json_roundtrip_preserves_hash() {
        let assets = vec![
            AssetType::Fungible,
            AssetType::Consumable {
                expires_at: Some(1735689600),
            },
            AssetType::Depreciating {
                schedule: DepreciationSchedule::Percentage {
                    rate_bps: 100,
                    period_secs: 2592000,
                },
            },
            AssetType::Transformable {
                outputs: vec![("product".to_string(), 2)],
            },
            AssetType::Service {
                duration_secs: Some(3600),
            },
            AssetType::Claim {
                due_by: Some(1735689600),
                conditions: Some("NET30".to_string()),
            },
        ];

        for asset in assets {
            let original_hash = compute_canonical_hash(&asset);
            let json = serde_json::to_string(&asset).unwrap();
            let restored: AssetType = serde_json::from_str(&json).unwrap();
            let restored_hash = compute_canonical_hash(&restored);

            assert_eq!(
                original_hash, restored_hash,
                "JSON roundtrip must preserve hash for {:?}",
                asset
            );
        }
    }

    #[test]
    fn test_asset_type_bincode_roundtrip_preserves_hash() {
        let assets = vec![
            AssetType::Fungible,
            AssetType::Consumable { expires_at: None },
            AssetType::Claim {
                due_by: None,
                conditions: None,
            },
        ];

        for asset in assets {
            let original_hash = compute_canonical_hash(&asset);
            let bytes = bincode::serialize(&asset).unwrap();
            let restored: AssetType = bincode::deserialize(&bytes).unwrap();
            let restored_hash = compute_canonical_hash(&restored);

            assert_eq!(
                original_hash, restored_hash,
                "bincode roundtrip must preserve hash for {:?}",
                asset
            );
        }
    }

    #[test]
    fn test_asset_type_default() {
        assert_eq!(AssetType::default(), AssetType::Fungible);
    }
}
