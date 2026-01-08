//! Distributed checkpoint storage for actor state persistence.
//!
//! Checkpoints enable:
//! - Actor recovery after executor failure
//! - Actor migration between executors
//! - State audit trail and debugging
//!
//! # Architecture
//!
//! - **In-memory cache**: Latest checkpoint per actor (fast access)
//! - **Persistent backend**: Sled-based storage (survives restarts)
//! - **Gossip distribution**: Checkpoint announcements propagated to network
//!
//! # Consistency Model
//!
//! Checkpoints are eventually consistent:
//! - Each executor caches checkpoints it sees
//! - Gossip ensures propagation across network
//! - Sequence numbers detect stale checkpoints
//! - Signatures prevent tampering

use crate::actor_model::{ActorCheckpoint, ActorId};
use crate::error::ComputeError;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for persistent checkpoint storage backends.
pub trait CheckpointBackend: Send + Sync {
    /// Store checkpoint persistently.
    fn store(&self, checkpoint: &ActorCheckpoint) -> Result<(), String>;

    /// Retrieve checkpoint by actor ID.
    ///
    /// Returns `None` if no checkpoint exists for this actor.
    fn retrieve(&self, actor_id: &ActorId) -> Result<Option<ActorCheckpoint>, String>;

    /// List all actor IDs with stored checkpoints.
    fn list_actors(&self) -> Result<Vec<ActorId>, String>;

    /// Delete checkpoint for actor (cleanup after termination).
    fn delete(&self, actor_id: &ActorId) -> Result<(), String>;

    /// Get total number of checkpoints stored.
    fn count(&self) -> Result<usize, String>;
}

/// Distributed checkpoint storage with in-memory cache and persistent backend.
pub struct CheckpointStore {
    /// In-memory cache: actor_id -> latest checkpoint
    cache: Arc<RwLock<HashMap<ActorId, ActorCheckpoint>>>,

    /// Persistent storage backend
    backend: Arc<dyn CheckpointBackend>,
}

