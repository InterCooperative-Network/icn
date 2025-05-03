/*!
# ICN Storage System

This crate implements the storage system for the ICN Runtime, including an abstract
storage backend trait and distributed blob storage primitives.

## Architectural Tenets
- Storage = Distributed Blob Storage with scoped access
- Content-addressing for integrity verification
- Federation-based replication policies defined in CCL
*/

use async_trait::async_trait;
use cid::Cid;
use futures::lock::Mutex;
use hashbrown::{HashMap, HashSet};
use sha2::{Sha256, Digest};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tracing;
use uuid::Uuid;
use std::collections::HashMap;

/// Helper function to create a multihash using SHA-256
fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    // Create a new SHA-256 multihash
    let mut buf = [0u8; 32];
    let digest = Sha256::digest(data);
    buf.copy_from_slice(digest.as_slice());
    
    // Create the multihash (code 0x12 is SHA256)
    cid::multihash::Multihash::wrap(0x12, &buf[..]).expect("valid multihash")
}

/// Errors that can occur during storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
    
    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),
    
    #[error("Blob not found: {0}")]
    BlobNotFound(String),
    
    #[error("Invalid CID: {0}")]
    InvalidCid(String),
    
    #[error("I/O error: {0}")]
    IoError(String),
    
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
    
    #[error("Async operation failed: {0}")]
    AsyncError(String),
    
    #[error("Operation not supported: {0}")]
    NotSupported(String),
    
    #[error("Blob too large: {0} bytes (max: {1} bytes)")]
    BlobTooLarge(u64, u64),
    
    #[error("Failed to pin blob: {0}")]
    PinningFailed(String),
    
    #[error("Replication failed: {0}")]
    ReplicationFailed(String),
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Trait for storage backends
/// The storage backend is an abstract interface that can be implemented by different
/// storage technologies (in-memory, local file system, distributed storage, etc.)
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Get a value by CID (deprecated, use get_blob or get_kv)
    #[deprecated(since = "0.2.0", note = "use get_blob or get_kv instead")]
    async fn get(&self, key: &Cid) -> StorageResult<Option<Vec<u8>>>;
    
    /// Put a value and return its CID (deprecated, use put_blob or put_kv)
    #[deprecated(since = "0.2.0", note = "use put_blob or put_kv instead")]
    async fn put(&self, value: &[u8]) -> StorageResult<Cid>;
    
    /// Check if a CID exists in storage (deprecated, use contains_blob or contains_kv)
    #[deprecated(since = "0.2.0", note = "use contains_blob or contains_kv instead")]
    async fn contains(&self, key: &Cid) -> StorageResult<bool>;
    
    /// Delete a value by CID (deprecated, use delete_blob or delete_kv)
    #[deprecated(since = "0.2.0", note = "use delete_blob or delete_kv instead")]
    async fn delete(&self, key: &Cid) -> StorageResult<()>;
    
    /// Start a transaction
    async fn begin_transaction(&self) -> StorageResult<()>;
    
    /// Commit a transaction
    async fn commit_transaction(&self) -> StorageResult<()>;
    
    /// Rollback a transaction
    async fn rollback_transaction(&self) -> StorageResult<()>;
    
    /// Flush changes to persistent storage
    async fn flush(&self) -> StorageResult<()>;
    
    /// List all CIDs in storage
    async fn list_all(&self) -> StorageResult<Vec<Cid>> {
        Err(StorageError::NotSupported("list_all operation not implemented for this backend".to_string()))
    }
    
    /// --- Blob Methods (Content-Addressed) ---
    
    /// Store a blob and return its content CID
    /// This method calculates a CID based on the content of the value
    async fn put_blob(&self, value_bytes: &[u8]) -> StorageResult<Cid>;
    
    /// Retrieve a blob by its content CID
    async fn get_blob(&self, content_cid: &Cid) -> StorageResult<Option<Vec<u8>>>;
    
    /// Check if a blob exists by its content CID
    async fn contains_blob(&self, content_cid: &Cid) -> StorageResult<bool>;
    
    /// Delete a blob by its content CID
    async fn delete_blob(&self, content_cid: &Cid) -> StorageResult<()>;
    
    /// --- Key-Value Methods (Key-Addressed) ---
    
    /// Store a value using a specific key CID
    /// The key CID is provided by the caller and used directly as the key
    async fn put_kv(&self, key_cid: Cid, value_bytes: Vec<u8>) -> StorageResult<()>;
    
    /// Retrieve a value using its key CID
    async fn get_kv(&self, key_cid: &Cid) -> StorageResult<Option<Vec<u8>>>;
    
    /// Check if a key exists
    async fn contains_kv(&self, key_cid: &Cid) -> StorageResult<bool>;
    
    /// Delete a value by its key CID
    async fn delete_kv(&self, key_cid: &Cid) -> StorageResult<()>;
}

/// Thread-safe, async in-memory implementation of StorageBackend
pub struct AsyncInMemoryStorage {
    data: Arc<Mutex<HashMap<Cid, Vec<u8>>>>,
    transaction: Arc<Mutex<Option<HashMap<Cid, Option<Vec<u8>>>>>>,
}

impl AsyncInMemoryStorage {
    /// Create a new async in-memory storage backend
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
            transaction: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl StorageBackend for AsyncInMemoryStorage {
    // Legacy methods (deprecated)
    #[allow(deprecated)]
    async fn get(&self, key: &Cid) -> StorageResult<Option<Vec<u8>>> {
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx) = &*tx_lock {
            if let Some(value) = tx.get(key) {
                return Ok(value.clone());
            }
        }
        
        let data_lock = self.data.lock().await;
        Ok(data_lock.get(key).cloned())
    }
    
