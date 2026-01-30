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
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::checkpoint_store::CheckpointStore;
use crate::error::ComputeError;
use crate::executor::LocalExecutor;
use crate::migration_manager::ActorMigrationManager;
use crate::result_quorum::{ResultQuorumManager, VerificationConfig};
use crate::task::TaskManager;
use crate::types::{ComputeMessage, TaskHash};

/// Actor managing distributed compute tasks
pub struct ComputeActor {
    /// Our DID
    own_did: String,
    /// Task manager
    task_manager: Arc<Mutex<TaskManager>>,
    /// Local executor
    executor: Arc<RwLock<LocalExecutor>>,
    /// Callback to send gossip messages
    send_callback: Option<SendCallback>,
    /// Callback to lookup trust scores
    trust_callback: TrustCallback,
    /// Callback to settle payments
    payment_callback: Option<PaymentCallback>,
    /// Callback to broadcast compute events
    event_callback: Option<EventCallback>,
    /// Signing key for results
    /// Reserved: Result signing for verifiable execution proofs
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
    /// Track required executor count for multi-executor verification (Issue #511)
    pending_executor_requirements: Arc<Mutex<HashMap<TaskHash, usize>>>,
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
    /// Contract registry handle for CclRef resolution
    contract_registry: Option<icn_ccl::ContractRegistryHandle>,
    /// Result quorum manager for multi-executor verification (Issue #511)
    quorum_manager: Arc<ResultQuorumManager>,
    /// Federated executor registry for cross-cooperative task placement (Phase 21)
    federation_registry: Option<Arc<crate::federation::FederatedExecutorRegistry>>,
    /// Our cooperative ID for federation purposes
    own_cooperative_id: Option<String>,
    /// Rate limiter for federated announcements (per cooperative)
    /// Maps cooperative_id -> (last_announce_time, count_in_window)
    federated_announce_rate_limiter: Arc<Mutex<HashMap<String, (u64, u32)>>>,
    /// Cell service for scope-aware placement (Epic 2 #932)
    cell_service: Option<Arc<dyn icn_kernel_api::CellService>>,
    /// Per-scope queue depths for demand tracking (Epic 2 #933)
    scope_queue_depths: Arc<Mutex<HashMap<icn_kernel_api::ScopeLevel, usize>>>,
    /// Maps task_hash → scope level for decrement on completion (Epic 2 #933)
    task_scope_map: Arc<Mutex<HashMap<TaskHash, icn_kernel_api::ScopeLevel>>>,
    /// Live capacity budget adjusted by demand feedback (Epic 2 #933)
    capacity_budget: Arc<Mutex<crate::scheduler::CapacityBudget>>,
    /// Configuration for demand-feedback adjustment loop (Epic 2 #933)
    demand_adjustment_config: crate::scheduler::DemandAdjustmentConfig,
    /// Resource refresh configuration
    resource_refresh_config: crate::scheduler::ResourceRefreshConfig,
    /// Current cached resource profile
    cached_capacity: Arc<Mutex<Option<crate::scheduler::NodeCapacity>>>,
    /// Shutdown signal sender for graceful termination
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl ComputeActor {
    /// Create a new compute actor
    pub fn new(own_did: String, trust_callback: TrustCallback) -> Self {
        Self {
            own_did,
            task_manager: Arc::new(Mutex::new(TaskManager::default())),
            executor: Arc::new(RwLock::new(LocalExecutor::new())),
            send_callback: None,
            trust_callback,
            payment_callback: None,
            event_callback: None,
            signing_key: vec![],
            executor_registry: Arc::new(Mutex::new(HashMap::new())),
            pending_consensus: Arc::new(Mutex::new(HashMap::new())),
            pending_offers: Arc::new(Mutex::new(HashMap::new())),
            pending_request_timestamps: Arc::new(Mutex::new(HashMap::new())),
            pending_executor_requirements: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent_tasks: 10, // Default: 10 concurrent tasks
            checkpoint_store: None,
            migration_manager: None,
            policy_manager: None,
            dispute_resolution: None,   // Phase 18 Week 4
            misbehavior_detector: None, // Set via set_misbehavior_detector()
            locality_callback: None,    // Phase 16C M5: Set via set_locality_callback()
            own_region: None,           // Set via set_region() or from config
            contract_registry: None,    // Set via set_contract_registry()
            quorum_manager: Arc::new(ResultQuorumManager::new(VerificationConfig::default())),
            federation_registry: None, // Phase 21: Set via set_federation_registry()
            own_cooperative_id: None,  // Phase 21: Set via set_cooperative_id()
            federated_announce_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
            cell_service: None,
            scope_queue_depths: Arc::new(Mutex::new(HashMap::new())),
            task_scope_map: Arc::new(Mutex::new(HashMap::new())),
            capacity_budget: Arc::new(Mutex::new(crate::scheduler::CapacityBudget::default())),
            demand_adjustment_config: crate::scheduler::DemandAdjustmentConfig::default(),
            resource_refresh_config: crate::scheduler::ResourceRefreshConfig::default(),
            cached_capacity: Arc::new(Mutex::new(None)),
            shutdown_tx: tokio::sync::broadcast::channel(1).0,
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

    /// Set the contract registry handle for CclRef resolution
    ///
    /// When set, CclRef task codes can be resolved to their contract definitions
    /// from the contract registry before execution.
    pub fn set_contract_registry(&mut self, registry: icn_ccl::ContractRegistryHandle) {
        // Create a resolver callback that synchronously fetches contracts from the registry
        let registry_clone = registry.clone();
        let resolver: crate::executor::ContractResolverCallback =
            Arc::new(move |hash: [u8; 32]| {
                let registry = registry_clone.clone();
                // Use block_in_place to safely call async from sync context
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        match registry.get_contract(&hash).await {
                            Ok(Some(contract)) => Some(contract),
                            Ok(None) => None,
                            Err(e) => {
                                tracing::warn!("Failed to get contract from registry: {}", e);
                                None
                            }
                        }
                    })
                })
            });

