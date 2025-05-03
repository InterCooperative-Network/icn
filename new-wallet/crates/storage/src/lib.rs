pub mod error;
pub mod traits;
pub mod file;
pub mod secure;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use error::{StorageError, StorageResult};
use traits::{
    KeyValueStorage, DocumentStorage, BinaryStorage, DagStorage,
    SecureStorage, StorageKey, initialize_storage_directories
};
use file::FileStorage;
use secure::SimpleSecureStorage;
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
        
        info!("Storage manager initialized at {:?}", base_dir);
        
        Ok(Self {
            base_dir,
            file_storage,
            secure_storage,
        })
    }
    
    /// Access the file storage provider
    pub fn file_storage(&self) -> Arc<FileStorage> {
        self.file_storage.clone()
    }
    
    /// Access the secure storage provider
    pub fn secure_storage(&self) -> Arc<SimpleSecureStorage> {
        self.secure_storage.clone()
    }
    
    /// Get the base directory
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
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
    
    /// Store a secret
    pub async fn store_secret<V: serde::Serialize + Send + Sync>(&self, key: &str, value: &V) -> StorageResult<()> {
        self.secure_storage.store_secret(key, value).await
    }
    
    /// Get a secret
    pub async fn get_secret<V: serde::de::DeserializeOwned + Send + Sync>(&self, key: &str) -> StorageResult<V> {
        self.secure_storage.get_secret(key).await
    }
    
    /// Delete a secret
    pub async fn delete_secret(&self, key: &str) -> StorageResult<()> {
        self.secure_storage.delete_secret(key).await
    }
    
    /// Store a DAG node
    pub async fn store_dag_node<T: serde::Serialize + Send + Sync>(&self, node_id: &str, node: &T) -> StorageResult<()> {
        self.file_storage.store_node(node_id, node).await
    }
    
    /// Get a DAG node
    pub async fn get_dag_node<T: serde::de::DeserializeOwned + Send + Sync>(&self, node_id: &str) -> StorageResult<T> {
        self.file_storage.get_node(node_id).await
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
        let manager = StorageManager::new(temp_dir.path()).await?;
        
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
        
        // Test object listing
        let user_ids = manager.list_objects("users").await?;
        assert_eq!(user_ids, vec!["user1"]);
        
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
        
        // Test DAG functionality
        #[derive(Serialize, Deserialize, Debug)]
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
        
        Ok(())
    }
} 