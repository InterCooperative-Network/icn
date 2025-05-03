use std::path::{Path, PathBuf};
use std::collections::HashMap;
use crate::error::{StorageError, StorageResult};
use crate::traits::{
    VersionMetadata, VersionedStorage, VersionedDocumentStorage, VersionedDagStorage,
    ensure_directory
};
use serde::{Serialize, de::DeserializeOwned};
use async_trait::async_trait;
use tokio::fs;
use tracing::{debug, warn};
use std::sync::Arc;
use sha2::{Sha256, Digest};

/// FileBasedVersionedStorage implements versioning on top of a file system
pub struct FileBasedVersionedStorage {
    /// Base directory for all versioned storage
    base_dir: PathBuf,
    
    /// Versioned key-value storage directory
    versioned_kv_dir: PathBuf,
    
    /// Versioned document storage directory
    versioned_docs_dir: PathBuf,
    
    /// Versioned DAG storage directory
    versioned_dag_dir: PathBuf,
}

impl FileBasedVersionedStorage {
    /// Create a new file-based versioned storage provider
    pub async fn new(base_dir: impl AsRef<Path>) -> StorageResult<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        let versions_dir = base_dir.join("versions");
        
        // Create required directories
        ensure_directory(&versions_dir).await?;
        
        let versioned_kv_dir = versions_dir.join("kv");
        let versioned_docs_dir = versions_dir.join("documents");
        let versioned_dag_dir = versions_dir.join("dag");
        
        ensure_directory(&versioned_kv_dir).await?;
        ensure_directory(&versioned_docs_dir).await?;
        ensure_directory(&versioned_dag_dir).await?;
        
        Ok(Self {
            base_dir,
            versioned_kv_dir,
            versioned_docs_dir,
            versioned_dag_dir,
        })
    }
    
    /// Compute a SHA-256 hash of serialized content
    fn compute_hash<T: Serialize>(&self, value: &T) -> StorageResult<String> {
        let serialized = serde_json::to_vec(value)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize for hashing: {}", e)))?;
            
        let hash = Sha256::digest(&serialized);
        Ok(format!("{:x}", hash))
    }
    
    /// Get the directory for a versioned key
    fn key_versions_dir(&self, key: &str) -> PathBuf {
        self.versioned_kv_dir.join(key)
    }
    
    /// Get the path for a specific version of a key
    fn key_version_path(&self, key: &str, version: u64) -> PathBuf {
        self.key_versions_dir(key).join(format!("v{}.json", version))
    }
    
    /// Get the metadata path for a key
    fn key_metadata_path(&self, key: &str) -> PathBuf {
        self.key_versions_dir(key).join("metadata.json")
    }
    
    /// Get the directory for versioned documents in a collection
    fn collection_versions_dir(&self, collection: &str) -> PathBuf {
        self.versioned_docs_dir.join(collection)
    }
    
    /// Get the directory for a specific document's versions
    fn document_versions_dir(&self, collection: &str, id: &str) -> PathBuf {
        self.collection_versions_dir(collection).join(id)
    }
    
    /// Get the path for a specific version of a document
    fn document_version_path(&self, collection: &str, id: &str, version: u64) -> PathBuf {
        self.document_versions_dir(collection, id).join(format!("v{}.json", version))
    }
    
    /// Get the metadata path for a document
    fn document_metadata_path(&self, collection: &str, id: &str) -> PathBuf {
        self.document_versions_dir(collection, id).join("metadata.json")
    }
    
    /// Get the directory for versioned DAG nodes
    fn node_versions_dir(&self, node_id: &str) -> PathBuf {
        self.versioned_dag_dir.join(node_id)
    }
    
    /// Get the path for a specific version of a DAG node
    fn node_version_path(&self, node_id: &str, version: u64) -> PathBuf {
        self.node_versions_dir(node_id).join(format!("v{}.json", version))
    }
    
    /// Get the metadata path for a DAG node
    fn node_metadata_path(&self, node_id: &str) -> PathBuf {
        self.node_versions_dir(node_id).join("metadata.json")
    }
    
    /// Load version metadata for an entity
    async fn load_version_metadata(&self, metadata_path: &Path) -> StorageResult<HashMap<u64, VersionMetadata>> {
        if !metadata_path.exists() {
            return Ok(HashMap::new());
        }
        
        let content = fs::read_to_string(metadata_path).await?;
        serde_json::from_str(&content)
            .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize version metadata: {}", e)))
    }
    
    /// Save version metadata for an entity
    async fn save_version_metadata(&self, metadata_path: &Path, metadata: &HashMap<u64, VersionMetadata>) -> StorageResult<()> {
        let serialized = serde_json::to_string_pretty(metadata)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize version metadata: {}", e)))?;
            
        // Ensure parent directory exists
        if let Some(parent) = metadata_path.parent() {
            ensure_directory(parent).await?;
        }
        
        fs::write(metadata_path, serialized).await?;
        
        Ok(())
    }
    
    /// Add a new version to metadata
    async fn add_version_metadata(
        &self, 
        metadata_path: &Path, 
        version: u64, 
        metadata: VersionMetadata
    ) -> StorageResult<()> {
        let mut versions = self.load_version_metadata(metadata_path).await?;
        versions.insert(version, metadata);
        self.save_version_metadata(metadata_path, &versions).await
    }
    
    /// Get the latest version number from metadata
    async fn get_latest_version_number(&self, metadata_path: &Path) -> StorageResult<Option<u64>> {
        let versions = self.load_version_metadata(metadata_path).await?;
        
        if versions.is_empty() {
            return Ok(None);
        }
        
        Ok(Some(*versions.keys().max().unwrap_or(&0)))
    }
}

