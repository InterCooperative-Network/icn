//! Actor migration coordination and state machine management.
//!
//! This module orchestrates actor migrations between executors, handling:
//! - Migration decision evaluation (via policy)
//! - Request/Accept/Reject handshake protocol
//! - Checkpoint creation and transfer
//! - Actor pause/resume coordination
//! - Cleanup and metrics
//!
//! # Migration Flow
//!
//! ```text
//! Source Executor                     Target Executor
//! ===============                     ===============
//!
//! 1. Evaluate migration
//!    (policy.should_migrate)
//!         |
//!         v
//! 2. Create checkpoint
//!         |
//!         v
//! 3. Send MigrationRequest -------->  4. Evaluate capacity
//!         |                               (policy.should_accept)
//!         |                                     |
//!         |                                     v
//!         |                          5. Send Accept/Reject
//!         |  <-----------------------------    |
//!         v
//! 6. Pause actor
//!         |
//!         v
//! 7. Final checkpoint
//!         |
//!         v
//! 8. Send MigrationComplete ------>  9. Resume actor
//!         |                               |
//!         v                               v
//! 10. Cleanup                        11. Acknowledge
//! ```

use crate::actor_model::{
    ActorCheckpoint, ActorId, ActorRuntimeState, MigrationDecision, MigrationReason, MigrationState,
};
use crate::actor_runtime::{ActorRuntimeCallback, ActorRuntimeCommand, PauseReason};
use crate::checkpoint_store::CheckpointStore;
use crate::error::ComputeError;
use crate::migration_policy::MigrationPolicy;
use crate::types::ComputeMessage;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Type alias for migration message sender (gossip callback)
pub type MigrationSender = Arc<dyn Fn(ComputeMessage) -> Result<(), ComputeError> + Send + Sync>;

/// Actor migration coordinator.
///
/// Manages migration lifecycle for all actors on this executor.
pub struct ActorMigrationManager {
    /// Current migration state per actor
    migrations: Arc<RwLock<HashMap<ActorId, MigrationState>>>,

    /// Migration policy
    policy: Arc<dyn MigrationPolicy>,

    /// Checkpoint store
    checkpoints: Arc<CheckpointStore>,

    /// Sender for migration messages
    send_message: MigrationSender,

    /// This executor's DID
    own_did: String,

    /// Callback to control actor runtime (pause/resume)
    actor_runtime: Option<ActorRuntimeCallback>,
}

impl ActorMigrationManager {
    /// Create new migration manager.
    pub fn new(
        policy: Arc<dyn MigrationPolicy>,
        checkpoints: Arc<CheckpointStore>,
        send_message: MigrationSender,
        own_did: String,
    ) -> Self {
        Self {
            migrations: Arc::new(RwLock::new(HashMap::new())),
            policy,
            checkpoints,
            send_message,
            own_did,
            actor_runtime: None,
        }
    }

    /// Set actor runtime callback for pause/resume operations.
    pub fn set_actor_runtime(&mut self, callback: ActorRuntimeCallback) {
        self.actor_runtime = Some(callback);
    }

    /// Pause an actor for migration.
    fn pause_actor(&self, actor_id: ActorId) -> Result<(), ComputeError> {
        if let Some(ref callback) = self.actor_runtime {
            callback(ActorRuntimeCommand::Pause {
                actor_id,
                reason: PauseReason::MigrationPending,
            })?;
            tracing::debug!(
                actor_id = %hex::encode(actor_id),
                "Actor paused for migration"
            );
        } else {
            tracing::warn!(
                actor_id = %hex::encode(actor_id),
                "No actor runtime callback set, skipping pause"
            );
        }
        Ok(())
    }

    /// Resume a paused actor (e.g., after migration rejection).
    fn resume_actor(&self, actor_id: ActorId) -> Result<(), ComputeError> {
        if let Some(ref callback) = self.actor_runtime {
            callback(ActorRuntimeCommand::Resume { actor_id })?;
            tracing::debug!(
                actor_id = %hex::encode(actor_id),
                "Actor resumed after migration cancellation"
            );
        } else {
            tracing::warn!(
                actor_id = %hex::encode(actor_id),
                "No actor runtime callback set, skipping resume"
            );
        }
        Ok(())
    }

