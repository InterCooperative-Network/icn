//! Federation governance proposal handlers.
//!
//! Handles federation-related proposals including:
//! - Joining/leaving federations
//! - Establishing/terminating clearing agreements
//! - Vouching for cooperatives
//! - Revoking vouches
//! - Updating federation policy
//!
//! # Design
//!
//! The `FederationHandler` implements the [`ExecutionCallback`] trait, receiving
//! federation proposals and coordinating with the federation registry, clearing
//! manager, and attestation store.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tracing::{debug, info, warn};

use super::execution::{ExecutionCallback, ProposalExecutionContext, ProposalType};

/// Handler for federation governance proposals.
///
/// This handler processes federation-related proposals such as joining/leaving
/// federations, establishing clearing agreements, and managing trust attestations.
///
/// # Note
///
/// Federation proposal execution now routes through the effect path via
/// `FederationService`. The legacy `governance_handlers/` has been deleted.
/// This handler translates proposal payloads to effects.
pub struct FederationHandler {
    /// Handler name for logging
    name: String,
}

impl FederationHandler {
    /// Create a new federation handler.
    pub fn new() -> Self {
        Self {
            name: "FederationHandler".to_string(),
        }
    }
}

impl Default for FederationHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionCallback for FederationHandler {
    async fn execute(&self, ctx: &ProposalExecutionContext) -> Result<()> {
        info!(
            proposal_id = %ctx.proposal_id,
            domain_id = %ctx.domain_id,
            "Executing federation proposal"
        );

        // Parse the federation action from the payload
        let action = ctx
            .payload
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        debug!(
            proposal_id = %ctx.proposal_id,
            action = %action,
            "Federation action type"
        );

        // Placeholder: In Phase 4 Sprint 3+, we'll migrate the actual
        // federation execution logic from icn-core here.
        //
        // For now, federation proposals are still executed via the executor
        // callback that has access to icn-core internals.

        warn!(
            proposal_id = %ctx.proposal_id,
            "Federation handler is a placeholder - execution delegated to icn-core"
        );

        // Return an error to indicate this handler is not yet active
        Err(anyhow!(
            "Federation handler not yet implemented - use executor callback"
        ))
    }

    fn handles(&self, proposal_type: &ProposalType) -> bool {
        matches!(proposal_type, ProposalType::Federation)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Federation action types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Variants used for routing logic in future migration
pub enum FederationAction {
    /// Join a federation
    JoinFederation,
    /// Leave a federation
    LeaveFederation,
    /// Establish a clearing agreement with another cooperative
    EstablishClearing,
    /// Terminate a clearing agreement
    TerminateClearing,
    /// Vouch for another cooperative
    VouchForCooperative,
    /// Revoke a previous vouch
    RevokeVouch,
    /// Update federation policy parameters
    UpdateFederationPolicy,
}

impl FederationAction {
    /// Parse action type from JSON payload.
    #[allow(dead_code)] // Used in tests and future handler migration
    pub fn from_payload(payload: &serde_json::Value) -> Option<Self> {
        let action_str = payload.get("action")?.as_str()?;
        match action_str {
            "JoinFederation" | "join_federation" => Some(Self::JoinFederation),
            "LeaveFederation" | "leave_federation" => Some(Self::LeaveFederation),
            "EstablishClearing" | "establish_clearing" => Some(Self::EstablishClearing),
            "TerminateClearing" | "terminate_clearing" => Some(Self::TerminateClearing),
            "VouchForCooperative" | "vouch_for_cooperative" => Some(Self::VouchForCooperative),
            "RevokeVouch" | "revoke_vouch" => Some(Self::RevokeVouch),
            "UpdateFederationPolicy" | "update_federation_policy" => {
                Some(Self::UpdateFederationPolicy)
            }
            _ => None,
        }
    }

    /// Get the action name for metrics/logging.
    #[allow(dead_code)] // Used in tests and future handler migration
    pub fn action_name(&self) -> &str {
        match self {
            Self::JoinFederation => "join_federation",
            Self::LeaveFederation => "leave_federation",
            Self::EstablishClearing => "establish_clearing",
            Self::TerminateClearing => "terminate_clearing",
            Self::VouchForCooperative => "vouch_for_cooperative",
            Self::RevokeVouch => "revoke_vouch",
            Self::UpdateFederationPolicy => "update_federation_policy",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_handler_handles() {
        let handler = FederationHandler::new();
        assert!(handler.handles(&ProposalType::Federation));
        assert!(!handler.handles(&ProposalType::Treasury));
        assert!(!handler.handles(&ProposalType::Protocol));
    }

    #[test]
    fn test_federation_handler_name() {
        let handler = FederationHandler::new();
        assert_eq!(handler.name(), "FederationHandler");
    }

    #[test]
    fn test_federation_action_from_payload() {
        let payload = serde_json::json!({"action": "JoinFederation"});
        assert_eq!(
            FederationAction::from_payload(&payload),
            Some(FederationAction::JoinFederation)
        );

        let payload = serde_json::json!({"action": "leave_federation"});
        assert_eq!(
            FederationAction::from_payload(&payload),
            Some(FederationAction::LeaveFederation)
        );

        let payload = serde_json::json!({"action": "Unknown"});
        assert_eq!(FederationAction::from_payload(&payload), None);
    }

    #[test]
    fn test_federation_action_name() {
        assert_eq!(
            FederationAction::JoinFederation.action_name(),
            "join_federation"
        );
        assert_eq!(
            FederationAction::LeaveFederation.action_name(),
            "leave_federation"
        );
        assert_eq!(
            FederationAction::EstablishClearing.action_name(),
            "establish_clearing"
        );
    }
}
