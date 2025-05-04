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
use sqlx::{Pool, Postgres};
use thiserror::Error;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};
use protocol::ThreadMessage;

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
    Storage(String),
    
    #[error("Compatibility error: {0}")]
    Compatibility(String),
    
    #[error("Other error: {0}")]
    Other(String),
}

/// Storage error to FederationError conversion
impl From<sqlx::Error> for FederationError {
    fn from(err: sqlx::Error) -> Self {
        FederationError::Storage(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, FederationError>;

/// Federation service for synchronizing content across nodes
#[derive(Clone)]
pub struct Federation {
    /// Database connection pool
    db_pool: Arc<Pool<Postgres>>,
    /// Federation network interface
    network: Arc<RwLock<FederationNetwork>>,
    /// Sync engine for thread/message synchronization
    sync_engine: Arc<RwLock<SyncEngine>>,
    /// Flag indicating if synchronization is enabled
    sync_enabled: bool,
}

impl Federation {
    /// Create a new Federation instance
    pub fn new(
        db_pool: Arc<Pool<Postgres>>,
        network: Arc<RwLock<FederationNetwork>>,
        sync_engine: Arc<RwLock<SyncEngine>>,
        sync_enabled: bool,
    ) -> Self {
        Self {
            db_pool,
            network,
            sync_engine,
            sync_enabled,
        }
    }
    
    /// Synchronize a thread with federation nodes
    pub async fn sync_thread(&self, thread_id: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
        if !self.sync_enabled {
            return Ok(());
        }
        
        info!("Synchronizing thread {} with federation peers", thread_id);
        
        // Fetch thread data from local database
        let thread = match sqlx::query!("SELECT * FROM threads WHERE id = $1", thread_id)
            .fetch_optional(self.db_pool.as_ref())
            .await? {
                Some(row) => row,
                None => {
                    return Err(Box::new(FederationError::ThreadSync(
                        format!("Thread {} not found in local database", thread_id)
                    )));
                }
            };
            
        // Fetch all messages in this thread
        let messages = sqlx::query!("SELECT * FROM messages WHERE thread_id = $1 ORDER BY created_at ASC", thread_id)
            .fetch_all(self.db_pool.as_ref())
            .await?;
            
        // Build the thread sync message
        let thread_message = ThreadMessage {
            id: thread.id,
            title: thread.title,
            creator_did: thread.creator_did,
            created_at: thread.created_at.to_rfc3339(),
            updated_at: thread.updated_at.to_rfc3339(),
            messages: messages.iter().map(|msg| protocol::Message {
                id: msg.id.clone(),
                content: msg.content.clone(),
                author_did: msg.author_did.clone(),
                created_at: msg.created_at.to_rfc3339(),
                thread_id: thread_id.to_string(),
                reply_to: msg.reply_to.clone(),
                signature: msg.signature.clone(),
            }).collect(),
            topic_type: thread.topic_type,
            federation_id: thread.federation_id,
            metadata: thread.metadata.clone(),
        };
        
        // Synchronize the thread through the sync engine
        let sync_engine = self.sync_engine.read().await;
        sync_engine.sync_thread(thread_message).await?;
        
        info!("Thread {} successfully synchronized with federation peers", thread_id);
        
        Ok(())
    }
    
    /// Synchronize a message with federation nodes
    pub async fn sync_message(&self, message_id: &str, thread_id: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
        if !self.sync_enabled {
            return Ok(());
        }
        
        info!("Synchronizing message {} in thread {} with federation peers", message_id, thread_id);
        
        // Fetch message data from local database
        let message = match sqlx::query!("SELECT * FROM messages WHERE id = $1", message_id)
            .fetch_optional(self.db_pool.as_ref())
            .await? {
                Some(row) => row,
                None => {
                    return Err(Box::new(FederationError::ThreadSync(
                        format!("Message {} not found in local database", message_id)
                    )));
                }
            };
            
        // Ensure message belongs to the specified thread
        if message.thread_id != thread_id {
            return Err(Box::new(FederationError::ThreadSync(
                format!("Message {} does not belong to thread {}", message_id, thread_id)
            )));
        }
            
        // Build the message sync object
        let sync_message = protocol::Message {
            id: message.id,
            content: message.content,
            author_did: message.author_did,
            created_at: message.created_at.to_rfc3339(),
            thread_id: thread_id.to_string(),
            reply_to: message.reply_to,
            signature: message.signature,
        };
        
        // Synchronize the message through the sync engine
        let sync_engine = self.sync_engine.read().await;
        sync_engine.sync_message(sync_message).await?;
        
        info!("Message {} successfully synchronized with federation peers", message_id);
        
        Ok(())
    }
    
    /// Check if federation sync is enabled
    pub fn is_sync_enabled(&self) -> bool {
        self.sync_enabled
    }
    
    /// Check if federation service is running
    pub fn is_running(&self) -> bool {
        true
    }
} 