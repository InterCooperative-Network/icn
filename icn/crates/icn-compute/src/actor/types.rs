//! Actor-specific types and callbacks for the compute actor.

use std::sync::Arc;

use crate::scheduler::NodeCapacity;
use crate::types::{ComputeMessage, ComputeResult, ExecutorCapability, TaskHash};

/// Information about an available executor
#[derive(Debug, Clone)]
pub(crate) struct ExecutorInfo {
    /// Executor's DID
    pub did: String,
    /// Cooperative ID this executor belongs to (None = local/same cooperative)
    #[allow(dead_code)]
    pub cooperative_id: Option<String>,
    /// Whether this executor is from a federated cooperative
    #[allow(dead_code)]
    pub is_federated: bool,
    /// Capabilities this executor offers
    pub capabilities: Vec<ExecutorCapability>,
    /// Current trust score (local, from icn-trust)
    #[allow(dead_code)]
    pub trust_score: f64,
    /// Federated trust score (attenuated based on coop trust)
    /// Formula: federated_trust = local_trust × coop_trust × attenuation_factor
    #[allow(dead_code)]
    pub federated_trust_score: Option<f64>,
    /// Last announcement timestamp (milliseconds since epoch, used for staleness detection)
    #[allow(dead_code)]
    pub last_seen: u64,
    /// Number of tasks currently executing
    pub tasks_executing: usize,
    /// Current capacity (CPU, memory, storage, GPU)
    pub capacity: Option<NodeCapacity>,
    /// Gateway endpoint for federated executors (used for result delivery)
    #[allow(dead_code)]
    pub gateway_endpoint: Option<String>,
}

/// Consensus tracking for task results
#[derive(Debug, Clone)]
pub(crate) struct ResultConsensus {
    /// Task hash (used for tracking in HashMap)
    #[allow(dead_code)]
    pub task_hash: TaskHash,
    /// Results received from different executors
    pub results: Vec<ComputeResult>,
    /// Number of required confirmations (default: 1 for now)
    pub required: usize,
}

/// Callback for sending compute messages via gossip
pub type SendCallback = Arc<dyn Fn(ComputeMessage) + Send + Sync>;

/// Callback for looking up trust scores
pub type TrustCallback = Arc<dyn Fn(&str) -> f64 + Send + Sync>;

/// Payment settlement request
#[derive(Debug, Clone)]
pub struct PaymentRequest {
    /// Payer DID (task submitter)
    pub from: String,
    /// Payee DID (executor)
    pub to: String,
    /// Amount to pay
    pub amount: u64,
    /// Currency
    pub currency: String,
    /// Task ID for memo
    pub task_id: String,
}

/// Callback for settling payments via ledger
pub type PaymentCallback = Arc<dyn Fn(PaymentRequest) + Send + Sync>;

/// Compute event for external notification
#[derive(Debug, Clone)]
pub enum ComputeEvent {
    /// A task was claimed by an executor
    TaskClaimed { task_hash: String, executor: String },
    /// A task completed execution
    TaskCompleted {
        task_hash: String,
        executor: String,
        outcome: String,
        fuel_used: u64,
        duration_ms: u64,
    },
}

/// Callback for broadcasting compute events (e.g., to WebSocket clients)
pub type EventCallback = Arc<dyn Fn(ComputeEvent) + Send + Sync>;

/// Callback for querying network locality data for a peer (Phase 16C M5 integration)
///
/// Takes a peer DID and returns locality context including:
/// - RTT to the peer (from network topology)
/// - Blob locality information
/// - Region information
pub type LocalityCallback = Arc<dyn Fn(&str) -> crate::scheduler::LocalityContext + Send + Sync>;
