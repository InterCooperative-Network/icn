//! # ICN Wallet Storage
//! 
//! The `icn-wallet-storage` crate provides robust, secure, and flexible data persistence 
//! capabilities for the ICN Wallet ecosystem. It implements various storage strategies to 
//! handle different types of wallet data with appropriate security guarantees.
//! 
//! ## Features
//! 
//! - **Multiple Storage Types**: Key-value, document, binary, and DAG storage implementations
//! - **Secure Storage**: Encrypted storage for sensitive data like private keys
//! - **Versioned Storage**: Track changes to documents with full version history
//! - **Searchable Indexes**: Secure indexing for efficient data retrieval
//! - **Lifecycle Management**: Handle different wallet states (active, locked, background)
//! - **Storage Namespacing**: Organize data efficiently with namespaced storage
//! 
//! ## Storage Types
//! 
//! - **Key-Value Storage**: Simple storage for configuration and settings
//! - **Document Storage**: JSON document storage with collection-based organization
//! - **Binary Storage**: Efficient storage for binary blobs like credential proofs
//! - **DAG Storage**: Specialized storage for DAG nodes with parent-child relationships
//! - **Secure Storage**: Encrypted storage for sensitive information
//! - **Versioned Storage**: Track document and DAG node history with metadata
//! 
//! ## Usage Example
//! 
//! ```rust,no_run
//! use icn_wallet_storage::StorageManager;
//! use serde::{Serialize, Deserialize};
//! 
//! #[derive(Serialize, Deserialize)]
//! struct UserProfile {
//!     name: String,
//!     email: String,
//! }
//! 
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize the storage manager
//!     let storage = StorageManager::new("wallet_data").await?;
//!     
//!     // Store application settings
//!     storage.store_setting("app_theme", &"dark").await?;
//!     
//!     // Store a user profile in a collection
//!     let profile = UserProfile {
//!         name: "Alice".to_string(),
//!         email: "alice@example.com".to_string(),
//!     };
//!     storage.store_object("profiles", "alice", &profile).await?;
//!     
//!     // Store sensitive data securely
//!     storage.store_secret("api_key", &"secret-api-key-value").await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod error;
pub mod traits;
pub mod file;
pub mod secure;
pub mod versioned;
pub mod lifecycle;
pub mod indexing;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use error::{StorageError, StorageResult};
use traits::{
    KeyValueStorage, DocumentStorage, BinaryStorage, DagStorage,
    SecureStorage, StorageKey, initialize_storage_directories,
    VersionedStorage, VersionedDocumentStorage, VersionedDagStorage, VersionMetadata
};
use file::FileStorage;
use secure::SimpleSecureStorage;
use versioned::FileBasedVersionedStorage;
use lifecycle::{LifecycleAwareStorageManager, AppState, LifecycleConfig};
use indexing::{SecureIndex, SearchResult, TermsExtraction};
use tracing::{debug, info};

/// The Storage Manager is a high-level interface that coordinates
/// access to different storage implementations
pub struct StorageManager {
    /// Base directory for storage
    base_dir: PathBuf,
    
    /// File-based storage for regular data
    file_storage: Arc<FileStorage>,
    
    /// Secure storage for sensitive data
    secure_storage: Arc<SimpleSecureStorage>,
    
    /// Versioned storage
    versioned_storage: Arc<FileBasedVersionedStorage>,

    /// Secure index for sensitive data
    secure_index: Option<Arc<SecureIndex>>,
}

impl StorageManager {
    /// Create a new storage manager
    pub async fn new(base_dir: impl AsRef<Path>) -> StorageResult<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        
        // Initialize storage directories
        initialize_storage_directories(&base_dir).await?;
        debug!("Initialized storage directories at {:?}", base_dir);
        
        // Create storage providers
        let file_storage = Arc::new(FileStorage::new(&base_dir).await?);
        let secure_storage = Arc::new(SimpleSecureStorage::new(&base_dir).await?);
        let versioned_storage = Arc::new(FileBasedVersionedStorage::new(&base_dir).await?);
        
        info!("Storage manager initialized at {:?}", base_dir);
        
