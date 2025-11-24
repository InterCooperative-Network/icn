//! Compute actor for distributed task execution.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::checkpoint_store::CheckpointStore;
use crate::error::ComputeError;
use crate::executor::{Executor, LocalExecutor};
use crate::migration_manager::ActorMigrationManager;
use crate::scheduler::PlacementPolicy;
use crate::task::{TaskManager, TaskStatus};
use crate::types::{ComputeMessage, ComputeResult, ComputeTask, ExecutorCapability, TaskHash};
use crate::{MIN_TRUST_EXECUTE, MIN_TRUST_SUBMIT};

/// Information about an available executor
#[derive(Debug, Clone)]
struct ExecutorInfo {
    /// Executor's DID
    did: String,
    /// Capabilities this executor offers
    capabilities: Vec<ExecutorCapability>,
    /// Current trust score (used for future executor selection)
    #[allow(dead_code)]
    trust_score: f64,
    /// Last announcement timestamp (milliseconds since epoch, used for staleness detection)
    #[allow(dead_code)]
    last_seen: u64,
    /// Number of tasks currently executing
    tasks_executing: usize,
}

/// Consensus tracking for task results
#[derive(Debug, Clone)]
struct ResultConsensus {
    /// Task hash (used for tracking in HashMap)
    #[allow(dead_code)]
    task_hash: TaskHash,
    /// Results received from different executors
    results: Vec<ComputeResult>,
    /// Number of required confirmations (default: 1 for now)
    required: usize,
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
    TaskClaimed {
        task_hash: String,
        executor: String,
    },
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

/// Handle for interacting with the ComputeActor
#[derive(Clone)]
pub struct ComputeHandle {
    tx: mpsc::Sender<ComputeCommand>,
}

impl ComputeHandle {
    /// Submit a task for distributed execution
    pub async fn submit(&self, task: ComputeTask) -> Result<TaskHash, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::Submit { task, resp: resp_tx })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// Get task status
    pub async fn status(&self, hash: TaskHash) -> Result<Option<TaskStatus>, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::Status { hash, resp: resp_tx })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))
    }

    /// Cancel a task
    pub async fn cancel_task(
        &self,
        hash: &TaskHash,
        requester: &str,
        reason: String,
    ) -> Result<(), ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::Cancel {
                hash: *hash,
                requester: requester.to_string(),
                reason,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// Handle incoming gossip message
    pub async fn handle_gossip(&self, msg: ComputeMessage) -> Result<(), ComputeError> {
        self.tx
            .send(ComputeCommand::GossipMessage(msg))
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))
    }

    // Policy management methods (Phase 16E)

    /// Set or update a cooperative scheduling policy
    pub async fn set_policy(
        &self,
        policy: crate::policy::CoopSchedulingPolicy,
    ) -> Result<(), ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::SetPolicy { policy, resp: resp_tx })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// Get policy for a cooperative
    pub async fn get_policy(&self, coop_id: &str) -> Option<crate::policy::CoopSchedulingPolicy> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::GetPolicy {
                coop_id: coop_id.to_string(),
                resp: resp_tx,
            })
            .await
            .ok()?;
        resp_rx.await.ok()?
    }

    /// List all policies
    pub async fn list_policies(&self) -> Vec<crate::policy::CoopSchedulingPolicy> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::ListPolicies { resp: resp_tx })
            .await
            .ok();
        resp_rx.await.unwrap_or_default()
    }

    /// Remove a policy
    pub async fn remove_policy(&self, coop_id: &str) -> Option<crate::policy::CoopSchedulingPolicy> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::RemovePolicy {
                coop_id: coop_id.to_string(),
                resp: resp_tx,
            })
            .await
            .ok()?;
        resp_rx.await.ok()?
    }

    /// Get usage record for a member in a cooperative
    pub async fn get_usage(
        &self,
        coop_id: &str,
        member_did: &str,
    ) -> Result<crate::policy::UsageRecord, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::GetUsage {
                coop_id: coop_id.to_string(),
                member_did: member_did.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }

    /// List all usage records for a cooperative
    pub async fn list_coop_usage(
        &self,
        coop_id: &str,
    ) -> Result<Vec<crate::policy::UsageRecord>, ComputeError> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ComputeCommand::ListCoopUsage {
                coop_id: coop_id.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| ComputeError::Internal("actor closed".into()))?;
        resp_rx
            .await
            .map_err(|_| ComputeError::Internal("no response".into()))?
    }
}

/// Commands sent to the ComputeActor
enum ComputeCommand {
    Submit {
        task: ComputeTask,
        resp: tokio::sync::oneshot::Sender<Result<TaskHash, ComputeError>>,
    },
    Status {
        hash: TaskHash,
        resp: tokio::sync::oneshot::Sender<Option<TaskStatus>>,
    },
    Cancel {
        hash: TaskHash,
        requester: String,
        reason: String,
        resp: tokio::sync::oneshot::Sender<Result<(), ComputeError>>,
    },
    GossipMessage(ComputeMessage),
    // Policy management commands (Phase 16E)
    SetPolicy {
        policy: crate::policy::CoopSchedulingPolicy,
        resp: tokio::sync::oneshot::Sender<Result<(), ComputeError>>,
    },
    GetPolicy {
        coop_id: String,
        resp: tokio::sync::oneshot::Sender<Option<crate::policy::CoopSchedulingPolicy>>,
    },
    ListPolicies {
        resp: tokio::sync::oneshot::Sender<Vec<crate::policy::CoopSchedulingPolicy>>,
    },
    RemovePolicy {
        coop_id: String,
        resp: tokio::sync::oneshot::Sender<Option<crate::policy::CoopSchedulingPolicy>>,
    },
    GetUsage {
        coop_id: String,
        member_did: String,
        resp: tokio::sync::oneshot::Sender<Result<crate::policy::UsageRecord, ComputeError>>,
    },
    ListCoopUsage {
        coop_id: String,
        resp: tokio::sync::oneshot::Sender<Result<Vec<crate::policy::UsageRecord>, ComputeError>>,
    },
}

/// Actor managing distributed compute tasks
pub struct ComputeActor {
    /// Our DID
    own_did: String,
    /// Task manager
    task_manager: Arc<Mutex<TaskManager>>,
    /// Local executor
    executor: Arc<LocalExecutor>,
    /// Callback to send gossip messages
    send_callback: Option<SendCallback>,
    /// Callback to lookup trust scores
    trust_callback: TrustCallback,
    /// Callback to settle payments
    payment_callback: Option<PaymentCallback>,
    /// Callback to broadcast compute events
    event_callback: Option<EventCallback>,
    /// Signing key for results (placeholder)
    #[allow(dead_code)]
    signing_key: Vec<u8>,
    /// Registry of available executors
    executor_registry: Arc<Mutex<HashMap<String, ExecutorInfo>>>,
    /// Pending consensus tracking for task results
    pending_consensus: Arc<Mutex<HashMap<TaskHash, ResultConsensus>>>,
    /// Pending placement offers (Phase 16B)
    pending_offers: Arc<Mutex<HashMap<TaskHash, Vec<crate::scheduler::PlacementOffer>>>>,
    /// Track when placement requests were sent (for duration metrics)
    pending_request_timestamps: Arc<Mutex<HashMap<TaskHash, u64>>>,
    /// Maximum concurrent tasks this executor will claim
    max_concurrent_tasks: usize,
    /// Checkpoint store for actor state persistence (Phase 16D)
    checkpoint_store: Option<Arc<CheckpointStore>>,
    /// Migration manager for actor migrations (Phase 16D)
    migration_manager: Option<Arc<ActorMigrationManager>>,
    /// Policy manager for cooperative scheduling policies (Phase 16E)
    policy_manager: Option<Arc<crate::policy::PolicyManager>>,
}

impl ComputeActor {
    /// Create a new compute actor
    pub fn new(own_did: String, trust_callback: TrustCallback) -> Self {
        Self {
            own_did,
            task_manager: Arc::new(Mutex::new(TaskManager::default())),
            executor: Arc::new(LocalExecutor::new()),
            send_callback: None,
            trust_callback,
            payment_callback: None,
            event_callback: None,
            signing_key: vec![],
            executor_registry: Arc::new(Mutex::new(HashMap::new())),
            pending_consensus: Arc::new(Mutex::new(HashMap::new())),
            pending_offers: Arc::new(Mutex::new(HashMap::new())),
            pending_request_timestamps: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent_tasks: 10, // Default: 10 concurrent tasks
            checkpoint_store: None,
            migration_manager: None,
            policy_manager: None,
        }
    }

    /// Set checkpoint store for actor state persistence
    pub fn set_checkpoint_store(&mut self, store: Arc<CheckpointStore>) {
        self.checkpoint_store = Some(store);
    }

    /// Set migration manager for actor migrations
    pub fn set_migration_manager(&mut self, manager: Arc<ActorMigrationManager>) {
        self.migration_manager = Some(manager);
    }

    /// Set policy manager for cooperative scheduling policies
    pub fn set_policy_manager(&mut self, manager: Arc<crate::policy::PolicyManager>) {
        self.policy_manager = Some(manager);
    }

    /// Set maximum concurrent tasks this executor will claim
    pub fn set_max_concurrent_tasks(&mut self, max: usize) {
        self.max_concurrent_tasks = max;
    }

    /// Check if we're at capacity for claiming new tasks
    async fn at_capacity(&self) -> bool {
        let registry = self.executor_registry.lock().await;
        if let Some(info) = registry.get(&self.own_did) {
            info.tasks_executing >= self.max_concurrent_tasks
        } else {
            false // If we're not registered, we can claim
        }
    }

    /// Set the callback for sending gossip messages
    pub fn set_send_callback(&mut self, cb: SendCallback) {
        self.send_callback = Some(cb);
    }

    /// Set the callback for settling payments
    pub fn set_payment_callback(&mut self, cb: PaymentCallback) {
        self.payment_callback = Some(cb);
    }

    /// Set the callback for broadcasting compute events
    pub fn set_event_callback(&mut self, cb: EventCallback) {
        self.event_callback = Some(cb);
    }

    /// Set the signing key for result signatures
    pub fn set_signing_key(&mut self, key: Vec<u8>) {
        self.signing_key = key;
    }