        // Set the resolver on the executor
        // Note: This requires exclusive access before spawning
        if let Some(executor) = Arc::get_mut(&mut self.executor) {
            executor.get_mut().set_contract_resolver(resolver);
            // Only record the registry if we successfully configured the resolver
            self.contract_registry = Some(registry);
        } else {
            tracing::warn!("Cannot set contract resolver: executor already shared");
        }
    }

    /// Set maximum concurrent tasks this executor will claim
    pub fn set_max_concurrent_tasks(&mut self, max: usize) {
        self.max_concurrent_tasks = max;
    }

    /// Set verification configuration for result quorum
    ///
    /// This configures thresholds for multi-executor verification:
    /// - Tasks below `low_value_threshold` use single executor
    /// - Tasks above `medium_value_threshold` require 3+ executors
    /// - Critical tasks above `high_value_threshold` require 5 executors
    pub fn set_verification_config(&mut self, config: VerificationConfig) {
        self.quorum_manager = Arc::new(ResultQuorumManager::new(config));
    }

    /// Get a reference to the quorum manager for result verification
    pub fn quorum_manager(&self) -> &Arc<ResultQuorumManager> {
        &self.quorum_manager
    }

    /// Set the federated executor registry for cross-cooperative task placement (Phase 21)
    ///
    /// When set, the compute actor can discover and use executors from federated
    /// cooperatives for task execution.
    pub fn set_federation_registry(
        &mut self,
        registry: Arc<crate::federation::FederatedExecutorRegistry>,
    ) {
        self.federation_registry = Some(registry);
    }

    /// Set this node's cooperative ID for federation purposes (Phase 21)
    ///
    /// This is used to identify which cooperative this node belongs to when
    /// coordinating with federated executors.
    pub fn set_cooperative_id(&mut self, coop_id: String) {
        self.own_cooperative_id = Some(coop_id);
    }

    /// Get this node's cooperative ID
    pub fn cooperative_id(&self) -> Option<&str> {
        self.own_cooperative_id.as_deref()
    }

    /// Set the demand adjustment configuration (Epic 2 #933).
    pub fn set_demand_adjustment_config(
        &mut self,
        config: crate::scheduler::DemandAdjustmentConfig,
    ) {
        self.demand_adjustment_config = config;
    }

    /// Set the cell service for scope-aware placement (Epic 2 #932).
    ///
    /// When set, placement scoring uses real scope relationships from the
    /// cell service instead of defaulting to `ScopeContext::empty()`.
    pub fn set_cell_service(&mut self, service: Arc<dyn icn_kernel_api::CellService>) {
        self.cell_service = Some(service);
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

    /// Set resource refresh configuration
    pub fn set_resource_refresh_config(&mut self, config: crate::scheduler::ResourceRefreshConfig) {
        self.resource_refresh_config = config;
    }

    /// Spawn the actor and return a handle
    pub fn spawn(self) -> ComputeHandle {
        let (tx, mut rx) = mpsc::channel::<ComputeCommand>(256);

        // Clone Arc references for the timeout checker
        let task_manager_clone = Arc::clone(&self.task_manager);
        let executor_registry_clone = Arc::clone(&self.executor_registry);
        let send_callback_clone = self.send_callback.clone();
        let scope_depths_clone = Arc::clone(&self.scope_queue_depths);
        let task_scope_map_clone = Arc::clone(&self.task_scope_map);

        // Spawn timeout checker task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                if let Err(e) = Self::check_timeouts(
                    &task_manager_clone,
                    &executor_registry_clone,
                    &send_callback_clone,
                    &scope_depths_clone,
                    &task_scope_map_clone,
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

        // Spawn quorum aggregator cleanup task (Issue #511)
        {
            let quorum_manager_clone = Arc::clone(&self.quorum_manager);
            tokio::spawn(async move {
                // Run cleanup every 30 seconds (collection window is 30s by default)
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;

                    match quorum_manager_clone.cleanup_expired() {
                        Ok(expired) => {
                            if !expired.is_empty() {
                                tracing::debug!(
                                    expired_count = expired.len(),
                                    "Cleaned up expired quorum aggregators"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "Failed to cleanup expired quorum aggregators"
                            );
                        }
                    }
                }
            });
        }

        // Spawn resource refresh task with graceful shutdown
        {
            let own_did = self.own_did.clone();
            let executor_registry_clone = Arc::clone(&self.executor_registry);
            let cached_capacity_clone = Arc::clone(&self.cached_capacity);
            let send_callback_clone = self.send_callback.clone();
            let event_callback_clone = self.event_callback.clone();
            let refresh_config = self.resource_refresh_config.clone();
            let mut shutdown_rx = self.shutdown_tx.subscribe();

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
                    refresh_config.refresh_interval_secs,
                ));

                // Discard the immediate first tick so the first refresh happens
                // after `refresh_interval_secs`, not immediately on actor start.
                interval.tick().await;

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Sense current resources
                            let new_capacity = crate::scheduler::NodeCapacity::sense_system_resources();

                            // Record refresh metric
                            icn_obs::metrics::compute::resource_refresh_inc();

                            // Check for changes
                            let mut cached = cached_capacity_clone.lock().await;
                            let changes = if let Some(ref old_capacity) = *cached {
                                new_capacity.detect_changes(old_capacity, refresh_config.change_threshold)
                            } else {
                                // First refresh - no previous capacity to compare
                                vec![]
                            };

                            // Update cache
                            *cached = Some(new_capacity.clone());
                            drop(cached);

                            // Record change metrics and emit events
                            if !changes.is_empty() {
                                tracing::info!(
                                    changes = ?changes,
                                    cpu_available = new_capacity.cpu_cores_available,
                                    memory_available = new_capacity.memory_mb_available,
                                    gpu_count = new_capacity.gpu_devices.len(),
                                    "Resource profile changed significantly"
                                );

                                for change_type in &changes {
                                    icn_obs::metrics::compute::resource_changes_inc(change_type.as_str());
                                }

                                // Emit event if callback is configured
                                if let Some(ref event_cb) = event_callback_clone {
                                    let event = crate::actor::ComputeEvent::ResourcesChanged {
                                        executor: own_did.clone(),
                                        capacity: new_capacity.clone(),
                                        changes: changes.clone(),
                                    };
                                    event_cb(event);
                                }

                                // Announce capacity via gossip only when changes detected
                                if let Some(ref send_cb) = send_callback_clone {
                                    let msg = crate::types::ComputeMessage::NodeCapacityAnnounce {
                                        executor: own_did.clone(),
                                        capacity: new_capacity.clone(),
                                    };
                                    send_cb(msg);
                                }
                            }

                            // Update executor registry with new capacity
                            let mut registry = executor_registry_clone.lock().await;
                            if let Some(info) = registry.get_mut(&own_did) {
                                info.capacity = Some(new_capacity);
                                tracing::debug!(
                                    "Updated own executor capacity in registry"
                                );
                            }
                            drop(registry);
                        }
                        _ = shutdown_rx.recv() => {
                            tracing::info!("Resource refresh task received shutdown signal");
                            break;
                        }
                    }
                }
            });
        }

        // Spawn demand-feedback capacity adjustment loop (Epic 2 #933)
        {
            let scope_depths = Arc::clone(&self.scope_queue_depths);
            let capacity_budget = Arc::clone(&self.capacity_budget);
            let config = self.demand_adjustment_config.clone();
            let mut shutdown_rx = self.shutdown_tx.subscribe();

            tokio::spawn(async move {
                tracing::info!(
                    interval_secs = config.interval_secs,
                    learning_rate = config.learning_rate,
                    min_samples = config.min_samples,
                    "Demand adjustment loop started"
                );
                let mut interval =
                    tokio::time::interval(tokio::time::Duration::from_secs(config.interval_secs));
                // Skip the immediate first tick
                interval.tick().await;

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            let depths = scope_depths.lock().await;
                            let total: usize = depths.values().sum();

                            // Only adjust if we have enough data points
                            if total >= config.min_samples {
                                // Build utilization map: fraction of total queue per scope
                                let mut utilization = HashMap::new();
                                for (&scope, &count) in depths.iter() {
                                    utilization.insert(scope, count as f64 / total as f64);
                                }
                                drop(depths);

                                let mut budget = capacity_budget.lock().await;
                                budget.adjust_from_demand(&utilization, config.learning_rate);
                                tracing::debug!(
                                    total_queued = total,
                                    budget = ?*budget,
                                    "Adjusted capacity budget from demand"
                                );
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            tracing::info!("Demand adjustment task received shutdown signal");
                            break;
                        }
                    }
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
                        if let Err(e) = self.handle_message(*msg).await {
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
                max_scope,
                cell_affinity,
                allowed_scopes,
            } => {
                self.on_placement_request(
                    task_hash,
                    submitter,
                    resource_profile,
                    locality_hints,
                    max_cost,
                    requested_at,
                    max_scope,
                    cell_affinity,
                    allowed_scopes,
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

            // Phase 21: Federation messages
            ComputeMessage::FederatedExecutorAnnounce {
                executor,
                cooperative_id,
                capabilities,
                attestation,
            } => {
                self.on_federated_executor_announce(
                    executor,
                    cooperative_id,
                    capabilities,
                    attestation,
                )
                .await
            }
            ComputeMessage::FederatedTaskRequest {
                task_hash,
                task,
                from_coop,
                to_coop,
                payment,
                requested_at,
            } => {
                self.on_federated_task_request(
                    task_hash,
                    *task,
                    from_coop,
                    to_coop,
                    payment,
                    requested_at,
                )
                .await
            }
            ComputeMessage::FederatedTaskResult {
                result,
                executor_coop,
                attestation_hash,
            } => {
                self.on_federated_task_result(result, executor_coop, attestation_hash)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests;
