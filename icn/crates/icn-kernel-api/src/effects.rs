//! Kernel-safe effect types for governance execution.
//!
//! These types contain ONLY primitive data (IDs, hashes, amounts, DIDs)
//! and NO domain-specific types from governance, trust, or other apps.
//!
//! The governance app translates domain-specific proposal payloads into
//! these kernel-safe effects. The kernel executes effects without needing
//! to understand domain semantics.
//!
//! # Architecture
//!
//! ```text
//! [Governance App]                    [Kernel]
//!      |                                  |
//! ProposalPayload                         |
//!      |                                  |
//!      v                                  |
//! translate() ─────> KernelEffect ─────> execute()
//!      |                                  |
//! (understands                     (only sees IDs,
//!  domain types)                    hashes, amounts)
//! ```

use serde::{Deserialize, Serialize};

/// Aggregate enum for all kernel-safe effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KernelEffect {
    /// Treasury-related effects
    Treasury(TreasuryEffect),
    /// Membership-related effects
    Membership(MembershipEffect),
    /// Protocol parameter effects
    Protocol(ProtocolEffect),
    /// Governance control effects (veto, force close)
    Control(ControlEffect),
    /// Federation effects
    Federation(FederationEffect),
    /// No-op effect (e.g., for text proposals)
    NoOp { reason: String },
}

// =============================================================================
// Treasury Effects
// =============================================================================

/// Treasury-related effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum TreasuryEffect {
    /// Create a budget allocation
    CreateBudget {
        treasury_did: String,
        budget_id: String,
        total_amount: i64,
        currency: String,
        name: String,
        validity_start: u64,
        validity_end: u64,
        /// Links back to governance decision receipt
        decision_receipt_id: String,
        /// Canonical content hash for verification
        decision_hash: String,
    },
    /// Spend from treasury to recipient
    Spend {
        treasury_did: String,
        recipient_did: String,
        amount: i64,
        currency: String,
        memo: String,
        /// Optional budget to charge this spend against.
        /// When present, the executor enforces the budget limit
        /// before submitting the ledger entry.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        budget_id: Option<String>,
        /// Links back to governance decision receipt
        decision_receipt_id: String,
        /// Canonical content hash for verification
        decision_hash: String,
    },
    /// Allocate funds to a budget category
    Allocate {
        treasury_did: String,
        budget_id: String,
        amount: i64,
        currency: String,
    },
    /// Transfer between accounts
    Transfer {
        from_did: String,
        to_did: String,
        amount: i64,
        currency: String,
        memo: String,
    },
    /// Surplus distribution to members
    DistributeSurplus {
        treasury_did: String,
        total_amount: i64,
        currency: String,
        /// List of (member_did, share_amount) pairs
        distributions: Vec<(String, i64)>,
    },
    /// Share redemption payout
    RedeemShares {
        treasury_did: String,
        member_did: String,
        share_count: u64,
        payout_amount: i64,
        currency: String,
    },
    /// Bond issuance
    IssueBond {
        treasury_did: String,
        bond_id: String,
        principal: i64,
        interest_rate_bps: u32,
        maturity_date: u64,
        currency: String,
    },
    /// Release an escrow — governance decided, now move the money.
    ///
    /// The escrow store provides domain-level idempotency: even if the
    /// decision-level executor replays this effect, the escrow record's
    /// `release_decision_hash` prevents double-disbursement.
    ReleaseEscrow {
        /// The escrow being released.
        escrow_id: String,
        /// Treasury funding the escrow (debit side).
        treasury_did: String,
        /// Beneficiary receiving funds (credit side).
        beneficiary_did: String,
        /// Amount to release.
        amount: i64,
        /// Currency / asset type.
        currency: String,
        /// Links back to governance decision receipt.
        decision_receipt_id: String,
        /// Canonical content hash for verification + idempotency.
        decision_hash: String,
    },
}

// =============================================================================
// Membership Effects
// =============================================================================