    /// Spawn the actor and return a handle
    pub fn spawn(self) -> ComputeHandle {
        let (tx, mut rx) = mpsc::channel::<ComputeCommand>(256);

        // Clone Arc references for the timeout checker
        let task_manager_clone = Arc::clone(&self.task_manager);
        let executor_registry_clone = Arc::clone(&self.executor_registry);
        let send_callback_clone = self.send_callback.clone();

        // Spawn timeout checker task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                if let Err(e) = Self::check_timeouts(
                    &task_manager_clone,
                    &executor_registry_clone,
                    &send_callback_clone,
                ).await {
                    tracing::warn!("timeout checker error: {}", e);
                }
            }
        });

        // Spawn migration manager task (Phase 16D Week 4)
        if let Some(ref migration_manager) = self.migration_manager {
            let manager_clone = Arc::clone(migration_manager);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;

                    // Detect and fail timed-out migrations (60 second timeout)
                    if let Err(e) = manager_clone.detect_timeouts(60).await {
                        tracing::warn!("migration timeout detection error: {}", e);
                    }

                    // Cleanup old migration records (keep for 5 minutes)
                    let _removed = manager_clone.cleanup_migrations(300).await;
                }
            });
        }

        // Spawn main command loop
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    ComputeCommand::Submit { task, resp } => {
                        let result = self.handle_submit(task).await;
                        let _ = resp.send(result);
                    }
                    ComputeCommand::Status { hash, resp } => {
                        let status = self.task_manager.lock().await.status(&hash).cloned();
                        let _ = resp.send(status);
                    }
                    ComputeCommand::Cancel { hash, requester, reason, resp } => {
                        let result = self.cancel_task(&hash, &requester, reason).await;
                        let _ = resp.send(result);
                    }
                    ComputeCommand::GossipMessage(msg) => {
                        if let Err(e) = self.handle_message(msg).await {
                            tracing::warn!("compute message error: {}", e);
                        }
                    }
                    // Policy management commands (Phase 16E)
                    ComputeCommand::SetPolicy { policy, resp } => {
                        if let Some(ref pm) = self.policy_manager {
                            pm.set_policy(policy).await;
                            let _ = resp.send(Ok(()));
                        } else {
                            let _ = resp.send(Err(ComputeError::Internal(
                                "policy manager not available".into(),
                            )));
                        }
                    }
                    ComputeCommand::GetPolicy { coop_id, resp } => {
                        if let Some(ref pm) = self.policy_manager {
                            let policy = pm.get_policy(&coop_id).await;
                            let _ = resp.send(policy);
                        } else {
                            let _ = resp.send(None);
                        }
                    }
                    ComputeCommand::ListPolicies { resp } => {
                        if let Some(ref pm) = self.policy_manager {
                            let policies = pm.list_policies().await;
                            let _ = resp.send(policies);
                        } else {
                            let _ = resp.send(vec![]);
                        }
                    }
                    ComputeCommand::RemovePolicy { coop_id, resp } => {
                        if let Some(ref pm) = self.policy_manager {
                            let removed = pm.remove_policy(&coop_id).await;
                            let _ = resp.send(removed);
                        } else {
                            let _ = resp.send(None);
                        }
                    }
                    ComputeCommand::GetUsage { coop_id, member_did, resp } => {
                        if let Some(ref pm) = self.policy_manager {
                            let usage_tracker = pm.usage_tracker();
                            match icn_identity::Did::from_str(&member_did) {
                                Ok(did) => {
                                    let result = usage_tracker.get_usage(&did, &coop_id).await;
                                    let _ = resp.send(result);
                                }
                                Err(_) => {
                                    let _ = resp.send(Err(ComputeError::InvalidInput(
                                        format!("invalid DID: {}", member_did),
                                    )));
                                }
                            }
                        } else {
                            let _ = resp.send(Err(ComputeError::Internal(
                                "policy manager not available".into(),
                            )));
                        }
                    }
                    ComputeCommand::ListCoopUsage { coop_id, resp } => {
                        if let Some(ref pm) = self.policy_manager {
                            let usage_tracker = pm.usage_tracker();
                            let result = usage_tracker.list_coop_usage(&coop_id).await;
                            let _ = resp.send(result);
                        } else {
                            let _ = resp.send(Err(ComputeError::Internal(
                                "policy manager not available".into(),
                            )));
                        }
                    }
                }
            }
        });

        ComputeHandle { tx }
    }

    /// Check for timed-out tasks and mark them as failed
    async fn check_timeouts(
        task_manager: &Arc<Mutex<TaskManager>>,
        executor_registry: &Arc<Mutex<HashMap<String, ExecutorInfo>>>,
        send_callback: &Option<SendCallback>,
    ) -> Result<(), ComputeError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mgr = task_manager.lock().await;
        let timed_out_tasks = mgr.find_timed_out(now);
        drop(mgr);

        let timeout_count = timed_out_tasks.len();
        if timeout_count > 0 {
            tracing::info!(
                count = timeout_count,
                "Found timed-out tasks, marking as failed"
            );
        }

        // Mark timed-out tasks as failed
        for (hash, executor_did) in timed_out_tasks {
            let task_hash_str = hex::encode(hash);
            tracing::warn!(
                task_hash = %task_hash_str,
                "Task exceeded deadline, marking as failed"
            );

            // Get task info for broadcasting
            let mut mgr = task_manager.lock().await;
            let task_id = mgr.get(&hash)
                .map(|t| t.id.clone())
                .unwrap_or_else(|| "unknown".to_string());

            // Mark as failed
            mgr.fail(&hash, "Deadline exceeded".to_string())?;
            drop(mgr);

            icn_obs::metrics::compute::tasks_timeout_inc();
            icn_obs::metrics::compute::tasks_completed_inc("timeout");

            // Decrement executor load if task was claimed
            if let Some(executor) = executor_did {
                let mut registry = executor_registry.lock().await;
                if let Some(info) = registry.get_mut(&executor) {
                    if info.tasks_executing > 0 {
                        info.tasks_executing -= 1;
                        icn_obs::metrics::compute::executor_load_set(&executor, info.tasks_executing as f64);
                    }
                }
            }

            // Broadcast failure via gossip
            if let Some(ref cb) = send_callback {
                let result = ComputeResult {
                    task_hash: hash,
                    task_id,
                    executor: "system".to_string(),
                    outcome: crate::types::ExecutionOutcome::Timeout,
                    fuel_used: 0,
                    duration_ms: 0,
                    completed_at: now,
                    signature: vec![], // System-generated results don't need signatures
                };
                cb(ComputeMessage::TaskResult(result));
            }
        }

        Ok(())
    }

    /// Handle task submission
    async fn handle_submit(&self, task: ComputeTask) -> Result<TaskHash, ComputeError> {
        tracing::debug!(
            task_id = %task.id,
            submitter = %task.submitter,
            fuel_limit = task.fuel_limit.0,
            "Validating task submission"
        );

        // Validate task parameters first
        if let Err(e) = task.validate() {
            tracing::warn!(
                task_id = %task.id,
                submitter = %task.submitter,
                error = %e,
                "Task validation failed"
            );
            return Err(e);
        }

        // Check submitter trust
        let trust = (self.trust_callback)(&task.submitter);
        if trust < MIN_TRUST_SUBMIT {
            tracing::warn!(
                task_id = %task.id,
                submitter = %task.submitter,
                trust_score = trust,
                required = MIN_TRUST_SUBMIT,
                "Task rejected: insufficient trust"
            );
            icn_obs::metrics::compute::tasks_rejected_trust_inc(&task.submitter, trust);
            return Err(ComputeError::InsufficientTrust {
                required: MIN_TRUST_SUBMIT,
                actual: trust,
            });
        }

        // Check policy compliance (Phase 16E)
        let mut adjusted_task = task.clone();
        if let Some(ref policy_manager) = self.policy_manager {
            // Parse submitter DID
            let submitter_did = icn_identity::Did::from_str(&task.submitter)
                .map_err(|e| ComputeError::InvalidInput(format!("Invalid submitter DID: {}", e)))?;

            // Extract coop_id from task (default to "default" if not specified)
            let coop_id = task.coop_id.as_deref().unwrap_or("default");

            // Check policy
            match policy_manager.check_submission(&task, &submitter_did, coop_id).await? {
                crate::policy::PolicyDecision::Reject { reason } => {
                    tracing::warn!(
                        task_id = %task.id,
                        submitter = %task.submitter,
                        coop_id = %coop_id,
                        reason = %reason,
                        "Task rejected by policy"
                    );
                    return Err(ComputeError::PolicyViolation(reason));
                }
                crate::policy::PolicyDecision::Allow {
                    adjusted_priority,
                    placement_constraints,
                } => {
                    // Apply policy adjustments
                    adjusted_task.priority = adjusted_priority;
                    adjusted_task.placement_constraints = Some(placement_constraints.clone());
                    tracing::debug!(
                        task_id = %task.id,
                        original_priority = ?task.priority,
                        adjusted_priority = ?adjusted_priority,
                        has_constraints = !placement_constraints.required_region.is_none(),
                        "Policy check passed with adjustments"
                    );
                }
            }
        }

        // Add to local task manager (use adjusted task)
        let hash = self.task_manager.lock().await.submit(adjusted_task.clone())?;
        icn_obs::metrics::compute::tasks_submitted_inc();

        tracing::info!(
            task_id = %task.id,
            task_hash = %hex::encode(hash),
            submitter = %task.submitter,
            fuel_limit = task.fuel_limit.0,
            "Task submitted successfully"
        );

        // Broadcast to network - use placement negotiation if resource profile provided
        if let Some(ref cb) = self.send_callback {
            if let Some(ref profile) = task.resource_profile {
                // Phase 16B: Use placement negotiation
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                tracing::debug!(
                    task_hash = %hex::encode(hash),
                    "Using placement negotiation (resource profile provided)"
                );

                // Store request timestamp for duration metrics
                self.pending_request_timestamps.lock().await.insert(hash, now);

                cb(ComputeMessage::PlacementRequest {
                    task_hash: hash,
                    submitter: task.submitter.clone(),
                    resource_profile: profile.clone(),
                    locality_hints: vec![], // Future enhancement
                    max_cost: task.payment_rate, // Use payment_rate as max cost
                    requested_at: now,
                });
            } else {
                // Phase 15: Legacy immediate claiming
                tracing::debug!(
                    task_hash = %hex::encode(hash),
                    "Using legacy claiming (no resource profile)"
                );

                cb(ComputeMessage::TaskSubmitted(task));
            }
        }

        Ok(hash)
    }

    /// Cancel a submitted task
    pub async fn cancel_task(
        &self,
        task_hash: &TaskHash,
        requester: &str,
        reason: String,
    ) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(task_hash);

        tracing::info!(
            task_hash = %task_hash_str,
            requester = %requester,
            reason = %reason,
            "Cancelling task"
        );

        // Cancel in local manager (validates authorization)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mut mgr = self.task_manager.lock().await;
        mgr.cancel(task_hash, requester, reason.clone())?;
        drop(mgr);

        icn_obs::metrics::compute::tasks_cancelled_inc();

        tracing::info!(
            task_hash = %task_hash_str,
            "Task cancelled locally"
        );

        // Broadcast cancellation to network
        if let Some(ref cb) = self.send_callback {
            cb(ComputeMessage::TaskCancelled {
                task_hash: *task_hash,
                submitter: requester.to_string(),
                reason,
                cancelled_at: now,
            });
        }

        Ok(())
    }

    /// Handle incoming compute message
    async fn handle_message(&self, msg: ComputeMessage) -> Result<(), ComputeError> {
        match msg {
            ComputeMessage::TaskSubmitted(task) => {
                self.on_task_submitted(task).await
            }
            ComputeMessage::TaskClaimed { task_hash, executor } => {
                self.on_task_claimed(task_hash, executor).await
            }
            ComputeMessage::TaskResult(result) => {
                self.on_task_result(result).await
            }
            ComputeMessage::TaskCancelled { task_hash, submitter, reason, cancelled_at } => {
                self.on_task_cancelled(task_hash, submitter, reason, cancelled_at).await
            }
            ComputeMessage::ExecutorAnnounce { executor, capabilities } => {
                self.on_executor_announce(executor, capabilities).await
            }
            ComputeMessage::PlacementRequest {
                task_hash,
                submitter,
                resource_profile,
                locality_hints,
                max_cost,
                requested_at,
            } => {
                self.on_placement_request(
                    task_hash,
                    submitter,
                    resource_profile,
                    locality_hints,
                    max_cost,
                    requested_at,
                )
                .await
            }
            ComputeMessage::PlacementOffer {
                task_hash,
                executor,
                score,
                cost,
                estimated_start,
                offered_at,
            } => {
                self.on_placement_offer(
                    task_hash,
                    executor,
                    score,
                    cost,
                    estimated_start,
                    offered_at,
                )
                .await
            }
            ComputeMessage::NodeCapacityAnnounce { executor, capacity } => {
                self.on_capacity_announce(executor, capacity).await
            }

            // Phase 16D: Checkpoint & Migration messages
            ComputeMessage::CheckpointAnnounce { checkpoint } => {
                self.on_checkpoint_announce(checkpoint).await
            }
            ComputeMessage::CheckpointQuery { actor_id, requester } => {
                self.on_checkpoint_query(actor_id, requester).await
            }
            ComputeMessage::CheckpointResponse { actor_id, checkpoint } => {
                self.on_checkpoint_response(actor_id, checkpoint).await
            }
            ComputeMessage::MigrationRequest {
                actor_id,
                from_executor,
                to_executor,
                checkpoint,
                reason,
            } => {
                self.on_migration_request(actor_id, from_executor, to_executor, checkpoint, reason).await
            }
            ComputeMessage::MigrationAccept { actor_id, to_executor } => {
                self.on_migration_accept(actor_id, to_executor).await
            }
            ComputeMessage::MigrationReject { actor_id, to_executor, reason } => {
                self.on_migration_reject(actor_id, to_executor, reason).await
            }
            ComputeMessage::MigrationComplete {
                actor_id,
                from_executor,
                to_executor,
                final_checkpoint,
                duration_ms,
            } => {
                self.on_migration_complete(actor_id, from_executor, to_executor, final_checkpoint, duration_ms).await
            }
        }
    }

    /// Handle received task submission
    async fn on_task_submitted(&self, task: ComputeTask) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(task.hash());

        tracing::debug!(
            task_id = %task.id,
            task_hash = %task_hash_str,
            submitter = %task.submitter,
            "Received task submission"
        );

        // Check if we can execute
        let our_trust = (self.trust_callback)(&self.own_did);
        if our_trust < MIN_TRUST_EXECUTE {
            tracing::debug!(
                task_id = %task.id,
                our_trust = our_trust,
                required = MIN_TRUST_EXECUTE,
                "Skipping task: insufficient executor trust"
            );
            return Ok(()); // We're not trusted enough to execute
        }

        // Check if we're at capacity
        if self.at_capacity().await {
            tracing::debug!(
                task_id = %task.id,
                max_concurrent = self.max_concurrent_tasks,
                "Skipping task: executor at capacity"
            );
            icn_obs::metrics::compute::tasks_rejected_capacity_inc();
            return Ok(());
        }

        // Check if we have required capabilities
        if !self.executor.can_execute(&task) {
            tracing::debug!(
                task_id = %task.id,
                "Skipping task: missing required capabilities"
            );
            return Ok(()); // Can't execute this task
        }

        // Store task
        let _hash = self.task_manager.lock().await.submit(task.clone())?;

        // Find highest-priority pending task we can execute
        let mgr = self.task_manager.lock().await;
        let pending = mgr.pending_by_priority();
        let highest_priority_task = pending
            .into_iter()
            .find(|(_, t)| self.executor.can_execute(t))
            .map(|(h, _)| h);
        drop(mgr);

        // Claim highest-priority task if available
        let (hash, claimed_task) = if let Some(h) = highest_priority_task {
            self.task_manager
                .lock()
                .await
                .claim(&h, self.own_did.clone())?;

            let mgr = self.task_manager.lock().await;
            let t = mgr.get(&h)
                .ok_or_else(|| ComputeError::TaskNotFound(hex::encode(h)))?
                .clone();
            drop(mgr);
            (h, t)
        } else {
            tracing::debug!("No suitable pending tasks found after submission");
            return Ok(());
        };

        icn_obs::metrics::compute::tasks_claimed_inc();

        // Track usage for quota enforcement (Phase 16E)
        if let Some(ref policy_manager) = self.policy_manager {
            // Parse submitter DID
            if let Ok(submitter_did) = icn_identity::Did::from_str(&claimed_task.submitter) {
                // Extract coop_id from task (default to "default" if not specified)
                let coop_id = claimed_task.coop_id.as_deref().unwrap_or("default");

                // Increment concurrent task counter
                if let Err(e) = policy_manager
                    .usage_tracker()
                    .task_claimed(coop_id, &submitter_did)
                    .await
                {
                    tracing::warn!(
                        task_id = %claimed_task.id,
                        submitter = %claimed_task.submitter,
                        error = %e,
                        "Failed to track task claim"
                    );
                }
            }
        }

        // Update our own executor load
        let mut registry = self.executor_registry.lock().await;
        if let Some(info) = registry.get_mut(&self.own_did) {
            info.tasks_executing += 1;
            icn_obs::metrics::compute::executor_load_set(&self.own_did, info.tasks_executing as f64);
        }
        drop(registry);

        let task_hash_str = hex::encode(hash);
        tracing::info!(
            task_id = %claimed_task.id,
            task_hash = %task_hash_str,
            priority = ?claimed_task.priority,
            executor = %self.own_did,
            "Claimed and executing highest-priority task"
        );

        // Broadcast claim
        if let Some(ref cb) = self.send_callback {
            cb(ComputeMessage::TaskClaimed {
                task_hash: hash,
                executor: self.own_did.clone(),
            });
        }

        // Broadcast event for external listeners (e.g., WebSocket clients)
        if let Some(ref cb) = self.event_callback {
            cb(ComputeEvent::TaskClaimed {
                task_hash: task_hash_str.clone(),
                executor: self.own_did.clone(),
            });
        }

        // Execute
        let start = std::time::Instant::now();
        let result = self
            .executor
            .execute_task(&claimed_task, &self.own_did, &self.signing_key)?;
        let duration = start.elapsed().as_secs_f64();

        // Record metrics
        icn_obs::metrics::compute::task_duration_record(duration);
        icn_obs::metrics::compute::fuel_used_record(result.fuel_used);
        icn_obs::metrics::compute::fuel_total_add(result.fuel_used);

        // Log outcome
        match &result.outcome {
            crate::types::ExecutionOutcome::Success(output) => {
                tracing::info!(
                    task_id = %claimed_task.id,
                    task_hash = %task_hash_str,
                    fuel_used = result.fuel_used,
                    duration_ms = result.duration_ms,
                    output_size = output.len(),
                    "Task executed successfully"
                );
                icn_obs::metrics::compute::tasks_completed_inc("success");
            }
            crate::types::ExecutionOutcome::Failed(reason) => {
                tracing::warn!(
                    task_id = %claimed_task.id,
                    task_hash = %task_hash_str,
                    fuel_used = result.fuel_used,
                    duration_ms = result.duration_ms,
                    reason = %reason,
                    "Task execution failed"
                );
                icn_obs::metrics::compute::tasks_failed_inc(reason);
            }
            crate::types::ExecutionOutcome::OutOfFuel => {
                tracing::warn!(
                    task_id = %claimed_task.id,
                    task_hash = %task_hash_str,
                    fuel_used = result.fuel_used,
                    duration_ms = result.duration_ms,
                    "Task ran out of fuel"
                );
                icn_obs::metrics::compute::tasks_out_of_fuel_inc();
                icn_obs::metrics::compute::tasks_completed_inc("out_of_fuel");
            }
            crate::types::ExecutionOutcome::Timeout => {
                tracing::warn!(
                    task_id = %claimed_task.id,
                    task_hash = %task_hash_str,
                    fuel_used = result.fuel_used,
                    duration_ms = result.duration_ms,
                    "Task execution timed out"
                );
                icn_obs::metrics::compute::tasks_timeout_inc();
                icn_obs::metrics::compute::tasks_completed_inc("timeout");
            }
        }

        // Record completion
        self.task_manager.lock().await.complete(result.clone())?;

        // Decrement our own executor load
        let mut registry = self.executor_registry.lock().await;
        if let Some(info) = registry.get_mut(&self.own_did) {
            if info.tasks_executing > 0 {
                info.tasks_executing -= 1;
                icn_obs::metrics::compute::executor_load_set(&self.own_did, info.tasks_executing as f64);
            }
        }
        drop(registry);

        // Track usage for quota enforcement (Phase 16E)
        if let Some(ref policy_manager) = self.policy_manager {
            // Parse submitter DID
            if let Ok(submitter_did) = icn_identity::Did::from_str(&claimed_task.submitter) {
                // Extract coop_id from task (default to "default" if not specified)
                let coop_id = claimed_task.coop_id.as_deref().unwrap_or("default");

                // Calculate credits spent (same formula as payment settlement)
                let credits_spent = if let Some(rate) = claimed_task.payment_rate {
                    (result.fuel_used * rate) / 1000
                } else {
                    0
                };

                let usage_tracker = policy_manager.usage_tracker();

                // Record execution (CPU hours and credits)
                if let Err(e) = usage_tracker
                    .record_execution(coop_id, &submitter_did, result.duration_ms, credits_spent)
                    .await
                {
                    tracing::warn!(
                        task_id = %claimed_task.id,
                        submitter = %claimed_task.submitter,
                        error = %e,
                        "Failed to record task execution"
                    );
                }

                // Decrement concurrent task counter
                if let Err(e) = usage_tracker.task_completed(coop_id, &submitter_did).await {
                    tracing::warn!(
                        task_id = %claimed_task.id,
                        submitter = %claimed_task.submitter,
                        error = %e,
                        "Failed to track task completion"
                    );
                }
            }
        }

        // Settle payment if configured and execution succeeded
        if let crate::types::ExecutionOutcome::Success(_) = &result.outcome {
            if let (Some(rate), Some(ref payment_cb)) = (claimed_task.payment_rate, &self.payment_callback) {
                let amount = (result.fuel_used * rate) / 1000; // rate is per 1000 fuel
                if amount > 0 {
                    let currency = claimed_task.payment_currency.clone().unwrap_or_else(|| "credits".to_string());
                    tracing::info!(
                        task_id = %claimed_task.id,
                        from = %claimed_task.submitter,
                        to = %self.own_did,
                        amount = amount,
                        currency = %currency,
                        "Settling payment for completed task"
                    );
                    payment_cb(PaymentRequest {
                        from: claimed_task.submitter.clone(),
                        to: self.own_did.clone(),
                        amount,
                        currency,
                        task_id: claimed_task.id.clone(),
                    });
                    icn_obs::metrics::compute::payments_settled_inc();
                    icn_obs::metrics::compute::payment_amount_add(amount);
                }
            }
        }

        // Broadcast result
        if let Some(ref cb) = self.send_callback {
            cb(ComputeMessage::TaskResult(result.clone()));
        }

        // Broadcast event for external listeners (e.g., WebSocket clients)
        if let Some(ref cb) = self.event_callback {
            let outcome_str = match &result.outcome {
                crate::types::ExecutionOutcome::Success(_) => "success",
                crate::types::ExecutionOutcome::Failed(_) => "failed",
                crate::types::ExecutionOutcome::OutOfFuel => "out_of_fuel",
                crate::types::ExecutionOutcome::Timeout => "timeout",
            };
            cb(ComputeEvent::TaskCompleted {
                task_hash: task_hash_str.clone(),
                executor: self.own_did.clone(),
                outcome: outcome_str.to_string(),
                fuel_used: result.fuel_used,
                duration_ms: result.duration_ms,
            });
        }

        Ok(())
    }

    /// Handle task claimed by another executor
    async fn on_task_claimed(
        &self,
        task_hash: TaskHash,
        executor: String,
    ) -> Result<(), ComputeError> {
        // Just record the claim if we know about the task
        let mut mgr = self.task_manager.lock().await;
        if mgr.get(&task_hash).is_some() {
            let _ = mgr.claim(&task_hash, executor.clone());
        }
        drop(mgr);

        // Update executor load tracking
        let mut registry = self.executor_registry.lock().await;
        if let Some(info) = registry.get_mut(&executor) {
            info.tasks_executing += 1;
            icn_obs::metrics::compute::executor_load_set(&executor, info.tasks_executing as f64);
        }

        Ok(())
    }

    /// Handle executor announcement
    async fn on_executor_announce(
        &self,
        did: String,
        capabilities: Vec<ExecutorCapability>,
    ) -> Result<(), ComputeError> {
        let trust_score = (self.trust_callback)(&did);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let info = ExecutorInfo {
            did: did.clone(),
            capabilities,
            trust_score,
            last_seen: now,
            tasks_executing: 0,
        };

        let mut registry = self.executor_registry.lock().await;
        registry.insert(did.clone(), info);
        icn_obs::metrics::compute::executors_available_set(registry.len() as f64);

        // Update executor load metric
        icn_obs::metrics::compute::executor_load_set(&did, 0.0);

        Ok(())
    }

    /// Get list of available executors with required capabilities
    #[allow(dead_code)]
    pub async fn find_executors(&self, required_caps: &[ExecutorCapability]) -> Vec<String> {
        let registry = self.executor_registry.lock().await;
        registry
            .values()
            .filter(|info| {
                // Check if executor has all required capabilities
                required_caps.iter().all(|cap| info.capabilities.contains(cap))
            })
            .map(|info| info.did.clone())
            .collect()
    }

    /// Handle task result with consensus checking
    async fn on_task_result(&self, result: ComputeResult) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(result.task_hash);

        tracing::debug!(
            task_id = %result.task_id,
            task_hash = %task_hash_str,
            executor = %result.executor,
            "Received task result"
        );

        // Verify signature
        let executor_did: icn_identity::Did = result.executor.parse()
            .map_err(|e| {
                tracing::warn!(
                    task_id = %result.task_id,
                    task_hash = %task_hash_str,
                    executor = %result.executor,
                    error = %e,
                    "Invalid executor DID in result"
                );
                icn_obs::metrics::compute::signatures_invalid_inc("invalid_did");
                ComputeError::InvalidSignature(format!("Invalid executor DID: {e}"))
            })?;

        if let Err(e) = result.verify_signature(&executor_did) {
            tracing::warn!(
                task_id = %result.task_id,
                task_hash = %task_hash_str,
                executor = %result.executor,
                error = %e,
                "Signature verification failed"
            );
            icn_obs::metrics::compute::signatures_invalid_inc("verification_failed");
            return Err(e);
        }

        tracing::debug!(
            task_id = %result.task_id,
            task_hash = %task_hash_str,
            executor = %result.executor,
            "Signature verified successfully"
        );
        icn_obs::metrics::compute::signatures_verified_inc();

        // Consensus checking: For now, we require just 1 result (single-executor mode)
        // In the future, this can be extended to require multiple matching results
        let required_confirmations = 1;

        let mut consensus_map = self.pending_consensus.lock().await;
        let consensus = consensus_map
            .entry(result.task_hash)
            .or_insert_with(|| ResultConsensus {
                task_hash: result.task_hash,
                results: vec![],
                required: required_confirmations,
            });

        // Add this result if not already present (by executor)
        if !consensus.results.iter().any(|r| r.executor == result.executor) {
            consensus.results.push(result.clone());
        }

        // Check if we have enough confirmations
        if consensus.results.len() >= consensus.required {
            // For now, just accept the first result
            // Future: compare results and handle disagreements
            let accepted_result = consensus.results[0].clone();

            // Clean up consensus tracking
            consensus_map.remove(&result.task_hash);
            drop(consensus_map);

            // Complete the task
            let executor_did = accepted_result.executor.clone();
            let task_hash = accepted_result.task_hash;
            let outcome = accepted_result.outcome.clone();
            let fuel_used = accepted_result.fuel_used;
            let duration_ms = accepted_result.duration_ms;

            let mut mgr = self.task_manager.lock().await;
            if mgr.get(&accepted_result.task_hash).is_some() {
                mgr.complete(accepted_result)?;
            }
            drop(mgr);

            // Decrement executor load
            let mut registry = self.executor_registry.lock().await;
            if let Some(info) = registry.get_mut(&executor_did) {
                if info.tasks_executing > 0 {
                    info.tasks_executing -= 1;
                    icn_obs::metrics::compute::executor_load_set(&executor_did, info.tasks_executing as f64);
                }
            }
            drop(registry);

            // Broadcast event for external listeners (e.g., WebSocket clients)
            if let Some(ref cb) = self.event_callback {
                let outcome_str = match &outcome {
                    crate::types::ExecutionOutcome::Success(_) => "success",
                    crate::types::ExecutionOutcome::Failed(_) => "failed",
                    crate::types::ExecutionOutcome::OutOfFuel => "out_of_fuel",
                    crate::types::ExecutionOutcome::Timeout => "timeout",
                };
                cb(ComputeEvent::TaskCompleted {
                    task_hash: hex::encode(task_hash),
                    executor: executor_did.clone(),
                    outcome: outcome_str.to_string(),
                    fuel_used,
                    duration_ms,
                });
            }
        }

        Ok(())
    }

    /// Handle task cancellation
    async fn on_task_cancelled(
        &self,
        task_hash: TaskHash,
        submitter: String,
        reason: String,
        cancelled_at: u64,
    ) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(task_hash);

        tracing::info!(
            task_hash = %task_hash_str,
            submitter = %submitter,
            reason = %reason,
            cancelled_at = cancelled_at,
            "Received task cancellation"
        );

        // Get the executor if task was claimed
        let executor_did = {
            let mgr = self.task_manager.lock().await;
            if let Some(status) = mgr.status(&task_hash) {
                if let TaskStatus::Claimed { executor, .. } = status {
                    Some(executor.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Cancel the task in our local manager
        let mut mgr = self.task_manager.lock().await;
        mgr.cancel(&task_hash, &submitter, reason)?;
        drop(mgr);

        icn_obs::metrics::compute::tasks_cancelled_inc();

        // Decrement executor load if task was claimed
        if let Some(executor) = executor_did {
            let mut registry = self.executor_registry.lock().await;
            if let Some(info) = registry.get_mut(&executor) {
                if info.tasks_executing > 0 {
                    info.tasks_executing -= 1;
                    icn_obs::metrics::compute::executor_load_set(&executor, info.tasks_executing as f64);
                }
            }
        }

        tracing::info!(
            task_hash = %task_hash_str,
            "Task cancelled successfully"
        );

        Ok(())
    }

    /// Handle placement request (Phase 16B)
    async fn on_placement_request(
        &self,
        task_hash: TaskHash,
        submitter: String,
        resource_profile: crate::scheduler::ResourceProfile,
        locality_hints: Vec<crate::scheduler::LocalityHint>,
        _max_cost: Option<u64>,
        _requested_at: u64,
    ) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(task_hash);

        tracing::debug!(
            task_hash = %task_hash_str,
            submitter = %submitter,
            "Received placement request"
        );

        // Track placement request received
        icn_obs::metrics::compute::placement_requests_received_inc();

        // Check if we can execute
        let our_trust = (self.trust_callback)(&self.own_did);
        if our_trust < MIN_TRUST_EXECUTE {
            tracing::debug!(
                task_hash = %task_hash_str,
                our_trust = our_trust,
                required = MIN_TRUST_EXECUTE,
                "Skipping placement: insufficient executor trust"
            );
            return Ok(());
        }

        // Check capacity
        let mut registry = self.executor_registry.lock().await;
        let capacity = if let Some(info) = registry.get_mut(&self.own_did) {
            // Create temporary NodeCapacity from ExecutorInfo
            // For now, we'll use placeholder values - Phase 16A will integrate real capacity tracking
            crate::scheduler::NodeCapacity {
                cpu_cores_total: 8.0,
                cpu_cores_available: 8.0 - info.tasks_executing as f64 * 0.5,
                memory_mb_total: 16384,
                memory_mb_available: 16384 - info.tasks_executing as u64 * 1024,
                storage_mb_available: 100_000,
                network_mbps: 1000.0,
                gpu_devices: vec![],
                updated_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            }
        } else {
            // Not registered yet, use defaults
            drop(registry);
            return Ok(());
        };

        let node_state = crate::scheduler::NodeState {
            did: self.own_did.clone(),
            capacity: capacity.clone(),
            executing_tasks: HashMap::new(),
            queue_depth: registry
                .get(&self.own_did)
                .map(|i| i.tasks_executing)
                .unwrap_or(0),
        };
        drop(registry);

        // Check if we have a placement policy (for now, use default)
        let policy = crate::scheduler::DefaultPlacementPolicy::default();

        // Build locality context (Phase 16C)
        // TODO: Integrate with network layer for RTT and blob registry for data locality
        let locality_ctx = crate::scheduler::LocalityContext::empty();

        // Check placement constraints from policy (Phase 16E)
        // Look up task to get its placement_constraints
        let mgr = self.task_manager.lock().await;
        if let Some(task) = mgr.get(&task_hash) {
            if let Some(ref constraints) = task.placement_constraints {
                // Check required region
                if let Some(ref required_region) = constraints.required_region {
                    // TODO: Get own region from config/network context
                    // For now, we don't have region info, so we'll skip this check
                    tracing::debug!(
                        task_hash = %task_hash_str,
                        required_region = %required_region,
                        "Task has region constraint (region checking not yet implemented)"
                    );
                }

                // Check executor whitelist
                if !constraints.allowed_executors.is_empty()
                    && !constraints.allowed_executors.contains(&self.own_did)
                {
                    tracing::info!(
                        task_hash = %task_hash_str,
                        executor = %self.own_did,
                        "Executor not in whitelist, skipping placement"
                    );
                    icn_obs::metrics::compute::placement_constraints_enforced_inc("whitelist");
                    drop(mgr);
                    return Ok(());
                }

                // Check executor blacklist
                if constraints.forbidden_executors.contains(&self.own_did) {
                    tracing::info!(
                        task_hash = %task_hash_str,
                        executor = %self.own_did,
                        "Executor in blacklist, skipping placement"
                    );
                    icn_obs::metrics::compute::placement_constraints_enforced_inc("blacklist");
                    drop(mgr);
                    return Ok(());
                }

                // Check required capabilities
                if !constraints.required_capabilities.is_empty() {
                    // Get our capabilities from registry
                    let registry = self.executor_registry.lock().await;
                    if let Some(info) = registry.get(&self.own_did) {
                        let our_caps = &info.capabilities;
                        for required in &constraints.required_capabilities {
                            // Convert string to capability and check
                            let has_cap = our_caps.iter().any(|cap| match cap {
                                crate::types::ExecutorCapability::Ccl => required == "Ccl",
                                crate::types::ExecutorCapability::Wasm => required == "Wasm",
                                crate::types::ExecutorCapability::Custom(name) => name == required,
                            });
                            if !has_cap {
                                tracing::info!(
                                    task_hash = %task_hash_str,
                                    executor = %self.own_did,
                                    missing_capability = %required,
                                    "Missing required capability, skipping placement"
                                );
                                icn_obs::metrics::compute::placement_constraints_enforced_inc(
                                    "capability",
                                );
                                drop(registry);
                                drop(mgr);
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        drop(mgr);

        // Score the task
        let offer = match policy.score_task(
            &task_hash,
            &resource_profile,
            &submitter,
            &node_state,
            our_trust,
            &locality_hints,
            &locality_ctx,
        ) {
            Some(o) => o,
            None => {
                tracing::debug!(
                    task_hash = %task_hash_str,
                    "Cannot execute task (capacity or policy rejection)"
                );
                return Ok(());
            }
        };

        tracing::info!(
            task_hash = %task_hash_str,
            score = offer.score,
            cost = offer.cost,
            "Computed placement score, starting deliberation"
        );

        // Track placement score
        icn_obs::metrics::compute::placement_score_observe(offer.score);

        // Deliberation window: wait 500ms before broadcasting offer
        // This allows all executors to compute scores simultaneously,
        // reducing race conditions where fastest network wins
        let send_callback = self.send_callback.clone();
        let task_manager = self.task_manager.clone();

        tokio::spawn(async move {
            // Wait deliberation period
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Check if task was already claimed by someone else
            let mgr = task_manager.lock().await;
            if let Some(status) = mgr.status(&task_hash) {
                if matches!(status, TaskStatus::Claimed { .. }) {
                    tracing::debug!(
                        task_hash = %hex::encode(task_hash),
                        "Task already claimed during deliberation, not broadcasting offer"
                    );
                    return;
                }
            }
            drop(mgr);

            // Broadcast offer after deliberation
            if let Some(cb) = send_callback {
                tracing::debug!(
                    task_hash = %hex::encode(task_hash),
                    score = offer.score,
                    "Broadcasting placement offer after deliberation"
                );

                // Track offer sent
                icn_obs::metrics::compute::placement_offers_sent_inc();

                cb(ComputeMessage::PlacementOffer {
                    task_hash,
                    executor: offer.executor,
                    score: offer.score,
                    cost: offer.cost,
                    estimated_start: offer.estimated_start,
                    offered_at: offer.offered_at,
                });
            }
        });

        Ok(())
    }

    /// Handle placement offer (Phase 16B)
    async fn on_placement_offer(
        &self,
        task_hash: TaskHash,
        executor: String,
        score: f64,
        cost: u64,
        estimated_start: u64,
        offered_at: u64,
    ) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(task_hash);

        // Create PlacementOffer struct
        let offer = crate::scheduler::PlacementOffer {
            executor: executor.clone(),
            score,
            cost,
            estimated_start,
            offered_at,
        };

        // Add to pending offers
        let mut offers_map = self.pending_offers.lock().await;
        let task_offers = offers_map.entry(task_hash).or_insert_with(Vec::new);

        // Check if we already have an offer from this executor (shouldn't happen)
        if task_offers.iter().any(|o| o.executor == executor) {
            tracing::warn!(
                task_hash = %task_hash_str,
                executor = %executor,
                "Duplicate offer from executor, ignoring"
            );
            return Ok(());
        }

        task_offers.push(offer);
        let offer_count = task_offers.len();
        drop(offers_map);

        tracing::debug!(
            task_hash = %task_hash_str,
            executor = %executor,
            score = score,
            offer_count = offer_count,
            "Received placement offer"
        );

        // Track offer received
        icn_obs::metrics::compute::placement_offers_received_inc();

        // If this is the first offer, spawn selection task
        if offer_count == 1 {
            let task_hash_copy = task_hash;
            let pending_offers = self.pending_offers.clone();
            let pending_timestamps = self.pending_request_timestamps.clone();
            let task_manager = self.task_manager.clone();
            let send_callback = self.send_callback.clone();

            tokio::spawn(async move {
                // Wait for offers to arrive (1000ms: 500ms deliberation + 500ms grace)
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                // Get all offers
                let mut offers_map = pending_offers.lock().await;
                let offers = offers_map.remove(&task_hash_copy).unwrap_or_default();
                drop(offers_map);

                // Always cleanup timestamp (prevent memory leak)
                let mut timestamps = pending_timestamps.lock().await;
                let requested_at = timestamps.remove(&task_hash_copy);
                drop(timestamps);

                if offers.is_empty() {
                    tracing::warn!(
                        task_hash = %hex::encode(task_hash_copy),
                        "No offers received for task"
                    );
                    return;
                }

                // Select highest score
                let winner = offers
                    .iter()
                    .max_by(|a, b| {
                        a.score
                            .partial_cmp(&b.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap();

                // Compute placement duration from original request time

                if let Some(requested_at) = requested_at {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    let duration_ms = now.saturating_sub(requested_at);
                    let duration_secs = duration_ms as f64 / 1000.0;

                    tracing::info!(
                        task_hash = %hex::encode(task_hash_copy),
                        winner = %winner.executor,
                        score = winner.score,
                        offer_count = offers.len(),
                        duration_secs = duration_secs,
                        "Selected executor for task"
                    );

                    // Track placement duration (true end-to-end latency)
                    icn_obs::metrics::compute::placement_duration_observe(duration_secs);
                } else {
                    tracing::info!(
                        task_hash = %hex::encode(task_hash_copy),
                        winner = %winner.executor,
                        score = winner.score,
                        offer_count = offers.len(),
                        "Selected executor for task (no duration tracking)"
                    );
                }

                // TODO: Track placement_wins_total and placement_losses_total
                // This requires executors to track which tasks they've offered on,
                // then listen for TaskClaimed messages to determine win/loss.
                // Implementation deferred to future work (Phase 16B+).

                // Claim task with winner
                let mut mgr = task_manager.lock().await;
                if let Err(e) = mgr.claim(&task_hash_copy, winner.executor.clone()) {
                    tracing::warn!(
                        task_hash = %hex::encode(task_hash_copy),
                        error = %e,
                        "Failed to claim task with winner"
                    );
                    return;
                }
                drop(mgr);

                // Broadcast claim
                if let Some(cb) = send_callback {
                    cb(ComputeMessage::TaskClaimed {
                        task_hash: task_hash_copy,
                        executor: winner.executor.clone(),
                    });
                }
            });
        }

        Ok(())
    }

    /// Handle capacity announcement (Phase 16A)
    async fn on_capacity_announce(
        &self,
        executor: String,
        capacity: crate::scheduler::NodeCapacity,
    ) -> Result<(), ComputeError> {
        tracing::debug!(
            executor = %executor,
            cpu_available = capacity.cpu_cores_available,
            memory_available = capacity.memory_mb_available,
            "Received capacity announcement"
        );

        // TODO: Store capacity in executor registry for placement decisions
        // For now, this is a stub

        Ok(())
    }

    // ===== Phase 16D: Checkpoint & Migration Handlers =====

    /// Handle checkpoint announcement (Phase 16D)
    async fn on_checkpoint_announce(
        &self,
        checkpoint: crate::actor_model::ActorCheckpoint,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(checkpoint.actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            sequence = checkpoint.sequence,
            executor = %checkpoint.executor,
            "Received checkpoint announcement"
        );

        // If we have a checkpoint store, verify and store the checkpoint
        if let Some(ref store) = self.checkpoint_store {
            // Verify signature and state hash
            if let Err(e) = store.verify(&checkpoint) {
                tracing::warn!(
                    actor_id = %actor_id_str,
                    executor = %checkpoint.executor,
                    error = %e,
                    "Checkpoint verification failed"
                );
                return Err(e);
            }

            // Store checkpoint (will reject if stale)
            match store.store(checkpoint).await {
                Ok(true) => {
                    tracing::debug!(
                        actor_id = %actor_id_str,
                        "Checkpoint stored successfully"
                    );
                }
                Ok(false) => {
                    tracing::debug!(
                        actor_id = %actor_id_str,
                        "Checkpoint rejected (stale)"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        actor_id = %actor_id_str,
                        error = %e,
                        "Failed to store checkpoint"
                    );
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Handle checkpoint query (Phase 16D)
    async fn on_checkpoint_query(
        &self,
        actor_id: crate::actor_model::ActorId,
        requester: String,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            requester = %requester,
            "Received checkpoint query"
        );

        // If we have a checkpoint store, try to retrieve and respond
        if let Some(ref store) = self.checkpoint_store {
            let checkpoint = store.get(&actor_id).await?;

            // Send response via gossip
            if let Some(ref cb) = self.send_callback {
                cb(ComputeMessage::CheckpointResponse {
                    actor_id,
                    checkpoint,
                });
            }
        }

        Ok(())
    }

    /// Handle checkpoint response (Phase 16D)
    async fn on_checkpoint_response(
        &self,
        actor_id: crate::actor_model::ActorId,
        checkpoint: Option<crate::actor_model::ActorCheckpoint>,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            has_checkpoint = checkpoint.is_some(),
            "Received checkpoint response"
        );

        // If we have a checkpoint and a store, store it
        if let (Some(checkpoint), Some(ref store)) = (checkpoint, &self.checkpoint_store) {
            // Verify and store
            store.verify(&checkpoint)?;
            store.store(checkpoint).await?;
        }

        Ok(())
    }

    /// Handle migration request (Phase 16D)
    async fn on_migration_request(
        &self,
        actor_id: crate::actor_model::ActorId,
        from_executor: String,
        to_executor: String,
        checkpoint: crate::actor_model::ActorCheckpoint,
        reason: crate::actor_model::MigrationReason,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            from = %from_executor,
            to = %to_executor,
            reason = ?reason,
            "Received migration request"
        );

        // Only handle if we're the target executor
        if to_executor != self.own_did {
            return Ok(());
        }

        // Delegate to migration manager if available
        if let Some(ref manager) = self.migration_manager {
            // Get current capacity for policy evaluation
            let capacity = crate::scheduler::NodeCapacity {
                cpu_cores_total: 8.0,
                cpu_cores_available: 8.0,
                memory_mb_total: 16384,
                memory_mb_available: 16384,
                storage_mb_available: 100_000,
                network_mbps: 1000.0,
                gpu_devices: vec![],
                updated_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };

            // Get current queue depth
            let registry = self.executor_registry.lock().await;
            let queue_depth = registry.get(&self.own_did).map(|i| i.tasks_executing).unwrap_or(0);
            drop(registry);

            manager.handle_migration_request(
                actor_id,
                from_executor,
                checkpoint,
                reason,
                &capacity,
                queue_depth,
            ).await?;
        }

        Ok(())
    }

    /// Handle migration accept (Phase 16D)
    async fn on_migration_accept(
        &self,
        actor_id: crate::actor_model::ActorId,
        to_executor: String,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            to = %to_executor,
            "Received migration accept"
        );

        // Delegate to migration manager if available
        if let Some(ref manager) = self.migration_manager {
            // Generate signing key for checkpoint
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);

            manager.handle_migration_accept(actor_id, to_executor, &signing_key).await?;
        }

        Ok(())
    }

    /// Handle migration reject (Phase 16D)
    async fn on_migration_reject(
        &self,
        actor_id: crate::actor_model::ActorId,
        to_executor: String,
        reason: String,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            to = %to_executor,
            reason = %reason,
            "Received migration reject"
        );

        // Delegate to migration manager if available
        if let Some(ref manager) = self.migration_manager {
            manager.handle_migration_reject(actor_id, to_executor, reason).await?;
        }

        Ok(())
    }

    /// Handle migration complete (Phase 16D)
    async fn on_migration_complete(
        &self,
        actor_id: crate::actor_model::ActorId,
        from_executor: String,
        to_executor: String,
        final_checkpoint: crate::actor_model::ActorCheckpoint,
        duration_ms: u64,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            from = %from_executor,
            to = %to_executor,
            duration_ms = duration_ms,
            "Received migration complete"
        );

        // Only handle if we're the target executor
        if to_executor != self.own_did {
            return Ok(());
        }

        // Delegate to migration manager if available
        if let Some(ref manager) = self.migration_manager {
            manager.handle_migration_complete(actor_id, from_executor, final_checkpoint, duration_ms).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExecutorCapability, FuelLimit, TaskCode};

    fn simple_ccl() -> String {
        r#"{
            "name": "SimpleReturn",
            "participants": ["did:icn:alice"],
            "currency": null,
            "state_vars": [],
            "rules": [{
                "name": "run",
                "params": [],
                "requires": [],
                "body": [{ "Return": { "value": { "Literal": { "Int": 42 } } } }]
            }],
            "triggers": []
        }"#.to_string()
    }

    fn make_task(id: &str, submitter: &str) -> ComputeTask {
        ComputeTask {
            id: id.into(),
            submitter: submitter.into(),
            coop_id: None,
            code: TaskCode::Ccl(simple_ccl()),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
        }
    }

    #[tokio::test]
    async fn test_submit_task() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.5);
        let actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
        let handle = actor.spawn();

        let task = make_task("task-1", "did:icn:alice");
        let hash = handle.submit(task).await.unwrap();

        let status = handle.status(hash).await.unwrap();
        assert!(status.is_some());
    }

    #[tokio::test]
    async fn test_submit_low_trust_rejected() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.05); // Below MIN_TRUST_SUBMIT
        let actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
        let handle = actor.spawn();

        let task = make_task("task-1", "did:icn:untrusted");
        let result = handle.submit(task).await;

        assert!(matches!(result, Err(ComputeError::InsufficientTrust { .. })));
    }

    #[tokio::test]
    async fn test_gossip_message_handling() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.5);
        let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
        // Set a valid signing key for result signatures
        actor.set_signing_key(vec![1u8; 32]);
        let handle = actor.spawn();

        let task = make_task("task-1", "did:icn:alice");
        let msg = ComputeMessage::TaskSubmitted(task.clone());

        handle.handle_gossip(msg).await.unwrap();

        // Task should be stored and executed
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let hash = task.hash();
        let status = handle.status(hash).await.unwrap();
        assert!(matches!(status, Some(TaskStatus::Completed { .. })));
    }

    #[tokio::test]
    async fn test_executor_tracking() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.8);
        let mut actor = ComputeActor::new("did:icn:submitter".into(), trust_cb);
        actor.set_signing_key(vec![1u8; 32]);
        let handle = actor.spawn();

        // Announce two executors with different capabilities
        let msg1 = ComputeMessage::ExecutorAnnounce {
            executor: "did:icn:executor1".into(),
            capabilities: vec![ExecutorCapability::Ccl],
        };
        let msg2 = ComputeMessage::ExecutorAnnounce {
            executor: "did:icn:executor2".into(),
            capabilities: vec![ExecutorCapability::Ccl, ExecutorCapability::Wasm],
        };

        handle.handle_gossip(msg1).await.unwrap();
        handle.handle_gossip(msg2).await.unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Both executors should be tracked in the registry
        // (We can't directly access the registry from here, but the announcements succeeded)
        // This test validates the message handling works without errors
    }

    #[tokio::test]
    async fn test_consensus_single_result() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.8);
        let mut actor = ComputeActor::new("did:icn:submitter".into(), trust_cb);
        actor.set_signing_key(vec![1u8; 32]);
        let handle = actor.spawn();

        // Submit a task
        let task = make_task("consensus-task", "did:icn:alice");
        let task_hash = handle.submit(task.clone()).await.unwrap();

        // Generate a valid keypair and result
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().as_str();
        let signing_key = keypair.to_signing_key_bytes();

        let mut result = crate::types::ComputeResult {
            task_hash,
            task_id: task.id.clone(),
            executor: executor_did.to_string(),
            outcome: crate::types::ExecutionOutcome::Success(b"42".to_vec()),
            fuel_used: 100,
            duration_ms: 10,
            completed_at: 1000,
            signature: vec![],
        };

        // Sign the result
        let ed25519_key = ed25519_dalek::SigningKey::from_bytes(&signing_key);
        result.sign(&ed25519_key);

        // Submit result via gossip
        let msg = ComputeMessage::TaskResult(result);
        handle.handle_gossip(msg).await.unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Task should be completed after consensus (1 result required)
        let status = handle.status(task_hash).await.unwrap();
        assert!(matches!(status, Some(TaskStatus::Completed { .. })));
    }

    #[tokio::test]
    async fn test_priority_based_claiming() {
        let trust_cb: TrustCallback = Arc::new(|_| 0.5);
        let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
        actor.set_signing_key(vec![1u8; 32]);

        // Set max concurrent to 1 so we can control execution order
        actor.set_max_concurrent_tasks(1);
        let handle = actor.spawn();

        // Submit tasks with different priorities
        let mut low_task = make_task("low-priority", "did:icn:alice");
        low_task.priority = crate::types::TaskPriority::Low;

        let mut normal_task = make_task("normal-priority", "did:icn:alice");
        normal_task.priority = crate::types::TaskPriority::Normal;

        let mut high_task = make_task("high-priority", "did:icn:alice");
        high_task.priority = crate::types::TaskPriority::High;

        let mut critical_task = make_task("critical-priority", "did:icn:alice");
        critical_task.priority = crate::types::TaskPriority::Critical;

        // Submit them via gossip (not direct submit which auto-executes)
        handle.handle_gossip(ComputeMessage::TaskSubmitted(low_task.clone())).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        handle.handle_gossip(ComputeMessage::TaskSubmitted(normal_task.clone())).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        handle.handle_gossip(ComputeMessage::TaskSubmitted(high_task.clone())).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        handle.handle_gossip(ComputeMessage::TaskSubmitted(critical_task.clone())).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // The critical task should have been claimed and completed first
        // Since we have max_concurrent=1, only highest priority runs at a time
        let critical_hash = critical_task.hash();
        let critical_status = handle.status(critical_hash).await.unwrap();
        assert!(
            matches!(critical_status, Some(TaskStatus::Completed { .. })),
            "Critical task should be completed first, got: {critical_status:?}"
        );

        // High should be next
        let high_hash = high_task.hash();
        let high_status = handle.status(high_hash).await.unwrap();
        assert!(
            matches!(high_status, Some(TaskStatus::Completed { .. })),
            "High priority task should be completed second, got: {high_status:?}"
        );
    }

    /// Integration test for Phase 16B placement negotiation
    /// Tests 5 executors competing for a task with deliberation window
    #[tokio::test]
    async fn test_placement_negotiation_multi_executor() {
        use crate::scheduler::ResourceProfile;
        use crate::types::ExecutorCapability;
        use std::sync::Arc;
        use tokio::sync::Mutex as TokioMutex;

        // Track all offers from all executors
        let offers_collected: Arc<TokioMutex<Vec<(String, f64)>>> = Arc::new(TokioMutex::new(vec![]));
        let claims_collected: Arc<TokioMutex<Vec<String>>> = Arc::new(TokioMutex::new(vec![]));

        // Create a compute-heavy task (CPU+RAM, no GPU to simplify test)
        let task = make_task("data-processing", "did:icn:submitter");
        let task_hash = task.hash();
        let resource_profile = ResourceProfile::compute_heavy(2.0, 4096);

        // Executor configurations: (did, trust)
        let executor_configs = vec![
            ("did:icn:executor-a", 0.9),  // Highest trust
            ("did:icn:executor-b", 0.7),  // Medium trust
            ("did:icn:executor-c", 0.5),  // Low trust (but above MIN_TRUST_EXECUTE = 0.3)
            ("did:icn:executor-d", 0.8),  // High trust
            ("did:icn:executor-e", 0.2),  // Very low trust (below MIN_TRUST_EXECUTE = 0.3, should reject)
        ];

        // Spawn all executors
        let mut executor_handles = vec![];
        for (did, trust) in executor_configs {
            let trust_cb: TrustCallback = Arc::new(move |_| trust);
            let mut actor = ComputeActor::new(did.to_string(), trust_cb);
            actor.set_signing_key(vec![1u8; 32]);

            // Note: Actors use placeholder capacity internally (8 cores, 16GB RAM)
            // which is sufficient for our compute_heavy(2.0, 4096) test task

            let offers_clone = offers_collected.clone();
            let claims_clone = claims_collected.clone();
            let did_owned = did.to_string();

            // Set up send callback to intercept offers and claims
            let send_cb: SendCallback = Arc::new(move |msg| {
                let offers_c = offers_clone.clone();
                let claims_c = claims_clone.clone();
                let did_c = did_owned.clone();

                tokio::spawn(async move {
                    match msg {
                        ComputeMessage::PlacementOffer { score, .. } => {
                            let mut offers = offers_c.lock().await;
                            offers.push((did_c.clone(), score));
                            tracing::info!(
                                executor = %did_c,
                                score = score,
                                "Executor broadcast offer"
                            );
                        }
                        ComputeMessage::TaskClaimed { executor, .. } => {
                            let mut claims = claims_c.lock().await;
                            claims.push(executor.clone());
                            tracing::info!(
                                executor = %executor,
                                "Task claimed"
                            );
                        }
                        _ => {}
                    }
                });
            });
            actor.set_send_callback(send_cb);

            let handle = actor.spawn();
            executor_handles.push((did.to_string(), handle));
        }

        // Register all executors by having them announce themselves
        tracing::info!("Registering all executors via ExecutorAnnounce");
        for (did, handle) in &executor_handles {
            let announce_msg = ComputeMessage::ExecutorAnnounce {
                executor: did.clone(),
                capabilities: vec![ExecutorCapability::Ccl], // CCL capability
            };
            handle.handle_gossip(announce_msg).await.unwrap();
        }

        // Small delay to ensure registrations complete
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Broadcast PlacementRequest to all executors
        let placement_request = ComputeMessage::PlacementRequest {
            task_hash,
            submitter: "did:icn:submitter".to_string(),
            resource_profile: resource_profile.clone(),
            locality_hints: vec![],
            max_cost: Some(1000),
            requested_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        tracing::info!("Broadcasting PlacementRequest to all executors");
        for (did, handle) in &executor_handles {
            tracing::debug!(executor = %did, "Sending PlacementRequest");
            handle.handle_gossip(placement_request.clone()).await.unwrap();
        }

        // Wait for deliberation window (500ms) + grace period (500ms) + processing time
        tracing::info!("Waiting for deliberation and offers...");
        tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

        // Check collected offers
        let offers = offers_collected.lock().await;
        tracing::info!("Collected {} offers", offers.len());
        for (executor, score) in offers.iter() {
            tracing::info!(executor = %executor, score = score, "Offer received");
        }

        // Verify expectations:
        // 1. executor-e should NOT offer (trust < MIN_TRUST_EXECUTE = 0.3)
        // 2. executor-a, executor-b, executor-c, executor-d should offer
        assert!(
            offers.iter().any(|(did, _)| did == "did:icn:executor-a"),
            "Executor A should offer (high trust)"
        );
        assert!(
            offers.iter().any(|(did, _)| did == "did:icn:executor-b"),
            "Executor B should offer (medium trust)"
        );
        assert!(
            offers.iter().any(|(did, _)| did == "did:icn:executor-c"),
            "Executor C should offer (trust above minimum)"
        );
        assert!(
            offers.iter().any(|(did, _)| did == "did:icn:executor-d"),
            "Executor D should offer (high trust, even if busy)"
        );
        assert!(
            !offers.iter().any(|(did, _)| did == "did:icn:executor-e"),
            "Executor E should NOT offer (trust below MIN_TRUST_EXECUTE = 0.3)"
        );

        // Check that we got 4 offers (all except executor-e)
        assert_eq!(
            offers.len(),
            4,
            "Expected 4 offers from executors above trust threshold"
        );

        // Find the highest score
        let highest = offers.iter().max_by(|a, b| {
            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some((winner_did, winner_score)) = highest {
            tracing::info!(
                winner = %winner_did,
                score = winner_score,
                "Highest scoring executor"
            );

            // Winner should be executor-a, executor-b, or executor-d (trust: 0.9, 0.7, 0.8)
            // Phase 16C: Trust now contributes 25% (reduced from 40%)
            // With 10% random jitter, executor-b can win with favorable jitter
            // executor-c (0.5) should still be excluded due to significantly lower trust
            assert!(
                winner_did == "did:icn:executor-a"
                    || winner_did == "did:icn:executor-b"
                    || winner_did == "did:icn:executor-d",
                "Winner should be executor A, B, or D (reasonable trust), got: {}", winner_did
            );
        } else {
            panic!("No offers received!");
        }

        tracing::info!("✅ Phase 16B placement negotiation test passed!");
        tracing::info!("   - 4 executors broadcast offers after deliberation window");
        tracing::info!("   - 1 executor rejected due to low trust");
        tracing::info!("   - High-trust executor won (executor-a, executor-b, or executor-d)");

        // Note: Full end-to-end test with submitter-side selection will be added
        // in a future integration test that simulates the complete gossip protocol
    }

    /// Integration test for Phase 16C locality-aware placement
    /// Demonstrates how RTT and data locality affect executor selection
    #[tokio::test]
    async fn test_locality_aware_placement_scoring() {
        use crate::scheduler::{
            DefaultPlacementPolicy, LocalityContext, LocalityHint, NodeCapacity, NodeState,
            PlacementPolicy, ResourceProfile,
        };

        // Create task requiring compute resources
        let task_hash = [5u8; 32];
        let profile = ResourceProfile::compute_heavy(2.0, 4096);
        let policy = DefaultPlacementPolicy::default();

        // Baseline node state (all executors have same capacity)
        let capacity = NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 8.0,
            memory_mb_total: 16384,
            memory_mb_available: 16384,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 0,
        };

        // Executor A: High trust (0.8), no locality information
        let node_a = NodeState {
            did: "did:icn:executor-a".to_string(),
            capacity: capacity.clone(),
            executing_tasks: std::collections::HashMap::new(),
            queue_depth: 0,
        };
        let locality_a = LocalityContext::empty();

        // Executor B: Medium trust (0.6), excellent RTT (10ms to submitter)
        let node_b = NodeState {
            did: "did:icn:executor-b".to_string(),
            capacity: capacity.clone(),
            executing_tasks: std::collections::HashMap::new(),
            queue_depth: 0,
        };
        let locality_b = LocalityContext {
            submitter_rtt_ms: Some(10), // 10ms RTT
            local_blob_count: 0,
            total_blob_count: 0,
            own_region: None,
            submitter_region: None,
        };

        // Executor C: Medium trust (0.6), excellent data locality (all blobs local)
        let node_c = NodeState {
            did: "did:icn:executor-c".to_string(),
            capacity: capacity.clone(),
            executing_tasks: std::collections::HashMap::new(),
            queue_depth: 0,
        };
        let locality_c = LocalityContext {
            submitter_rtt_ms: None,
            local_blob_count: 5, // All 5 required blobs are local
            total_blob_count: 5,
            own_region: None,
            submitter_region: None,
        };

        // Executor D: Low trust (0.4), both good RTT and data locality
        let node_d = NodeState {
            did: "did:icn:executor-d".to_string(),
            capacity: capacity.clone(),
            executing_tasks: std::collections::HashMap::new(),
            queue_depth: 0,
        };
        let locality_d = LocalityContext {
            submitter_rtt_ms: Some(15), // 15ms RTT
            local_blob_count: 5,        // All blobs local
            total_blob_count: 5,
            own_region: None,
            submitter_region: None,
        };

        let no_hints: Vec<LocalityHint> = vec![];

        // Score all executors
        let offer_a = policy
            .score_task(&task_hash, &profile, "submitter", &node_a, 0.8, &no_hints, &locality_a)
            .unwrap();

        let offer_b = policy
            .score_task(&task_hash, &profile, "submitter", &node_b, 0.6, &no_hints, &locality_b)
            .unwrap();

        let offer_c = policy
            .score_task(&task_hash, &profile, "submitter", &node_c, 0.6, &no_hints, &locality_c)
            .unwrap();

        let offer_d = policy
            .score_task(&task_hash, &profile, "submitter", &node_d, 0.4, &no_hints, &locality_d)
            .unwrap();

        tracing::info!("=== Phase 16C Locality-Aware Placement Scoring ===");
        tracing::info!(
            "Executor A (trust=0.8, no locality): score = {:.4}",
            offer_a.score
        );
        tracing::info!(
            "Executor B (trust=0.6, RTT=10ms): score = {:.4}",
            offer_b.score
        );
        tracing::info!(
            "Executor C (trust=0.6, 5/5 blobs local): score = {:.4}",
            offer_c.score
        );
        tracing::info!(
            "Executor D (trust=0.4, RTT=15ms + 5/5 blobs): score = {:.4}",
            offer_d.score
        );

        // Analysis: With rebalanced weights (Phase 16C Week 3):
        // - Trust: 25% (was 40%)
        // - Capacity: 20%
        // - Queue: 15%
        // - RTT: 15% (NEW)
        // - Data locality: 15% (NEW)
        // - Hints: 10% (NEW)
        // - Jitter: 10%

        // Expected scoring breakdown (approximate, excluding jitter):
        // Executor A: trust(0.20) + capacity(0.20) + queue(0.15) = 0.55
        // Executor B: trust(0.15) + capacity(0.20) + queue(0.15) + rtt(0.148) = 0.648
        // Executor C: trust(0.15) + capacity(0.20) + queue(0.15) + data(0.15) = 0.65
        // Executor D: trust(0.10) + capacity(0.20) + queue(0.15) + rtt(0.145) + data(0.15) = 0.745

        // Verify that locality factors make a significant difference
        // Executor B (good RTT) should beat A (higher trust but no locality)
        assert!(
            offer_b.score + 0.03 > offer_a.score,
            "Good RTT should compensate for lower trust"
        );

        // Executor C (good data locality) should beat A
        assert!(
            offer_c.score + 0.03 > offer_a.score,
            "Good data locality should compensate for lower trust"
        );

        // Executor D (both RTT + data) should score highest despite lowest trust
        // D has 0.4 trust (0.10 contribution) but gets 0.148 + 0.15 = 0.298 from locality
        // A has 0.8 trust (0.20 contribution) but gets 0.0 from locality
        // Net: D gets ~0.10 more points from locality than A gets from higher trust
        assert!(
            offer_d.score + 0.05 > offer_a.score,
            "Combined RTT + data locality should beat high trust alone"
        );

        tracing::info!("✅ Phase 16C locality-aware placement test passed!");
        tracing::info!("   - Locality factors (RTT + data) effectively compensate for lower trust");
        tracing::info!("   - Executor selection properly balances trust, capacity, and locality");
        tracing::info!("   - Scoring demonstrates Phase 16C's 'compute goes to data' principle");
    }

    /// Integration test for Phase 16D actor migration
    /// Demonstrates complete migration flow with checkpoint transfer
    #[tokio::test]
    async fn test_actor_migration_integration() {
        use crate::actor_model::{ActorCheckpoint, ActorId, ActorRuntimeState, MigrationReason};
        use crate::checkpoint_store::{CheckpointStore, InMemoryBackend};
        use crate::migration_manager::{ActorMigrationManager, MigrationSender};
        use crate::migration_policy::DefaultMigrationPolicy;
        use crate::scheduler::NodeCapacity;
        use std::sync::Arc;
        use tokio::sync::Mutex as TokioMutex;

        // Track migration messages
        let messages_collected: Arc<TokioMutex<Vec<ComputeMessage>>> = Arc::new(TokioMutex::new(vec![]));

        // Create two executors
        let executor_a_keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_a_did = executor_a_keypair.did().to_string();

        let executor_b_keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_b_did = executor_b_keypair.did().to_string();

        tracing::info!("Executor A: {}", executor_a_did);
        tracing::info!("Executor B: {}", executor_b_did);

        // Create checkpoint stores for both executors
        let store_a = Arc::new(CheckpointStore::new(Arc::new(InMemoryBackend::new())));
        let store_b = Arc::new(CheckpointStore::new(Arc::new(InMemoryBackend::new())));

        // Create migration managers
        let policy = Arc::new(DefaultMigrationPolicy::default());

        let messages_a = messages_collected.clone();
        let send_a: MigrationSender = Arc::new(move |msg| {
            let m = messages_a.clone();
            tokio::spawn(async move {
                m.lock().await.push(msg);
            });
            Ok(())
        });

        let manager_a = Arc::new(ActorMigrationManager::new(
            policy.clone(),
            store_a.clone(),
            send_a,
            executor_a_did.clone(),
        ));

        let messages_b = messages_collected.clone();
        let send_b: MigrationSender = Arc::new(move |msg| {
            let m = messages_b.clone();
            tokio::spawn(async move {
                m.lock().await.push(msg);
            });
            Ok(())
        });

        let manager_b = Arc::new(ActorMigrationManager::new(
            policy.clone(),
            store_b.clone(),
            send_b,
            executor_b_did.clone(),
        ));

        // Create ComputeActors
        let trust_cb: TrustCallback = Arc::new(|_| 0.8);

        let mut actor_a = ComputeActor::new(executor_a_did.clone(), trust_cb.clone());
        actor_a.set_checkpoint_store(store_a.clone());
        actor_a.set_migration_manager(manager_a.clone());
        let _handle_a = actor_a.spawn();

        let mut actor_b = ComputeActor::new(executor_b_did.clone(), trust_cb.clone());
        actor_b.set_checkpoint_store(store_b.clone());
        actor_b.set_migration_manager(manager_b.clone());
        let handle_b = actor_b.spawn();

        // Create a stateful actor on executor A
        let actor_id: ActorId = [1u8; 32];
        let actor_state = vec![42u8, 43, 44]; // Simple state

        let mut checkpoint = ActorCheckpoint {
            actor_id,
            sequence: 1,
            state: actor_state.clone(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            executor: executor_a_did.clone(),
            state_hash: ActorCheckpoint::compute_state_hash(&actor_state),
            signature: vec![],
        };

        // Sign checkpoint
        let signing_key_a = ed25519_dalek::SigningKey::from_bytes(&executor_a_keypair.to_signing_key_bytes());
        checkpoint.sign(&signing_key_a);

        // Store initial checkpoint on A
        store_a.store(checkpoint.clone()).await.unwrap();

        tracing::info!("Created stateful actor on executor A");

        // Simulate migration evaluation on A (overloaded)
        let network_state = crate::migration_policy::NetworkState {
            executor_load: 0.94, // Very high load on current executor
            available_executors: vec![
                crate::migration_policy::ExecutorInfo {
                    did: executor_a_did.clone(),
                    trust_score: 0.8,
                    load: 0.94, // Very high - overloaded
                    capacity: NodeCapacity {
                        cpu_cores_total: 8.0,
                        cpu_cores_available: 0.5,
                        memory_mb_total: 16384,
                        memory_mb_available: 2048,
                        storage_mb_available: 100_000,
                        network_mbps: 1000.0,
                        gpu_devices: vec![],
                        updated_at: 0,
                    },
                    queue_depth: 15, // High load
                    last_seen: 0,
                },
                crate::migration_policy::ExecutorInfo {
                    did: executor_b_did.clone(),
                    trust_score: 0.8,
                    load: 0.12, // Very low - idle
                    capacity: NodeCapacity {
                        cpu_cores_total: 8.0,
                        cpu_cores_available: 7.5,
                        memory_mb_total: 16384,
                        memory_mb_available: 15000,
                        storage_mb_available: 100_000,
                        network_mbps: 1000.0,
                        gpu_devices: vec![],
                        updated_at: 0,
                    },
                    queue_depth: 1, // Low load
                    last_seen: 0,
                },
            ],
            executor_rtt: std::collections::HashMap::new(),
            blob_locations: std::collections::HashMap::new(),
            snapshot_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        let actor_runtime_state = ActorRuntimeState {
            actor_id,
            uptime_secs: 3600,
            memory_bytes: 1024 * 1024,
            cpu_utilization: 0.5,
            msg_throughput: 10.0,
            last_checkpoint_at: checkpoint.created_at,
            checkpoint_sequence: 1,
            required_blobs: vec![],
        };

        // Evaluate migration
        let decision = manager_a.evaluate_migration(actor_runtime_state, &network_state).await;

        assert!(decision.is_some(), "Migration should be recommended");
        let decision = decision.unwrap();

        tracing::info!(
            "Migration decision: from={} to={} reason={:?} benefit={}",
            decision.from_executor,
            decision.target_executor,
            decision.reason,
            decision.benefit_score
        );

        assert_eq!(decision.target_executor, executor_b_did);

        // Initiate migration from A
        manager_a.initiate_migration(decision, &signing_key_a).await.unwrap();

        // Give time for message to be sent
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check messages
        let messages = messages_collected.lock().await;
        assert!(!messages.is_empty(), "Should have sent migration request");

        // Find the MigrationRequest message
        let migration_request = messages.iter().find_map(|msg| {
            if let ComputeMessage::MigrationRequest {
                actor_id: aid,
                from_executor,
                to_executor,
                checkpoint: ckpt,
                reason,
            } = msg {
                Some((aid, from_executor, to_executor, ckpt, reason))
            } else {
                None
            }
        });

        assert!(migration_request.is_some(), "Should have MigrationRequest message");
        let (aid, from, to, ckpt, reason) = migration_request.unwrap();

        assert_eq!(aid, &actor_id);
        assert_eq!(from, &executor_a_did);
        assert_eq!(to, &executor_b_did);
        assert!(matches!(reason, MigrationReason::LoadBalancing));

        tracing::info!("Migration request sent from A to B");

        // Clone values before dropping messages
        let migration_msg = ComputeMessage::MigrationRequest {
            actor_id: *aid,
            from_executor: from.clone(),
            to_executor: to.clone(),
            checkpoint: ckpt.clone(),
            reason: reason.clone(),
        };

        // Clear messages for next phase
        drop(messages);
        messages_collected.lock().await.clear();

        // Executor B handles migration request
        handle_b.handle_gossip(migration_msg).await.unwrap();

        // Give time for B to evaluate and respond
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check for MigrationAccept message
        let messages = messages_collected.lock().await;
        let migration_accept = messages.iter().find_map(|msg| {
            if let ComputeMessage::MigrationAccept { actor_id: aid, to_executor } = msg {
                Some((aid, to_executor))
            } else {
                None
            }
        });

        assert!(migration_accept.is_some(), "Should have MigrationAccept message");
        let (aid, to) = migration_accept.unwrap();

        assert_eq!(aid, &actor_id);
        assert_eq!(to, &executor_b_did);

        tracing::info!("Migration accepted by B");

        // Verify checkpoint was stored on B
        let checkpoint_b = store_b.get(&actor_id).await.unwrap();
        assert!(checkpoint_b.is_some(), "Checkpoint should be stored on B");
        let checkpoint_b = checkpoint_b.unwrap();
        assert_eq!(checkpoint_b.state, actor_state);
        assert_eq!(checkpoint_b.sequence, 1);

        tracing::info!("✅ Phase 16D actor migration integration test passed!");
        tracing::info!("   - Migration decision correctly identified overloaded executor");
        tracing::info!("   - Migration request sent from A to B with checkpoint");
        tracing::info!("   - Executor B accepted migration and stored checkpoint");
        tracing::info!("   - Actor state preserved across migration");
    }
}
