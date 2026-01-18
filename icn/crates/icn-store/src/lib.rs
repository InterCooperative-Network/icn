//! ICN Store - Persistent key-value storage abstraction
#![allow(missing_docs)]
// Prevent panics in production code paths
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// Budget management for ledger accounts
pub mod budgets;
/// Escrow functionality for conditional payments
pub mod escrow;
/// Storage maintenance tasks
pub mod maintenance;
/// Notification storage and management
pub mod notifications;
/// Peer cache for persisting discovered peers
pub mod peer_cache;
/// Proof-of-storage challenge system
pub mod pos;
/// Storage quota management
pub mod quotas;
/// Recurring payment scheduling
pub mod recurring_payments;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

// Re-export quota types
pub use quotas::{QuotaPriority, QuotaStats, StorageItem, StorageQuota, StorageQuotaManager};

// Re-export peer cache types
pub use peer_cache::{CachedPeer, PeerCache, PeerSource, DEFAULT_PEER_TTL};

// Re-export proof-of-storage types
pub use pos::{
    ChallengeConfig, ChallengeState, ContentChunkTree, MerkleProof, MerkleProofData,
    PendingChallenge, StorageChallenge, StorageFailureReason, StorageProof,
    CHALLENGE_PROTOCOL_VERSION, DEFAULT_BLOCKS_PER_CHALLENGE, DEFAULT_CHUNK_SIZE,
    MAX_BYTE_SAMPLE_SIZE,
};

// Re-export maintenance types
pub use maintenance::{
    MaintenanceConfig, MaintenanceHandle, MaintenanceManager, MaintenanceResult,
};

// Note: StoreTransaction, TransactionalStore, and StoreTransactionError are publicly defined
// in this file at the crate root (accessible as icn_store::TransactionalStore, etc.).

/// Content hash type (32-byte SHA-256)
pub type ContentHash = [u8; 32];

/// Replica information for a single peer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplicaInfo {
    /// DID of the peer holding this replica
    pub peer_did: String,
    /// When this replica was last seen/verified
    pub last_seen: SystemTime,
    /// Health status of this replica
    pub health: ReplicaHealth,
}

/// Health status of a replica
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplicaHealth {
    /// Replica verified recently (healthy)
    Healthy,
    /// Replica not verified recently (stale)
    Stale,
    /// Peer reported as offline/unreachable
    Unreachable,
    /// Replica failed proof-of-storage challenges (potential Byzantine behavior)
    /// The u32 is the consecutive failure count
    Unhealthy(u32),
}

/// Metadata about all replicas for a given content hash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaMetadata {
    /// Content hash this metadata describes
    pub content_hash: ContentHash,
    /// List of all known replicas
    pub replicas: Vec<ReplicaInfo>,
    /// When this metadata was last updated
    pub updated_at: SystemTime,

    // Proof-of-Storage fields (optional for backward compatibility)
    /// Merkle root of content chunks (for challenge verification)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<[u8; 32]>,
    /// Content size in bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_size: Option<u64>,
    /// Chunk size used for Merkle tree (default: 4KB)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_size: Option<u32>,
    /// Last challenge timestamp per replica (peer DID string in `did:icn:<base58-pubkey>` format -> Unix seconds).
    ///
    /// Callers are responsible for ensuring that the map keys are valid peer DIDs before insertion.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub last_challenged: HashMap<String, u64>,
    /// Last successful verification per replica (peer DID string in `did:icn:<base58-pubkey>` format -> Unix seconds).
    ///
    /// Callers are responsible for ensuring that the map keys are valid peer DIDs before insertion.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub last_verified: HashMap<String, u64>,
}

impl ReplicaMetadata {
    /// Create new replica metadata for a content hash
    pub fn new(content_hash: ContentHash) -> Self {
        Self {
            content_hash,
            replicas: Vec::new(),
            updated_at: SystemTime::now(),
            merkle_root: None,
            content_size: None,
            chunk_size: None,
            last_challenged: HashMap::new(),
            last_verified: HashMap::new(),
        }
    }

    /// Create replica metadata with Merkle tree info for PoS verification
    pub fn with_merkle_info(
        content_hash: ContentHash,
        merkle_root: [u8; 32],
        content_size: u64,
        chunk_size: u32,
    ) -> Self {
        Self {
            content_hash,
            replicas: Vec::new(),
            updated_at: SystemTime::now(),
            merkle_root: Some(merkle_root),
            content_size: Some(content_size),
            chunk_size: Some(chunk_size),
            last_challenged: HashMap::new(),
            last_verified: HashMap::new(),
        }
    }

    /// Set Merkle tree info for PoS verification
    pub fn set_merkle_info(&mut self, merkle_root: [u8; 32], content_size: u64, chunk_size: u32) {
        self.merkle_root = Some(merkle_root);
        self.content_size = Some(content_size);
        self.chunk_size = Some(chunk_size);
        self.updated_at = SystemTime::now();
    }

    /// Record a challenge being sent to a replica
    pub fn record_challenge(&mut self, peer_did: &str) {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.last_challenged.insert(peer_did.to_string(), now);
        self.updated_at = SystemTime::now();
    }

    /// Record successful verification of a replica
    pub fn record_verification(&mut self, peer_did: &str) {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.last_verified.insert(peer_did.to_string(), now);
        self.updated_at = SystemTime::now();
    }

    /// Check if a replica was recently verified (within given seconds)
    pub fn was_recently_verified(&self, peer_did: &str, within_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.last_verified
            .get(peer_did)
            .map(|verified_at| now.saturating_sub(*verified_at) < within_secs)
            .unwrap_or(false)
    }

