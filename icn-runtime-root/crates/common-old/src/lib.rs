/*!
# ICN Common

This crate provides common types, traits, and utilities shared across other ICN components.
This helps prevent circular dependencies between core subsystems.
*/

use anyhow::Result;
use async_trait::async_trait;
use cid::Cid;

/// The DagStore trait defines methods for interacting with a DAG storage system.
#[async_trait]
pub trait DagStore: Send + Sync {
    /// Checks if a CID exists in the DAG store.
    async fn contains(&self, cid: &Cid) -> Result<bool, String>;
    
    /// Retrieves data associated with a CID from the DAG store.
    async fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, String>;
    
    /// Stores data in the DAG store and returns the resulting CID.
    async fn put(&self, data: &[u8]) -> Result<Cid, String>;
}

/// Module for error types used across ICN components
pub mod errors {
    use thiserror::Error;
    
    /// Common error type for storage operations
    #[derive(Error, Debug)]
    pub enum StorageError {
        #[error("Invalid CID: {0}")]
        InvalidCid(String),
        
        #[error("Not found: {0}")]
        NotFound(String),
        
        #[error("Storage error: {0}")]
        StorageError(String),
        
        #[error("Serialization error: {0}")]
        SerializationError(String),
    }
}

/// Module for common utilities used across ICN components
pub mod utils {
    use cid::Cid;
    
    /// Create a CID v1 with raw codec and SHA-256 multihash from data
    pub fn create_cid_from_data(data: &[u8]) -> Cid {
        // Use the sha2 crate directly to calculate SHA-256
        use sha2::{Sha256, Digest};
        let hash = Sha256::digest(data);
        
        // Create a CID v1 using the cid crate's native methods
        let mh = cid::multihash::Multihash::wrap(0x12, &hash).unwrap(); // 0x12 is the code for SHA-256
        Cid::new_v1(0x55, mh) // 0x55 is the multicodec code for raw
    }
} 