//! Placement and executor management handlers for ComputeActor.
//!
//! Phase 16B/16C: Distributed task placement with deliberation protocol.

use super::types::ExecutorInfo;
use super::ComputeActor;
use crate::error::ComputeError;
use crate::scheduler::PlacementPolicy;
use crate::task::TaskStatus;
use crate::types::{ComputeMessage, ExecutorCapability, TaskHash};
use crate::MIN_TRUST_EXECUTE;
use std::collections::HashMap;

impl ComputeActor {
    /// Handle executor announcement
    pub(super) async fn on_executor_announce(
        &self,
        did: String,
        capabilities: Vec<ExecutorCapability>,
    ) -> Result<(), ComputeError> {
        let trust_score = (self.trust_callback)(&did);
        let now = icn_time::current_timestamp_millis();

        let info = ExecutorInfo {
            did: did.clone(),
            cooperative_id: None, // Local executor
            is_federated: false,  // Not from federation
            capabilities,
            trust_score,
            federated_trust_score: None, // N/A for local executors
            last_seen: now,
            tasks_executing: 0,
            capacity: None,
            gateway_endpoint: None, // N/A for local executors
        };

        let mut registry = self.executor_registry.lock().await;
        registry.insert(did.clone(), info);
        icn_obs::metrics::compute::executors_available_set(registry.len() as f64);
        // Note: Per-executor load tracking removed (high-cardinality)

        Ok(())
    }

    /// Get list of available executors with required capabilities
    ///
    /// Reserved: Capability-based executor filtering for task placement.
    /// Currently placement uses scheduler directly; this is for future API use.
    #[allow(dead_code)]
    pub async fn find_executors(&self, required_caps: &[ExecutorCapability]) -> Vec<String> {
        let registry = self.executor_registry.lock().await;
        registry
            .values()
            .filter(|info| {
                // Check if executor has all required capabilities
                required_caps
                    .iter()
                    .all(|cap| info.capabilities.contains(cap))
            })
            .map(|info| info.did.clone())
            .collect()
    }

    /// Get capacity information for an executor
    ///
    /// Returns None if the executor is not registered or has no capacity info.
    /// Reserved: Capacity monitoring API for gateway/admin interfaces.
    #[allow(dead_code)]
    pub async fn get_executor_capacity(
        &self,
        executor_did: &str,
    ) -> Option<crate::scheduler::NodeCapacity> {
        let registry = self.executor_registry.lock().await;
        registry
            .get(executor_did)
            .and_then(|info| info.capacity.clone())
    }

    /// Get capacity information for all registered executors
    ///
    /// Returns a map of executor DID to capacity. Executors without capacity info are excluded.
    /// Reserved: Cluster-wide capacity dashboard for operators.
    #[allow(dead_code)]
    pub async fn get_all_executor_capacities(
        &self,
    ) -> HashMap<String, crate::scheduler::NodeCapacity> {
        let registry = self.executor_registry.lock().await;
        registry
            .iter()
            .filter_map(|(did, info)| info.capacity.clone().map(|cap| (did.clone(), cap)))
            .collect()
    }