    #[allow(deprecated)]
    async fn put(&self, value: &[u8]) -> StorageResult<Cid> {
        // Create a multihash using SHA-256
        let mh = create_sha256_multihash(value);
        
        // Create CID v0 with the digest
        let cid = Cid::new_v0(mh)
            .map_err(|e| StorageError::InvalidCid(e.to_string()))?;
        
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx) = &*tx_lock {
            let mut tx_clone = tx.clone();
            tx_clone.insert(cid, Some(value.to_vec()));
            drop(tx_lock);
            
            let mut tx_lock = self.transaction.lock().await;
            *tx_lock = Some(tx_clone);
        } else {
            let mut data_lock = self.data.lock().await;
            data_lock.insert(cid, value.to_vec());
        }
        
        Ok(cid)
    }
    
    #[allow(deprecated)]
    async fn contains(&self, key: &Cid) -> StorageResult<bool> {
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx) = &*tx_lock {
            if let Some(value) = tx.get(key) {
                return Ok(value.is_some());
            }
        }
        
        let data_lock = self.data.lock().await;
        Ok(data_lock.contains_key(key))
    }
    
    #[allow(deprecated)]
    async fn delete(&self, key: &Cid) -> StorageResult<()> {
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx) = &*tx_lock {
            let mut tx_clone = tx.clone();
            tx_clone.insert(*key, None);
            drop(tx_lock);
            
            let mut tx_lock = self.transaction.lock().await;
            *tx_lock = Some(tx_clone);
        } else {
            let mut data_lock = self.data.lock().await;
            data_lock.remove(key);
        }
        
        Ok(())
    }
    
    // Transaction methods
    async fn begin_transaction(&self) -> StorageResult<()> {
        let mut tx_lock = self.transaction.lock().await;
        
        if tx_lock.is_some() {
            return Err(StorageError::TransactionFailed("Transaction already in progress".to_string()));
        }
        
        *tx_lock = Some(HashMap::new());
        Ok(())
    }
    
    async fn commit_transaction(&self) -> StorageResult<()> {
        let tx_opt = {
            let mut tx_lock = self.transaction.lock().await;
            tx_lock.take()
        };
        
        if let Some(tx) = tx_opt {
            let mut data_lock = self.data.lock().await;
            
            for (key, value_opt) in tx {
                if let Some(value) = value_opt {
                    data_lock.insert(key, value);
                } else {
                    data_lock.remove(&key);
                }
            }
            
            Ok(())
        } else {
            Err(StorageError::TransactionFailed("No transaction in progress".to_string()))
        }
    }
    
    async fn rollback_transaction(&self) -> StorageResult<()> {
        let mut tx_lock = self.transaction.lock().await;
        
        if tx_lock.is_some() {
            *tx_lock = None;
            Ok(())
        } else {
            Err(StorageError::TransactionFailed("No transaction in progress".to_string()))
        }
    }
    
    async fn flush(&self) -> StorageResult<()> {
        // In-memory storage doesn't need to flush
        Ok(())
    }
    
    async fn list_all(&self) -> StorageResult<Vec<Cid>> {
        let data_lock = self.data.lock().await;
        Ok(data_lock.keys().cloned().collect())
    }
    
    // New blob methods
    async fn put_blob(&self, value_bytes: &[u8]) -> StorageResult<Cid> {
        // Create a multihash using SHA-256
        let mh = create_sha256_multihash(value_bytes);
        
        // Create CID v0 with the digest
        let cid = Cid::new_v0(mh)
            .map_err(|e| StorageError::InvalidCid(e.to_string()))?;
        
        // Check if we're in a transaction, handle accordingly
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx) = &*tx_lock {
            let mut tx_clone = tx.clone();
            tx_clone.insert(cid, Some(value_bytes.to_vec()));
            drop(tx_lock);
            
            let mut tx_lock = self.transaction.lock().await;
            *tx_lock = Some(tx_clone);
        } else {
            let mut data_lock = self.data.lock().await;
            data_lock.insert(cid, value_bytes.to_vec());
        }
        
        Ok(cid)
    }
    
    async fn get_blob(&self, cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        // Check if we're in a transaction
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx) = &*tx_lock {
            if let Some(value) = tx.get(cid) {
                return Ok(value.clone());
            }
        }
        
        // If not in transaction, or not found in transaction, check the main storage
        let data_lock = self.data.lock().await;
        Ok(data_lock.get(cid).cloned())
    }
    
    async fn contains_blob(&self, content_cid: &Cid) -> StorageResult<bool> {
        // Check if we're in a transaction
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx) = &*tx_lock {
            if let Some(value) = tx.get(content_cid) {
                return Ok(value.is_some());
            }
        }
        
        // If not in transaction or not found in transaction, check main storage
        let data_lock = self.data.lock().await;
        Ok(data_lock.contains_key(content_cid))
    }
    
    async fn delete_blob(&self, content_cid: &Cid) -> StorageResult<()> {
        let mut data_lock = self.data.lock().await;
        data_lock.remove(content_cid);
        Ok(())
    }
    
    // New key-value methods
    async fn put_kv(&self, key_cid: Cid, value_bytes: Vec<u8>) -> StorageResult<()> {
        // For this implementation, put_kv is simply a direct insert into the map with the provided key
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx) = &*tx_lock {
            let mut tx_clone = tx.clone();
            tx_clone.insert(key_cid, Some(value_bytes));
            drop(tx_lock);
            
            let mut tx_lock = self.transaction.lock().await;
            *tx_lock = Some(tx_clone);
        } else {
            let mut data_lock = self.data.lock().await;
            data_lock.insert(key_cid, value_bytes);
        }
        
        Ok(())
    }
    
    async fn get_kv(&self, key_cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        let data_lock = self.data.lock().await;
        Ok(data_lock.get(key_cid).cloned())
    }
    
    async fn contains_kv(&self, key_cid: &Cid) -> StorageResult<bool> {
        let data_lock = self.data.lock().await;
        Ok(data_lock.contains_key(key_cid))
    }
    
    async fn delete_kv(&self, key_cid: &Cid) -> StorageResult<()> {
        let mut data_lock = self.data.lock().await;
        data_lock.remove(key_cid);
        Ok(())
    }
}

