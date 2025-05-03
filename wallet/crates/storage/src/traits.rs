use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use crate::error::StorageResult;
use std::path::Path;
use chrono::{DateTime, Utc};

/// StorageKey is a typesafe wrapper for keys used in storage
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageKey(pub String);

impl StorageKey {
    /// Create a new storage key
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
    
    /// Create a namespaced key
    pub fn namespaced(namespace: &str, key: &str) -> Self {
        Self(format!("{}:{}", namespace, key))
    }
    
    /// Get the string representation of the key
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for StorageKey {
    fn from(key: String) -> Self {
        Self(key)
    }
}

impl<'a> From<&'a str> for StorageKey {
    fn from(key: &'a str) -> Self {
        Self(key.to_string())
    }
}

/// Metadata for a storage item version
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionMetadata {
    /// Version number (incremented sequentially)
    pub version: u64,
    
    /// When this version was created
    pub timestamp: DateTime<Utc>,
    
    /// Optional author identifier (e.g., DID)
    pub author: Option<String>,
    
    /// Optional content hash for integrity verification
    pub content_hash: Option<String>,
    
    /// Additional arbitrary metadata
    pub extra: std::collections::HashMap<String, String>,
}

impl VersionMetadata {
    /// Create new version metadata
    pub fn new(version: u64) -> Self {
        Self {
            version,
            timestamp: Utc::now(),
            author: None,
            content_hash: None,
            extra: std::collections::HashMap::new(),
        }
    }
    
    /// Create new version metadata with author
    pub fn with_author(version: u64, author: impl Into<String>) -> Self {
        let mut metadata = Self::new(version);
        metadata.author = Some(author.into());
        metadata
    }
    
    /// Add a content hash for integrity verification
    pub fn with_content_hash(mut self, hash: impl Into<String>) -> Self {
        self.content_hash = Some(hash.into());
        self
    }
    
    /// Add extra metadata
    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

/// Core storage interface for key-value operations
#[async_trait]
pub trait KeyValueStorage: Send + Sync {
    /// Store a value with the given key
    async fn set<V: Serialize + Send + Sync>(&self, key: &StorageKey, value: &V) -> StorageResult<()>;
    
    /// Retrieve a value by key
    async fn get<V: DeserializeOwned + Send + Sync>(&self, key: &StorageKey) -> StorageResult<V>;
    
    /// Check if a key exists
    async fn contains(&self, key: &StorageKey) -> StorageResult<bool>;
    
    /// Delete a key and its associated value
    async fn delete(&self, key: &StorageKey) -> StorageResult<()>;
    
    /// List all keys with a given prefix
    async fn list_keys(&self, prefix: &str) -> StorageResult<Vec<StorageKey>>;
}

/// Interface for versioned key-value operations
#[async_trait]
pub trait VersionedStorage: Send + Sync {
    /// Store a value with version metadata
    async fn set_versioned<V: Serialize + Send + Sync>(
        &self, 
        key: &str, 
        value: &V, 
        metadata: VersionMetadata
    ) -> StorageResult<()>;
    
    /// Get a specific version of a value
    async fn get_versioned<V: DeserializeOwned + Send + Sync>(
        &self, 
        key: &str, 
        version: u64
    ) -> StorageResult<(V, VersionMetadata)>;
    
    /// Get the latest version of a value
    async fn get_latest<V: DeserializeOwned + Send + Sync>(
        &self, 
        key: &str
    ) -> StorageResult<(V, VersionMetadata)>;
    
    /// List all versions for a key
    async fn list_versions(&self, key: &str) -> StorageResult<Vec<VersionMetadata>>;
    
    /// Get the latest version number for a key
    async fn get_latest_version(&self, key: &str) -> StorageResult<Option<u64>>;
}

/// Interface for storing documents (complex objects)
#[async_trait]
pub trait DocumentStorage: Send + Sync {
    /// Store a document in a collection
    async fn store_document<T: Serialize + Send + Sync>(
        &self, 
        collection: &str, 
        id: &str, 
        document: &T
    ) -> StorageResult<()>;
    
    /// Retrieve a document by collection and id
    async fn get_document<T: DeserializeOwned + Send + Sync>(
        &self, 
        collection: &str, 
        id: &str
    ) -> StorageResult<T>;
    
    /// Check if a document exists
    async fn document_exists(&self, collection: &str, id: &str) -> StorageResult<bool>;
    
    /// Delete a document
    async fn delete_document(&self, collection: &str, id: &str) -> StorageResult<()>;
    
