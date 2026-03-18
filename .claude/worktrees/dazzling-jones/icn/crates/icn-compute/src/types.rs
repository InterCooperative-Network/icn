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

    // ========================================================================
    // Workload Manifest Fields (E1 - Constitution for Execution)
    // ========================================================================
    /// Hash of input data for deterministic verification.
    ///
    /// Required for Canonical tasks; allows re-execution verification.
    /// Computed as blake3(inputs) by the submitter.
    #[serde(default)]
    pub inputs_hash: Option<ContentHash>,

    /// Hash of the CCL policy governing this execution.
    ///
    /// Links execution to governance: the policy defines what this task
    /// is allowed to do and what constraints apply to its outputs.
    /// Required for Canonical tasks that mutate state.
    #[serde(default)]
    pub policy_hash: Option<ContentHash>,

    /// Classification for state mutation permissions (E1).
    ///
    /// Canonical: outputs can mutate ledger/governance state.
    /// Advisory: outputs are suggestions only, cannot mutate state.
    #[serde(default)]
    pub determinism_class: DeterminismClass,

    /// Privacy classification for inputs/outputs (E1).
    ///
    /// Controls audit visibility tier and access permissions.
    #[serde(default)]
    pub privacy_class: PrivacyClass,

    // ========================================================================
    // Storage Specification Fields (E4 - Storage Tiers & Data Locality)
    // ========================================================================
    /// Storage class for task outputs (E4).
    ///
    /// Determines how outputs are stored and replicated.
    /// Default: ServiceState (mutable, coop-scoped).
    #[serde(default)]
    pub storage_class: Option<icn_kernel_api::storage::StorageClass>,

    /// Data locality constraints for task execution (E4).
    ///
    /// Determines where the task can execute and what data it can access.
    /// Default: CellLocal (most restrictive).
    #[serde(default)]
    pub data_locality: Option<icn_kernel_api::storage::DataLocality>,

    // ========================================================================
    // Commons Credit Scope (E7 - #948)
    // ========================================================================
    /// Scope level for commons credit settlement (E7 - #948).
    ///
    /// Determines how task completion is settled economically:
    /// - `Local` (default): standard payment settlement via `settle_receipt()`.
    /// - `Commons`: commons credit settlement via `settle_commons_receipt()`.
    ///   Executor earns commons credits from the mint; submitter spends credits to the mint.
    ///
    /// **The default is `Local`** — tasks never accidentally collectivize resources.
    /// Commons must be an explicit opt-in by the submitter at task creation.
    /// Defaulting to a wider scope would silently collectivize resources.
    #[serde(default)]
    pub scope: icn_kernel_api::ScopeLevel,
}

impl ComputeTask {
    /// Compute the task hash
    pub fn hash(&self) -> TaskHash {
        let bytes = icn_encoding::encode(self).unwrap_or_default();
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

        // Validate workload manifest fields (E1)
        // Canonical tasks that can mutate state have stricter requirements
        if self.determinism_class == DeterminismClass::Canonical {
            // Canonical tasks must have inputs_hash for verification
            if self.inputs_hash.is_none() && !self.inputs.is_empty() {
                return Err(crate::error::ComputeError::InvalidCode(
                    "Canonical tasks with inputs must provide inputs_hash for verification".into(),
                ));
            }
            // Verify inputs_hash matches actual inputs when provided
            if let Some(provided_hash) = &self.inputs_hash {
                if !self.inputs.is_empty() {
                    let computed_hash = *blake3::hash(&self.inputs).as_bytes();
                    if provided_hash != &computed_hash {
                        return Err(crate::error::ComputeError::InvalidCode(
                            "inputs_hash does not match blake3 hash of inputs".into(),
                        ));
                    }
                }
            }
        }

        // Validate storage specification fields (E4)
        // Canonical tasks producing outputs SHOULD use StorageClass::Canonical
        if self.determinism_class == DeterminismClass::Canonical {
            if let Some(storage_class) = &self.storage_class {
                if !storage_class.can_hold_canonical_output() {
                    // This is a SHOULD, not a MUST - log warning but don't reject
                    // The scheduler may apply stricter enforcement
                }
            }
        }

        // CellLocal data cannot be accessed by tasks with FederationMirrored locality
        // None defaults to CellLocal behavior for safety
        let effective_locality = self
            .data_locality
            .unwrap_or(icn_kernel_api::storage::DataLocality::CellLocal);
        if let Some(ref fed_constraints) = self.federation_constraints {
            use crate::scheduler::FederationPolicy;
            // Federation tasks (AllowFederated or FederatedWhitelist) should not use CellLocal
            let allows_federation = !matches!(
                fed_constraints.federation_policy,
                FederationPolicy::LocalOnly
            );
            if effective_locality.is_cell_local() && allows_federation {
                return Err(crate::error::ComputeError::InvalidCode(
                    "Tasks with CellLocal data locality cannot allow federated execution".into(),
                ));
            }
        }

        Ok(())
    }
}

