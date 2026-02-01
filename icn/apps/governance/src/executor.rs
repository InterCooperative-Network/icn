//! Proposal Executor Implementation
//!
//! Implements the `ProposalExecutor` trait from `icn-kernel-api`.
//! This provides the execution interface for governance proposals.
//!
//! # Architecture
//!
//! The executor bridges the kernel dispatch layer and the domain logic:
//! - The kernel calls `execute_proposal()` with opaque JSON
//! - The executor deserializes and validates the payload
//! - Domain-specific execution is delegated to injected handlers
//!
//! # Phased Migration
//!
//! Phase 4 Sprint 2 establishes the trait boundary. Handler logic remains
//! in icn-core but is accessed via injected closures/callbacks. Future
//! sprints will migrate handler implementations into this module.

use icn_kernel_api::services::ProposalExecutor;
use std::sync::Arc;
use tracing::{error, info};

/// Callback type for executing proposals with domain logic
///
/// The kernel-facing executor is stateless. It delegates actual execution
/// to this callback, which has access to icn-core internals.
///
/// Arguments:
/// - `proposal_id`: Opaque proposal identifier
/// - `payload`: Serialized proposal payload
/// - `decided_at`: Acceptance timestamp (seconds since epoch)
/// - `domain_id`: Governance domain identifier
///
/// Returns:
/// - `Ok(())`: Execution succeeded
/// - `Err(String)`: Execution failed with error message
pub type ExecutionCallback =
    Arc<dyn Fn(&str, &serde_json::Value, u64, &str) -> Result<(), String> + Send + Sync>;

/// Minimal proposal executor for Phase 4 Sprint 2
///
/// This executor implements the kernel-api trait and delegates to
/// an injected callback for actual execution. The callback has access
/// to icn-core internals (ledger, governance actor, etc.).
///
/// Future sprints will move handler logic from icn-core into this executor,
/// reducing the kernel dependency surface.
pub struct GovernanceProposalExecutor {
    /// Callback for executing proposals with domain logic
    execution_callback: ExecutionCallback,
}

impl GovernanceProposalExecutor {
    /// Create a new proposal executor with an execution callback
    ///
    /// The callback provides access to domain-specific execution logic
    /// that requires icn-core types.
    pub fn new(execution_callback: ExecutionCallback) -> Self {
        Self { execution_callback }
    }
}

impl ProposalExecutor for GovernanceProposalExecutor {
    fn execute_proposal(
        &self,
        proposal_id: &str,
        payload: &serde_json::Value,
        decided_at: u64,
        domain_id: &str,
    ) -> Result<(), String> {
        info!(
            proposal_id = %proposal_id,
            domain_id = %domain_id,
            "Executing governance proposal"
        );

        // Delegate to the injected callback
        match (self.execution_callback)(proposal_id, payload, decided_at, domain_id) {
            Ok(()) => {
                info!(
                    proposal_id = %proposal_id,
                    "Proposal execution succeeded"
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    proposal_id = %proposal_id,
                    error = %e,
                    "Proposal execution failed"
                );
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_success() {
        let callback: ExecutionCallback = Arc::new(|_id, _payload, _decided_at, _domain| Ok(()));
        let executor = GovernanceProposalExecutor::new(callback);

        let result = executor.execute_proposal(
            "proposal-123",
            &serde_json::json!({"type": "test"}),
            1000,
            "domain-1",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_executor_failure() {
        let callback: ExecutionCallback =
            Arc::new(|_id, _payload, _decided_at, _domain| Err("test error".to_string()));
        let executor = GovernanceProposalExecutor::new(callback);

        let result = executor.execute_proposal(
            "proposal-123",
            &serde_json::json!({"type": "test"}),
            1000,
            "domain-1",
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "test error");
    }

    #[test]
    fn test_executor_passes_arguments() {
        let callback: ExecutionCallback = Arc::new(|id, payload, decided_at, domain| {
            assert_eq!(id, "proposal-456");
            assert_eq!(payload["type"], "budget");
            assert_eq!(decided_at, 2000);
            assert_eq!(domain, "domain-2");
            Ok(())
        });
        let executor = GovernanceProposalExecutor::new(callback);

        let result = executor.execute_proposal(
            "proposal-456",
            &serde_json::json!({"type": "budget"}),
            2000,
            "domain-2",
        );

        assert!(result.is_ok());
    }
}
