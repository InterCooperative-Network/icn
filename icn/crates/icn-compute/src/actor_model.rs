//! Actor model primitives for stateful, long-running computation.
//!
//! This module extends the Phase 15 task-based compute layer with:
//! - Persistent actor state (checkpointing)
//! - Actor migration between executors
//! - Lifecycle management (spawn, run, migrate, terminate)
//!
//! # Design Philosophy
//!
//! - **Incremental**: Builds on existing `ComputeTask` infrastructure
//! - **Backward Compatible**: Stateless tasks continue to work unchanged
//! - **Trust-Governed**: Migration respects trust + locality constraints
//! - **Gossip-Native**: State propagated via gossip, no central coordinator

use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};

/// Unique identifier for a long-running actor (distinct from TaskHash).
///
/// Actor IDs are stable across migrations, while TaskHash changes per execution.
pub type ActorId = [u8; 32];

/// Actor execution mode.
///
/// Determines whether task is ephemeral (current behavior) or stateful (Phase 16D).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorMode {
    /// Ephemeral task - no state persistence, single execution (Phase 15 behavior)
    Ephemeral,

    /// Long-running actor with persistent state
    Stateful {
        /// Checkpoint interval in seconds (e.g., 60 = checkpoint every minute)
        checkpoint_interval_secs: u64,
        /// Maximum state size in bytes (prevents unbounded growth)
        max_state_size_bytes: u64,
    },
}

impl Default for ActorMode {
    fn default() -> Self {
        Self::Ephemeral
    }
}

/// Actor state snapshot for checkpointing.
///
/// Checkpoints enable:
/// - Recovery from executor failure
/// - Migration between executors
/// - Audit trail of actor execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorCheckpoint {
    /// Actor ID (stable across checkpoints)
    pub actor_id: ActorId,

    /// Checkpoint sequence number (monotonically increasing per actor)
    pub sequence: u64,

    /// Serialized state (CCL Value or arbitrary bytes)
    pub state: Vec<u8>,

    /// Checkpoint creation timestamp (Unix milliseconds)
    pub created_at: u64,

    /// Executor that created this checkpoint
    pub executor: String,

    /// Blake3 hash of state for integrity verification
    pub state_hash: [u8; 32],

    /// Ed25519 signature over (actor_id, sequence, state_hash, created_at, executor)
    pub signature: Vec<u8>,
}

impl ActorCheckpoint {
    /// Create canonical signing payload (deterministic serialization)
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();

        payload.extend_from_slice(&self.actor_id);
        payload.extend_from_slice(&self.sequence.to_le_bytes());
        payload.extend_from_slice(&self.state_hash);
        payload.extend_from_slice(&self.created_at.to_le_bytes());
        payload.extend_from_slice(self.executor.as_bytes());

        payload
    }

    /// Sign the checkpoint with executor's Ed25519 key
    pub fn sign(&mut self, signing_key: &ed25519_dalek::SigningKey) {
        let payload = self.signing_payload();
        let signature = signing_key.sign(&payload);
        self.signature = signature.to_bytes().to_vec();
    }

    /// Verify checkpoint signature against executor DID
    pub fn verify_signature(
        &self,
        executor_did: &icn_identity::Did,
    ) -> Result<(), crate::error::ComputeError> {
        use ed25519_dalek::Verifier;

        // Extract public key from executor DID
        let verifying_key = executor_did
            .to_verifying_key()
            .map_err(|e| {
                crate::error::ComputeError::InvalidSignature(format!(
                    "Cannot extract public key from DID: {e}"
                ))
            })?;

        let signature = ed25519_dalek::Signature::from_bytes(
            self.signature
                .as_slice()
                .try_into()
                .map_err(|_| {
                    crate::error::ComputeError::InvalidSignature("Invalid signature length".into())
                })?,
        );

        let payload = self.signing_payload();
        verifying_key
            .verify(&payload, &signature)
            .map_err(|e| {
                crate::error::ComputeError::InvalidSignature(format!(
                    "Signature verification failed: {e}"
                ))
            })?;

        Ok(())
    }

    /// Compute state hash for integrity check
    pub fn compute_state_hash(state: &[u8]) -> [u8; 32] {
        *blake3::hash(state).as_bytes()
    }
}

/// Reason for actor migration between executors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationReason {
    /// Executor overloaded (load balancing)
    LoadBalancing,

    /// Executor going offline (planned maintenance)
    Maintenance,

    /// Better data locality available (Phase 16C integration)
    LocalityOptimization,

    /// Cooperative policy enforcement (e.g., data sovereignty)
    Policy(String),

    /// Executor failure (forced migration)
    FailureRecovery,
}

/// Reason for actor termination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminationReason {
    /// Actor completed normally
    Completed,

    /// Actor crashed (runtime error)
    Failed(String),

    /// Explicit termination request (e.g., resource limits, user request)
    Killed(String),

    /// Executor failure (actor lost)
    ExecutorFailure,
}

