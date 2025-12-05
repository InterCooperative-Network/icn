//! Contract Execution Dispute Resolution
//!
//! Phase 18 Week 4: Contract Execution Disputes
//!
//! This module provides dispute resolution for compute task execution. When a compute result
//! is challenged, the system re-executes the contract to verify correctness and records
//! misbehavior if discrepancies are found.

use crate::interpreter::Interpreter;
use crate::{Contract, Value};
use anyhow::{anyhow, Result};
use icn_identity::Did;
use icn_store::{ContentHash, Store};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{info, warn};

/// Gossip topic for filing disputes
pub const TOPIC_DISPUTES_FILE: &str = "disputes:file";

/// Callback for recording misbehavior violations
/// Arguments: (violator_did, task_hash, evidence_bytes)
pub type MisbehaviorCallback = Arc<dyn Fn(&Did, ContentHash, Vec<u8>) + Send + Sync>;

/// Callback for querying trust scores from the trust graph
/// Takes a DID string and returns the trust score (0.0 to 1.0)
pub type TrustCallback = Arc<dyn Fn(&str) -> f64 + Send + Sync>;

/// Unique identifier for a dispute
pub type DisputeId = [u8; 32];

/// Message types for dispute gossip protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisputeMessage {
    /// Dispute filed for a compute task
    DisputeFiled {
        dispute_id: DisputeId,
        task_hash: ContentHash,
        executor: Did,
        challenger: Did,
        reason: DisputeReason,
        filed_at: u64,
    },
    /// Dispute resolved
    DisputeResolved {
        dispute_id: DisputeId,
        outcome: DisputeOutcome,
        resolved_at: u64,
    },
}

/// Outcome of a dispute resolution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisputeOutcome {
    /// Original submitter was correct, challenger was wrong
    SubmitterCorrect { verified_result: Value },

    /// Executor was correct, original submitter made an error
    ExecutorCorrect { verified_result: Value },

    /// Both were wrong, third-party re-execution found different result
    BothWrong { correct_result: Value },

    /// Cannot determine outcome automatically, requires human arbitration
    Inconclusive {
        reason: String,
        mediator_assigned: Option<Did>,
    },
}

/// Reason why a dispute was filed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisputeReason {
    /// Claimed result doesn't match expected output
    IncorrectResult { expected: Value, actual: Value },

    /// Execution exceeded fuel limit but wasn't reported
    FuelLimitExceeded { claimed_fuel: u64, actual_fuel: u64 },

    /// Contract execution failed but success was claimed
    ExecutionFailed { error: String },

    /// Timeout occurred but wasn't reported
    TimeoutNotReported {
        claimed_duration_ms: u64,
        actual_duration_ms: u64,
    },
}

/// Evidence provided when filing a dispute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeEvidence {
    /// Original task hash that's being disputed
    pub task_hash: ContentHash,

    /// Claimed result from the executor
    pub claimed_result: Value,

    /// Reason for the dispute
    pub reason: DisputeReason,

    /// Additional evidence data (contract inputs, execution logs, etc.)
    pub additional_data: Vec<u8>,

    /// Timestamp when dispute was filed
    pub filed_at: SystemTime,
}

/// Status of a dispute
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisputeStatus {
    /// Dispute has been filed, awaiting re-execution
    Pending,

    /// Re-execution in progress
    Investigating,

    /// Resolved automatically via re-execution
    Resolved {
        outcome: DisputeOutcome,
        resolved_at: SystemTime,
    },

    /// Assigned to mediator for manual review
    UnderMediation {
        mediator: Did,
        assigned_at: SystemTime,
    },

    /// Closed by mediator
    Closed {
        outcome: DisputeOutcome,
        mediator: Did,
        closed_at: SystemTime,
    },
}

/// A dispute record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispute {
    pub dispute_id: DisputeId,
    pub task_hash: ContentHash,
    pub challenger: Did,
    pub executor: Did,
    pub evidence: DisputeEvidence,
    pub status: DisputeStatus,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

/// Configuration for dispute resolution
#[derive(Debug, Clone)]
pub struct DisputeConfig {
    /// Maximum time to wait for re-execution (default: 60 seconds)
    pub re_execution_timeout: Duration,

    /// Whether to auto-assign mediators for inconclusive disputes
    pub auto_assign_mediators: bool,

    /// Minimum trust score required to be a mediator (default: 0.7 = Federated)
    pub mediator_min_trust: f64,
}

impl Default for DisputeConfig {
    fn default() -> Self {
        Self {
            re_execution_timeout: Duration::from_secs(60),
            auto_assign_mediators: true,
            mediator_min_trust: 0.7, // Federated trust class
        }
    }
}

/// Mediator information including expertise and workload
#[derive(Debug, Clone)]
pub struct MediatorInfo {
    /// The mediator's DID
    pub did: Did,
    /// Domain expertise tags (e.g., "finance", "technical", "governance")
    pub expertise_tags: Vec<String>,
}

/// Dispute resolution system for contract execution
pub struct DisputeResolutionSystem {
    config: DisputeConfig,
    dispute_store: Arc<dyn Store>,
    disputes: HashMap<DisputeId, Dispute>,
    mediator_pool: Vec<MediatorInfo>,
    misbehavior_callback: Option<MisbehaviorCallback>,
    trust_callback: Option<TrustCallback>,
}

impl DisputeResolutionSystem {
    pub fn new(config: DisputeConfig, dispute_store: Arc<dyn Store>) -> Self {
        Self {
            config,
            dispute_store,
            disputes: HashMap::new(),
            mediator_pool: Vec::new(),
            misbehavior_callback: None,
            trust_callback: None,
        }
    }

    /// Set the trust callback for querying mediator trust scores
    pub fn set_trust_callback(&mut self, callback: TrustCallback) {
        self.trust_callback = Some(callback);
    }

    /// Set the misbehavior callback for recording violations
    pub fn set_misbehavior_callback(&mut self, callback: MisbehaviorCallback) {
        self.misbehavior_callback = Some(callback);
    }