    /// Add or update a replica entry
    pub fn add_replica(&mut self, peer_did: String, health: ReplicaHealth) {
        // Update existing replica if found
        for replica in &mut self.replicas {
            if replica.peer_did == peer_did {
                replica.last_seen = SystemTime::now();
                replica.health = health;
                self.updated_at = SystemTime::now();
                return;
            }
        }

        // Add new replica
        self.replicas.push(ReplicaInfo {
            peer_did,
            last_seen: SystemTime::now(),
            health,
        });
        self.updated_at = SystemTime::now();
    }

    /// Remove a replica entry
    pub fn remove_replica(&mut self, peer_did: &str) -> bool {
        let before_len = self.replicas.len();
        self.replicas.retain(|r| r.peer_did != peer_did);
        if self.replicas.len() < before_len {
            self.updated_at = SystemTime::now();
            true
        } else {
            false
        }
    }

    /// Get count of healthy replicas
    pub fn healthy_count(&self) -> usize {
        self.replicas
            .iter()
            .filter(|r| r.health == ReplicaHealth::Healthy)
            .count()
    }

    /// Get all healthy replica DIDs
    pub fn healthy_replicas(&self) -> Vec<String> {
        self.replicas
            .iter()
            .filter(|r| r.health == ReplicaHealth::Healthy)
            .map(|r| r.peer_did.clone())
            .collect()
    }

    /// Mark a replica as unhealthy due to failed proof-of-storage challenges
    ///
    /// This is called when a replica exceeds the grace period for failed challenges.
    /// Returns true if the replica was found and marked unhealthy.
    pub fn mark_replica_unhealthy(&mut self, peer_did: &str, failure_count: u32) -> bool {
        for replica in &mut self.replicas {
            if replica.peer_did == peer_did {
                replica.health = ReplicaHealth::Unhealthy(failure_count);
                replica.last_seen = SystemTime::now();
                self.updated_at = SystemTime::now();
                return true;
            }
        }
        false
    }

    /// Get count of unhealthy replicas
    pub fn unhealthy_count(&self) -> usize {
        self.replicas
            .iter()
            .filter(|r| matches!(r.health, ReplicaHealth::Unhealthy(_)))
            .count()
    }

    /// Get all unhealthy replica DIDs
    pub fn unhealthy_replicas(&self) -> Vec<String> {
        self.replicas
            .iter()
            .filter(|r| matches!(r.health, ReplicaHealth::Unhealthy(_)))
            .map(|r| r.peer_did.clone())
            .collect()
    }

    /// Check if re-replication is needed based on healthy vs unhealthy replica ratio
    ///
    /// Returns true if the number of healthy replicas is below the minimum threshold.
    pub fn needs_re_replication(&self, min_healthy_replicas: usize) -> bool {
        self.healthy_count() < min_healthy_replicas
    }
}

/// Storage trait for pluggable backends
pub trait Store: Send + Sync {
    /// Get a value by key
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    /// Store a key-value pair
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    /// Delete a key
    fn delete(&self, key: &[u8]) -> Result<()>;
    /// Scan all key-value pairs with the given prefix
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Count entries with a given prefix without loading values
    ///
    /// More efficient than `scan().len()` as it doesn't deserialize values.
    fn scan_count(&self, prefix: &[u8]) -> Result<usize> {
        // Default implementation uses scan - backends can override for efficiency
        Ok(self.scan(prefix)?.len())
    }

    /// Scan entries with pagination support
    ///
    /// Returns entries starting at `offset` with a maximum of `limit` entries.
    /// More memory-efficient than loading all entries for large datasets.
    ///
    /// # Arguments
    /// * `prefix` - Key prefix to scan
    /// * `offset` - Number of entries to skip
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// Tuple of (entries, total_count) where total_count is the total matching entries
    #[allow(clippy::type_complexity)]
    fn scan_paginated(
        &self,
        prefix: &[u8],
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, usize)> {
        // Default implementation - backends can override for efficiency
        let all = self.scan(prefix)?;
        let total = all.len();
        let paginated = all.into_iter().skip(offset).take(limit).collect();
        Ok((paginated, total))
    }

    /// Scan entries in reverse key order with pagination support
    ///
    /// Returns entries in reverse key order (most recent first for timestamp-keyed data),
    /// starting at `offset` with a maximum of `limit` entries.
    /// This is efficient for audit trails where keys include timestamps.
    ///
    /// # Arguments
    /// * `prefix` - Key prefix to scan
    /// * `offset` - Number of entries to skip
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// Tuple of (entries, total_count) where entries are in reverse key order
    #[allow(clippy::type_complexity)]
    fn scan_reverse_paginated(
        &self,
        prefix: &[u8],
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, usize)> {
        // Default implementation - backends can override for efficiency
        // For Sled, this can use .rev() on the iterator for true efficiency
        let all = self.scan(prefix)?;
        let total = all.len();
        let paginated = all.into_iter().rev().skip(offset).take(limit).collect();
        Ok((paginated, total))
    }

    // Replica tracking operations (Phase 17)
    /// Get replica metadata for a content hash
    fn get_replica_metadata(&self, content_hash: &ContentHash) -> Result<Option<ReplicaMetadata>>;

    /// Store replica metadata for a content hash
    fn put_replica_metadata(&self, metadata: &ReplicaMetadata) -> Result<()>;

    /// List all content hashes with replica metadata
    fn list_replica_hashes(&self) -> Result<Vec<ContentHash>>;

    /// Get count of replicas for a content hash
    fn get_replica_count(&self, content_hash: &ContentHash) -> Result<usize> {
        Ok(self
            .get_replica_metadata(content_hash)?
            .map(|m| m.replicas.len())
            .unwrap_or(0))
    }

    /// Add or update a replica for a content hash
    fn add_replica(
        &self,
        content_hash: &ContentHash,
        peer_did: String,
        health: ReplicaHealth,
    ) -> Result<()> {
        let mut metadata = self
            .get_replica_metadata(content_hash)?
            .unwrap_or_else(|| ReplicaMetadata::new(*content_hash));

        metadata.add_replica(peer_did, health);
        self.put_replica_metadata(&metadata)
    }