impl CheckpointStore {
    /// Create new checkpoint store with given backend.
    pub fn new(backend: Arc<dyn CheckpointBackend>) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            backend,
        }
    }

    /// Store checkpoint (in-memory + persistent).
    ///
    /// Returns `Ok(true)` if this is a new or updated checkpoint,
    /// `Ok(false)` if checkpoint is stale (older sequence number).
    pub async fn store(&self, checkpoint: ActorCheckpoint) -> Result<bool, ComputeError> {
        let actor_id = checkpoint.actor_id;

        // Check if this is newer than cached checkpoint
        let mut cache = self.cache.write().await;

        let is_newer = if let Some(existing) = cache.get(&actor_id) {
            if checkpoint.sequence <= existing.sequence {
                tracing::debug!(
                    actor_id = %hex::encode(actor_id),
                    incoming_seq = checkpoint.sequence,
                    cached_seq = existing.sequence,
                    "Ignoring stale checkpoint"
                );
                return Ok(false); // Stale checkpoint
            }
            true
        } else {
            true // First checkpoint for this actor
        };

        if is_newer {
            // Persist to backend
            self.backend.store(&checkpoint).map_err(|e| {
                ComputeError::Internal(format!("Failed to persist checkpoint: {e}"))
            })?;

            // Update cache
            cache.insert(actor_id, checkpoint.clone());

            tracing::debug!(
                actor_id = %hex::encode(actor_id),
                sequence = checkpoint.sequence,
                state_size = checkpoint.state.len(),
                "Stored checkpoint"
            );

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Retrieve latest checkpoint for actor.
    pub async fn get(&self, actor_id: &ActorId) -> Result<Option<ActorCheckpoint>, ComputeError> {
        // Check cache first
        let cache = self.cache.read().await;
        if let Some(checkpoint) = cache.get(actor_id) {
            return Ok(Some(checkpoint.clone()));
        }
        drop(cache);

        // Cache miss - load from backend
        let checkpoint = self
            .backend
            .retrieve(actor_id)
            .map_err(|e| ComputeError::Internal(format!("Failed to retrieve checkpoint: {e}")))?;

        // Update cache if found
        if let Some(ref cp) = checkpoint {
            let mut cache = self.cache.write().await;
            cache.insert(*actor_id, cp.clone());
        }

        Ok(checkpoint)
    }

    /// Get next sequence number for actor.
    pub async fn next_sequence(&self, actor_id: &ActorId) -> u64 {
        let cache = self.cache.read().await;
        if let Some(checkpoint) = cache.get(actor_id) {
            checkpoint.sequence + 1
        } else {
            0 // First checkpoint
        }
    }

    /// Delete checkpoint for actor (e.g., after termination).
    pub async fn delete(&self, actor_id: &ActorId) -> Result<(), ComputeError> {
        // Remove from cache
        let mut cache = self.cache.write().await;
        cache.remove(actor_id);
        drop(cache);

        // Remove from backend
        self.backend
            .delete(actor_id)
            .map_err(|e| ComputeError::Internal(format!("Failed to delete checkpoint: {e}")))?;

        tracing::debug!(
            actor_id = %hex::encode(actor_id),
            "Deleted checkpoint"
        );

        Ok(())
    }

    /// List all actors with checkpoints.
    pub async fn list_actors(&self) -> Result<Vec<ActorId>, ComputeError> {
        self.backend
            .list_actors()
            .map_err(|e| ComputeError::Internal(format!("Failed to list actors: {e}")))
    }

    /// Get total number of checkpoints.
    pub async fn count(&self) -> Result<usize, ComputeError> {
        self.backend
            .count()
            .map_err(|e| ComputeError::Internal(format!("Failed to count checkpoints: {e}")))
    }

    /// Verify checkpoint signature.
    pub fn verify(&self, checkpoint: &ActorCheckpoint) -> Result<(), ComputeError> {
        // Parse executor DID
        let executor_did: icn_identity::Did = checkpoint
            .executor
            .parse()
            .map_err(|e| ComputeError::InvalidSignature(format!("Invalid executor DID: {e}")))?;

        // Verify signature
        checkpoint.verify_signature(&executor_did)?;

        // Verify state hash
        let computed_hash = ActorCheckpoint::compute_state_hash(&checkpoint.state);
        if computed_hash != checkpoint.state_hash {
            return Err(ComputeError::InvalidSignature("State hash mismatch".into()));
        }

        Ok(())
    }

    /// Prune old checkpoints (keep only latest N per actor).
    ///
    /// Note: Current implementation keeps only latest checkpoint.
    /// Future enhancement: Keep configurable checkpoint history.
    pub async fn prune(&self, _max_age_secs: u64) -> Result<usize, ComputeError> {
        // Currently a no-op since we only store latest checkpoint
        // Future: Implement multi-checkpoint history with pruning
        Ok(0)
    }
}

/// In-memory checkpoint backend (for testing).
pub struct InMemoryBackend {
    checkpoints: Arc<std::sync::RwLock<HashMap<ActorId, ActorCheckpoint>>>,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            checkpoints: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointBackend for InMemoryBackend {
    fn store(&self, checkpoint: &ActorCheckpoint) -> Result<(), String> {
        let mut checkpoints = self
            .checkpoints
            .write()
            .map_err(|e| format!("Lock error: {e}"))?;
        checkpoints.insert(checkpoint.actor_id, checkpoint.clone());
        Ok(())
    }

    fn retrieve(&self, actor_id: &ActorId) -> Result<Option<ActorCheckpoint>, String> {
        let checkpoints = self
            .checkpoints
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;
        Ok(checkpoints.get(actor_id).cloned())
    }

    fn list_actors(&self) -> Result<Vec<ActorId>, String> {
        let checkpoints = self
            .checkpoints
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;
        Ok(checkpoints.keys().copied().collect())
    }

    fn delete(&self, actor_id: &ActorId) -> Result<(), String> {
        let mut checkpoints = self
            .checkpoints
            .write()
            .map_err(|e| format!("Lock error: {e}"))?;
        checkpoints.remove(actor_id);
        Ok(())
    }

    fn count(&self) -> Result<usize, String> {
        let checkpoints = self
            .checkpoints
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;
        Ok(checkpoints.len())
    }
}

/// Sled-based persistent checkpoint backend.
pub struct SledCheckpointBackend {
    db: sled::Db,
}

impl SledCheckpointBackend {
    /// Create new Sled backend at given path.
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let db = sled::open(path).map_err(|e| format!("Failed to open Sled DB: {e}"))?;
        Ok(Self { db })
    }

    /// Create new Sled backend with temporary directory (for testing).
    #[cfg(test)]
    pub fn new_temp() -> Result<Self, String> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| format!("Failed to open temp Sled DB: {e}"))?;
        Ok(Self { db })
    }
}

