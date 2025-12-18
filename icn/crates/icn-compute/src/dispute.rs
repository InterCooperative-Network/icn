//! Dispute resolution for compute execution
//!
//! This module provides mechanisms for detecting and resolving disputes
//! when executors provide conflicting or incorrect results.
//!
//! # Verification Modes
//!
//! - **SingleExecutor**: Default, fastest, cheapest
//! - **MultiExecutor**: N executors, majority consensus
//! - **Optimistic**: One executor, challengeable within window
//!
//! # Dispute Workflow
//!
//! 1. Differing results detected → Create ComputeDispute
//! 2. Evidence collection (24h window)
//! 3. Re-execution by arbiter (high-trust node)
//! 4. Resolution → Pay correct executors, penalize incorrect ones

use crate::error::{ComputeError, Result};
use crate::types::TaskHash;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Verification mode for task execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "type")]
pub enum VerificationMode {
    /// Single executor (default - fastest, cheapest)
    #[default]
    SingleExecutor,

    /// Multiple executors with majority consensus
    MultiExecutor {
        /// Number of executors to use
        count: usize,
        /// Minimum percentage for consensus (0.0-1.0)
        #[serde(default = "default_consensus_threshold")]
        consensus_threshold: f64,
    },

    /// Optimistic execution with challenge window
    Optimistic {
        /// Challenge window duration (seconds)
        challenge_window_secs: u64,
    },
}

fn default_consensus_threshold() -> f64 {
    0.67 // 2/3 majority
}

/// Result of a compute task execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComputeResult {
    /// Task hash
    pub task_hash: TaskHash,
    /// Executor DID
    pub executor: Did,
    /// Result data (serialized)
    pub output: Vec<u8>,
    /// Fuel consumed
    pub fuel_consumed: u64,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Timestamp of execution
    pub executed_at: u64,
    /// Ed25519 signature over (task_hash || output || fuel_consumed || executed_at)
    pub signature: Vec<u8>,
}

impl ComputeResult {
    /// Create signing bytes for this result
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.task_hash);
        bytes.extend_from_slice(&self.output);
        bytes.extend_from_slice(&self.fuel_consumed.to_le_bytes());
        bytes.extend_from_slice(&self.executed_at.to_le_bytes());
        bytes
    }

    /// Sign this result with the given keypair
    pub fn sign(mut self, keypair: &icn_identity::KeyPair) -> Self {
        let bytes = self.signing_bytes();
        let signature = keypair.sign(&bytes);
        self.signature = signature.to_vec();
        self
    }

    /// Verify the signature on this result
    pub fn verify_signature(&self) -> Result<()> {
        if self.signature.is_empty() {
            return Err(ComputeError::InvalidResult("Missing signature".into()));
        }

        let verifying_key = self.executor.to_verifying_key().map_err(|e| {
            ComputeError::InvalidResult(format!("Failed to extract public key: {e}"))
        })?;

        let signature = ed25519_dalek::Signature::from_slice(&self.signature)
            .map_err(|e| ComputeError::InvalidResult(format!("Invalid signature format: {e}")))?;

        use ed25519_dalek::Verifier;
        verifying_key
            .verify(&self.signing_bytes(), &signature)
            .map_err(|e| {
                ComputeError::InvalidResult(format!("Signature verification failed: {e}"))
            })?;

        Ok(())
    }

    /// Check if two results are equivalent (same output hash)
    pub fn is_equivalent(&self, other: &ComputeResult) -> bool {
        blake3::hash(&self.output) == blake3::hash(&other.output)
    }
}

/// A dispute over compute execution results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeDispute {
    /// Unique dispute ID
    pub dispute_id: String,
    /// Task that was disputed
    pub task_hash: TaskHash,
    /// Task submitter DID
    pub submitter: Did,
    /// Conflicting results from different executors
    pub results: Vec<ComputeResult>,
    /// Evidence submitted during dispute
    pub evidence: Vec<Evidence>,
    /// When the dispute was initiated
    pub initiated_at: u64,
    /// Evidence collection deadline
    pub evidence_deadline: u64,
    /// Current status
    pub status: DisputeStatus,
    /// Resolution (if resolved)
    pub resolution: Option<DisputeResolution>,
}

impl ComputeDispute {
    /// Create a new dispute
    pub fn new(task_hash: TaskHash, submitter: Did, results: Vec<ComputeResult>) -> Self {
        let now = current_timestamp();
        let dispute_id = format!("dispute-{}-{}", hex::encode(&task_hash[..8]), now);

        Self {
            dispute_id,
            task_hash,
            submitter,
            results,
            evidence: Vec::new(),
            initiated_at: now,
            evidence_deadline: now + 24 * 60 * 60, // 24 hours
            status: DisputeStatus::EvidenceCollection,
            resolution: None,
        }
    }

