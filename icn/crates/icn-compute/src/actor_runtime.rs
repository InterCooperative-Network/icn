//! Stateful actor runtime for lifecycle management.
//!
//! This module provides pause/resume capabilities for long-running stateful actors
//! during migrations and checkpointing operations.
//!
//! # Actor States
//!
//! ```text
//! Running ──────► Paused ──────► Running
//!    │               │
//!    │               └───► Migrating ───► (transferred to target)
//!    │
//!    └───► Terminated
//! ```

use crate::actor_model::{ActorCheckpoint, ActorId, ActorMode};
use crate::error::ComputeError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// State of a stateful actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatefulActorState {
    /// Actor is actively processing messages.
    Running,

    /// Actor is paused (e.g., for checkpointing).
    /// Messages are queued until resumed.
    Paused {
        /// Reason for pause
        reason: PauseReason,
        /// When the actor was paused
        paused_at: u64,
    },

    /// Actor is being migrated to another executor.
    /// No new messages accepted, final checkpoint in progress.
    Migrating {
        /// Target executor DID
        target_executor: String,
        /// When migration started
        started_at: u64,
    },

    /// Actor has been terminated.
    Terminated {
        /// Reason for termination
        reason: String,
        /// When terminated
        terminated_at: u64,
    },
}

/// Reason for pausing an actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PauseReason {
    /// Paused for periodic checkpointing
    Checkpointing,
    /// Paused for migration preparation
    MigrationPending,
    /// Paused by user/admin request
    UserRequest,
    /// Paused due to resource constraints
    ResourceLimit,
}

/// Information about a running stateful actor.
#[derive(Debug, Clone)]
pub struct StatefulActorInfo {
    /// Actor ID
    pub actor_id: ActorId,

    /// Current state
    pub state: StatefulActorState,

    /// Actor mode configuration
    pub mode: ActorMode,

    /// Serialized actor state (latest)
    pub current_state: Vec<u8>,

    /// Last checkpoint sequence number
    pub last_checkpoint_seq: u64,

    /// When actor was spawned
    pub spawned_at: u64,

    /// Memory usage in bytes
    pub memory_bytes: u64,

    /// Number of messages processed
    pub messages_processed: u64,

    /// Queued messages (when paused)
    pub queued_messages: Vec<Vec<u8>>,
}

impl StatefulActorInfo {
    /// Create a new actor info for a freshly spawned actor.
    pub fn new(actor_id: ActorId, mode: ActorMode) -> Self {
        Self {
            actor_id,
            state: StatefulActorState::Running,
            mode,
            current_state: vec![],
            last_checkpoint_seq: 0,
            spawned_at: now_millis(),
            memory_bytes: 0,
            messages_processed: 0,
            queued_messages: vec![],
        }
    }

    /// Create actor info from a checkpoint (for restoration).
    pub fn from_checkpoint(checkpoint: &ActorCheckpoint, mode: ActorMode) -> Self {
        Self {
            actor_id: checkpoint.actor_id,
            state: StatefulActorState::Running,
            mode,
            current_state: checkpoint.state.clone(),
            last_checkpoint_seq: checkpoint.sequence,
            spawned_at: now_millis(),
            memory_bytes: checkpoint.state.len() as u64,
            messages_processed: 0,
            queued_messages: vec![],
        }
    }
}

/// Callback for actor runtime control operations.
/// Used by migration manager to coordinate with actor execution.
pub type ActorRuntimeCallback =
    Arc<dyn Fn(ActorRuntimeCommand) -> Result<(), ComputeError> + Send + Sync>;

/// Commands sent to the actor runtime.
#[derive(Debug, Clone)]
pub enum ActorRuntimeCommand {
    /// Pause actor execution
    Pause {
        actor_id: ActorId,
        reason: PauseReason,
    },

    /// Resume actor execution
    Resume { actor_id: ActorId },

    /// Restore actor from checkpoint
    Restore {
        actor_id: ActorId,
        checkpoint: ActorCheckpoint,
    },

    /// Terminate actor
    Terminate { actor_id: ActorId, reason: String },

    /// Get current state for checkpointing
    GetState { actor_id: ActorId },
}

/// Registry for managing stateful actors.
pub struct StatefulActorRegistry {
    /// Actors by ID
    actors: Arc<RwLock<HashMap<ActorId, StatefulActorInfo>>>,

