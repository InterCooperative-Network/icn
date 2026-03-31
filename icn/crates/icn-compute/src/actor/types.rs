//! Actor-specific types and callbacks for the compute actor.

use std::sync::Arc;

use crate::scheduler::NodeCapacity;
use crate::types::{ComputeMessage, ComputeResult, ExecutorCapability, TaskHash};

/// Information about an available executor
///
/// Some fields are reserved for future federated compute features and are
/// currently unused. See Phase 29 cleanup (#156) for tracking.
#[derive(Debug, Clone)]
pub(crate) struct ExecutorInfo {
    /// Executor's DID
    pub did: String,
    /// Cooperative ID this executor belongs to (None = local/same cooperative)
    /// Reserved: Federated compute - cross-cooperative task routing
    #[allow(dead_code)]
    pub cooperative_id: Option<String>,
    /// Whether this executor is from a federated cooperative
    /// Reserved: Federated compute - trust attenuation for remote executors
    #[allow(dead_code)]
    pub is_federated: bool,
    /// Capabilities this executor offers
    pub capabilities: Vec<ExecutorCapability>,
    /// Current trust score (local, from icn-trust)
    /// Reserved: Trust-gated execution - minimum trust for task assignment
    #[allow(dead_code)]
    pub trust_score: f64,
    /// Federated trust score (attenuated based on coop trust)
    /// Formula: federated_trust = local_trust × coop_trust × attenuation_factor
    /// Reserved: Federated compute - cross-cooperative trust scoring
    #[allow(dead_code)]
    pub federated_trust_score: Option<f64>,
    /// Last announcement timestamp (milliseconds since epoch, used for staleness detection)
    /// Reserved: Executor health - detect stale/offline executors
    #[allow(dead_code)]
    pub last_seen: u64,
    /// Number of tasks currently executing
    pub tasks_executing: usize,
    /// Current capacity (CPU, memory, storage, GPU)
    pub capacity: Option<NodeCapacity>,
    /// Gateway endpoint for federated executors (used for result delivery)
    /// Reserved: Federated compute - route results back through gateway
    #[allow(dead_code)]
    pub gateway_endpoint: Option<String>,
}

