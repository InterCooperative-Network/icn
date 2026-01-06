//! Result quorum verification for high-value compute tasks.
//!
//! This module implements multi-executor verification to prevent incorrect
//! or malicious results from single executors on valuable computations.
//!
//! # Task Value Classification
//!
//! Tasks are classified by estimated value:
//! - **Low**: Single executor sufficient
//! - **Medium**: 2-executor verification
//! - **High**: 3+ executor quorum
//!
//! # Verification Flow
//!
//! 1. Task submitted with `verification_config`
//! 2. Scheduler assigns multiple executors based on value
//! 3. Results collected from all executors
//! 4. Quorum determines canonical result
//! 5. Divergent results escalate to dispute
//!
//! # Configuration
//!
//! ```toml
//! [compute.verification]
//! low_value_threshold = 100      # Below: single executor
//! medium_value_threshold = 1000  # Below: 2 executors
//! high_value_quorum = 3          # Above medium: 3+ executors
//! consensus_threshold = 0.67     # 2/3 majority for consensus
//! ```

use crate::error::{ComputeError, Result};
use crate::types::{ComputeResult, TaskHash};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Task value classification for verification requirements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskValue {
    /// Low value - single executor sufficient
    #[default]
    Low,
    /// Medium value - 2-executor verification
    Medium,
    /// High value - 3+ executor quorum
    High,
    /// Critical value - maximum verification
    Critical,
}

impl TaskValue {
    /// Get the required executor count for this value level
    pub fn required_executors(&self) -> usize {
        match self {
            TaskValue::Low => 1,
            TaskValue::Medium => 2,
            TaskValue::High => 3,
            TaskValue::Critical => 5,
        }
    }

    /// Classify a task based on its estimated value in credits
    pub fn from_credits(credits: u64, config: &VerificationConfig) -> Self {
        if credits < config.low_value_threshold {
            TaskValue::Low
        } else if credits < config.medium_value_threshold {
            TaskValue::Medium
        } else if credits < config.high_value_threshold {
            TaskValue::High
        } else {
            TaskValue::Critical
        }
    }
}

/// Configuration for result verification thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Value threshold below which single executor is used (credits)
    pub low_value_threshold: u64,

    /// Value threshold below which 2 executors are used (credits)
    pub medium_value_threshold: u64,

    /// Value threshold above which max executors are used (credits)
    pub high_value_threshold: u64,

    /// Number of executors for high-value tasks
    pub high_value_quorum: usize,

    /// Minimum consensus percentage (0.0-1.0) for accepting results
    pub consensus_threshold: f64,

    /// Time window (ms) to collect results before evaluating quorum
    pub collection_window_ms: u64,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            low_value_threshold: 100,
            medium_value_threshold: 1000,
            high_value_threshold: 10000,
            high_value_quorum: 3,
            consensus_threshold: 0.67,    // 2/3 majority
            collection_window_ms: 30_000, // 30 seconds
        }
    }
}

impl VerificationConfig {
    /// Determine the required executor count for a given task value
    pub fn required_executors(&self, value: TaskValue) -> usize {
        match value {
            TaskValue::Low => 1,
            TaskValue::Medium => 2,
            TaskValue::High => self.high_value_quorum,
            TaskValue::Critical => self.high_value_quorum.max(5),
        }
    }
}

/// Per-task verification requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskVerification {
    /// Task value classification
    pub value: TaskValue,

    /// Required number of executors
    pub required_executors: usize,

    /// Consensus threshold for this task
    pub consensus_threshold: f64,

    /// Explicit executor DIDs (if pre-selected)
    pub assigned_executors: Vec<Did>,
}

impl TaskVerification {
    /// Create verification requirements from task value
    pub fn from_value(value: TaskValue, config: &VerificationConfig) -> Self {
        Self {
            value,
            required_executors: config.required_executors(value),
            consensus_threshold: config.consensus_threshold,
            assigned_executors: Vec::new(),
        }
    }

    /// Check if single executor is sufficient
    pub fn is_single_executor(&self) -> bool {
        self.required_executors == 1
    }
}

impl Default for TaskVerification {
    fn default() -> Self {
        Self {
            value: TaskValue::Low,
            required_executors: 1,
            consensus_threshold: 0.67,
            assigned_executors: Vec::new(),
        }
    }
}

