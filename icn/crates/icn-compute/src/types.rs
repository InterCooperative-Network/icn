//! Core types for the compute layer.

use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};

/// Unique task identifier (blake3 hash of task content)
pub type TaskHash = [u8; 32];

/// Human-readable task ID
pub type TaskId = String;

/// Fuel limit for execution (prevents infinite loops)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuelLimit(pub u64);

impl Default for FuelLimit {
    fn default() -> Self {
        Self(10_000) // 10k operations default
    }
}

/// Capabilities an executor advertises
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutorCapability {
    /// Can execute CCL contracts
    Ccl,
    /// Can execute WASM modules (future)
    Wasm,
    /// Custom capability
    Custom(String),
}

/// A compute task to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeTask {
    /// Unique task ID (set by submitter)
    pub id: TaskId,
    /// DID of task submitter
    pub submitter: String,
    /// Task code (CCL source or WASM bytes reference)
    pub code: TaskCode,
    /// Input data (serialized)
    pub inputs: Vec<u8>,
    /// Maximum fuel for execution
    pub fuel_limit: FuelLimit,
    /// Required capabilities
    pub required_capabilities: Vec<ExecutorCapability>,
    /// Timestamp (Unix millis)
    pub created_at: u64,
    /// Optional deadline (Unix millis)
    pub deadline: Option<u64>,
    /// Payment rate per 1000 fuel units (in credits)
    pub payment_rate: Option<u64>,
    /// Currency for payment (default: "credits")
    pub payment_currency: Option<String>,
}

impl ComputeTask {
    /// Compute the task hash
    pub fn hash(&self) -> TaskHash {
        let bytes = bincode::serialize(self).unwrap_or_default();
        *blake3::hash(&bytes).as_bytes()
    }
}

/// Task code format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskCode {
    /// CCL source code
    Ccl(String),
    /// Reference to WASM module by hash
    WasmRef(TaskHash),
    /// Inline WASM bytes (small modules only)
    WasmInline(Vec<u8>),
}

/// Result of task execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeResult {
    /// Hash of the executed task
    pub task_hash: TaskHash,
    /// Task ID for correlation
    pub task_id: TaskId,
    /// DID of executor
    pub executor: String,
    /// Execution outcome
    pub outcome: ExecutionOutcome,
    /// Fuel consumed
    pub fuel_used: u64,
    /// Execution duration (millis)
    pub duration_ms: u64,
    /// Timestamp
    pub completed_at: u64,
    /// Ed25519 signature over result
    pub signature: Vec<u8>,
}

impl ComputeResult {
    /// Compute canonical hash for signing (deterministic serialization)
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.task_hash);
        payload.extend_from_slice(self.task_id.as_bytes());
        payload.extend_from_slice(self.executor.as_bytes());

        // Outcome hash
        match &self.outcome {
            ExecutionOutcome::Success(data) => {
                payload.push(0);
                payload.extend_from_slice(data);
            }
            ExecutionOutcome::Failed(msg) => {
                payload.push(1);
                payload.extend_from_slice(msg.as_bytes());
            }
            ExecutionOutcome::OutOfFuel => {
                payload.push(2);
            }
            ExecutionOutcome::Timeout => {
                payload.push(3);
            }
        }

        payload.extend_from_slice(&self.fuel_used.to_le_bytes());
        payload.extend_from_slice(&self.duration_ms.to_le_bytes());
        payload.extend_from_slice(&self.completed_at.to_le_bytes());

        payload
    }

    /// Sign the result with Ed25519
    pub fn sign(&mut self, signing_key: &ed25519_dalek::SigningKey) {
        let payload = self.signing_payload();
        let signature = signing_key.sign(&payload);
        self.signature = signature.to_bytes().to_vec();
    }

    /// Verify the signature
    pub fn verify_signature(&self, executor_did: &icn_identity::Did) -> Result<(), crate::error::ComputeError> {
        use ed25519_dalek::Verifier;

        // Extract public key from DID
        let verifying_key = executor_did.to_verifying_key()
            .map_err(|e| crate::error::ComputeError::InvalidSignature(format!("Cannot extract public key from DID: {}", e)))?;

        let signature = ed25519_dalek::Signature::from_bytes(
            self.signature.as_slice().try_into()
                .map_err(|_| crate::error::ComputeError::InvalidSignature("Invalid signature length".into()))?
        );

        let payload = self.signing_payload();
        verifying_key.verify(&payload, &signature)
            .map_err(|e| crate::error::ComputeError::InvalidSignature(format!("Signature verification failed: {}", e)))?;

        Ok(())
    }
}

