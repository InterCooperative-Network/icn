//! Governance proposal execution handlers.
//!
//! These handlers process accepted proposals and execute their effects
//! on the system (treasury operations, protocol changes, federation governance).
//!
//! # Architecture
//!
//! The handler infrastructure follows a trait-based callback pattern:
//! - [`ExecutionCallback`] trait defines the interface for proposal execution
//! - [`ProposalExecutionContext`] provides execution context
//! - Handler implementations are injected at runtime
//!
//! This allows the governance actor to remain decoupled from icn-core internals
//! while still executing domain-specific operations.

mod execution;
mod federation;
mod protocol;
mod treasury;

pub use execution::{
    DecisionReceiptId, ExecutionCallback, ExecutionOutcome, GovernanceExecutor,
    ProposalExecutionContext, ProposalType, ProtocolChange, ProtocolExecutor, TreasuryExecutor,
    TreasuryOperation, TreasuryOperationType,
};
pub use federation::FederationHandler;
pub use protocol::ProtocolHandler;
pub use treasury::{Allocation, AllocationReceipt, DisburseParams, TreasuryHandler};