    /// List all document ids in a collection
    async fn list_documents(&self, collection: &str) -> StorageResult<Vec<String>>;
}

/// Interface for versioned document storage
#[async_trait]
pub trait VersionedDocumentStorage: Send + Sync {
    /// Store a document with version metadata
    async fn store_versioned_document<T: Serialize + Send + Sync>(
        &self,
        collection: &str,
        id: &str,
        document: &T,
        metadata: VersionMetadata
    ) -> StorageResult<()>;
    
    /// Get a specific version of a document
    async fn get_versioned_document<T: DeserializeOwned + Send + Sync>(
        &self,
        collection: &str,
        id: &str,
        version: u64
    ) -> StorageResult<(T, VersionMetadata)>;
    
    /// Get the latest version of a document
    async fn get_latest_document<T: DeserializeOwned + Send + Sync>(
        &self,
        collection: &str,
        id: &str
    ) -> StorageResult<(T, VersionMetadata)>;
    
    /// List all versions for a document
    async fn list_document_versions(
        &self,
        collection: &str,
        id: &str
    ) -> StorageResult<Vec<VersionMetadata>>;
}

/// Interface for storing and reading raw binary data
#[async_trait]
pub trait BinaryStorage: Send + Sync {
    /// Store binary data
    async fn store_binary(&self, path: &str, data: &[u8]) -> StorageResult<()>;
    
    /// Retrieve binary data
    async fn get_binary(&self, path: &str) -> StorageResult<Vec<u8>>;
    
    /// Delete binary data
    async fn delete_binary(&self, path: &str) -> StorageResult<()>;
    
    /// Check if binary data exists
    async fn binary_exists(&self, path: &str) -> StorageResult<bool>;
}

/// Interface for secure storage (for sensitive data)
#[async_trait]
pub trait SecureStorage: Send + Sync {
    /// Store sensitive data securely
    async fn store_secret<V: Serialize + Send + Sync>(&self, key: &str, value: &V) -> StorageResult<()>;
    
    /// Retrieve sensitive data
    async fn get_secret<V: DeserializeOwned + Send + Sync>(&self, key: &str) -> StorageResult<V>;
    
    /// Delete sensitive data
    async fn delete_secret(&self, key: &str) -> StorageResult<()>;
}

/// Interface for DAG node storage
#[async_trait]
pub trait DagStorage: Send + Sync {
    /// Store a DAG node
    async fn store_node<T: Serialize + Send + Sync>(&self, node_id: &str, node: &T) -> StorageResult<()>;
    
    /// Retrieve a DAG node
    async fn get_node<T: DeserializeOwned + Send + Sync>(&self, node_id: &str) -> StorageResult<T>;
    
    /// List all nodes
    async fn list_nodes(&self) -> StorageResult<Vec<String>>;
    
    /// Delete a node
    async fn delete_node(&self, node_id: &str) -> StorageResult<()>;
    
    /// Get node children
    async fn get_children(&self, node_id: &str) -> StorageResult<Vec<String>>;
    
    /// Add a child relationship
    async fn add_child(&self, parent_id: &str, child_id: &str) -> StorageResult<()>;
}

/// Interface for versioned DAG storage
#[async_trait]
pub trait VersionedDagStorage: Send + Sync {
    /// Store a DAG node with version metadata
    async fn store_node_versioned<T: Serialize + Send + Sync>(
        &self,
        node_id: &str,
        node: &T,
        metadata: VersionMetadata
    ) -> StorageResult<()>;
    
    /// Get a specific version of a DAG node
    async fn get_node_versioned<T: DeserializeOwned + Send + Sync>(
        &self,
        node_id: &str,
        version: u64
    ) -> StorageResult<(T, VersionMetadata)>;
    
    /// List all versions of a DAG node
    async fn list_node_versions(&self, node_id: &str) -> StorageResult<Vec<VersionMetadata>>;
    
    /// Get the latest version of a DAG node
    async fn get_latest_node<T: DeserializeOwned + Send + Sync>(
        &self,
        node_id: &str
    ) -> StorageResult<(T, VersionMetadata)>;
}

/// Create a storage directory if it doesn't exist
pub async fn ensure_directory(path: impl AsRef<Path>) -> StorageResult<()> {
    let path = path.as_ref();
    if !path.exists() {
        tokio::fs::create_dir_all(path).await?;
    }
    Ok(())
}

/// Initialize a storage directory with standard subdirectories
pub async fn initialize_storage_directories(base_dir: impl AsRef<Path>) -> StorageResult<()> {
    let base_dir = base_dir.as_ref();
    
    ensure_directory(base_dir).await?;
    ensure_directory(base_dir.join("kv")).await?;
    ensure_directory(base_dir.join("documents")).await?;
    ensure_directory(base_dir.join("binary")).await?;
    ensure_directory(base_dir.join("dag")).await?;
    ensure_directory(base_dir.join("secure")).await?;
    ensure_directory(base_dir.join("versions")).await?;
    
    Ok(())
} 