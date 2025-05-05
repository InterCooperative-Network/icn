use std::path::{Path, PathBuf};
use std::collections::HashMap;
use crate::error::{StorageError, StorageResult};
use crate::traits::{
    KeyValueStorage, DocumentStorage, BinaryStorage, DagStorage, 
    StorageKey, ensure_directory, initialize_storage_directories
};
use serde::{Serialize, de::DeserializeOwned};
use async_trait::async_trait;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use std::sync::Arc;

/// File-based storage implementation
pub struct FileStorage {
    /// Base directory for all storage
    base_dir: PathBuf,
    
    /// Key-value storage directory
    kv_dir: PathBuf,
    
    /// Document storage directory
    documents_dir: PathBuf,
    
    /// Binary storage directory
    binary_dir: PathBuf,
    
    /// DAG storage directory
    dag_dir: PathBuf,
    
    /// DAG relationships cache
    dag_relationships: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl FileStorage {
    /// Create a new file storage provider
    pub async fn new(base_dir: impl AsRef<Path>) -> StorageResult<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        
        // Initialize all required directories
        initialize_storage_directories(&base_dir).await?;
        
        let storage = Self {
            kv_dir: base_dir.join("kv"),
            documents_dir: base_dir.join("documents"),
            binary_dir: base_dir.join("binary"),
            dag_dir: base_dir.join("dag"),
            base_dir,
            dag_relationships: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Initialize DAG relationships from disk
        storage.load_dag_relationships().await?;
        
        Ok(storage)
    }
    
    /// Load DAG relationships from disk
    async fn load_dag_relationships(&self) -> StorageResult<()> {
        let relationships_path = self.dag_dir.join("relationships.json");
        
        if relationships_path.exists() {
            let content = fs::read_to_string(&relationships_path).await?;
            let relationships: HashMap<String, Vec<String>> = serde_json::from_str(&content)
                .map_err(|e| StorageError::SerializationError(format!("Failed to parse DAG relationships: {}", e)))?;
                
            let mut cache = self.dag_relationships.write().await;
            *cache = relationships;
            debug!("Loaded DAG relationships from disk");
        }
        
        Ok(())
    }
    
    /// Save DAG relationships to disk
    async fn save_dag_relationships(&self) -> StorageResult<()> {
        let relationships_path = self.dag_dir.join("relationships.json");
        let relationships = self.dag_relationships.read().await;
        
        let serialized = serde_json::to_string_pretty(&*relationships)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize DAG relationships: {}", e)))?;
            
        fs::write(&relationships_path, serialized).await?;
        debug!("Saved DAG relationships to disk");
        
        Ok(())
    }
    
    /// Generate a full path for a key-value item
    fn kv_path(&self, key: &StorageKey) -> PathBuf {
        self.kv_dir.join(format!("{}.json", key.as_str()))
    }
    
    /// Generate a path for a collection directory in document storage
    fn collection_dir(&self, collection: &str) -> PathBuf {
        self.documents_dir.join(collection)
    }
    
    /// Generate a full path for a document
    fn document_path(&self, collection: &str, id: &str) -> PathBuf {
        self.collection_dir(collection).join(format!("{}.json", id))
    }
    
    /// Generate a full path for a binary file
    fn binary_path(&self, path: &str) -> PathBuf {
        self.binary_dir.join(path)
    }
    
    /// Generate a full path for a DAG node
    fn dag_node_path(&self, node_id: &str) -> PathBuf {
        self.dag_dir.join(format!("{}.json", node_id))
    }
}

#[async_trait]
impl KeyValueStorage for FileStorage {
    async fn set<V: Serialize + Send + Sync>(&self, key: &StorageKey, value: &V) -> StorageResult<()> {
        let path = self.kv_path(key);
        
        // Ensure the parent directory exists
        if let Some(parent) = path.parent() {
            ensure_directory(parent).await?;
        }
        
        let serialized = serde_json::to_string_pretty(value)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize value: {}", e)))?;
            
        fs::write(&path, serialized).await?;
        debug!("Stored value for key: {}", key.as_str());
        
        Ok(())
    }
    
