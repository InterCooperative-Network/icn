use crate::federation::protocol::{
    ThreadMessage, CredentialLinkMessage, ThreadSyncRequestMessage, SyncMessage
};
use crate::federation::network::{FederationNetwork, NetworkTopic};
use crate::storage::{ThreadRepository, CredentialLinkRepository, Result as StorageResult};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};
use uuid::Uuid;
use super::FederationError;
use sqlx::{Pool, Postgres};
use tracing::{info, warn, error, debug};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use tokio::sync::{mpsc};

type Result<T> = std::result::Result<T, FederationError>;

/// The SyncEngine handles synchronization of threads and messages
/// between federation nodes
pub struct SyncEngine {
    /// Database connection pool
    db_pool: Arc<Pool<Postgres>>,
    
    /// Network interface for peer communication
    network: Arc<RwLock<FederationNetwork>>,
    
    /// Synchronization queue for outgoing messages
    sync_queue_tx: mpsc::Sender<SyncTask>,
    
    /// Flag indicating if sync is enabled
    enabled: bool,
}

/// Sync task types handled by the sync engine
#[derive(Debug)]
enum SyncTask {
    /// Synchronize a thread and its messages
    Thread(ThreadMessage),
    
    /// Synchronize a single message
    Message(Message),
}

impl SyncEngine {
    /// Create a new SyncEngine
    pub fn new(
        db_pool: Arc<Pool<Postgres>>,
        network: Arc<RwLock<FederationNetwork>>,
        enabled: bool,
    ) -> Self {
        let (sync_queue_tx, mut sync_queue_rx) = mpsc::channel::<SyncTask>(100);
        
        // Spawn a worker task to process the sync queue
        let db_pool_clone = db_pool.clone();
        let network_clone = network.clone();
        tokio::spawn(async move {
            while let Some(task) = sync_queue_rx.recv().await {
                let result = match task {
                    SyncTask::Thread(thread) => {
                        debug!("Processing thread sync task for thread {}", thread.id);
                        Self::process_thread_sync(db_pool_clone.clone(), network_clone.clone(), thread).await
                    },
                    SyncTask::Message(message) => {
                        debug!("Processing message sync task for message {}", message.id);
                        Self::process_message_sync(db_pool_clone.clone(), network_clone.clone(), message).await
                    }
                };
                
                if let Err(e) = result {
                    error!("Error processing sync task: {:?}", e);
                }
            }
        });
        
        Self {
            db_pool,
            network,
            sync_queue_tx,
            enabled,
        }
    }
    
    /// Sync a thread with federation nodes
    pub async fn sync_thread(&self, thread: ThreadMessage) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        
        // Queue the thread sync task
        self.sync_queue_tx.send(SyncTask::Thread(thread)).await
            .map_err(|e| FederationError::ThreadSync(format!("Failed to queue thread sync: {}", e)))?;
            