/// Replication factor for distributed storage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationFactor {
    /// Replicate to a specific number of nodes
    Fixed(u32),
    
    /// Replicate to a fraction of nodes
    Percentage(u8),
    
    /// Replicate based on geographic distribution
    Geographic(u32),
}

/// Commands that can be sent to the federation layer
#[derive(Debug, Clone)]
pub enum FederationCommand {
    /// Announce a blob's CID via Kademlia
    AnnounceBlob(Cid),
    
    /// Identify replication targets for a pinned blob
    IdentifyReplicationTargets {
        /// The CID of the blob to replicate
        cid: Cid,
        
        /// The replication policy to apply
        policy: ReplicationPolicy,
        
        /// Context ID for policy lookup (optional, if not specified will use default)
        context_id: Option<String>,
    }
}

/// Replication policy for blobs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplicationPolicy {
    /// Replicate to N peers
    Factor(u32),
    
    /// Replicate to specific peers (PeerIds are stored as base58 strings)
    Peers(Vec<String>),
    
    /// No replication required
    None,
}

/// Trait for distributed blob storage that provides content-addressed storage with
/// pinning capabilities and replication controls
#[async_trait]
pub trait DistributedStorage: Send + Sync {
    /// Store blob content, returning its CID
    async fn put_blob(&self, content: &[u8]) -> StorageResult<Cid>;
    
    /// Retrieve blob content by CID
    async fn get_blob(&self, cid: &Cid) -> StorageResult<Option<Vec<u8>>>;
    
    /// Check if a blob exists locally or is known to the storage layer
    async fn blob_exists(&self, cid: &Cid) -> StorageResult<bool>;
    
    /// Get the size of a blob in bytes
    async fn blob_size(&self, cid: &Cid) -> StorageResult<Option<u64>>;
    
    /// Check if a blob is pinned (locally preserved)
    async fn is_pinned(&self, cid: &Cid) -> StorageResult<bool>;
    
    /// Pin a blob to ensure it's preserved locally
    async fn pin_blob(&self, cid: &Cid) -> StorageResult<()>;
    
    /// Unpin a blob, allowing it to be garbage collected if not otherwise referenced
    async fn unpin_blob(&self, cid: &Cid) -> StorageResult<()>;
    
    // TODO: Implement ReplicationStatus type and flesh out replication features
    // async fn replication_status(&self, cid: &Cid) -> StorageResult<ReplicationStatus>;
    
    // TODO: Implement proper replication with policy engine integration
    // async fn replicate_blob(&self, cid: &Cid, policy_id: &str) -> StorageResult<()>;
}

/// In-memory implementation of DistributedStorage for testing and development
pub struct InMemoryBlobStore {
    /// Map of CIDs to blob content
    blobs: Arc<Mutex<HashMap<Cid, Vec<u8>>>>,
    /// Set of pinned CIDs
    pins: Arc<Mutex<HashSet<Cid>>>,
    /// Maximum allowed blob size in bytes (0 means unlimited)
    max_blob_size: u64,
    /// Optional channel for sending CIDs to be announced via Kademlia
    kad_announcer: Option<mpsc::Sender<Cid>>,
    /// Optional channel for sending federation commands
    fed_cmd_sender: Option<mpsc::Sender<FederationCommand>>,
}

impl InMemoryBlobStore {
    /// Create a new in-memory blob store with no size limits
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size: 0, // No limit by default
            kad_announcer: None,
            fed_cmd_sender: None,
        }
    }
    
    /// Create a new in-memory blob store with a maximum blob size
    pub fn with_max_size(max_blob_size: u64) -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size,
            kad_announcer: None,
            fed_cmd_sender: None,
        }
    }
    
    /// Create a new in-memory blob store with an announcement channel
    pub fn with_announcer(kad_announcer: mpsc::Sender<Cid>) -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size: 0,
            kad_announcer: Some(kad_announcer),
            fed_cmd_sender: None,
        }
    }
    
    /// Create a new in-memory blob store with both a federation command channel and announcement channel
    pub fn with_federation(
        kad_announcer: mpsc::Sender<Cid>,
        fed_cmd_sender: mpsc::Sender<FederationCommand>
    ) -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size: 0,
            kad_announcer: Some(kad_announcer),
            fed_cmd_sender: Some(fed_cmd_sender),
        }
    }
    
    /// Create a new in-memory blob store with size limit and federation channels
    pub fn with_max_size_and_federation(
        max_blob_size: u64,
        kad_announcer: mpsc::Sender<Cid>,
        fed_cmd_sender: mpsc::Sender<FederationCommand>
    ) -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size,
            kad_announcer: Some(kad_announcer),
            fed_cmd_sender: Some(fed_cmd_sender),
        }
    }
    
    /// Set the Kademlia announcer channel
    pub fn set_announcer(&mut self, kad_announcer: mpsc::Sender<Cid>) {
        self.kad_announcer = Some(kad_announcer);
    }
    
    /// Set the federation command channel
    pub fn set_federation_sender(&mut self, fed_cmd_sender: mpsc::Sender<FederationCommand>) {
        self.fed_cmd_sender = Some(fed_cmd_sender);
    }
    
    /// Get the number of blobs in storage
    pub async fn blob_count(&self) -> usize {
        let blobs = self.blobs.lock().await;
        blobs.len()
    }
    
    /// Get the number of pinned blobs
    pub async fn pin_count(&self) -> usize {
        let pins = self.pins.lock().await;
        pins.len()
    }
}