    /// Restore an actor from checkpoint (target executor).
    fn restore_actor(
        &self,
        actor_id: ActorId,
        checkpoint: ActorCheckpoint,
    ) -> Result<(), ComputeError> {
        if let Some(ref callback) = self.actor_runtime {
            callback(ActorRuntimeCommand::Restore {
                actor_id,
                checkpoint,
            })?;
            tracing::debug!(
                actor_id = %hex::encode(actor_id),
                "Actor restored from checkpoint"
            );
        } else {
            tracing::warn!(
                actor_id = %hex::encode(actor_id),
                "No actor runtime callback set, skipping restore"
            );
        }
        Ok(())
    }

    /// Evaluate if actor should migrate (called periodically).
    ///
    /// Returns `Some(decision)` if migration recommended, `None` otherwise.
    pub async fn evaluate_migration(
        &self,
        actor_state: ActorRuntimeState,
        network_state: &crate::migration_policy::NetworkState,
    ) -> Option<MigrationDecision> {
        let actor_id = actor_state.actor_id;

        // Check if already migrating
        let migrations = self.migrations.read().await;
        if let Some(state) = migrations.get(&actor_id) {
            if state.is_active() {
                tracing::debug!(
                    actor_id = %hex::encode(actor_id),
                    state = ?state,
                    "Migration already in progress, skipping evaluation"
                );
                return None;
            }
        }
        drop(migrations);

        // Query policy
        let decision =
            self.policy
                .should_migrate(&actor_id, &self.own_did, &actor_state, network_state)?;

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            target = %decision.target_executor,
            reason = ?decision.reason,
            benefit_score = decision.benefit_score,
            cost_ms = decision.cost_estimate_ms,
            "Migration decision: should migrate"
        );