        Ok(())
    }
    
    /// Sync a message with federation nodes
    pub async fn sync_message(&self, message: Message) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        
        // Queue the message sync task
        self.sync_queue_tx.send(SyncTask::Message(message)).await
            .map_err(|e| FederationError::ThreadSync(format!("Failed to queue message sync: {}", e)))?;
            
        Ok(())
    }
    
    /// Process a thread sync task
    async fn process_thread_sync(
        db_pool: Arc<Pool<Postgres>>,
        network: Arc<RwLock<FederationNetwork>>,
        thread: ThreadMessage
    ) -> Result<()> {
        // Create the sync message
        let sync_message = SyncMessage::Thread(thread.clone());
        
        // Get connected peers
        let peers = {
            let network = network.read().await;
            network.get_connected_peers().await
        };
        
        if peers.is_empty() {
            warn!("No connected peers for thread sync");
            return Ok(());
        }
        
        // Broadcast to all peers
        for peer in peers {
            debug!("Sending thread {} sync to peer {}", thread.id, peer);
            let network = network.write().await;
            if let Err(e) = network.send_message(&peer, &sync_message).await {
                error!("Failed to sync thread {} with peer {}: {:?}", thread.id, peer, e);
            }
        }
        
        Ok(())
    }
    
    /// Process a message sync task
    async fn process_message_sync(
        db_pool: Arc<Pool<Postgres>>,
        network: Arc<RwLock<FederationNetwork>>,
        message: Message
    ) -> Result<()> {
        // Create the sync message
        let sync_message = SyncMessage::Message(message.clone());
        
        // Get connected peers
        let peers = {
            let network = network.read().await;
            network.get_connected_peers().await
        };
        
        if peers.is_empty() {
            warn!("No connected peers for message sync");
            return Ok(());
        }
        
        // Broadcast to all peers
        for peer in peers {
            debug!("Sending message {} sync to peer {}", message.id, peer);
            let network = network.write().await;
            if let Err(e) = network.send_message(&peer, &sync_message).await {
                error!("Failed to sync message {} with peer {}: {:?}", message.id, peer, e);
            }
        }
        
        Ok(())
    }
    
    /// Handle an incoming sync message from a peer
    pub async fn handle_sync_message(&self, peer_id: &str, sync_message: SyncMessage) -> Result<()> {
        match sync_message {
            SyncMessage::Thread(thread) => {
                info!("Received thread sync for thread {} from peer {}", thread.id, peer_id);
                self.handle_thread_sync(thread).await?;
            },
            SyncMessage::Message(message) => {
                info!("Received message sync for message {} from peer {}", message.id, peer_id);
                self.handle_message_sync(message).await?;
            }
        }
        
        Ok(())
    }
    
    /// Handle an incoming thread sync
    async fn handle_thread_sync(&self, thread: ThreadMessage) -> Result<()> {
        // Check if we already have this thread
        let exists = sqlx::query!("SELECT COUNT(*) FROM threads WHERE id = $1", thread.id)
            .fetch_one(self.db_pool.as_ref())
            .await?
            .count
            .unwrap_or(0) > 0;
            
        if exists {
            // Update existing thread
            sqlx::query!(
                "UPDATE threads SET 
                title = $1, 
                creator_did = $2, 
                topic_type = $3, 
                federation_id = $4, 
                metadata = $5,
                updated_at = NOW()
                WHERE id = $6",
                thread.title,
                thread.creator_did,
                thread.topic_type,
                thread.federation_id,
                thread.metadata,
                thread.id
            )
            .execute(self.db_pool.as_ref())
            .await?;
        } else {
            // Create new thread
            sqlx::query!(
                "INSERT INTO threads 
                (id, title, creator_did, topic_type, federation_id, metadata, created_at, updated_at) 
                VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())",
                thread.id,
                thread.title,
                thread.creator_did,
                thread.topic_type,
                thread.federation_id,
                thread.metadata
            )
            .execute(self.db_pool.as_ref())
            .await?;
        }
        
        // Process all messages in the thread
        for message in thread.messages {
            self.handle_message_sync(message).await?;
        }
        
        Ok(())
    }
    
    /// Handle an incoming message sync
    async fn handle_message_sync(&self, message: Message) -> Result<()> {
        // Check if thread exists
        let thread_exists = sqlx::query!("SELECT COUNT(*) FROM threads WHERE id = $1", message.thread_id)
            .fetch_one(self.db_pool.as_ref())
            .await?
            .count
            .unwrap_or(0) > 0;
            
        if !thread_exists {
            return Err(FederationError::ThreadSync(
                format!("Thread {} for message {} not found", message.thread_id, message.id)
            ));
        }
        
        // Check if we already have this message
        let exists = sqlx::query!("SELECT COUNT(*) FROM messages WHERE id = $1", message.id)
            .fetch_one(self.db_pool.as_ref())
            .await?
            .count
            .unwrap_or(0) > 0;
            
        if !exists {
            // Create new message
            sqlx::query!(
                "INSERT INTO messages 
                (id, thread_id, author_did, content, reply_to, signature, created_at) 
                VALUES ($1, $2, $3, $4, $5, $6, NOW())",
                message.id,
                message.thread_id,
                message.author_did,
                message.content,
                message.reply_to,
                message.signature,
            )
            .execute(self.db_pool.as_ref())
            .await?;
            
            info!("Synced new message {} in thread {}", message.id, message.thread_id);
        }
        
        Ok(())
    }
}