    /// Add evidence to the dispute
    pub fn add_evidence(&mut self, evidence: Evidence) -> Result<()> {
        if current_timestamp() > self.evidence_deadline {
            return Err(ComputeError::DisputeClosed);
        }

        if !matches!(self.status, DisputeStatus::EvidenceCollection) {
            return Err(ComputeError::InvalidDisputeState);
        }

        self.evidence.push(evidence);
        Ok(())
    }

    /// Check if evidence collection period is over
    pub fn is_evidence_period_over(&self) -> bool {
        current_timestamp() > self.evidence_deadline
    }
}

/// Status of a dispute
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DisputeStatus {
    /// Collecting evidence from executors
    EvidenceCollection,
    /// Assigned to arbiter for re-execution
    ArbitrationPending,
    /// Arbiter is re-executing
    Reexecuting,
    /// Dispute resolved
    Resolved,
    /// Dispute closed without resolution
    Closed,
}

/// Evidence submitted during a dispute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// DID of evidence submitter
    pub submitter: Did,
    /// Type of evidence
    pub evidence_type: EvidenceType,
    /// Evidence data
    pub data: Vec<u8>,
    /// When submitted
    pub submitted_at: u64,
    /// Signature over (evidence_type || data || submitted_at)
    pub signature: Vec<u8>,
}

/// Type of evidence that can be submitted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EvidenceType {
    /// Execution logs from executor
    ExecutionLogs,
    /// Input data verification
    InputData,
    /// Environment snapshot
    EnvironmentSnapshot,
    /// Witness statement from observer
    WitnessStatement,
    /// Other evidence
    Other(String),
}

/// Resolution of a dispute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisputeResolution {
    /// Consensus reached via majority vote
    Consensus {
        /// The canonical result
        result: ComputeResult,
        /// Number of executors agreeing
        majority: usize,
        /// Total number of executors
        total: usize,
    },

    /// Re-execution by trusted arbiter
    Reexecution {
        /// The arbiter who re-executed
        arbiter: Did,
        /// The canonical result
        canonical_result: ComputeResult,
        /// Correct executors (will be paid)
        correct_executors: Vec<Did>,
        /// Incorrect executors (will be penalized)
        incorrect_executors: Vec<Did>,
    },

    /// Task quarantined (no resolution possible)
    Quarantine {
        /// Reason for quarantine
        reason: String,
    },
}

/// Manager for compute disputes
pub struct DisputeManager {
    /// Active disputes
    disputes: Arc<RwLock<HashMap<String, ComputeDispute>>>,
    /// Minimum trust score for arbiters
    min_arbiter_trust: f64,
}

impl DisputeManager {
    /// Create a new dispute manager
    pub fn new(min_arbiter_trust: f64) -> Self {
        Self {
            disputes: Arc::new(RwLock::new(HashMap::new())),
            min_arbiter_trust,
        }
    }

    /// Initiate a new dispute
    pub fn initiate_dispute(
        &self,
        task_hash: TaskHash,
        submitter: Did,
        results: Vec<ComputeResult>,
    ) -> Result<String> {
        if results.len() < 2 {
            return Err(ComputeError::InvalidResult(
                "Need at least 2 conflicting results to initiate dispute".into(),
            ));
        }

        // Verify all signatures
        for result in &results {
            result.verify_signature()?;
        }

        // Check if results actually differ
        let first_hash = blake3::hash(&results[0].output);
        let has_conflict = results
            .iter()
            .skip(1)
            .any(|r| blake3::hash(&r.output) != first_hash);

        if !has_conflict {
            return Err(ComputeError::InvalidResult(
                "Results are equivalent, no dispute needed".into(),
            ));
        }

        let dispute = ComputeDispute::new(task_hash, submitter, results);
        let dispute_id = dispute.dispute_id.clone();

        let mut disputes = self.disputes.write().map_err(|_| ComputeError::LockError)?;

        disputes.insert(dispute_id.clone(), dispute);

        Ok(dispute_id)
    }

    /// Add evidence to a dispute
    pub fn add_evidence(&self, dispute_id: &str, evidence: Evidence) -> Result<()> {
        let mut disputes = self.disputes.write().map_err(|_| ComputeError::LockError)?;

        let dispute = disputes.get_mut(dispute_id).ok_or(ComputeError::NotFound)?;

        dispute.add_evidence(evidence)
    }

