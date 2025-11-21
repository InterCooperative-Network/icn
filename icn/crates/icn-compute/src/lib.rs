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
mod error;
mod executor;
mod task;
mod types;

pub use actor::{ComputeActor, ComputeHandle, PaymentCallback, PaymentRequest, SendCallback, TrustCallback};
pub use error::ComputeError;
pub use executor::{ExecutionContext, Executor, LocalExecutor};
pub use task::{TaskManager, TaskStatus};
pub use types::{
    ComputeMessage, ComputeResult, ComputeTask, ExecutionOutcome, ExecutorCapability, FuelLimit,
    TaskCode, TaskHash, TaskId, TaskPriority,
};

/// Gossip topic for task submission
pub const TOPIC_SUBMIT: &str = "compute:submit";

/// Gossip topic for task claims
pub const TOPIC_CLAIM: &str = "compute:claim";

/// Gossip topic for results
pub const TOPIC_RESULT: &str = "compute:result";

/// Gossip topic for cancellations
pub const TOPIC_CANCEL: &str = "compute:cancel";

/// Minimum trust score to submit tasks (0.0 - 1.0)
pub const MIN_TRUST_SUBMIT: f64 = 0.1;

/// Minimum trust score to execute tasks (0.0 - 1.0)
pub const MIN_TRUST_EXECUTE: f64 = 0.3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topics_defined() {
        assert!(!TOPIC_SUBMIT.is_empty());
        assert!(!TOPIC_CLAIM.is_empty());
        assert!(!TOPIC_RESULT.is_empty());
    }
}
