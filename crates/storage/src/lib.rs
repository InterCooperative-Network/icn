use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use thiserror::Error;
use uuid::Uuid;

/// Error type for storage operations
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Item not found: {0}")]
    NotFound(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Storage driver trait
#[async_trait]
pub trait StorageDriver: Send + Sync {
    /// Store an item
    async fn store(&self, key: &str, value: &[u8]) -> StorageResult<()>;
    
    /// Retrieve an item
    async fn retrieve(&self, key: &str) -> StorageResult<Vec<u8>>;
    
    /// Delete an item
    async fn delete(&self, key: &str) -> StorageResult<()>;
    
    /// List all keys
    async fn list_keys(&self) -> StorageResult<Vec<String>>;
}

/// In-memory storage driver
pub struct InMemoryStorage {
    data: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl InMemoryStorage {
    /// Create a new in-memory storage
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl StorageDriver for InMemoryStorage {
    async fn store(&self, key: &str, value: &[u8]) -> StorageResult<()> {
        let mut data = self.data.lock().unwrap();
        data.insert(key.to_string(), value.to_vec());
        Ok(())
    }
    
    async fn retrieve(&self, key: &str) -> StorageResult<Vec<u8>> {
        let data = self.data.lock().unwrap();
        data.get(key)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(key.to_string()))
    }
    
    async fn delete(&self, key: &str) -> StorageResult<()> {
        let mut data = self.data.lock().unwrap();
        data.remove(key);
        Ok(())
    }
    
    async fn list_keys(&self) -> StorageResult<Vec<String>> {
        let data = self.data.lock().unwrap();
        Ok(data.keys().cloned().collect())
    }
}

/// Storage manager
pub struct StorageManager {
    driver: Box<dyn StorageDriver>,
}

impl StorageManager {
    /// Create a new storage manager
    pub fn new(driver: Box<dyn StorageDriver>) -> Self {
        Self { driver }
    }
    
    /// Store a serializable item
    pub async fn store<T: Serialize>(&self, key: &str, value: &T) -> StorageResult<()> {
        let json = serde_json::to_vec(value).map_err(|e| {
            StorageError::SerializationError(format!("Failed to serialize: {}", e))
        })?;
        
        self.driver.store(key, &json).await
    }
    
    /// Retrieve a deserializable item
    pub async fn retrieve<T: for<'de> Deserialize<'de>>(&self, key: &str) -> StorageResult<T> {
        let data = self.driver.retrieve(key).await?;
        
        serde_json::from_slice(&data).map_err(|e| {
            StorageError::SerializationError(format!("Failed to deserialize: {}", e))
        })
    }
    
    /// Delete an item
    pub async fn delete(&self, key: &str) -> StorageResult<()> {
        self.driver.delete(key).await
    }
    
    /// List all keys
    pub async fn list_keys(&self) -> StorageResult<Vec<String>> {
        self.driver.list_keys().await
    }
} 