/// Membership-related effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MembershipEffect {
    /// Add a new member
    AddMember {
        entity_id: String,
        member_did: String,
        role: String,
        tier: String,
    },
    /// Remove a member
    RemoveMember {
        entity_id: String,
        member_did: String,
        reason: String,
    },
    /// Change member role/tier
    UpdateMember {
        entity_id: String,
        member_did: String,
        new_role: Option<String>,
        new_tier: Option<String>,
    },
    /// Freeze member (suspend rights)
    FreezeMember {
        entity_id: String,
        member_did: String,
        reason: String,
        duration_secs: Option<u64>,
    },
    /// Unfreeze member (restore rights)
    UnfreezeMember {
        entity_id: String,
        member_did: String,
    },
}

// =============================================================================
// Protocol Effects
// =============================================================================

/// Protocol parameter effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change_type", rename_all = "snake_case")]
pub enum ProtocolEffect {
    /// Change a protocol parameter
    SetParameter {
        parameter_name: String,
        old_value_hash: String,
        new_value_json: String,
        effective_at: u64,
    },
    /// Apply protocol upgrade
    Upgrade {
        version: String,
        upgrade_hash: String,
        activation_height: u64,
    },
    /// Update scheduling policy
    SetSchedulingPolicy {
        coop_id: String,
        policy_hash: String,
        policy_json: String,
    },
    /// Update governance config
    SetGovernanceConfig {
        domain_id: String,
        config_hash: String,
        config_json: String,
    },
}

// =============================================================================
// Control Effects
// =============================================================================

/// Governance control effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "control_type", rename_all = "snake_case")]
pub enum ControlEffect {
    /// Veto a pending proposal
    VetoProposal {
        target_proposal_id: String,
        veto_reason: String,
    },
    /// Force close a proposal
    ForceCloseProposal {
        target_proposal_id: String,
        close_reason: String,
    },
    /// Text proposal (informational, no state change)
    TextResolution { resolution_hash: String },
}

// =============================================================================
// Federation Effects
// =============================================================================

/// Federation-related effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "federation_action", rename_all = "snake_case")]
pub enum FederationEffect {
    /// Join a federation
    JoinFederation {
        coop_did: String,
        federation_id: String,
    },
    /// Leave a federation
    LeaveFederation {
        coop_did: String,
        federation_id: String,
    },
    /// Establish clearing agreement
    EstablishClearing {
        coop_a_did: String,
        coop_b_did: String,
        agreement_hash: String,
    },
    /// Vouch for another cooperative
    VouchForCoop {
        voucher_did: String,
        vouchee_did: String,
        attestation_hash: String,
    },
}

// =============================================================================
// Dispute Effects (for completeness)
// =============================================================================

/// Dispute resolution effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "dispute_action", rename_all = "snake_case")]
pub enum DisputeEffect {
    /// Resolve a dispute
    ResolveDispute {
        dispute_id: String,
        resolution_hash: String,
        compensations: Vec<(String, i64, String)>, // (did, amount, currency)
    },
    /// Rollback ledger entries
    RollbackLedger {
        entry_ids: Vec<String>,
        rollback_reason: String,
        authorized_by: String,
    },
}

// =============================================================================
// Resource Effects
// =============================================================================

/// Resource access effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "resource_action", rename_all = "snake_case")]
pub enum ResourceEffect {
    /// Grant resource access
    GrantAccess {
        grantee_did: String,
        resource_type: String,
        access_model_hash: String,
    },
    /// Revoke resource access
    RevokeAccess {
        grantee_did: String,
        resource_type: String,
    },
}

// =============================================================================
// SDIS Effects
// =============================================================================

/// SDIS (Sovereign Digital Identity System) effects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "sdis_action", rename_all = "snake_case")]
pub enum SdisEffect {
    /// Approve steward for enrollment ceremonies
    ApproveSteward {
        steward_did: String,
        capabilities_hash: String,
    },
    /// Revoke steward status
    RevokeSteward { steward_did: String, reason: String },
}