impl Default for InMemoryBlobStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DistributedStorage for InMemoryBlobStore {
    async fn put_blob(&self, content: &[u8]) -> StorageResult<Cid> {
        // Check size limit if configured
        if self.max_blob_size > 0 && content.len() as u64 > self.max_blob_size {
            return Err(StorageError::BlobTooLarge(content.len() as u64, self.max_blob_size));
        }
        
        // Hash the content with SHA-256
        let mh = create_sha256_multihash(content);
        
        // Create CID v0 with the digest
        let cid = Cid::new_v0(mh)
            .map_err(|e| StorageError::InvalidCid(e.to_string()))?;
        
        // Store the blob
        let mut blobs = self.blobs.lock().await;
        blobs.insert(cid, content.to_vec());
        
        // If we have a Kad announcer, let's announce the CID
        if let Some(announcer) = self.kad_announcer.clone() {
            tokio::spawn(async move {
                if let Err(e) = announcer.send(cid).await {
                    tracing::warn!("Failed to send CID to announcer: {:?}", e);
                }
            });
        }
        
        Ok(cid)
    }
    
    async fn get_blob(&self, cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        let blobs = self.blobs.lock().await;
        Ok(blobs.get(cid).cloned())
    }
    
    async fn blob_exists(&self, cid: &Cid) -> StorageResult<bool> {
        let blobs = self.blobs.lock().await;
        Ok(blobs.contains_key(cid))
    }
    
    async fn blob_size(&self, cid: &Cid) -> StorageResult<Option<u64>> {
        let blobs = self.blobs.lock().await;
        Ok(blobs.get(cid).map(|blob| blob.len() as u64))
    }
    
    async fn is_pinned(&self, cid: &Cid) -> StorageResult<bool> {
        let pins = self.pins.lock().await;
        Ok(pins.contains(cid))
    }
    
    async fn pin_blob(&self, cid: &Cid) -> StorageResult<()> {
        // First check if the blob exists
        let blob_exists = self.blob_exists(cid).await?;
        if !blob_exists {
            return Err(StorageError::BlobNotFound(cid.to_string()));
        }
        
        // Pin the blob
        let mut pins = self.pins.lock().await;
        let newly_pinned = pins.insert(*cid);
        drop(pins); // Release lock as soon as possible
        
        // If this is a newly pinned blob, trigger replication
        if newly_pinned {
            tracing::debug!(%cid, "Blob newly pinned, triggering replication process");
            
            if let Some(sender) = &self.fed_cmd_sender {
                // Use a default replication policy for now
                // In a real implementation, this should be looked up based on blob context
                let default_policy = ReplicationPolicy::Factor(3); // Default to 3 replicas
                
                match sender.send(FederationCommand::IdentifyReplicationTargets {
                    cid: *cid,
                    policy: default_policy,
                    context_id: None, // Default context - will be determined by federation layer
                }).await {
                    Ok(_) => {
                        tracing::debug!(%cid, "Sent replication target identification request");
                    },
                    Err(e) => {
                        tracing::error!(%cid, "Failed to send replication target request: {}", e);
                        // Continue anyway since the local pin succeeded
                    }
                }
            } else {
                tracing::debug!(%cid, "No federation command channel available, skipping replication");
            }
        } else {
            tracing::debug!(%cid, "Blob was already pinned, not triggering replication");
        }
        
        Ok(())
    }
    
    async fn unpin_blob(&self, cid: &Cid) -> StorageResult<()> {
        let mut pins = self.pins.lock().await;
        pins.remove(cid);
        
        Ok(())
    }
}

/// A filesystem-based implementation of StorageBackend that stores data on disk
/// using a sharded directory structure based on CID prefixes.
pub struct FilesystemStorageBackend {
    /// The root directory where all data will be stored
    data_dir: std::path::PathBuf,
    /// Holds pending changes during a transaction. `None` means no active transaction.
    /// `Some(HashMap)` holds the transaction state. Key is CID.
    /// Value is `Some(Vec<u8>)` for a PUT/update, `None` for a DELETE.
    transaction: Arc<Mutex<Option<HashMap<Cid, Option<Vec<u8>>>>>>,
}

impl FilesystemStorageBackend {
    /// Create a new FilesystemStorageBackend with the given data directory
    pub fn new<P: AsRef<std::path::Path>>(data_dir: P) -> StorageResult<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        
        // Create the base directories if they don't exist
        let blobs_dir = data_dir.join("blobs");
        let kv_dir = data_dir.join("kv");
        
        // Use tokio's fs to create directories (could use std::fs for sync, but prefering async)
        if !blobs_dir.exists() {
            std::fs::create_dir_all(&blobs_dir)
                .map_err(|e| StorageError::IoError(format!("Failed to create blobs directory: {}", e)))?;
        }
        
        if !kv_dir.exists() {
            std::fs::create_dir_all(&kv_dir)
                .map_err(|e| StorageError::IoError(format!("Failed to create kv directory: {}", e)))?;
        }
        