/// Actor lifecycle event (broadcast via gossip for observability).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorEvent {
    /// Actor spawned on executor
    Spawned {
        actor_id: ActorId,
        executor: String,
        mode: ActorMode,
        spawned_at: u64,
    },

    /// Actor checkpointed
    Checkpointed {
        actor_id: ActorId,
        executor: String,
        sequence: u64,
        state_size_bytes: usize,
        checkpointed_at: u64,
    },

    /// Actor migrating between executors
    Migrating {
        actor_id: ActorId,
        from_executor: String,
        to_executor: String,
        reason: MigrationReason,
        initiated_at: u64,
    },

    /// Migration completed successfully
    MigrationCompleted {
        actor_id: ActorId,
        to_executor: String,
        duration_ms: u64,
        completed_at: u64,
    },

    /// Migration failed
    MigrationFailed {
        actor_id: ActorId,
        from_executor: String,
        to_executor: String,
        reason: String,
        failed_at: u64,
    },

    /// Actor terminated
    Terminated {
        actor_id: ActorId,
        executor: String,
        reason: TerminationReason,
        terminated_at: u64,
    },
}

/// Actor runtime state for migration decisions.
///
/// Collected by executor to evaluate migration triggers.
#[derive(Debug, Clone)]
pub struct ActorRuntimeState {
    /// Actor ID
    pub actor_id: ActorId,

    /// How long actor has been running (seconds)
    pub uptime_secs: u64,

    /// Current memory usage (bytes)
    pub memory_bytes: u64,

    /// CPU utilization (0.0 - 1.0)
    pub cpu_utilization: f64,

    /// Messages processed per second
    pub msg_throughput: f64,

    /// Last checkpoint timestamp (Unix milliseconds)
    pub last_checkpoint_at: u64,

    /// Checkpoint sequence number
    pub checkpoint_sequence: u64,

    /// Required input blobs (for locality evaluation)
    pub required_blobs: Vec<[u8; 32]>,
}

/// Migration state machine per actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationState {
    /// No migration in progress
    Idle,

    /// Waiting for target executor to accept
    Requesting {
        target: String,
        sent_at: u64,
    },

    /// Target accepted, source creating final checkpoint
    Checkpointing {
        target: String,
        started_at: u64,
    },

    /// Transferring checkpoint to target
    Transferring {
        target: String,
        checkpoint_sequence: u64,
        started_at: u64,
    },

    /// Target loading checkpoint and resuming actor
    Restoring {
        target: String,
        started_at: u64,
    },

    /// Migration complete
    Complete {
        new_executor: String,
        completed_at: u64,
    },

    /// Migration failed (actor remains on source)
    Failed {
        reason: String,
        failed_at: u64,
    },
}

impl MigrationState {
    /// Check if migration is in progress
    pub fn is_active(&self) -> bool {
        !matches!(self, MigrationState::Idle | MigrationState::Complete { .. } | MigrationState::Failed { .. })
    }

    /// Get target executor if migration active
    pub fn target_executor(&self) -> Option<&str> {
        match self {
            MigrationState::Requesting { target, .. }
            | MigrationState::Checkpointing { target, .. }
            | MigrationState::Transferring { target, .. }
            | MigrationState::Restoring { target, .. } => Some(target.as_str()),
            _ => None,
        }
    }
}

/// Migration decision from policy evaluation.
#[derive(Debug, Clone)]
pub struct MigrationDecision {
    /// Actor ID to migrate
    pub actor_id: ActorId,

    /// Current executor
    pub from_executor: String,

    /// Target executor DID
    pub target_executor: String,

    /// Reason for migration
    pub reason: MigrationReason,

    /// Estimated migration cost (network transfer + downtime, milliseconds)
    pub cost_estimate_ms: u64,

    /// Expected benefit (0.0 - 10.0 scale, higher = more beneficial)
    pub benefit_score: f64,
}