        Some(decision)
    }

    /// Initiate migration (source executor).
    ///
    /// Creates checkpoint and sends `MigrationRequest` to target.
    pub async fn initiate_migration(
        &self,
        decision: MigrationDecision,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<(), ComputeError> {
        let actor_id = decision.actor_id;
        let target = decision.target_executor.clone();

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            target = %target,
            reason = ?decision.reason,
            "Initiating migration"
        );

        // Update state to Requesting
        let mut migrations = self.migrations.write().await;
        migrations.insert(
            actor_id,
            MigrationState::Requesting {
                target: target.clone(),
                sent_at: now_millis(),
            },
        );
        drop(migrations);

        // Get latest checkpoint (or create one)
        let checkpoint = match self.checkpoints.get(&actor_id).await? {
            Some(cp) => cp,
            None => {
                // Create initial checkpoint
                self.create_checkpoint(actor_id, vec![], signing_key)
                    .await?
            }
        };

        // Send migration request
        let msg = ComputeMessage::MigrationRequest {
            actor_id,
            from_executor: self.own_did.clone(),
            to_executor: target.clone(),
            checkpoint: checkpoint.clone(),
            reason: decision.reason,
        };

        (self.send_message)(msg)?;

        tracing::debug!(
            actor_id = %hex::encode(actor_id),
            target = %target,
            checkpoint_seq = checkpoint.sequence,
            "Migration request sent"
        );

        Ok(())
    }

    /// Handle incoming migration request (target executor).
    ///
    /// Evaluates acceptance and sends Accept/Reject message.
    pub async fn handle_migration_request(
        &self,
        actor_id: ActorId,
        from_executor: String,
        checkpoint: ActorCheckpoint,
        reason: MigrationReason,
        current_capacity: &crate::scheduler::NodeCapacity,
        current_queue_depth: usize,
    ) -> Result<(), ComputeError> {
        tracing::info!(
            actor_id = %hex::encode(actor_id),
            from = %from_executor,
            reason = ?reason,
            checkpoint_seq = checkpoint.sequence,
            "Received migration request"
        );

        // Verify checkpoint signature
        self.checkpoints.verify(&checkpoint)?;

        // Store checkpoint
        self.checkpoints.store(checkpoint.clone()).await?;

        // Evaluate acceptance
        let estimated_memory = checkpoint.state.len() as u64;
        let should_accept = self.policy.should_accept_migration(
            &actor_id,
            &from_executor,
            estimated_memory,
            current_capacity,
            current_queue_depth,
        );

        if should_accept {
            tracing::info!(
                actor_id = %hex::encode(actor_id),
                from = %from_executor,
                "Accepting migration"
            );

            // Update state to Restoring
            let mut migrations = self.migrations.write().await;
            migrations.insert(
                actor_id,
                MigrationState::Restoring {
                    target: self.own_did.clone(),
                    started_at: now_millis(),
                },
            );
            drop(migrations);

            // Send accept
            let msg = ComputeMessage::MigrationAccept {
                actor_id,
                to_executor: self.own_did.clone(),
            };

            (self.send_message)(msg)?;
        } else {
            tracing::warn!(
                actor_id = %hex::encode(actor_id),
                from = %from_executor,
                "Rejecting migration (insufficient capacity or queue too deep)"
            );

            // Send reject
            let msg = ComputeMessage::MigrationReject {
                actor_id,
                to_executor: self.own_did.clone(),
                reason: "Insufficient capacity or queue too deep".into(),
            };

            (self.send_message)(msg)?;
        }

        Ok(())
    }

    /// Handle migration acceptance (source executor).
    ///
    /// Pauses actor, creates final checkpoint, sends completion.
    pub async fn handle_migration_accept(
        &self,
        actor_id: ActorId,
        to_executor: String,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<(), ComputeError> {
        tracing::info!(
            actor_id = %hex::encode(actor_id),
            target = %to_executor,
            "Migration accepted by target"
        );

        // Update state to Checkpointing
        let mut migrations = self.migrations.write().await;
        migrations.insert(
            actor_id,
            MigrationState::Checkpointing {
                target: to_executor.clone(),
                started_at: now_millis(),
            },
        );
        drop(migrations);

        // Pause actor execution before creating final checkpoint
        self.pause_actor(actor_id)?;

        // Create final checkpoint
        let final_checkpoint = self
            .create_checkpoint(actor_id, vec![], signing_key)
            .await?;

        // Update state to Transferring
        let mut migrations = self.migrations.write().await;
        migrations.insert(
            actor_id,
            MigrationState::Transferring {
                target: to_executor.clone(),
                checkpoint_sequence: final_checkpoint.sequence,
                started_at: now_millis(),
            },
        );
        drop(migrations);

        // Calculate duration
        let migrations = self.migrations.read().await;
        let duration_ms =
            if let Some(MigrationState::Requesting { sent_at, .. }) = migrations.get(&actor_id) {
                now_millis().saturating_sub(*sent_at)
            } else {
                0
            };
        drop(migrations);

        // Send completion
        let msg = ComputeMessage::MigrationComplete {
            actor_id,
            from_executor: self.own_did.clone(),
            to_executor: to_executor.clone(),
            final_checkpoint,
            duration_ms,
        };

        (self.send_message)(msg)?;

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            target = %to_executor,
            duration_ms,
            "Migration complete message sent"
        );

        Ok(())
    }

    /// Handle migration rejection (source executor).
    pub async fn handle_migration_reject(
        &self,
        actor_id: ActorId,
        to_executor: String,
        reason: String,
    ) -> Result<(), ComputeError> {
        tracing::warn!(
            actor_id = %hex::encode(actor_id),
            target = %to_executor,
            reason = %reason,
            "Migration rejected by target"
        );

        // Update state to Failed
        let mut migrations = self.migrations.write().await;
        migrations.insert(
            actor_id,
            MigrationState::Failed {
                reason: reason.clone(),
                failed_at: now_millis(),
            },
        );
        drop(migrations);

        // Resume actor execution since migration was rejected
        self.resume_actor(actor_id)?;

        Ok(())
    }

    /// Handle migration completion (target executor).
    ///
    /// Stores final checkpoint and resumes actor.
    pub async fn handle_migration_complete(
        &self,
        actor_id: ActorId,
        from_executor: String,
        final_checkpoint: ActorCheckpoint,
        duration_ms: u64,
    ) -> Result<(), ComputeError> {
        tracing::info!(
            actor_id = %hex::encode(actor_id),
            from = %from_executor,
            duration_ms,
            checkpoint_seq = final_checkpoint.sequence,
            "Migration completed"
        );

        // Verify final checkpoint
        self.checkpoints.verify(&final_checkpoint)?;

        // Store final checkpoint
        self.checkpoints.store(final_checkpoint.clone()).await?;

        // Update state to Complete
        let mut migrations = self.migrations.write().await;
        migrations.insert(
            actor_id,
            MigrationState::Complete {
                new_executor: self.own_did.clone(),
                completed_at: now_millis(),
            },
        );
        drop(migrations);

        // Restore and resume actor execution with checkpoint state
        self.restore_actor(actor_id, final_checkpoint)?;

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            "Actor migrated successfully and resumed"
        );

        Ok(())
    }

    /// Cleanup completed/failed migrations (called periodically).
    ///
    /// Removes migrations older than `max_age_secs`.
    pub async fn cleanup_migrations(&self, max_age_secs: u64) -> usize {
        let now = now_millis();
        let max_age_ms = max_age_secs * 1000;

        let mut migrations = self.migrations.write().await;
        let before_count = migrations.len();

        migrations.retain(|_actor_id, state| match state {
            MigrationState::Complete { completed_at, .. } => {
                now.saturating_sub(*completed_at) < max_age_ms
            }
            MigrationState::Failed { failed_at, .. } => now.saturating_sub(*failed_at) < max_age_ms,
            _ => true, // Keep active migrations
        });

        let removed = before_count - migrations.len();

        if removed > 0 {
            tracing::debug!(
                removed,
                remaining = migrations.len(),
                "Cleaned up old migration records"
            );
        }

        removed
    }

    /// Detect and fail timed-out migrations.
    ///
    /// Checks for migrations stuck in transient states and marks them as failed.
    /// Returns number of migrations timed out.
    pub async fn detect_timeouts(&self, timeout_secs: u64) -> Result<usize, ComputeError> {
        let now = now_millis();
        let timeout_ms = timeout_secs * 1000;
        let mut timed_out = Vec::new();

        // Find timed-out migrations
        {
            let migrations = self.migrations.read().await;
            for (actor_id, state) in migrations.iter() {
                let is_timeout = match state {
                    MigrationState::Requesting { sent_at, .. } => {
                        now.saturating_sub(*sent_at) > timeout_ms
                    }
                    MigrationState::Checkpointing { started_at, .. } => {
                        // Checkpointing should be fast (seconds), use timeout
                        now.saturating_sub(*started_at) > timeout_ms
                    }
                    MigrationState::Transferring { started_at, .. } => {
                        // Checkpoint transfer can take longer for large states
                        now.saturating_sub(*started_at) > timeout_ms
                    }
                    MigrationState::Restoring { started_at, .. } => {
                        // Restoring should be fast (seconds), use timeout
                        now.saturating_sub(*started_at) > timeout_ms
                    }
                    _ => false,
                };

                if is_timeout {
                    timed_out.push((*actor_id, state.clone()));
                }
            }
        }

        // Mark timed-out migrations as failed
        if !timed_out.is_empty() {
            let mut migrations = self.migrations.write().await;
            for (actor_id, old_state) in &timed_out {
                let target = match old_state {
                    MigrationState::Requesting { target, .. } => target.clone(),
                    _ => "unknown".to_string(),
                };

                tracing::warn!(
                    actor_id = %hex::encode(actor_id),
                    target = %target,
                    state = ?old_state,
                    "Migration timed out"
                );

                migrations.insert(
                    *actor_id,
                    MigrationState::Failed {
                        reason: "Migration timed out".to_string(),
                        failed_at: now,
                    },
                );
            }
        }

        Ok(timed_out.len())
    }

    /// Get migration state for actor.
    pub async fn get_state(&self, actor_id: &ActorId) -> Option<MigrationState> {
        let migrations = self.migrations.read().await;
        migrations.get(actor_id).cloned()
    }

    /// Create checkpoint for actor.
    async fn create_checkpoint(
        &self,
        actor_id: ActorId,
        state: Vec<u8>,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<ActorCheckpoint, ComputeError> {
        let sequence = self.checkpoints.next_sequence(&actor_id).await;
        let state_hash = ActorCheckpoint::compute_state_hash(&state);

        let mut checkpoint = ActorCheckpoint {
            actor_id,
            sequence,
            state,
            created_at: now_millis(),
            executor: self.own_did.clone(),
            state_hash,
            signature: vec![],
        };

        checkpoint.sign(signing_key);

        // Store checkpoint
        self.checkpoints.store(checkpoint.clone()).await?;

        Ok(checkpoint)
    }
}