#[async_trait]
impl VersionedStorage for FileBasedVersionedStorage {
    async fn set_versioned<V: Serialize + Send + Sync>(
        &self, 
        key: &str, 
        value: &V, 
        mut metadata: VersionMetadata
    ) -> StorageResult<()> {
        // Compute content hash if not provided
        if metadata.content_hash.is_none() {
            metadata.content_hash = Some(self.compute_hash(value)?);
        }
        
        // Get paths
        let key_dir = self.key_versions_dir(key);
        let version_path = self.key_version_path(key, metadata.version);
        let metadata_path = self.key_metadata_path(key);
        
        // Ensure directory exists
        ensure_directory(&key_dir).await?;
        
        // Serialize and save the value
        let serialized = serde_json::to_string_pretty(value)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize versioned value: {}", e)))?;
        
        fs::write(&version_path, serialized).await?;
        
        // Add metadata
        self.add_version_metadata(&metadata_path, metadata.version, metadata).await?;
        
        debug!("Stored version {} for key: {}", metadata.version, key);
        
        Ok(())
    }
    
    async fn get_versioned<V: DeserializeOwned + Send + Sync>(
        &self, 
        key: &str, 
        version: u64
    ) -> StorageResult<(V, VersionMetadata)> {
        // Get paths
        let version_path = self.key_version_path(key, version);
        let metadata_path = self.key_metadata_path(key);
        
        // Check if version exists
        if !version_path.exists() {
            return Err(StorageError::NotFound(format!("Version {} not found for key: {}", version, key)));
        }
        
        // Load metadata
        let versions = self.load_version_metadata(&metadata_path).await?;
        let metadata = versions.get(&version)
            .ok_or_else(|| StorageError::NotFound(format!("Metadata for version {} not found for key: {}", version, key)))?
            .clone();
        
        // Load value
        let content = fs::read_to_string(&version_path).await?;
        let value = serde_json::from_str(&content)
            .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize versioned value: {}", e)))?;
        
        Ok((value, metadata))
    }
    
