//! Migration and checkpoint handlers for ComputeActor.
//!
//! Phase 16D: Actor state persistence and migration.

use super::{ComputeActor, ComputeMessage};
use crate::error::ComputeError;

impl ComputeActor {
    /// Handle checkpoint announcement (Phase 16D)
    pub(super) async fn on_checkpoint_announce(
        &self,
        checkpoint: crate::actor_model::ActorCheckpoint,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(checkpoint.actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            sequence = checkpoint.sequence,
            executor = %checkpoint.executor,
            "Received checkpoint announcement"
        );

        // If we have a checkpoint store, verify and store the checkpoint
        if let Some(ref store) = self.checkpoint_store {
            // Verify signature and state hash
            if let Err(e) = store.verify(&checkpoint) {
                tracing::warn!(
                    actor_id = %actor_id_str,
                    executor = %checkpoint.executor,
                    error = %e,
                    "Checkpoint verification failed"
                );
                return Err(e);
            }

            // Store checkpoint (will reject if stale)
            match store.store(checkpoint).await {
                Ok(true) => {
                    tracing::debug!(
                        actor_id = %actor_id_str,
                        "Checkpoint stored successfully"
                    );
                }
                Ok(false) => {
                    tracing::debug!(
                        actor_id = %actor_id_str,
                        "Checkpoint rejected (stale)"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        actor_id = %actor_id_str,
                        error = %e,
                        "Failed to store checkpoint"
                    );
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Handle checkpoint query (Phase 16D)
    pub(super) async fn on_checkpoint_query(
        &self,
        actor_id: crate::actor_model::ActorId,
        requester: String,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            requester = %requester,
            "Received checkpoint query"
        );

        // If we have a checkpoint store, try to retrieve and respond
        if let Some(ref store) = self.checkpoint_store {
            let checkpoint = store.get(&actor_id).await?;

            // Send response via gossip
            if let Some(ref cb) = self.send_callback {
                cb(ComputeMessage::CheckpointResponse {
                    actor_id,
                    checkpoint,
                });
            }
        }

        Ok(())
    }

    /// Handle checkpoint response (Phase 16D)
    pub(super) async fn on_checkpoint_response(
        &self,
        actor_id: crate::actor_model::ActorId,
        checkpoint: Option<crate::actor_model::ActorCheckpoint>,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            has_checkpoint = checkpoint.is_some(),
            "Received checkpoint response"
        );

        // If we have a checkpoint and a store, store it
        if let (Some(checkpoint), Some(ref store)) = (checkpoint, &self.checkpoint_store) {
            // Verify and store
            store.verify(&checkpoint)?;
            store.store(checkpoint).await?;
        }

        Ok(())
    }

    /// Handle migration request (Phase 16D)
    pub(super) async fn on_migration_request(
        &self,
        actor_id: crate::actor_model::ActorId,
        from_executor: String,
        to_executor: String,
        checkpoint: crate::actor_model::ActorCheckpoint,
        reason: crate::actor_model::MigrationReason,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            from = %from_executor,
            to = %to_executor,
            reason = ?reason,
            "Received migration request"
        );

        // Only handle if we're the target executor
        if to_executor != self.own_did {
            return Ok(());
        }

        // Delegate to migration manager if available
        if let Some(ref manager) = self.migration_manager {
            // Get current capacity for policy evaluation
            let capacity = crate::scheduler::NodeCapacity {
                cpu_cores_total: 8.0,
                cpu_cores_available: 8.0,
                memory_mb_total: 16384,
                memory_mb_available: 16384,
                storage_mb_available: 100_000,
                network_mbps: 1000.0,
                gpu_devices: vec![],
                updated_at: icn_time::current_timestamp_millis(),
            };

            // Get current queue depth
            let registry = self.executor_registry.lock().await;
            let queue_depth = registry
                .get(&self.own_did)
                .map(|i| i.tasks_executing)
                .unwrap_or(0);
            drop(registry);

            manager
                .handle_migration_request(
                    actor_id,
                    from_executor,
                    checkpoint,
                    reason,
                    &capacity,
                    queue_depth,
                )
                .await?;
        }

        Ok(())
    }

    /// Handle migration accept (Phase 16D)
    pub(super) async fn on_migration_accept(
        &self,
        actor_id: crate::actor_model::ActorId,
        to_executor: String,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            to = %to_executor,
            "Received migration accept"
        );

        // Delegate to migration manager if available
        if let Some(ref manager) = self.migration_manager {
            // Generate signing key for checkpoint
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);

            manager
                .handle_migration_accept(actor_id, to_executor, &signing_key)
                .await?;
        }

        Ok(())
    }

    /// Handle migration reject (Phase 16D)
    pub(super) async fn on_migration_reject(
        &self,
        actor_id: crate::actor_model::ActorId,
        to_executor: String,
        reason: String,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            to = %to_executor,
            reason = %reason,
            "Received migration reject"
        );

        // Delegate to migration manager if available
        if let Some(ref manager) = self.migration_manager {
            manager
                .handle_migration_reject(actor_id, to_executor, reason)
                .await?;
        }

        Ok(())
    }

    /// Handle migration complete (Phase 16D)
    pub(super) async fn on_migration_complete(
        &self,
        actor_id: crate::actor_model::ActorId,
        from_executor: String,
        to_executor: String,
        final_checkpoint: crate::actor_model::ActorCheckpoint,
        duration_ms: u64,
    ) -> Result<(), ComputeError> {
        let actor_id_str = hex::encode(actor_id);

        tracing::debug!(
            actor_id = %actor_id_str,
            from = %from_executor,
            to = %to_executor,
            duration_ms = duration_ms,
            "Received migration complete"
        );

        // Only handle if we're the target executor
        if to_executor != self.own_did {
            return Ok(());
        }

        // Delegate to migration manager if available
        if let Some(ref manager) = self.migration_manager {
            manager
                .handle_migration_complete(actor_id, from_executor, final_checkpoint, duration_ms)
                .await?;
        }

        Ok(())
    }
}
