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

/// Task priority level (higher priority tasks execute first)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum TaskPriority {
    /// Low priority - background tasks
    Low = 0,
    /// Normal priority - default
    #[default]
    Normal = 1,
    /// High priority - important tasks
    High = 2,
    /// Critical priority - urgent tasks only
    Critical = 3,
}

/// A compute task to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeTask {
    /// Unique task ID (set by submitter)
    pub id: TaskId,
    /// DID of task submitter
    pub submitter: String,
    /// Cooperative ID (for policy enforcement and usage tracking)
    #[serde(default)]
    pub coop_id: Option<String>,
    /// Task code (CCL source or WASM bytes reference)
    pub code: TaskCode,
    /// Input data (serialized)
    pub inputs: Vec<u8>,
    /// Maximum fuel for execution
    pub fuel_limit: FuelLimit,
    /// Required capabilities
    pub required_capabilities: Vec<ExecutorCapability>,
    /// Priority level
    #[serde(default)]
    pub priority: TaskPriority,
    /// Timestamp (Unix millis)
    pub created_at: u64,
    /// Optional deadline (Unix millis)
    pub deadline: Option<u64>,
    /// Payment rate per 1000 fuel units (in credits)
    pub payment_rate: Option<u64>,
    /// Currency for payment (default: "credits")
    pub payment_currency: Option<String>,
    /// Resource requirements (Phase 16B - for placement negotiation)
    #[serde(default)]
    pub resource_profile: Option<crate::scheduler::ResourceProfile>,
    /// Actor mode for stateful execution (Phase 16D Week 4)
    /// None = ephemeral (default), Some(mode) = stateful actor
    #[serde(default)]
    pub actor_mode: Option<crate::actor_model::ActorMode>,
    /// Placement constraints from policy (Phase 16E Week 2)
    #[serde(default)]
    pub placement_constraints: Option<crate::policy::PlacementConstraints>,
    /// Federation placement constraints (Phase 21 - cross-cooperative execution)
    #[serde(default)]
    pub federation_constraints: Option<crate::scheduler::FederatedPlacementConstraints>,
    /// Estimated task value in credits (Issue #478 - for verification requirements)
    /// Used to determine how many executors should verify the result
    #[serde(default)]
    pub estimated_value: Option<u64>,
    /// Verification requirements for multi-executor result quorum (Issue #478)
    /// If None, determined automatically from estimated_value
    #[serde(default)]
    pub verification: Option<crate::result_quorum::TaskVerification>,
}

impl ComputeTask {
    /// Compute the task hash
    pub fn hash(&self) -> TaskHash {
        let bytes = icn_encoding::encode_bincode_legacy(self).unwrap_or_default();
        *blake3::hash(&bytes).as_bytes()
    }

    /// Validate task parameters
    pub fn validate(&self) -> Result<(), crate::error::ComputeError> {
        // Validate task ID
        if self.id.is_empty() {
            return Err(crate::error::ComputeError::InvalidCode(
                "Task ID cannot be empty".into(),
            ));
        }
        if self.id.len() > 256 {
            return Err(crate::error::ComputeError::InvalidCode(format!(
                "Task ID too long: {} bytes (max 256)",
                self.id.len()
            )));
        }

        // Validate submitter DID format
        if !self.submitter.starts_with("did:icn:") {
            return Err(crate::error::ComputeError::InvalidCode(format!(
                "Invalid submitter DID format: {}",
                self.submitter
            )));
        }

        // Validate fuel limit
        const MIN_FUEL: u64 = 100;
        const MAX_FUEL: u64 = 10_000_000; // 10M operations max
        if self.fuel_limit.0 < MIN_FUEL {
            return Err(crate::error::ComputeError::InvalidCode(format!(
                "Fuel limit too low: {} (min {})",
                self.fuel_limit.0, MIN_FUEL
            )));
        }
        if self.fuel_limit.0 > MAX_FUEL {
            return Err(crate::error::ComputeError::InvalidCode(format!(
                "Fuel limit too high: {} (max {})",
                self.fuel_limit.0, MAX_FUEL
            )));
        }

        // Validate task code
        match &self.code {
            TaskCode::Ccl(source) => {
                if source.is_empty() {
                    return Err(crate::error::ComputeError::InvalidCode(
                        "CCL source cannot be empty".into(),
                    ));
                }
                if source.len() > 1_000_000 {
                    return Err(crate::error::ComputeError::InvalidCode(format!(
                        "CCL source too large: {} bytes (max 1MB)",
                        source.len()
                    )));
                }
            }
            TaskCode::WasmInline(bytes) => {
                if bytes.is_empty() {
                    return Err(crate::error::ComputeError::InvalidCode(
                        "WASM bytes cannot be empty".into(),
                    ));
                }
                if bytes.len() > 5_000_000 {
                    return Err(crate::error::ComputeError::InvalidCode(format!(
                        "WASM module too large: {} bytes (max 5MB)",
                        bytes.len()
                    )));
                }
            }
            TaskCode::WasmRef(_) => {
                // Hash is always valid by type
            }
            TaskCode::CclRef { rule, .. } => {
                // Hash is always valid by type, but rule name must be non-empty
                if rule.is_empty() {
                    return Err(crate::error::ComputeError::InvalidCode(
                        "Rule name cannot be empty".into(),
                    ));
                }
                if rule.len() > 256 {
                    return Err(crate::error::ComputeError::InvalidCode(format!(
                        "Rule name too long: {} chars (max 256)",
                        rule.len()
                    )));
                }
            }
        }

        // Validate input size
        const MAX_INPUT_SIZE: usize = 1_000_000; // 1MB max
        if self.inputs.len() > MAX_INPUT_SIZE {
            return Err(crate::error::ComputeError::InvalidCode(format!(
                "Input too large: {} bytes (max 1MB)",
                self.inputs.len()
            )));
        }

        // Validate deadline is in the future if provided
        if let Some(deadline) = self.deadline {
            let now = icn_time::current_timestamp_millis();

            if deadline <= now {
                return Err(crate::error::ComputeError::DeadlineExceeded);
            }
        }

        // Validate payment rate if provided
        if let Some(rate) = self.payment_rate {
            const MAX_RATE: u64 = 1_000_000; // 1M credits per 1000 fuel max
            if rate > MAX_RATE {
                return Err(crate::error::ComputeError::InvalidCode(format!(
                    "Payment rate too high: {rate} (max {MAX_RATE})"
                )));
            }
        }

        // Validate capabilities
        if self.required_capabilities.is_empty() {
            return Err(crate::error::ComputeError::InvalidCode(
                "At least one capability required".into(),
            ));
        }

        Ok(())
    }
}

