//! Proposal execution callback infrastructure.
//!
//! This module provides the core abstraction for executing governance proposals.
//! The [`ExecutionCallback`] trait defines a uniform interface that handler
//! implementations must satisfy.
//!
//! # Kernel Trait Integration
//!
//! The handlers use kernel execution traits from [`icn_kernel_api::governance`]:
//! - [`GovernanceExecutor`] - Combined executor providing treasury + protocol executors
//! - [`TreasuryExecutor`] - Treasury operations (spend, allocate, reserve, release)
//! - [`ProtocolExecutor`] - Protocol parameter changes
//!
//! These traits define the boundary between governance domain logic and kernel
//! execution services.

use anyhow::Result;
use async_trait::async_trait;
use icn_governance::proof::GovernanceDecisionReceipt;

// Re-export kernel governance types for convenience
pub use icn_kernel_api::governance::{
    DecisionReceiptId, ExecutionOutcome, GovernanceExecutor, ProtocolChange, ProtocolExecutor,
    TreasuryExecutor, TreasuryOperation, TreasuryOperationType,
};

/// Proposal type classification for handler routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalType {
    /// Treasury operations (budgets, withdrawals, transfers)
    Treasury,
    /// Protocol parameter changes
    Protocol,
    /// Federation governance (join/leave, clearing, vouch)
    Federation,
    /// Membership changes
    Membership,
    /// Configuration changes
    Config,
    /// Text proposals (no execution needed)
    Text,
    /// Dispute resolution
    Dispute,
    /// Other/unknown proposal types
    Other(String),
}

impl ProposalType {
    /// Get the string label for metrics
    pub fn as_metric_label(&self) -> &str {
        match self {
            Self::Treasury => "treasury",
            Self::Protocol => "protocol",
            Self::Federation => "federation",
            Self::Membership => "membership",
            Self::Config => "config",
            Self::Text => "text",
            Self::Dispute => "dispute",
            Self::Other(s) => s.as_str(),
        }
    }
}

/// Context for proposal execution.
///
/// Contains all metadata needed for executing a governance proposal,
/// including the cryptographic receipt proving the decision.
#[derive(Debug, Clone)]
pub struct ProposalExecutionContext {
    /// Unique proposal identifier
    pub proposal_id: String,
    /// Governance domain identifier
    pub domain_id: String,
    /// Cryptographic receipt proving the governance decision
    pub receipt: GovernanceDecisionReceipt,
    /// Timestamp when the proposal was decided (seconds since epoch)
    pub decided_at: u64,
    /// The serialized proposal payload
    pub payload: serde_json::Value,
}

impl ProposalExecutionContext {
    /// Create a new execution context.
    pub fn new(
        proposal_id: impl Into<String>,
        domain_id: impl Into<String>,
        receipt: GovernanceDecisionReceipt,
        decided_at: u64,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            proposal_id: proposal_id.into(),
            domain_id: domain_id.into(),
            receipt,
            decided_at,
            payload,
        }
    }
}

/// Callback trait for executing proposal outcomes.
///
/// Implementations of this trait handle specific proposal types.
/// The handler is responsible for:
/// - Validating the proposal payload
/// - Executing the required state changes
/// - Reporting success/failure via metrics
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to support concurrent execution.
#[async_trait]
pub trait ExecutionCallback: Send + Sync {
    /// Execute a proposal outcome.
    ///
    /// # Arguments
    /// * `ctx` - The execution context containing proposal metadata and payload
    ///
    /// # Returns
    /// * `Ok(())` - Execution succeeded
    /// * `Err(e)` - Execution failed with the given error
    async fn execute(&self, ctx: &ProposalExecutionContext) -> Result<()>;

    /// Check if this callback handles the given proposal type.
    ///
    /// # Arguments
    /// * `proposal_type` - The type of proposal to check
    ///
    /// # Returns
    /// * `true` if this handler can process the proposal type
    /// * `false` otherwise
    fn handles(&self, proposal_type: &ProposalType) -> bool;

    /// Get the name of this handler for logging/metrics.
    fn name(&self) -> &str;
}

// =============================================================================
// Payload-to-Effect Translation
// =============================================================================

use icn_governance::ProposalPayload;
use icn_kernel_api::effects::{
    ControlEffect, FederationEffect, KernelEffect, MembershipEffect, ProtocolEffect, TreasuryEffect,
};