/// Content hash for contract/WASM addressing
pub type ContentHash = [u8; 32];

// ============================================================================
// Workload Manifest Types (E1 - Constitution for Execution)
// ============================================================================

/// Classification for whether workload outputs can mutate canonical state.
///
/// This is the "split the universe" rule: only Canonical workloads can
/// mutate ledger state, governance records, or trust edges.
///
/// # Examples
///
/// ```
/// use icn_compute::DeterminismClass;
///
/// // Governance tallying must be deterministic
/// let tally = DeterminismClass::Canonical;
///
/// // ML recommendations are advisory only
/// let forecast = DeterminismClass::Advisory;
///
/// assert!(tally.can_mutate_state());
/// assert!(!forecast.can_mutate_state());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismClass {
    /// Must be deterministic; outputs can mutate canonical state.
    ///
    /// Examples: governance tallying, budget computation, settlement/netting,
    /// eligibility evaluation, trust edge updates.
    #[default]
    Canonical,
    /// Can be probabilistic; outputs are suggestions only.
    ///
    /// Examples: demand forecasting, routing optimization, recommendations,
    /// anomaly detection, predictive maintenance.
    Advisory,
}

impl DeterminismClass {
    /// Returns true if workloads of this class can mutate canonical state.
    #[inline]
    pub const fn can_mutate_state(&self) -> bool {
        matches!(self, DeterminismClass::Canonical)
    }

    /// Returns true if workloads must produce deterministic outputs.
    #[inline]
    pub const fn requires_determinism(&self) -> bool {
        matches!(self, DeterminismClass::Canonical)
    }
}

/// Privacy classification for workload inputs and outputs.
///
/// Maps directly to audit visibility tiers defined in the governance model.
/// Controls who can observe task inputs, outputs, and execution traces.
///
/// # Examples
///
/// ```
/// use icn_compute::PrivacyClass;
///
/// let public_task = PrivacyClass::Public;
/// let member_task = PrivacyClass::Member;
/// let sensitive = PrivacyClass::NeedToKnow;
///
/// assert!(public_task.visible_to_public());
/// assert!(!member_task.visible_to_public());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    /// Anyone can see inputs, outputs, and execution traces.
    /// Suitable for public governance, commons computation, open data.
    #[default]
    Public,
    /// Only organization/cooperative members can observe.
    /// Suitable for internal operations, member-only services.
    Member,
    /// Selective disclosure via ZK proofs; only authorized parties see data.
    /// Suitable for personal data, sensitive financial info, health records.
    NeedToKnow,
}

impl PrivacyClass {
    /// Returns true if data is visible to the general public.
    #[inline]
    pub const fn visible_to_public(&self) -> bool {
        matches!(self, PrivacyClass::Public)
    }

    /// Returns true if membership verification is required for access.
    #[inline]
    pub const fn requires_membership(&self) -> bool {
        matches!(self, PrivacyClass::Member | PrivacyClass::NeedToKnow)
    }