/// Content hash for contract/WASM addressing
pub type ContentHash = [u8; 32];

/// Task code format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskCode {
    /// CCL source code (inline)
    Ccl(String),
    /// Reference to CCL contract by hash (from registry)
    CclRef {
        /// Content hash of the contract
        hash: ContentHash,
        /// Rule to execute
        rule: String,
    },
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
    pub fn verify_signature(
        &self,
        executor_did: &icn_identity::Did,
    ) -> Result<(), crate::error::ComputeError> {
        use ed25519_dalek::Verifier;

        // Extract public key from DID
        let verifying_key = executor_did.to_verifying_key().map_err(|e| {
            crate::error::ComputeError::InvalidSignature(format!(
                "Cannot extract public key from DID: {e}"
            ))
        })?;

        let signature =
            ed25519_dalek::Signature::from_bytes(self.signature.as_slice().try_into().map_err(
                |_| crate::error::ComputeError::InvalidSignature("Invalid signature length".into()),
            )?);

        let payload = self.signing_payload();
        verifying_key.verify(&payload, &signature).map_err(|e| {
            crate::error::ComputeError::InvalidSignature(format!(
                "Signature verification failed: {e}"
            ))
        })?;

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
    /// New task submitted (Phase 15 - legacy)
    TaskSubmitted(Box<ComputeTask>),
    /// Executor claims a task
    TaskClaimed {
        task_hash: TaskHash,
        executor: String,
    },
    /// Task result published
    TaskResult(ComputeResult),
    /// Task cancelled by submitter
    TaskCancelled {
        task_hash: TaskHash,
        submitter: String,
        reason: String,
        cancelled_at: u64,
    },
    /// Executor announces capabilities
    ExecutorAnnounce {
        executor: String,
        capabilities: Vec<ExecutorCapability>,
    },
    /// Request placement for a task (Phase 16B)
    PlacementRequest {
        task_hash: TaskHash,
        submitter: String,
        resource_profile: crate::scheduler::ResourceProfile,
        locality_hints: Vec<crate::scheduler::LocalityHint>,
        max_cost: Option<u64>,
        requested_at: u64,
    },
    /// Executor offers to execute a task (Phase 16B)
    PlacementOffer {
        task_hash: TaskHash,
        executor: String,
        score: f64,
        cost: u64,
        estimated_start: u64,
        offered_at: u64,
    },
    /// Executor announces capacity (Phase 16A)
    NodeCapacityAnnounce {
        executor: String,
        capacity: crate::scheduler::NodeCapacity,
    },

    // === Phase 16D: Actor Checkpointing & Migration ===
    /// Checkpoint announcement (Phase 16D)
    CheckpointAnnounce {
        checkpoint: crate::actor_model::ActorCheckpoint,
    },

    /// Query for latest checkpoint (Phase 16D)
    CheckpointQuery {
        actor_id: crate::actor_model::ActorId,
        requester: String,
    },

    /// Response to checkpoint query (Phase 16D)
    CheckpointResponse {
        actor_id: crate::actor_model::ActorId,
        checkpoint: Option<crate::actor_model::ActorCheckpoint>,
    },

    /// Initiate actor migration (Phase 16D)
    MigrationRequest {
        actor_id: crate::actor_model::ActorId,
        from_executor: String,
        to_executor: String,
        checkpoint: crate::actor_model::ActorCheckpoint,
        reason: crate::actor_model::MigrationReason,
    },

    /// Target accepts migration (Phase 16D)
    MigrationAccept {
        actor_id: crate::actor_model::ActorId,
        to_executor: String,
    },

    /// Target rejects migration (Phase 16D)
    MigrationReject {
        actor_id: crate::actor_model::ActorId,
        to_executor: String,
        reason: String,
    },

    /// Source confirms actor stopped and migration complete (Phase 16D)
    MigrationComplete {
        actor_id: crate::actor_model::ActorId,
        from_executor: String,
        to_executor: String,
        final_checkpoint: crate::actor_model::ActorCheckpoint,
        duration_ms: u64,
    },

    // === Cross-Cooperative Federation (Issue #515) ===
    /// Federated executor announces capabilities from another cooperative
    FederatedExecutorAnnounce {
        /// Executor DID (federated format: did:icn:coop-id:key)
        executor: String,
        /// Source cooperative ID
        cooperative_id: String,
        /// Executor capabilities
        capabilities: Vec<ExecutorCapability>,
        /// Attestation from source cooperative (serialized)
        attestation: crate::federation::FederatedExecutorAttestation,
    },

    /// Request task execution on federated executor
    FederatedTaskRequest {
        /// Task hash
        task_hash: TaskHash,
        /// The task to execute (boxed for size)
        task: Box<ComputeTask>,
        /// Requesting cooperative
        from_coop: String,
        /// Target cooperative
        to_coop: String,
        /// Payment terms for cross-coop execution
        payment: crate::federation::FederatedPaymentTerms,
        /// When this request was sent
        requested_at: u64,
    },

    /// Result from federated executor
    FederatedTaskResult {
        /// The computation result
        result: ComputeResult,
        /// Cooperative that executed the task
        executor_coop: String,
        /// Hash of attestation proving result integrity
        attestation_hash: [u8; 32],
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
            coop_id: None,
            code: TaskCode::Ccl("return 42".into()),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
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
        let bytes = icn_encoding::encode_bincode_legacy(&msg).unwrap();
        let decoded: ComputeMessage = icn_encoding::decode_bincode_legacy(&bytes).unwrap();

        // Verify correct variant and executor
        assert!(
            matches!(&decoded, ComputeMessage::TaskClaimed { .. }),
            "Expected TaskClaimed variant, got: {decoded:?}"
        );

        if let ComputeMessage::TaskClaimed { executor, .. } = decoded {
            assert_eq!(executor, "did:icn:bob");
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

    // Validation tests

    fn valid_task() -> ComputeTask {
        ComputeTask {
            id: "test-task-1".into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::Ccl(r#"{"name": "Test"}"#.into()),
            inputs: vec![],
            fuel_limit: FuelLimit(10_000),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
        }
    }

    #[test]
    fn test_validate_valid_task() {
        let task = valid_task();
        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_id() {
        let mut task = valid_task();
        task.id = "".into();
        let result = task.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_id_too_long() {
        let mut task = valid_task();
        task.id = "x".repeat(300);
        let result = task.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_validate_invalid_did() {
        let mut task = valid_task();
        task.submitter = "not-a-did".into();
        let result = task.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid submitter DID"));
    }

    #[test]
    fn test_validate_fuel_too_low() {
        let mut task = valid_task();
        task.fuel_limit = FuelLimit(50); // Below MIN_FUEL (100)
        let result = task.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too low"));
    }

    #[test]
    fn test_validate_fuel_too_high() {
        let mut task = valid_task();
        task.fuel_limit = FuelLimit(20_000_000); // Above MAX_FUEL (10M)
        let result = task.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too high"));
    }

    #[test]
    fn test_validate_empty_ccl() {
        let mut task = valid_task();
        task.code = TaskCode::Ccl("".into());
        let result = task.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_ccl_too_large() {
        let mut task = valid_task();
        task.code = TaskCode::Ccl("x".repeat(2_000_000)); // Above 1MB
        let result = task.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn test_validate_input_too_large() {
        let mut task = valid_task();
        task.inputs = vec![0u8; 2_000_000]; // Above 1MB
        let result = task.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Input too large"));
    }

    #[test]
    fn test_validate_payment_rate_too_high() {
        let mut task = valid_task();
        task.payment_rate = Some(2_000_000); // Above MAX_RATE (1M)
        let result = task.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Payment rate too high"));
    }

    #[test]
    fn test_validate_no_capabilities() {
        let mut task = valid_task();
        task.required_capabilities = vec![];
        let result = task.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("At least one capability"));
    }
}
