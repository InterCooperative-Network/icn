//! ICN Distributed Compute Layer
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
mod checkpoint_store;
mod error;
mod executor;
mod migration_manager;
mod migration_policy;
mod policy;
mod scheduler;
mod task;
mod types;
mod wasm_executor;
mod wasm_registry;

pub use actor::{
    ComputeActor, ComputeEvent, ComputeHandle, EventCallback, PaymentCallback, PaymentRequest,
    SendCallback, TrustCallback,
};
pub use actor_model::{
    ActorCheckpoint, ActorEvent, ActorId, ActorMode, ActorRuntimeState, MigrationDecision,
    MigrationReason, MigrationState, TerminationReason,
};
pub use checkpoint_store::{
    CheckpointBackend, CheckpointStore, InMemoryBackend, SledCheckpointBackend,
};
pub use error::ComputeError;
pub use executor::{ExecutionContext, Executor, LocalExecutor};
pub use migration_manager::{ActorMigrationManager, MigrationSender};
pub use migration_policy::{
    DefaultMigrationPolicy, ExecutorInfo, LocalityFirstPolicy, MigrationPolicy, NetworkState,
};
pub use policy::{
    CoopSchedulingPolicy, EnforcementMode, MemberQuota, PlacementConstraints, PolicyDecision,
    PolicyManager, SchedulingRule, UsageRecord, UsageTracker,
};
pub use scheduler::{
    DefaultPlacementPolicy, GpuDevice, GpuSpec, LocalityContext, LocalityHint, NodeCapacity,
    NodeState, PlacementOffer, PlacementPolicy, PlacementRequest, ResourceProfile,
};
pub use task::{TaskManager, TaskStatus};
pub use types::{
    ComputeMessage, ComputeResult, ComputeTask, ExecutionOutcome, ExecutorCapability, FuelLimit,
    TaskCode, TaskHash, TaskId, TaskPriority,
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

/// Minimum trust score to submit tasks (0.0 - 1.0)
pub const MIN_TRUST_SUBMIT: f64 = 0.1;

/// Minimum trust score to execute tasks (0.0 - 1.0)
pub const MIN_TRUST_EXECUTE: f64 = 0.3;

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
