#![deny(missing_docs)]
#![deny(dead_code)]
#![deny(unused_imports)]

/*!
# ICN Storage System

This crate implements the storage system for the ICN Runtime, including an abstract
storage backend trait and distributed blob storage primitives.

## Architectural Tenets
- Storage = Distributed Blob Storage with scoped access
- Content-addressing for integrity verification
- Federation-based replication policies defined in CCL
*/

// Include test module
#[cfg(test)]
mod tests;

use async_trait::async_trait;
use cid::Cid;
use futures::lock::Mutex;
use std::collections::{HashMap, HashSet};
use sha2::{Sha256, Digest};
use std::sync::Arc;
use std::path::Path;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tracing;
use uuid::Uuid;
use anyhow::{anyhow, Context, Result};

// Local crate imports using proper path notation
#[cfg(any(feature = "with-dag", test))]
pub use icn_dag::{DagNode, DagNodeBuilder, codec::DagCborCodec};
use libipld::codec::Codec;

// Conditional RocksDB imports
#[cfg(feature = "rocksdb-storage")]
use rocksdb::{DBWithThreadMode, MultiThreaded, Options, ColumnFamilyDescriptor, WriteBatch, IteratorMode};

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

/// A storage backend represents a persistent store for content-addressed data.
/// 
/// Implementations of this trait provide basic CRUD operations for content-addressed
/// storage (blobs) and key-value storage. The difference is that in content-addressed
/// storage, the key (CID) is derived from the content itself, while in key-value
/// storage, the key is provided by the caller.
/// 
/// This trait is meant to be implemented by different storage technologies
/// (in-memory, local file system, distributed storage, etc.)
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

/// A trait for managing storage that is independent of DAG-specific functionality
#[async_trait]
pub trait BasicStorageManager: Send + Sync {
    /// Stores a binary blob and returns its content-addressed CID.
    async fn store_blob(&self, data: &[u8]) -> Result<Cid>;

    /// Retrieves a binary blob by its CID.
    async fn get_blob(&self, cid: &Cid) -> Result<Option<Vec<u8>>>;
    
    /// Checks if a blob exists
    async fn contains_blob(&self, cid: &Cid) -> Result<bool>;
    
    /// Creates a new namespace for entity storage
    async fn create_namespace(&self, namespace: &str) -> Result<()>;
    
    /// Checks if a namespace exists
    async fn namespace_exists(&self, namespace: &str) -> Result<bool>;
    
    /// Stores data in a specific namespace with a key
    async fn store_in_namespace(&self, namespace: &str, key: &str, data: &[u8]) -> Result<()>;
    
    /// Retrieves data from a namespace by key
    async fn get_from_namespace(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>>;
    
    /// Checks if a key exists in a namespace
    async fn contains_in_namespace(&self, namespace: &str, key: &str) -> Result<bool>;
}

/// A storage manager provides higher-level operations for managing DAG nodes 
/// with entity isolation.
/// 
/// This trait builds on top of StorageBackend to provide entity-specific storage
/// for DAG nodes. It allows for organizing data by entity (identified by a DID),
/// and provides methods for storing, retrieving, and querying DAG nodes within 
/// each entity's namespace.
/// 
/// Implementations should ensure that data from different entities is properly
/// isolated, and that operations on one entity's data don't affect other entities.
#[async_trait]
pub trait StorageManager: Send + Sync {
    /// Stores the genesis node for a *new* entity DAG.
    /// Calculates the node's CID and persists it within the entity's designated storage area.
    /// Creates the entity storage space if it doesn't exist.
    /// Returns the CID and the persisted DagNode.
    async fn store_new_dag_root(
        &self,
        entity_did: &str,
        node_builder: DagNodeBuilder,
    ) -> Result<(Cid, DagNode)>;

    /// Stores a regular (non-genesis) DAG node for an existing entity.
    /// Calculates the node's CID and persists it within the entity's storage area.
    /// Returns the CID and the persisted DagNode.
    /// Assumes the entity's storage space already exists.
    async fn store_node(
        &self,
        entity_did: &str,
        node_builder: DagNodeBuilder,
    ) -> Result<(Cid, DagNode)>;

    /// Retrieves a DAG node by its CID from a specific entity's DAG storage.
    async fn get_node(&self, entity_did: &str, cid: &Cid) -> Result<Option<DagNode>>;

    /// Checks if a DAG node exists within a specific entity's DAG storage.
    async fn contains_node(&self, entity_did: &str, cid: &Cid) -> Result<bool>;