    /// File a dispute for a compute task result
    pub async fn file_dispute(
        &mut self,
        task_hash: ContentHash,
        executor: Did,
        challenger: Did,
        evidence: DisputeEvidence,
    ) -> Result<DisputeId> {
        // Generate dispute ID from task hash + challenger + timestamp
        let dispute_id = self.generate_dispute_id(&task_hash, &challenger);

        info!(
            "Filing dispute {} for task {:?} by {}",
            hex::encode(dispute_id),
            hex::encode(task_hash),
            challenger
        );

        // Create dispute record
        let dispute = Dispute {
            dispute_id,
            task_hash,
            challenger: challenger.clone(),
            executor: executor.clone(),
            evidence: evidence.clone(),
            status: DisputeStatus::Pending,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
        };

        // Store dispute
        self.disputes.insert(dispute_id, dispute.clone());
        self.persist_dispute(&dispute)?;

        // Note: Investigation must be triggered separately via investigate_dispute()
        // once the contract and execution context are available. This allows for
        // flexible investigation timing and avoids coupling dispute filing with
        // contract retrieval.

        Ok(dispute_id)
    }

    /// Re-execute a contract to verify the claimed result
    pub async fn investigate_dispute(
        &mut self,
        dispute_id: DisputeId,
        contract: &Contract,
        rule_name: &str,
        args: HashMap<String, Value>,
    ) -> Result<DisputeOutcome> {
        // Get the dispute and extract the fields we need before any mutable operations
        let (challenger, executor, evidence_claimed_result, task_hash) = {
            let dispute = self
                .disputes
                .get(&dispute_id)
                .ok_or_else(|| anyhow!("Dispute not found"))?;
            (
                dispute.challenger.clone(),
                dispute.executor.clone(),
                dispute.evidence.claimed_result.clone(),
                dispute.evidence.task_hash,
            )
        };

        // Update status to investigating
        {
            let dispute = self.disputes.get_mut(&dispute_id).unwrap();
            dispute.status = DisputeStatus::Investigating;
            dispute.updated_at = SystemTime::now();
        }

        info!(
            "Investigating dispute {} - re-executing contract",
            hex::encode(dispute_id)
        );

        // Create execution context for re-execution
        let context = crate::types::ExecutionContext {
            caller: challenger.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            fuel: 10000, // Sufficient fuel for re-execution
            capabilities: vec![],
            participants: vec![executor.clone(), challenger.clone()],
        };

        let state = crate::types::ContractState::new();

        // Create a fresh interpreter for clean re-execution
        let interpreter = Interpreter::new(contract.clone(), state, context);

        // Re-execute the contract with timeout enforcement
        let timeout_duration = self.config.re_execution_timeout;
        let rule_name_owned = rule_name.to_string();

        // Wrap synchronous execution in spawn_blocking and apply timeout
        let execution_future = tokio::task::spawn_blocking(move || {
            interpreter.execute_rule(&rule_name_owned, args)
        });

        let re_execution_result = match tokio::time::timeout(timeout_duration, execution_future).await {
            Ok(Ok(Ok(result))) => result.value,
            Ok(Ok(Err(e))) => {
                warn!(
                    "Re-execution failed for dispute {}: {}",
                    hex::encode(dispute_id),
                    e
                );

                // If re-execution fails, mark as inconclusive
                let mediator = {
                    let dispute = self.disputes.get(&dispute_id).unwrap();
                    self.assign_mediator(dispute)?
                };

                let outcome = DisputeOutcome::Inconclusive {
                    reason: format!("Re-execution failed: {e}"),
                    mediator_assigned: mediator,
                };

                {
                    let dispute = self.disputes.get_mut(&dispute_id).unwrap();
                    dispute.status = DisputeStatus::Resolved {
                        outcome: outcome.clone(),
                        resolved_at: SystemTime::now(),
                    };
                }

                return Ok(outcome);
            }
            Ok(Err(join_error)) => {
                warn!(
                    "Re-execution task panicked for dispute {}: {}",
                    hex::encode(dispute_id),
                    join_error
                );

                let mediator = {
                    let dispute = self.disputes.get(&dispute_id).unwrap();
                    self.assign_mediator(dispute)?
                };

                let outcome = DisputeOutcome::Inconclusive {
                    reason: format!("Re-execution task panicked: {join_error}"),
                    mediator_assigned: mediator,
                };

                {
                    let dispute = self.disputes.get_mut(&dispute_id).unwrap();
                    dispute.status = DisputeStatus::Resolved {
                        outcome: outcome.clone(),
                        resolved_at: SystemTime::now(),
                    };
                }

                return Ok(outcome);
            }
            Err(_elapsed) => {
                // Timeout occurred - mark as inconclusive and assign mediator
                warn!(
                    "Re-execution timed out after {:?} for dispute {}",
                    timeout_duration,
                    hex::encode(dispute_id)
                );

                let mediator = {
                    let dispute = self.disputes.get(&dispute_id).unwrap();
                    self.assign_mediator(dispute)?
                };

                let outcome = DisputeOutcome::Inconclusive {
                    reason: format!(
                        "Re-execution timed out after {}s",
                        timeout_duration.as_secs()
                    ),
                    mediator_assigned: mediator,
                };

                {
                    let dispute = self.disputes.get_mut(&dispute_id).unwrap();
                    dispute.status = DisputeStatus::Resolved {
                        outcome: outcome.clone(),
                        resolved_at: SystemTime::now(),
                    };
                }

                return Ok(outcome);
            }
        };

        // Compare results (we already extracted evidence_claimed_result earlier)
        let outcome = if re_execution_result == evidence_claimed_result {
            // Executor was correct
            DisputeOutcome::ExecutorCorrect {
                verified_result: re_execution_result.clone(),
            }
        } else {
            // Check if challenger's expected result matches re-execution
            let challenger_correct = {
                let dispute = self.disputes.get(&dispute_id).unwrap();
                match &dispute.evidence.reason {
                    DisputeReason::IncorrectResult { expected, .. } => {
                        *expected == re_execution_result
                    }
                    _ => false,
                }
            };

            if challenger_correct {
                // Submitter was correct, executor was wrong - record misbehavior
                if let Some(ref callback) = self.misbehavior_callback {
                    let evidence = format!(
                        "Executor {executor} provided incorrect result. Expected: {re_execution_result:?}, Got: {evidence_claimed_result:?}"
                    )
                    .into_bytes();
                    callback(&executor, task_hash, evidence);
                }
                DisputeOutcome::SubmitterCorrect {
                    verified_result: re_execution_result.clone(),
                }
            } else {
                // Both were wrong - record misbehavior for executor
                if let Some(ref callback) = self.misbehavior_callback {
                    let evidence = format!(
                        "Executor {executor} provided incorrect result. Expected: {re_execution_result:?}, Got: {evidence_claimed_result:?}"
                    )
                    .into_bytes();
                    callback(&executor, task_hash, evidence);
                }
                DisputeOutcome::BothWrong {
                    correct_result: re_execution_result.clone(),
                }
            }
        };

        info!(
            "Dispute {} resolved: {:?}",
            hex::encode(dispute_id),
            outcome
        );

        // Update dispute status
        let dispute_for_persist = {
            let dispute = self.disputes.get_mut(&dispute_id).unwrap();
            dispute.status = DisputeStatus::Resolved {
                outcome: outcome.clone(),
                resolved_at: SystemTime::now(),
            };
            dispute.updated_at = SystemTime::now();
            dispute.clone()
        };

        self.persist_dispute(&dispute_for_persist)?;

        Ok(outcome)
    }