/// Status of result collection for a multi-executor task
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuorumStatus {
    /// Still collecting results
    Collecting { received: usize, required: usize },
    /// Consensus reached
    Consensus {
        result_hash: [u8; 32],
        agreeing: usize,
        total: usize,
    },
    /// No consensus - results diverge
    Divergent { result_groups: usize, total: usize },
    /// Collection timed out
    Timeout { received: usize, required: usize },
}

/// Collected result from an executor
#[derive(Debug, Clone)]
pub struct CollectedResult {
    /// The compute result
    pub result: ComputeResult,
    /// Hash of the result output (for comparison)
    pub output_hash: [u8; 32],
    /// When the result was received
    pub received_at: u64,
}

impl CollectedResult {
    /// Create from a ComputeResult
    pub fn from_result(result: ComputeResult, received_at: u64) -> Self {
        let output_hash = match &result.outcome {
            crate::types::ExecutionOutcome::Success(data) => *blake3::hash(data).as_bytes(),
            crate::types::ExecutionOutcome::Failed(msg) => *blake3::hash(msg.as_bytes()).as_bytes(),
            crate::types::ExecutionOutcome::OutOfFuel => *blake3::hash(b"OUT_OF_FUEL").as_bytes(),
            crate::types::ExecutionOutcome::Timeout => *blake3::hash(b"TIMEOUT").as_bytes(),
        };

        Self {
            result,
            output_hash,
            received_at,
        }
    }
}

/// Aggregator for collecting and evaluating multi-executor results
#[derive(Debug)]
pub struct ResultAggregator {
    /// Task hash
    task_hash: TaskHash,
    /// Verification requirements
    verification: TaskVerification,
    /// Collected results by executor DID
    results: HashMap<String, CollectedResult>,
    /// When collection started (for debugging and timeout calculations)
    #[allow(dead_code)]
    started_at: u64,
    /// Collection deadline
    deadline: u64,
}

/// Default collection window in milliseconds (30 seconds)
pub const DEFAULT_COLLECTION_WINDOW_MS: u64 = 30_000;

impl ResultAggregator {
    /// Create a new result aggregator for a task
    ///
    /// # Arguments
    /// * `task_hash` - Hash of the task being verified
    /// * `verification` - Verification requirements for this task
    /// * `started_at` - Timestamp when collection started (milliseconds)
    /// * `collection_window_ms` - Time window to collect results (milliseconds)
    pub fn new(
        task_hash: TaskHash,
        verification: TaskVerification,
        started_at: u64,
        collection_window_ms: u64,
    ) -> Self {
        let deadline = started_at + collection_window_ms;

        Self {
            task_hash,
            verification,
            results: HashMap::new(),
            started_at,
            deadline,
        }
    }

    /// Create a new result aggregator with default collection window (30s)
    pub fn with_default_window(
        task_hash: TaskHash,
        verification: TaskVerification,
        started_at: u64,
    ) -> Self {
        Self::new(
            task_hash,
            verification,
            started_at,
            DEFAULT_COLLECTION_WINDOW_MS,
        )
    }

    /// Set custom collection deadline
    pub fn with_deadline(mut self, deadline: u64) -> Self {
        self.deadline = deadline;
        self
    }

    /// Add a result from an executor
    ///
    /// This method verifies:
    /// 1. Task hash matches the expected task
    /// 2. Ed25519 signature is valid for the claimed executor DID
    /// 3. No duplicate results from the same executor
    /// 4. Collection window has not expired
    pub fn add_result(&mut self, result: ComputeResult) -> Result<()> {
        // Verify task hash matches
        if result.task_hash != self.task_hash {
            return Err(ComputeError::InvalidResult(format!(
                "Result task hash mismatch: expected {}, got {}",
                hex::encode(self.task_hash),
                hex::encode(result.task_hash)
            )));
        }

        let executor = result.executor.clone();

        // CRITICAL: Verify Ed25519 signature before accepting result
        // This prevents malicious nodes from forging results claiming to be from other executors
        let executor_did = Did::from_str(&executor).map_err(|e| {
            ComputeError::InvalidResult(format!("Invalid executor DID '{executor}': {e}"))
        })?;
        result.verify_signature(&executor_did)?;

        let now = icn_time::current_timestamp_millis();

        // Check if we already have a result from this executor
        if self.results.contains_key(&executor) {
            return Err(ComputeError::InvalidResult(format!(
                "Duplicate result from executor {executor}"
            )));
        }

        // Check if collection window has passed
        if now > self.deadline {
            return Err(ComputeError::InvalidResult(
                "Result collection window has closed".into(),
            ));
        }

        let collected = CollectedResult::from_result(result, now);
        self.results.insert(executor, collected);

        Ok(())
    }