    /// Returns true if ZK proof verification is required for access.
    #[inline]
    pub const fn requires_zk_proof(&self) -> bool {
        matches!(self, PrivacyClass::NeedToKnow)
    }
}

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
        /// Maximum scope level for placement (Epic 2).
        /// Backward-compatible: defaults to None (no restriction).
        #[serde(default)]
        max_scope: Option<icn_kernel_api::ScopeLevel>,
        /// Preferred cell for placement (Epic 2).
        /// Backward-compatible: defaults to None.
        #[serde(default)]
        cell_affinity: Option<icn_kernel_api::CellId>,
        /// Explicit list of allowed scope levels (Epic 2).
        /// Empty means all scopes accepted.
        #[serde(default)]
        allowed_scopes: Vec<icn_kernel_api::ScopeLevel>,
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
        /// Cell this node belongs to. `None` = unaffiliated (Epic 6 #947).
        #[serde(default)]
        cell_id: Option<icn_kernel_api::CellId>,
        /// Explicit capacity budget. `None` means use default for the
        /// affiliation status (Epic 6 #947).
        #[serde(default)]
        capacity_budget: Option<crate::scheduler::CapacityBudget>,
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
            // E1: Workload manifest fields
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
            scope: Default::default(),
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
        let bytes = icn_encoding::encode(&msg).unwrap();
        let decoded: ComputeMessage = icn_encoding::decode(&bytes).unwrap();

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
            // E1: Workload manifest fields
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
            scope: Default::default(),
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

    // ========================================================================
    // E1: Workload Manifest Tests
    // ========================================================================

    #[test]
    fn test_determinism_class_default() {
        assert_eq!(DeterminismClass::default(), DeterminismClass::Canonical);
    }

    #[test]
    fn test_determinism_class_can_mutate_state() {
        assert!(DeterminismClass::Canonical.can_mutate_state());
        assert!(!DeterminismClass::Advisory.can_mutate_state());
    }

    #[test]
    fn test_determinism_class_requires_determinism() {
        assert!(DeterminismClass::Canonical.requires_determinism());
        assert!(!DeterminismClass::Advisory.requires_determinism());
    }

    #[test]
    fn test_privacy_class_default() {
        assert_eq!(PrivacyClass::default(), PrivacyClass::Public);
    }

    #[test]
    fn test_privacy_class_visibility() {
        assert!(PrivacyClass::Public.visible_to_public());
        assert!(!PrivacyClass::Member.visible_to_public());
        assert!(!PrivacyClass::NeedToKnow.visible_to_public());
    }

    #[test]
    fn test_privacy_class_requires_membership() {
        assert!(!PrivacyClass::Public.requires_membership());
        assert!(PrivacyClass::Member.requires_membership());
        assert!(PrivacyClass::NeedToKnow.requires_membership());
    }

    #[test]
    fn test_privacy_class_requires_zk_proof() {
        assert!(!PrivacyClass::Public.requires_zk_proof());
        assert!(!PrivacyClass::Member.requires_zk_proof());
        assert!(PrivacyClass::NeedToKnow.requires_zk_proof());
    }

    #[test]
    fn test_validate_canonical_task_with_inputs_requires_hash() {
        let mut task = valid_task();
        task.determinism_class = DeterminismClass::Canonical;
        task.inputs = vec![1, 2, 3]; // Non-empty inputs
        task.inputs_hash = None; // Missing hash

        let result = task.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("inputs_hash for verification"));
    }

    #[test]
    fn test_validate_canonical_task_with_inputs_and_hash_ok() {
        let mut task = valid_task();
        task.determinism_class = DeterminismClass::Canonical;
        task.inputs = vec![1, 2, 3];
        task.inputs_hash = Some(*blake3::hash(&task.inputs).as_bytes());

        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_validate_advisory_task_no_hash_required() {
        let mut task = valid_task();
        task.determinism_class = DeterminismClass::Advisory;
        task.inputs = vec![1, 2, 3]; // Non-empty inputs
        task.inputs_hash = None; // No hash required for advisory

        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_validate_canonical_empty_inputs_no_hash_ok() {
        let mut task = valid_task();
        task.determinism_class = DeterminismClass::Canonical;
        task.inputs = vec![]; // Empty inputs
        task.inputs_hash = None; // No hash needed for empty inputs

        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_determinism_class_serialization() {
        let canonical = DeterminismClass::Canonical;
        let advisory = DeterminismClass::Advisory;

        let canonical_json = serde_json::to_string(&canonical).unwrap();
        let advisory_json = serde_json::to_string(&advisory).unwrap();

        assert_eq!(canonical_json, r#""canonical""#);
        assert_eq!(advisory_json, r#""advisory""#);

        // Deserialize back
        let parsed: DeterminismClass = serde_json::from_str(&canonical_json).unwrap();
        assert_eq!(parsed, DeterminismClass::Canonical);
    }

    #[test]
    fn test_privacy_class_serialization() {
        let public = PrivacyClass::Public;
        let member = PrivacyClass::Member;
        let ntk = PrivacyClass::NeedToKnow;

        assert_eq!(serde_json::to_string(&public).unwrap(), r#""public""#);
        assert_eq!(serde_json::to_string(&member).unwrap(), r#""member""#);
        assert_eq!(serde_json::to_string(&ntk).unwrap(), r#""need_to_know""#);
    }

    // ========================================================================
    // E4: Storage Specification Tests
    // ========================================================================

    #[test]
    fn test_storage_class_default_is_service_state() {
        use icn_kernel_api::storage::StorageClass;
        assert_eq!(StorageClass::default(), StorageClass::ServiceState);
    }

    #[test]
    fn test_data_locality_default_is_cell_local() {
        use icn_kernel_api::storage::DataLocality;
        assert_eq!(DataLocality::default(), DataLocality::CellLocal);
    }

    #[test]
    fn test_task_with_storage_class() {
        use icn_kernel_api::storage::StorageClass;
        let mut task = valid_task();
        task.storage_class = Some(StorageClass::Canonical);
        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_task_with_data_locality() {
        use icn_kernel_api::storage::DataLocality;
        let mut task = valid_task();
        task.data_locality = Some(DataLocality::CoopReplicated);
        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_task_with_all_storage_fields() {
        use icn_kernel_api::storage::{DataLocality, StorageClass};
        let mut task = valid_task();
        task.storage_class = Some(StorageClass::ServiceState);
        task.data_locality = Some(DataLocality::CoopReplicated);
        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_validate_cell_local_with_cross_coop_rejected() {
        use icn_kernel_api::storage::DataLocality;
        let mut task = valid_task();
        task.data_locality = Some(DataLocality::CellLocal);
        task.federation_constraints = Some(crate::scheduler::FederatedPlacementConstraints {
            base: crate::policy::PlacementConstraints::default(),
            federation_policy: crate::scheduler::FederationPolicy::AllowFederated,
            min_federated_trust: None,
            local_preference_weight: None,
        });

        let result = task.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("CellLocal data locality cannot allow federated"));
    }

    #[test]
    fn test_validate_cell_local_without_cross_coop_ok() {
        use icn_kernel_api::storage::DataLocality;
        let mut task = valid_task();
        task.data_locality = Some(DataLocality::CellLocal);
        task.federation_constraints = Some(crate::scheduler::FederatedPlacementConstraints {
            base: crate::policy::PlacementConstraints::default(),
            federation_policy: crate::scheduler::FederationPolicy::LocalOnly,
            min_federated_trust: None,
            local_preference_weight: None,
        });

        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_validate_federation_mirrored_with_cross_coop_ok() {
        use icn_kernel_api::storage::DataLocality;
        let mut task = valid_task();
        task.data_locality = Some(DataLocality::FederationMirrored);
        task.federation_constraints = Some(crate::scheduler::FederatedPlacementConstraints {
            base: crate::policy::PlacementConstraints::default(),
            federation_policy: crate::scheduler::FederationPolicy::AllowFederated,
            min_federated_trust: None,
            local_preference_weight: None,
        });

        assert!(task.validate().is_ok());
    }

    #[test]
    fn test_storage_class_serialization() {
        use icn_kernel_api::storage::StorageClass;

        let canonical = StorageClass::Canonical;
        let service = StorageClass::ServiceState;
        let blobs = StorageClass::Blobs;

        assert_eq!(serde_json::to_string(&canonical).unwrap(), r#""canonical""#);
        assert_eq!(
            serde_json::to_string(&service).unwrap(),
            r#""service_state""#
        );
        assert_eq!(serde_json::to_string(&blobs).unwrap(), r#""blobs""#);
    }

    #[test]
    fn test_data_locality_serialization() {
        use icn_kernel_api::storage::DataLocality;

        assert_eq!(
            serde_json::to_string(&DataLocality::CellLocal).unwrap(),
            r#""cell_local""#
        );
        assert_eq!(
            serde_json::to_string(&DataLocality::CoopReplicated).unwrap(),
            r#""coop_replicated""#
        );
        assert_eq!(
            serde_json::to_string(&DataLocality::FederationMirrored).unwrap(),
            r#""federation_mirrored""#
        );
        assert_eq!(
            serde_json::to_string(&DataLocality::CommonsPublic).unwrap(),
            r#""commons_public""#
        );
    }

    #[test]
    fn test_data_locality_access_rules() {
        use icn_kernel_api::storage::DataLocality;

        // FederationMirrored cannot access CellLocal
        assert!(!DataLocality::FederationMirrored.can_access(DataLocality::CellLocal));

        // CellLocal can access all levels
        assert!(DataLocality::CellLocal.can_access(DataLocality::CellLocal));
        assert!(DataLocality::CellLocal.can_access(DataLocality::CommonsPublic));

        // Same level access is always allowed
        assert!(DataLocality::CoopReplicated.can_access(DataLocality::CoopReplicated));
    }
}