    /// Assign a mediator to a dispute using trust-weighted selection
    ///
    /// Selection criteria:
    /// 1. Filter by conflict-of-interest (mediator != executor or challenger)
    /// 2. Filter by minimum trust threshold
    /// 3. Weight by trust score (higher trust = more likely)
    /// 4. Consider workload (fewer active disputes = preferred)
    fn assign_mediator(&self, dispute: &Dispute) -> Result<Option<Did>> {
        if !self.config.auto_assign_mediators {
            return Ok(None);
        }

        if self.mediator_pool.is_empty() {
            warn!("No mediators available in pool");
            return Ok(None);
        }

        // Get eligible mediators (filter by conflict-of-interest)
        let eligible: Vec<&MediatorInfo> = self
            .mediator_pool
            .iter()
            .filter(|m| !self.has_conflict_of_interest(&m.did, dispute))
            .collect();

        if eligible.is_empty() {
            warn!(
                "No eligible mediators (all have conflicts of interest) for dispute {}",
                hex::encode(dispute.dispute_id)
            );
            return Ok(None);
        }

        // Score each eligible mediator
        let mut scored_mediators: Vec<(&MediatorInfo, f64)> = eligible
            .iter()
            .filter_map(|m| {
                let score = self.calculate_mediator_score(m, dispute);
                if score > 0.0 {
                    Some((*m, score))
                } else {
                    None
                }
            })
            .collect();

        if scored_mediators.is_empty() {
            warn!(
                "No mediators meet minimum trust threshold ({}) for dispute {}",
                self.config.mediator_min_trust,
                hex::encode(dispute.dispute_id)
            );
            return Ok(None);
        }

        // Sort by score descending
        scored_mediators.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Select the top-scored mediator
        let selected = &scored_mediators[0].0;
        let score = scored_mediators[0].1;

        info!(
            "Assigned mediator {} (score: {:.3}) to dispute {}",
            selected.did,
            score,
            hex::encode(dispute.dispute_id)
        );

        Ok(Some(selected.did.clone()))
    }

    /// Check if a mediator has a conflict of interest with a dispute
    fn has_conflict_of_interest(&self, mediator: &Did, dispute: &Dispute) -> bool {
        // Mediator cannot be the executor or challenger
        mediator == &dispute.executor || mediator == &dispute.challenger
    }

    /// Calculate a mediator's score for a dispute
    ///
    /// Score = trust_score * (1 - workload_penalty)
    /// - trust_score: from trust graph (0.0 to 1.0)
    /// - workload_penalty: based on active dispute count (0.0 to 0.5)
    fn calculate_mediator_score(&self, mediator: &MediatorInfo, _dispute: &Dispute) -> f64 {
        // Get trust score (default to 0.5 if no callback)
        let trust_score = self
            .trust_callback
            .as_ref()
            .map(|cb| cb(&mediator.did.to_string()))
            .unwrap_or(0.5);

        // Filter by minimum trust threshold
        if trust_score < self.config.mediator_min_trust {
            return 0.0;
        }

        // Calculate workload (count active disputes for this mediator)
        let active_disputes = self.get_mediator_workload(&mediator.did);

        // Workload penalty: 0.1 per active dispute, max 0.5
        let workload_penalty = (active_disputes as f64 * 0.1).min(0.5);

        // Final score
        let score = trust_score * (1.0 - workload_penalty);

        score
    }

    /// Get the number of active disputes assigned to a mediator
    fn get_mediator_workload(&self, mediator: &Did) -> usize {
        self.disputes
            .values()
            .filter(|d| match &d.status {
                DisputeStatus::UnderMediation { mediator: m, .. } => m == mediator,
                _ => false,
            })
            .count()
    }

    /// Add a mediator to the pool
    pub fn add_mediator(&mut self, mediator: Did) {
        self.add_mediator_with_expertise(mediator, vec![]);
    }

    /// Add a mediator to the pool with expertise tags
    pub fn add_mediator_with_expertise(&mut self, mediator: Did, expertise_tags: Vec<String>) {
        if !self.mediator_pool.iter().any(|m| m.did == mediator) {
            info!(
                "Adding mediator {} to pool with expertise: {:?}",
                mediator, expertise_tags
            );
            self.mediator_pool.push(MediatorInfo {
                did: mediator,
                expertise_tags,
            });
        }
    }

