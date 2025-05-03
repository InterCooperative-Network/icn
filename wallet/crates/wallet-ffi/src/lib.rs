//! FFI bridge for wallet functionality
//! This module provides Foreign Function Interface (FFI) bindings for the wallet core functionality
//! using the uniffi framework.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};
use once_cell::sync::Lazy;
use tokio::runtime::{Builder, Runtime};
use uniffi::deps::log;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};
use serde::{Serialize, Deserialize};
use thiserror::Error;

// Re-export wallet crate elements that we use directly
use wallet_core::config::{self, WalletConfig as CoreWalletConfig, SyncConfig};
use wallet_core::dag::{DagThread, DagNode, CachedDagThreadInfo as CoreCachedDagThreadInfo, ThreadType};
use wallet_core::error::{WalletError as CoreWalletError, WalletResult};
use wallet_core::identity::IdentityWallet;
use wallet_core::store::file::FileStore;
use wallet_agent::{ActionQueue, PendingAction};
use wallet_agent::{ActionProcessor, ProcessingStatus, ThreadConflict, ConflictResolutionStrategy};
use wallet_types::action::{ActionStatus as CoreActionStatus, ActionType};
use wallet_types::network::{NetworkStatus, NodeSubmissionResponse};
use wallet_sync::trust::TrustBundleValidator;
use wallet_agent::governance::TrustBundle;

// Global Tokio runtime for async operations
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Builder::new_multi_thread()
        .enable_all()
        .thread_name("wallet-ffi-runtime")
        .worker_threads(4)
        .build()
        .expect("Failed to create tokio runtime")
});

// Initialize logging once at startup
static LOGGING: Lazy<()> = Lazy::new(|| {
    // Initialize logging if RUST_LOG is set, otherwise set a default
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,wallet_ffi=debug");
    }
    
    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(false) // Disable ANSI colors for mobile compatibility
        .finish();
        
    let _ = tracing::subscriber::set_global_default(subscriber);
    info!("Wallet FFI logging initialized");
});

// Error type for FFI
#[derive(Debug, Error)]
pub enum WalletError {
    #[error("Store error: {0}")]
    StoreError(String),
    
    #[error("Sync error: {0}")]
    SyncError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Cryptographic error: {0}")]
    CryptoError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    
    #[error("Unknown error: {0}")]
    UnknownError(String),
}

// Convert core wallet errors to FFI errors
impl From<CoreWalletError> for WalletError {
    fn from(error: CoreWalletError) -> Self {
        match error {
            CoreWalletError::StoreError(msg) => WalletError::StoreError(msg),
            CoreWalletError::NotFound(msg) => WalletError::NotFound(msg),
            CoreWalletError::CryptoError(msg) => WalletError::CryptoError(msg),
            CoreWalletError::SerializationError(msg) => WalletError::SerializationError(msg),
            CoreWalletError::ValidationError(msg) => WalletError::ValidationError(msg),
            CoreWalletError::ConfigError(msg) => WalletError::ConfigError(msg),
            _ => WalletError::UnknownError(format!("{}", error)),
        }
    }
}

// Convert SyncError to our FFI error type
impl From<wallet_sync::error::SyncError> for WalletError {
    fn from(error: wallet_sync::error::SyncError) -> Self {
        match error {
            wallet_sync::error::SyncError::CoreError(core_err) => core_err.into(),
            wallet_sync::error::SyncError::ConnectionError(msg) => WalletError::SyncError(msg),
            wallet_sync::error::SyncError::NotFound(msg) => WalletError::NotFound(msg),
            wallet_sync::error::SyncError::ValidationError(msg) => WalletError::ValidationError(msg),
            wallet_sync::error::SyncError::Offline(msg) => WalletError::SyncError(msg),
            _ => WalletError::SyncError(format!("{}", error)),
        }
    }
}

// Convert ActionProcessorError to our FFI error type
impl From<wallet_agent::error::AgentError> for WalletError {
    fn from(error: wallet_agent::error::AgentError) -> Self {
        match error {
            wallet_agent::error::AgentError::CoreError(core_err) => core_err.into(),
            wallet_agent::error::AgentError::ValidationError(msg) => WalletError::ValidationError(msg),
            wallet_agent::error::AgentError::NotFound(msg) => WalletError::NotFound(msg),
            _ => WalletError::UnknownError(format!("{}", error)),
        }
    }
}

