use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use wallet_core::identity::IdentityWallet;
use wallet_agent::queue::ProposalQueue;
use wallet_agent::governance::Guardian;
use wallet_agent::agoranet::AgoraNetClient;
use wallet_sync::client::SyncClient;
use crate::error::{ApiResult, ApiError};
use wallet_core::store::LocalWalletStore;
use wallet_agent::queue::ActionQueue;

/// The shared application state
pub struct AppState<S: LocalWalletStore> {
    /// The store implementation
    pub store: S,
    /// Configuration for the application
    pub config: AppConfig,
}

/// Configuration for the application
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// The base URL for the federation API
    pub federation_url: String,
    /// The path to the wallet data directory
    pub data_dir: String,
    /// Whether to auto-sync with the federation
    pub auto_sync: bool,
    /// The sync interval in seconds
    pub sync_interval: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            federation_url: "https://federation.example.com/api".to_string(),
            data_dir: "./wallet-data".to_string(),
            auto_sync: true,
            sync_interval: 60,
        }
    }
}

impl<S: LocalWalletStore> AppState<S> {
    /// Create a new AppState with the given store and config
    pub fn new(store: S, config: AppConfig) -> Self {
        Self {
            store,
            config,
        }
    }
    
    /// Create a new AppState with the given store and default config
    pub fn with_store(store: S) -> Self {
        Self {
            store,
            config: AppConfig::default(),
        }
    }
    
    /// Get an action queue using this state's store
    pub fn action_queue(&self) -> ActionQueue<S> {
        ActionQueue::new(self.store.clone())
    }
    
    /// Create a sync client using this state's config
    pub fn sync_client(&self) -> SyncClient {
        SyncClient::new(&self.config.federation_url)
    }
}

pub struct AppState {
    pub identities: RwLock<HashMap<String, IdentityWallet>>,
    pub data_dir: PathBuf,
    pub active_identity_id: RwLock<Option<String>>,
    pub agoranet_url: Option<String>,
}

impl AppState {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        let data_path = data_dir.as_ref().to_path_buf();
        
        Self {
            identities: RwLock::new(HashMap::new()),
            data_dir: data_path,
            active_identity_id: RwLock::new(None),
            agoranet_url: None,
        }
    }
    
    pub fn with_agoranet_url(mut self, url: &str) -> Self {
        self.agoranet_url = Some(url.to_string());
        self
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
        let bundle_storage = self.data_dir.join("bundles");
        
        Ok(Guardian::new(identity, queue).with_bundle_storage(bundle_storage))
    }
    
    pub async fn create_sync_client(&self) -> ApiResult<SyncClient> {
        let identity = self.get_active_identity().await?;
        let result = SyncClient::new(identity, None)
            .map_err(ApiError::SyncError)?;
        Ok(result)
    }
    
    pub async fn create_agoranet_client(&self) -> ApiResult<AgoraNetClient> {
        let identity = self.get_active_identity().await?;
        Ok(AgoraNetClient::new(identity, self.agoranet_url.clone()))
    }
}

pub type SharedState = Arc<AppState>; 