    /// Get current quorum status
    pub fn status(&self) -> QuorumStatus {
        let received = self.results.len();
        let required = self.verification.required_executors;

        // Still collecting
        if received < required {
            let now = icn_time::current_timestamp_millis();
            if now > self.deadline {
                return QuorumStatus::Timeout { received, required };
            }
            return QuorumStatus::Collecting { received, required };
        }

        // All results received - evaluate consensus
        self.evaluate_consensus()
    }

    /// Evaluate consensus among collected results
    fn evaluate_consensus(&self) -> QuorumStatus {
        // Group results by output hash
        let mut groups: HashMap<[u8; 32], Vec<&CollectedResult>> = HashMap::new();
        for result in self.results.values() {
            groups.entry(result.output_hash).or_default().push(result);
        }

        let total = self.results.len();
        let threshold = (total as f64 * self.verification.consensus_threshold).ceil() as usize;

        // Find majority group
        let (majority_hash, majority_group) = groups
            .iter()
            .max_by_key(|(_, group)| group.len())
            .map(|(hash, group)| (*hash, group.len()))
            .unwrap_or(([0u8; 32], 0));

        // Check if majority meets threshold
        if majority_group >= threshold {
            QuorumStatus::Consensus {
                result_hash: majority_hash,
                agreeing: majority_group,
                total,
            }
        } else {
            QuorumStatus::Divergent {
                result_groups: groups.len(),
                total,
            }
        }
    }

    /// Get the canonical result (if consensus reached)
    ///
    /// This method evaluates consensus and returns the canonical result in one pass,
    /// avoiding redundant computation compared to calling status() then looking up the result.
    pub fn canonical_result(&self) -> Option<&ComputeResult> {
        let received = self.results.len();
        let required = self.verification.required_executors;

        // Not enough results yet
        if received < required {
            return None;
        }

        // Find majority group directly
        let mut groups: HashMap<[u8; 32], Vec<&CollectedResult>> = HashMap::new();
        for result in self.results.values() {
            groups.entry(result.output_hash).or_default().push(result);
        }

        let total = self.results.len();
        let threshold = (total as f64 * self.verification.consensus_threshold).ceil() as usize;

        // Find and return canonical result if consensus met
        groups
            .iter()
            .find(|(_, group)| group.len() >= threshold)
            .and_then(|(_, group)| group.first())
            .map(|r| &r.result)
    }

    /// Get all collected results
    pub fn all_results(&self) -> impl Iterator<Item = &ComputeResult> {
        self.results.values().map(|r| &r.result)
    }

    /// Get executors grouped by their result
    pub fn executor_groups(&self) -> HashMap<[u8; 32], Vec<String>> {
        let mut groups: HashMap<[u8; 32], Vec<String>> = HashMap::new();
        for (executor, result) in &self.results {
            groups
                .entry(result.output_hash)
                .or_default()
                .push(executor.clone());
        }
        groups
    }
}

/// Manager for coordinating result quorum across multiple tasks
pub struct ResultQuorumManager {
    /// Active aggregators by task hash
    aggregators: Arc<RwLock<HashMap<TaskHash, ResultAggregator>>>,
    /// Verification configuration
    config: VerificationConfig,
}