    /// Retrieves the bytes of a DAG node by its CID from a specific entity's DAG.
    /// Useful if the caller wants to handle deserialization themselves.
    async fn get_node_bytes(&self, entity_did: &str, cid: &Cid) -> Result<Option<Vec<u8>>>;

    /// Stores multiple nodes for an entity in a single batch operation.
    /// This is more efficient than calling store_node repeatedly.
    async fn store_nodes_batch(
        &self,
        entity_did: &str,
        node_builders: Vec<DagNodeBuilder>,
    ) -> Result<Vec<(Cid, DagNode)>>;

    /// Stores a binary blob and returns its content-addressed CID.
    /// This is an internal utility method primarily used by the DAG storage methods.
    async fn store_blob(&self, data: &[u8]) -> Result<Cid>;

    /// Retrieves a binary blob by its CID.
    /// This is an internal utility method primarily used by the DAG storage methods.
    async fn get_blob(&self, cid: &Cid) -> Result<Option<Vec<u8>>>;
}

// The DagStorageManager trait is conditionally defined when the dag feature is available
#[cfg(any(feature = "with-dag", test))]
pub mod dag_storage {
    use super::*;
    
    /// A storage manager provides higher-level operations for managing DAG nodes 
    /// with entity isolation.
    /// 
    /// This trait builds on top of BasicStorageManager to provide entity-specific storage
    /// for DAG nodes. It allows for organizing data by entity (identified by a DID),
    /// and provides methods for storing, retrieving, and querying DAG nodes within 
    /// each entity's namespace.
    /// 
    /// Implementations should ensure that data from different entities is properly
    /// isolated, and that operations on one entity's data don't affect other entities.
    #[async_trait]
    pub trait DagStorageManager: BasicStorageManager {
        /// Stores the genesis node for a *new* entity DAG.
        /// Calculates the node's CID and persists it within the entity's designated storage area.
        /// Creates the entity storage space if it doesn't exist.
        /// Returns the CID and the persisted DagNode.
        async fn store_new_dag_root(
            &self,
            entity_did: &str,
            node_builder: super::DagNodeBuilder,
        ) -> Result<(Cid, super::DagNode)>;
    
        /// Stores a regular (non-genesis) DAG node for an existing entity.
        /// Calculates the node's CID and persists it within the entity's storage area.
        /// Returns the CID and the persisted DagNode.
        /// Assumes the entity's storage space already exists.
        async fn store_node(
            &self,
            entity_did: &str,
            node_builder: super::DagNodeBuilder,
        ) -> Result<(Cid, super::DagNode)>;
    
        /// Retrieves a DAG node by its CID from a specific entity's DAG storage.
        async fn get_node(&self, entity_did: &str, cid: &Cid) -> Result<Option<super::DagNode>>;
    
        /// Checks if a DAG node exists within a specific entity's DAG storage.
        async fn contains_node(&self, entity_did: &str, cid: &Cid) -> Result<bool>;
    
        /// Retrieves the bytes of a DAG node by its CID from a specific entity's DAG.
        /// Useful if the caller wants to handle deserialization themselves.
        async fn get_node_bytes(&self, entity_did: &str, cid: &Cid) -> Result<Option<Vec<u8>>>;
    
        /// Stores multiple nodes for an entity in a single batch operation.
        /// This is more efficient than calling store_node repeatedly.
        async fn store_nodes_batch(
            &self,
            entity_did: &str,
            node_builders: Vec<super::DagNodeBuilder>,
        ) -> Result<Vec<(Cid, super::DagNode)>>;
    }
}

// More code, implementations, etc.

// Thread-safe, async in-memory implementation of StorageBackend
// [AsyncInMemoryStorage implementation...]

// RocksDB implementation (conditionally compiled)
#[cfg(feature = "rocksdb-storage")]
mod rocksdb_storage {
    use super::*;
    use std::path::PathBuf;
    
    /// RocksDB backed storage manager implementation
    pub struct RocksDBStorageManager {
        db: Arc<DBWithThreadMode<MultiThreaded>>,
        path: PathBuf,
    }
    
    #[cfg(feature = "rocksdb-storage")]
    impl RocksDBStorageManager {
        /// Create a new RocksDB storage manager
        pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
            let path_buf = path.as_ref().to_path_buf();
            
            // Create database directory if it doesn't exist
            std::fs::create_dir_all(&path_buf)?;
            
            // Set up database options
            let mut db_opts = Options::default();
            db_opts.create_if_missing(true);
            db_opts.create_missing_column_families(true);
            