    /// Attempt to resolve dispute via consensus
    pub fn resolve_by_consensus(&self, dispute_id: &str) -> Result<DisputeResolution> {
        let mut disputes = self.disputes.write().map_err(|_| ComputeError::LockError)?;

        let dispute = disputes.get_mut(dispute_id).ok_or(ComputeError::NotFound)?;

        if !dispute.is_evidence_period_over() {
            return Err(ComputeError::InvalidDisputeState);
        }

        // Group results by output hash
        let mut result_groups: HashMap<[u8; 32], Vec<ComputeResult>> = HashMap::new();
        for result in &dispute.results {
            let hash = *blake3::hash(&result.output).as_bytes();
            result_groups.entry(hash).or_default().push(result.clone());
        }

        // Find majority
        let total = dispute.results.len();
        let (_, majority_group) = result_groups
            .iter()
            .max_by_key(|(_, group)| group.len())
            .ok_or_else(|| ComputeError::InvalidResult("No results to resolve".into()))?;

        let majority = majority_group.len();

        // Require >50% for consensus
        if majority as f64 / total as f64 <= 0.5 {
            return Err(ComputeError::InvalidResult(
                "No majority consensus reached".into(),
            ));
        }

        let resolution = DisputeResolution::Consensus {
            result: majority_group[0].clone(),
            majority,
            total,
        };

        dispute.resolution = Some(resolution.clone());
        dispute.status = DisputeStatus::Resolved;

        Ok(resolution)
    }

    /// Assign an arbiter for re-execution
    pub fn assign_arbiter(
        &self,
        dispute_id: &str,
        _arbiter: Did,
        arbiter_trust: f64,
    ) -> Result<()> {
        if arbiter_trust < self.min_arbiter_trust {
            return Err(ComputeError::InvalidResult(format!(
                "Arbiter trust {} below minimum {}",
                arbiter_trust, self.min_arbiter_trust
            )));
        }

        let mut disputes = self.disputes.write().map_err(|_| ComputeError::LockError)?;

        let dispute = disputes.get_mut(dispute_id).ok_or(ComputeError::NotFound)?;

        if !dispute.is_evidence_period_over() {
            return Err(ComputeError::InvalidDisputeState);
        }

        dispute.status = DisputeStatus::ArbitrationPending;

        Ok(())
    }

    /// Submit arbiter's re-execution result
    pub fn submit_arbiter_result(
        &self,
        dispute_id: &str,
        arbiter: Did,
        canonical_result: ComputeResult,
    ) -> Result<DisputeResolution> {
        canonical_result.verify_signature()?;

        let mut disputes = self.disputes.write().map_err(|_| ComputeError::LockError)?;

        let dispute = disputes.get_mut(dispute_id).ok_or(ComputeError::NotFound)?;

        if arbiter != canonical_result.executor {
            return Err(ComputeError::InvalidResult(
                "Arbiter DID doesn't match result executor".into(),
            ));
        }

        // Compare with original results to identify correct/incorrect executors
        let canonical_hash = blake3::hash(&canonical_result.output);
        let mut correct_executors = Vec::new();
        let mut incorrect_executors = Vec::new();

        for result in &dispute.results {
            let result_hash = blake3::hash(&result.output);
            if result_hash == canonical_hash {
                correct_executors.push(result.executor.clone());
            } else {
                incorrect_executors.push(result.executor.clone());
            }
        }

        let resolution = DisputeResolution::Reexecution {
            arbiter,
            canonical_result,
            correct_executors,
            incorrect_executors,
        };

        dispute.resolution = Some(resolution.clone());
        dispute.status = DisputeStatus::Resolved;

        Ok(resolution)
    }

    /// Quarantine a task (no resolution possible)
    pub fn quarantine_dispute(&self, dispute_id: &str, reason: String) -> Result<()> {
        let mut disputes = self.disputes.write().map_err(|_| ComputeError::LockError)?;

        let dispute = disputes.get_mut(dispute_id).ok_or(ComputeError::NotFound)?;

        let resolution = DisputeResolution::Quarantine { reason };
        dispute.resolution = Some(resolution);
        dispute.status = DisputeStatus::Closed;

        Ok(())
    }

    /// Get a dispute by ID
    pub fn get_dispute(&self, dispute_id: &str) -> Result<ComputeDispute> {
        let disputes = self.disputes.read().map_err(|_| ComputeError::LockError)?;

        disputes
            .get(dispute_id)
            .cloned()
            .ok_or(ComputeError::NotFound)
    }