    /// Remove a mediator from the pool
    pub fn remove_mediator(&mut self, mediator: &Did) {
        self.mediator_pool.retain(|m| &m.did != mediator);
        info!("Removed mediator {} from pool", mediator);
    }

    /// Get a dispute by ID
    pub fn get_dispute(&self, dispute_id: &DisputeId) -> Option<&Dispute> {
        self.disputes.get(dispute_id)
    }

    /// Get all disputes for a specific executor
    pub fn get_disputes_by_executor(&self, executor: &Did) -> Vec<&Dispute> {
        self.disputes
            .values()
            .filter(|d| &d.executor == executor)
            .collect()
    }

    /// Get all disputes filed by a challenger
    pub fn get_disputes_by_challenger(&self, challenger: &Did) -> Vec<&Dispute> {
        self.disputes
            .values()
            .filter(|d| &d.challenger == challenger)
            .collect()
    }

    /// Get dispute statistics
    pub fn get_stats(&self) -> DisputeStats {
        let mut stats = DisputeStats {
            total_disputes: self.disputes.len(),
            pending: 0,
            investigating: 0,
            resolved_auto: 0,
            under_mediation: 0,
            closed: 0,
            submitter_correct: 0,
            executor_correct: 0,
            both_wrong: 0,
            inconclusive: 0,
        };

        for dispute in self.disputes.values() {
            match &dispute.status {
                DisputeStatus::Pending => stats.pending += 1,
                DisputeStatus::Investigating => stats.investigating += 1,
                DisputeStatus::Resolved { outcome, .. } => {
                    stats.resolved_auto += 1;
                    match outcome {
                        DisputeOutcome::SubmitterCorrect { .. } => stats.submitter_correct += 1,
                        DisputeOutcome::ExecutorCorrect { .. } => stats.executor_correct += 1,
                        DisputeOutcome::BothWrong { .. } => stats.both_wrong += 1,
                        DisputeOutcome::Inconclusive { .. } => stats.inconclusive += 1,
                    }
                }
                DisputeStatus::UnderMediation { .. } => stats.under_mediation += 1,
                DisputeStatus::Closed { outcome, .. } => {
                    stats.closed += 1;
                    match outcome {
                        DisputeOutcome::SubmitterCorrect { .. } => stats.submitter_correct += 1,
                        DisputeOutcome::ExecutorCorrect { .. } => stats.executor_correct += 1,
                        DisputeOutcome::BothWrong { .. } => stats.both_wrong += 1,
                        DisputeOutcome::Inconclusive { .. } => stats.inconclusive += 1,
                    }
                }
            }
        }

        stats
    }

    /// Generate a dispute ID from task hash and challenger
    fn generate_dispute_id(&self, task_hash: &ContentHash, challenger: &Did) -> DisputeId {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(task_hash);
        hasher.update(challenger.to_string().as_bytes());
        hasher.update(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .to_le_bytes(),
        );

        let result = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&result);
        id
    }

    /// Persist a dispute to storage
    fn persist_dispute(&self, dispute: &Dispute) -> Result<()> {
        let key = format!("dispute:{}", hex::encode(dispute.dispute_id));
        let value = serde_json::to_vec(dispute)?;
        self.dispute_store.put(key.as_bytes(), &value)?;
        Ok(())
    }
}

/// Statistics about disputes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisputeStats {
    pub total_disputes: usize,
    pub pending: usize,
    pub investigating: usize,
    pub resolved_auto: usize,
    pub under_mediation: usize,
    pub closed: usize,
    pub submitter_correct: usize,
    pub executor_correct: usize,
    pub both_wrong: usize,
    pub inconclusive: usize,
}

/// Message type for DisputeActor operations
pub enum DisputeActorMsg {
    /// File a new dispute
    FileDispute {
        task_hash: ContentHash,
        executor: Did,
        challenger: Did,
        evidence: DisputeEvidence,
        reply: tokio::sync::oneshot::Sender<Result<DisputeId>>,
    },
    /// Investigate a dispute (re-execute contract)
    InvestigateDispute {
        dispute_id: DisputeId,
        contract: Contract,
        rule_name: String,
        args: HashMap<String, Value>,
        reply: tokio::sync::oneshot::Sender<Result<DisputeOutcome>>,
    },
    /// Get dispute by ID
    GetDispute {
        dispute_id: DisputeId,
        reply: tokio::sync::oneshot::Sender<Option<Dispute>>,
    },
    /// Get statistics
    GetStats {
        reply: tokio::sync::oneshot::Sender<DisputeStats>,
    },
    /// Add mediator to pool
    AddMediator {
        mediator: Did,
    },
    /// Remove mediator from pool
    RemoveMediator {
        mediator: Did,
    },
    /// Set misbehavior callback
    SetMisbehaviorCallback {
        callback: MisbehaviorCallback,
    },
    /// Set trust callback for mediator scoring
    SetTrustCallback {
        callback: TrustCallback,
    },
    /// Add mediator with expertise tags
    AddMediatorWithExpertise {
        mediator: Did,
        expertise_tags: Vec<String>,
    },
}

impl std::fmt::Debug for DisputeActorMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileDispute {
                task_hash,
                executor,
                challenger,
                evidence,
                ..
            } => f
                .debug_struct("FileDispute")
                .field("task_hash", task_hash)
                .field("executor", executor)
                .field("challenger", challenger)
                .field("evidence", evidence)
                .finish(),
            Self::InvestigateDispute {
                dispute_id,
                contract,
                rule_name,
                args,
                ..
            } => f
                .debug_struct("InvestigateDispute")
                .field("dispute_id", dispute_id)
                .field("contract", contract)
                .field("rule_name", rule_name)
                .field("args", args)
                .finish(),
            Self::GetDispute { dispute_id, .. } => f
                .debug_struct("GetDispute")
                .field("dispute_id", dispute_id)
                .finish(),
            Self::GetStats { .. } => f.debug_struct("GetStats").finish(),
            Self::AddMediator { mediator } => f
                .debug_struct("AddMediator")
                .field("mediator", mediator)
                .finish(),
            Self::RemoveMediator { mediator } => f
                .debug_struct("RemoveMediator")
                .field("mediator", mediator)
                .finish(),
            Self::SetMisbehaviorCallback { .. } => {
                f.debug_struct("SetMisbehaviorCallback").finish()
            }
            Self::SetTrustCallback { .. } => {
                f.debug_struct("SetTrustCallback").finish()
            }
            Self::AddMediatorWithExpertise {
                mediator,
                expertise_tags,
            } => f
                .debug_struct("AddMediatorWithExpertise")
                .field("mediator", mediator)
                .field("expertise_tags", expertise_tags)
                .finish(),
        }
    }
}