/// Get current time in milliseconds since UNIX epoch.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint_store::InMemoryBackend;
    use crate::migration_policy::{DefaultMigrationPolicy, ExecutorInfo, NetworkState};
    use crate::scheduler::NodeCapacity;
    use std::sync::Mutex;

    fn make_manager(own_did: &str) -> (ActorMigrationManager, Arc<Mutex<Vec<ComputeMessage>>>) {
        let policy = Arc::new(DefaultMigrationPolicy::default());
        let backend = Arc::new(InMemoryBackend::new());
        let checkpoints = Arc::new(CheckpointStore::new(backend));

        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent_messages.clone();

        let send_message: MigrationSender = Arc::new(move |msg| {
            sent_clone.lock().unwrap().push(msg);
            Ok(())
        });

        let manager =
            ActorMigrationManager::new(policy, checkpoints, send_message, own_did.to_string());

        (manager, sent_messages)
    }

    fn make_capacity() -> NodeCapacity {
        NodeCapacity {
            cpu_cores_total: 8.0,
            cpu_cores_available: 6.0,
            memory_mb_total: 16384,
            memory_mb_available: 12288,
            storage_mb_available: 100_000,
            network_mbps: 1000.0,
            gpu_devices: vec![],
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn test_evaluate_migration() {
        let (manager, _) = make_manager("did:icn:executor-a");

        let actor_state = ActorRuntimeState {
            actor_id: [1u8; 32],
            uptime_secs: 60,
            memory_bytes: 1024 * 1024,
            cpu_utilization: 0.5,
            msg_throughput: 10.0,
            last_checkpoint_at: 1000,
            checkpoint_sequence: 0,
            required_blobs: vec![],
        };

        // Overloaded executor -> should recommend migration
        let network_state = NetworkState {
            executor_load: 0.90, // Above 80% threshold
            available_executors: vec![ExecutorInfo {
                did: "did:icn:executor-b".to_string(),
                trust_score: 0.9,
                load: 0.10, // Much lower load
                capacity: make_capacity(),
                queue_depth: 0,
                last_seen: 1000,
            }],
            executor_rtt: HashMap::new(),
            blob_locations: HashMap::new(),
            snapshot_at: 1000,
        };

        let decision = manager
            .evaluate_migration(actor_state, &network_state)
            .await;

        assert!(decision.is_some());
        let dec = decision.unwrap();
        assert_eq!(dec.target_executor, "did:icn:executor-b");
    }

    #[tokio::test]
    async fn test_initiate_migration() {
        let (manager, sent_messages) = make_manager("did:icn:executor-a");

        let keypair = icn_identity::KeyPair::generate().unwrap();
        let signing_key_bytes = keypair.to_signing_key_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

        let decision = MigrationDecision {
            actor_id: [2u8; 32],
            from_executor: "did:icn:executor-a".to_string(),
            target_executor: "did:icn:executor-b".to_string(),
            reason: MigrationReason::LoadBalancing,
            cost_estimate_ms: 500,
            benefit_score: 5.0,
        };

        manager
            .initiate_migration(decision, &signing_key)
            .await
            .unwrap();

        // Check state
        let state = manager.get_state(&[2u8; 32]).await;
        assert!(matches!(state, Some(MigrationState::Requesting { .. })));

        // Check message sent
        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            &messages[0],
            ComputeMessage::MigrationRequest { .. }
        ));
    }

    #[tokio::test]
    async fn test_handle_migration_request_accept() {
        // Create manager with proper DID matching a keypair
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().to_string();
        let (manager, sent_messages) = make_manager(&executor_did);

        let signing_key_bytes = keypair.to_signing_key_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

        // Create checkpoint (will use manager's DID which matches keypair)
        let actor_id = [3u8; 32];
        let checkpoint = manager
            .create_checkpoint(actor_id, b"state".to_vec(), &signing_key)
            .await
            .unwrap();

        let capacity = make_capacity();

        manager
            .handle_migration_request(
                actor_id,
                "did:icn:executor-a".to_string(),
                checkpoint,
                MigrationReason::LoadBalancing,
                &capacity,
                5, // Low queue depth
            )
            .await
            .unwrap();

        // Check state
        let state = manager.get_state(&actor_id).await;
        assert!(matches!(state, Some(MigrationState::Restoring { .. })));

        // Check accept message sent
        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            &messages[0],
            ComputeMessage::MigrationAccept { .. }
        ));
    }

    #[tokio::test]
    async fn test_handle_migration_request_reject() {
        // Create manager with proper DID matching a keypair
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().to_string();
        let (manager, sent_messages) = make_manager(&executor_did);

        let signing_key_bytes = keypair.to_signing_key_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

        let actor_id = [4u8; 32];
        let checkpoint = manager
            .create_checkpoint(actor_id, b"state".to_vec(), &signing_key)
            .await
            .unwrap();

        let capacity = make_capacity();

        manager
            .handle_migration_request(
                actor_id,
                "did:icn:executor-a".to_string(),
                checkpoint,
                MigrationReason::LoadBalancing,
                &capacity,
                25, // High queue depth (>20 threshold)
            )
            .await
            .unwrap();

        // Check reject message sent
        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            &messages[0],
            ComputeMessage::MigrationReject { .. }
        ));
    }

    #[tokio::test]
    async fn test_cleanup_migrations() {
        let (manager, _) = make_manager("did:icn:executor-a");

        // Add completed migration
        let mut migrations = manager.migrations.write().await;
        migrations.insert(
            [5u8; 32],
            MigrationState::Complete {
                new_executor: "did:icn:executor-b".to_string(),
                completed_at: now_millis() - 10_000, // 10 seconds ago
            },
        );
        drop(migrations);

        // Cleanup with 5-second threshold
        let removed = manager.cleanup_migrations(5).await;
        assert_eq!(removed, 1);

        // Verify removed
        let state = manager.get_state(&[5u8; 32]).await;
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_detect_timeouts_requesting() {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().to_string();
        let (manager, _sent_messages) = make_manager(&executor_did);

        let actor_id = [6u8; 32];

        // Manually insert a migration in Requesting state with old timestamp
        {
            let mut migrations = manager.migrations.write().await;
            migrations.insert(
                actor_id,
                MigrationState::Requesting {
                    target: "did:icn:executor-b".to_string(),
                    sent_at: now_millis() - 120_000, // 2 minutes ago
                },
            );
        }

        // Detect timeouts with 60-second threshold
        let timed_out = manager.detect_timeouts(60).await.unwrap();
        assert_eq!(timed_out, 1);

        // Verify migration marked as failed
        let state = manager.get_state(&actor_id).await;
        assert!(matches!(state, Some(MigrationState::Failed { .. })));

        if let Some(MigrationState::Failed { reason, .. }) = state {
            assert_eq!(reason, "Migration timed out");
        }
    }

    #[tokio::test]
    async fn test_detect_timeouts_checkpointing() {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().to_string();
        let (manager, _sent_messages) = make_manager(&executor_did);

        let actor_id = [7u8; 32];

        // Insert migration in Checkpointing state with old timestamp
        {
            let mut migrations = manager.migrations.write().await;
            migrations.insert(
                actor_id,
                MigrationState::Checkpointing {
                    target: "did:icn:executor-b".to_string(),
                    started_at: now_millis() - 90_000, // 90 seconds ago
                },
            );
        }

        // Detect timeouts (60s threshold)
        let timed_out = manager.detect_timeouts(60).await.unwrap();
        assert_eq!(timed_out, 1);

        // Verify failure
        let state = manager.get_state(&actor_id).await;
        assert!(matches!(state, Some(MigrationState::Failed { .. })));
    }

    #[tokio::test]
    async fn test_detect_timeouts_transferring() {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().to_string();
        let (manager, _sent_messages) = make_manager(&executor_did);

        let actor_id = [8u8; 32];

        // Insert migration in Transferring state with old timestamp
        {
            let mut migrations = manager.migrations.write().await;
            migrations.insert(
                actor_id,
                MigrationState::Transferring {
                    target: "did:icn:executor-b".to_string(),
                    checkpoint_sequence: 5,
                    started_at: now_millis() - 75_000, // 75 seconds ago
                },
            );
        }

        // Detect timeouts (60s threshold)
        let timed_out = manager.detect_timeouts(60).await.unwrap();
        assert_eq!(timed_out, 1);

        // Verify failure
        let state = manager.get_state(&actor_id).await;
        assert!(matches!(state, Some(MigrationState::Failed { .. })));
    }

    #[tokio::test]
    async fn test_detect_timeouts_restoring() {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().to_string();
        let (manager, _sent_messages) = make_manager(&executor_did);

        let actor_id = [9u8; 32];

        // Insert migration in Restoring state with old timestamp
        {
            let mut migrations = manager.migrations.write().await;
            migrations.insert(
                actor_id,
                MigrationState::Restoring {
                    target: "did:icn:executor-b".to_string(),
                    started_at: now_millis() - 80_000, // 80 seconds ago
                },
            );
        }

        // Detect timeouts (60s threshold)
        let timed_out = manager.detect_timeouts(60).await.unwrap();
        assert_eq!(timed_out, 1);

        // Verify failure
        let state = manager.get_state(&actor_id).await;
        assert!(matches!(state, Some(MigrationState::Failed { .. })));
    }

    #[tokio::test]
    async fn test_detect_timeouts_multiple_states() {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor_did = keypair.did().to_string();
        let (manager, _sent_messages) = make_manager(&executor_did);

        // Insert multiple timed-out migrations in different states
        {
            let mut migrations = manager.migrations.write().await;
            let old_time = now_millis() - 100_000; // 100 seconds ago

            migrations.insert(
                [10u8; 32],
                MigrationState::Requesting {
                    target: "did:icn:a".to_string(),
                    sent_at: old_time,
                },
            );
            migrations.insert(
                [11u8; 32],
                MigrationState::Checkpointing {
                    target: "did:icn:b".to_string(),
                    started_at: old_time,
                },
            );
            migrations.insert(
                [12u8; 32],
                MigrationState::Transferring {
                    target: "did:icn:c".to_string(),
                    checkpoint_sequence: 1,
                    started_at: old_time,
                },
            );
            migrations.insert(
                [13u8; 32],
                MigrationState::Restoring {
                    target: "did:icn:d".to_string(),
                    started_at: old_time,
                },
            );
        }

        // Detect timeouts (60s threshold should catch all 4)
        let timed_out = manager.detect_timeouts(60).await.unwrap();
        assert_eq!(timed_out, 4);

        // Verify all marked as failed
        for actor_id in [[10u8; 32], [11u8; 32], [12u8; 32], [13u8; 32]] {
            let state = manager.get_state(&actor_id).await;
            assert!(matches!(state, Some(MigrationState::Failed { .. })));
        }
    }
}