            // Try to list existing column families
            let cf_names = DBWithThreadMode::<MultiThreaded>::list_cf(&db_opts, &path_buf)
                .unwrap_or_else(|_| vec!["default".to_string(), "blobs".to_string()]);
            
            // Create column family descriptors
            let cf_descriptors: Vec<ColumnFamilyDescriptor> = cf_names
                .iter()
                .map(|name| ColumnFamilyDescriptor::new(name, Options::default()))
                .collect();
            
            // Open the database with column families
            let db = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(&db_opts, &path_buf, cf_descriptors)
                .map_err(|e| anyhow!("Failed to open RocksDB: {}", e))?;
            
            Ok(Self {
                db: Arc::new(db),
                path: path_buf,
            })
        }
        
        // Helper method to get or create a column family handle
        fn get_or_create_cf_handle(&self, cf_name: &str) -> Result<Arc<rocksdb::ColumnFamily>> {
            match self.db.cf_handle(cf_name) {
                Some(handle) => Ok(handle),
                None => {
                    // Column family doesn't exist, create it
                    self.db.create_cf(cf_name, &Options::default())?;
                    
                    // Get the newly created column family handle
                    self.db.cf_handle(cf_name)
                        .ok_or_else(|| anyhow!("Failed to get column family handle after creation: {}", cf_name))
                        .map(Arc::new)
                }
            }
        }
    }
    
    #[async_trait]
    impl StorageManager for RocksDBStorageManager {
        // [Implementation details removed for brevity]
        // These would be implemented if RocksDB were fully supported
        
        async fn store_new_dag_root(&self, entity_did: &str, node_builder: DagNodeBuilder) -> Result<(Cid, DagNode)> {
            // Generate the node
            let node = node_builder.build()?;
            let cid = node.cid;
            
            // Serialize the node
            let serialized = DagCborCodec.encode(&node)?;
            
            // Store as blob
            let _ = self.store_blob(&serialized).await?;
            
            // Get or create column family for entity
            let cf = self.get_or_create_cf_handle(entity_did)?;
            
            // Store in entity's column family
            self.db.put_cf(&cf, cid.to_bytes(), serialized)?;
            
            Ok((cid, node))
        }
        
        async fn store_node(&self, entity_did: &str, node_builder: DagNodeBuilder) -> Result<(Cid, DagNode)> {
            // Implementation omitted for brevity
            // Would follow similar pattern as store_new_dag_root
            unimplemented!("Method not implemented")
        }
        
        async fn get_node(&self, entity_did: &str, cid: &Cid) -> Result<Option<DagNode>> {
            // Implementation omitted for brevity
            unimplemented!("Method not implemented")
        }
        
        async fn contains_node(&self, entity_did: &str, cid: &Cid) -> Result<bool> {
            // Implementation omitted for brevity
            unimplemented!("Method not implemented")
        }
        
        async fn get_node_bytes(&self, entity_did: &str, cid: &Cid) -> Result<Option<Vec<u8>>> {
            // Implementation omitted for brevity
            unimplemented!("Method not implemented")
        }
        
        async fn store_nodes_batch(&self, entity_did: &str, node_builders: Vec<DagNodeBuilder>) -> Result<Vec<(Cid, DagNode)>> {
            // Implementation omitted for brevity
            unimplemented!("Method not implemented")
        }
        
        async fn store_blob(&self, data: &[u8]) -> Result<Cid> {
            // Hash the data to create a CID
            let mh = create_sha256_multihash(data);
            let cid = Cid::new_v1(0x55, mh); // 0x55 is the multicodec code for raw binary
            
            // Store the data with the CID as the key
            let cf = self.get_or_create_cf_handle("blobs")?;
            self.db.put_cf(&cf, cid.to_bytes(), data)?;
            
            Ok(cid)
        }
        
        async fn get_blob(&self, cid: &Cid) -> Result<Option<Vec<u8>>> {
            let cf = self.get_or_create_cf_handle("blobs")?;
            Ok(self.db.get_cf(&cf, cid.to_bytes())?)
        }
    }
}

/// Thread-safe in-memory implementation of StorageManager for testing
pub struct MemoryStorageManager {
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    nodes: Arc<Mutex<HashMap<String, HashMap<String, Vec<u8>>>>>,
}

impl MemoryStorageManager {
    /// Create a new in-memory storage manager
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
            nodes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Helper function to generate a blob key from a CID
    fn blob_key(cid: &Cid) -> String {
        cid.to_string()
    }
    
    /// Helper function to generate node keys from a DID and CID
    fn node_key(did: &str, cid: &Cid) -> (String, String) {
        (did.to_string(), cid.to_string())
    }
}

