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

type Result<T> = std::result::Result<T, FederationError>;

/// Engine for thread and credential link synchronization
pub struct SyncEngine {
    /// Reference to the network layer
    network: Arc<RwLock<FederationNetwork>>,
    
    /// Database connection pool
    db_pool: PgPool,
    
    /// Thread repository
    thread_repo: ThreadRepository,
    
    /// Credential link repository
    link_repo: CredentialLinkRepository,
    
    /// Handle for the background sync task
    sync_task: Option<JoinHandle<()>>,
    
    /// Whether the sync engine is running
    running: bool,
}

impl SyncEngine {
    /// Create a new sync engine
    pub fn new(network: Arc<RwLock<FederationNetwork>>, db_pool: PgPool) -> Self {
        let thread_repo = ThreadRepository::new(db_pool.clone());
        let link_repo = CredentialLinkRepository::new(db_pool.clone());
        
        Self {
            network,
            db_pool,
            thread_repo,
            link_repo,
            sync_task: None,
            running: false,
        }
    }
    
    /// Start the sync engine
    pub async fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }
        
        self.running = true;
        
        // Clone components for use in the task
        let network = self.network.clone();
        let thread_repo = ThreadRepository::new(self.db_pool.clone());
        let link_repo = CredentialLinkRepository::new(self.db_pool.clone());
        
        // Spawn the background task for event handling
        let task = tokio::spawn(async move {
            let mut event_interval = interval(Duration::from_secs(5));
            
            loop {
                tokio::select! {
                    _ = event_interval.tick() => {
                        // Regular background sync tasks
                    }
                    
                    // Handle network events
                    Some(event) = async {
                        let mut net = network.write().await;
                        net.next_event().await
                    } => {
                        if let crate::federation::network::NetworkEvent::Message { peer_id, topic, data } = event {
                            match topic {
                                NetworkTopic::ThreadAnnounce => {
                                    if let Ok(msg) = SyncMessage::from_bytes(&data) {
                                        if let SyncMessage::Thread(thread_msg) = msg {
                                            // Handle thread announcement
                                            let _ = handle_thread_announcement(
                                                &thread_repo, 
                                                &thread_msg
                                            ).await;
                                        }
                                    }
                                }
                                NetworkTopic::CredentialLinkAnnounce => {
                                    if let Ok(msg) = SyncMessage::from_bytes(&data) {
                                        if let SyncMessage::CredentialLink(link_msg) = msg {
                                            // Handle credential link announcement
                                            let _ = handle_credential_link_announcement(
                                                &link_repo, 
                                                &link_msg
                                            ).await;
                                        }
                                    }
                                }
                                NetworkTopic::ThreadSyncRequest => {
                                    if let Ok(msg) = SyncMessage::from_bytes(&data) {
                                        if let SyncMessage::SyncRequest(req_msg) = msg {
                                            // Handle thread sync request
                                            let _ = handle_thread_sync_request(
                                                &thread_repo,
                                                &link_repo,
                                                &network,
                                                &req_msg,
                                                &peer_id
                                            ).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        
        self.sync_task = Some(task);
        
        Ok(())
    }
    
    /// Stop the sync engine
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        
        if let Some(task) = self.sync_task.take() {
            task.abort();
            let _ = task.await;
        }
        
        self.running = false;
        
        Ok(())
    }
    
    /// Announce a new thread to the federation
    pub async fn announce_thread(&self, thread_id: &str) -> Result<()> {
        // Get thread from storage
        let thread_uuid = Uuid::parse_str(thread_id)
            .map_err(|_| FederationError::Other("Invalid thread ID format".to_string()))?;
        
        let thread = self.thread_repo.get_thread(thread_uuid).await?;
        
        // Create thread message
        let thread_msg = ThreadMessage::new(
            thread.id.to_string(),
            thread.title.clone(),
            thread.proposal_cid.clone(),
            "did:icn:local".to_string(), // TODO: Use actual local DID
        );
        
        // Create sync message
        let sync_msg = SyncMessage::Thread(thread_msg);
        let data = sync_msg.to_bytes()
            .map_err(|e| FederationError::Serialization(e.to_string()))?;
        
        // Publish to the network
        let mut network = self.network.write().await;
        network.publish(NetworkTopic::ThreadAnnounce, data).await
            .map_err(|e| FederationError::Network(e.to_string()))?;
        
        Ok(())
    }
    
    /// Announce a new credential link to the federation
    pub async fn announce_credential_link(&self, thread_id: &str, link_id: &str) -> Result<()> {
        // Get thread UUID
        let thread_uuid = Uuid::parse_str(thread_id)
            .map_err(|_| FederationError::Other("Invalid thread ID format".to_string()))?;
            
        // Get link UUID
        let link_uuid = Uuid::parse_str(link_id)
            .map_err(|_| FederationError::Other("Invalid link ID format".to_string()))?;
        
        // Retrieve all links for the thread
        let links = self.link_repo.get_links_for_thread(thread_uuid).await?;
        
        // Find the specific link
        let link = links.into_iter()
            .find(|l| l.id == link_uuid)
            .ok_or_else(|| FederationError::Other("Credential link not found".to_string()))?;
        
        // Create credential link message
        let link_msg = CredentialLinkMessage::new(
            link.id.to_string(),
            link.thread_id.to_string(),
            link.credential_cid.clone(),
            link.linked_by.clone(),
        );
        
        // Create sync message
        let sync_msg = SyncMessage::CredentialLink(link_msg);
        let data = sync_msg.to_bytes()
            .map_err(|e| FederationError::Serialization(e.to_string()))?;
        
        // Publish to the network
        let mut network = self.network.write().await;
        network.publish(NetworkTopic::CredentialLinkAnnounce, data).await
            .map_err(|e| FederationError::Network(e.to_string()))?;
        
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