    async fn get<V: DeserializeOwned + Send + Sync>(&self, key: &StorageKey) -> StorageResult<V> {
        let path = self.kv_path(key);
        
        if !path.exists() {
            return Err(StorageError::NotFound(format!("Key not found: {}", key.as_str())));
        }
        
        let content = fs::read_to_string(&path).await?;
        let value = serde_json::from_str(&content)
            .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize value: {}", e)))?;
            
        Ok(value)
    }
    
    async fn contains(&self, key: &StorageKey) -> StorageResult<bool> {
        let path = self.kv_path(key);
        Ok(path.exists())
    }
    
    async fn delete(&self, key: &StorageKey) -> StorageResult<()> {
        let path = self.kv_path(key);
        
        if path.exists() {
            fs::remove_file(&path).await?;
            debug!("Deleted key: {}", key.as_str());
        }
        
        Ok(())
    }
    
    async fn list_keys(&self, prefix: &str) -> StorageResult<Vec<StorageKey>> {
        let mut keys = Vec::new();
        let mut entries = fs::read_dir(&self.kv_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            
            // Skip directories and non-json files
            if !path.is_file() || path.extension().map_or(true, |ext| ext != "json") {
                continue;
            }
            
            if let Some(file_stem) = path.file_stem() {
                if let Some(key_str) = file_stem.to_str() {
                    if key_str.starts_with(prefix) {
                        keys.push(StorageKey::new(key_str));
                    }
                }
            }
        }
        
        Ok(keys)
    }
}

#[async_trait]
impl DocumentStorage for FileStorage {
    async fn store_document<T: Serialize + Send + Sync>(
        &self, 
        collection: &str, 
        id: &str, 
        document: &T
    ) -> StorageResult<()> {
        let collection_dir = self.collection_dir(collection);
        ensure_directory(&collection_dir).await?;
        
        let path = self.document_path(collection, id);
        
        let serialized = serde_json::to_string_pretty(document)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize document: {}", e)))?;
            
        fs::write(&path, serialized).await?;
        debug!("Stored document {}/{}", collection, id);
        
        Ok(())
    }
    
    async fn get_document<T: DeserializeOwned + Send + Sync>(
        &self, 
        collection: &str, 
        id: &str
    ) -> StorageResult<T> {
        let path = self.document_path(collection, id);
        
        if !path.exists() {
            return Err(StorageError::NotFound(format!("Document not found: {}/{}", collection, id)));
        }
        
        let content = fs::read_to_string(&path).await?;
        let document = serde_json::from_str(&content)
            .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize document: {}", e)))?;
            
        Ok(document)
    }
    
    async fn document_exists(&self, collection: &str, id: &str) -> StorageResult<bool> {
        let path = self.document_path(collection, id);
        Ok(path.exists())
    }
    
    async fn delete_document(&self, collection: &str, id: &str) -> StorageResult<()> {
        let path = self.document_path(collection, id);
        
        if path.exists() {
            fs::remove_file(&path).await?;
            debug!("Deleted document {}/{}", collection, id);
        }
        
        Ok(())
    }
    
    async fn list_documents(&self, collection: &str) -> StorageResult<Vec<String>> {
        let collection_dir = self.collection_dir(collection);
        
        if !collection_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut documents = Vec::new();
        let mut entries = fs::read_dir(&collection_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            
            // Skip directories and non-json files
            if !path.is_file() || path.extension().map_or(true, |ext| ext != "json") {
                continue;
            }
            
            if let Some(file_stem) = path.file_stem() {
                if let Some(id) = file_stem.to_str() {
                    documents.push(id.to_string());
                }
            }
        }
        
        Ok(documents)
    }
}

#[async_trait]
impl BinaryStorage for FileStorage {
    async fn store_binary(&self, path: &str, data: &[u8]) -> StorageResult<()> {
        let full_path = self.binary_path(path);
        
        // Ensure the parent directory exists
        if let Some(parent) = full_path.parent() {
            ensure_directory(parent).await?;
        }
        
        fs::write(&full_path, data).await?;
        debug!("Stored binary data at {}", path);
        
        Ok(())
    }
    
