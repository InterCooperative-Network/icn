/*!
 * Storage models and interfaces
 *
 * This module defines the core storage interfaces used by the ICN Runtime.
 */

use crate::Cid;
use crate::dag::{DagNode, DagNodeBuilder};
use async_trait::async_trait;
use thiserror::Error;

/// Errors that can occur during storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    /// Key not found
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    
    /// Serialization failed
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
    
    /// Deserialization failed
    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),
    
    /// Blob not found
    #[error("Blob not found: {0}")]
    BlobNotFound(String),
    
    /// Invalid CID
    #[error("Invalid CID: {0}")]
    InvalidCid(String),
    
    /// I/O error
    #[error("I/O error: {0}")]
    IoError(String),
    
    /// Transaction failed
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
    
    /// Operation not supported
    #[error("Operation not supported: {0}")]
    NotSupported(String),
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// A basic storage backend for content-addressed data
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store a blob and return its content CID
    async fn put_blob(&self, value_bytes: &[u8]) -> StorageResult<Cid>;
    
    /// Retrieve a blob by its content CID
    async fn get_blob(&self, content_cid: &Cid) -> StorageResult<Option<Vec<u8>>>;
    
    /// Check if a blob exists by its content CID
    async fn contains_blob(&self, content_cid: &Cid) -> StorageResult<bool>;
    
    /// Delete a blob by its content CID
    async fn delete_blob(&self, content_cid: &Cid) -> StorageResult<()>;
    
    /// Store a value using a specific key CID
    async fn put_kv(&self, key_cid: Cid, value_bytes: Vec<u8>) -> StorageResult<()>;
    
    /// Retrieve a value using its key CID
    async fn get_kv(&self, key_cid: &Cid) -> StorageResult<Option<Vec<u8>>>;
    
    /// Check if a key exists
    async fn contains_kv(&self, key_cid: &Cid) -> StorageResult<bool>;
    
    /// Delete a value by its key CID
    async fn delete_kv(&self, key_cid: &Cid) -> StorageResult<()>;
    
    /// Begin a transaction
    async fn begin_transaction(&self) -> StorageResult<()>;
    
    /// Commit a transaction
    async fn commit_transaction(&self) -> StorageResult<()>;
    
    /// Rollback a transaction
    async fn rollback_transaction(&self) -> StorageResult<()>;
}

/// A basic storage manager for entity-specific data
#[async_trait]
pub trait BasicStorageManager: Send + Sync {
    /// Stores a binary blob and returns its content-addressed CID
    async fn store_blob(&self, data: &[u8]) -> crate::Result<Cid>;

    /// Retrieves a binary blob by its CID
    async fn get_blob(&self, cid: &Cid) -> crate::Result<Option<Vec<u8>>>;
    
    /// Checks if a blob exists
    async fn contains_blob(&self, cid: &Cid) -> crate::Result<bool>;
    
    /// Creates a new namespace for entity storage
    async fn create_namespace(&self, namespace: &str) -> crate::Result<()>;
    
    /// Checks if a namespace exists
    async fn namespace_exists(&self, namespace: &str) -> crate::Result<bool>;
    
    /// Stores data in a specific namespace with a key
    async fn store_in_namespace(&self, namespace: &str, key: &str, data: &[u8]) -> crate::Result<()>;
    
    /// Retrieves data from a namespace by key
    async fn get_from_namespace(&self, namespace: &str, key: &str) -> crate::Result<Option<Vec<u8>>>;
    
    /// Checks if a key exists in a namespace
    async fn contains_in_namespace(&self, namespace: &str, key: &str) -> crate::Result<bool>;
}

/// An advanced storage manager for DAG operations
#[async_trait]
pub trait DagStorageManager: BasicStorageManager {
    /// Stores the genesis node for a *new* entity DAG.
    /// Creates the entity storage space if it doesn't exist.
    async fn store_new_dag_root<B: DagNodeBuilder + Send + Sync>(
        &self,
        entity_did: &str,
        node_builder: B,
    ) -> crate::Result<(Cid, DagNode)>;

    /// Stores a regular (non-genesis) DAG node for an existing entity.
    async fn store_node<B: DagNodeBuilder + Send + Sync>(
        &self,
        entity_did: &str,
        node_builder: B,
    ) -> crate::Result<(Cid, DagNode)>;

    /// Retrieves a DAG node by its CID from a specific entity's DAG.
    async fn get_node(&self, entity_did: &str, cid: &Cid) -> crate::Result<Option<DagNode>>;

    /// Checks if a DAG node exists within a specific entity's DAG.
    async fn contains_node(&self, entity_did: &str, cid: &Cid) -> crate::Result<bool>;

    /// Retrieves the raw bytes of a DAG node
    async fn get_node_bytes(&self, entity_did: &str, cid: &Cid) -> crate::Result<Option<Vec<u8>>>;

    /// Stores multiple nodes for an entity in a single batch operation.
    /// Note: This requires the builder type B to be Clone + Send + Sync.
    async fn store_nodes_batch<B: DagNodeBuilder + Clone + Send + Sync>(
        &self,
        entity_did: &str,
        node_builders: Vec<B>,
    ) -> crate::Result<Vec<(Cid, DagNode)>>;
} 