/// Outcome of task execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionOutcome {
    /// Successful execution with output
    Success(Vec<u8>),
    /// Execution failed with error
    Failed(String),
    /// Ran out of fuel
    OutOfFuel,
    /// Deadline exceeded
    Timeout,
}

/// Messages sent over gossip for compute coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComputeMessage {
    /// New task submitted
    TaskSubmitted(ComputeTask),
    /// Executor claims a task
    TaskClaimed {
        task_hash: TaskHash,
        executor: String,
    },
    /// Task result published
    TaskResult(ComputeResult),
    /// Executor announces capabilities
    ExecutorAnnounce {
        executor: String,
        capabilities: Vec<ExecutorCapability>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_hash_deterministic() {
        let task = ComputeTask {
            id: "test-1".into(),
            submitter: "did:icn:alice".into(),
            code: TaskCode::Ccl("return 42".into()),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Ccl],
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
        };

        let hash1 = task.hash();
        let hash2 = task.hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_fuel_limit_default() {
        assert_eq!(FuelLimit::default().0, 10_000);
    }

    #[test]
    fn test_message_serialization() {
        let msg = ComputeMessage::TaskClaimed {
            task_hash: [0u8; 32],
            executor: "did:icn:bob".into(),
        };
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ComputeMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ComputeMessage::TaskClaimed { executor, .. } => {
                assert_eq!(executor, "did:icn:bob");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_result_signing_and_verification() {
        // Generate a keypair for testing
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did();

        // Create a test result
        let mut result = ComputeResult {
            task_hash: [1u8; 32],
            task_id: "test-task".into(),
            executor: executor_did.to_string(),
            outcome: ExecutionOutcome::Success(vec![42]),
            fuel_used: 1000,
            duration_ms: 100,
            completed_at: 123456789,
            signature: vec![],
        };

        // Sign the result using keypair.sign()
        let payload = result.signing_payload();
        let signature = keypair.sign(&payload);
        result.signature = signature.to_bytes().to_vec();

        // Verify signature is not empty
        assert!(!result.signature.is_empty());
        assert_eq!(result.signature.len(), 64); // Ed25519 signatures are 64 bytes

        // Verify the signature
        assert!(result.verify_signature(executor_did).is_ok());
    }

    #[test]
    fn test_result_signature_verification_fails_wrong_did() {
        // Generate two different keypairs
        let signer_keypair = icn_identity::KeyPair::generate().unwrap();
        let wrong_keypair = icn_identity::KeyPair::generate().unwrap();

        // Create and sign result with first keypair
        let mut result = ComputeResult {
            task_hash: [1u8; 32],
            task_id: "test-task".into(),
            executor: signer_keypair.did().to_string(),
            outcome: ExecutionOutcome::Success(vec![42]),
            fuel_used: 1000,
            duration_ms: 100,
            completed_at: 123456789,
            signature: vec![],
        };

        let payload = result.signing_payload();
        let signature = signer_keypair.sign(&payload);
        result.signature = signature.to_bytes().to_vec();

        // Try to verify with different DID - should fail
        assert!(result.verify_signature(wrong_keypair.did()).is_err());
    }

    #[test]
    fn test_result_signature_verification_fails_tampered() {
        let keypair = icn_identity::KeyPair::generate().unwrap();

        let mut result = ComputeResult {
            task_hash: [1u8; 32],
            task_id: "test-task".into(),
            executor: keypair.did().to_string(),
            outcome: ExecutionOutcome::Success(vec![42]),
            fuel_used: 1000,
            duration_ms: 100,
            completed_at: 123456789,
            signature: vec![],
        };

        let payload = result.signing_payload();
        let signature = keypair.sign(&payload);
        result.signature = signature.to_bytes().to_vec();

        // Tamper with the result after signing
        result.fuel_used = 9999;

        // Verification should fail
        assert!(result.verify_signature(keypair.did()).is_err());
    }

    #[test]
    fn test_result_signing_payload_deterministic() {
        let result = ComputeResult {
            task_hash: [1u8; 32],
            task_id: "test-task".into(),
            executor: "did:icn:executor".into(),
            outcome: ExecutionOutcome::Success(vec![42, 43]),
            fuel_used: 1000,
            duration_ms: 100,
            completed_at: 123456789,
            signature: vec![],
        };

        let payload1 = result.signing_payload();
        let payload2 = result.signing_payload();

        assert_eq!(payload1, payload2);
        assert!(!payload1.is_empty());
    }
}