    /// Remove a replica for a content hash
    fn remove_replica(&self, content_hash: &ContentHash, peer_did: &str) -> Result<bool> {
        if let Some(mut metadata) = self.get_replica_metadata(content_hash)? {
            let removed = metadata.remove_replica(peer_did);
            if removed {
                self.put_replica_metadata(&metadata)?;
            }
            Ok(removed)
        } else {
            Ok(false)
        }
    }

    /// Check if this store backend supports transactions
    ///
    /// Returns true if `transaction()` can be called on this store.
    /// Backends without transaction support will return false.
    fn supports_transactions(&self) -> bool {
        false
    }
}

/// Operations available within a transaction
///
/// This trait provides the subset of Store operations that can be performed
/// atomically within a transaction. All operations are deferred until the
/// transaction commits.
pub trait StoreTransaction {
    /// Get a value by key within the transaction
    ///
    /// Returns the value if it exists, considering any pending modifications
    /// within this transaction.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Set a key-value pair within the transaction
    ///
    /// The value will be written when the transaction commits.
    fn set(&self, key: &[u8], value: &[u8]) -> Result<()>;

    /// Delete a key within the transaction
    ///
    /// The deletion will be applied when the transaction commits.
    fn delete(&self, key: &[u8]) -> Result<()>;
}

/// Extension trait for stores that support transactional batch operations
///
/// This trait is separate from `Store` to maintain dyn-compatibility of the base trait.
/// Use this when you need atomic multi-key operations.
///
/// # Retry Semantics
///
/// The closure is constrained to [`Fn`] (not [`FnOnce`]) because the underlying
/// storage backend (Sled) may retry the transaction if it encounters conflicts
/// with concurrent operations. This means:
///
/// - **The closure may be called multiple times** - design for idempotency
/// - **Avoid side effects** that should not repeat (network calls, external state)
/// - **Any non-transactional side effects** should happen AFTER the transaction
///   returns `Ok`, based on the returned result
///
/// # Example
/// ```ignore
/// use icn_store::{SledStore, TransactionalStore};
///
/// let store = SledStore::temporary()?;
/// store.transaction(|tx| {
///     let balance = tx.get(b"balance:alice")?.unwrap_or_default();
///     let new_balance = parse_balance(&balance)? + 100;
///     tx.set(b"balance:alice", &encode_balance(new_balance))?;
///     tx.set(b"log:12345", b"credited alice 100")?;
///     Ok(())
/// })?;
/// ```
pub trait TransactionalStore: Store {
    /// Execute a transactional batch of operations atomically
    ///
    /// All operations within the transaction are applied atomically - either all
    /// succeed or all are rolled back. This is useful for maintaining consistency
    /// when multiple keys need to be updated together.
    ///
    /// # Arguments
    /// * `f` - A closure that receives a `StoreTransaction` and returns a result.
    ///   If the closure returns `Ok`, all operations are committed.
    ///   If it returns `Err`, all operations are rolled back.
    ///
    /// # Important: Retry Behavior
    ///
    /// The closure uses `Fn` (not `FnOnce`) because the transaction may be
    /// retried on conflicts. Avoid side effects inside the closure that are
    /// not safe to repeat.
    ///
    /// # Use Cases
    /// - Ledger entry + index update
    /// - Trust edge + cache invalidation
    /// - Contract state + event emission
    fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: Fn(&dyn StoreTransaction) -> Result<T>;
}

/// Error indicating a store transaction was aborted
#[derive(Debug, Clone, thiserror::Error)]
pub enum StoreTransactionError {
    /// Transaction was explicitly aborted by user code
    #[error("Transaction aborted: {reason}")]
    Aborted { reason: String },

    /// Transaction failed due to a conflict (e.g., concurrent modification)
    ///
    /// Note: Sled's current `TransactionError` type only exposes `Abort` and
    /// `Storage` variants (conflicts are handled internally via retry). This
    /// variant is reserved for future use if more detailed conflict information
    /// becomes available from the storage backend.
    #[error("Transaction conflict: another operation modified the same keys")]
    Conflict,

    /// Storage backend error during transaction
    #[error("Storage error during transaction: {details}")]
    StorageError { details: String },
}

/// Storage statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    /// Size of the database on disk in bytes
    pub size_on_disk_bytes: u64,
    /// Space amplification factor (actual size / logical size)
    /// Values close to 1.0 indicate efficient storage
    pub space_amplification: f64,
    /// Number of entries in the database (if available)
    pub entry_count: Option<usize>,
}

/// Sled-based storage implementation
pub struct SledStore {
    db: sled::Db,
}