    async fn get_latest<V: DeserializeOwned + Send + Sync>(
        &self, 
        key: &str
    ) -> StorageResult<(V, VersionMetadata)> {
        // Get metadata path
        let metadata_path = self.key_metadata_path(key);
        
        // Get latest version
        let latest_version = self.get_latest_version_number(&metadata_path).await?
            .ok_or_else(|| StorageError::NotFound(format!("No versions found for key: {}", key)))?;
        
        // Get that version
        self.get_versioned(key, latest_version).await
    }
    
    async fn list_versions(&self, key: &str) -> StorageResult<Vec<VersionMetadata>> {
        let metadata_path = self.key_metadata_path(key);
        
        if !metadata_path.exists() {
            return Ok(Vec::new());
        }
        
        let versions = self.load_version_metadata(&metadata_path).await?;
        
        // Sort by version number
        let mut result: Vec<_> = versions.values().cloned().collect();
        result.sort_by_key(|m| m.version);
        
        Ok(result)
    }
    
    async fn get_latest_version(&self, key: &str) -> StorageResult<Option<u64>> {
        let metadata_path = self.key_metadata_path(key);
        self.get_latest_version_number(&metadata_path).await
    }
}

#[async_trait]
impl VersionedDocumentStorage for FileBasedVersionedStorage {
    async fn store_versioned_document<T: Serialize + Send + Sync>(
        &self,
        collection: &str,
        id: &str,
        document: &T,
        mut metadata: VersionMetadata
    ) -> StorageResult<()> {
        // Compute content hash if not provided
        if metadata.content_hash.is_none() {
            metadata.content_hash = Some(self.compute_hash(document)?);
        }
        
        // Get paths
        let doc_dir = self.document_versions_dir(collection, id);
        let version_path = self.document_version_path(collection, id, metadata.version);
        let metadata_path = self.document_metadata_path(collection, id);
        
        // Ensure directory exists
        ensure_directory(&doc_dir).await?;
        
        // Serialize and save the document
        let serialized = serde_json::to_string_pretty(document)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize versioned document: {}", e)))?;
        
        fs::write(&version_path, serialized).await?;
        
        // Add metadata
        self.add_version_metadata(&metadata_path, metadata.version, metadata).await?;
        
        debug!("Stored version {} for document {}/{}", metadata.version, collection, id);
        
        Ok(())
    }
    
    async fn get_versioned_document<T: DeserializeOwned + Send + Sync>(
        &self,
        collection: &str,
        id: &str,
        version: u64
    ) -> StorageResult<(T, VersionMetadata)> {
        // Get paths
        let version_path = self.document_version_path(collection, id, version);
        let metadata_path = self.document_metadata_path(collection, id);
        
        // Check if version exists
        if !version_path.exists() {
            return Err(StorageError::NotFound(
                format!("Version {} not found for document {}/{}", version, collection, id)
            ));
        }
        
        // Load metadata
        let versions = self.load_version_metadata(&metadata_path).await?;
        let metadata = versions.get(&version)
            .ok_or_else(|| StorageError::NotFound(
                format!("Metadata for version {} not found for document {}/{}", version, collection, id)
            ))?
            .clone();
        
        // Load document
        let content = fs::read_to_string(&version_path).await?;
        let document = serde_json::from_str(&content)
            .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize versioned document: {}", e)))?;
        
        Ok((document, metadata))
    }
    
    async fn get_latest_document<T: DeserializeOwned + Send + Sync>(
        &self,
        collection: &str,
        id: &str
    ) -> StorageResult<(T, VersionMetadata)> {
        // Get metadata path
        let metadata_path = self.document_metadata_path(collection, id);
        
        // Get latest version
        let latest_version = self.get_latest_version_number(&metadata_path).await?
            .ok_or_else(|| StorageError::NotFound(
                format!("No versions found for document {}/{}", collection, id)
            ))?;
        
        // Get that version
        self.get_versioned_document(collection, id, latest_version).await
    }
    