        Ok(Self { 
            data_dir,
            transaction: Arc::new(Mutex::new(None))
        })
    }
    
    /// Helper method to get the file path for a blob CID
    fn get_blob_path(&self, cid: &Cid) -> std::path::PathBuf {
        let cid_string = cid.to_string();
        
        // Skip the multibase prefix (usually 'b' for Base32)
        let prefix_offset = if cid_string.starts_with('b') { 1 } else { 0 };
        
        // Extract the first 4 characters after the prefix for two levels of sharding
        // (2 characters each level)
        let shard_chars: Vec<char> = cid_string.chars().skip(prefix_offset).take(4).collect();
        
        if shard_chars.len() < 4 {
            // If the CID string is too short, just use what we have
            // This is a fallback and should rarely happen with proper CIDs
            let shard_level_1 = shard_chars.iter().take(2).collect::<String>();
            let shard_level_2 = shard_chars.iter().skip(2).take(2).collect::<String>();
            self.data_dir.join("blobs").join(shard_level_1).join(shard_level_2).join(&cid_string)
        } else {
            // Normal case with 4+ characters
            let shard_level_1 = shard_chars[0..2].iter().collect::<String>();
            let shard_level_2 = shard_chars[2..4].iter().collect::<String>();
            self.data_dir.join("blobs").join(shard_level_1).join(shard_level_2).join(&cid_string)
        }
    }
    
    /// Helper method to get the file path for a key-value CID
    fn get_kv_path(&self, key_cid: &Cid) -> std::path::PathBuf {
        let cid_string = key_cid.to_string();
        
        // Skip the multibase prefix (usually 'b' for Base32)
        let prefix_offset = if cid_string.starts_with('b') { 1 } else { 0 };
        
        // Extract the first 4 characters after the prefix for two levels of sharding
        // (2 characters each level)
        let shard_chars: Vec<char> = cid_string.chars().skip(prefix_offset).take(4).collect();
        
        if shard_chars.len() < 4 {
            // If the CID string is too short, just use what we have
            let shard_level_1 = shard_chars.iter().take(2).collect::<String>();
            let shard_level_2 = shard_chars.iter().skip(2).take(2).collect::<String>();
            self.data_dir.join("kv").join(shard_level_1).join(shard_level_2).join(&cid_string)
        } else {
            // Normal case with 4+ characters
            let shard_level_1 = shard_chars[0..2].iter().collect::<String>();
            let shard_level_2 = shard_chars[2..4].iter().collect::<String>();
            self.data_dir.join("kv").join(shard_level_1).join(shard_level_2).join(&cid_string)
        }
    }

    /// Internal helper method to write a blob to the filesystem
    async fn _write_blob_to_filesystem(&self, cid: &Cid, value_bytes: &[u8]) -> StorageResult<()> {
        // Calculate the path where we'll store this blob
        let final_path = self.get_blob_path(cid);
        
        // Create the directory structure if it doesn't exist
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| StorageError::IoError(format!("Failed to create directory: {}", e)))?;
        }
        
        // Generate a unique temporary file path in the same directory
        let temp_path = if let Some(parent) = final_path.parent() {
            let temp_filename = format!("{}.tmp", Uuid::new_v4());
            parent.join(temp_filename)
        } else {
            return Err(StorageError::IoError("Invalid file path".to_string()));
        };
        
        // TODO: Acquire exclusive file lock on final_path using fs2 or similar for process safety
        
        // Create and open the temporary file
        let file = tokio::fs::File::create(&temp_path).await
            .map_err(|e| StorageError::IoError(format!("Failed to create temporary file: {}", e)))?;
        
        // Use a buffered writer for better performance
        let mut writer = BufWriter::new(file);
        
        // Write the data to the temporary file
        writer.write_all(value_bytes).await
            .map_err(|e| StorageError::IoError(format!("Failed to write to temporary file: {}", e)))?;
        
        // Ensure all data is flushed to disk
        writer.flush().await
            .map_err(|e| StorageError::IoError(format!("Failed to flush temporary file: {}", e)))?;
        
        // Atomically rename the temp file to the final file path
        tokio::fs::rename(&temp_path, &final_path).await
            .map_err(|e| {
                // Try to clean up the temp file on error
                let _ = std::fs::remove_file(&temp_path);
                StorageError::IoError(format!("Failed to rename temporary file: {}", e))
            })?;
        
        // TODO: Release file lock here
        
        Ok(())
    }

    /// Internal helper method to write a key-value pair to the filesystem
    async fn _write_kv_to_filesystem(&self, key_cid: &Cid, value_bytes: &[u8]) -> StorageResult<()> {
        // Calculate the path where we'll store this key-value
        let final_path = self.get_kv_path(key_cid);
        
        // Create the directory structure if it doesn't exist
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| StorageError::IoError(format!("Failed to create directory: {}", e)))?;
        }
        
        // Generate a unique temporary file path in the same directory
        let temp_path = if let Some(parent) = final_path.parent() {
            let temp_filename = format!("{}.tmp", Uuid::new_v4());
            parent.join(temp_filename)
        } else {
            return Err(StorageError::IoError("Invalid file path".to_string()));
        };
        
        // TODO: Acquire exclusive file lock on final_path using fs2 or similar for process safety
        
        // Create and open the temporary file
        let file = tokio::fs::File::create(&temp_path).await
            .map_err(|e| StorageError::IoError(format!("Failed to create temporary file: {}", e)))?;
        
        // Use a buffered writer for better performance
        let mut writer = BufWriter::new(file);
        
        // Write the data to the temporary file
        writer.write_all(value_bytes).await
            .map_err(|e| StorageError::IoError(format!("Failed to write to temporary file: {}", e)))?;
        
        // Ensure all data is flushed to disk
        writer.flush().await
            .map_err(|e| StorageError::IoError(format!("Failed to flush temporary file: {}", e)))?;
        
        // Atomically rename the temp file to the final file path
        tokio::fs::rename(&temp_path, &final_path).await
            .map_err(|e| {
                // Try to clean up the temp file on error
                let _ = std::fs::remove_file(&temp_path);
                StorageError::IoError(format!("Failed to rename temporary file: {}", e))
            })?;
        
        // TODO: Release file lock here
        
        Ok(())
    }

    /// Internal helper method to read a blob from the filesystem
    async fn _read_blob_from_filesystem(&self, cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        // Calculate the file path
        let file_path = self.get_blob_path(cid);
        
        // TODO: Acquire shared file lock on file_path
        
        // Try to open the file
        let file_result = tokio::fs::File::open(&file_path).await;
        match file_result {
            Ok(mut file) => {
                // File exists, read its entire content
                let mut buffer = Vec::new();
                match file.read_to_end(&mut buffer).await {
                    Ok(_) => {
                        // TODO: Release shared file lock
                        // Optional integrity check could go here
                        Ok(Some(buffer))
                    },
                    Err(e) => {
                        // TODO: Release shared file lock
                        Err(StorageError::IoError(format!("Failed to read file: {}", e)))
                    }
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // TODO: Release shared file lock (if acquired)
                Ok(None)
            },
            Err(e) => {
                // TODO: Release shared file lock (if acquired)
                Err(StorageError::IoError(format!("Failed to open file: {}", e)))
            }
        }
    }

    /// Internal helper method to read a key-value pair from the filesystem
    async fn _read_kv_from_filesystem(&self, key_cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        // Calculate the file path
        let file_path = self.get_kv_path(key_cid);
        
        // TODO: Acquire shared file lock on file_path
        
        // Try to open the file
        let file_result = tokio::fs::File::open(&file_path).await;
        match file_result {
            Ok(mut file) => {
                // File exists, read its entire content
                let mut buffer = Vec::new();
                match file.read_to_end(&mut buffer).await {
                    Ok(_) => {
                        // TODO: Release shared file lock
                        Ok(Some(buffer))
                    },
                    Err(e) => {
                        // TODO: Release shared file lock
                        Err(StorageError::IoError(format!("Failed to read file: {}", e)))
                    }
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // TODO: Release shared file lock (if acquired)
                Ok(None)
            },
            Err(e) => {
                // TODO: Release shared file lock (if acquired)
                Err(StorageError::IoError(format!("Failed to open file: {}", e)))
            }
        }
    }

    /// Internal helper method to check if a blob exists on the filesystem
    async fn _blob_exists_on_filesystem(&self, cid: &Cid) -> StorageResult<bool> {
        let path = self.get_blob_path(cid);
        match tokio::fs::metadata(path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StorageError::IoError(format!("Failed to check if file exists: {}", e))),
        }
    }

    /// Internal helper method to check if a key-value pair exists on the filesystem
    async fn _kv_exists_on_filesystem(&self, key_cid: &Cid) -> StorageResult<bool> {
        let path = self.get_kv_path(key_cid);
        match tokio::fs::metadata(path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StorageError::IoError(format!("Failed to check if file exists: {}", e))),
        }
    }

    /// Internal helper method to delete a blob from the filesystem
    async fn _delete_blob_from_filesystem(&self, cid: &Cid) -> StorageResult<()> {
        let path = self.get_blob_path(cid);
        match tokio::fs::remove_file(path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // Idempotent delete
            Err(e) => Err(StorageError::IoError(format!("Failed to delete file: {}", e))),
        }
    }

    /// Internal helper method to delete a key-value pair from the filesystem
    async fn _delete_kv_from_filesystem(&self, key_cid: &Cid) -> StorageResult<()> {
        let path = self.get_kv_path(key_cid);
        match tokio::fs::remove_file(path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // Idempotent delete
            Err(e) => Err(StorageError::IoError(format!("Failed to delete file: {}", e))),
        }
    }
}

