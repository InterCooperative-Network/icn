//! Task lifecycle handlers for ComputeActor.
//!
//! Handles task submission, claiming, result consensus, and cancellation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::consensus::{outcome_to_value, results_match};
use super::types::{ComputeEvent, ExecutorInfo, PaymentRequest, ResultConsensus, SendCallback};
use super::ComputeActor;
use crate::error::ComputeError;
use crate::executor::Executor;
use crate::task::{TaskManager, TaskStatus};
use crate::types::{ComputeMessage, ComputeResult, ComputeTask, TaskHash};
use crate::{MIN_TRUST_EXECUTE, MIN_TRUST_SUBMIT};

impl ComputeActor {
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
                            additional_data: bincode::serde::encode_to_vec(
                                (&first_result.fuel_used, &conflicting.fuel_used),
                                bincode::config::legacy(),
                            )
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
                    icn_obs::metrics::compute::executor_load_set(
                        &executor_did,
                        info.tasks_executing as f64,
                    );
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
                    icn_obs::metrics::compute::executor_load_set(
                        &executor,
                        info.tasks_executing as f64,
                    );
                }
            }
        }

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
            icn_obs::metrics::compute::executor_load_set(
                &self.own_did,
                info.tasks_executing as f64,
            );
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

        // Record contribution metrics (Phase 21.1)
        // Fuel approximates CPU work; we convert fuel to CPU-seconds using a ratio
        // For now, 1000 fuel = 1 CPU-second (calibrated based on CCL interpreter)
        let cpu_seconds = result.fuel_used / 1000;
        if cpu_seconds > 0 {
            icn_obs::metrics::contribution::compute_cpu_seconds_add(&self.own_did, cpu_seconds);
        }
        icn_obs::metrics::contribution::compute_job_completed(&self.own_did, duration);

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
                icn_obs::metrics::compute::executor_load_set(
                    &self.own_did,
                    info.tasks_executing as f64,
                );
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

    /// Check for timed-out tasks and mark them as failed
    pub(super) async fn check_timeouts(
        task_manager: &Arc<Mutex<TaskManager>>,
        executor_registry: &Arc<Mutex<HashMap<String, ExecutorInfo>>>,
        send_callback: &Option<SendCallback>,
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
                        icn_obs::metrics::compute::executor_load_set(
                            &executor,
                            info.tasks_executing as f64,
                        );
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

        // Add to local task manager (use adjusted task)
        let hash = self
            .task_manager
            .lock()
            .await
            .submit(adjusted_task.clone())?;
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
                let now = icn_time::current_timestamp_millis();

                tracing::debug!(
                    task_hash = %hex::encode(hash),
                    "Using placement negotiation (resource profile provided)"
                );

                // Store request timestamp for duration metrics
                self.pending_request_timestamps
                    .lock()
                    .await
                    .insert(hash, now);

                cb(ComputeMessage::PlacementRequest {
                    task_hash: hash,
                    submitter: task.submitter.clone(),
                    resource_profile: profile.clone(),
                    locality_hints: vec![],      // Future enhancement
                    max_cost: task.payment_rate, // Use payment_rate as max cost
                    requested_at: now,
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
}