/// Translate a governance proposal payload to kernel-safe effects.
///
/// This is the key boundary function: the governance app understands domain
/// types (ProposalPayload, TreasuryProposalOperation, etc.), but the kernel
/// only sees KernelEffect with primitive types.
///
/// # Arguments
/// * `payload` - The domain-specific proposal payload
/// * `decision_receipt_id` - The receipt ID for audit linkage
///
/// # Returns
/// A vector of kernel effects (usually 1, but some proposals produce multiple)
pub fn translate_payload_to_effects(
    payload: &ProposalPayload,
    decision_receipt_id: &str,
    decision_hash: &str,
) -> Vec<KernelEffect> {
    match payload {
        // Treasury proposals
        ProposalPayload::Treasury { operation } => {
            translate_treasury_operation(operation, decision_receipt_id, decision_hash)
        }

        ProposalPayload::Budget {
            amount,
            currency,
            purpose,
            ..
        } => vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
            treasury_did: String::new(), // Filled by context
            budget_id: format!("budget-{purpose}"),
            total_amount: *amount,
            currency: currency.clone(),
            name: purpose.clone(),
            validity_start: 0,
            validity_end: u64::MAX,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })],

        // Membership proposals
        ProposalPayload::Membership { action, member } => {
            translate_membership_action(action, member)
        }

        ProposalPayload::FreezeMember { member, reason, .. } => {
            vec![KernelEffect::Membership(MembershipEffect::FreezeMember {
                entity_id: String::new(), // Filled by caller with context
                member_did: member.to_string(),
                reason: reason.clone(),
                duration_secs: None,
            })]
        }

        ProposalPayload::UnfreezeMember { member, reason: _ } => {
            vec![KernelEffect::Membership(MembershipEffect::UnfreezeMember {
                entity_id: String::new(),
                member_did: member.to_string(),
            })]
        }

        // Protocol proposals
        ProposalPayload::ConfigChange { new_config } => {
            vec![KernelEffect::Protocol(
                ProtocolEffect::SetGovernanceConfig {
                    domain_id: String::new(),
                    config_hash: blake3::hash(new_config.as_bytes()).to_hex().to_string(),
                    config_json: new_config.clone(),
                },
            )]
        }

        ProposalPayload::SchedulingPolicy {
            coop_id,
            policy_json,
        } => {
            vec![KernelEffect::Protocol(
                ProtocolEffect::SetSchedulingPolicy {
                    coop_id: coop_id.clone(),
                    policy_hash: blake3::hash(policy_json.as_bytes()).to_hex().to_string(),
                    policy_json: policy_json.clone(),
                },
            )]
        }

        ProposalPayload::ProtocolUpgrade { version, .. } => {
            vec![KernelEffect::Protocol(ProtocolEffect::Upgrade {
                version: version.to_string(),
                upgrade_hash: String::new(),
                activation_height: 0,
            })]
        }

        // Control proposals
        ProposalPayload::VetoProposal {
            target_proposal_id,
            reason,
        } => vec![KernelEffect::Control(ControlEffect::VetoProposal {
            target_proposal_id: target_proposal_id.clone(),
            veto_reason: reason.clone(),
        })],

        ProposalPayload::ForceCloseProposal {
            target_proposal_id,
            reason,
            ..
        } => vec![KernelEffect::Control(ControlEffect::ForceCloseProposal {
            target_proposal_id: target_proposal_id.clone(),
            close_reason: reason.clone(),
        })],

        ProposalPayload::Text { body } => {
            vec![KernelEffect::Control(ControlEffect::TextResolution {
                resolution_hash: blake3::hash(body.as_bytes()).to_hex().to_string(),
            })]
        }

        // Federation proposals
        ProposalPayload::Federation(fed_proposal) => translate_federation_proposal(fed_proposal),

        // Fallback for unhandled types
        _ => vec![KernelEffect::NoOp {
            reason: format!(
                "Unhandled proposal type: {:?}",
                std::mem::discriminant(payload)
            ),
        }],
    }
}

