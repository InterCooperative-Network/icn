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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    },
    /// Spend from treasury to recipient
    Spend {
        treasury_did: String,
        recipient_did: String,
        amount: i64,
        currency: String,
        memo: String,
        decision_receipt_id: String,
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
}

// =============================================================================
// Membership Effects
// =============================================================================

/// Membership-related effects
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            decision_receipt_id: "receipt-456".into(),
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
            decision_receipt_id: "r1".into(),
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
            decision_receipt_id: String::new(),
        };
    }
}
