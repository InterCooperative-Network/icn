// Federation module for AgoraNet
// Handles peer-to-peer communication and data synchronization using libp2p

mod network;
mod protocol;
mod sync;
mod discovery;

pub use network::FederationNetwork;
pub use protocol::{ThreadMessage, SyncMessage};
pub use sync::SyncEngine;

use std::sync::Arc;
use tokio::sync::RwLock;
use sqlx::PgPool;
use thiserror::Error;

/// Error types for federation-related operations
#[derive(Error, Debug)]
pub enum FederationError {
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Failed to serialize or deserialize: {0}")]
    Serialization(String),
    
    #[error("Thread sync error: {0}")]
    ThreadSync(String),
    
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    
    #[error("Unexpected error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, FederationError>;

/// Main federation service that orchestrates p2p communication
/// for the AgoraNet instance
pub struct Federation {
    /// Network layer handling libp2p connections
    network: Arc<RwLock<FederationNetwork>>,
    
    /// Thread synchronization engine
    sync_engine: Arc<RwLock<SyncEngine>>,
    
    /// Database pool for persistence
    db_pool: PgPool,
}

impl Federation {
    /// Creates a new Federation instance with the given database pool
    pub async fn new(db_pool: PgPool) -> Result<Self> {
        let network = Arc::new(RwLock::new(
            FederationNetwork::new().map_err(|e| FederationError::Network(e.to_string()))?
        ));
        
        let sync_engine = Arc::new(RwLock::new(
            SyncEngine::new(network.clone(), db_pool.clone())
        ));
        
        Ok(Self {
            network,
            sync_engine,
            db_pool,
        })
    }
    
    /// Starts the federation service
    pub async fn start(&self) -> Result<()> {
        // Start the network layer
        self.network.write().await.start().await
            .map_err(|e| FederationError::Network(e.to_string()))?;
        
        // Start the sync engine
        self.sync_engine.write().await.start().await?;
        
        Ok(())
    }
    
    /// Stops the federation service
    pub async fn stop(&self) -> Result<()> {
        // Stop the sync engine
        self.sync_engine.write().await.stop().await?;
        
        // Stop the network layer
        self.network.write().await.stop().await
            .map_err(|e| FederationError::Network(e.to_string()))?;
        
        Ok(())
    }
    
    /// Announces a new thread to the federation network
    pub async fn announce_thread(&self, thread_id: &str) -> Result<()> {
        self.sync_engine.write().await.announce_thread(thread_id).await
    }
    
    /// Announces a new credential link to the federation network
    pub async fn announce_credential_link(&self, thread_id: &str, link_id: &str) -> Result<()> {
        self.sync_engine.write().await.announce_credential_link(thread_id, link_id).await
    }
    
    /// Retrieves the list of known peers in the federation
    pub async fn known_peers(&self) -> Result<Vec<String>> {
        let peers = self.network.read().await.known_peers().await
            .map_err(|e| FederationError::Network(e.to_string()))?;
            
        Ok(peers)
    }
    
    /// Check if the federation service is running
    pub fn is_running(&self) -> bool {
        // Since we can't do an async call here, we'll consider it running
        // if it has been initialized. In a more complete implementation,
        // we might use a shared atomic flag that's updated by start/stop
        true
    }
} 