/// Translate treasury operations to kernel effects
fn translate_treasury_operation(
    operation: &icn_governance::TreasuryProposalOperation,
    decision_receipt_id: &str,
    decision_hash: &str,
) -> Vec<KernelEffect> {
    use icn_governance::TreasuryProposalOperation;
    const TREASURY_SPEND_UNSUPPORTED_REASON: &str =
        "Treasury Spend translation unsupported: missing treasury_did and currency in payload";

    match operation {
        TreasuryProposalOperation::Withdraw {
            treasury_did,
            recipient,
            amount,
            currency,
            purpose,
            ..
        } => vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: treasury_did.to_string(),
            recipient_did: recipient.to_string(),
            amount: *amount,
            currency: currency.clone(),
            memo: purpose.clone(),
            budget_id: None, // TODO: wire budget_id from proposal payload
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })],

        TreasuryProposalOperation::Spend { .. } => vec![KernelEffect::NoOp {
            reason: TREASURY_SPEND_UNSUPPORTED_REASON.to_string(),
        }],

        TreasuryProposalOperation::CreateBudget {
            treasury_did,
            purpose,
            amount,
            currency,
            period_end,
        } => vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
            treasury_did: treasury_did.to_string(),
            budget_id: format!("budget-{purpose}"),
            total_amount: *amount,
            currency: currency.clone(),
            name: purpose.clone(),
            validity_start: 0,
            validity_end: period_end.unwrap_or(u64::MAX),
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })],

        // Fallback for other treasury operations
        _ => vec![KernelEffect::NoOp {
            reason: format!(
                "Treasury operation not yet translated: {:?}",
                std::mem::discriminant(operation)
            ),
        }],
    }
}

/// Translate membership actions to kernel effects
fn translate_membership_action(
    action: &icn_governance::MembershipAction,
    member: &icn_identity::Did,
) -> Vec<KernelEffect> {
    use icn_governance::MembershipAction;

    match action {
        MembershipAction::Add => {
            vec![KernelEffect::Membership(MembershipEffect::AddMember {
                entity_id: String::new(),
                member_did: member.to_string(),
                role: String::new(),
                tier: String::new(),
            })]
        }
        MembershipAction::Remove => {
            vec![KernelEffect::Membership(MembershipEffect::RemoveMember {
                entity_id: String::new(),
                member_did: member.to_string(),
                reason: String::new(),
            })]
        }
    }
}

/// Translate federation proposals to kernel effects
fn translate_federation_proposal(
    proposal: &icn_governance::FederationProposal,
) -> Vec<KernelEffect> {
    use icn_governance::FederationProposal;

    match proposal {
        FederationProposal::JoinFederation { federation_id, .. } => {
            vec![KernelEffect::Federation(FederationEffect::JoinFederation {
                coop_did: String::new(),
                federation_id: federation_id.clone(),
            })]
        }
        FederationProposal::LeaveFederation { federation_id, .. } => {
            vec![KernelEffect::Federation(
                FederationEffect::LeaveFederation {
                    coop_did: String::new(),
                    federation_id: federation_id.clone(),
                },
            )]
        }
        FederationProposal::EstablishClearing {
            partner_coop_did, ..
        } => {
            vec![KernelEffect::Federation(
                FederationEffect::EstablishClearing {
                    coop_a_did: String::new(),
                    coop_b_did: partner_coop_did.to_string(),
                    agreement_hash: String::new(),
                },
            )]
        }
        // Fallback for other federation proposals
        _ => vec![KernelEffect::NoOp {
            reason: format!(
                "Federation proposal not yet translated: {:?}",
                std::mem::discriminant(proposal)
            ),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposal_type_metric_label() {
        assert_eq!(ProposalType::Treasury.as_metric_label(), "treasury");
        assert_eq!(ProposalType::Protocol.as_metric_label(), "protocol");
        assert_eq!(ProposalType::Federation.as_metric_label(), "federation");
        assert_eq!(
            ProposalType::Other("custom".to_string()).as_metric_label(),
            "custom"
        );
    }

    #[test]
    fn test_execution_context_creation() {
        use icn_governance::tally::VoteTally;

        let tally = VoteTally {
            for_votes: 100,
            against_votes: 50,
            abstain_votes: 10,
        };
        let receipt = GovernanceDecisionReceipt::new(
            "proposal-123".to_string(),
            "domain-1".to_string(),
            icn_governance::proof::ProofOutcome::Accepted,
            tally,
            &[], // empty votes slice
        );
        let ctx = ProposalExecutionContext::new(
            "proposal-123",
            "domain-1",
            receipt.clone(),
            1000,
            serde_json::json!({"type": "test"}),
        );

        assert_eq!(ctx.proposal_id, "proposal-123");
        assert_eq!(ctx.domain_id, "domain-1");
        assert_eq!(ctx.decided_at, 1000);
        assert_eq!(ctx.receipt.proposal_id, "proposal-123");
    }
}