        Ok(Self {
            base_dir,
            file_storage,
            secure_storage,
            versioned_storage,
            secure_index: None,
        })
    }
    
    /// Initialize secure indexing (optional, for searching sensitive data)
    pub async fn init_secure_indexing(&mut self) -> StorageResult<()> {
        let secure_index = SecureIndex::new(&self.base_dir, self.secure_storage.clone()).await?;
        self.secure_index = Some(Arc::new(secure_index));
        Ok(())
    }
    
    /// Access the file storage provider
    pub fn file_storage(&self) -> Arc<FileStorage> {
        self.file_storage.clone()
    }
    
    /// Access the secure storage provider
    pub fn secure_storage(&self) -> Arc<SimpleSecureStorage> {
        self.secure_storage.clone()
    }
    
    /// Access the versioned storage provider
    pub fn versioned_storage(&self) -> Arc<FileBasedVersionedStorage> {
        self.versioned_storage.clone()
    }
    
    /// Access the secure index (if initialized)
    pub fn secure_index(&self) -> Option<Arc<SecureIndex>> {
        self.secure_index.clone()
    }
    
    /// Get the base directory
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
    
    /// Create a lifecycle-aware storage manager from this manager
    pub async fn with_lifecycle(&self) -> StorageResult<LifecycleAwareStorageManager> {
        LifecycleAwareStorageManager::new(self.base_dir.clone()).await
    }
    
    /// Create a lifecycle-aware storage manager with custom config
    pub async fn with_lifecycle_config(
        &self, 
        config: LifecycleConfig
    ) -> StorageResult<LifecycleAwareStorageManager> {
        LifecycleAwareStorageManager::with_config(self.base_dir.clone(), config).await
    }
    
    /// Store a setting
    pub async fn store_setting<V: serde::Serialize + Send + Sync>(&self, key: &str, value: &V) -> StorageResult<()> {
        let storage_key = StorageKey::namespaced("settings", key);
        self.file_storage.set(&storage_key, value).await
    }
    
    /// Get a setting
    pub async fn get_setting<V: serde::de::DeserializeOwned + Send + Sync>(&self, key: &str) -> StorageResult<V> {
        let storage_key = StorageKey::namespaced("settings", key);
        self.file_storage.get(&storage_key).await
    }
    
    /// Check if a setting exists
    pub async fn has_setting(&self, key: &str) -> StorageResult<bool> {
        let storage_key = StorageKey::namespaced("settings", key);
        self.file_storage.contains(&storage_key).await
    }
    
    /// Delete a setting
    pub async fn delete_setting(&self, key: &str) -> StorageResult<()> {
        let storage_key = StorageKey::namespaced("settings", key);
        self.file_storage.delete(&storage_key).await
    }
    
    /// Store an object in a collection
    pub async fn store_object<T: serde::Serialize + Send + Sync>(
        &self, 
        collection: &str, 
        id: &str, 
        object: &T
    ) -> StorageResult<()> {
        self.file_storage.store_document(collection, id, object).await
    }
    
    /// Get an object from a collection
    pub async fn get_object<T: serde::de::DeserializeOwned + Send + Sync>(
        &self, 
        collection: &str, 
        id: &str
    ) -> StorageResult<T> {
        self.file_storage.get_document(collection, id).await
    }
    
    /// List all object IDs in a collection
    pub async fn list_objects(&self, collection: &str) -> StorageResult<Vec<String>> {
        self.file_storage.list_documents(collection).await
    }
    
    /// Delete an object from a collection
    pub async fn delete_object(&self, collection: &str, id: &str) -> StorageResult<()> {
        self.file_storage.delete_document(collection, id).await
    }
    
    /// Store a versioned object with automated version numbering
    pub async fn store_object_versioned<T: serde::Serialize + Send + Sync>(
        &self,
        collection: &str,
        id: &str,
        object: &T,
        author: Option<&str>
    ) -> StorageResult<u64> {
        // Get latest version number
        let metadata_path = self.versioned_storage.document_metadata_path(collection, id);
        let latest_version = self.versioned_storage.get_latest_version_number(&metadata_path).await?
            .unwrap_or(0);
        
        // Create new version
        let new_version = latest_version + 1;
        
        // Create metadata
        let metadata = if let Some(author_id) = author {
            VersionMetadata::with_author(new_version, author_id)
        } else {
            VersionMetadata::new(new_version)
        };
        
        // Store versioned document
        self.versioned_storage.store_versioned_document(collection, id, object, metadata).await?;
        
        Ok(new_version)
    }
    
    /// Get a specific version of an object
    pub async fn get_object_version<T: serde::de::DeserializeOwned + Send + Sync>(
        &self,
        collection: &str,
        id: &str,
        version: u64
    ) -> StorageResult<(T, VersionMetadata)> {
        self.versioned_storage.get_versioned_document(collection, id, version).await
    }
    
    /// Get the latest version of an object
    pub async fn get_object_latest<T: serde::de::DeserializeOwned + Send + Sync>(
        &self,
        collection: &str,
        id: &str
    ) -> StorageResult<(T, VersionMetadata)> {
        self.versioned_storage.get_latest_document(collection, id).await
    }
    
    /// List all versions of an object
    pub async fn list_object_versions(
        &self,
        collection: &str,
        id: &str
    ) -> StorageResult<Vec<VersionMetadata>> {
        self.versioned_storage.list_document_versions(collection, id).await
    }
    
    /// Store a secret
    pub async fn store_secret<V: serde::Serialize + Send + Sync>(&self, key: &str, value: &V) -> StorageResult<()> {
        self.secure_storage.store_secret(key, value).await
    }
    
    /// Store a secret and index it for searching
    pub async fn store_secret_with_indexing<V: serde::Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &V,
        index_name: &str,
        metadata: Option<&str>,
        extraction: TermsExtraction
    ) -> StorageResult<()> {
        if let Some(index) = &self.secure_index {
            index.store_and_index(index_name, key, value, metadata, extraction).await
        } else {
            // Fall back to regular storage without indexing
            self.store_secret(key, value).await
        }
    }
    
    /// Search for secrets in an index
    pub async fn search_secrets(
        &self,
        index_name: &str,
        query: &str,
        max_results: usize
    ) -> StorageResult<Vec<SearchResult>> {
        if let Some(index) = &self.secure_index {
            index.search(index_name, query, max_results).await
        } else {
            return Err(StorageError::Other("Secure indexing not initialized".to_string()));
        }
    }
    
    /// Get a secret
    pub async fn get_secret<V: serde::de::DeserializeOwned + Send + Sync>(&self, key: &str) -> StorageResult<V> {
        self.secure_storage.get_secret(key).await
    }
    
    /// Delete a secret
    pub async fn delete_secret(&self, key: &str) -> StorageResult<()> {
        self.secure_storage.delete_secret(key).await
    }
    
    /// Delete a secret and remove it from indexes
    pub async fn delete_secret_with_indexing(
        &self,
        key: &str,
        index_name: &str
    ) -> StorageResult<()> {
        if let Some(index) = &self.secure_index {
            index.delete_and_remove(index_name, key).await
        } else {
            // Fall back to regular deletion without indexing
            self.delete_secret(key).await
        }
    }
    
    /// Store a DAG node
    pub async fn store_dag_node<T: serde::Serialize + Send + Sync>(&self, node_id: &str, node: &T) -> StorageResult<()> {
        self.file_storage.store_node(node_id, node).await
    }
    
    /// Get a DAG node
    pub async fn get_dag_node<T: serde::de::DeserializeOwned + Send + Sync>(&self, node_id: &str) -> StorageResult<T> {
        self.file_storage.get_node(node_id).await
    }
    
    /// Store a versioned DAG node with automated version numbering
    pub async fn store_dag_node_versioned<T: serde::Serialize + Send + Sync>(
        &self,
        node_id: &str,
        node: &T,
        author: Option<&str>
    ) -> StorageResult<u64> {
        // Get latest version number
        let metadata_path = self.versioned_storage.node_metadata_path(node_id);
        let latest_version = self.versioned_storage.get_latest_version_number(&metadata_path).await?
            .unwrap_or(0);
        
        // Create new version
        let new_version = latest_version + 1;
        
        // Create metadata
        let metadata = if let Some(author_id) = author {
            VersionMetadata::with_author(new_version, author_id)
        } else {
            VersionMetadata::new(new_version)
        };
        
        // Store versioned node
        self.versioned_storage.store_node_versioned(node_id, node, metadata).await?;
        
        Ok(new_version)
    }
    
    /// Get a specific version of a DAG node
    pub async fn get_dag_node_version<T: serde::de::DeserializeOwned + Send + Sync>(
        &self,
        node_id: &str,
        version: u64
    ) -> StorageResult<(T, VersionMetadata)> {
        self.versioned_storage.get_node_versioned(node_id, version).await
    }
    
    /// Get the latest version of a DAG node
    pub async fn get_dag_node_latest<T: serde::de::DeserializeOwned + Send + Sync>(
        &self,
        node_id: &str
    ) -> StorageResult<(T, VersionMetadata)> {
        self.versioned_storage.get_latest_node(node_id).await
    }
    
    /// List all versions of a DAG node
    pub async fn list_dag_node_versions(&self, node_id: &str) -> StorageResult<Vec<VersionMetadata>> {
        self.versioned_storage.list_node_versions(node_id).await
    }
    
    /// List all DAG node IDs
    pub async fn list_dag_nodes(&self) -> StorageResult<Vec<String>> {
        self.file_storage.list_nodes().await
    }
    
    /// Add a child relationship between DAG nodes
    pub async fn add_dag_child(&self, parent_id: &str, child_id: &str) -> StorageResult<()> {
        self.file_storage.add_child(parent_id, child_id).await
    }
    
    /// Get children of a DAG node
    pub async fn get_dag_children(&self, node_id: &str) -> StorageResult<Vec<String>> {
        self.file_storage.get_children(node_id).await
    }
    
    /// Store binary data
    pub async fn store_binary(&self, path: &str, data: &[u8]) -> StorageResult<()> {
        self.file_storage.store_binary(path, data).await
    }
    
    /// Get binary data
    pub async fn get_binary(&self, path: &str) -> StorageResult<Vec<u8>> {
        self.file_storage.get_binary(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Serialize, Deserialize};
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_storage_manager() -> StorageResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create storage manager
        let mut manager = StorageManager::new(temp_dir.path()).await?;
        
        // Test settings
        manager.store_setting("test_mode", &true).await?;
        let test_mode: bool = manager.get_setting("test_mode").await?;
        assert!(test_mode);
        
        // Test objects
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct TestUser {
            name: String,
            email: String,
        }
        
        let user = TestUser {
            name: "John Doe".to_string(),
            email: "john@example.com".to_string(),
        };
        
        manager.store_object("users", "user1", &user).await?;
        let retrieved: TestUser = manager.get_object("users", "user1").await?;
        assert_eq!(user, retrieved);
        
        // Test versioned objects
        let user_v1 = TestUser {
            name: "John Doe".to_string(),
            email: "john@example.com".to_string(),
        };
        
        let user_v2 = TestUser {
            name: "John Doe".to_string(),
            email: "john.doe@example.com".to_string(),
        };
        
        let v1 = manager.store_object_versioned("users", "user2", &user_v1, Some("admin")).await?;
        let v2 = manager.store_object_versioned("users", "user2", &user_v2, Some("admin")).await?;
        
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
        
        let (retrieved_v1, metadata_v1) = manager.get_object_version::<TestUser>("users", "user2", 1).await?;
        assert_eq!(retrieved_v1, user_v1);
        assert_eq!(metadata_v1.version, 1);
        assert_eq!(metadata_v1.author, Some("admin".to_string()));
        
        let versions = manager.list_object_versions("users", "user2").await?;
        assert_eq!(versions.len(), 2);
        
        // Test object listing
        let user_ids = manager.list_objects("users").await?;
        assert!(user_ids.contains(&"user1".to_string()));
        assert!(user_ids.contains(&"user2".to_string()));
        
        // Test secrets
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct Credentials {
            username: String,
            password: String,
        }
        
        let creds = Credentials {
            username: "admin".to_string(),
            password: "secret123".to_string(),
        };
        
        manager.store_secret("admin_creds", &creds).await?;
        let retrieved_creds: Credentials = manager.get_secret("admin_creds").await?;
        assert_eq!(creds, retrieved_creds);
        
        // Initialize secure indexing
        manager.init_secure_indexing().await?;
        
        // Test indexed secrets
        manager.store_secret_with_indexing(
            "user_creds", 
            &Credentials { username: "user".to_string(), password: "user123".to_string() },
            "credentials",
            None,
            TermsExtraction::Both
        ).await?;
        
        // Search indexed secrets
        let results = manager.search_secrets("credentials", "user", 10).await?;
        assert_eq!(results.len(), 1);
        
        // Test DAG functionality
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct DagNode {
            content: String,
        }
        
        let node1 = DagNode { content: "Node 1".to_string() };
        let node2 = DagNode { content: "Node 2".to_string() };
        
        manager.store_dag_node("node1", &node1).await?;
        manager.store_dag_node("node2", &node2).await?;
        manager.add_dag_child("node1", "node2").await?;
        
        let children = manager.get_dag_children("node1").await?;
        assert_eq!(children, vec!["node2"]);
        
        // Test versioned DAG nodes
        let node1_v1 = DagNode { content: "Node 1 - v1".to_string() };
        let node1_v2 = DagNode { content: "Node 1 - v2".to_string() };
        
        let v1 = manager.store_dag_node_versioned("node3", &node1_v1, Some("user1")).await?;
        let v2 = manager.store_dag_node_versioned("node3", &node1_v2, Some("user1")).await?;
        
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
        
        let (retrieved_v1, _) = manager.get_dag_node_version::<DagNode>("node3", 1).await?;
        assert_eq!(retrieved_v1, node1_v1);
        
        let (retrieved_latest, meta_latest) = manager.get_dag_node_latest::<DagNode>("node3").await?;
        assert_eq!(retrieved_latest, node1_v2);
        assert_eq!(meta_latest.version, 2);
        
        // Test lifecycle-aware manager
        let lifecycle_manager = manager.with_lifecycle().await?;
        lifecycle_manager.handle_state_change(AppState::Active).await?;
        assert_eq!(lifecycle_manager.current_state().await, AppState::Active);
        
        Ok(())
    }
} 