impl SledStore {
    /// Open a Sled database at the given path
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(SledStore { db })
    }

    /// Create a temporary in-memory Sled database
    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        Ok(SledStore { db })
    }

    /// Get direct access to underlying Sled database
    ///
    /// This is useful for components that need raw Sled access
    /// rather than the Store trait abstraction.
    pub fn db(&self) -> &sled::Db {
        &self.db
    }

    /// Flush all pending writes to disk
    ///
    /// This ensures data durability by syncing all buffered writes.
    /// Returns the number of bytes flushed.
    pub fn flush(&self) -> Result<usize> {
        let start = std::time::Instant::now();
        let flushed = self.db.flush()?;
        icn_obs::metrics::storage::flush_total_inc();
        icn_obs::metrics::storage::flush_bytes_add(flushed as u64);
        icn_obs::metrics::storage::flush_duration_record(start.elapsed().as_secs_f64());
        Ok(flushed)
    }

    /// Asynchronously flush all pending writes to disk
    ///
    /// Async version of [`flush`] that does not block the current thread
    /// while waiting for the I/O operation to complete.
    pub async fn flush_async(&self) -> Result<usize> {
        let start = std::time::Instant::now();
        let flushed = self.db.flush_async().await?;
        icn_obs::metrics::storage::flush_total_inc();
        icn_obs::metrics::storage::flush_bytes_add(flushed as u64);
        icn_obs::metrics::storage::flush_duration_record(start.elapsed().as_secs_f64());
        Ok(flushed)
    }

    /// Get storage statistics for monitoring
    ///
    /// Returns disk usage and space amplification metrics.
    /// Also updates the storage metrics gauges for Prometheus.
    /// Useful for deciding when maintenance is needed.
    pub fn stats(&self) -> Result<StorageStats> {
        let size_on_disk_bytes = self.db.size_on_disk()?;

        // Space amplification: ratio of actual disk usage to logical data size
        // Values close to 1.0 indicate efficient storage
        // Higher values may indicate fragmentation or pending garbage collection
        let space_amplification = self.db.space_amplification()?;

        // Update Prometheus metrics
        icn_obs::metrics::storage::size_bytes_set(size_on_disk_bytes);
        icn_obs::metrics::storage::space_amplification_set(space_amplification);

        Ok(StorageStats {
            size_on_disk_bytes,
            space_amplification,
            entry_count: None, // Counting all entries is expensive
        })
    }

    /// Get the size of the database on disk in bytes
    pub fn size_on_disk(&self) -> Result<u64> {
        Ok(self.db.size_on_disk()?)
    }

    /// Check if the database needs optimization
    ///
    /// Returns true if space amplification exceeds the threshold (default: 2.0)
    /// indicating significant storage overhead that may benefit from maintenance.
    ///
    /// Note: Sled performs automatic garbage collection, so high amplification
    /// may resolve naturally over time with normal operations.
    pub fn needs_optimization(&self, threshold: f64) -> Result<bool> {
        let amplification = self.db.space_amplification()?;
        Ok(amplification > threshold)
    }

    /// Generate a unique ID
    ///
    /// Useful for creating unique keys without coordination.
    pub fn generate_id(&self) -> Result<u64> {
        Ok(self.db.generate_id()?)
    }
}

impl Store for SledStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get(key)?.map(|v| v.to_vec()))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.insert(key, value)?;
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.db.remove(key)?;
        Ok(())
    }

    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (k, v) = item?;
            results.push((k.to_vec(), v.to_vec()));
        }

        Ok(results)
    }

    fn scan_count(&self, prefix: &[u8]) -> Result<usize> {
        // Efficient count using iterator - doesn't materialize values
        let count = self.db.scan_prefix(prefix).count();
        Ok(count)
    }

    fn scan_paginated(
        &self,
        prefix: &[u8],
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, usize)> {
        // First get total count efficiently
        let total = self.db.scan_prefix(prefix).count();

        // Then get paginated results
        let mut results = Vec::with_capacity(limit.min(total.saturating_sub(offset)));
        for item in self.db.scan_prefix(prefix).skip(offset).take(limit) {
            let (k, v) = item?;
            results.push((k.to_vec(), v.to_vec()));
        }

        Ok((results, total))
    }

    fn scan_reverse_paginated(
        &self,
        prefix: &[u8],
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, usize)> {
        // First get total count efficiently (without loading values)
        let total = self.db.scan_prefix(prefix).count();

        // Then get paginated results in reverse order
        // Sled's scan_prefix().rev() efficiently iterates in reverse
        let mut results = Vec::with_capacity(limit.min(total.saturating_sub(offset)));
        for item in self.db.scan_prefix(prefix).rev().skip(offset).take(limit) {
            let (k, v) = item?;
            results.push((k.to_vec(), v.to_vec()));
        }

        Ok((results, total))
    }

    // Replica tracking implementation
    fn get_replica_metadata(&self, content_hash: &ContentHash) -> Result<Option<ReplicaMetadata>> {
        let key = Self::replica_key(content_hash);
        if let Some(value) = self.db.get(&key)? {
            let metadata: ReplicaMetadata =
                serde_json::from_slice(&value).context("Failed to deserialize replica metadata")?;
            Ok(Some(metadata))
        } else {
            Ok(None)
        }
    }

    fn put_replica_metadata(&self, metadata: &ReplicaMetadata) -> Result<()> {
        let key = Self::replica_key(&metadata.content_hash);
        let value = serde_json::to_vec(metadata).context("Failed to serialize replica metadata")?;
        self.db.insert(&key, value)?;
        Ok(())
    }

    fn list_replica_hashes(&self) -> Result<Vec<ContentHash>> {
        let prefix = b"replica:";
        let mut hashes = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (key, _) = item?;
            // Key format: "replica:" + 32-byte hash
            if key.len() == prefix.len() + 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&key[prefix.len()..]);
                hashes.push(hash);
            }
        }

        Ok(hashes)
    }

    fn supports_transactions(&self) -> bool {
        true
    }
}

/// Wrapper for Sled's transactional tree that implements StoreTransaction
struct SledTransaction<'a> {
    tree: &'a sled::transaction::TransactionalTree,
}

impl StoreTransaction for SledTransaction<'_> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.tree
            .get(key)
            .map(|opt| opt.map(|v| v.to_vec()))
            .map_err(|e| anyhow::anyhow!("Transaction get failed: {e}"))
    }

    fn set(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.tree
            .insert(key, value)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("Transaction set failed: {e}"))
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.tree
            .remove(key)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("Transaction delete failed: {e}"))
    }
}

impl TransactionalStore for SledStore {
    fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: Fn(&dyn StoreTransaction) -> Result<T>,
    {
        use sled::transaction::ConflictableTransactionError;

        // Use Sled's transaction API
        let result = self.db.transaction(|tx_tree| {
            let tx = SledTransaction { tree: tx_tree };

            // Call user's closure
            match f(&tx) {
                Ok(value) => Ok(value),
                Err(e) => {
                    // Convert anyhow::Error to abort
                    Err(ConflictableTransactionError::Abort(e.to_string()))
                }
            }
        });

        match result {
            Ok(value) => Ok(value),
            Err(sled::transaction::TransactionError::Abort(reason)) => {
                Err(StoreTransactionError::Aborted { reason }.into())
            }
            Err(sled::transaction::TransactionError::Storage(e)) => {
                Err(StoreTransactionError::StorageError {
                    details: e.to_string(),
                }
                .into())
            }
        }
    }
}