impl ResultQuorumManager {
    /// Create a new result quorum manager
    pub fn new(config: VerificationConfig) -> Self {
        Self {
            aggregators: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Register a task for multi-executor verification
    pub fn register_task(&self, task_hash: TaskHash, verification: TaskVerification) -> Result<()> {
        let now = icn_time::current_timestamp_millis();

        let aggregator = ResultAggregator::new(
            task_hash,
            verification,
            now,
            self.config.collection_window_ms,
        );

        let mut aggregators = self
            .aggregators
            .write()
            .map_err(|_| ComputeError::LockError)?;

        if aggregators.contains_key(&task_hash) {
            return Err(ComputeError::InvalidResult(format!(
                "Task {} already registered for quorum verification",
                hex::encode(task_hash)
            )));
        }

        aggregators.insert(task_hash, aggregator);
        Ok(())
    }

    /// Submit a result for quorum evaluation
    pub fn submit_result(&self, result: ComputeResult) -> Result<QuorumStatus> {
        let task_hash = result.task_hash;

        let mut aggregators = self
            .aggregators
            .write()
            .map_err(|_| ComputeError::LockError)?;

        let aggregator = aggregators.get_mut(&task_hash).ok_or_else(|| {
            ComputeError::TaskNotFound(format!(
                "No quorum aggregator for task {}",
                hex::encode(task_hash)
            ))
        })?;

        aggregator.add_result(result)?;
        Ok(aggregator.status())
    }

    /// Get the status of a task's quorum
    pub fn get_status(&self, task_hash: &TaskHash) -> Result<QuorumStatus> {
        let aggregators = self
            .aggregators
            .read()
            .map_err(|_| ComputeError::LockError)?;

        let aggregator = aggregators.get(task_hash).ok_or_else(|| {
            ComputeError::TaskNotFound(format!(
                "No quorum aggregator for task {}",
                hex::encode(task_hash)
            ))
        })?;

        Ok(aggregator.status())
    }

    /// Get the canonical result if consensus reached
    pub fn get_canonical_result(&self, task_hash: &TaskHash) -> Result<Option<ComputeResult>> {
        let aggregators = self
            .aggregators
            .read()
            .map_err(|_| ComputeError::LockError)?;

        let aggregator = aggregators.get(task_hash).ok_or_else(|| {
            ComputeError::TaskNotFound(format!(
                "No quorum aggregator for task {}",
                hex::encode(task_hash)
            ))
        })?;

        Ok(aggregator.canonical_result().cloned())
    }

    /// Get executor groupings for a task (useful for dispute evidence)
    pub fn get_executor_groups(
        &self,
        task_hash: &TaskHash,
    ) -> Result<HashMap<[u8; 32], Vec<String>>> {
        let aggregators = self
            .aggregators
            .read()
            .map_err(|_| ComputeError::LockError)?;

        let aggregator = aggregators.get(task_hash).ok_or_else(|| {
            ComputeError::TaskNotFound(format!(
                "No quorum aggregator for task {}",
                hex::encode(task_hash)
            ))
        })?;

        Ok(aggregator.executor_groups())
    }

    /// Remove a completed task's aggregator
    pub fn remove_task(&self, task_hash: &TaskHash) -> Result<()> {
        let mut aggregators = self
            .aggregators
            .write()
            .map_err(|_| ComputeError::LockError)?;

        aggregators.remove(task_hash);
        Ok(())
    }

    /// Clean up timed-out aggregators
    pub fn cleanup_expired(&self) -> Result<Vec<TaskHash>> {
        let now = icn_time::current_timestamp_millis();

        let mut aggregators = self
            .aggregators
            .write()
            .map_err(|_| ComputeError::LockError)?;

        let expired: Vec<TaskHash> = aggregators
            .iter()
            .filter(|(_, agg)| now > agg.deadline)
            .map(|(hash, _)| *hash)
            .collect();

        for hash in &expired {
            aggregators.remove(hash);
        }

        Ok(expired)
    }

    /// Get the verification config
    pub fn config(&self) -> &VerificationConfig {
        &self.config
    }

    /// Classify a task value based on estimated credits
    pub fn classify_value(&self, estimated_credits: u64) -> TaskValue {
        TaskValue::from_credits(estimated_credits, &self.config)
    }

    /// Create verification requirements for a task
    pub fn create_verification(&self, value: TaskValue) -> TaskVerification {
        TaskVerification::from_value(value, &self.config)
    }
}

impl Default for ResultQuorumManager {
    fn default() -> Self {
        Self::new(VerificationConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExecutionOutcome;

    /// Test executor with keypair for signing results
    struct TestExecutor {
        keypair: icn_identity::KeyPair,
        did: String,
    }

    impl TestExecutor {
        fn new() -> Self {
            let keypair = icn_identity::KeyPair::generate().unwrap();
            let did = keypair.did().to_string();
            Self { keypair, did }
        }

        fn signed_result(&self, task_hash: TaskHash, output: &[u8]) -> ComputeResult {
            let mut result = ComputeResult {
                task_hash,
                task_id: "test-task".into(),
                executor: self.did.clone(),
                outcome: ExecutionOutcome::Success(output.to_vec()),
                fuel_used: 1000,
                duration_ms: 100,
                completed_at: icn_time::current_timestamp_millis(),
                signature: vec![],
            };
            // Sign with the executor's keypair
            let payload = result.signing_payload();
            let signature = self.keypair.sign(&payload);
            result.signature = signature.to_bytes().to_vec();
            result
        }
    }

    #[test]
    fn test_task_value_classification() {
        let config = VerificationConfig::default();

        assert_eq!(TaskValue::from_credits(50, &config), TaskValue::Low);
        assert_eq!(TaskValue::from_credits(500, &config), TaskValue::Medium);
        assert_eq!(TaskValue::from_credits(5000, &config), TaskValue::High);
        assert_eq!(TaskValue::from_credits(50000, &config), TaskValue::Critical);
    }

    #[test]
    fn test_required_executors() {
        assert_eq!(TaskValue::Low.required_executors(), 1);
        assert_eq!(TaskValue::Medium.required_executors(), 2);
        assert_eq!(TaskValue::High.required_executors(), 3);
        assert_eq!(TaskValue::Critical.required_executors(), 5);
    }

    #[test]
    fn test_single_executor_consensus() {
        let task_hash = [1u8; 32];
        let verification = TaskVerification {
            value: TaskValue::Low,
            required_executors: 1,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        aggregator
            .add_result(exec1.signed_result(task_hash, b"output"))
            .unwrap();

        assert!(matches!(
            aggregator.status(),
            QuorumStatus::Consensus { agreeing: 1, .. }
        ));
    }

    #[test]
    fn test_multi_executor_consensus() {
        let task_hash = [2u8; 32];
        let verification = TaskVerification {
            value: TaskValue::High,
            required_executors: 3,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();
        let exec3 = TestExecutor::new();

        // Add 3 agreeing results
        aggregator
            .add_result(exec1.signed_result(task_hash, b"same"))
            .unwrap();
        aggregator
            .add_result(exec2.signed_result(task_hash, b"same"))
            .unwrap();
        aggregator
            .add_result(exec3.signed_result(task_hash, b"same"))
            .unwrap();

        let status = aggregator.status();
        assert!(
            matches!(
                status,
                QuorumStatus::Consensus {
                    agreeing: 3,
                    total: 3,
                    ..
                }
            ),
            "Expected consensus with 3 agreeing, got: {status:?}"
        );
    }

    #[test]
    fn test_divergent_results() {
        let task_hash = [3u8; 32];
        let verification = TaskVerification {
            value: TaskValue::High,
            required_executors: 3,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();
        let exec3 = TestExecutor::new();

        // Add 3 different results - no consensus possible
        aggregator
            .add_result(exec1.signed_result(task_hash, b"output1"))
            .unwrap();
        aggregator
            .add_result(exec2.signed_result(task_hash, b"output2"))
            .unwrap();
        aggregator
            .add_result(exec3.signed_result(task_hash, b"output3"))
            .unwrap();

        let status = aggregator.status();
        assert!(
            matches!(
                status,
                QuorumStatus::Divergent {
                    result_groups: 3,
                    total: 3
                }
            ),
            "Expected divergent with 3 groups, got: {status:?}"
        );
    }

    #[test]
    fn test_majority_consensus() {
        let task_hash = [4u8; 32];
        let verification = TaskVerification {
            value: TaskValue::High,
            required_executors: 3,
            consensus_threshold: 0.50, // Simple majority: 2/3 > 50%
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();
        let exec3 = TestExecutor::new();

        // 2 agree, 1 differs - 2/3 = 66.7% meets 50% threshold
        aggregator
            .add_result(exec1.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec2.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec3.signed_result(task_hash, b"wrong"))
            .unwrap();

        let status = aggregator.status();
        assert!(
            matches!(
                status,
                QuorumStatus::Consensus {
                    agreeing: 2,
                    total: 3,
                    ..
                }
            ),
            "Expected consensus with 2/3, got: {status:?}"
        );
    }

    #[test]
    fn test_duplicate_executor_rejected() {
        let task_hash = [5u8; 32];
        let verification = TaskVerification {
            value: TaskValue::Medium,
            required_executors: 2,
            consensus_threshold: 0.67,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();

        // First submission succeeds
        aggregator
            .add_result(exec1.signed_result(task_hash, b"output"))
            .unwrap();

        // Second submission from same executor is rejected
        let result2 = exec1.signed_result(task_hash, b"different");
        assert!(aggregator.add_result(result2).is_err());
    }

    #[test]
    fn test_wrong_task_hash_rejected() {
        let task_hash = [6u8; 32];
        let wrong_hash = [7u8; 32];
        let verification = TaskVerification::default();

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        // Result signed for wrong task hash
        let result = exec1.signed_result(wrong_hash, b"output");
        assert!(aggregator.add_result(result).is_err());
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let task_hash = [10u8; 32];
        let verification = TaskVerification::default();

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();

        // Create result claiming to be from exec1 but signed by exec2
        let mut forged_result = ComputeResult {
            task_hash,
            task_id: "test-task".into(),
            executor: exec1.did.clone(), // Claims to be exec1
            outcome: ExecutionOutcome::Success(b"output".to_vec()),
            fuel_used: 1000,
            duration_ms: 100,
            completed_at: icn_time::current_timestamp_millis(),
            signature: vec![],
        };
        // Sign with exec2's key (wrong key)
        let payload = forged_result.signing_payload();
        let signature = exec2.keypair.sign(&payload);
        forged_result.signature = signature.to_bytes().to_vec();

        // Should be rejected due to signature verification failure
        let err = aggregator.add_result(forged_result).unwrap_err();
        assert!(
            err.to_string().contains("Signature verification failed"),
            "Expected signature verification error, got: {err}"
        );
    }

    #[test]
    fn test_result_quorum_manager() {
        let manager = ResultQuorumManager::default();
        let task_hash = [8u8; 32];

        // Classify and create verification
        let value = manager.classify_value(500); // Medium
        assert_eq!(value, TaskValue::Medium);

        let verification = manager.create_verification(value);
        assert_eq!(verification.required_executors, 2);

        // Register task
        manager.register_task(task_hash, verification).unwrap();

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();

        // Submit first result
        let result1 = exec1.signed_result(task_hash, b"output");
        let status = manager.submit_result(result1).unwrap();
        assert!(matches!(
            status,
            QuorumStatus::Collecting {
                received: 1,
                required: 2
            }
        ));

        // Submit second result (same output)
        let result2 = exec2.signed_result(task_hash, b"output");
        let status = manager.submit_result(result2).unwrap();
        assert!(matches!(
            status,
            QuorumStatus::Consensus {
                agreeing: 2,
                total: 2,
                ..
            }
        ));

        // Get canonical result
        let canonical = manager.get_canonical_result(&task_hash).unwrap();
        assert!(canonical.is_some());
    }

    #[test]
    fn test_executor_groups() {
        let task_hash = [9u8; 32];
        let verification = TaskVerification {
            value: TaskValue::High,
            required_executors: 4,
            consensus_threshold: 0.5,
            assigned_executors: vec![],
        };

        let mut aggregator = ResultAggregator::with_default_window(
            task_hash,
            verification,
            icn_time::current_timestamp_millis(),
        );

        let exec1 = TestExecutor::new();
        let exec2 = TestExecutor::new();
        let exec3 = TestExecutor::new();
        let exec4 = TestExecutor::new();

        // 2 groups: 3 correct, 1 wrong
        aggregator
            .add_result(exec1.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec2.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec3.signed_result(task_hash, b"correct"))
            .unwrap();
        aggregator
            .add_result(exec4.signed_result(task_hash, b"wrong"))
            .unwrap();

        let groups = aggregator.executor_groups();
        assert_eq!(groups.len(), 2);

        // Find the majority group (should have 3 executors)
        let majority_count = groups.values().map(|v| v.len()).max().unwrap();
        assert_eq!(majority_count, 3);
    }
}