/// Handle for interacting with the DisputeActor
#[derive(Clone)]
pub struct DisputeActorHandle {
    tx: tokio::sync::mpsc::Sender<DisputeActorMsg>,
}

impl DisputeActorHandle {
    /// File a dispute for a compute task result
    pub async fn file_dispute(
        &self,
        task_hash: ContentHash,
        executor: Did,
        challenger: Did,
        evidence: DisputeEvidence,
    ) -> Result<DisputeId> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(DisputeActorMsg::FileDispute {
                task_hash,
                executor,
                challenger,
                evidence,
                reply,
            })
            .await
            .map_err(|_| anyhow!("DisputeActor channel closed"))?;
        rx.await.map_err(|_| anyhow!("Reply channel closed"))?
    }

    /// Investigate a dispute by re-executing the contract
    pub async fn investigate_dispute(
        &self,
        dispute_id: DisputeId,
        contract: Contract,
        rule_name: String,
        args: HashMap<String, Value>,
    ) -> Result<DisputeOutcome> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(DisputeActorMsg::InvestigateDispute {
                dispute_id,
                contract,
                rule_name,
                args,
                reply,
            })
            .await
            .map_err(|_| anyhow!("DisputeActor channel closed"))?;
        rx.await.map_err(|_| anyhow!("Reply channel closed"))?
    }

    /// Get a dispute by ID
    pub async fn get_dispute(&self, dispute_id: DisputeId) -> Option<Dispute> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(DisputeActorMsg::GetDispute { dispute_id, reply })
            .await
            .ok()?;
        rx.await.ok()?
    }

    /// Get dispute statistics
    pub async fn get_stats(&self) -> DisputeStats {
        let (reply, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(DisputeActorMsg::GetStats { reply }).await.is_ok() {
            rx.await.unwrap_or_default()
        } else {
            DisputeStats::default()
        }
    }

    /// Add a mediator to the pool
    pub async fn add_mediator(&self, mediator: Did) {
        let _ = self.tx.send(DisputeActorMsg::AddMediator { mediator }).await;
    }

    /// Remove a mediator from the pool
    pub async fn remove_mediator(&self, mediator: Did) {
        let _ = self
            .tx
            .send(DisputeActorMsg::RemoveMediator { mediator })
            .await;
    }

    /// Set misbehavior callback for recording violations
    pub async fn set_misbehavior_callback(&self, callback: MisbehaviorCallback) {
        let _ = self
            .tx
            .send(DisputeActorMsg::SetMisbehaviorCallback { callback })
            .await;
    }

    /// Set trust callback for mediator scoring
    pub async fn set_trust_callback(&self, callback: TrustCallback) {
        let _ = self
            .tx
            .send(DisputeActorMsg::SetTrustCallback { callback })
            .await;
    }

    /// Add a mediator with expertise tags
    pub async fn add_mediator_with_expertise(&self, mediator: Did, expertise_tags: Vec<String>) {
        let _ = self
            .tx
            .send(DisputeActorMsg::AddMediatorWithExpertise {
                mediator,
                expertise_tags,
            })
            .await;
    }
}

/// DisputeActor wraps DisputeResolutionSystem for integration with supervisor
pub struct DisputeActor {
    system: DisputeResolutionSystem,
    rx: tokio::sync::mpsc::Receiver<DisputeActorMsg>,
}

impl DisputeActor {
    /// Spawn a new DisputeActor and return its handle
    pub fn spawn(config: DisputeConfig, dispute_store: Arc<dyn Store>) -> DisputeActorHandle {
        let system = DisputeResolutionSystem::new(config, dispute_store);
        Self::spawn_with_system(system)
    }

    /// Spawn a DisputeActor with a shared DisputeResolutionSystem
    ///
    /// This allows sharing the dispute system with other components (e.g., ComputeActor)
    /// while still having the actor manage the message processing loop.
    pub fn spawn_with_system(system: DisputeResolutionSystem) -> DisputeActorHandle {
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let actor = DisputeActor { system, rx };

        tokio::spawn(async move {
            actor.run().await;
        });

        DisputeActorHandle { tx }
    }

    /// Create a shared DisputeResolutionSystem for use with ComputeActor
    ///
    /// Returns both the shared system (for ComputeActor) and the actor handle.
    /// The system is wrapped in Arc<RwLock> so ComputeActor can access it directly.
    pub fn spawn_shared(
        config: DisputeConfig,
        dispute_store: Arc<dyn Store>,
    ) -> (
        Arc<tokio::sync::RwLock<DisputeResolutionSystem>>,
        DisputeActorHandle,
    ) {
        let system = Arc::new(tokio::sync::RwLock::new(DisputeResolutionSystem::new(
            config,
            dispute_store,
        )));

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Clone the Arc for the actor
        let actor_system = system.clone();

        tokio::spawn(async move {
            Self::run_shared(actor_system, rx).await;
        });

        (system, DisputeActorHandle { tx })
    }

