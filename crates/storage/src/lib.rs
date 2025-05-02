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
use icn_identity::{IdentityId, IdentityScope};
use multihash::{self, Code, MultihashDigest};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing;

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
    /// Get a value by CID
    async fn get(&self, key: &Cid) -> StorageResult<Option<Vec<u8>>>;
    
    /// Put a value and return its CID
    async fn put(&self, value: &[u8]) -> StorageResult<Cid>;
    
    /// Check if a CID exists in storage
    async fn contains(&self, key: &Cid) -> StorageResult<bool>;
    
    /// Delete a value by CID
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
    
    async fn put(&self, value: &[u8]) -> StorageResult<Cid> {
        // Hash the content with SHA-256
        let mh = Code::Sha2_256.digest(value);
        
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
}

impl InMemoryBlobStore {
    /// Create a new in-memory blob store with no size limits
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size: 0, // No limit by default
            kad_announcer: None,
        }
    }
    
    /// Create a new in-memory blob store with a maximum blob size
    pub fn with_max_size(max_blob_size: u64) -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size,
            kad_announcer: None,
        }
    }
    
    /// Create a new in-memory blob store with an announcement channel
    pub fn with_announcer(kad_announcer: mpsc::Sender<Cid>) -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size: 0,
            kad_announcer: Some(kad_announcer),
        }
    }
    
    /// Create a new in-memory blob store with both a size limit and an announcement channel
    pub fn with_max_size_and_announcer(max_blob_size: u64, kad_announcer: mpsc::Sender<Cid>) -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            pins: Arc::new(Mutex::new(HashSet::new())),
            max_blob_size,
            kad_announcer: Some(kad_announcer),
        }
    }
    
    /// Set the Kademlia announcer channel
    pub fn set_announcer(&mut self, kad_announcer: mpsc::Sender<Cid>) {
        self.kad_announcer = Some(kad_announcer);
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
        // Check blob size if there's a limit
        if self.max_blob_size > 0 && content.len() as u64 > self.max_blob_size {
            return Err(StorageError::BlobTooLarge(
                content.len() as u64,
                self.max_blob_size,
            ));
        }
        
        // Hash the content with SHA-256
        let mh = Code::Sha2_256.digest(content);
        
        // Create CID v0 with the digest
        let cid = Cid::new_v0(mh)
            .map_err(|e| StorageError::InvalidCid(e.to_string()))?;
        
        // Store the blob
        let mut blobs = self.blobs.lock().await;
        blobs.insert(cid, content.to_vec());
        
        // Announce the blob via Kademlia if announcer is available
        if let Some(sender) = &self.kad_announcer {
            match sender.send(cid).await {
                Ok(_) => {
                    tracing::debug!(%cid, "Sent CID for Kademlia announcement");
                },
                Err(e) => {
                    tracing::error!(%cid, "Failed to send CID for Kademlia announcement: {}", e);
                    // Continue anyway since the blob was stored successfully
                }
            }
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
        pins.insert(*cid);
        
        Ok(())
    }
    
    async fn unpin_blob(&self, cid: &Cid) -> StorageResult<()> {
        let mut pins = self.pins.lock().await;
        pins.remove(cid);
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    
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
} 