#[async_trait]
impl StorageBackend for FilesystemStorageBackend {
    // --- Legacy methods ---
    #[allow(deprecated)]
    async fn get(&self, key: &Cid) -> StorageResult<Option<Vec<u8>>> {
        // For backwards compatibility, we'll try both blob and kv storage
        if let Some(blob) = self.get_blob(key).await? {
            return Ok(Some(blob));
        }
        
        self.get_kv(key).await
    }
    
    #[allow(deprecated)]
    async fn put(&self, value: &[u8]) -> StorageResult<Cid> {
        // For backwards compatibility, just use put_blob since it's content-addressed
        self.put_blob(value).await
    }
    
    #[allow(deprecated)]
    async fn contains(&self, key: &Cid) -> StorageResult<bool> {
        // Check both blob and kv storage
        if self.contains_blob(key).await? {
            return Ok(true);
        }
        
        self.contains_kv(key).await
    }
    
    #[allow(deprecated)]
    async fn delete(&self, key: &Cid) -> StorageResult<()> {
        // Try to delete from both blob and kv storage
        // We don't care if one fails if the other succeeds
        let _ = self.delete_blob(key).await;
        let _ = self.delete_kv(key).await;
        
        // Return success regardless of whether anything was actually deleted
        Ok(())
    }
    
    // --- Transaction methods ---
    async fn begin_transaction(&self) -> StorageResult<()> {
        let mut tx_lock = self.transaction.lock().await;
        
        if tx_lock.is_some() {
            return Err(StorageError::TransactionFailed("Transaction already in progress".to_string()));
        }
        
        // Start new transaction
        *tx_lock = Some(HashMap::new());
        Ok(())
    }
    
    async fn commit_transaction(&self) -> StorageResult<()> {
        let tx_opt = {
            let mut tx_lock = self.transaction.lock().await;
            tx_lock.take()
        };
        
        if let Some(tx) = tx_opt {
            // Apply all changes from transaction
            for (cid, value_opt) in tx {
                if is_blob_cid(&cid) {
                    // Handle blob operations
                    if let Some(value) = value_opt {
                        // Write/update blob
                        self._write_blob_to_filesystem(&cid, &value).await?;
                    } else {
                        // Delete blob
                        self._delete_blob_from_filesystem(&cid).await?;
                    }
                } else {
                    // Handle key-value operations
                    if let Some(value) = value_opt {
                        // Write/update key-value
                        self._write_kv_to_filesystem(&cid, &value).await?;
                    } else {
                        // Delete key-value
                        self._delete_kv_from_filesystem(&cid).await?;
                    }
                }
            }
            Ok(())
        } else {
            Err(StorageError::TransactionFailed("No transaction in progress".to_string()))
        }
    }
    
    async fn rollback_transaction(&self) -> StorageResult<()> {
        let mut tx_lock = self.transaction.lock().await;
        
        if tx_lock.is_some() {
            *tx_lock = None;
            Ok(())
        } else {
            Err(StorageError::TransactionFailed("No transaction in progress".to_string()))
        }
    }
    
    async fn flush(&self) -> StorageResult<()> {
        // Filesystem backend writes directly to disk, so flush is a no-op
        Ok(())
    }
    
    // --- Blob methods ---
    