    /// This executor's DID
    own_did: String,
}

impl StatefulActorRegistry {
    /// Create a new registry.
    pub fn new(own_did: String) -> Self {
        Self {
            actors: Arc::new(RwLock::new(HashMap::new())),
            own_did,
        }
    }

    /// Register a new stateful actor.
    pub async fn register(&self, actor_id: ActorId, mode: ActorMode) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        if actors.contains_key(&actor_id) {
            return Err(ComputeError::Internal(format!(
                "Actor {} already registered",
                hex::encode(actor_id)
            )));
        }

        let info = StatefulActorInfo::new(actor_id, mode);
        actors.insert(actor_id, info);

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            executor = %self.own_did,
            "Registered stateful actor"
        );

        Ok(())
    }

    /// Register an actor from a checkpoint (migration restore).
    pub async fn register_from_checkpoint(
        &self,
        checkpoint: &ActorCheckpoint,
        mode: ActorMode,
    ) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        let actor_id = checkpoint.actor_id;
        let info = StatefulActorInfo::from_checkpoint(checkpoint, mode);
        actors.insert(actor_id, info);

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            executor = %self.own_did,
            checkpoint_seq = checkpoint.sequence,
            state_size = checkpoint.state.len(),
            "Restored actor from checkpoint"
        );

        Ok(())
    }

    /// Pause an actor.
    pub async fn pause(&self, actor_id: ActorId, reason: PauseReason) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        let info = actors.get_mut(&actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        // Can only pause if running
        if info.state != StatefulActorState::Running {
            return Err(ComputeError::Internal(format!(
                "Cannot pause actor {} in state {:?}",
                hex::encode(actor_id),
                info.state
            )));
        }

        info.state = StatefulActorState::Paused {
            reason: reason.clone(),
            paused_at: now_millis(),
        };

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            reason = ?reason,
            "Paused actor"
        );

        Ok(())
    }

    /// Resume a paused actor.
    pub async fn resume(&self, actor_id: ActorId) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        let info = actors.get_mut(&actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        // Can only resume if paused
        match &info.state {
            StatefulActorState::Paused { .. } => {
                let queued_count = info.queued_messages.len();
                info.state = StatefulActorState::Running;

                tracing::info!(
                    actor_id = %hex::encode(actor_id),
                    queued_messages = queued_count,
                    "Resumed actor"
                );

                Ok(())
            }
            _ => Err(ComputeError::Internal(format!(
                "Cannot resume actor {} in state {:?}",
                hex::encode(actor_id),
                info.state
            ))),
        }
    }

    /// Mark actor as migrating.
    pub async fn start_migration(
        &self,
        actor_id: ActorId,
        target_executor: String,
    ) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        let info = actors.get_mut(&actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        // Can only migrate from paused state
        match &info.state {
            StatefulActorState::Paused { .. } => {
                info.state = StatefulActorState::Migrating {
                    target_executor: target_executor.clone(),
                    started_at: now_millis(),
                };

                tracing::info!(
                    actor_id = %hex::encode(actor_id),
                    target = %target_executor,
                    "Actor migration started"
                );

                Ok(())
            }
            _ => Err(ComputeError::Internal(format!(
                "Cannot migrate actor {} from state {:?}",
                hex::encode(actor_id),
                info.state
            ))),
        }
    }

    /// Complete migration (remove actor from source).
    pub async fn complete_migration(&self, actor_id: ActorId) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        let info = actors.get(&actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        // Can only complete if migrating
        if !matches!(info.state, StatefulActorState::Migrating { .. }) {
            return Err(ComputeError::Internal(format!(
                "Cannot complete migration for actor {} in state {:?}",
                hex::encode(actor_id),
                info.state
            )));
        }

        actors.remove(&actor_id);

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            "Actor migration completed, removed from source"
        );

        Ok(())
    }

    /// Terminate an actor.
    pub async fn terminate(&self, actor_id: ActorId, reason: String) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        let info = actors.get_mut(&actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        info.state = StatefulActorState::Terminated {
            reason: reason.clone(),
            terminated_at: now_millis(),
        };

        tracing::info!(
            actor_id = %hex::encode(actor_id),
            reason = %reason,
            "Terminated actor"
        );

        Ok(())
    }

    /// Get actor info.
    pub async fn get(&self, actor_id: &ActorId) -> Option<StatefulActorInfo> {
        let actors = self.actors.read().await;
        actors.get(actor_id).cloned()
    }

    /// Get actor state for checkpointing.
    pub async fn get_state(&self, actor_id: &ActorId) -> Result<Vec<u8>, ComputeError> {
        let actors = self.actors.read().await;

        let info = actors.get(actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        Ok(info.current_state.clone())
    }

    /// Update actor state (e.g., after processing a message).
    pub async fn update_state(
        &self,
        actor_id: &ActorId,
        new_state: Vec<u8>,
    ) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        let info = actors.get_mut(actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        info.memory_bytes = new_state.len() as u64;
        info.current_state = new_state;
        info.messages_processed += 1;

        Ok(())
    }

    /// Queue a message for a paused actor.
    pub async fn queue_message(
        &self,
        actor_id: &ActorId,
        message: Vec<u8>,
    ) -> Result<(), ComputeError> {
        let mut actors = self.actors.write().await;

        let info = actors.get_mut(actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        if !matches!(info.state, StatefulActorState::Paused { .. }) {
            return Err(ComputeError::Internal(format!(
                "Cannot queue message for actor {} in state {:?}",
                hex::encode(actor_id),
                info.state
            )));
        }

        info.queued_messages.push(message);
        Ok(())
    }

    /// Drain queued messages for a resumed actor.
    pub async fn drain_queued_messages(
        &self,
        actor_id: &ActorId,
    ) -> Result<Vec<Vec<u8>>, ComputeError> {
        let mut actors = self.actors.write().await;

        let info = actors.get_mut(actor_id).ok_or_else(|| {
            ComputeError::Internal(format!("Actor {} not found", hex::encode(actor_id)))
        })?;

        Ok(std::mem::take(&mut info.queued_messages))
    }

    /// List all actors.
    pub async fn list(&self) -> Vec<StatefulActorInfo> {
        let actors = self.actors.read().await;
        actors.values().cloned().collect()
    }

    /// Count actors by state.
    pub async fn count_by_state(&self) -> HashMap<String, usize> {
        let actors = self.actors.read().await;
        let mut counts = HashMap::new();

        for info in actors.values() {
            let state_name = match &info.state {
                StatefulActorState::Running => "running",
                StatefulActorState::Paused { .. } => "paused",
                StatefulActorState::Migrating { .. } => "migrating",
                StatefulActorState::Terminated { .. } => "terminated",
            };
            *counts.entry(state_name.to_string()).or_insert(0) += 1;
        }

        counts
    }

    /// Cleanup terminated actors older than max_age_secs.
    pub async fn cleanup(&self, max_age_secs: u64) -> usize {
        let now = now_millis();
        let max_age_ms = max_age_secs * 1000;

        let mut actors = self.actors.write().await;
        let before_count = actors.len();

        actors.retain(|_actor_id, info| {
            if let StatefulActorState::Terminated { terminated_at, .. } = &info.state {
                now.saturating_sub(*terminated_at) < max_age_ms
            } else {
                true
            }
        });

        before_count - actors.len()
    }

    /// Create a callback for migration manager integration.
    pub fn create_callback(&self) -> ActorRuntimeCallback {
        let actors = Arc::clone(&self.actors);

        Arc::new(move |command| {
            // Use tokio runtime to handle async operations
            let actors = Arc::clone(&actors);
            match command {
                ActorRuntimeCommand::Pause { actor_id, reason } => {
                    let handle = tokio::runtime::Handle::try_current()
                        .map_err(|e| ComputeError::Internal(e.to_string()))?;

                    handle.block_on(async {
                        let mut actors = actors.write().await;
                        let info = actors.get_mut(&actor_id).ok_or_else(|| {
                            ComputeError::Internal(format!(
                                "Actor {} not found",
                                hex::encode(actor_id)
                            ))
                        })?;

                        if info.state != StatefulActorState::Running {
                            return Err(ComputeError::Internal(format!(
                                "Cannot pause actor {} in state {:?}",
                                hex::encode(actor_id),
                                info.state
                            )));
                        }

                        info.state = StatefulActorState::Paused {
                            reason,
                            paused_at: now_millis(),
                        };

                        Ok(())
                    })
                }

                ActorRuntimeCommand::Resume { actor_id } => {
                    let handle = tokio::runtime::Handle::try_current()
                        .map_err(|e| ComputeError::Internal(e.to_string()))?;

                    handle.block_on(async {
                        let mut actors = actors.write().await;
                        let info = actors.get_mut(&actor_id).ok_or_else(|| {
                            ComputeError::Internal(format!(
                                "Actor {} not found",
                                hex::encode(actor_id)
                            ))
                        })?;

                        if !matches!(info.state, StatefulActorState::Paused { .. }) {
                            return Err(ComputeError::Internal(format!(
                                "Cannot resume actor {} in state {:?}",
                                hex::encode(actor_id),
                                info.state
                            )));
                        }

                        info.state = StatefulActorState::Running;
                        Ok(())
                    })
                }

                ActorRuntimeCommand::Restore {
                    actor_id,
                    checkpoint,
                } => {
                    let handle = tokio::runtime::Handle::try_current()
                        .map_err(|e| ComputeError::Internal(e.to_string()))?;

                    handle.block_on(async {
                        let mut actors = actors.write().await;

                        // Create new entry or update existing
                        let info = actors.entry(actor_id).or_insert_with(|| {
                            StatefulActorInfo::new(actor_id, ActorMode::default())
                        });

                        info.current_state = checkpoint.state;
                        info.last_checkpoint_seq = checkpoint.sequence;
                        info.state = StatefulActorState::Running;
                        info.memory_bytes = info.current_state.len() as u64;

                        Ok(())
                    })
                }

                ActorRuntimeCommand::Terminate { actor_id, reason } => {
                    let handle = tokio::runtime::Handle::try_current()
                        .map_err(|e| ComputeError::Internal(e.to_string()))?;

                    handle.block_on(async {
                        let mut actors = actors.write().await;
                        let info = actors.get_mut(&actor_id).ok_or_else(|| {
                            ComputeError::Internal(format!(
                                "Actor {} not found",
                                hex::encode(actor_id)
                            ))
                        })?;

                        info.state = StatefulActorState::Terminated {
                            reason,
                            terminated_at: now_millis(),
                        };

                        Ok(())
                    })
                }

                ActorRuntimeCommand::GetState { actor_id: _ } => {
                    // This is a query, not a command - would need different return type
                    // For now, this is a no-op as the migration manager gets state via checkpoint_store
                    Ok(())
                }
            }
        })
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

    fn test_actor_id() -> ActorId {
        [1u8; 32]
    }

    #[tokio::test]
    async fn test_register_actor() {
        let registry = StatefulActorRegistry::new("did:icn:test".to_string());
        let actor_id = test_actor_id();

        registry
            .register(actor_id, ActorMode::default())
            .await
            .unwrap();

        let info = registry.get(&actor_id).await.unwrap();
        assert_eq!(info.state, StatefulActorState::Running);
    }

    #[tokio::test]
    async fn test_register_duplicate_fails() {
        let registry = StatefulActorRegistry::new("did:icn:test".to_string());
        let actor_id = test_actor_id();

        registry
            .register(actor_id, ActorMode::default())
            .await
            .unwrap();
        let result = registry.register(actor_id, ActorMode::default()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let registry = StatefulActorRegistry::new("did:icn:test".to_string());
        let actor_id = test_actor_id();

        registry
            .register(actor_id, ActorMode::default())
            .await
            .unwrap();

        // Pause
        registry
            .pause(actor_id, PauseReason::Checkpointing)
            .await
            .unwrap();
        let info = registry.get(&actor_id).await.unwrap();
        assert!(matches!(info.state, StatefulActorState::Paused { .. }));

        // Resume
        registry.resume(actor_id).await.unwrap();
        let info = registry.get(&actor_id).await.unwrap();
        assert_eq!(info.state, StatefulActorState::Running);
    }

    #[tokio::test]
    async fn test_cannot_pause_paused_actor() {
        let registry = StatefulActorRegistry::new("did:icn:test".to_string());
        let actor_id = test_actor_id();

        registry
            .register(actor_id, ActorMode::default())
            .await
            .unwrap();
        registry
            .pause(actor_id, PauseReason::Checkpointing)
            .await
            .unwrap();

        let result = registry
            .pause(actor_id, PauseReason::UserRequest)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_migration_flow() {
        let registry = StatefulActorRegistry::new("did:icn:source".to_string());
        let actor_id = test_actor_id();

        // Register and pause
        registry
            .register(actor_id, ActorMode::default())
            .await
            .unwrap();
        registry
            .pause(actor_id, PauseReason::MigrationPending)
            .await
            .unwrap();

        // Start migration
        registry
            .start_migration(actor_id, "did:icn:target".to_string())
            .await
            .unwrap();

        let info = registry.get(&actor_id).await.unwrap();
        assert!(matches!(info.state, StatefulActorState::Migrating { .. }));

        // Complete migration
        registry.complete_migration(actor_id).await.unwrap();
        assert!(registry.get(&actor_id).await.is_none());
    }

    #[tokio::test]
    async fn test_queue_messages_when_paused() {
        let registry = StatefulActorRegistry::new("did:icn:test".to_string());
        let actor_id = test_actor_id();

        registry
            .register(actor_id, ActorMode::default())
            .await
            .unwrap();
        registry
            .pause(actor_id, PauseReason::Checkpointing)
            .await
            .unwrap();

        // Queue messages
        registry
            .queue_message(&actor_id, b"msg1".to_vec())
            .await
            .unwrap();
        registry
            .queue_message(&actor_id, b"msg2".to_vec())
            .await
            .unwrap();

        // Drain after resume
        registry.resume(actor_id).await.unwrap();
        let messages = registry.drain_queued_messages(&actor_id).await.unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], b"msg1");
        assert_eq!(messages[1], b"msg2");
    }

    #[tokio::test]
    async fn test_restore_from_checkpoint() {
        let registry = StatefulActorRegistry::new("did:icn:target".to_string());

        let checkpoint = ActorCheckpoint {
            actor_id: test_actor_id(),
            sequence: 5,
            state: b"restored state".to_vec(),
            created_at: 1000,
            executor: "did:icn:source".to_string(),
            state_hash: [0u8; 32],
            signature: vec![],
        };

        registry
            .register_from_checkpoint(&checkpoint, ActorMode::default())
            .await
            .unwrap();

        let info = registry.get(&test_actor_id()).await.unwrap();
        assert_eq!(info.state, StatefulActorState::Running);
        assert_eq!(info.current_state, b"restored state");
        assert_eq!(info.last_checkpoint_seq, 5);
    }

    #[tokio::test]
    async fn test_terminate_and_cleanup() {
        let registry = StatefulActorRegistry::new("did:icn:test".to_string());
        let actor_id = test_actor_id();

        registry
            .register(actor_id, ActorMode::default())
            .await
            .unwrap();
        registry
            .terminate(actor_id, "test termination".to_string())
            .await
            .unwrap();

        // Actor should still exist but be terminated
        let info = registry.get(&actor_id).await.unwrap();
        assert!(matches!(info.state, StatefulActorState::Terminated { .. }));

        // Cleanup should remove it (with 0 max age for immediate cleanup)
        let removed = registry.cleanup(0).await;
        assert_eq!(removed, 1);
        assert!(registry.get(&actor_id).await.is_none());
    }

    #[tokio::test]
    async fn test_count_by_state() {
        let registry = StatefulActorRegistry::new("did:icn:test".to_string());

        // Register multiple actors in different states
        registry
            .register([1u8; 32], ActorMode::default())
            .await
            .unwrap();
        registry
            .register([2u8; 32], ActorMode::default())
            .await
            .unwrap();
        registry
            .register([3u8; 32], ActorMode::default())
            .await
            .unwrap();

        // Pause one
        registry
            .pause([2u8; 32], PauseReason::Checkpointing)
            .await
            .unwrap();

        // Terminate one
        registry
            .terminate([3u8; 32], "done".to_string())
            .await
            .unwrap();

        let counts = registry.count_by_state().await;
        assert_eq!(counts.get("running"), Some(&1));
        assert_eq!(counts.get("paused"), Some(&1));
        assert_eq!(counts.get("terminated"), Some(&1));
    }

    #[tokio::test]
    async fn test_update_state() {
        let registry = StatefulActorRegistry::new("did:icn:test".to_string());
        let actor_id = test_actor_id();

        registry
            .register(actor_id, ActorMode::default())
            .await
            .unwrap();

        registry
            .update_state(&actor_id, b"new state".to_vec())
            .await
            .unwrap();

        let info = registry.get(&actor_id).await.unwrap();
        assert_eq!(info.current_state, b"new state");
        assert_eq!(info.messages_processed, 1);
        assert_eq!(info.memory_bytes, 9); // "new state" length
    }
}