#[async_trait]
impl StorageManager for MemoryStorageManager {
    async fn store_new_dag_root(
        &self,
        entity_did: &str,
        node_builder: DagNodeBuilder,
    ) -> Result<(Cid, DagNode)> {
        // Generate the node
        let node = node_builder.build()?;
        let cid = node.cid;
        
        // Store it using store_blob
        let serialized = DagCborCodec.encode(&node)?;
        let _ = self.store_blob(&serialized).await?;
        
        // Store in entity's namespace
        let (did_key, node_key) = Self::node_key(entity_did, &cid);
        
        // Create entity namespace if it doesn't exist
        let mut nodes = self.nodes.lock().await;
        if !nodes.contains_key(&did_key) {
            nodes.insert(did_key.clone(), HashMap::new());
        }
        
        // Add node to entity namespace
        if let Some(entity_nodes) = nodes.get_mut(&did_key) {
            entity_nodes.insert(node_key, serialized);
        }
        
        Ok((cid, node))
    }
    
    async fn store_node(
        &self,
        entity_did: &str,
        node_builder: DagNodeBuilder,
    ) -> Result<(Cid, DagNode)> {
        // Generate the node
        let node = node_builder.build()?;
        let cid = node.cid;
        
        // Serialize the node
        let serialized = DagCborCodec.encode(&node)?;
        
        // Store the raw blob
        let _ = self.store_blob(&serialized).await?;
        
        // Get entity namespace keys
        let (did_key, node_key) = Self::node_key(entity_did, &cid);
        
        // Store in entity namespace
        let mut nodes = self.nodes.lock().await;
        if let Some(entity_nodes) = nodes.get_mut(&did_key) {
            entity_nodes.insert(node_key, serialized);
        } else {
            return Err(anyhow!("Entity namespace does not exist: {}", entity_did));
        }
        
        Ok((cid, node))
    }
    
    async fn get_node(&self, entity_did: &str, cid: &Cid) -> Result<Option<DagNode>> {
        // Get serialized bytes
        if let Some(bytes) = self.get_node_bytes(entity_did, cid).await? {
            // Deserialize
            let node = DagCborCodec.decode::<DagNode>(&bytes)?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }
    
    async fn contains_node(&self, entity_did: &str, cid: &Cid) -> Result<bool> {
        let (did_key, node_key) = Self::node_key(entity_did, cid);
        let nodes = self.nodes.lock().await;
        
        if let Some(entity_nodes) = nodes.get(&did_key) {
            Ok(entity_nodes.contains_key(&node_key))
        } else {
            Ok(false)
        }
    }
    
    async fn get_node_bytes(&self, entity_did: &str, cid: &Cid) -> Result<Option<Vec<u8>>> {
        let (did_key, node_key) = Self::node_key(entity_did, cid);
        let nodes = self.nodes.lock().await;
        
        if let Some(entity_nodes) = nodes.get(&did_key) {
            Ok(entity_nodes.get(&node_key).cloned())
        } else {
            Ok(None)
        }
    }
    
    async fn store_nodes_batch(
        &self,
        entity_did: &str,
        node_builders: Vec<DagNodeBuilder>,
    ) -> Result<Vec<(Cid, DagNode)>> {
        let mut results = Vec::new();
        
        for builder in node_builders {
            let (cid, node) = self.store_node(entity_did, builder).await?;
            results.push((cid, node));
        }
        
        Ok(results)
    }
    
    async fn store_blob(&self, data: &[u8]) -> Result<Cid> {
        // Hash the data to create a CID
        let mh = create_sha256_multihash(data);
        let cid = Cid::new_v1(0x55, mh); // 0x55 is the multicodec code for raw binary
        
        // Store the blob
        let key = Self::blob_key(&cid);
        let mut blobs = self.blobs.lock().await;
        blobs.insert(key, data.to_vec());
        
        Ok(cid)
    }
    
    async fn get_blob(&self, cid: &Cid) -> Result<Option<Vec<u8>>> {
        let key = Self::blob_key(cid);
        let blobs = self.blobs.lock().await;
        Ok(blobs.get(&key).cloned())
    }
}

impl Default for MemoryStorageManager {
    fn default() -> Self {
        Self::new()
    }
}

// Add test module
#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper function to create a test node builder
    #[allow(dead_code)]
    fn create_test_node_builder(payload_value: serde_json::Value) -> DagNodeBuilder {
        unimplemented!("Test helper not implemented");
    }
    
    // Add more tests as needed...
} 
