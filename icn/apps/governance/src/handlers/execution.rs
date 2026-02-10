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