    /// Run loop for shared system variant
    async fn run_shared(
        system: Arc<tokio::sync::RwLock<DisputeResolutionSystem>>,
        mut rx: tokio::sync::mpsc::Receiver<DisputeActorMsg>,
    ) {
        info!("DisputeActor started (shared mode)");

        while let Some(msg) = rx.recv().await {
            match msg {
                DisputeActorMsg::FileDispute {
                    task_hash,
                    executor,
                    challenger,
                    evidence,
                    reply,
                } => {
                    let mut sys = system.write().await;
                    let result = sys.file_dispute(task_hash, executor, challenger, evidence).await;
                    let _ = reply.send(result);
                }
                DisputeActorMsg::InvestigateDispute {
                    dispute_id,
                    contract,
                    rule_name,
                    args,
                    reply,
                } => {
                    let mut sys = system.write().await;
                    let result = sys.investigate_dispute(dispute_id, &contract, &rule_name, args).await;
                    let _ = reply.send(result);
                }
                DisputeActorMsg::GetDispute { dispute_id, reply } => {
                    let sys = system.read().await;
                    let _ = reply.send(sys.get_dispute(&dispute_id).cloned());
                }
                DisputeActorMsg::GetStats { reply } => {
                    let sys = system.read().await;
                    let _ = reply.send(sys.get_stats());
                }
                DisputeActorMsg::AddMediator { mediator } => {
                    let mut sys = system.write().await;
                    sys.add_mediator(mediator);
                }
                DisputeActorMsg::RemoveMediator { mediator } => {
                    let mut sys = system.write().await;
                    sys.remove_mediator(&mediator);
                }
                DisputeActorMsg::SetMisbehaviorCallback { callback } => {
                    let mut sys = system.write().await;
                    sys.set_misbehavior_callback(callback);
                }
                DisputeActorMsg::SetTrustCallback { callback } => {
                    let mut sys = system.write().await;
                    sys.set_trust_callback(callback);
                }
                DisputeActorMsg::AddMediatorWithExpertise {
                    mediator,
                    expertise_tags,
                } => {
                    let mut sys = system.write().await;
                    sys.add_mediator_with_expertise(mediator, expertise_tags);
                }
            }
        }

        info!("DisputeActor stopped (shared mode)");
    }

    /// Main actor loop
    async fn run(mut self) {
        info!("DisputeActor started");

        while let Some(msg) = self.rx.recv().await {
            match msg {
                DisputeActorMsg::FileDispute {
                    task_hash,
                    executor,
                    challenger,
                    evidence,
                    reply,
                } => {
                    let result = self
                        .system
                        .file_dispute(task_hash, executor, challenger, evidence)
                        .await;
                    let _ = reply.send(result);
                }
                DisputeActorMsg::InvestigateDispute {
                    dispute_id,
                    contract,
                    rule_name,
                    args,
                    reply,
                } => {
                    let result = self
                        .system
                        .investigate_dispute(dispute_id, &contract, &rule_name, args)
                        .await;
                    let _ = reply.send(result);
                }
                DisputeActorMsg::GetDispute { dispute_id, reply } => {
                    let dispute = self.system.get_dispute(&dispute_id).cloned();
                    let _ = reply.send(dispute);
                }
                DisputeActorMsg::GetStats { reply } => {
                    let stats = self.system.get_stats();
                    let _ = reply.send(stats);
                }
                DisputeActorMsg::AddMediator { mediator } => {
                    self.system.add_mediator(mediator);
                }
                DisputeActorMsg::RemoveMediator { mediator } => {
                    self.system.remove_mediator(&mediator);
                }
                DisputeActorMsg::SetMisbehaviorCallback { callback } => {
                    self.system.set_misbehavior_callback(callback);
                }
                DisputeActorMsg::SetTrustCallback { callback } => {
                    self.system.set_trust_callback(callback);
                }
                DisputeActorMsg::AddMediatorWithExpertise {
                    mediator,
                    expertise_tags,
                } => {
                    self.system.add_mediator_with_expertise(mediator, expertise_tags);
                }
            }
        }

        info!("DisputeActor stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Expr, Rule, Stmt};
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn make_test_contract() -> Contract {
        use crate::ast::Param;
        use crate::BinOp;

        // Simple contract that adds two numbers
        let add_rule = Rule {
            name: "add".to_string(),
            params: vec![
                Param {
                    name: "a".to_string(),
                },
                Param {
                    name: "b".to_string(),
                },
            ],
            requires: vec![],
            body: vec![Stmt::Return {
                value: Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Var("a".to_string())),
                    right: Box::new(Expr::Var("b".to_string())),
                },
            }],
        };

        Contract {
            name: "test_add".to_string(),
            participants: vec![],
            currency: None,
            state_vars: vec![],
            rules: vec![add_rule],
            triggers: vec![],
        }
    }

