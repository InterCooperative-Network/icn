/*!
 * ICN Wallet DAG Storage Manager
 *
 * Provides functionality for interacting with DAG storage from wallet context.
 */

use async_trait::async_trait;
use thiserror::Error;
use std::sync::{Arc, Mutex};
use icn_storage::Storage;
use icn_dag::DagManager;

/// Error types for DAG operations
#[derive(Error, Debug)]
pub enum DagError {
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("DAG error: {0}")]
    DagError(String),
    
    #[error("CID not found: {0}")]
    NotFound(String),
    
    #[error("Invalid DAG node: {0}")]
    InvalidNode(String),
}

/// Interface for accessing DAG storage
#[async_trait]
pub trait DagStorageManager: Send + Sync {
    /// Get a DAG node by CID
    async fn get_node(&self, cid: &str) -> Result<icn_dag::DagNode, DagError>;
    
    /// Get DAG node metadata
    async fn get_metadata(&self, cid: &str) -> Result<icn_dag::DagNodeMetadata, DagError>;
    
    /// Check if a DAG node exists
    async fn node_exists(&self, cid: &str) -> Result<bool, DagError>;
}

/// Create a local DAG storage manager for verification
pub async fn create_local_dag_store() -> anyhow::Result<impl DagStorageManager> {
    // Create a memory storage instance
    let storage = Arc::new(Mutex::new(icn_storage::MemoryStorage::new()));
    
    // Create a new DAG manager
    let dag_manager = Arc::new(DagManager::new(storage.clone()));
    
    // Return the local DAG storage manager
    Ok(LocalDagStore {
        dag_manager,
    })
}

/// A DAG storage manager that uses the local storage
pub struct LocalDagStore {
    dag_manager: Arc<DagManager>,
}

#[async_trait]
impl DagStorageManager for LocalDagStore {
    async fn get_node(&self, cid: &str) -> Result<icn_dag::DagNode, DagError> {
        self.dag_manager
            .get_node(cid)
            .map_err(|e| DagError::DagError(format!("Failed to get DAG node: {}", e)))
    }
    
    async fn get_metadata(&self, cid: &str) -> Result<icn_dag::DagNodeMetadata, DagError> {
        self.dag_manager
            .get_metadata(cid)
            .map_err(|e| DagError::DagError(format!("Failed to get DAG metadata: {}", e)))
    }
    
    async fn node_exists(&self, cid: &str) -> Result<bool, DagError> {
        Ok(self.dag_manager.node_exists(cid))
    }
} 