    async fn put_blob(&self, value_bytes: &[u8]) -> StorageResult<Cid> {
        // Create a multihash using SHA-256
        let mh = create_sha256_multihash(value_bytes);
        
        // Create CID v0 with the digest
        let cid = Cid::new_v0(mh)
            .map_err(|e| StorageError::InvalidCid(e.to_string()))?;
        
        // Check if we're in a transaction
        let mut tx_lock = self.transaction.lock().await;
        
        if let Some(tx_map) = &mut *tx_lock {
            // We're in a transaction, stage the change
            tx_map.insert(cid, Some(value_bytes.to_vec()));
        } else {
            // No transaction, write directly to filesystem
            drop(tx_lock); // Release the lock before I/O
            self._write_blob_to_filesystem(&cid, value_bytes).await?;
        }
        
        Ok(cid)
    }
    
    async fn get_blob(&self, cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        // Check if we're in a transaction
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx_map) = &*tx_lock {
            // Check if the CID exists in the transaction map
            if let Some(value_opt) = tx_map.get(cid) {
                // Found in transaction
                return Ok(value_opt.clone());
            }
            // Not found in transaction, release lock and continue to filesystem check
        }
        
        // No transaction or not found in transaction, check the filesystem
        drop(tx_lock); // Release the lock before I/O
        self._read_blob_from_filesystem(cid).await
    }
    
    async fn contains_blob(&self, content_cid: &Cid) -> StorageResult<bool> {
        // Check if we're in a transaction
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx_map) = &*tx_lock {
            // Check if the CID exists in the transaction map
            if let Some(value_opt) = tx_map.get(content_cid) {
                // Found in transaction, check if it's a delete
                return Ok(value_opt.is_some());
            }
            // Not found in transaction, release lock and continue to filesystem check
        }
        
        // No transaction or not found in transaction, check the filesystem
        drop(tx_lock); // Release the lock before I/O
        self._blob_exists_on_filesystem(content_cid).await
    }
    
    async fn delete_blob(&self, content_cid: &Cid) -> StorageResult<()> {
        // Check if we're in a transaction
        let mut tx_lock = self.transaction.lock().await;
        
        if let Some(tx_map) = &mut *tx_lock {
            // We're in a transaction, stage the delete
            tx_map.insert(*content_cid, None);
        } else {
            // No transaction, delete directly from filesystem
            drop(tx_lock); // Release the lock before I/O
            self._delete_blob_from_filesystem(content_cid).await?;
        }
        
        Ok(())
    }
    
    // --- Key-Value methods ---
    
    async fn put_kv(&self, key_cid: Cid, value_bytes: Vec<u8>) -> StorageResult<()> {
        // Check if we're in a transaction
        let mut tx_lock = self.transaction.lock().await;
        
        if let Some(tx_map) = &mut *tx_lock {
            // We're in a transaction, stage the change
            tx_map.insert(key_cid, Some(value_bytes));
        } else {
            // No transaction, write directly to filesystem
            drop(tx_lock); // Release the lock before I/O
            self._write_kv_to_filesystem(&key_cid, &value_bytes).await?;
        }
        
        Ok(())
    }
    
    async fn get_kv(&self, key_cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        // Check if we're in a transaction
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx_map) = &*tx_lock {
            // Check if the CID exists in the transaction map
            if let Some(value_opt) = tx_map.get(key_cid) {
                // Found in transaction
                return Ok(value_opt.clone());
            }
            // Not found in transaction, release lock and continue to filesystem check
        }
        
        // No transaction or not found in transaction, check the filesystem
        drop(tx_lock); // Release the lock before I/O
        self._read_kv_from_filesystem(key_cid).await
    }
    
    async fn contains_kv(&self, key_cid: &Cid) -> StorageResult<bool> {
        // Check if we're in a transaction
        let tx_lock = self.transaction.lock().await;
        
        if let Some(tx_map) = &*tx_lock {
            // Check if the CID exists in the transaction map
            if let Some(value_opt) = tx_map.get(key_cid) {
                // Found in transaction, check if it's a delete
                return Ok(value_opt.is_some());
            }
            // Not found in transaction, release lock and continue to filesystem check
        }
        
        // No transaction or not found in transaction, check the filesystem
        drop(tx_lock); // Release the lock before I/O
        self._kv_exists_on_filesystem(key_cid).await
    }
    
    async fn delete_kv(&self, key_cid: &Cid) -> StorageResult<()> {
        // Check if we're in a transaction
        let mut tx_lock = self.transaction.lock().await;
        
        if let Some(tx_map) = &mut *tx_lock {
            // We're in a transaction, stage the delete
            tx_map.insert(*key_cid, None);
        } else {
            // No transaction, delete directly from filesystem
            drop(tx_lock); // Release the lock before I/O
            self._delete_kv_from_filesystem(key_cid).await?;
        }
        
        Ok(())
    }
}

