//! Task lifecycle handlers for ComputeActor.
//!
//! Handles task submission, claiming, result consensus, and cancellation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::consensus::{outcome_to_value, results_match};
use super::types::{
    CommonsPaymentRequest, ComputeEvent, ExecutorInfo, PaymentRequest, ResultConsensus,
    SendCallback,
};
use super::ComputeActor;
use crate::error::ComputeError;
use crate::executor::Executor;
use crate::policy::CharterPriority;
use crate::result_quorum::{QuorumStatus, TaskVerification};
use crate::task::{TaskManager, TaskStatus};
use crate::types::{ComputeMessage, ComputeResult, ComputeTask, TaskHash, TaskPriority};
use crate::{MIN_TRUST_EXECUTE, MIN_TRUST_SUBMIT};

impl ComputeActor {
    /// Determine verification requirements for a task based on its estimated value
    /// or explicit verification configuration.
    ///
    /// Returns the verification requirements and whether multi-executor verification is needed.
    fn determine_verification(&self, task: &ComputeTask) -> (TaskVerification, bool) {
        // Use explicit verification if provided
        if let Some(ref verification) = task.verification {
            let requires_multi = verification.required_executors > 1;
            return (verification.clone(), requires_multi);
        }

        // Otherwise, classify based on estimated value
        let estimated_credits = task.estimated_value.unwrap_or(0);
        let value = self.quorum_manager.classify_value(estimated_credits);
        let verification = self.quorum_manager.create_verification(value);
        let requires_multi = !verification.is_single_executor();

        (verification, requires_multi)
    }