    async fn get_binary(&self, path: &str) -> StorageResult<Vec<u8>> {
        let full_path = self.binary_path(path);
        
        if !full_path.exists() {
            return Err(StorageError::NotFound(format!("Binary file not found: {}", path)));
        }
        
        let data = fs::read(&full_path).await?;
        Ok(data)
    }
    
    async fn delete_binary(&self, path: &str) -> StorageResult<()> {
        let full_path = self.binary_path(path);
        
        if full_path.exists() {
            fs::remove_file(&full_path).await?;
            debug!("Deleted binary file {}", path);
        }
        
        Ok(())
    }
    
    async fn binary_exists(&self, path: &str) -> StorageResult<bool> {
        let full_path = self.binary_path(path);
        Ok(full_path.exists())
    }
}

#[async_trait]
impl DagStorage for FileStorage {
    async fn store_node<T: Serialize + Send + Sync>(&self, node_id: &str, node: &T) -> StorageResult<()> {
        let path = self.dag_node_path(node_id);
        
        let serialized = serde_json::to_string_pretty(node)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize DAG node: {}", e)))?;
            
        fs::write(&path, serialized).await?;
        debug!("Stored DAG node: {}", node_id);
        
        Ok(())
    }
    
    async fn get_node<T: DeserializeOwned + Send + Sync>(&self, node_id: &str) -> StorageResult<T> {
        let path = self.dag_node_path(node_id);
        
        if !path.exists() {
            return Err(StorageError::NotFound(format!("DAG node not found: {}", node_id)));
        }
        
        let content = fs::read_to_string(&path).await?;
        let node = serde_json::from_str(&content)
            .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize DAG node: {}", e)))?;
            
        Ok(node)
    }
    
    async fn list_nodes(&self) -> StorageResult<Vec<String>> {
        let mut nodes = Vec::new();
        let mut entries = fs::read_dir(&self.dag_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            
            // Skip directories, non-json files, and relationships.json
            if !path.is_file() || 
               path.extension().map_or(true, |ext| ext != "json") ||
               path.file_name().map_or(false, |f| f == "relationships.json") {
                continue;
            }
            
            if let Some(file_stem) = path.file_stem() {
                if let Some(id) = file_stem.to_str() {
                    nodes.push(id.to_string());
                }
            }
        }
        
        Ok(nodes)
    }
    
    async fn delete_node(&self, node_id: &str) -> StorageResult<()> {
        let path = self.dag_node_path(node_id);
        
        if path.exists() {
            fs::remove_file(&path).await?;
            debug!("Deleted DAG node: {}", node_id);
            
            // Remove relationships
            let mut relationships = self.dag_relationships.write().await;
            relationships.remove(node_id);
            
            // Remove as child from all parents
            for children in relationships.values_mut() {
                if let Some(pos) = children.iter().position(|id| id == node_id) {
                    children.remove(pos);
                }
            }
            
            // Save updated relationships
            drop(relationships);
            self.save_dag_relationships().await?;
        }
        
        Ok(())
    }
    
    async fn get_children(&self, node_id: &str) -> StorageResult<Vec<String>> {
        let relationships = self.dag_relationships.read().await;
        
        Ok(relationships.get(node_id)
            .cloned()
            .unwrap_or_else(Vec::new))
    }
    
    async fn add_child(&self, parent_id: &str, child_id: &str) -> StorageResult<()> {
        // Ensure both nodes exist
        let parent_path = self.dag_node_path(parent_id);
        let child_path = self.dag_node_path(child_id);
        
        if !parent_path.exists() {
            return Err(StorageError::NotFound(format!("Parent node not found: {}", parent_id)));
        }
        
        if !child_path.exists() {
            return Err(StorageError::NotFound(format!("Child node not found: {}", child_id)));
        }
        
        // Update relationships
        let mut relationships = self.dag_relationships.write().await;
        
        let children = relationships.entry(parent_id.to_string())
            .or_insert_with(Vec::new);
            
        // Don't add duplicate child
        if !children.contains(&child_id.to_string()) {
            children.push(child_id.to_string());
            
            // Save updated relationships
            drop(relationships);
            self.save_dag_relationships().await?;
            
            debug!("Added child relationship: {} -> {}", parent_id, child_id);
        }
        
        Ok(())
    }
} 