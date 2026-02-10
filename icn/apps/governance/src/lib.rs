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
//! [`ProtocolParameterStore`], [`GovernanceExecutor`]) rather than concrete
//! kernel implementations, maintaining clean separation between kernel
//! mechanisms and domain semantics.
//!
//! ## Handlers
//!
//! The [`handlers`] module provides the execution callback infrastructure for
//! processing accepted proposals. Each handler type (treasury, protocol,
//! federation) implements the [`handlers::ExecutionCallback`] trait.
//!
//! When kernel executors are configured via [`GovernanceHandle::with_executor`],
//! handlers can delegate execution to the kernel-provided executor traits:
//! - [`TreasuryExecutor`] for treasury operations
//! - [`ProtocolExecutor`] for protocol parameter changes
//!
//! [`EventEmitter`]: icn_kernel_api::events::EventEmitter
//! [`ProtocolParameterStore`]: icn_kernel_api::protocol_params::ProtocolParameterStore
//! [`GovernanceExecutor`]: icn_kernel_api::governance::GovernanceExecutor
//! [`TreasuryExecutor`]: icn_kernel_api::governance::TreasuryExecutor
//! [`ProtocolExecutor`]: icn_kernel_api::governance::ProtocolExecutor

pub mod actor;
pub mod executor;
pub mod handlers;
pub mod registry;

pub use actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite, GovernanceHandle};
pub use executor::{ExecutionCallback, GovernanceProposalExecutor};
pub use handlers::{
    translate_payload_to_effects, Allocation, AllocationReceipt, DisburseParams,
    ExecutionCallback as HandlerCallback, FederationHandler, ProposalExecutionContext,
    ProposalType, ProtocolHandler, TreasuryHandler,
};

// Re-export registry types for decision/meeting management
pub use registry::{DecisionFilter, DecisionIndexEntry, DecisionRegistry, DecisionStatus, Meeting};

// Re-export kernel effect types for kernel/app boundary
pub use icn_kernel_api::effects::{
    ControlEffect, EffectResult, FederationEffect, KernelEffect, MembershipEffect, ProtocolEffect,
    TreasuryEffect,
};

// Re-export kernel governance traits for convenience
pub use icn_kernel_api::governance::{
    DecisionReceiptId, ExecutionOutcome, GovernanceExecutor, ProtocolChange, ProtocolExecutor,
    TreasuryExecutor, TreasuryOperation, TreasuryOperationType,
};

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