impl CheckpointBackend for SledCheckpointBackend {
    fn store(&self, checkpoint: &ActorCheckpoint) -> Result<(), String> {
        let key = checkpoint.actor_id;
        let value = icn_encoding::encode_versioned(checkpoint)
            .map_err(|e| format!("Serialization error: {e}"))?;

        self.db
            .insert(key, value)
            .map_err(|e| format!("Sled insert error: {e}"))?;

        self.db
            .flush()
            .map_err(|e| format!("Sled flush error: {e}"))?;

        Ok(())
    }

    fn retrieve(&self, actor_id: &ActorId) -> Result<Option<ActorCheckpoint>, String> {
        let value = self
            .db
            .get(actor_id)
            .map_err(|e| format!("Sled get error: {e}"))?;

        if let Some(bytes) = value {
            let checkpoint: ActorCheckpoint =
                icn_encoding::decode_versioned(&bytes)
                    .map_err(|e| format!("Deserialization error: {e}"))?;
            Ok(Some(checkpoint))
        } else {
            Ok(None)
        }
    }

    fn list_actors(&self) -> Result<Vec<ActorId>, String> {
        let mut actors = Vec::new();

        for item in self.db.iter() {
            let (key, _) = item.map_err(|e| format!("Sled iteration error: {e}"))?;
            let actor_id: ActorId = key
                .as_ref()
                .try_into()
                .map_err(|_| "Invalid actor ID length".to_string())?;
            actors.push(actor_id);
        }

        Ok(actors)
    }

    fn delete(&self, actor_id: &ActorId) -> Result<(), String> {
        self.db
            .remove(actor_id)
            .map_err(|e| format!("Sled remove error: {e}"))?;

        self.db
            .flush()
            .map_err(|e| format!("Sled flush error: {e}"))?;

        Ok(())
    }

    fn count(&self) -> Result<usize, String> {
        Ok(self.db.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_checkpoint(actor_id: ActorId, sequence: u64, state: Vec<u8>) -> ActorCheckpoint {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let executor = keypair.did().to_string();
        let signing_key_bytes = keypair.to_signing_key_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

        let state_hash = ActorCheckpoint::compute_state_hash(&state);

        let mut checkpoint = ActorCheckpoint {
            actor_id,
            sequence,
            state,
            created_at: 1000,
            executor,
            state_hash,
            signature: vec![],
        };

        checkpoint.sign(&signing_key);
        checkpoint
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend);

        let actor_id = [1u8; 32];
        let checkpoint = make_checkpoint(actor_id, 0, b"state data".to_vec());

        // Store checkpoint
        let stored = store.store(checkpoint.clone()).await.unwrap();
        assert!(stored, "First checkpoint should be stored");

        // Retrieve checkpoint
        let retrieved = store.get(&actor_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().sequence, 0);
    }

    #[tokio::test]
    async fn test_ignore_stale_checkpoint() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend);

        let actor_id = [2u8; 32];

        // Store checkpoint with sequence 5
        let checkpoint1 = make_checkpoint(actor_id, 5, b"state v5".to_vec());
        store.store(checkpoint1).await.unwrap();

        // Try to store older checkpoint (sequence 3)
        let checkpoint2 = make_checkpoint(actor_id, 3, b"state v3".to_vec());
        let stored = store.store(checkpoint2).await.unwrap();
        assert!(!stored, "Older checkpoint should be ignored");