impl MigrationDecision {
    /// Check if migration is worth the cost (benefit > threshold)
    pub fn is_beneficial(&self, min_benefit: f64) -> bool {
        self.benefit_score >= min_benefit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_mode_default() {
        let mode = ActorMode::default();
        assert_eq!(mode, ActorMode::Ephemeral);
    }

    #[test]
    fn test_checkpoint_signing_and_verification() {
        // Generate keypair for testing
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did();
        let signing_key_bytes = keypair.to_signing_key_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

        // Create checkpoint
        let state = b"actor state data".to_vec();
        let state_hash = ActorCheckpoint::compute_state_hash(&state);

        let mut checkpoint = ActorCheckpoint {
            actor_id: [1u8; 32],
            sequence: 5,
            state: state.clone(),
            created_at: 1234567890,
            executor: executor_did.to_string(),
            state_hash,
            signature: vec![],
        };

        // Sign checkpoint
        checkpoint.sign(&signing_key);

        // Verify signature is not empty
        assert!(!checkpoint.signature.is_empty());
        assert_eq!(checkpoint.signature.len(), 64); // Ed25519 signature size

        // Verify signature
        assert!(checkpoint.verify_signature(executor_did).is_ok());
    }

    #[test]
    fn test_checkpoint_verification_fails_wrong_did() {
        // Generate two different keypairs
        let signer_keypair = icn_identity::KeyPair::generate().unwrap();
        let wrong_keypair = icn_identity::KeyPair::generate().unwrap();

        let signing_key_bytes = signer_keypair.to_signing_key_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

        // Create and sign checkpoint
        let state = b"actor state".to_vec();
        let state_hash = ActorCheckpoint::compute_state_hash(&state);

        let mut checkpoint = ActorCheckpoint {
            actor_id: [2u8; 32],
            sequence: 1,
            state,
            created_at: 9999,
            executor: signer_keypair.did().to_string(),
            state_hash,
            signature: vec![],
        };

        checkpoint.sign(&signing_key);

        // Try to verify with wrong DID - should fail
        assert!(checkpoint.verify_signature(wrong_keypair.did()).is_err());
    }

    #[test]
    fn test_checkpoint_verification_fails_tampered_state() {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let signing_key_bytes = keypair.to_signing_key_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

        let state = b"original state".to_vec();
        let state_hash = ActorCheckpoint::compute_state_hash(&state);

        let mut checkpoint = ActorCheckpoint {
            actor_id: [3u8; 32],
            sequence: 10,
            state,
            created_at: 5555,
            executor: keypair.did().to_string(),
            state_hash,
            signature: vec![],
        };

        checkpoint.sign(&signing_key);

        // Tamper with state after signing
        checkpoint.sequence = 11;

        // Verification should fail
        assert!(checkpoint.verify_signature(keypair.did()).is_err());
    }

    #[test]
    fn test_migration_state_is_active() {
        assert!(!MigrationState::Idle.is_active());
        assert!(MigrationState::Requesting {
            target: "did:icn:target".to_string(),
            sent_at: 1000
        }
        .is_active());
        assert!(MigrationState::Checkpointing {
            target: "did:icn:target".to_string(),
            started_at: 1500,
        }
        .is_active());
        assert!(!MigrationState::Complete {
            new_executor: "did:icn:target".to_string(),
            completed_at: 2000
        }
        .is_active());
        assert!(!MigrationState::Failed {
            reason: "timeout".to_string(),
            failed_at: 3000
        }
        .is_active());
    }

    #[test]
    fn test_migration_state_target_executor() {
        let requesting = MigrationState::Requesting {
            target: "did:icn:target".to_string(),
            sent_at: 1000,
        };
        assert_eq!(requesting.target_executor(), Some("did:icn:target"));

        let idle = MigrationState::Idle;
        assert_eq!(idle.target_executor(), None);

        let complete = MigrationState::Complete {
            new_executor: "did:icn:target".to_string(),
            completed_at: 2000,
        };
        assert_eq!(complete.target_executor(), None);
    }

    #[test]
    fn test_migration_decision_is_beneficial() {
        let decision = MigrationDecision {
            actor_id: [4u8; 32],
            from_executor: "did:icn:source".to_string(),
            target_executor: "did:icn:target".to_string(),
            reason: MigrationReason::LoadBalancing,
            cost_estimate_ms: 500,
            benefit_score: 7.5,
        };

        assert!(decision.is_beneficial(5.0)); // 7.5 >= 5.0
        assert!(!decision.is_beneficial(8.0)); // 7.5 < 8.0
    }

    #[test]
    fn test_actor_checkpoint_state_hash() {
        let state = b"test state data";
        let hash1 = ActorCheckpoint::compute_state_hash(state);
        let hash2 = ActorCheckpoint::compute_state_hash(state);

        // Hash should be deterministic
        assert_eq!(hash1, hash2);

        // Different state should produce different hash
        let different_state = b"different state";
        let hash3 = ActorCheckpoint::compute_state_hash(different_state);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_checkpoint_signing_payload_deterministic() {
        let state = b"state".to_vec();
        let state_hash = ActorCheckpoint::compute_state_hash(&state);

        let checkpoint = ActorCheckpoint {
            actor_id: [5u8; 32],
            sequence: 42,
            state,
            created_at: 123456,
            executor: "did:icn:test".to_string(),
            state_hash,
            signature: vec![],
        };

        let payload1 = checkpoint.signing_payload();
        let payload2 = checkpoint.signing_payload();

        assert_eq!(payload1, payload2);
        assert!(!payload1.is_empty());
    }
}