/// Handle a thread announcement message
async fn handle_thread_announcement(
    thread_repo: &ThreadRepository,
    msg: &ThreadMessage,
) -> Result<()> {
    // Check if thread already exists
    let thread_uuid = match Uuid::parse_str(&msg.thread_id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(FederationError::Other("Invalid thread ID format".to_string())),
    };
    
    // Check if thread exists
    match thread_repo.get_thread(thread_uuid).await {
        Ok(_) => {
            // Thread already exists, ignore
            Ok(())
        }
        Err(crate::storage::StorageError::NotFound) => {
            // Thread doesn't exist, create it
            thread_repo.create_thread(&msg.title, msg.proposal_cid.as_deref()).await?;
            Ok(())
        }
        Err(e) => Err(FederationError::Storage(e)),
    }
}

/// Handle a credential link announcement message
async fn handle_credential_link_announcement(
    link_repo: &CredentialLinkRepository,
    msg: &CredentialLinkMessage,
) -> Result<()> {
    // Parse UUIDs
    let _link_id = match Uuid::parse_str(&msg.link_id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(FederationError::Other("Invalid link ID format".to_string())),
    };
    
    let _thread_id = match Uuid::parse_str(&msg.thread_id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(FederationError::Other("Invalid thread ID format".to_string())),
    };
    
    // Create request object for repository
    let link_req = crate::routes::credentials::CredentialLinkRequest {
        thread_id: msg.thread_id.clone(),
        credential_cid: msg.credential_cid.clone(),
        signer_did: msg.linked_by.clone(),
    };
    
    // Create the credential link
    match link_repo.create_credential_link(&link_req).await {
        Ok(_) => Ok(()),
        Err(e) => Err(FederationError::Storage(e)),
    }
}

/// Handle a thread sync request message
async fn handle_thread_sync_request(
    thread_repo: &ThreadRepository,
    link_repo: &CredentialLinkRepository,
    network: &Arc<RwLock<FederationNetwork>>,
    msg: &ThreadSyncRequestMessage,
    requester_peer_id: &str,
) -> Result<()> {
    // Parse thread ID
    let thread_id = match Uuid::parse_str(&msg.thread_id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(FederationError::Other("Invalid thread ID format".to_string())),
    };
    
    // Get thread
    let thread = match thread_repo.get_thread(thread_id).await {
        Ok(t) => t,
        Err(e) => return Err(FederationError::Storage(e)),
    };
    
    // Get credential links for thread
    let links = match link_repo.get_links_for_thread(thread_id).await {
        Ok(l) => l,
        Err(e) => return Err(FederationError::Storage(e)),
    };
    
    // Announce thread to the requester
    let thread_msg = ThreadMessage::new(
        thread.id.to_string(),
        thread.title.clone(),
        thread.proposal_cid.clone(),
        "did:icn:local".to_string(), // TODO: Use actual local DID
    );
    
    let sync_msg = SyncMessage::Thread(thread_msg);
    let data = sync_msg.to_bytes()
        .map_err(|e| FederationError::Serialization(e.to_string()))?;
    
    // Send thread message to the requester
    let network_handle = network.write().await;
    // Direct send_to_peer implementation would need to be added to FederationNetwork
    // For now, we'll just publish to the topic
    network_handle.publish(NetworkTopic::ThreadAnnounce, data.clone()).await
        .map_err(|e| FederationError::Network(e.to_string()))?;
    
    // Announce credential links
    for link in links {
        let link_msg = CredentialLinkMessage::new(
            link.id.to_string(),
            link.thread_id.to_string(),
            link.credential_cid.clone(),
            link.linked_by.clone(),
        );
        
        let sync_msg = SyncMessage::CredentialLink(link_msg);
        let data = sync_msg.to_bytes()
            .map_err(|e| FederationError::Serialization(e.to_string()))?;
        
        // Send credential link message to the requester
        network_handle.publish(NetworkTopic::CredentialLinkAnnounce, data.clone()).await
            .map_err(|e| FederationError::Network(e.to_string()))?;
    }
    
    Ok(())
} 