        // Verify sequence 5 is still cached
        let retrieved = store.get(&actor_id).await.unwrap();
        assert_eq!(retrieved.unwrap().sequence, 5);
    }

    #[tokio::test]
    async fn test_update_with_newer_checkpoint() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend);

        let actor_id = [3u8; 32];

        // Store checkpoint sequence 0
        let checkpoint1 = make_checkpoint(actor_id, 0, b"state v0".to_vec());
        store.store(checkpoint1).await.unwrap();

        // Update with sequence 1
        let checkpoint2 = make_checkpoint(actor_id, 1, b"state v1".to_vec());
        let stored = store.store(checkpoint2).await.unwrap();
        assert!(stored, "Newer checkpoint should be stored");

        // Verify sequence 1 is cached
        let retrieved = store.get(&actor_id).await.unwrap();
        assert_eq!(retrieved.unwrap().sequence, 1);
    }

    #[tokio::test]
    async fn test_next_sequence() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend);

        let actor_id = [4u8; 32];

        // First checkpoint should get sequence 0
        assert_eq!(store.next_sequence(&actor_id).await, 0);

        // Store checkpoint with sequence 0
        let checkpoint = make_checkpoint(actor_id, 0, b"state".to_vec());
        store.store(checkpoint).await.unwrap();

        // Next should be 1
        assert_eq!(store.next_sequence(&actor_id).await, 1);
    }

    #[tokio::test]
    async fn test_delete_checkpoint() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend);

        let actor_id = [5u8; 32];
        let checkpoint = make_checkpoint(actor_id, 0, b"state".to_vec());

        store.store(checkpoint).await.unwrap();

        // Verify exists
        assert!(store.get(&actor_id).await.unwrap().is_some());

        // Delete
        store.delete(&actor_id).await.unwrap();

        // Verify removed
        assert!(store.get(&actor_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_list_actors() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend);

        let actor1 = [10u8; 32];
        let actor2 = [20u8; 32];
        let actor3 = [30u8; 32];

        store
            .store(make_checkpoint(actor1, 0, b"state1".to_vec()))
            .await
            .unwrap();
        store
            .store(make_checkpoint(actor2, 0, b"state2".to_vec()))
            .await
            .unwrap();
        store
            .store(make_checkpoint(actor3, 0, b"state3".to_vec()))
            .await
            .unwrap();

        let actors = store.list_actors().await.unwrap();
        assert_eq!(actors.len(), 3);
        assert!(actors.contains(&actor1));
        assert!(actors.contains(&actor2));
        assert!(actors.contains(&actor3));
    }

    #[tokio::test]
    async fn test_count() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend);

        assert_eq!(store.count().await.unwrap(), 0);

        store
            .store(make_checkpoint([1u8; 32], 0, b"state".to_vec()))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        store
            .store(make_checkpoint([2u8; 32], 0, b"state".to_vec()))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_verify_checkpoint() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend);

        let checkpoint = make_checkpoint([6u8; 32], 0, b"state data".to_vec());

        // Valid checkpoint should verify
        assert!(store.verify(&checkpoint).is_ok());

        // Tamper with state
        let mut tampered = checkpoint.clone();
        tampered.state = b"tampered state".to_vec();

        // Should fail verification (state hash mismatch)
        assert!(store.verify(&tampered).is_err());
    }

    #[test]
    fn test_sled_backend_store_retrieve() {
        let backend = SledCheckpointBackend::new_temp().unwrap();

        let actor_id = [7u8; 32];
        let checkpoint = make_checkpoint(actor_id, 0, b"sled state".to_vec());

        // Store
        backend.store(&checkpoint).unwrap();

        // Retrieve
        let retrieved = backend.retrieve(&actor_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().sequence, 0);
    }

    #[test]
    fn test_sled_backend_list_and_delete() {
        let backend = SledCheckpointBackend::new_temp().unwrap();

        let actor1 = [11u8; 32];
        let actor2 = [12u8; 32];

        backend
            .store(&make_checkpoint(actor1, 0, b"state1".to_vec()))
            .unwrap();
        backend
            .store(&make_checkpoint(actor2, 0, b"state2".to_vec()))
            .unwrap();

        // List
        let actors = backend.list_actors().unwrap();
        assert_eq!(actors.len(), 2);

        // Count
        assert_eq!(backend.count().unwrap(), 2);

        // Delete one
        backend.delete(&actor1).unwrap();
        assert_eq!(backend.count().unwrap(), 1);

        // Verify deletion
        assert!(backend.retrieve(&actor1).unwrap().is_none());
        assert!(backend.retrieve(&actor2).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_cache_miss_loads_from_backend() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = CheckpointStore::new(backend.clone());

        let actor_id = [8u8; 32];
        let checkpoint = make_checkpoint(actor_id, 0, b"backend state".to_vec());

        // Store directly in backend (bypass cache)
        backend.store(&checkpoint).unwrap();

        // Cache should be empty
        {
            let cache = store.cache.read().await;
            assert!(!cache.contains_key(&actor_id));
        }

        // Retrieve should load from backend into cache
        let retrieved = store.get(&actor_id).await.unwrap();
        assert!(retrieved.is_some());

        // Now cache should have it
        {
            let cache = store.cache.read().await;
            assert!(cache.contains_key(&actor_id));
        }
    }
}