/// Consensus tracking for task results
///
/// Tracks results from multiple executors for Byzantine fault tolerance.
#[derive(Debug, Clone)]
pub(crate) struct ResultConsensus {
    /// Task hash (used for tracking in HashMap)
    /// Reserved: Multi-executor consensus - correlate results across executors
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

/// Notification emitted when a federated task result is confirmed and
/// a cross-entity clearing obligation should be recorded.
///
/// The compute layer fires this callback after a federated executor reports
/// task completion. The receiving layer (gateway/lifecycle) converts it into
/// a `ClearingReceipt` and queues it into `ReceiptClearingManager`.
///
/// Keeping this separate from `PaymentRequest` preserves the semantic
/// distinction between local settlement (ledger debit/credit) and
/// cross-entity clearing (netting → scheduled settlement).
///
/// Note: fields use `entity_id` (not `coop_id`) because the ICN substrate
/// supports multiple organizational kinds (cooperatives, communities,
/// federations). The downstream clearing infrastructure maps entity IDs to
/// the appropriate clearing agreement.
#[derive(Debug, Clone)]
pub struct FederationClearingNotification {
    /// Entity ID of the submitter (task buyer — cooperative, community, or federation)
    pub from_entity_id: String,
    /// DID of the submitter node
    pub from_did: String,
    /// Entity ID of the executor (task seller — cooperative, community, or federation)
    pub to_entity_id: String,
    /// DID of the executor node
    pub to_did: String,
    /// Settlement amount (in smallest currency unit)
    pub amount: u64,
    /// Currency for the transfer
    pub currency: String,
    /// Task identifier for memo/audit trail
    pub task_id: String,
    /// Attestation hash — used as the clearing receipt's dedup key
    pub attestation_hash: [u8; 32],
}

/// Callback for recording cross-cooperative clearing obligations.
pub type FederationClearingCallback = Arc<dyn Fn(FederationClearingNotification) + Send + Sync>;

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
    /// Resource capacity changed significantly
    ResourcesChanged {
        executor: String,
        capacity: NodeCapacity,
        changes: Vec<crate::scheduler::ResourceChangeType>,
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

/// Callback for querying a submitter's ledger balance (E7 - commons credit ceiling checks).
///
/// Takes a submitter DID string and returns their current credit balance as a signed integer.
/// Negative balances indicate debt. Used by `CommonsPoolPolicy::validate_submitter_credit`.
pub type BalanceCallback = Arc<dyn Fn(&str) -> i64 + Send + Sync>;

/// Commons credit settlement request (E7 - #948).
///
/// Fired by the compute actor when a commons-scope task completes successfully.
/// The injected `CommonsSettlementCallback` is responsible for calling
/// `SettlementEngine::settle_commons_receipt()` and appending the resulting
/// journal entries to the ledger.
#[derive(Debug, Clone)]
pub struct CommonsPaymentRequest {
    /// DID of the contributor (executor — earns commons credits from the mint).
    pub contributor: String,
    /// DID of the consumer (submitter — spends commons credits to the mint).
    pub consumer: String,
    /// Amount of commons credits to settle (already computed by the actor).
    pub amount: u64,
    /// Receipt hash for deduplication — used as nonce in journal entries.
    pub receipt_hash: [u8; 32],
    /// Task ID for audit trail.
    pub task_id: String,
}

/// Callback for settling commons credits via the ledger (E7 - #948).
///
/// Injected into `ComputeActor` at construction time by whoever owns the
/// `SettlementEngine`. The callback is responsible for:
/// 1. Fetching the consumer's current commons credit balance.
/// 2. Constructing a `CommonsSettlementRequest` and calling `settle_commons_receipt()`.
/// 3. Appending the resulting earn/spend entries to the ledger.
///
/// **Settlement semantics change with reservation (#1404):** once `CommonsReserveCallback`
/// is configured, the app-layer implementation of this callback must reconcile against
/// the hold established for `task_id` rather than performing an unrelated free-pool debit.
/// The callback interface is unchanged; the contract is not.
pub type CommonsSettlementCallback = Arc<dyn Fn(CommonsPaymentRequest) + Send + Sync>;

/// Request to reserve commons credits at task submission time (#1404).
///
/// Fired by `handle_submit()` for Commons-scoped tasks after the advisory balance
/// floor check passes. The callee MUST atomically read the submitter's balance and
/// hold `amount` credits against `task_id`. If the balance is insufficient after
/// the atomic read (race condition — another task consumed credits between the
/// advisory check and this call), return `Err(reason)`.
#[derive(Debug, Clone)]
pub struct CommonsReserveRequest {
    /// DID of the consumer (submitter — credits are held from their balance).
    pub consumer: String,
    /// Credits to reserve. Computed by `cost::compute_credits_required(&task)`.
    pub amount: i64,
    /// Task ID used as the reservation key. Unique per submission.
    pub task_id: String,
}

/// Authoritative race-free admission gate for commons tasks (#1404).
///
/// The advisory balance check in `handle_submit()` is a fast-fail hint only.
/// This callback is the **authoritative** gate: it must atomically read-and-hold,
/// making it safe against concurrent submissions from the same submitter.
///
/// Returns `Ok(())` if successfully reserved; `Err(reason)` if the balance is
/// insufficient (race condition). On `Err`, `handle_submit()` rejects the task.
///
/// **Backward compatibility:** if this callback is not configured, the reservation
/// gate is skipped silently. This is **rollout compatibility mode**, not race-free
/// correctness mode. Configure this callback to enable correct enforcement.
pub type CommonsReserveCallback =
    Arc<dyn Fn(CommonsReserveRequest) -> Result<(), String> + Send + Sync>;

/// Request to release a previously reserved commons credit hold (#1404).
///
/// Fired by `release_commons_reservation()` on all non-success termination paths:
/// execution failure, timeout, explicit cancel, and WasmRef resolution failure
/// (both missing-registry and fetch-error cases). See `lifecycle.rs` module doc
/// for the complete state-machine diagram.
#[derive(Debug, Clone)]
pub struct CommonsReleaseRequest {
    /// DID of the consumer (submitter).
    pub consumer: String,
    /// Task ID used as the reservation key.
    pub task_id: String,
    /// Originally reserved amount (for audit). The callee keys on `task_id`, not this.
    pub reserved_amount: i64,
}

/// Callback to release a pending commons credit reservation (#1404).
///
/// Must be idempotent: if no reservation exists for `task_id`, the callee
/// must succeed silently (no-op). This handles double-release races gracefully.
pub type CommonsReleaseCallback = Arc<dyn Fn(CommonsReleaseRequest) + Send + Sync>;