    /// Handle task result with consensus checking
    pub(super) async fn on_task_result(&self, result: ComputeResult) -> Result<(), ComputeError> {
        let task_hash_str = hex::encode(result.task_hash);

        tracing::debug!(
            task_id = %result.task_id,
            task_hash = %task_hash_str,
            executor = %result.executor,
            "Received task result"
        );

        // Verify signature
        let executor_did: icn_identity::Did = result.executor.parse().map_err(|e| {
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

            // Record Byzantine violation for invalid signature (Phase 18)
            if let Some(ref detector) = self.misbehavior_detector {
                let message_hash = {
                    use sha2::{Digest, Sha256};
                    let mut hasher = Sha256::new();
                    hasher.update(result.task_hash);
                    hasher.update(result.task_id.as_bytes());
                    hasher.finalize().to_vec()
                };

                let violation = icn_security::Violation::InvalidSignature {
                    message_hash: message_hash.clone().try_into().unwrap_or([0u8; 32]),
                };

                let detector_clone = detector.clone();
                let executor_clone = executor_did.clone();
                tokio::spawn(async move {
                    detector_clone.write().await.record_violation(
                        &executor_clone,
                        violation,
                        message_hash,
                    );
                });
            }

            return Err(e);
        }

        tracing::debug!(
            task_id = %result.task_id,
            task_hash = %task_hash_str,
            executor = %result.executor,
            "Signature verified successfully"
        );
        icn_obs::metrics::compute::signatures_verified_inc();

        // Check if this task is registered for multi-executor verification (Issue #511)
        let quorum_result = self.quorum_manager.submit_result(result.clone());

        // Handle quorum-based verification if task is registered
        if let Ok(status) = quorum_result {
            match status {
                QuorumStatus::Collecting { received, required } => {
                    tracing::debug!(
                        task_hash = %task_hash_str,
                        received = received,
                        required = required,
                        "Multi-executor task: collecting results"
                    );
                    return Ok(()); // Wait for more results
                }
                QuorumStatus::Consensus {
                    result_hash,
                    agreeing,
                    total,
                } => {
                    tracing::info!(
                        task_hash = %task_hash_str,
                        agreeing = agreeing,
                        total = total,
                        result_hash = %hex::encode(result_hash),
                        "Multi-executor task: consensus reached"
                    );

                    // Record quorum metrics
                    icn_obs::metrics::compute::quorum_consensus_inc("multi_executor");

                    // Get canonical result and proceed to completion
                    if let Ok(Some(canonical)) =
                        self.quorum_manager.get_canonical_result(&result.task_hash)
                    {
                        // Clean up quorum tracking
                        let _ = self.quorum_manager.remove_task(&result.task_hash);
                        return self.complete_task_result(canonical).await;
                    }
                }
                QuorumStatus::Divergent {
                    result_groups,
                    total,
                } => {
                    tracing::warn!(
                        task_hash = %task_hash_str,
                        result_groups = result_groups,
                        total = total,
                        "Multi-executor task: results diverge, escalating to dispute"
                    );

                    // Record quorum metrics
                    icn_obs::metrics::compute::quorum_divergent_inc(
                        "multi_executor",
                        result_groups,
                    );

                    // Get executor groups for dispute evidence
                    if let Ok(groups) = self.quorum_manager.get_executor_groups(&result.task_hash) {
                        // File disputes for divergent results
                        self.handle_divergent_results(&result.task_hash, groups)
                            .await;
                    }

                    // Accept majority result if available (before cleanup)
                    let canonical = self.quorum_manager.get_canonical_result(&result.task_hash);

                    // Clean up quorum tracking
                    let _ = self.quorum_manager.remove_task(&result.task_hash);

                    if let Ok(Some(canonical)) = canonical {
                        return self.complete_task_result(canonical).await;
                    }
                    return Ok(()); // No canonical result
                }
                QuorumStatus::Timeout { received, required } => {
                    tracing::warn!(
                        task_hash = %task_hash_str,
                        received = received,
                        required = required,
                        "Multi-executor task: collection timed out"
                    );

                    // Record quorum metrics
                    icn_obs::metrics::compute::quorum_timeout_inc(
                        "multi_executor",
                        received,
                        required,
                    );

                    // Proceed with available results if any (before cleanup)
                    let canonical = self.quorum_manager.get_canonical_result(&result.task_hash);

                    // Clean up quorum tracking
                    let _ = self.quorum_manager.remove_task(&result.task_hash);

                    if let Ok(Some(canonical)) = canonical {
                        return self.complete_task_result(canonical).await;
                    }
                    return Ok(());
                }
            }
        }

        // Fallback: Single-executor consensus (legacy path)
        // This handles tasks not registered with quorum manager
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
        if !consensus
            .results
            .iter()
            .any(|r| r.executor == result.executor)
        {
            consensus.results.push(result.clone());
        }

        // Check if we have enough confirmations
        if consensus.results.len() >= consensus.required {
            // Detect conflicts: compare all results against the first one
            let first_result = &consensus.results[0];
            let mut conflicts: Vec<&ComputeResult> = Vec::new();

            for r in consensus.results.iter().skip(1) {
                if !results_match(first_result, r) {
                    conflicts.push(r);
                }
            }

            // If we detected conflicts, record metrics and file disputes
            if !conflicts.is_empty() {
                tracing::warn!(
                    task_id = %result.task_id,
                    task_hash = %task_hash_str,
                    conflict_count = conflicts.len(),
                    "Compute result conflict detected between executors"
                );

                // Record conflict metric
                icn_obs::metrics::compute::result_conflicts_inc(&task_hash_str);

                // Auto-file disputes for each conflicting result
                if let Some(ref dispute_system) = self.dispute_resolution {
                    for conflicting in &conflicts {
                        let first_executor: icn_identity::Did = match first_result.executor.parse()
                        {
                            Ok(did) => did,
                            Err(_) => continue,
                        };
                        let conflicting_executor: icn_identity::Did =
                            match conflicting.executor.parse() {
                                Ok(did) => did,
                                Err(_) => continue,
                            };

                        let evidence = icn_ccl::DisputeEvidence {
                            task_hash: result.task_hash,
                            claimed_result: outcome_to_value(&conflicting.outcome),
                            reason: icn_ccl::DisputeReason::IncorrectResult {
                                expected: outcome_to_value(&first_result.outcome),
                                actual: outcome_to_value(&conflicting.outcome),
                            },
                            additional_data: icn_encoding::encode(&(
                                first_result.fuel_used,
                                conflicting.fuel_used,
                            ))
                            .unwrap_or_default(),
                            filed_at: std::time::SystemTime::now(),
                        };

                        let dispute_system_clone = dispute_system.clone();
                        let conflicting_executor_clone = conflicting_executor.clone();
                        let first_executor_clone = first_executor.clone();
                        let task_hash_for_dispute = result.task_hash;

                        tokio::spawn(async move {
                            let mut system = dispute_system_clone.write().await;
                            match system
                                .file_dispute(
                                    task_hash_for_dispute,
                                    conflicting_executor_clone,
                                    first_executor_clone,
                                    evidence,
                                )
                                .await
                            {
                                Ok(dispute_id) => {
                                    icn_obs::metrics::compute::result_conflict_disputes_filed_inc();
                                    tracing::info!(
                                        dispute_id = hex::encode(dispute_id),
                                        "Auto-filed dispute for compute result conflict"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        "Failed to auto-file dispute for result conflict"
                                    );
                                }
                            }
                        });
                    }
                }
            }

            // Accept the first result (future: implement majority voting)
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
                }
            }
            drop(registry);
            self.decrement_scope_queue(&task_hash).await;

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

    /// Complete a task with the given result (used by both quorum and legacy paths)
    async fn complete_task_result(&self, result: ComputeResult) -> Result<(), ComputeError> {
        let executor_did = result.executor.clone();
        let task_hash = result.task_hash;
        let outcome = result.outcome.clone();
        let fuel_used = result.fuel_used;
        let duration_ms = result.duration_ms;

        let mut mgr = self.task_manager.lock().await;
        if mgr.get(&task_hash).is_some() {
            mgr.complete(result)?;
        }
        drop(mgr);

        // Decrement executor load
        let mut registry = self.executor_registry.lock().await;
        if let Some(info) = registry.get_mut(&executor_did) {
            if info.tasks_executing > 0 {
                info.tasks_executing -= 1;
            }
        }
        drop(registry);
        self.decrement_scope_queue(&task_hash).await;

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

        Ok(())
    }

    /// Handle divergent results from multi-executor verification (Issue #511)
    async fn handle_divergent_results(
        &self,
        task_hash: &TaskHash,
        executor_groups: std::collections::HashMap<[u8; 32], Vec<String>>,
    ) {
        let task_hash_str = hex::encode(task_hash);

        // Record conflict metric
        icn_obs::metrics::compute::result_conflicts_inc(&task_hash_str);

        // Find the majority group (largest)
        let majority_group = executor_groups
            .iter()
            .max_by_key(|(_, executors)| executors.len())
            .map(|(hash, executors)| (hash, executors.clone()));

        // Auto-file disputes against minority groups
        if let Some(ref dispute_system) = self.dispute_resolution {
            if let Some((majority_hash, majority_executors)) = majority_group {
                // Use first executor in majority as the "correct" reference
                if let Some(majority_executor_str) = majority_executors.first() {
                    let majority_executor: icn_identity::Did = match majority_executor_str.parse() {
                        Ok(did) => did,
                        Err(_) => return,
                    };

                    // File disputes against each minority group
                    for (result_hash, executors) in &executor_groups {
                        if result_hash == majority_hash {
                            continue; // Skip majority group
                        }

                        for executor_str in executors {
                            let minority_executor: icn_identity::Did = match executor_str.parse() {
                                Ok(did) => did,
                                Err(_) => continue,
                            };

                            let evidence = icn_ccl::DisputeEvidence {
                                task_hash: *task_hash,
                                claimed_result: icn_ccl::Value::String(hex::encode(result_hash)),
                                reason: icn_ccl::DisputeReason::IncorrectResult {
                                    expected: icn_ccl::Value::String(hex::encode(majority_hash)),
                                    actual: icn_ccl::Value::String(hex::encode(result_hash)),
                                },
                                additional_data: vec![],
                                filed_at: std::time::SystemTime::now(),
                            };

                            let dispute_system_clone = dispute_system.clone();
                            let minority_executor_clone = minority_executor.clone();
                            let majority_executor_clone = majority_executor.clone();
                            let task_hash_for_dispute = *task_hash;

                            tokio::spawn(async move {
                                let mut system = dispute_system_clone.write().await;
                                match system
                                    .file_dispute(
                                        task_hash_for_dispute,
                                        minority_executor_clone,
                                        majority_executor_clone,
                                        evidence,
                                    )
                                    .await
                                {
                                    Ok(dispute_id) => {
                                        icn_obs::metrics::compute::result_conflict_disputes_filed_inc(
                                        );
                                        tracing::info!(
                                            dispute_id = hex::encode(dispute_id),
                                            "Auto-filed dispute for multi-executor result divergence"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            error = %e,
                                            "Failed to auto-file dispute for result divergence"
                                        );
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
    }

    /// Handle task cancellation
    pub(super) async fn on_task_cancelled(
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
            if let Some(TaskStatus::Claimed { executor, .. }) = mgr.status(&task_hash) {
                Some(executor.clone())
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
                }
            }
        }
        self.decrement_scope_queue(&task_hash).await;

        // Clean up pending placement state (Issue #511 - prevent memory leak)
        {
            let mut requirements = self.pending_executor_requirements.lock().await;
            requirements.remove(&task_hash);
        }
        {
            let mut offers = self.pending_offers.lock().await;
            offers.remove(&task_hash);
        }
        {
            let mut timestamps = self.pending_request_timestamps.lock().await;
            timestamps.remove(&task_hash);
        }

        // Clean up quorum tracking if task was registered
        let _ = self.quorum_manager.remove_task(&task_hash);

        tracing::info!(
            task_hash = %task_hash_str,
            "Task cancelled successfully"
        );

        Ok(())
    }

    /// Handle received task submission
    pub(super) async fn on_task_submitted(&self, task: ComputeTask) -> Result<(), ComputeError> {
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
        {
            let executor = self.executor.read().await;
            if !executor.can_execute(&task) {
                tracing::debug!(
                    task_id = %task.id,
                    "Skipping task: missing required capabilities"
                );
                return Ok(()); // Can't execute this task
            }
        }

        // Store task
        let _hash = self.task_manager.lock().await.submit(task.clone())?;

        // Find highest-priority pending task we can execute
        let highest_priority_task = {
            let mgr = self.task_manager.lock().await;
            let pending = mgr.pending_by_priority();
            let executor = self.executor.read().await;
            pending
                .into_iter()
                .find(|(_, t)| executor.can_execute(t))
                .map(|(h, _)| h)
        };

        // Claim highest-priority task if available
        let (hash, claimed_task) = if let Some(h) = highest_priority_task {
            self.task_manager
                .lock()
                .await
                .claim(&h, self.own_did.clone())?;

            let mgr = self.task_manager.lock().await;
            let t = mgr
                .get(&h)
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
            // Note: Per-executor load tracking removed (high-cardinality)
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

        // Resolve WasmRef to WasmInline before execution (Issue #1074).
        // Authz has already happened above (trust check, capacity check, policy check).
        // This fetch is async and must happen BEFORE the sync Executor::execute() call.
        let mut resolved_task = claimed_task.clone();
        if let crate::types::TaskCode::WasmRef(ref wasm_hash) = claimed_task.code {
            let wasm_hash_hex = hex::encode(wasm_hash);
            match &self.wasm_registry {
                Some(registry) => {
                    tracing::debug!(
                        task_id = %claimed_task.id,
                        wasm_hash = %wasm_hash_hex,
                        "Resolving WasmRef module (local or remote)"
                    );
                    match registry.resolve_or_fetch(wasm_hash).await {
                        Ok(wasm_bytes) => {
                            tracing::info!(
                                task_id = %claimed_task.id,
                                wasm_hash = %wasm_hash_hex,
                                size = wasm_bytes.len(),
                                "WasmRef resolved successfully"
                            );
                            resolved_task.code = crate::types::TaskCode::WasmInline(wasm_bytes);
                        }
                        Err(e) => {
                            tracing::warn!(
                                task_id = %claimed_task.id,
                                wasm_hash = %wasm_hash_hex,
                                error = %e,
                                "WasmRef resolution failed"
                            );
                            // Mark task as failed and clean up
                            self.task_manager
                                .lock()
                                .await
                                .fail(&hash, format!("Module fetch failed: {e}"))?;
                            let mut registry = self.executor_registry.lock().await;
                            if let Some(info) = registry.get_mut(&self.own_did) {
                                if info.tasks_executing > 0 {
                                    info.tasks_executing -= 1;
                                }
                            }
                            drop(registry);
                            self.decrement_scope_queue(&hash).await;
                            return Err(ComputeError::ModuleFetchFailed {
                                hash: wasm_hash_hex,
                                reason: e.to_string(),
                            });
                        }
                    }
                }
                None => {
                    tracing::warn!(
                        task_id = %claimed_task.id,
                        wasm_hash = %wasm_hash_hex,
                        "WasmRef requires WASM registry (not configured)"
                    );
                    self.task_manager
                        .lock()
                        .await
                        .fail(&hash, "WASM registry not configured".to_string())?;
                    let mut registry = self.executor_registry.lock().await;
                    if let Some(info) = registry.get_mut(&self.own_did) {
                        if info.tasks_executing > 0 {
                            info.tasks_executing -= 1;
                        }
                    }
                    drop(registry);
                    self.decrement_scope_queue(&hash).await;
                    return Err(ComputeError::ModuleFetchFailed {
                        hash: wasm_hash_hex,
                        reason: "WASM registry not configured".to_string(),
                    });
                }
            }
        }

        // Execute
        let start = std::time::Instant::now();
        let mut result = {
            let executor = self.executor.read().await;
            executor.execute_task(&resolved_task, &self.own_did, &self.signing_key)?
        };
        let duration = start.elapsed().as_secs_f64();

        // When a WasmRef was resolved to WasmInline, the resolved_task has a
        // different hash than the original claimed_task. The result must carry
        // the *original* task hash so it matches the TaskManager entry.
        // Re-sign because signing_payload includes task_hash.
        if matches!(claimed_task.code, crate::types::TaskCode::WasmRef(_)) {
            result.task_hash = hash;
            let signing_key_bytes: [u8; 32] =
                self.signing_key.as_slice().try_into().map_err(|_| {
                    ComputeError::InvalidSignature("Invalid signing key length".into())
                })?;
            let ed25519_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
            result.sign(&ed25519_key);
        }

        // Record metrics
        icn_obs::metrics::compute::task_duration_record(duration);
        icn_obs::metrics::compute::fuel_used_record(result.fuel_used);
        icn_obs::metrics::compute::fuel_total_add(result.fuel_used);

        // Record contribution metrics (Phase 21.1)
        // Fuel approximates CPU work; we convert fuel to CPU-seconds using a ratio
        // For now, 1000 fuel = 1 CPU-second (calibrated based on CCL interpreter)
        let cpu_seconds = result.fuel_used / 1000;
        if cpu_seconds > 0 {
            icn_obs::metrics::contribution::total_compute_cpu_seconds_add(cpu_seconds);
        }
        icn_obs::metrics::contribution::total_compute_job_completed(duration);

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
            }
        }
        drop(registry);
        self.decrement_scope_queue(&result.task_hash).await;

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
            if let (Some(rate), Some(ref payment_cb)) =
                (claimed_task.payment_rate, &self.payment_callback)
            {
                let amount = (result.fuel_used * rate) / 1000; // rate is per 1000 fuel
                if amount > 0 {
                    let currency = claimed_task
                        .payment_currency
                        .clone()
                        .unwrap_or_else(|| "credits".to_string());
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

        // Commons credit settlement (E7 - #948)
        //
        // When a commons-scope task completes successfully, fire the commons settlement
        // callback. The callback is responsible for fetching the consumer's balance,
        // calling settle_commons_receipt(), and appending the entries to the ledger.
        //
        // Belt + suspenders: settle_commons_receipt() hard-rejects any scope != Commons,
        // so even if this check is bypassed, the ledger invariant holds.
        if let crate::types::ExecutionOutcome::Success(_) = &result.outcome {
            if claimed_task.scope == icn_kernel_api::ScopeLevel::Commons {
                if let Some(ref commons_cb) = self.commons_settlement_callback {
                    // Compute credits earned from resource consumption.
                    // duration_ms approximates CPU time; this mirrors the formula in
                    // icn_ledger::commons_credits::compute_credits_earned() with
                    // memory/storage/egress=0 (not yet tracked per-result).
                    // Formula: cpu_millis + (mem_mb_millis/1000) + (storage/1M) + (egress/100k)
                    // Minimum 1 credit per commons task to account for task overhead.
                    let credits: u64 = result.duration_ms.max(1);
                    tracing::info!(
                        task_id = %claimed_task.id,
                        contributor = %self.own_did,
                        consumer = %claimed_task.submitter,
                        credits = credits,
                        "Settling commons credits for completed task"
                    );
                    commons_cb(CommonsPaymentRequest {
                        contributor: self.own_did.clone(),
                        consumer: claimed_task.submitter.clone(),
                        amount: credits,
                        receipt_hash: result.task_hash,
                        task_id: claimed_task.id.clone(),
                    });
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
    pub(super) async fn on_task_claimed(
        &self,
        task_hash: TaskHash,
        executor: String,
    ) -> Result<(), ComputeError> {
        // Single lock: look up submitter and claim atomically to avoid a
        // race where the task is removed between two separate locks.
        let submitter = {
            let mut mgr = self.task_manager.lock().await;
            let submitter = mgr.get(&task_hash).map(|t| t.submitter.clone());
            if submitter.is_some() {
                let _ = mgr.claim(&task_hash, executor.clone());
            }
            submitter
        };

        // Update executor load tracking
        let mut registry = self.executor_registry.lock().await;
        if let Some(info) = registry.get_mut(&executor) {
            info.tasks_executing += 1;
        }
        drop(registry);

        // Epic 2 (#933): Track per-scope queue depth
        if let Some(submitter) = submitter {
            let scope = match &self.cell_service {
                Some(cs) => cs.peer_scope(&submitter),
                None => icn_kernel_api::ScopeLevel::Commons,
            };
            {
                let mut depths = self.scope_queue_depths.lock().await;
                *depths.entry(scope).or_insert(0) += 1;
            }
            {
                let mut map = self.task_scope_map.lock().await;
                map.insert(task_hash, scope);
                icn_obs::metrics::compute::task_scope_map_size_set(map.len() as f64);
            }
        }

        Ok(())
    }

    /// Check for timed-out tasks and mark them as failed
    pub(super) async fn check_timeouts(
        task_manager: &Arc<Mutex<TaskManager>>,
        executor_registry: &Arc<Mutex<HashMap<String, ExecutorInfo>>>,
        send_callback: &Option<SendCallback>,
        scope_queue_depths: &Arc<Mutex<HashMap<icn_kernel_api::ScopeLevel, usize>>>,
        task_scope_map: &Arc<Mutex<HashMap<TaskHash, icn_kernel_api::ScopeLevel>>>,
    ) -> Result<(), ComputeError> {
        let now = icn_time::current_timestamp_millis();

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
            let task_id = mgr
                .get(&hash)
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
                    }
                }
            }
            // Decrement scope queue depth
            {
                let scope = task_scope_map.lock().await.remove(&hash);
                if let Some(scope) = scope {
                    let mut depths = scope_queue_depths.lock().await;
                    if let Some(count) = depths.get_mut(&scope) {
                        *count = count.saturating_sub(1);
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

    /// Handle task submission (validation and broadcasting)
    pub(super) async fn handle_submit(&self, task: ComputeTask) -> Result<TaskHash, ComputeError> {
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

        // Commons pool governance checks (E7 - #1134)
        // Standing and credit ceiling validation gate access to shared compute resources.
        if let Some(ref policy) = self.commons_pool_policy {
            // Standing check: member must meet minimum participation threshold.
            // Uses the same trust score already computed above.
            policy.check_standing(trust).inspect_err(|_| {
                tracing::warn!(
                    task_id = %task.id,
                    submitter = %task.submitter,
                    trust_score = trust,
                    required = policy.min_standing,
                    "Commons task rejected: insufficient member standing"
                );
            })?;

            // Credit ceiling check: submitter must have headroom under their ceiling.
            if let Some(ref balance_cb) = self.balance_callback {
                let balance = balance_cb(&task.submitter);
                policy
                    .validate_submitter_credit(&task, balance)
                    .inspect_err(|_| {
                        tracing::warn!(
                            task_id = %task.id,
                            submitter = %task.submitter,
                            balance = balance,
                            "Commons task rejected: credit ceiling exceeded"
                        );
                    })?;
            }
        }

        // Commons credit floor check (#1397)
        // Prevents zero-balance submitters from consuming commons compute.
        // Applies only to Commons-scoped tasks; Local/Cell tasks are unaffected.
        // Enforcement is opt-in: if balance_callback is not configured, this gate
        // is skipped silently (e.g., during tests that only exercise scheduling logic).
        // V1 floor: 1 credit. Dynamic per-task cost estimation is a future concern.
        if task.scope == icn_kernel_api::ScopeLevel::Commons {
            if let Some(ref balance_cb) = self.balance_callback {
                let balance = balance_cb(&task.submitter);
                let required = crate::cost::compute_credits_required(&task);
                if balance < required {
                    tracing::warn!(
                        task_id = %task.id,
                        submitter = %task.submitter,
                        balance,
                        required,
                        "Commons task rejected: insufficient credits"
                    );
                    return Err(ComputeError::InsufficientCommonsCredits { balance, required });
                }
            }
        }

        // Check policy compliance (Phase 16E)
        let mut adjusted_task = task.clone();
        if let Some(ref policy_manager) = self.policy_manager {
            // Parse submitter DID
            let submitter_did = icn_identity::Did::from_str(&task.submitter)
                .map_err(|e| ComputeError::InvalidInput(format!("Invalid submitter DID: {e}")))?;

            // Extract coop_id from task (default to "default" if not specified)
            let coop_id = task.coop_id.as_deref().unwrap_or("default");

            // Check policy
            match policy_manager
                .check_submission(&task, &submitter_did, coop_id)
                .await?
            {
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
                        has_constraints = placement_constraints.required_region.is_some(),
                        "Policy check passed with adjustments"
                    );
                }
            }
        }

        // Apply charter-defined priority ordering (E7 - #1134).
        // Charter priority may cap or reorder priorities relative to member standing.
        // This runs after `CoopSchedulingPolicy` adjustments so both layers compose.
        if let Some(ref policy) = self.commons_pool_policy {
            let original = adjusted_task.priority;
            adjusted_task.priority = apply_commons_charter_priority(policy, &adjusted_task, trust);
            if adjusted_task.priority != original {
                tracing::debug!(
                    task_id = %task.id,
                    original_priority = ?original,
                    charter_priority = ?adjusted_task.priority,
                    charter_policy = ?policy.priority,
                    "Charter priority applied"
                );
            }
        }

        // Add to local task manager (use adjusted task)
        let hash = self
            .task_manager
            .lock()
            .await
            .submit(adjusted_task.clone())?;
        icn_obs::metrics::compute::tasks_submitted_inc();

        // Determine verification requirements (Issue #511)
        let (verification, requires_multi_executor) = self.determine_verification(&adjusted_task);

        tracing::info!(
            task_id = %task.id,
            task_hash = %hex::encode(hash),
            submitter = %task.submitter,
            fuel_limit = task.fuel_limit.0,
            verification_value = ?verification.value,
            required_executors = verification.required_executors,
            "Task submitted successfully"
        );

        // Register high-value tasks with quorum manager for multi-executor verification
        if requires_multi_executor {
            if let Err(e) = self
                .quorum_manager
                .register_task(hash, verification.clone())
            {
                tracing::warn!(
                    task_hash = %hex::encode(hash),
                    error = %e,
                    "Failed to register task for quorum verification"
                );
                // Continue anyway - task can still be executed with single-executor fallback
            } else {
                tracing::debug!(
                    task_hash = %hex::encode(hash),
                    required_executors = verification.required_executors,
                    "Task registered for multi-executor verification"
                );
            }
        }

        // Broadcast to network - use placement negotiation if resource profile provided
        if let Some(ref cb) = self.send_callback {
            if let Some(ref profile) = task.resource_profile {
                // Phase 16B: Use placement negotiation
                let now = icn_time::current_timestamp_millis();

                tracing::debug!(
                    task_hash = %hex::encode(hash),
                    required_executors = verification.required_executors,
                    "Using placement negotiation (resource profile provided)"
                );

                // Store request timestamp for duration metrics
                self.pending_request_timestamps
                    .lock()
                    .await
                    .insert(hash, now);

                // Store required executor count for multi-executor selection (Issue #511)
                if requires_multi_executor {
                    self.pending_executor_requirements
                        .lock()
                        .await
                        .insert(hash, verification.required_executors);
                }

                cb(ComputeMessage::PlacementRequest {
                    task_hash: hash,
                    submitter: task.submitter.clone(),
                    resource_profile: profile.clone(),
                    locality_hints: vec![],      // Future enhancement
                    max_cost: task.payment_rate, // Use payment_rate as max cost
                    requested_at: now,
                    max_scope: None,        // TODO: populate from task constraints
                    cell_affinity: None,    // TODO: populate from task constraints
                    allowed_scopes: vec![], // TODO: populate from task constraints
                });
            } else {
                // Phase 15: Legacy immediate claiming
                tracing::debug!(
                    task_hash = %hex::encode(hash),
                    "Using legacy claiming (no resource profile)"
                );

                cb(ComputeMessage::TaskSubmitted(Box::new(task)));
            }
        }

        Ok(hash)
    }

    /// Cancel a submitted task
    pub(super) async fn cancel_task(
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
        let now = icn_time::current_timestamp_millis();

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

    /// Decrement the per-scope queue depth for a completed/failed task (Epic 2 #933).
    pub(super) async fn decrement_scope_queue(&self, task_hash: &TaskHash) {
        let scope = {
            let mut map = self.task_scope_map.lock().await;
            let scope = map.remove(task_hash);
            icn_obs::metrics::compute::task_scope_map_size_set(map.len() as f64);
            scope
        };
        if let Some(scope) = scope {
            let mut depths = self.scope_queue_depths.lock().await;
            if let Some(count) = depths.get_mut(&scope) {
                *count = count.saturating_sub(1);
            }
        }
    }
}

/// Apply charter-defined priority ordering for commons compute (E7 - #1134).
///
/// Charter policies express cooperative values as scheduling constraints:
/// - `Fifo`: Pure arrival order. Equal treatment, minimal overhead.
/// - `EmergencyFirst`: Critical tasks preempt; cooperative constitution declares emergencies.
/// - `UbsFirst`: Essential services (health, energy, comms) hold Critical priority;
///   commercial tasks are capped at Normal to preserve headroom.
/// - `FairShare`: Member standing gates maximum priority. High contributors get
///   full priority access; low-standing members are throttled proportionally.
///
/// This runs after `CoopSchedulingPolicy` adjustments so both layers compose.
pub(super) fn apply_commons_charter_priority(
    policy: &crate::policy::CommonsPoolPolicy,
    task: &ComputeTask,
    standing: f64,
) -> TaskPriority {
    match policy.priority {
        // Arrival order: no priority modification.
        CharterPriority::Fifo => task.priority,
        // Emergency policy: preserve priority as-is; Critical tasks already sort highest.
        CharterPriority::EmergencyFirst => task.priority,
        // UBS-first: UBS infrastructure submits as Critical.
        // Non-essential commercial work is capped at Normal.
        CharterPriority::UbsFirst => {
            if task.priority < TaskPriority::Critical {
                task.priority.min(TaskPriority::Normal)
            } else {
                TaskPriority::Critical
            }
        }
        // Fair share: standing proportionally gates maximum allowed priority.
        // Ensures high-participation members aren't crowded out by low-standing submitters.
        CharterPriority::FairShare => {
            if standing >= 0.7 {
                task.priority // High standing: full priority access
            } else if standing >= 0.4 {
                task.priority.min(TaskPriority::Normal) // Medium: cap at Normal
            } else {
                task.priority.min(TaskPriority::Low) // Low standing: cap at Low
            }
        }
    }
}