    #[tokio::test]
    async fn test_file_dispute() {
        let config = DisputeConfig::default();
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        let task_hash = [1u8; 32];
        let executor = make_test_did();
        let challenger = make_test_did();

        let evidence = DisputeEvidence {
            task_hash,
            claimed_result: Value::Int(10),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(5),
                actual: Value::Int(10),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id = system
            .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
            .await
            .unwrap();

        // Verify dispute was created
        let dispute = system.get_dispute(&dispute_id).unwrap();
        assert_eq!(dispute.executor, executor);
        assert_eq!(dispute.challenger, challenger);
        assert!(matches!(dispute.status, DisputeStatus::Pending));
    }

    #[tokio::test]
    async fn test_investigate_dispute_executor_correct() {
        let config = DisputeConfig::default();
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        let contract = make_test_contract();
        let task_hash = [2u8; 32];
        let executor = make_test_did();
        let challenger = make_test_did();

        // Executor claimed result of 5 (which is correct for 2+3)
        let evidence = DisputeEvidence {
            task_hash,
            claimed_result: Value::Int(5),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4), // Challenger thinks it should be 4
                actual: Value::Int(5),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id = system
            .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
            .await
            .unwrap();

        // Investigate with correct inputs (2+3=5)
        let mut args = std::collections::HashMap::new();
        args.insert("a".to_string(), Value::Int(2));
        args.insert("b".to_string(), Value::Int(3));

        let outcome = system
            .investigate_dispute(dispute_id, &contract, "add", args)
            .await
            .unwrap();

        // Executor was correct
        assert!(matches!(outcome, DisputeOutcome::ExecutorCorrect { .. }));
    }

    #[tokio::test]
    async fn test_investigate_dispute_both_wrong() {
        let config = DisputeConfig::default();
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        let contract = make_test_contract();
        let task_hash = [3u8; 32];
        let executor = make_test_did();
        let challenger = make_test_did();

        // Executor claimed result of 10 (incorrect for 2+3)
        let evidence = DisputeEvidence {
            task_hash,
            claimed_result: Value::Int(10),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4), // Challenger also wrong
                actual: Value::Int(10),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id = system
            .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
            .await
            .unwrap();

        // Investigate with correct inputs (2+3=5)
        let mut args = std::collections::HashMap::new();
        args.insert("a".to_string(), Value::Int(2));
        args.insert("b".to_string(), Value::Int(3));

        let outcome = system
            .investigate_dispute(dispute_id, &contract, "add", args)
            .await
            .unwrap();

        // Both were wrong, correct answer is 5
        match outcome {
            DisputeOutcome::BothWrong { correct_result } => {
                assert_eq!(correct_result, Value::Int(5));
            }
            _ => panic!("Expected BothWrong outcome"),
        }
    }

    #[test]
    fn test_mediator_management() {
        let config = DisputeConfig::default();
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        let mediator1 = make_test_did();
        let mediator2 = make_test_did();

        // Add mediators
        system.add_mediator(mediator1.clone());
        system.add_mediator(mediator2.clone());

        assert_eq!(system.mediator_pool.len(), 2);

        // Remove mediator
        system.remove_mediator(&mediator1);
        assert_eq!(system.mediator_pool.len(), 1);
        assert!(system.mediator_pool.iter().any(|m| m.did == mediator2));
    }

    #[tokio::test]
    async fn test_dispute_stats() {
        let config = DisputeConfig::default();
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        let contract = make_test_contract();

        // File multiple disputes
        for i in 0..3 {
            let task_hash = [i; 32];
            let executor = make_test_did();
            let challenger = make_test_did();

            let evidence = DisputeEvidence {
                task_hash,
                claimed_result: Value::Int(5),
                reason: DisputeReason::IncorrectResult {
                    expected: Value::Int(4),
                    actual: Value::Int(5),
                },
                additional_data: vec![],
                filed_at: SystemTime::now(),
            };

            let dispute_id = system
                .file_dispute(task_hash, executor, challenger, evidence)
                .await
                .unwrap();

            // Investigate first two
            if i < 2 {
                let mut args = std::collections::HashMap::new();
                args.insert("a".to_string(), Value::Int(2));
                args.insert("b".to_string(), Value::Int(3));

                let _ = system
                    .investigate_dispute(dispute_id, &contract, "add", args)
                    .await;
            }
        }

        let stats = system.get_stats();
        assert_eq!(stats.total_disputes, 3);
        assert_eq!(stats.resolved_auto, 2);
        assert_eq!(stats.pending, 1);
    }

    #[tokio::test]
    async fn test_investigate_dispute_timeout() {
        // Use a very short timeout (1ms) to trigger timeout behavior
        let config = DisputeConfig {
            re_execution_timeout: Duration::from_millis(1),
            auto_assign_mediators: true,
            mediator_min_trust: 0.7,
        };
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        // Add a mediator so assignment can succeed
        let mediator = make_test_did();
        system.add_mediator(mediator.clone());

        let contract = make_test_contract();
        let task_hash = [99u8; 32];
        let executor = make_test_did();
        let challenger = make_test_did();

        let evidence = DisputeEvidence {
            task_hash,
            claimed_result: Value::Int(5),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4),
                actual: Value::Int(5),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id = system
            .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
            .await
            .unwrap();

        let mut args = std::collections::HashMap::new();
        args.insert("a".to_string(), Value::Int(2));
        args.insert("b".to_string(), Value::Int(3));

        let outcome = system
            .investigate_dispute(dispute_id, &contract, "add", args)
            .await
            .unwrap();

        // With such a short timeout, this may or may not timeout depending on scheduling.
        // Accept either a successful resolution OR an Inconclusive timeout.
        match outcome {
            DisputeOutcome::Inconclusive { reason, .. } => {
                // If it timed out, verify the reason mentions timeout
                assert!(
                    reason.contains("timed out") || reason.contains("failed") || reason.contains("panicked"),
                    "Expected timeout/failure reason, got: {}",
                    reason
                );
            }
            DisputeOutcome::ExecutorCorrect { .. } => {
                // Contract executed fast enough - this is also valid
            }
            other => {
                panic!("Unexpected outcome: {:?}", other);
            }
        }
    }

    #[tokio::test]
    async fn test_trust_weighted_mediator_selection() {
        let config = DisputeConfig {
            auto_assign_mediators: true,
            mediator_min_trust: 0.5, // Lower threshold for test
            ..Default::default()
        };
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        // Add three mediators with different trust scores
        let low_trust_mediator = make_test_did();
        let mid_trust_mediator = make_test_did();
        let high_trust_mediator = make_test_did();

        system.add_mediator(low_trust_mediator.clone());
        system.add_mediator(mid_trust_mediator.clone());
        system.add_mediator(high_trust_mediator.clone());

        // Set up trust callback that returns different scores
        let lt = low_trust_mediator.to_string();
        let mt = mid_trust_mediator.to_string();
        let ht = high_trust_mediator.to_string();
        let trust_cb: TrustCallback = Arc::new(move |did: &str| {
            if did == lt {
                0.6
            } else if did == mt {
                0.75
            } else if did == ht {
                0.9
            } else {
                0.5
            }
        });
        system.set_trust_callback(trust_cb);

        // Create a dispute with parties NOT in the mediator pool
        let executor = make_test_did();
        let challenger = make_test_did();
        let task_hash = [10u8; 32];

        let evidence = DisputeEvidence {
            task_hash,
            claimed_result: Value::Int(5),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4),
                actual: Value::Int(5),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id = system
            .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
            .await
            .unwrap();

        let dispute = system.get_dispute(&dispute_id).unwrap();

        // High trust mediator should be selected
        let selected = system.assign_mediator(dispute).unwrap();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap(), high_trust_mediator);
    }