    /// Handle placement request (Phase 16B)
    pub(super) async fn on_placement_request(
        &self,
        task_hash: TaskHash,
        submitter: String,
        resource_profile: crate::scheduler::ResourceProfile,
        locality_hints: Vec<crate::scheduler::LocalityHint>,
        _max_cost: Option<u64>,
        requested_at: u64,
        max_scope: Option<icn_kernel_api::ScopeLevel>,
        cell_affinity: Option<icn_kernel_api::CellId>,
        allowed_scopes: Vec<icn_kernel_api::ScopeLevel>,
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
                updated_at: icn_time::current_timestamp_millis(),
            }
        } else {
            // Not registered yet, use defaults
            drop(registry);
            return Ok(());
        };

        let scope_depths = self.scope_queue_depths.lock().await.clone();
        let node_state = crate::scheduler::NodeState {
            did: self.own_did.clone(),
            capacity: capacity.clone(),
            executing_tasks: HashMap::new(),
            queue_depth: registry
                .get(&self.own_did)
                .map(|i| i.tasks_executing)
                .unwrap_or(0),
            scope_queue_depths: scope_depths,
        };
        drop(registry);

        // Check if we have a placement policy (for now, use default)
        let policy = crate::scheduler::DefaultPlacementPolicy::default();

        // Build locality context (Phase 16C / M5)
        // Use locality callback if available, otherwise use empty context
        let locality_ctx = if let Some(ref locality_cb) = self.locality_callback {
            (locality_cb)(&submitter)
        } else {
            crate::scheduler::LocalityContext::empty()
        };

        // Check placement constraints from policy (Phase 16E)
        // Extract constraints from task manager first, then drop the lock to avoid
        // nested lock acquisition with executor_registry (potential deadlock fix)
        let (placement_constraints, federation_constraints, task_coop_id) = {
            let mgr = self.task_manager.lock().await;
            if let Some(task) = mgr.get(&task_hash) {
                (
                    task.placement_constraints.clone(),
                    task.federation_constraints.clone(),
                    task.coop_id.clone(),
                )
            } else {
                (None, None, None)
            }
            // mgr lock is dropped here
        };

        // Check placement constraints (no longer holding task_manager lock)
        if let Some(ref constraints) = placement_constraints {
            // Check required region
            if let Some(ref required_region) = constraints.required_region {
                if let Some(ref own_region) = self.own_region {
                    if own_region != required_region {
                        tracing::debug!(
                            task_hash = %task_hash_str,
                            required_region = %required_region,
                            own_region = %own_region,
                            "Task requires different region, skipping claim"
                        );
                        return Ok(());
                    }
                } else {
                    // No region configured, cannot claim region-specific tasks
                    tracing::debug!(
                        task_hash = %task_hash_str,
                        required_region = %required_region,
                        "Task requires region but node has no region configured, skipping claim"
                    );
                    return Ok(());
                }
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
                return Ok(());
            }

            // Check required capabilities
            if !constraints.required_capabilities.is_empty() {
                // Get our capabilities from registry (safe - not holding task_manager)
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
                            return Ok(());
                        }
                    }
                }
                // registry lock dropped here
            }
        }

        // Phase 21: Check federation placement constraints
        if let Some(ref fed_constraints) = federation_constraints {
            // Determine if we're a federated executor (from another cooperative)
            let is_local = self.own_cooperative_id.is_none()
                || task_coop_id.as_ref() == self.own_cooperative_id.as_ref();

            // Check federation policy
            if !fed_constraints.allows_cooperative(self.own_cooperative_id.as_deref(), is_local) {
                tracing::info!(
                    task_hash = %task_hash_str,
                    executor = %self.own_did,
                    our_coop = ?self.own_cooperative_id,
                    task_coop = ?task_coop_id,
                    policy = ?fed_constraints.federation_policy,
                    "Federation policy rejects this executor, skipping placement"
                );
                icn_obs::metrics::compute::placement_constraints_enforced_inc("federation_policy");
                return Ok(());
            }

            // Check federated trust threshold if we're a federated executor
            if !is_local {
                let min_trust = fed_constraints.effective_min_trust();
                // Look up our federated trust score from registry (safe - not holding task_manager)
                let registry = self.executor_registry.lock().await;
                if let Some(info) = registry.get(&self.own_did) {
                    if let Some(fed_trust) = info.federated_trust_score {
                        if fed_trust < min_trust {
                            tracing::info!(
                                task_hash = %task_hash_str,
                                executor = %self.own_did,
                                federated_trust = fed_trust,
                                min_required = min_trust,
                                "Federated trust below threshold, skipping placement"
                            );
                            icn_obs::metrics::compute::placement_constraints_enforced_inc(
                                "federation_trust",
                            );
                            return Ok(());
                        }
                    }
                }
                // registry lock dropped here
            }
        }

        // Epic 2 (#932/#933): Populate scope context from CellService and live budget.
        let budget = self.capacity_budget.read().await.clone();
        let scope_ctx = match &self.cell_service {
            Some(cs) => {
                let peer_scope = cs.peer_scope(&submitter);
                let executor_cell = cs.local_cell();
                crate::scheduler::ScopeContext {
                    peer_scope,
                    executor_cell,
                    capacity_budget: budget,
                }
            }
            None => crate::scheduler::ScopeContext {
                capacity_budget: budget,
                ..crate::scheduler::ScopeContext::empty()
            },
        };

        // Build a PlacementRequest for the scoring call
        let placement_request = crate::scheduler::PlacementRequest {
            task_hash,
            resource_profile: resource_profile.clone(),
            locality_hints: locality_hints.clone(),
            max_cost: _max_cost,
            requested_at,
            max_scope,
            cell_affinity,
            allowed_scopes: allowed_scopes.clone(),
        };

        // Score the task
        let offer = match policy.score_task(
            &placement_request,
            &submitter,
            &node_state,
            our_trust,
            &locality_ctx,
            &scope_ctx,
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

        // Deliberation window: wait until deadline before broadcasting offer
        // Uses relative timing based on request timestamp to avoid clock skew (M9 fix)
        // This allows all executors to broadcast at approximately the same wall-clock time,
        // regardless of network latency in receiving the request
        let send_callback = self.send_callback.clone();
        let task_manager = self.task_manager.clone();

        // Calculate deadline based on request timestamp
        let deadline = requested_at + crate::DELIBERATION_PERIOD_MS;
        let now = icn_time::current_timestamp_millis();
        let remaining_ms = deadline.saturating_sub(now);

        tokio::spawn(async move {
            // Wait remaining deliberation period (relative to request timestamp)
            if remaining_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(remaining_ms)).await;
            }

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
    pub(super) async fn on_placement_offer(
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
            let pending_requirements = self.pending_executor_requirements.clone();
            let task_manager = self.task_manager.clone();
            let send_callback = self.send_callback.clone();

            tokio::spawn(async move {
                // Wait for offers to arrive (1000ms: 500ms deliberation + 500ms grace)
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                // Get all offers
                let mut offers_map = pending_offers.lock().await;
                let mut offers = offers_map.remove(&task_hash_copy).unwrap_or_default();
                drop(offers_map);

                // Always cleanup timestamp and requirements (prevent memory leak)
                let mut timestamps = pending_timestamps.lock().await;
                let requested_at = timestamps.remove(&task_hash_copy);
                drop(timestamps);

                let mut requirements = pending_requirements.lock().await;
                let required_executors = requirements.remove(&task_hash_copy).unwrap_or(1);
                drop(requirements);

                if offers.is_empty() {
                    tracing::warn!(
                        task_hash = %hex::encode(task_hash_copy),
                        "No offers received for task"
                    );
                    return;
                }

                // Sort offers by score (descending) with deterministic tie-breaking
                offers.sort_by(|a, b| {
                    const EPSILON: f64 = 1e-9;
                    let score_diff = b.score - a.score; // Descending order
                    if score_diff.abs() < EPSILON {
                        // Scores are effectively equal, use DID as tie-breaker
                        a.executor.cmp(&b.executor)
                    } else if score_diff > 0.0 {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Less
                    }
                });

                // Select top N executors for multi-executor verification (Issue #511)
                let selected_count = required_executors.min(offers.len());
                let selected_executors: Vec<_> = offers.iter().take(selected_count).collect();

                // Defensive check: ensure we have at least one executor selected
                if selected_executors.is_empty() {
                    tracing::warn!(
                        task_hash = %hex::encode(task_hash_copy),
                        required_executors = required_executors,
                        offer_count = offers.len(),
                        "No executors selected for task (unexpected state)"
                    );
                    return;
                }

                // Compute placement duration from original request time
                if let Some(requested_at) = requested_at {
                    let now = icn_time::current_timestamp_millis();
                    let duration_ms = now.saturating_sub(requested_at);
                    let duration_secs = duration_ms as f64 / 1000.0;

                    if selected_count > 1 {
                        tracing::info!(
                            task_hash = %hex::encode(task_hash_copy),
                            selected_count = selected_count,
                            required_executors = required_executors,
                            offer_count = offers.len(),
                            duration_secs = duration_secs,
                            executors = ?selected_executors.iter().map(|e| &e.executor).collect::<Vec<_>>(),
                            "Selected multiple executors for multi-executor verification"
                        );
                    } else {
                        tracing::info!(
                            task_hash = %hex::encode(task_hash_copy),
                            winner = %selected_executors[0].executor,
                            score = selected_executors[0].score,
                            offer_count = offers.len(),
                            duration_secs = duration_secs,
                            "Selected executor for task"
                        );
                    }

                    // Track placement duration (true end-to-end latency)
                    icn_obs::metrics::compute::placement_duration_observe(duration_secs);
                } else if selected_count > 1 {
                    tracing::info!(
                        task_hash = %hex::encode(task_hash_copy),
                        selected_count = selected_count,
                        required_executors = required_executors,
                        offer_count = offers.len(),
                        "Selected multiple executors for multi-executor verification (no duration tracking)"
                    );
                } else {
                    tracing::info!(
                        task_hash = %hex::encode(task_hash_copy),
                        winner = %selected_executors[0].executor,
                        score = selected_executors[0].score,
                        offer_count = offers.len(),
                        "Selected executor for task (no duration tracking)"
                    );
                }

                // Track placement wins and losses
                // Selected executors get wins, all others are losses
                for _ in 0..selected_count {
                    icn_obs::metrics::compute::placement_wins_inc();
                }
                if offers.len() > selected_count {
                    for _ in 0..(offers.len() - selected_count) {
                        icn_obs::metrics::compute::placement_losses_inc();
                    }
                }

                // For single executor, claim the task as before
                // For multi-executor, we don't claim - all selected executors will execute
                // and their results will be collected by the quorum manager
                if selected_count == 1 {
                    // Single executor: claim the task
                    let mut mgr = task_manager.lock().await;
                    if let Err(e) =
                        mgr.claim(&task_hash_copy, selected_executors[0].executor.clone())
                    {
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
                            executor: selected_executors[0].executor.clone(),
                        });
                    }
                } else {
                    // Multi-executor verification mode (Issue #511):
                    // - All selected executors receive TaskClaimed and execute independently
                    // - Results are collected by ResultQuorumManager for consensus verification
                    // - Only the first executor is recorded in TaskManager as the "primary"
                    //   for tracking purposes; all executors' results are equally valid
                    // - TaskManager.claim() doesn't affect execution - it's just bookkeeping
                    if let Some(cb) = send_callback {
                        for selected in &selected_executors {
                            cb(ComputeMessage::TaskClaimed {
                                task_hash: task_hash_copy,
                                executor: selected.executor.clone(),
                            });
                        }
                    }

                    // Record first executor as "primary" in TaskManager for tracking.
                    // Note: This is purely for bookkeeping - all executors execute and
                    // their results are verified by the quorum manager regardless.
                    if let Some(primary) = selected_executors.first() {
                        let mut mgr = task_manager.lock().await;
                        if let Err(e) = mgr.claim(&task_hash_copy, primary.executor.clone()) {
                            tracing::warn!(
                                task_hash = %hex::encode(task_hash_copy),
                                error = %e,
                                "Failed to claim task with primary executor"
                            );
                        }
                        drop(mgr);
                    }
                }
            });
        }

        Ok(())
    }

    /// Handle capacity announcement (Phase 16A)
    pub(super) async fn on_capacity_announce(
        &self,
        executor: String,
        capacity: crate::scheduler::NodeCapacity,
        cell_id: Option<icn_kernel_api::CellId>,
        capacity_budget: Option<crate::scheduler::CapacityBudget>,
    ) -> Result<(), ComputeError> {
        tracing::debug!(
            executor = %executor,
            cpu_available = capacity.cpu_cores_available,
            memory_available = capacity.memory_mb_available,
            has_cell = cell_id.is_some(),
            has_budget = capacity_budget.is_some(),
            "Received capacity announcement"
        );

        // Store capacity in executor registry for placement decisions
        let mut registry = self.executor_registry.lock().await;
        if let Some(info) = registry.get_mut(&executor) {
            info.capacity = Some(capacity.clone());
            info.last_seen = capacity.updated_at;
            tracing::debug!(
                executor = %executor,
                cpu_cores = capacity.cpu_cores_available,
                memory_mb = capacity.memory_mb_available,
                storage_mb = capacity.storage_mb_available,
                gpus = capacity.gpu_devices.len(),
                "Updated executor capacity in registry"
            );
        } else {
            // Executor not yet registered - create entry with capacity
            let trust_score = (self.trust_callback)(&executor);
            let info = ExecutorInfo {
                did: executor.clone(),
                cooperative_id: None,     // Local executor
                is_federated: false,      // Not from federation
                capabilities: Vec::new(), // Will be populated on ExecutorAvailable message
                trust_score,
                federated_trust_score: None, // N/A for local executors
                last_seen: capacity.updated_at,
                tasks_executing: 0,
                capacity: Some(capacity.clone()),
                gateway_endpoint: None, // N/A for local executors
            };
            registry.insert(executor.clone(), info);
            tracing::debug!(
                executor = %executor,
                "Created executor entry from capacity announcement"
            );
        }

        // Update metrics for executor capacity
        icn_obs::metrics::compute::executors_available_set(registry.len() as f64);
        drop(registry);

        // --- Commons pool decision matrix (Epic 6 #947) ---
        //
        // | cell_id | capacity_budget | Behavior                                     |
        // |---------|-----------------|----------------------------------------------|
        // | None    | None            | Unaffiliated. Full commons: commons_share=1.0 |
        // | None    | Some(b)         | Unaffiliated with explicit budget. Use b.     |
        // | Some(_) | None            | Affiliated, no explicit budget. Use default.  |
        // | Some(_) | Some(b)         | Affiliated with explicit budget. Use b.       |
        //
        // Invariant: affiliated nodes without explicit budget default to
        // `CapacityBudget::default()` (0.10 commons), NOT full commons.
        let effective_budget = match (&cell_id, capacity_budget) {
            (None, None) => {
                // Unaffiliated node — full commons participation.
                crate::scheduler::CapacityBudget {
                    local_reserve: 0.0,
                    cell_share: 0.0,
                    org_share: 0.0,
                    federation_share: 0.0,
                    commons_share: 1.0,
                }
            }
            (None, Some(b)) => b,
            (Some(_), None) => {
                // Affiliated with no explicit budget — use default (0.10 commons).
                crate::scheduler::CapacityBudget::default()
            }
            (Some(_), Some(b)) => b,
        };

        // Lock discipline: acquire write lock, mutate, release immediately.
        let commons_share = effective_budget.commons_share;
        if commons_share > 0.0 {
            let participant = crate::commons_pool::CommonsParticipant {
                did: executor.clone(),
                capacity,
                budget: effective_budget,
                last_announce: std::time::Instant::now(),
            };
            let mut pool = self.commons_pool.write().await;
            pool.add_participant(participant);
            let count = pool.participant_count();
            drop(pool);
            tracing::debug!(
                executor = %executor,
                commons_share,
                pool_size = count,
                "Added/updated node in commons pool"
            );
        } else {
            let mut pool = self.commons_pool.write().await;
            pool.remove_participant(&executor);
            drop(pool);
            tracing::debug!(
                executor = %executor,
                "Removed node from commons pool (zero commons share)"
            );
        }

        Ok(())
    }

    /// Handle federated executor announcement (Phase 21)
    ///
    /// Called when a federated cooperative announces available executors.
    /// These executors are registered with attenuated trust scores.
    ///
    /// # Security
    /// This method verifies the attestation signature to prevent forged announcements.
    pub(super) async fn on_federated_executor_announce(
        &self,
        executor: String,
        cooperative_id: String,
        capabilities: Vec<ExecutorCapability>,
        attestation: crate::federation::FederatedExecutorAttestation,
    ) -> Result<(), ComputeError> {
        let local_trust = attestation.trust_attestation.trust_score;

        tracing::info!(
            executor = %executor,
            cooperative_id = %cooperative_id,
            capabilities = ?capabilities,
            attested_trust = local_trust,
            "Received federated executor announcement"
        );

        // Security: Verify attestation is signed
        if !attestation.is_signed() {
            tracing::warn!(
                executor = %executor,
                cooperative_id = %cooperative_id,
                "Rejecting unsigned federated executor attestation"
            );
            return Err(ComputeError::InvalidSignature(
                "Attestation must be signed by source cooperative".to_string(),
            ));
        }

        // Security: Verify attestation is not expired
        if attestation.is_expired() {
            tracing::warn!(
                executor = %executor,
                cooperative_id = %cooperative_id,
                "Rejecting expired federated executor attestation"
            );
            return Err(ComputeError::InvalidSignature(
                "Attestation has expired".to_string(),
            ));
        }

        // Security: Verify the source cooperative DID matches the claimed cooperative_id
        // and verify the signature
        let source_did = &attestation.trust_attestation.source_coop_did;
        let verifying_key = source_did.to_verifying_key().map_err(|e| {
            ComputeError::InvalidSignature(format!(
                "Failed to extract verifying key from source DID: {e}"
            ))
        })?;

        match attestation.verify(&verifying_key) {
            Ok(true) => {
                tracing::debug!(
                    executor = %executor,
                    cooperative_id = %cooperative_id,
                    "Federated executor attestation signature verified"
                );
            }
            Ok(false) => {
                tracing::warn!(
                    executor = %executor,
                    cooperative_id = %cooperative_id,
                    "Rejecting federated executor attestation: invalid signature"
                );
                return Err(ComputeError::InvalidSignature(
                    "Attestation signature verification failed".to_string(),
                ));
            }
            Err(e) => {
                tracing::warn!(
                    executor = %executor,
                    cooperative_id = %cooperative_id,
                    error = %e,
                    "Error verifying federated executor attestation"
                );
                return Err(e);
            }
        }

        // Rate limiting: prevent announcement flooding from any cooperative
        const MAX_ANNOUNCES_PER_WINDOW: u32 = 10; // Max 10 announcements per window
        const RATE_LIMIT_WINDOW_MS: u64 = 60_000; // 1 minute window

        let now = icn_time::current_timestamp_millis();
        {
            let mut rate_limiter = self.federated_announce_rate_limiter.lock().await;
            let entry = rate_limiter.entry(cooperative_id.clone()).or_insert((0, 0));

            // Check if we're in a new window
            if now - entry.0 > RATE_LIMIT_WINDOW_MS {
                // Reset window
                entry.0 = now;
                entry.1 = 1;
            } else {
                // Same window, check limit
                if entry.1 >= MAX_ANNOUNCES_PER_WINDOW {
                    tracing::warn!(
                        executor = %executor,
                        cooperative_id = %cooperative_id,
                        count = entry.1,
                        "Rate limiting federated executor announcement"
                    );
                    return Err(ComputeError::PolicyViolation(format!(
                        "Rate limit exceeded: max {MAX_ANNOUNCES_PER_WINDOW} announcements per minute from {cooperative_id}"
                    )));
                }
                entry.1 += 1;
            }
        }

        // Calculate attenuated trust score
        // Get our trust in the announcing cooperative
        let coop_trust = (self.trust_callback)(&cooperative_id);
        let federated_trust = crate::federation::FederatedExecutorRegistry::attenuate_trust_static(
            local_trust,
            coop_trust,
        );

        let now = icn_time::current_timestamp_millis();

        let info = ExecutorInfo {
            did: executor.clone(),
            cooperative_id: Some(cooperative_id.clone()),
            is_federated: true,
            capabilities,
            trust_score: local_trust, // Original trust from source coop
            federated_trust_score: Some(federated_trust),
            last_seen: now,
            tasks_executing: 0,
            capacity: None, // Will be updated via capacity announcements
            gateway_endpoint: attestation.gateway_endpoint.clone(),
        };

        // Register in local executor registry
        let mut registry = self.executor_registry.lock().await;
        registry.insert(executor.clone(), info);
        icn_obs::metrics::compute::executors_available_set(registry.len() as f64);

        tracing::debug!(
            executor = %executor,
            cooperative_id = %cooperative_id,
            federated_trust = federated_trust,
            "Registered federated executor"
        );

        Ok(())
    }

    /// Handle federated task request (Phase 21)
    ///
    /// Called when another cooperative requests task execution on our executors.
    /// Validates payment terms and routes to local placement.
    pub(super) async fn on_federated_task_request(
        &self,
        task_hash: TaskHash,
        task: crate::types::ComputeTask,
        from_coop: String,
        to_coop: String,
        payment: crate::federation::FederatedPaymentTerms,
        requested_at: u64,
    ) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(task_hash);

        tracing::info!(
            task_hash = %task_hash_str,
            from_coop = %from_coop,
            to_coop = %to_coop,
            amount = payment.amount,
            "Received federated task request"
        );

        // Verify this request is intended for our cooperative
        if let Some(ref our_coop) = self.own_cooperative_id {
            if our_coop != &to_coop {
                tracing::warn!(
                    task_hash = %task_hash_str,
                    our_coop = %our_coop,
                    to_coop = %to_coop,
                    "Federated task request not intended for our cooperative"
                );
                return Ok(());
            }
        } else {
            tracing::warn!(
                task_hash = %task_hash_str,
                "No cooperative ID configured, cannot process federated task"
            );
            return Ok(());
        }

        // Verify payment terms are acceptable
        const MIN_PAYMENT_THRESHOLD: u64 = 1; // Minimum 1 credit required for federated tasks

        if payment.amount < MIN_PAYMENT_THRESHOLD {
            tracing::warn!(
                task_hash = %task_hash_str,
                offered = payment.amount,
                minimum = MIN_PAYMENT_THRESHOLD,
                "Rejecting federated task: payment below minimum threshold"
            );
            return Err(ComputeError::PolicyViolation(format!(
                "Payment amount {} below minimum threshold {}",
                payment.amount, MIN_PAYMENT_THRESHOLD
            )));
        }

        // Validate exchange variance is reasonable (prevent excessive fees)
        const MAX_ALLOWED_EXCHANGE_VARIANCE: f64 = 0.25; // 25% max
        if payment.max_exchange_variance > MAX_ALLOWED_EXCHANGE_VARIANCE {
            tracing::warn!(
                task_hash = %task_hash_str,
                variance = payment.max_exchange_variance,
                max_allowed = MAX_ALLOWED_EXCHANGE_VARIANCE,
                "Rejecting federated task: exchange variance too high"
            );
            return Err(ComputeError::PolicyViolation(format!(
                "Exchange variance {} exceeds maximum {}",
                payment.max_exchange_variance, MAX_ALLOWED_EXCHANGE_VARIANCE
            )));
        }

        tracing::debug!(
            task_hash = %task_hash_str,
            amount = payment.amount,
            trigger = ?payment.payment_trigger,
            "Payment terms validated for federated task"
        );

        // Store task in task manager with federated metadata
        {
            let mut mgr = self.task_manager.lock().await;
            if let Err(e) = mgr.submit(task.clone()) {
                tracing::warn!(
                    task_hash = %task_hash_str,
                    error = %e,
                    "Failed to submit federated task to task manager"
                );
                return Err(e);
            }
        }

        // Emit local placement request
        // This will be handled by our local executors
        if let Some(ref cb) = self.send_callback {
            cb(ComputeMessage::PlacementRequest {
                task_hash,
                submitter: from_coop.clone(),
                resource_profile: task.resource_profile.unwrap_or_default(),
                locality_hints: vec![],
                max_cost: Some(payment.amount),
                requested_at,
                max_scope: None,     // TODO: populate from federated task constraints
                cell_affinity: None, // TODO: populate from federated task constraints
                allowed_scopes: vec![], // TODO: populate from federated task constraints
            });
        }

        tracing::debug!(
            task_hash = %task_hash_str,
            from_coop = %from_coop,
            "Initiated local placement for federated task"
        );

        Ok(())
    }

    /// Handle federated task result (Phase 21)
    ///
    /// Called when a federated executor reports task completion.
    /// Routes the result back to the original submitter and triggers payment settlement.
    pub(super) async fn on_federated_task_result(
        &self,
        result: crate::types::ComputeResult,
        executor_coop: String,
        attestation_hash: [u8; 32],
    ) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(result.task_hash);
        let success = matches!(result.outcome, crate::types::ExecutionOutcome::Success(_));

        tracing::info!(
            task_hash = %task_hash_str,
            executor = %result.executor,
            executor_coop = %executor_coop,
            success = success,
            attestation_hash = %hex::encode(attestation_hash),
            "Received federated task result"
        );

        // Process the result through normal channels
        // The attestation_hash can be used to verify the result came from a trusted source
        self.on_task_result(result.clone()).await?;

        // TODO: Trigger payment settlement via ClearingManager
        // This would involve:
        // 1. Verify attestation_hash matches expected value
        // 2. Look up payment terms from original FederatedTaskRequest
        // 3. Call ClearingManager::propose_transfer()

        tracing::debug!(
            task_hash = %task_hash_str,
            executor_coop = %executor_coop,
            "Processed federated task result"
        );

        Ok(())
    }
}