// =============================================================================
// Execution Result
// =============================================================================

/// Result of executing a kernel effect
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectResult {
    /// The effect that was executed
    pub effect_id: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Human-readable outcome message
    pub message: String,
    /// Optional hash of state change (for audit)
    pub state_change_hash: Option<String>,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_treasury_effect_serde() {
        let effect = TreasuryEffect::Spend {
            treasury_did: "did:icn:treasury123".into(),
            recipient_did: "did:icn:alice".into(),
            amount: 100,
            currency: "HOURS".into(),
            memo: "Tool purchase".into(),
            budget_id: None,
            decision_receipt_id: "receipt-456".into(),
            decision_hash: "abc123".into(),
        };

        let json = serde_json::to_string(&effect).unwrap();
        assert!(json.contains("spend"));
        assert!(json.contains("treasury_did"));

        let parsed: TreasuryEffect = serde_json::from_str(&json).unwrap();
        match parsed {
            TreasuryEffect::Spend { amount, .. } => assert_eq!(amount, 100),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_kernel_effect_tagged_serde() {
        let effect = KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: "did:icn:t1".into(),
            recipient_did: "did:icn:r1".into(),
            amount: 50,
            currency: "USD".into(),
            memo: "test".into(),
            budget_id: None,
            decision_receipt_id: "r1".into(),
            decision_hash: "hash1".into(),
        });

        let json = serde_json::to_string(&effect).unwrap();
        assert!(json.contains("\"type\":\"treasury\""));
    }

    #[test]
    fn test_membership_effect_serde() {
        let effect = MembershipEffect::FreezeMember {
            entity_id: "coop-1".into(),
            member_did: "did:icn:bob".into(),
            reason: "Policy violation".into(),
            duration_secs: Some(86400),
        };

        let json = serde_json::to_string(&effect).unwrap();
        assert!(json.contains("freeze_member"));
        assert!(json.contains("duration_secs"));
    }

    #[test]
    fn test_no_domain_types() {
        // This test documents that effect types use only primitives.
        // All fields are: String, i64, u64, u32, bool, Option<T>, Vec<T>
        // No governance-specific types like ProposalId, TreasuryProposalOperation, etc.
        let _: TreasuryEffect = TreasuryEffect::Spend {
            treasury_did: String::new(),
            recipient_did: String::new(),
            amount: 0i64,
            currency: String::new(),
            memo: String::new(),
            budget_id: None,
            decision_receipt_id: String::new(),
            decision_hash: String::new(),
        };
    }

    // =========================================================================
    // Serialization roundtrip tests for cross-node transport (Stage 7)
    // =========================================================================
    //
    // NOTE: KernelEffect and sub-effects use internally tagged enums
    // (#[serde(tag = "...")]) which are incompatible with bincode.
    // Cross-node transport uses JSON for effects, which supports tagged enums
    // and provides human-readable wire format for debugging.

    /// Helper to test JSON roundtrip with equality assertion
    fn assert_json_roundtrip<T>(original: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + PartialEq,
    {
        let json = serde_json::to_string(original).expect("serialize to JSON");
        let recovered: T = serde_json::from_str(&json).expect("deserialize from JSON");
        assert_eq!(original, &recovered, "JSON roundtrip failed");
    }

    /// Helper to test bincode roundtrip with equality assertion
    /// NOTE: Only works for types without serde(tag) attributes
    fn assert_bincode_roundtrip<T>(original: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + PartialEq,
    {
        let bytes = bincode::serialize(original).expect("serialize to bincode");
        let recovered: T = bincode::deserialize(&bytes).expect("deserialize from bincode");
        assert_eq!(original, &recovered, "bincode roundtrip failed");
    }

    #[test]
    fn test_serialize_kernel_effect_roundtrip_json() {
        // Tests all KernelEffect variants serialize/deserialize via JSON
        let effects = vec![
            KernelEffect::Treasury(TreasuryEffect::Spend {
                treasury_did: "did:icn:treasury123".into(),
                recipient_did: "did:icn:alice".into(),
                amount: 100,
                currency: "HOURS".into(),
                memo: "Tool purchase".into(),
                budget_id: None,
                decision_receipt_id: "receipt-456".into(),
                decision_hash: "abc123".into(),
            }),
            KernelEffect::Treasury(TreasuryEffect::CreateBudget {
                treasury_did: "did:icn:treasury".into(),
                budget_id: "budget-2024".into(),
                total_amount: 50000,
                currency: "USD".into(),
                name: "Operations Q4".into(),
                validity_start: 1700000000,
                validity_end: 1710000000,
                decision_receipt_id: "r1".into(),
                decision_hash: "h1".into(),
            }),
            KernelEffect::Treasury(TreasuryEffect::DistributeSurplus {
                treasury_did: "did:icn:treasury".into(),
                total_amount: 10000,
                currency: "HOURS".into(),
                distributions: vec![
                    ("did:icn:alice".into(), 3000),
                    ("did:icn:bob".into(), 4000),
                    ("did:icn:carol".into(), 3000),
                ],
            }),
            KernelEffect::Membership(MembershipEffect::AddMember {
                entity_id: "coop-1".into(),
                member_did: "did:icn:bob".into(),
                role: "worker".into(),
                tier: "standard".into(),
            }),
            KernelEffect::Membership(MembershipEffect::UpdateMember {
                entity_id: "coop-1".into(),
                member_did: "did:icn:bob".into(),
                new_role: Some("coordinator".into()),
                new_tier: None,
            }),
            KernelEffect::Membership(MembershipEffect::FreezeMember {
                entity_id: "coop-1".into(),
                member_did: "did:icn:carol".into(),
                reason: "Policy violation".into(),
                duration_secs: Some(86400),
            }),
            KernelEffect::Protocol(ProtocolEffect::SetParameter {
                parameter_name: "voting_period".into(),
                old_value_hash: "oldhash".into(),
                new_value_json: r#"{"days":7}"#.into(),
                effective_at: 1700000000,
            }),
            KernelEffect::Protocol(ProtocolEffect::Upgrade {
                version: "1.2.0".into(),
                upgrade_hash: "upgradehash".into(),
                activation_height: 100000,
            }),
            KernelEffect::Control(ControlEffect::VetoProposal {
                target_proposal_id: "prop-123".into(),
                veto_reason: "Insufficient quorum".into(),
            }),
            KernelEffect::Control(ControlEffect::TextResolution {
                resolution_hash: "reshash".into(),
            }),
            KernelEffect::Federation(FederationEffect::JoinFederation {
                coop_did: "did:icn:coop1".into(),
                federation_id: "fed-regional".into(),
            }),
            KernelEffect::Federation(FederationEffect::VouchForCoop {
                voucher_did: "did:icn:coop1".into(),
                vouchee_did: "did:icn:coop2".into(),
                attestation_hash: "atthash".into(),
            }),
            KernelEffect::NoOp {
                reason: "Text proposal only".into(),
            },
        ];

        for effect in &effects {
            assert_json_roundtrip(effect);
        }
    }

    #[test]
    fn test_serialize_effect_result_roundtrip() {
        // EffectResult has no tag attribute, so bincode works
        let results = vec![
            EffectResult {
                effect_id: "eff-1".into(),
                success: true,
                message: "Budget created successfully".into(),
                state_change_hash: Some("statehash123".into()),
            },
            EffectResult {
                effect_id: "eff-2".into(),
                success: false,
                message: "Insufficient funds".into(),
                state_change_hash: None,
            },
        ];

        for result in &results {
            assert_json_roundtrip(result);
            assert_bincode_roundtrip(result);
        }
    }

    #[test]
    fn test_serialize_all_treasury_variants_roundtrip() {
        // JSON-only due to serde(tag) on TreasuryEffect
        let variants = vec![
            TreasuryEffect::CreateBudget {
                treasury_did: "did:icn:t1".into(),
                budget_id: "b1".into(),
                total_amount: 1000,
                currency: "USD".into(),
                name: "Test".into(),
                validity_start: 0,
                validity_end: 1,
                decision_receipt_id: "r1".into(),
                decision_hash: "h1".into(),
            },
            TreasuryEffect::Spend {
                treasury_did: "did:icn:t1".into(),
                recipient_did: "did:icn:r1".into(),
                amount: 100,
                currency: "USD".into(),
                memo: "test".into(),
                budget_id: None,
                decision_receipt_id: "r1".into(),
                decision_hash: "h1".into(),
            },
            TreasuryEffect::Allocate {
                treasury_did: "did:icn:t1".into(),
                budget_id: "b1".into(),
                amount: 500,
                currency: "USD".into(),
            },
            TreasuryEffect::Transfer {
                from_did: "did:icn:a".into(),
                to_did: "did:icn:b".into(),
                amount: 200,
                currency: "USD".into(),
                memo: "xfer".into(),
            },
            TreasuryEffect::DistributeSurplus {
                treasury_did: "did:icn:t1".into(),
                total_amount: 1000,
                currency: "USD".into(),
                distributions: vec![("did:icn:m1".into(), 500), ("did:icn:m2".into(), 500)],
            },
            TreasuryEffect::RedeemShares {
                treasury_did: "did:icn:t1".into(),
                member_did: "did:icn:m1".into(),
                share_count: 10,
                payout_amount: 1000,
                currency: "USD".into(),
            },
            TreasuryEffect::IssueBond {
                treasury_did: "did:icn:t1".into(),
                bond_id: "bond-1".into(),
                principal: 10000,
                interest_rate_bps: 500,
                maturity_date: 1750000000,
                currency: "USD".into(),
            },
            TreasuryEffect::ReleaseEscrow {
                escrow_id: "esc-1".into(),
                treasury_did: "did:icn:t1".into(),
                beneficiary_did: "did:icn:alice".into(),
                amount: 5000,
                currency: "HOURS".into(),
                decision_receipt_id: "r1".into(),
                decision_hash: "h1".into(),
            },
        ];

        for variant in &variants {
            assert_json_roundtrip(variant);
        }
    }

    #[test]
    fn test_serialize_all_membership_variants_roundtrip() {
        // JSON-only due to serde(tag) on MembershipEffect
        let variants = vec![
            MembershipEffect::AddMember {
                entity_id: "e1".into(),
                member_did: "did:icn:m1".into(),
                role: "worker".into(),
                tier: "standard".into(),
            },
            MembershipEffect::RemoveMember {
                entity_id: "e1".into(),
                member_did: "did:icn:m1".into(),
                reason: "Voluntary exit".into(),
            },
            MembershipEffect::UpdateMember {
                entity_id: "e1".into(),
                member_did: "did:icn:m1".into(),
                new_role: Some("lead".into()),
                new_tier: Some("premium".into()),
            },
            MembershipEffect::FreezeMember {
                entity_id: "e1".into(),
                member_did: "did:icn:m1".into(),
                reason: "Investigation".into(),
                duration_secs: Some(3600),
            },
            MembershipEffect::UnfreezeMember {
                entity_id: "e1".into(),
                member_did: "did:icn:m1".into(),
            },
        ];

        for variant in &variants {
            assert_json_roundtrip(variant);
        }
    }

    #[test]
    fn test_serialize_decision_receipt_id_roundtrip() {
        use crate::governance::DecisionReceiptId;

        let id = DecisionReceiptId::new("gov:proposal:2024-pilot:receipt:test-001");
        assert_json_roundtrip(&id);
        assert_bincode_roundtrip(&id);
    }
}