/// Helper function to determine if a CID is a blob content CID or a key CID
/// This is a simplified heuristic; you might want to improve this based on your CID scheme
fn is_blob_cid(cid: &Cid) -> bool {
    // In this implementation, we're assuming CIDs starting with "Qm" (base58 v0 CIDs) 
    // or "baf..." (base32 v1 CIDs) are content-addressed blobs
    let cid_str = cid.to_string();
    cid_str.starts_with("Qm") || (cid_str.starts_with("b") && cid_str.chars().nth(1) == Some('a'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    
    // We import tempfile only in the test module
    #[cfg(test)]
    use tempfile;

    #[tokio::test]
    async fn test_async_storage_blob_ops() -> Result<(), Box<dyn Error>> {
        let storage = AsyncInMemoryStorage::new();
        
        // Create a test blob
        let content = b"test content";
        
        // Compute the expected CID
        let mh = create_sha256_multihash(content);
        let expected_cid = Cid::new_v0(mh)?;
        
        // Test put_blob
        let cid = storage.put_blob(content).await?;
        assert_eq!(cid, expected_cid);
        
        // Test get_blob
        let retrieved = storage.get_blob(&cid).await?;
        assert_eq!(retrieved, Some(content.to_vec()));
        
        // Test contains_blob
        assert!(storage.contains_blob(&cid).await?);
        
        // Test delete_blob
        storage.delete_blob(&cid).await?;
        assert_eq!(storage.get_blob(&cid).await?, None);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_async_storage_kv_ops() -> Result<(), Box<dyn Error>> {
        let storage = AsyncInMemoryStorage::new();
        
        // Create a key CID
        let key_str = "test_key";
        let key_hash = create_sha256_multihash(key_str.as_bytes());
        let key_cid = Cid::new_v1(0x71, key_hash);
        
        // Test KV operations
        let test_value = b"Test value for KV operations".to_vec();
        storage.put_kv(key_cid, test_value.clone()).await?;
        
        // Verify the value can be retrieved
        let retrieved = storage.get_kv(&key_cid).await?.unwrap();
        assert_eq!(retrieved, test_value);
        
        // Verify contains operation
        assert!(storage.contains_kv(&key_cid).await?);
        
        // Delete the value
        storage.delete_kv(&key_cid).await?;
        
        // Verify it's gone
        assert!(!storage.contains_kv(&key_cid).await?);
        assert!(storage.get_kv(&key_cid).await?.is_none());
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_async_storage_transactions() -> Result<(), Box<dyn Error>> {
        // Create a new in-memory storage
        let storage = AsyncInMemoryStorage::new();
        
        // Create test data
        let test_data = b"Test data for transactions".to_vec();
        
        // Begin a transaction
        storage.begin_transaction().await?;
        
        // Perform operations inside the transaction
        let cid = storage.put_blob(&test_data).await?;
        
        // The data should be accessible within the transaction
        assert!(storage.contains_blob(&cid).await?);
        
        // But not yet committed to the main storage
        storage.rollback_transaction().await?;
        
        // After rollback, the data should not be accessible
        assert!(!storage.contains_blob(&cid).await?);
        
        // Try again with a commit
        storage.begin_transaction().await?;
        let cid = storage.put_blob(&test_data).await?;
        storage.commit_transaction().await?;
        
        // Now the data should be accessible
        assert!(storage.contains_blob(&cid).await?);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_in_memory_blob_store() -> Result<(), Box<dyn Error>> {
        // Create a new blob store with a 1MB max size
        let store = InMemoryBlobStore::with_max_size(1024 * 1024);
        
        // Create some test data
        let content = b"This is a test blob".to_vec();
        
        // Store the blob and get its CID
        let cid = store.put_blob(&content).await?;
        
        // Check that the blob exists
        assert!(store.blob_exists(&cid).await?);
        
        // Get the blob content
        let retrieved = store.get_blob(&cid).await?.unwrap();
        assert_eq!(retrieved, content);
        
        // Check the blob size
        let size = store.blob_size(&cid).await?.unwrap();
        assert_eq!(size, content.len() as u64);
        
        // Check that the blob is not pinned by default
        assert!(!store.is_pinned(&cid).await?);
        
        // Pin the blob
        store.pin_blob(&cid).await?;
        
        // Check that the blob is now pinned
        assert!(store.is_pinned(&cid).await?);
        
        // Unpin the blob
        store.unpin_blob(&cid).await?;
        
        // Check that the blob is no longer pinned
        assert!(!store.is_pinned(&cid).await?);
        
        // Check blob count and pin count
        assert_eq!(store.blob_count().await, 1);
        assert_eq!(store.pin_count().await, 0);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_blob_size_limit() -> Result<(), Box<dyn Error>> {
        // Create a blob store with a very small size limit
        let store = InMemoryBlobStore::with_max_size(10);
        
        // Create a blob that's within the limit
        let small_content = b"Small".to_vec();
        let small_cid = store.put_blob(&small_content).await?;
        assert!(store.blob_exists(&small_cid).await?);
        
        // Create a blob that exceeds the limit
        let large_content = b"This is too large for our limit".to_vec();
        let result = store.put_blob(&large_content).await;
        
        // Verify we get a BlobTooLarge error
        assert!(matches!(result, Err(StorageError::BlobTooLarge(_, _))));
        
        Ok(())
    }

    #[tokio::test]
    async fn test_filesystem_storage_backend() -> Result<(), Box<dyn Error>> {
        // Create a temporary directory for testing
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();
        
        // Create a new FilesystemStorageBackend
        let storage = FilesystemStorageBackend::new(temp_path)?;
        
        // Test put_blob and get_blob
        let test_data = b"Hello, filesystem storage!";
        let cid = storage.put_blob(test_data).await?;
        
        // Verify the CID
        assert!(cid.to_string().starts_with("Qm"));
        
        // Retrieve the data
        let retrieved = storage.get_blob(&cid).await?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), test_data);
        
        // Test contains_blob
        let exists = storage.contains_blob(&cid).await?;
        assert!(exists);
        
        // Test key-value operations
        let kv_data = b"Key-value data";
        storage.put_kv(cid, kv_data.to_vec()).await?;
        
        let kv_retrieved = storage.get_kv(&cid).await?;
        assert!(kv_retrieved.is_some());
        assert_eq!(kv_retrieved.unwrap(), kv_data);
        
        // Check that both blob and kv files exist
        let blob_path = storage.get_blob_path(&cid);
        let kv_path = storage.get_kv_path(&cid);
        
        assert!(blob_path.exists());
        assert!(kv_path.exists());
        
        // Delete operations
        storage.delete_blob(&cid).await?;
        let blob_exists = storage.contains_blob(&cid).await?;
        assert!(!blob_exists);
        
        storage.delete_kv(&cid).await?;
        let kv_exists = storage.contains_kv(&cid).await?;
        assert!(!kv_exists);
        
        Ok(())
    }
} 