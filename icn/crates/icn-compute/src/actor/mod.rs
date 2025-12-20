//! Compute actor for distributed task execution.
//!
//! This module contains the ComputeActor for distributed compute coordination.
//! The actor handles task submission, executor coordination, and result consensus.

mod command;
mod consensus;
mod handle;
mod lifecycle;
mod migration;
mod placement;
mod types;

// Re-export public types
pub use handle::ComputeHandle;
pub use types::{
    ComputeEvent, EventCallback, LocalityCallback, PaymentCallback, PaymentRequest, SendCallback,
    TrustCallback,
};

// Internal imports from submodules
use command::ComputeCommand;
use types::{ExecutorInfo, ResultConsensus};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::checkpoint_store::CheckpointStore;
use crate::error::ComputeError;
use crate::executor::LocalExecutor;
use crate::migration_manager::ActorMigrationManager;
use crate::task::TaskManager;
use crate::types::{ComputeMessage, TaskHash};

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
    /// Dispute resolution system for contract execution (Phase 18 Week 4)
    dispute_resolution: Option<Arc<tokio::sync::RwLock<icn_ccl::DisputeResolutionSystem>>>,
    /// Byzantine fault detector for compute verification failures (Phase 18)
    misbehavior_detector: Option<Arc<tokio::sync::RwLock<icn_security::MisbehaviorDetector>>>,
    /// Locality callback for network topology data (Phase 16C M5)
    locality_callback: Option<LocalityCallback>,
    /// Node's own region identifier (e.g., "us-west", "eu-central")
    own_region: Option<String>,
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
            dispute_resolution: None,   // Phase 18 Week 4
            misbehavior_detector: None, // Set via set_misbehavior_detector()
            locality_callback: None,    // Phase 16C M5: Set via set_locality_callback()
            own_region: None,           // Set via set_region() or from config
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

    /// Set dispute resolution system (Phase 18 Week 4)
    pub fn set_dispute_resolution(
        &mut self,
        system: Arc<tokio::sync::RwLock<icn_ccl::DisputeResolutionSystem>>,
    ) {
        self.dispute_resolution = Some(system);
    }

    /// Set misbehavior detector for Byzantine fault detection (Phase 18)
    pub fn set_misbehavior_detector(
        &mut self,
        detector: Arc<tokio::sync::RwLock<icn_security::MisbehaviorDetector>>,
    ) {
        self.misbehavior_detector = Some(detector);
    }

    /// Set locality callback for network topology data (Phase 16C M5)
    ///
    /// This callback provides RTT and data locality information for task placement.
    /// When set, placement offers will use real network topology data instead of defaults.
    pub fn set_locality_callback(&mut self, cb: LocalityCallback) {
        self.locality_callback = Some(cb);
    }

    /// Set the node's region identifier
    ///
    /// Used for region-based task placement constraints.
    /// Region identifiers should follow a consistent naming scheme (e.g., "us-west", "eu-central").
    pub fn set_region(&mut self, region: String) {
        self.own_region = Some(region);
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
                )
                .await
                {
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
                        let result = self.handle_submit(*task).await;
                        let _ = resp.send(result);
                    }
                    ComputeCommand::Status { hash, resp } => {
                        let status = self.task_manager.lock().await.status(&hash).cloned();
                        let _ = resp.send(status);
                    }
                    ComputeCommand::Cancel {
                        hash,
                        requester,
                        reason,
                        resp,
                    } => {
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
                    ComputeCommand::GetUsage {
                        coop_id,
                        member_did,
                        resp,
                    } => {
                        if let Some(ref pm) = self.policy_manager {
                            let usage_tracker = pm.usage_tracker();
                            match icn_identity::Did::from_str(&member_did) {
                                Ok(did) => {
                                    let result = usage_tracker.get_usage(&did, &coop_id).await;
                                    let _ = resp.send(result);
                                }
                                Err(_) => {
                                    let _ = resp.send(Err(ComputeError::InvalidInput(format!(
                                        "invalid DID: {member_did}"
                                    ))));
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
                    // Phase 18 Week 4: Dispute resolution commands
                    ComputeCommand::FileDispute {
                        task_hash,
                        executor,
                        challenger,
                        expected_result,
                        actual_result,
                        resp,
                    } => {
                        if let Some(ref dispute_system) = self.dispute_resolution {
                            let evidence = icn_ccl::DisputeEvidence {
                                task_hash,
                                claimed_result: actual_result.clone(),
                                reason: icn_ccl::DisputeReason::IncorrectResult {
                                    expected: expected_result,
                                    actual: actual_result,
                                },
                                additional_data: vec![],
                                filed_at: std::time::SystemTime::now(),
                            };

                            let executor_did: icn_identity::Did = match executor.parse() {
                                Ok(did) => did,
                                Err(_) => {
                                    let _ = resp.send(Err(ComputeError::InvalidInput(format!(
                                        "invalid executor DID: {executor}"
                                    ))));
                                    continue;
                                }
                            };

                            let challenger_did: icn_identity::Did = match challenger.parse() {
                                Ok(did) => did,
                                Err(_) => {
                                    let _ = resp.send(Err(ComputeError::InvalidInput(format!(
                                        "invalid challenger DID: {challenger}"
                                    ))));
                                    continue;
                                }
                            };

                            let mut system = dispute_system.write().await;
                            match system
                                .file_dispute(task_hash, executor_did, challenger_did, evidence)
                                .await
                            {
                                Ok(dispute_id) => {
                                    icn_obs::metrics::compute::disputes_filed_inc();
                                    tracing::info!(
                                        dispute_id = hex::encode(dispute_id),
                                        task_hash = hex::encode(task_hash),
                                        "Dispute filed successfully"
                                    );
                                    let _ = resp.send(Ok(dispute_id));
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        task_hash = hex::encode(task_hash),
                                        error = %e,
                                        "Failed to file dispute"
                                    );
                                    let _ = resp.send(Err(ComputeError::Internal(e.to_string())));
                                }
                            }
                        } else {
                            let _ = resp.send(Err(ComputeError::Internal(
                                "dispute resolution not available".into(),
                            )));
                        }
                    }
                    ComputeCommand::GetDisputeStatus { dispute_id, resp } => {
                        if let Some(ref dispute_system) = self.dispute_resolution {
                            let system = dispute_system.read().await;
                            let status = system.get_dispute(&dispute_id).map(|d| d.status.clone());
                            let _ = resp.send(status);
                        } else {
                            let _ = resp.send(None);
                        }
                    }
                }
            }
        });

        ComputeHandle { tx }
    }

    /// Handle incoming compute message
    async fn handle_message(&self, msg: ComputeMessage) -> Result<(), ComputeError> {
        match msg {
            ComputeMessage::TaskSubmitted(task) => self.on_task_submitted(*task).await,
            ComputeMessage::TaskClaimed {
                task_hash,
                executor,
            } => self.on_task_claimed(task_hash, executor).await,
            ComputeMessage::TaskResult(result) => self.on_task_result(result).await,
            ComputeMessage::TaskCancelled {
                task_hash,
                submitter,
                reason,
                cancelled_at,
            } => {
                self.on_task_cancelled(task_hash, submitter, reason, cancelled_at)
                    .await
            }
            ComputeMessage::ExecutorAnnounce {
                executor,
                capabilities,
            } => self.on_executor_announce(executor, capabilities).await,
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
            ComputeMessage::CheckpointQuery {
                actor_id,
                requester,
            } => self.on_checkpoint_query(actor_id, requester).await,
            ComputeMessage::CheckpointResponse {
                actor_id,
                checkpoint,
            } => self.on_checkpoint_response(actor_id, checkpoint).await,
            ComputeMessage::MigrationRequest {
                actor_id,
                from_executor,
                to_executor,
                checkpoint,
                reason,
            } => {
                self.on_migration_request(actor_id, from_executor, to_executor, checkpoint, reason)
                    .await
            }
            ComputeMessage::MigrationAccept {
                actor_id,
                to_executor,
            } => self.on_migration_accept(actor_id, to_executor).await,
            ComputeMessage::MigrationReject {
                actor_id,
                to_executor,
                reason,
            } => {
                self.on_migration_reject(actor_id, to_executor, reason)
                    .await
            }
            ComputeMessage::MigrationComplete {
                actor_id,
                from_executor,
                to_executor,
                final_checkpoint,
                duration_ms,
            } => {
                self.on_migration_complete(
                    actor_id,
                    from_executor,
                    to_executor,
                    final_checkpoint,
                    duration_ms,
                )
                .await
            }
        }
    }
}

#[cfg(test)]
mod tests;