    #[tokio::test]
    async fn test_conflict_of_interest_detection() {
        let config = DisputeConfig {
            auto_assign_mediators: true,
            mediator_min_trust: 0.3,
            ..Default::default()
        };
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        // Create parties
        let executor = make_test_did();
        let challenger = make_test_did();
        let neutral_mediator = make_test_did();

        // Add executor and challenger as mediators (conflict of interest)
        // plus one neutral mediator
        system.add_mediator(executor.clone());
        system.add_mediator(challenger.clone());
        system.add_mediator(neutral_mediator.clone());

        // Set up trust callback (all equal trust)
        let trust_cb: TrustCallback = Arc::new(|_| 0.8);
        system.set_trust_callback(trust_cb);

        let task_hash = [11u8; 32];
        let evidence = DisputeEvidence {
            task_hash,
            claimed_result: Value::Int(5),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4),
                actual: Value::Int(5),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id = system
            .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
            .await
            .unwrap();

        let dispute = system.get_dispute(&dispute_id).unwrap();

        // Only neutral_mediator should be selected (executor and challenger have conflict)
        let selected = system.assign_mediator(dispute).unwrap();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap(), neutral_mediator);
    }

    #[tokio::test]
    async fn test_all_mediators_have_conflict() {
        let config = DisputeConfig {
            auto_assign_mediators: true,
            mediator_min_trust: 0.3,
            ..Default::default()
        };
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        // Create parties
        let executor = make_test_did();
        let challenger = make_test_did();

        // Only add executor and challenger as mediators (all have conflict)
        system.add_mediator(executor.clone());
        system.add_mediator(challenger.clone());

        let trust_cb: TrustCallback = Arc::new(|_| 0.8);
        system.set_trust_callback(trust_cb);

        let task_hash = [12u8; 32];
        let evidence = DisputeEvidence {
            task_hash,
            claimed_result: Value::Int(5),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4),
                actual: Value::Int(5),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id = system
            .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
            .await
            .unwrap();

        let dispute = system.get_dispute(&dispute_id).unwrap();

        // No mediator should be selected (all have conflict)
        let selected = system.assign_mediator(dispute).unwrap();
        assert!(selected.is_none());
    }

    #[tokio::test]
    async fn test_mediator_below_trust_threshold() {
        let config = DisputeConfig {
            auto_assign_mediators: true,
            mediator_min_trust: 0.7, // High threshold
            ..Default::default()
        };
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        let low_trust_mediator = make_test_did();
        system.add_mediator(low_trust_mediator.clone());

        // Trust callback returns score below threshold
        let trust_cb: TrustCallback = Arc::new(|_| 0.5); // Below 0.7 threshold
        system.set_trust_callback(trust_cb);

        let executor = make_test_did();
        let challenger = make_test_did();
        let task_hash = [13u8; 32];

        let evidence = DisputeEvidence {
            task_hash,
            claimed_result: Value::Int(5),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4),
                actual: Value::Int(5),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id = system
            .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
            .await
            .unwrap();

        let dispute = system.get_dispute(&dispute_id).unwrap();

        // No mediator should be selected (below trust threshold)
        let selected = system.assign_mediator(dispute).unwrap();
        assert!(selected.is_none());
    }

    #[tokio::test]
    async fn test_mediator_workload_balancing() {
        let config = DisputeConfig {
            auto_assign_mediators: true,
            mediator_min_trust: 0.5,
            ..Default::default()
        };
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        let busy_mediator = make_test_did();
        let idle_mediator = make_test_did();

        system.add_mediator(busy_mediator.clone());
        system.add_mediator(idle_mediator.clone());

        // Equal trust for both
        let trust_cb: TrustCallback = Arc::new(|_| 0.8);
        system.set_trust_callback(trust_cb);

        // Create an existing dispute and assign to busy_mediator manually
        let executor1 = make_test_did();
        let challenger1 = make_test_did();
        let task_hash1 = [20u8; 32];

        let evidence1 = DisputeEvidence {
            task_hash: task_hash1,
            claimed_result: Value::Int(5),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4),
                actual: Value::Int(5),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id1 = system
            .file_dispute(task_hash1, executor1.clone(), challenger1.clone(), evidence1)
            .await
            .unwrap();

        // Manually set this dispute to be under mediation by busy_mediator
        {
            let dispute = system.disputes.get_mut(&dispute_id1).unwrap();
            dispute.status = DisputeStatus::UnderMediation {
                mediator: busy_mediator.clone(),
                assigned_at: SystemTime::now(),
            };
        }

        // Now create a new dispute
        let executor2 = make_test_did();
        let challenger2 = make_test_did();
        let task_hash2 = [21u8; 32];

        let evidence2 = DisputeEvidence {
            task_hash: task_hash2,
            claimed_result: Value::Int(5),
            reason: DisputeReason::IncorrectResult {
                expected: Value::Int(4),
                actual: Value::Int(5),
            },
            additional_data: vec![],
            filed_at: SystemTime::now(),
        };

        let dispute_id2 = system
            .file_dispute(task_hash2, executor2.clone(), challenger2.clone(), evidence2)
            .await
            .unwrap();

        let dispute = system.get_dispute(&dispute_id2).unwrap();

        // idle_mediator should be selected (lower workload)
        let selected = system.assign_mediator(dispute).unwrap();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap(), idle_mediator);
    }

    #[test]
    fn test_add_mediator_with_expertise() {
        let config = DisputeConfig::default();
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut system = DisputeResolutionSystem::new(config, store);

        let mediator = make_test_did();
        let expertise = vec!["finance".to_string(), "technical".to_string()];

        system.add_mediator_with_expertise(mediator.clone(), expertise.clone());

        assert_eq!(system.mediator_pool.len(), 1);
        assert_eq!(system.mediator_pool[0].did, mediator);
        assert_eq!(system.mediator_pool[0].expertise_tags, expertise);
    }
}