    /// List all active disputes
    pub fn list_active_disputes(&self) -> Result<Vec<ComputeDispute>> {
        let disputes = self.disputes.read().map_err(|_| ComputeError::LockError)?;

        Ok(disputes
            .values()
            .filter(|d| !matches!(d.status, DisputeStatus::Resolved | DisputeStatus::Closed))
            .cloned()
            .collect())
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_task_hash() -> TaskHash {
        [1u8; 32]
    }

    fn test_result(executor_kp: &KeyPair, output: &[u8]) -> ComputeResult {
        ComputeResult {
            task_hash: test_task_hash(),
            executor: executor_kp.did().clone(),
            output: output.to_vec(),
            fuel_consumed: 1000,
            execution_time_ms: 100,
            executed_at: current_timestamp(),
            signature: Vec::new(),
        }
        .sign(executor_kp)
    }

    #[test]
    fn test_result_signature() {
        let kp = KeyPair::generate().unwrap();
        let result = test_result(&kp, b"output");

        assert!(result.verify_signature().is_ok());
    }

    #[test]
    fn test_result_equivalence() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();

        let result1 = test_result(&kp1, b"same");
        let result2 = test_result(&kp2, b"same");
        let result3 = test_result(&kp1, b"different");

        assert!(result1.is_equivalent(&result2));
        assert!(!result1.is_equivalent(&result3));
    }

    #[test]
    fn test_dispute_initiation() {
        let manager = DisputeManager::new(0.7);
        let submitter = KeyPair::generate().unwrap().did().clone();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();

        let result1 = test_result(&kp1, b"output1");
        let result2 = test_result(&kp2, b"output2");

        let dispute_id = manager
            .initiate_dispute(test_task_hash(), submitter, vec![result1, result2])
            .unwrap();

        let dispute = manager.get_dispute(&dispute_id).unwrap();
        assert_eq!(dispute.results.len(), 2);
        assert_eq!(dispute.status, DisputeStatus::EvidenceCollection);
    }

    #[test]
    fn test_dispute_no_conflict() {
        let manager = DisputeManager::new(0.7);
        let submitter = KeyPair::generate().unwrap().did().clone();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();

        // Same output - no conflict
        let result1 = test_result(&kp1, b"same");
        let result2 = test_result(&kp2, b"same");

        let err = manager
            .initiate_dispute(test_task_hash(), submitter, vec![result1, result2])
            .unwrap_err();

        match err {
            ComputeError::InvalidResult(msg) => {
                assert!(msg.contains("equivalent"));
            }
            _ => panic!("Expected InvalidResult error"),
        }
    }

    #[test]
    fn test_consensus_resolution() {
        let manager = DisputeManager::new(0.7);
        let submitter = KeyPair::generate().unwrap().did().clone();

        // 3 executors: 2 agree, 1 differs
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let kp3 = KeyPair::generate().unwrap();

        let result1 = test_result(&kp1, b"correct");
        let result2 = test_result(&kp2, b"correct");
        let result3 = test_result(&kp3, b"wrong");

        let dispute_id = manager
            .initiate_dispute(test_task_hash(), submitter, vec![result1, result2, result3])
            .unwrap();

        // Force evidence period to be over
        {
            let mut disputes = manager.disputes.write().unwrap();
            let dispute = disputes.get_mut(&dispute_id).unwrap();
            dispute.evidence_deadline = current_timestamp() - 1;
        }

        let resolution = manager.resolve_by_consensus(&dispute_id).unwrap();

        match resolution {
            DisputeResolution::Consensus {
                majority, total, ..
            } => {
                assert_eq!(majority, 2);
                assert_eq!(total, 3);
            }
            _ => panic!("Expected Consensus resolution"),
        }
    }

    #[test]
    fn test_arbiter_assignment() {
        let manager = DisputeManager::new(0.7);
        let submitter = KeyPair::generate().unwrap().did().clone();
        let arbiter = KeyPair::generate().unwrap().did().clone();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();

        let result1 = test_result(&kp1, b"output1");
        let result2 = test_result(&kp2, b"output2");

        let dispute_id = manager
            .initiate_dispute(test_task_hash(), submitter, vec![result1, result2])
            .unwrap();

        // Force evidence period to be over
        {
            let mut disputes = manager.disputes.write().unwrap();
            let dispute = disputes.get_mut(&dispute_id).unwrap();
            dispute.evidence_deadline = current_timestamp() - 1;
        }

        // High trust arbiter
        assert!(manager
            .assign_arbiter(&dispute_id, arbiter.clone(), 0.9)
            .is_ok());

        // Low trust arbiter should fail
        let low_trust_arbiter = KeyPair::generate().unwrap().did().clone();
        assert!(manager
            .assign_arbiter(&dispute_id, low_trust_arbiter, 0.3)
            .is_err());
    }
}