    async fn list_document_versions(
        &self,
        collection: &str,
        id: &str
    ) -> StorageResult<Vec<VersionMetadata>> {
        let metadata_path = self.document_metadata_path(collection, id);
        
        if !metadata_path.exists() {
            return Ok(Vec::new());
        }
        
        let versions = self.load_version_metadata(&metadata_path).await?;
        
        // Sort by version number
        let mut result: Vec<_> = versions.values().cloned().collect();
        result.sort_by_key(|m| m.version);
        
        Ok(result)
    }
}

#[async_trait]
impl VersionedDagStorage for FileBasedVersionedStorage {
    async fn store_node_versioned<T: Serialize + Send + Sync>(
        &self,
        node_id: &str,
        node: &T,
        mut metadata: VersionMetadata
    ) -> StorageResult<()> {
        // Compute content hash if not provided
        if metadata.content_hash.is_none() {
            metadata.content_hash = Some(self.compute_hash(node)?);
        }
        
        // Get paths
        let node_dir = self.node_versions_dir(node_id);
        let version_path = self.node_version_path(node_id, metadata.version);
        let metadata_path = self.node_metadata_path(node_id);
        
        // Ensure directory exists
        ensure_directory(&node_dir).await?;
        
        // Serialize and save the node
        let serialized = serde_json::to_string_pretty(node)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize versioned DAG node: {}", e)))?;
        
        fs::write(&version_path, serialized).await?;
        
        // Add metadata
        self.add_version_metadata(&metadata_path, metadata.version, metadata).await?;
        
        debug!("Stored version {} for DAG node {}", metadata.version, node_id);
        
        Ok(())
    }
    
    async fn get_node_versioned<T: DeserializeOwned + Send + Sync>(
        &self,
        node_id: &str,
        version: u64
    ) -> StorageResult<(T, VersionMetadata)> {
        // Get paths
        let version_path = self.node_version_path(node_id, version);
        let metadata_path = self.node_metadata_path(node_id);
        
        // Check if version exists
        if !version_path.exists() {
            return Err(StorageError::NotFound(
                format!("Version {} not found for DAG node {}", version, node_id)
            ));
        }
        
        // Load metadata
        let versions = self.load_version_metadata(&metadata_path).await?;
        let metadata = versions.get(&version)
            .ok_or_else(|| StorageError::NotFound(
                format!("Metadata for version {} not found for DAG node {}", version, node_id)
            ))?
            .clone();
        
        // Load node
        let content = fs::read_to_string(&version_path).await?;
        let node = serde_json::from_str(&content)
            .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize versioned DAG node: {}", e)))?;
        
        Ok((node, metadata))
    }
    
    async fn list_node_versions(&self, node_id: &str) -> StorageResult<Vec<VersionMetadata>> {
        let metadata_path = self.node_metadata_path(node_id);
        
        if !metadata_path.exists() {
            return Ok(Vec::new());
        }
        
        let versions = self.load_version_metadata(&metadata_path).await?;
        
        // Sort by version number
        let mut result: Vec<_> = versions.values().cloned().collect();
        result.sort_by_key(|m| m.version);
        
        Ok(result)
    }
    
    async fn get_latest_node<T: DeserializeOwned + Send + Sync>(
        &self,
        node_id: &str
    ) -> StorageResult<(T, VersionMetadata)> {
        // Get metadata path
        let metadata_path = self.node_metadata_path(node_id);
        
        // Get latest version
        let latest_version = self.get_latest_version_number(&metadata_path).await?
            .ok_or_else(|| StorageError::NotFound(
                format!("No versions found for DAG node {}", node_id)
            ))?;
        
        // Get that version
        self.get_node_versioned(node_id, latest_version).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use serde::{Serialize, Deserialize};
    
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }
    
    #[tokio::test]
    async fn test_versioned_kv_storage() -> StorageResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create versioned storage
        let storage = FileBasedVersionedStorage::new(temp_dir.path()).await?;
        
