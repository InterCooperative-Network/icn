//! ICN Governance Actor
//!
//! This crate provides the [`GovernanceActor`] for distributed decision-making
//! across the ICN network. It manages governance state, proposal lifecycle,
//! voting, and outcome evaluation.
//!
//! ## Architecture
//!
//! The governance actor is a domain-specific application that sits above the
//! kernel layer. It depends on kernel-api traits (e.g., [`EventEmitter`],
//! [`ProtocolParameterStore`]) rather than concrete kernel implementations,
//! maintaining clean separation between kernel mechanisms and domain semantics.
//!
//! [`EventEmitter`]: icn_kernel_api::events::EventEmitter
//! [`ProtocolParameterStore`]: icn_kernel_api::protocol_params::ProtocolParameterStore

pub mod actor;
pub mod executor;

pub use actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite, GovernanceHandle};
pub use executor::{ExecutionCallback, GovernanceProposalExecutor};

// Re-export governance types needed by supervisor initialization.
// This allows icn-core to import from icn_governance_actor instead of
// icn_governance directly, reducing direct governance coupling.
pub use icn_governance::{MembershipResolver, StaticMembershipResolver};

/// Create a ProposalExecutor instance.
///
/// This is the entry point for proposal execution. The executor requires
/// an execution callback that has access to icn-core internals.
///
/// # Arguments
/// * `execution_callback` - Callback for executing proposals with domain logic
pub fn create_executor(
    execution_callback: executor::ExecutionCallback,
) -> std::sync::Arc<dyn icn_kernel_api::services::ProposalExecutor> {
    std::sync::Arc::new(GovernanceProposalExecutor::new(execution_callback))
}