impl SledStore {
    /// Generate storage key for replica metadata
    fn replica_key(content_hash: &ContentHash) -> Vec<u8> {
        let mut key = Vec::with_capacity(8 + 32);
        key.extend_from_slice(b"replica:");
        key.extend_from_slice(content_hash);
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hash() -> ContentHash {
        let mut hash = [0u8; 32];
        hash[0] = 0xAA;
        hash[31] = 0xBB;
        hash
    }

    fn test_hash2() -> ContentHash {
        let mut hash = [0u8; 32];
        hash[0] = 0xCC;
        hash[31] = 0xDD;
        hash
    }

    #[test]
    fn test_replica_metadata_new() {
        let hash = test_hash();
        let metadata = ReplicaMetadata::new(hash);

        assert_eq!(metadata.content_hash, hash);
        assert_eq!(metadata.replicas.len(), 0);
    }

    #[test]
    fn test_replica_metadata_add_replica() {
        let hash = test_hash();
        let mut metadata = ReplicaMetadata::new(hash);

        // Add first replica
        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        assert_eq!(metadata.replicas.len(), 1);
        assert_eq!(metadata.replicas[0].peer_did, "did:icn:peer1");
        assert_eq!(metadata.replicas[0].health, ReplicaHealth::Healthy);

        // Add second replica
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Healthy);
        assert_eq!(metadata.replicas.len(), 2);

        // Update first replica (should not add duplicate)
        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Stale);
        assert_eq!(metadata.replicas.len(), 2);
        assert_eq!(metadata.replicas[0].health, ReplicaHealth::Stale);
    }

    #[test]
    fn test_replica_metadata_remove_replica() {
        let hash = test_hash();
        let mut metadata = ReplicaMetadata::new(hash);

        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Healthy);
        assert_eq!(metadata.replicas.len(), 2);

        // Remove existing replica
        let removed = metadata.remove_replica("did:icn:peer1");
        assert!(removed);
        assert_eq!(metadata.replicas.len(), 1);
        assert_eq!(metadata.replicas[0].peer_did, "did:icn:peer2");

        // Try to remove non-existent replica
        let removed = metadata.remove_replica("did:icn:peer3");
        assert!(!removed);
        assert_eq!(metadata.replicas.len(), 1);
    }

    #[test]
    fn test_replica_metadata_healthy_count() {
        let hash = test_hash();
        let mut metadata = ReplicaMetadata::new(hash);

        assert_eq!(metadata.healthy_count(), 0);

        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Stale);
        metadata.add_replica("did:icn:peer3".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer4".to_string(), ReplicaHealth::Unreachable);

        assert_eq!(metadata.healthy_count(), 2);
        assert_eq!(metadata.healthy_replicas().len(), 2);
        assert!(metadata
            .healthy_replicas()
            .contains(&"did:icn:peer1".to_string()));
        assert!(metadata
            .healthy_replicas()
            .contains(&"did:icn:peer3".to_string()));
    }

    #[test]
    fn test_store_replica_metadata_roundtrip() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash = test_hash();

        // Initially no metadata
        assert!(store.get_replica_metadata(&hash)?.is_none());

        // Create and store metadata
        let mut metadata = ReplicaMetadata::new(hash);
        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Stale);
        store.put_replica_metadata(&metadata)?;

        // Retrieve and verify
        let retrieved = store.get_replica_metadata(&hash)?.unwrap();
        assert_eq!(retrieved.content_hash, hash);
        assert_eq!(retrieved.replicas.len(), 2);
        assert_eq!(retrieved.replicas[0].peer_did, "did:icn:peer1");
        assert_eq!(retrieved.replicas[1].peer_did, "did:icn:peer2");

        Ok(())
    }

    #[test]
    fn test_store_get_replica_count() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash = test_hash();

        // Initially 0
        assert_eq!(store.get_replica_count(&hash)?, 0);

        // Add replicas via convenience method
        store.add_replica(&hash, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
        assert_eq!(store.get_replica_count(&hash)?, 1);

        store.add_replica(&hash, "did:icn:peer2".to_string(), ReplicaHealth::Healthy)?;
        assert_eq!(store.get_replica_count(&hash)?, 2);

        Ok(())
    }

    #[test]
    fn test_store_add_replica() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash = test_hash();

        // Add first replica
        store.add_replica(&hash, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
        let metadata = store.get_replica_metadata(&hash)?.unwrap();
        assert_eq!(metadata.replicas.len(), 1);
        assert_eq!(metadata.replicas[0].peer_did, "did:icn:peer1");

        // Add second replica
        store.add_replica(&hash, "did:icn:peer2".to_string(), ReplicaHealth::Stale)?;
        let metadata = store.get_replica_metadata(&hash)?.unwrap();
        assert_eq!(metadata.replicas.len(), 2);

        // Update first replica health
        store.add_replica(
            &hash,
            "did:icn:peer1".to_string(),
            ReplicaHealth::Unreachable,
        )?;
        let metadata = store.get_replica_metadata(&hash)?.unwrap();
        assert_eq!(metadata.replicas.len(), 2); // Still 2, not 3
        assert_eq!(metadata.replicas[0].health, ReplicaHealth::Unreachable);

        Ok(())
    }

    #[test]
    fn test_store_remove_replica() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash = test_hash();

        // Setup: add two replicas
        store.add_replica(&hash, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
        store.add_replica(&hash, "did:icn:peer2".to_string(), ReplicaHealth::Healthy)?;

        // Remove existing replica
        let removed = store.remove_replica(&hash, "did:icn:peer1")?;
        assert!(removed);
        assert_eq!(store.get_replica_count(&hash)?, 1);

        // Try to remove non-existent replica
        let removed = store.remove_replica(&hash, "did:icn:peer3")?;
        assert!(!removed);
        assert_eq!(store.get_replica_count(&hash)?, 1);

        // Remove from non-existent hash
        let removed = store.remove_replica(&test_hash2(), "did:icn:peer1")?;
        assert!(!removed);

        Ok(())
    }

    #[test]
    fn test_store_list_replica_hashes() -> Result<()> {
        let store = SledStore::temporary()?;
        let hash1 = test_hash();
        let hash2 = test_hash2();

        // Initially empty
        assert_eq!(store.list_replica_hashes()?.len(), 0);

        // Add metadata for two hashes
        store.add_replica(&hash1, "did:icn:peer1".to_string(), ReplicaHealth::Healthy)?;
        store.add_replica(&hash2, "did:icn:peer2".to_string(), ReplicaHealth::Healthy)?;

        // Should list both hashes
        let hashes = store.list_replica_hashes()?;
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains(&hash1));
        assert!(hashes.contains(&hash2));

        Ok(())
    }

    #[test]
    fn test_replica_metadata_serialization() -> Result<()> {
        let hash = test_hash();
        let mut metadata = ReplicaMetadata::new(hash);
        metadata.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
        metadata.add_replica("did:icn:peer2".to_string(), ReplicaHealth::Stale);

        // Serialize
        let json = serde_json::to_string(&metadata)?;
        assert!(json.contains("did:icn:peer1"));
        assert!(json.contains("Healthy"));
        assert!(json.contains("Stale"));

        // Deserialize
        let deserialized: ReplicaMetadata = serde_json::from_str(&json)?;
        assert_eq!(deserialized.content_hash, hash);
        assert_eq!(deserialized.replicas.len(), 2);

        Ok(())
    }

    #[test]
    fn test_replica_key_format() {
        let hash = test_hash();
        let key = SledStore::replica_key(&hash);

        // Should be "replica:" + 32 bytes
        assert_eq!(key.len(), 8 + 32);
        assert_eq!(&key[0..8], b"replica:");
        assert_eq!(&key[8..], &hash);
    }

    #[test]
    fn test_basic_store_operations() -> Result<()> {
        let store = SledStore::temporary()?;

        // Test put and get
        let key = b"test:key1";
        let value = b"hello world";
        store.put(key, value)?;

        let retrieved = store.get(key)?;
        assert_eq!(retrieved.as_deref(), Some(value.as_slice()));

        // Test overwrite
        let new_value = b"updated value";
        store.put(key, new_value)?;
        let retrieved = store.get(key)?;
        assert_eq!(retrieved.as_deref(), Some(new_value.as_slice()));

        // Test get non-existent key
        let missing = store.get(b"does:not:exist")?;
        assert!(missing.is_none());

        // Test delete
        store.delete(key)?;
        let deleted = store.get(key)?;
        assert!(deleted.is_none());

        Ok(())
    }

    #[test]
    fn test_store_scan() -> Result<()> {
        let store = SledStore::temporary()?;

        // Insert multiple values with same prefix
        store.put(b"users:alice", b"Alice data")?;
        store.put(b"users:bob", b"Bob data")?;
        store.put(b"users:charlie", b"Charlie data")?;
        store.put(b"posts:post1", b"Post data")?;

        // Scan users
        let users = store.scan(b"users:")?;
        assert_eq!(users.len(), 3);

        // Scan posts
        let posts = store.scan(b"posts:")?;
        assert_eq!(posts.len(), 1);

        // Scan with non-matching prefix
        let empty = store.scan(b"nonexistent:")?;
        assert_eq!(empty.len(), 0);

        Ok(())
    }

    #[test]
    fn test_store_scan_count() -> Result<()> {
        let store = SledStore::temporary()?;

        // Insert multiple values
        store.put(b"items:1", b"data1")?;
        store.put(b"items:2", b"data2")?;
        store.put(b"items:3", b"data3")?;
        store.put(b"other:1", b"other")?;

        // Count with prefix
        assert_eq!(store.scan_count(b"items:")?, 3);
        assert_eq!(store.scan_count(b"other:")?, 1);
        assert_eq!(store.scan_count(b"nonexistent:")?, 0);

        Ok(())
    }

    #[test]
    fn test_store_scan_paginated() -> Result<()> {
        let store = SledStore::temporary()?;

        // Insert 10 items
        for i in 0..10 {
            store.put(format!("items:{i:02}").as_bytes(), b"data")?;
        }

        // Get first page (5 items)
        let (page1, total) = store.scan_paginated(b"items:", 0, 5)?;
        assert_eq!(total, 10);
        assert_eq!(page1.len(), 5);

        // Get second page
        let (page2, total) = store.scan_paginated(b"items:", 5, 5)?;
        assert_eq!(total, 10);
        assert_eq!(page2.len(), 5);

        // Get partial last page
        let (page3, total) = store.scan_paginated(b"items:", 8, 5)?;
        assert_eq!(total, 10);
        assert_eq!(page3.len(), 2);

        // Offset beyond total
        let (empty, total) = store.scan_paginated(b"items:", 20, 5)?;
        assert_eq!(total, 10);
        assert_eq!(empty.len(), 0);

        Ok(())
    }

    #[test]
    fn test_store_scan_reverse_paginated() -> Result<()> {
        let store = SledStore::temporary()?;

        // Insert items (they'll be sorted by key)
        store.put(b"items:001", b"first")?;
        store.put(b"items:002", b"second")?;
        store.put(b"items:003", b"third")?;
        store.put(b"items:004", b"fourth")?;
        store.put(b"items:005", b"fifth")?;

        // Get first page in reverse (should start with 005)
        let (page, total) = store.scan_reverse_paginated(b"items:", 0, 3)?;
        assert_eq!(total, 5);
        assert_eq!(page.len(), 3);
        assert_eq!(&page[0].0, b"items:005");
        assert_eq!(&page[1].0, b"items:004");
        assert_eq!(&page[2].0, b"items:003");

        // Get second page
        let (page2, _) = store.scan_reverse_paginated(b"items:", 3, 3)?;
        assert_eq!(page2.len(), 2);
        assert_eq!(&page2[0].0, b"items:002");
        assert_eq!(&page2[1].0, b"items:001");

        Ok(())
    }

    #[test]
    fn test_replica_health_enum() {
        // Test all enum variants
        let healthy = ReplicaHealth::Healthy;
        let stale = ReplicaHealth::Stale;
        let unreachable = ReplicaHealth::Unreachable;

        // Test equality
        assert_eq!(healthy, ReplicaHealth::Healthy);
        assert_ne!(healthy, stale);
        assert_ne!(stale, unreachable);

        // Test serialization
        let json = serde_json::to_string(&healthy).unwrap();
        assert_eq!(json, "\"Healthy\"");

        let parsed: ReplicaHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, healthy);
    }

    #[test]
    fn test_storage_stats() -> Result<()> {
        let store = SledStore::temporary()?;

        // Get initial stats - temporary DB may start at 0
        let stats = store.stats()?;
        // Space amplification should be a valid ratio (may be 0 for empty DB)
        assert!(stats.space_amplification >= 0.0);

        // Add some data
        for i in 0..100 {
            store.put(format!("key:{i}").as_bytes(), b"test value")?;
        }
        store.flush()?;

        // Stats should reflect the added data
        let stats_after = store.stats()?;
        assert!(stats_after.size_on_disk_bytes >= stats.size_on_disk_bytes);

        Ok(())
    }

    #[test]
    fn test_flush() -> Result<()> {
        let store = SledStore::temporary()?;

        // Add some data
        store.put(b"test:key", b"test value")?;

        // Flush should succeed and return bytes flushed
        let _flushed = store.flush()?;
        // flushed may be 0 if already flushed - just verify it doesn't error

        // Data should still be readable after flush
        let value = store.get(b"test:key")?;
        assert_eq!(value.as_deref(), Some(b"test value".as_slice()));

        Ok(())
    }

    #[test]
    fn test_size_on_disk() -> Result<()> {
        let store = SledStore::temporary()?;

        let initial_size = store.size_on_disk()?;

        // Add data
        for i in 0..50 {
            store.put(format!("data:{i}").as_bytes(), &vec![0u8; 1000])?;
        }
        store.flush()?;

        let final_size = store.size_on_disk()?;
        // Size should have grown (may not be linear due to compression/overhead)
        assert!(final_size >= initial_size);

        Ok(())
    }

    #[test]
    fn test_needs_optimization() -> Result<()> {
        let store = SledStore::temporary()?;

        // Fresh database should not need optimization
        // (space amplification should be close to 1.0)
        let needs_opt = store.needs_optimization(10.0)?;
        assert!(!needs_opt, "Fresh database should not need optimization");

        Ok(())
    }

    #[test]
    fn test_generate_id() -> Result<()> {
        let store = SledStore::temporary()?;

        // Generate IDs should be unique
        let id1 = store.generate_id()?;
        let id2 = store.generate_id()?;
        let id3 = store.generate_id()?;

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);

        Ok(())
    }

    #[test]
    fn test_replica_info_creation() {
        let info = ReplicaInfo {
            peer_did: "did:icn:test".to_string(),
            last_seen: SystemTime::now(),
            health: ReplicaHealth::Healthy,
        };

        assert_eq!(info.peer_did, "did:icn:test");
        assert_eq!(info.health, ReplicaHealth::Healthy);

        // Test serialization roundtrip
        let json = serde_json::to_string(&info).unwrap();
        let parsed: ReplicaInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.peer_did, info.peer_did);
        assert_eq!(parsed.health, info.health);
    }

    #[test]
    fn test_sled_store_temporary() -> Result<()> {
        // Temporary store should be independent
        let store1 = SledStore::temporary()?;
        let store2 = SledStore::temporary()?;

        store1.put(b"key", b"value1")?;

        // Store 2 should not see store 1's data
        assert!(store2.get(b"key")?.is_none());

        Ok(())
    }

    #[test]
    fn test_store_delete_nonexistent() -> Result<()> {
        let store = SledStore::temporary()?;

        // Deleting non-existent key should succeed silently
        store.delete(b"nonexistent:key")?;

        // Verify the key still doesn't exist
        assert!(store.get(b"nonexistent:key")?.is_none());

        Ok(())
    }

    #[test]
    fn test_store_empty_values() -> Result<()> {
        let store = SledStore::temporary()?;

        // Empty value
        store.put(b"empty:value", b"")?;
        let retrieved = store.get(b"empty:value")?;
        assert_eq!(retrieved, Some(vec![]));

        // Empty key (edge case)
        store.put(b"", b"value for empty key")?;
        let retrieved = store.get(b"")?;
        assert_eq!(
            retrieved.as_deref(),
            Some(b"value for empty key".as_slice())
        );

        Ok(())
    }

    #[test]
    fn test_store_binary_data() -> Result<()> {
        let store = SledStore::temporary()?;

        // Binary data with null bytes
        let binary_key = &[0x00, 0x01, 0x02, 0xFF, 0xFE];
        let binary_value = &[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00];

        store.put(binary_key, binary_value)?;
        let retrieved = store.get(binary_key)?;
        assert_eq!(retrieved.as_deref(), Some(binary_value.as_slice()));

        Ok(())
    }

    // Transaction tests

    #[test]
    fn test_supports_transactions() {
        let store = SledStore::temporary().unwrap();
        assert!(store.supports_transactions());
    }

    #[test]
    fn test_transaction_commit() -> Result<()> {
        let store = SledStore::temporary()?;

        // Execute a transaction that sets multiple keys
        store.transaction(|tx| {
            tx.set(b"balance:alice", b"100")?;
            tx.set(b"balance:bob", b"200")?;
            tx.set(b"log:0001", b"initial deposit")?;
            Ok(())
        })?;

        // Verify all keys were committed
        assert_eq!(store.get(b"balance:alice")?, Some(b"100".to_vec()));
        assert_eq!(store.get(b"balance:bob")?, Some(b"200".to_vec()));
        assert_eq!(store.get(b"log:0001")?, Some(b"initial deposit".to_vec()));

        Ok(())
    }

    #[test]
    fn test_transaction_rollback_on_error() -> Result<()> {
        let store = SledStore::temporary()?;

        // Set initial value
        store.put(b"balance:alice", b"100")?;

        // Execute a transaction that fails partway through
        let result: Result<()> = store.transaction(|tx| {
            tx.set(b"balance:alice", b"50")?; // This would commit on success
            tx.set(b"balance:bob", b"50")?; // This too

            // Simulate an error condition
            anyhow::bail!("Simulated error: insufficient funds")
        });

        // Transaction should have failed
        assert!(result.is_err());

        // Original value should be preserved (rollback)
        assert_eq!(store.get(b"balance:alice")?, Some(b"100".to_vec()));

        // New key should not exist (rollback)
        assert!(store.get(b"balance:bob")?.is_none());

        Ok(())
    }

    #[test]
    fn test_transaction_read_own_writes() -> Result<()> {
        let store = SledStore::temporary()?;

        // Set initial value
        store.put(b"counter", b"10")?;

        // Transaction that reads and modifies
        let final_value = store.transaction(|tx| {
            // Read initial value
            let current = tx.get(b"counter")?.unwrap_or_default();
            let current_num: i32 = String::from_utf8_lossy(&current).parse().unwrap_or(0);

            // Increment and write
            let new_value = (current_num + 5).to_string();
            tx.set(b"counter", new_value.as_bytes())?;

            // Read back (should see our write)
            let updated = tx.get(b"counter")?.unwrap_or_default();
            Ok(String::from_utf8_lossy(&updated).to_string())
        })?;

        assert_eq!(final_value, "15");
        assert_eq!(store.get(b"counter")?, Some(b"15".to_vec()));

        Ok(())
    }

    #[test]
    fn test_transaction_delete() -> Result<()> {
        let store = SledStore::temporary()?;

        // Set initial values
        store.put(b"key:a", b"value_a")?;
        store.put(b"key:b", b"value_b")?;
        store.put(b"key:c", b"value_c")?;

        // Transaction that deletes some keys
        store.transaction(|tx| {
            tx.delete(b"key:a")?;
            tx.delete(b"key:b")?;
            // Keep key:c
            Ok(())
        })?;

        // Verify deletions were committed
        assert!(store.get(b"key:a")?.is_none());
        assert!(store.get(b"key:b")?.is_none());
        assert_eq!(store.get(b"key:c")?, Some(b"value_c".to_vec()));

        Ok(())
    }

    #[test]
    fn test_transaction_delete_rollback() -> Result<()> {
        let store = SledStore::temporary()?;

        // Set initial value
        store.put(b"important:key", b"precious_data")?;

        // Transaction that deletes then fails
        let result: Result<()> = store.transaction(|tx| {
            tx.delete(b"important:key")?;
            anyhow::bail!("Abort after delete")
        });

        // Transaction should have failed
        assert!(result.is_err());

        // Key should still exist (deletion rolled back)
        assert_eq!(
            store.get(b"important:key")?,
            Some(b"precious_data".to_vec())
        );

        Ok(())
    }

    #[test]
    fn test_transaction_returns_value() -> Result<()> {
        let store = SledStore::temporary()?;

        store.put(b"data:x", b"42")?;

        // Transaction that returns a computed value
        let result: i32 = store.transaction(|tx| {
            let value = tx.get(b"data:x")?.unwrap_or_default();
            let num: i32 = String::from_utf8_lossy(&value).parse().unwrap_or(0);
            Ok(num * 2)
        })?;

        assert_eq!(result, 84);

        Ok(())
    }

    #[test]
    fn test_transaction_error_message() {
        let store = SledStore::temporary().unwrap();

        let result: Result<()> = store.transaction(|_tx| anyhow::bail!("Custom error message"));

        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("Custom error message"),
            "Error should contain original message: {err_msg}"
        );
    }

    #[test]
    fn test_transaction_multiple_operations_atomicity() -> Result<()> {
        let store = SledStore::temporary()?;

        // Set up a ledger scenario: transfer from alice to bob
        store.put(b"balance:alice", b"1000")?;
        store.put(b"balance:bob", b"500")?;

        // Successful transfer
        store.transaction(|tx| {
            // Read balances
            let alice = String::from_utf8_lossy(&tx.get(b"balance:alice")?.unwrap_or_default())
                .parse::<i32>()
                .unwrap_or(0);
            let bob = String::from_utf8_lossy(&tx.get(b"balance:bob")?.unwrap_or_default())
                .parse::<i32>()
                .unwrap_or(0);

            // Transfer 100
            let transfer = 100;
            tx.set(b"balance:alice", (alice - transfer).to_string().as_bytes())?;
            tx.set(b"balance:bob", (bob + transfer).to_string().as_bytes())?;
            tx.set(b"log:transfer:001", b"alice->bob:100")?;

            Ok(())
        })?;

        // Verify the transfer completed atomically
        assert_eq!(store.get(b"balance:alice")?, Some(b"900".to_vec()));
        assert_eq!(store.get(b"balance:bob")?, Some(b"600".to_vec()));
        assert_eq!(
            store.get(b"log:transfer:001")?,
            Some(b"alice->bob:100".to_vec())
        );

        Ok(())
    }
}