        // Create test data
        let data_v1 = TestData {
            name: "Test".to_string(),
            value: 42,
        };
        
        let data_v2 = TestData {
            name: "Test Updated".to_string(),
            value: 84,
        };
        
        // Store versions
        let metadata_v1 = VersionMetadata::new(1);
        storage.set_versioned("test_key", &data_v1, metadata_v1.clone()).await?;
        
        let metadata_v2 = VersionMetadata::with_author(2, "test_user");
        storage.set_versioned("test_key", &data_v2, metadata_v2.clone()).await?;
        
        // Get specific version
        let (retrieved_v1, meta_v1) = storage.get_versioned::<TestData>("test_key", 1).await?;
        assert_eq!(retrieved_v1, data_v1);
        assert_eq!(meta_v1.version, 1);
        
        // Get latest version
        let (retrieved_latest, meta_latest) = storage.get_latest::<TestData>("test_key").await?;
        assert_eq!(retrieved_latest, data_v2);
        assert_eq!(meta_latest.version, 2);
        assert_eq!(meta_latest.author, Some("test_user".to_string()));
        
        // List versions
        let versions = storage.list_versions("test_key").await?;
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, 1);
        assert_eq!(versions[1].version, 2);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_versioned_document_storage() -> StorageResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create versioned storage
        let storage = FileBasedVersionedStorage::new(temp_dir.path()).await?;
        
        // Create test document
        let doc_v1 = TestData {
            name: "Document".to_string(),
            value: 100,
        };
        
        let doc_v2 = TestData {
            name: "Document Updated".to_string(),
            value: 200,
        };
        
        // Store versions
        let metadata_v1 = VersionMetadata::new(1);
        storage.store_versioned_document("test_collection", "doc1", &doc_v1, metadata_v1).await?;
        
        let metadata_v2 = VersionMetadata::new(2);
        storage.store_versioned_document("test_collection", "doc1", &doc_v2, metadata_v2).await?;
        
        // Get specific version
        let (retrieved_v1, _) = storage.get_versioned_document::<TestData>("test_collection", "doc1", 1).await?;
        assert_eq!(retrieved_v1, doc_v1);
        
        // Get latest version
        let (retrieved_latest, _) = storage.get_latest_document::<TestData>("test_collection", "doc1").await?;
        assert_eq!(retrieved_latest, doc_v2);
        
        // List versions
        let versions = storage.list_document_versions("test_collection", "doc1").await?;
        assert_eq!(versions.len(), 2);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_versioned_dag_storage() -> StorageResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create versioned storage
        let storage = FileBasedVersionedStorage::new(temp_dir.path()).await?;
        
        // Create test DAG node
        let node_v1 = TestData {
            name: "Node".to_string(),
            value: 1000,
        };
        
        let node_v2 = TestData {
            name: "Node Updated".to_string(),
            value: 2000,
        };
        
        // Store versions
        let metadata_v1 = VersionMetadata::new(1)
            .with_content_hash("test_hash_v1");
        storage.store_node_versioned("node1", &node_v1, metadata_v1).await?;
        
        let metadata_v2 = VersionMetadata::new(2)
            .with_content_hash("test_hash_v2");
        storage.store_node_versioned("node1", &node_v2, metadata_v2).await?;
        
        // Get specific version
        let (retrieved_v1, meta_v1) = storage.get_node_versioned::<TestData>("node1", 1).await?;
        assert_eq!(retrieved_v1, node_v1);
        assert_eq!(meta_v1.content_hash, Some("test_hash_v1".to_string()));
        
        // Get latest version
        let (retrieved_latest, meta_latest) = storage.get_latest_node::<TestData>("node1").await?;
        assert_eq!(retrieved_latest, node_v2);
        assert_eq!(meta_latest.content_hash, Some("test_hash_v2".to_string()));
        
        // List versions
        let versions = storage.list_node_versions("node1").await?;
        assert_eq!(versions.len(), 2);
        
        Ok(())
    }
} 