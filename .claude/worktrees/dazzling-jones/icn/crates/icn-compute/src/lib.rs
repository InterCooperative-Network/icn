//! ICN Distributed Compute Layer
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! Trust-gated task distribution and execution for cooperative networks.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  Submitter  │────▶│   Gossip    │────▶│  Executor   │
//! │  (Task)     │     │  (Routing)  │     │  (Result)   │
//! └─────────────┘     └─────────────┘     └─────────────┘
//!       │                   │                   │
//!       └───────────────────┴───────────────────┘
//!                     Trust Graph
//!                 (Access Control)
//! ```
//!
//! # Topics
//!
//! - `compute:submit` - Task submission
//! - `compute:claim` - Executor claims task
//! - `compute:result` - Execution results

mod actor;
mod actor_model;
mod actor_runtime;
mod checkpoint_store;
pub mod commons_pool;
mod dispute;
mod error;
mod executor;
mod federation;
mod migration_manager;
mod migration_policy;
mod policy;
pub mod receipt;
mod result_quorum;
mod scheduler;
mod task;
mod types;
mod wasm_executor;
mod wasm_registry;

pub use actor::{
    BalanceCallback, CommonsPaymentRequest, CommonsSettlementCallback, ComputeActor, ComputeEvent,
    ComputeHandle, EventCallback, LocalityCallback, PaymentCallback, PaymentRequest, SendCallback,
    TrustCallback,
};
pub use actor_model::{
    ActorCheckpoint, ActorEvent, ActorId, ActorMode, ActorRuntimeState, MigrationDecision,
    MigrationReason, MigrationState, TerminationReason,
};
pub use actor_runtime::{
    ActorRuntimeCallback, ActorRuntimeCommand, PauseReason, StatefulActorInfo,
    StatefulActorRegistry, StatefulActorState,
};
pub use checkpoint_store::{
    CheckpointBackend, CheckpointStore, InMemoryBackend, SledCheckpointBackend,
};
pub use commons_pool::{
    AggregateCapacity, CommonsParticipant, CommonsPool, SybilPolicy, SybilRejection,
};
pub use dispute::{
    ComputeDispute, ComputeResult as DisputeComputeResult, DisputeManager, DisputeResolution,
    DisputeStatus, Evidence, EvidenceType, VerificationMode,
};
pub use error::ComputeError;
pub use executor::{ContractResolverCallback, ExecutionContext, Executor, LocalExecutor};
pub use federation::{
    FederatedExecutorAttestation, FederatedExecutorInfo, FederatedExecutorRegistry,
    FederatedPaymentTerms, PaymentTrigger,
};
pub use migration_manager::{ActorMigrationManager, MigrationSender};
pub use migration_policy::{
    DefaultMigrationPolicy, ExecutorInfo, LocalityFirstPolicy, MigrationPolicy, NetworkState,
};
pub use policy::{
    CharterPriority, CommonsPoolPolicy, CoopSchedulingPolicy, EnforcementMode, MemberQuota,
    PlacementConstraints, PolicyDecision, PolicyManager, SchedulingRule, UsageRecord, UsageTracker,
};
pub use receipt::{ExecutionReceipt, ReceiptHash, SettlementMessage, SignatureBytes};
pub use result_quorum::{
    CollectedResult, QuorumStatus, ResultAggregator, ResultQuorumManager, TaskValue,
    TaskVerification, VerificationConfig, CLOCK_SKEW_TOLERANCE_MS, DEFAULT_COLLECTION_WINDOW_MS,
    MAX_CONCURRENT_VERIFICATIONS,
};
pub use scheduler::{
    CapacityBudget, DefaultPlacementPolicy, DemandAdjustmentConfig, FederatedPlacementConstraints,
    FederationPolicy, GpuDevice, GpuSpec, LocalityContext, LocalityHint, NodeCapacity, NodeState,
    PlacementOffer, PlacementPolicy, PlacementRequest, ResourceChangeType, ResourceProfile,
    ResourceRefreshConfig, ScopeContext,
};
pub use task::{TaskManager, TaskStatus};
pub use types::{
    ComputeMessage, ComputeResult, ComputeTask, ContentHash, DeterminismClass, ExecutionOutcome,
    ExecutorCapability, FuelLimit, PrivacyClass, TaskCode, TaskHash, TaskId, TaskPriority,
};
pub use wasm_executor::WasmExecutor;
pub use wasm_registry::{
    compute_hash as compute_wasm_hash, WasmHash, WasmMetadata, WasmRegistry, WasmRegistryError,
    WasmRegistryStats,
};

/// Gossip topic for task submission
pub const TOPIC_SUBMIT: &str = "compute:submit";

/// Gossip topic for task claims
pub const TOPIC_CLAIM: &str = "compute:claim";

/// Gossip topic for results
pub const TOPIC_RESULT: &str = "compute:result";

/// Gossip topic for cancellations
pub const TOPIC_CANCEL: &str = "compute:cancel";

/// Gossip topic for checkpoint announcements (Phase 16D)
pub const TOPIC_CHECKPOINT: &str = "compute:checkpoint";

/// Gossip topic for migration coordination (Phase 16D)
pub const TOPIC_MIGRATION: &str = "compute:migration";

/// Gossip topic for dispute resolution (Phase 20)
pub const TOPIC_DISPUTE: &str = "compute:dispute";

/// Gossip topic for cross-cooperative federation (Phase 21)
pub const TOPIC_FEDERATION: &str = "compute:federation";

/// Gossip topic for settlement receipts (Epic 4)
pub const TOPIC_SETTLEMENT_RECEIPT: &str = "settlement:receipt";

/// Gossip topic for settlement disputes (Epic 4)
pub const TOPIC_SETTLEMENT_DISPUTE: &str = "settlement:dispute";

/// Minimum trust score to submit tasks (0.0 - 1.0)
pub const MIN_TRUST_SUBMIT: f64 = 0.1;

/// Minimum trust score to execute tasks (0.0 - 1.0)
pub const MIN_TRUST_EXECUTE: f64 = 0.3;

/// Deliberation period for placement offers in milliseconds (Phase 16B)
///
/// Executors wait this duration after receiving a PlacementRequest before
/// broadcasting their offers. Uses relative timing based on request timestamp
/// to avoid clock skew issues (M9 fix).
pub const DELIBERATION_PERIOD_MS: u64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topics_have_correct_prefix() {
        // Verify topics follow the expected naming convention
        assert!(TOPIC_SUBMIT.starts_with("compute:"));
        assert!(TOPIC_CLAIM.starts_with("compute:"));
        assert!(TOPIC_RESULT.starts_with("compute:"));
    }
}