// Enum representing sync status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Idle,
    Syncing,
    Error,
    Offline,
}

// Enum representing action status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Rejected,
}

// Conversion from core action status
impl From<CoreActionStatus> for ActionStatus {
    fn from(status: CoreActionStatus) -> Self {
        match status {
            CoreActionStatus::Pending => ActionStatus::Pending,
            CoreActionStatus::Processing => ActionStatus::Processing,
            CoreActionStatus::Completed => ActionStatus::Completed,
            CoreActionStatus::Failed => ActionStatus::Failed,
            CoreActionStatus::Rejected => ActionStatus::Rejected,
        }
    }
}

// Enum representing DAG thread types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DagThreadType {
    Proposal,
    Vote,
    Anchor,
    Custom,
}

// Conversion from core thread type
impl From<ThreadType> for DagThreadType {
    fn from(thread_type: ThreadType) -> Self {
        match thread_type {
            ThreadType::Proposal => DagThreadType::Proposal,
            ThreadType::Vote => DagThreadType::Vote,
            ThreadType::Anchor => DagThreadType::Anchor,
            ThreadType::Custom(_) => DagThreadType::Custom,
        }
    }
}

// Data structures for FFI

#[derive(Debug, Clone)]
pub struct IdentityInfo {
    pub id: String,
    pub display_name: String,
    pub created_at: String,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct IdentityDetails {
    pub id: String,
    pub display_name: String,
    pub created_at: String,
    pub is_active: bool,
    pub credential_ids: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct TrustBundleInfo {
    pub id: String,
    pub epoch: u64,
    pub is_active: bool,
    pub guardians: Vec<String>,
    pub threshold: u32,
}

impl From<TrustBundle> for TrustBundleInfo {
    fn from(bundle: TrustBundle) -> Self {
        Self {
            id: bundle.id,
            epoch: bundle.epoch,
            is_active: bundle.active,
            guardians: bundle.guardians,
            threshold: bundle.threshold as u32,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CachedDagThreadInfo {
    pub thread_id: String,
    pub thread_type: DagThreadType,
    pub node_cids: Vec<String>,
    pub head_cid: String,
    pub tail_cid: String,
    pub last_updated: String,
    pub metadata: HashMap<String, String>,
}

impl From<CoreCachedDagThreadInfo> for CachedDagThreadInfo {
    fn from(info: CoreCachedDagThreadInfo) -> Self {
        Self {
            thread_id: info.thread_id,
            thread_type: info.thread_type.into(),
            node_cids: info.node_cids,
            head_cid: info.head_cid,
            tail_cid: info.tail_cid,
            last_updated: info.last_updated.to_rfc3339(),
            metadata: info.metadata,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActionInfo {
    pub id: String,
    pub creator_id: String,
    pub action_type: String,
    pub created_at: String,
    pub status: ActionStatus,
    pub error_message: Option<String>,
    pub payload: HashMap<String, String>,
}

impl From<Action> for ActionInfo {
    fn from(action: Action) -> Self {
        let payload = if let Ok(map) = serde_json::from_value::<HashMap<String, String>>(action.payload.clone()) {
            map
        } else {
            HashMap::new()
        };
        
        Self {
            id: action.id,
            creator_id: action.creator_id,
            action_type: action.action_type,
            created_at: action.created_at.to_rfc3339(),
            status: ActionStatus::Pending, // Default, should be overridden by ActionRecord
            error_message: None,
            payload,
        }
    }
}

impl From<ActionRecord> for ActionInfo {
    fn from(record: ActionRecord) -> Self {
        let mut info = record.action.into();
        info.status = record.status.into();
        info.error_message = record.error_message;
        info
    }
}

#[derive(Debug, Clone)]
pub struct SyncStatusInfo {
    pub status: SyncStatus,
    pub last_sync_timestamp: Option<u64>,
    pub is_connected: bool,
    pub error_message: Option<String>,
    pub pending_actions: u64,
}

#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub storage_path: String,
    pub federation_urls: Vec<String>,
    pub sync_interval_seconds: u64,
    pub auto_sync_on_startup: bool,
}

impl From<CoreWalletConfig> for WalletConfig {
    fn from(config: CoreWalletConfig) -> Self {
        Self {
            storage_path: config.store_path.to_string_lossy().to_string(),
            federation_urls: config.sync.federation_urls,
            sync_interval_seconds: config.sync.sync_interval_seconds,
            auto_sync_on_startup: config.sync.auto_sync_on_startup,
        }
    }
}

impl Into<CoreWalletConfig> for WalletConfig {
    fn into(self) -> CoreWalletConfig {
        CoreWalletConfig {
            store_path: PathBuf::from(self.storage_path),
            sync: SyncConfig {
                federation_urls: self.federation_urls,
                sync_interval_seconds: self.sync_interval_seconds,
                auto_sync_on_startup: self.auto_sync_on_startup,
                ..SyncConfig::default()
            },
            ..CoreWalletConfig::default()
        }
    }
}

// The main API for mobile platforms
pub struct WalletApi {
    store: Arc<FileStore>,
    processor: Arc<Mutex<ActionProcessor<FileStore>>>,
    sync_manager: Arc<Mutex<SyncManager<FileStore>>>,
    config: Arc<Mutex<CoreWalletConfig>>,
    identity: Arc<Mutex<IdentityWallet>>,
}

// Custom validator for TrustBundle and DagNode - to be enhanced
pub struct DataValidator {
    trust_validator: TrustBundleValidator,
}

impl DataValidator {
    pub fn new(identity: IdentityWallet) -> Self {
        Self {
            trust_validator: TrustBundleValidator::new(identity),
        }
    }
    
    // Validate trust bundle with enhanced checks
    pub fn validate_trust_bundle(&self, bundle: &TrustBundle) -> Result<(), WalletError> {
        // Basic validation from existing validator
        self.trust_validator.validate_bundle(bundle)
            .map_err(|e| WalletError::ValidationError(format!("Trust bundle validation failed: {}", e)))?;
        
        // Enhanced validations:
        
        // 1. Check timestamp is not in the future
        let now = std::time::SystemTime::now();
        if let Ok(duration) = bundle.created_at.duration_since(now) {
            if duration.as_secs() > 60 {  // Allow 1 minute clock skew
                return Err(WalletError::ValidationError(
                    format!("Trust bundle has a future timestamp: {:?}", bundle.created_at)
                ));
            }
        }
        
        // 2. Check threshold is reasonable based on guardian count
        if bundle.threshold < 1 || bundle.threshold as usize > bundle.guardians.len() {
            return Err(WalletError::ValidationError(
                format!("Invalid threshold: {} for {} guardians", bundle.threshold, bundle.guardians.len())
            ));
        }
        
        // 3. Check expiration if set
        if let Some(expires_at) = bundle.expires_at {
            if expires_at < now {
                return Err(WalletError::ValidationError(
                    format!("Trust bundle has expired at {:?}", expires_at)
                ));
            }
        }
        
        // 4. Check signature validation
        // In a real implementation, we'd verify signatures from enough guardians to meet threshold
        // This would require cryptographic signature validation logic
        
        Ok(())
    }
    
    // Validate a DAG node
    pub fn validate_dag_node(&self, node: &DagNode, expected_cid: Option<&str>) -> Result<(), WalletError> {
        // This would be a more comprehensive validation than just checking CID
        
        // 1. Basic structural validation
        if node.cid.is_empty() {
            return Err(WalletError::ValidationError("DAG node missing CID".to_string()));
        }
        
        // 2. Verify CID if provided
        if let Some(cid) = expected_cid {
            if node.cid != cid {
                return Err(WalletError::ValidationError(
                    format!("CID mismatch: expected {}, got {}", cid, node.cid)
                ));
            }
        }
        
        // 3. Check timestamp is reasonable
        let now = std::time::SystemTime::now();
        if let Ok(duration) = node.timestamp.duration_since(now) {
            if duration.as_secs() > 60 {  // Allow 1 minute clock skew
                return Err(WalletError::ValidationError(
                    format!("DAG node has a future timestamp: {:?}", node.timestamp)
                ));
            }
        }
        
        // 4. Check parent references (must be valid CIDs)
        for parent in &node.parents {
            if parent.is_empty() {
                return Err(WalletError::ValidationError("Empty parent CID reference".to_string()));
            }
            
            // Additional CID format validation could be done here
        }
        
        // 5. Check signature validation
        if node.signatures.is_empty() {
            return Err(WalletError::ValidationError("DAG node has no signatures".to_string()));
        }
        
        // 6. Verify creator exists and is authorized (in real impl)
        if node.creator.is_empty() {
            return Err(WalletError::ValidationError("DAG node missing creator".to_string()));
        }
        
        // In a full implementation, we'd verify:
        // - Signatures match the content and are by the creator
        // - Creator is authorized for this content type
        // - Content schema validation based on content_type
        
        Ok(())
    }
}

impl WalletApi {
    // Create a new wallet API instance
    pub fn new(config: Option<WalletConfig>) -> Result<Self, WalletError> {
        // Ensure logging is initialized
        Lazy::force(&LOGGING);
        
        // Convert config or use default
        let core_config = match config {
            Some(cfg) => cfg.into(),
            None => CoreWalletConfig::default(),
        };
        
        // Create store
        let store_path = core_config.store_path.clone();
        let store = Arc::new(FileStore::new(store_path));
        
        // Initialize store
        RUNTIME.block_on(async {
            store.init().await.map_err(|e| WalletError::StoreError(format!("Failed to initialize store: {}", e)))?;
            Ok::<_, WalletError>(())
        })?;
        
        // Create a default identity if none exists
        let identity = RUNTIME.block_on(async {
            let identities = store.list_identities().await.map_err(|e| {
                WalletError::StoreError(format!("Failed to list identities: {}", e))
            })?;
            
            if identities.is_empty() {
                debug!("No identities found, creating default agent identity");
                
                // Create a new identity for the agent
                let id_wallet = IdentityWallet::new("agent", Some("Wallet Agent")).map_err(|e| {
                    WalletError::CryptoError(format!("Failed to create identity: {}", e))
                })?;
                
                // Save the identity
                store.save_identity(&id_wallet).await.map_err(|e| {
                    WalletError::StoreError(format!("Failed to save identity: {}", e))
                })?;
                
                Ok::<_, WalletError>(id_wallet)
            } else {
                // Load the first identity
                let first_id = &identities[0];
                store.load_identity(first_id).await.map_err(|e| {
                    WalletError::StoreError(format!("Failed to load identity: {}", e))
                })
            }
        })?;
        
        // Create sync manager
        let sync_config = SyncManagerConfig::from(&core_config.sync);
        let sync_manager = SyncManager::new(identity.clone(), store.clone(), Some(sync_config));
        
        // Create action processor with sync manager
        let processor = ActionProcessor::with_sync_manager(store.clone(), sync_manager.clone());
        
        // Start sync manager
        RUNTIME.block_on(async {
            sync_manager.start().await.map_err(|e| {
                WalletError::SyncError(format!("Failed to start sync manager: {}", e))
            })
        })?;
        
        Ok(Self {
            store,
            processor: Arc::new(Mutex::new(processor)),
            sync_manager: Arc::new(Mutex::new(sync_manager)),
            config: Arc::new(Mutex::new(core_config)),
            identity: Arc::new(Mutex::new(identity)),
        })
    }
    
    // Identity management
    
    pub fn create_identity(&self, scope: String, metadata: HashMap<String, String>) -> Result<String, WalletError> {
        let display_name = metadata.get("displayName").cloned().unwrap_or_else(|| scope.clone());
        
        let identity = RUNTIME.block_on(async {
            // Create identity
            let id_wallet = IdentityWallet::new(&scope, Some(&display_name)).map_err(|e| {
                WalletError::CryptoError(format!("Failed to create identity: {}", e))
            })?;
            
            let id = id_wallet.did.to_string();
            
            // Store the identity
            self.store.save_identity(&id_wallet).await.map_err(|e| {
                WalletError::StoreError(format!("Failed to save identity: {}", e))
            })?;
            
            Ok::<_, WalletError>(id)
        })?;
        
        Ok(identity)
    }
    
    pub fn list_identities(&self) -> Result<Vec<IdentityInfo>, WalletError> {
        RUNTIME.block_on(async {
            let ids = self.store.list_identities().await.map_err(|e| {
                WalletError::StoreError(format!("Failed to list identities: {}", e))
            })?;
            
            let mut results = Vec::with_capacity(ids.len());
            
            for id in ids {
                let wallet = self.store.load_identity(&id).await.map_err(|e| {
                    WalletError::StoreError(format!("Failed to load identity {}: {}", id, e))
                })?;
                
                results.push(IdentityInfo {
                    id: wallet.did.to_string(),
                    display_name: wallet.metadata.get("displayName")
                        .cloned()
                        .unwrap_or_else(|| wallet.scope.clone()),
                    created_at: wallet.created_at.to_rfc3339(),
                    is_active: true,
                });
            }
            
            Ok(results)
        })
    }
    
    pub fn get_identity(&self, id: String) -> Result<IdentityDetails, WalletError> {
        RUNTIME.block_on(async {
            let wallet = self.store.load_identity(&id).await.map_err(|e| {
                WalletError::NotFound(format!("Identity not found: {}", e))
            })?;
            
            // In a real implementation, we'd fetch associated credentials too
            
            Ok(IdentityDetails {
                id: wallet.did.to_string(),
                display_name: wallet.metadata.get("displayName")
                    .cloned()
                    .unwrap_or_else(|| wallet.scope.clone()),
                created_at: wallet.created_at.to_rfc3339(),
                is_active: true,
                credential_ids: Vec::new(), // Would be populated in real implementation
                metadata: wallet.metadata,
            })
        })
    }
    
    pub fn delete_identity(&self, id: String) -> Result<bool, WalletError> {
        // This is a placeholder. In a real implementation, we would:
        // 1. Check if identity exists
        // 2. Mark it as inactive or archive it
        // 3. Return success
        
        // Currently, the wallet-core doesn't expose a delete_identity method
        // We'd need to add this functionality
        
        Err(WalletError::ConfigError("Identity deletion not implemented".into()))
    }
    
    // Action management
    
    pub fn queue_action(&self, creator_id: String, action_type: String, payload: HashMap<String, String>) -> Result<String, WalletError> {
        let processor = self.processor.lock().unwrap();
        
        RUNTIME.block_on(async {
            // Convert payload to JSON Value
            let payload_value = serde_json::to_value(payload).map_err(|e| {
                WalletError::SerializationError(format!("Failed to serialize payload: {}", e))
            })?;
            
            // Create and queue the action
            let action_id = processor.queue_action(&creator_id, &action_type, payload_value).await
                .map_err(|e| WalletError::from(e))?;
                
            Ok(action_id)
        })
    }
    
    pub fn process_action(&self, action_id: String) -> Result<(), WalletError> {
        let processor = self.processor.lock().unwrap();
        
        RUNTIME.block_on(async {
            processor.process_action(&action_id).await
                .map_err(|e| WalletError::from(e))?;
            Ok(())
        })
    }
    
    pub fn list_actions(&self, status: Option<ActionStatus>) -> Result<Vec<ActionInfo>, WalletError> {
        let processor = self.processor.lock().unwrap();
        
        RUNTIME.block_on(async {
            // In a real implementation, we'd filter by status
            // For now, we'll return all actions
            
            let actions = processor.list_actions().await
                .map_err(|e| WalletError::from(e))?;
                
            Ok(actions.into_iter().map(ActionInfo::from).collect())
        })
    }
    
    pub fn get_action_status(&self, action_id: String) -> Result<ActionInfo, WalletError> {
        let processor = self.processor.lock().unwrap();
        
        RUNTIME.block_on(async {
            let record = processor.get_action(&action_id).await
                .map_err(|e| WalletError::from(e))?;
                
            Ok(record.into())
        })
    }
    
    // Sync management
    
    pub fn trigger_sync(&self) -> Result<(), WalletError> {
        let sync_manager = self.sync_manager.lock().unwrap();
        
        RUNTIME.block_on(async {
            sync_manager.sync_all().await
                .map_err(|e| WalletError::from(e))
        })
    }
    
    pub fn get_sync_status(&self) -> Result<SyncStatusInfo, WalletError> {
        let sync_manager = self.sync_manager.lock().unwrap();
        
        RUNTIME.block_on(async {
            let network_status = sync_manager.get_network_status().await
                .map_err(|e| WalletError::from(e))?;
                
            // Get federation state (using default ID)
            let federation_state = sync_manager.get_sync_state("default").await;
            
            let last_sync_timestamp = federation_state
                .map(|state| state.last_sync_time.duration_since(std::time::UNIX_EPOCH).ok())
                .flatten()
                .map(|d| d.as_secs());
                
            let status = if !network_status.is_connected {
                SyncStatus::Offline
            } else if network_status.failed_operations > 0 {
                SyncStatus::Error
            } else if network_status.pending_submissions > 0 {
                SyncStatus::Syncing
            } else {
                SyncStatus::Idle
            };
            
            Ok(SyncStatusInfo {
                status,
                last_sync_timestamp,
                is_connected: network_status.is_connected,
                error_message: None, // Would be populated from actual errors
                pending_actions: network_status.pending_submissions as u64,
            })
        })
    }
    
    // Trust bundles
    
    pub fn list_trust_bundles(&self) -> Result<Vec<TrustBundleInfo>, WalletError> {
        let sync_manager = self.sync_manager.lock().unwrap();
        
        RUNTIME.block_on(async {
            // Trigger a sync of trust bundles
            let bundles = sync_manager.sync_trust_bundles(&sync_manager.config.federation_urls[0]).await
                .map_err(|e| WalletError::from(e))?;
                
            Ok(bundles.into_iter().map(TrustBundleInfo::from).collect())
        })
    }
    
    pub fn get_trust_bundle(&self, id: String) -> Result<Option<TrustBundleInfo>, WalletError> {
        // In a real implementation, we'd fetch a specific bundle by ID
        // For now, we'll simulate by listing all and finding the matching one
        
        let bundles = self.list_trust_bundles()?;
        let found = bundles.into_iter().find(|b| b.id == id);
        
        Ok(found)
    }
    
    // DAG thread operations
    
    pub fn list_dag_threads(&self) -> Result<Vec<String>, WalletError> {
        RUNTIME.block_on(async {
            self.store.list_dag_threads().await
                .map_err(|e| WalletError::StoreError(format!("Failed to list DAG threads: {}", e)))
        })
    }
    
    pub fn get_dag_thread_cache(&self, thread_id: String) -> Result<Option<CachedDagThreadInfo>, WalletError> {
        RUNTIME.block_on(async {
            match self.store.load_dag_thread(&thread_id).await {
                Ok(thread) => {
                    // Create cache info from thread
                    let cache_info = CoreCachedDagThreadInfo::new(
                        &thread_id,
                        thread.thread_type,
                        &thread.root_cid
                    );
                    
                    Ok(Some(cache_info.into()))
                },
                Err(CoreWalletError::NotFound(_)) => Ok(None),
                Err(e) => Err(WalletError::StoreError(format!("Failed to load DAG thread: {}", e))),
            }
        })
    }
    
    // Configuration
    
    pub fn get_config(&self) -> Result<WalletConfig, WalletError> {
        let config = self.config.lock().unwrap();
        Ok(config.clone().into())
    }
    
    pub fn update_config(&self, config: WalletConfig) -> Result<(), WalletError> {
        let mut current_config = self.config.lock().unwrap();
        let new_config: CoreWalletConfig = config.into();
        
        // Update sync manager configuration if needed
        if current_config.sync != new_config.sync {
            let mut sync_manager = self.sync_manager.lock().unwrap();
            
            RUNTIME.block_on(async {
                let sync_config = SyncManagerConfig::from(&new_config.sync);
                sync_manager.update_config(sync_config).await
                    .map_err(|e| WalletError::from(e))
            })?;
        }
        
        // Update stored config
        *current_config = new_config;
        
        Ok(())
    }
}

// Define the uniffi scaffolding
uniffi::include_scaffolding!("wallet"); 