use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use wallet_core::identity::IdentityWallet;
use wallet_agent::queue::ProposalQueue;
use wallet_agent::governance::Guardian;
use wallet_sync::client::SyncClient;
use crate::error::{ApiResult, ApiError};

pub struct AppState {
    pub identities: RwLock<HashMap<String, IdentityWallet>>,
    pub data_dir: PathBuf,
    pub active_identity_id: RwLock<Option<String>>,
}

impl AppState {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        let data_path = data_dir.as_ref().to_path_buf();
        
        Self {
            identities: RwLock::new(HashMap::new()),
            data_dir: data_path,
            active_identity_id: RwLock::new(None),
        }
    }
    
    pub async fn get_active_identity(&self) -> ApiResult<IdentityWallet> {
        let active_id = self.active_identity_id.read().await;
        
        if let Some(id) = active_id.as_ref() {
            let identities = self.identities.read().await;
            
            if let Some(identity) = identities.get(id) {
                return Ok(identity.clone());
            }
        }
        
        Err(ApiError::AuthError("No active identity selected".to_string()))
    }
    
    pub async fn set_active_identity(&self, id: &str) -> ApiResult<()> {
        let identities = self.identities.read().await;
        
        if identities.contains_key(id) {
            let mut active_id = self.active_identity_id.write().await;
            *active_id = Some(id.to_string());
            Ok(())
        } else {
            Err(ApiError::NotFound(format!("Identity not found: {}", id)))
        }
    }
    
    pub async fn create_proposal_queue(&self) -> ApiResult<ProposalQueue> {
        let identity = self.get_active_identity().await?;
        let queue_dir = self.data_dir.join("queue");
        Ok(ProposalQueue::new(queue_dir, identity))
    }
    
    pub async fn create_guardian(&self) -> ApiResult<Guardian> {
        let identity = self.get_active_identity().await?;
        let queue = self.create_proposal_queue().await?;
        Ok(Guardian::new(identity, queue))
    }
    
    pub async fn create_sync_client(&self) -> ApiResult<SyncClient> {
        let identity = self.get_active_identity().await?;
        let result = SyncClient::new(identity, None)
            .map_err(ApiError::SyncError)?;
        Ok(result)
    }
}

pub type SharedState = Arc<AppState>; 