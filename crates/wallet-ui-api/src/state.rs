use std::sync::Arc;
use std::path::PathBuf;
use wallet_core::identity::IdentityWallet;
use wallet_agent::queue::ActionQueue;
use wallet_sync::SyncManager;
use wallet_core::store::{
    LocalWalletStore, 
    FileStore,
    SecurePlatform,
    create_mock_secure_store
};

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

/// The shared application state
pub struct AppState<S: LocalWalletStore> {
    /// The store implementation
    pub store: S,
    /// Configuration for the application
    pub config: AppConfig,
    /// Whether secure storage is enabled
    pub secure_storage_enabled: bool,
}

impl<S: LocalWalletStore> AppState<S> {
    /// Create a new AppState with the given store and config
    pub fn new(store: S, config: AppConfig) -> Self {
        Self {
            store,
            config,
            secure_storage_enabled: true,
        }
    }
    
    /// Create a new AppState with the given store and default config
    pub fn with_store(store: S) -> Self {
        Self {
            store,
            config: AppConfig::default(),
            secure_storage_enabled: true,
        }
    }
    
    /// Create an action queue using this state's store
    pub fn action_queue(&self) -> ActionQueue<S> {
        ActionQueue::new(self.store.clone())
    }
    
    /// Create a sync client using this state's config
    pub fn sync_client(&self) -> SyncManager<S> {
        // For simplicity, we'll use the first identity if available
        // In a real implementation, this would use the active identity
        let identities = match self.store.list_identities() {
            Ok(ids) if !ids.is_empty() => ids,
            _ => return SyncManager::new(
                IdentityWallet::new(
                    wallet_core::identity::IdentityScope::Service, 
                    None
                ),
                self.store.clone(),
                None
            ),
        };
        
        let identity = match self.store.load_identity(&identities[0]) {
            Ok(id) => id,
            _ => IdentityWallet::new(
                wallet_core::identity::IdentityScope::Service, 
                None
            ),
        };
        
        SyncManager::new(identity, self.store.clone(), None)
    }
}

/// Create a file-based store
pub fn create_file_store(data_dir: &str) -> FileStore {
    FileStore::new(data_dir)
}

/// Create a secure store for the appropriate platform
pub fn create_secure_store(data_dir: &str, platform: SecurePlatform) -> impl LocalWalletStore {
    create_mock_secure_store(platform, data_dir)
} 