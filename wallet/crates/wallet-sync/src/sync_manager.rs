use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use serde::{Serialize, Deserialize};
use wallet_core::identity::IdentityWallet;
use wallet_core::dag::{DagNode, DagThread};
use wallet_core::error::WalletResult;
use wallet_core::store::LocalWalletStore;
use crate::error::{SyncResult, SyncError};
use crate::trust::TrustBundleValidator;
use crate::client::SyncClient;
use wallet_types::TrustBundle;
use wallet_types::network::{NetworkStatus, NodeSubmissionResponse};
use reqwest::{Client as HttpClient, StatusCode};
use backoff::{ExponentialBackoff, future::retry};
use tracing::{info, warn, error, debug};
use wallet_core::config::{WalletConfig, SyncConfig};

// Default federation endpoint URLs
const DEFAULT_FEDERATION_NODE_URL: &str = "http://mock-federation-node";
const DEFAULT_BUNDLE_ENDPOINT: &str = "/bundles/latest";
const DEFAULT_NODE_ENDPOINT: &str = "/nodes";
const DEFAULT_THREAD_ENDPOINT: &str = "/threads";
const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Represents the sync state for a federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSyncState {
    /// The federation ID
    pub federation_id: String,
    /// The federation URL
    pub federation_url: String,
    /// The last synced epoch for this federation
    pub last_synced_epoch: u64,
    /// The last sync time for this federation
    pub last_sync_time: SystemTime,
    /// Number of trust bundles last synced
    pub trust_bundles_count: usize,
    /// Number of DAG headers last synced
    pub dag_headers_count: usize,
}

/// Configuration for the SyncManager
#[derive(Debug, Clone)]
pub struct SyncManagerConfig {
    /// URLs of federation nodes to sync from
    pub federation_urls: Vec<String>,
    /// Path to store sync state
    pub sync_state_path: PathBuf,
    /// Default sync interval in seconds
    pub sync_interval_seconds: u64,
    /// Whether to auto-sync on startup
    pub auto_sync_on_startup: bool,
    /// Whether to auto-sync periodically
    pub auto_sync_periodic: bool,
    /// HTTP request timeout in seconds
    pub request_timeout_seconds: u64,
    /// Maximum number of retry attempts for HTTP requests
    pub max_retry_attempts: u32,
}

impl Default for SyncManagerConfig {
    fn default() -> Self {
        Self {
            federation_urls: vec![DEFAULT_FEDERATION_NODE_URL.to_string()],
            sync_state_path: PathBuf::from("./storage/sync"),
            sync_interval_seconds: 3600, // 1 hour
            auto_sync_on_startup: true,
            auto_sync_periodic: true,
            request_timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            max_retry_attempts: MAX_RETRY_ATTEMPTS,
        }
    }
}

impl From<&SyncConfig> for SyncManagerConfig {
    fn from(config: &SyncConfig) -> Self {
        Self {
            federation_urls: config.federation_urls.clone(),
            sync_state_path: PathBuf::from("./storage/sync"), // Use the default for now
            sync_interval_seconds: config.sync_interval_seconds,
            auto_sync_on_startup: config.auto_sync_on_startup,
            auto_sync_periodic: config.auto_sync_periodic,
            request_timeout_seconds: config.request_timeout_seconds,
            max_retry_attempts: config.max_retry_attempts,
        }
    }
}

/// Mock data for the TrustBundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockTrustBundleData {
    /// Federation ID
    pub federation_id: String,
    /// Version number
    pub version: u64,
    /// Epoch number
    pub epoch: u64,
    /// List of guardian DIDs
    pub guardians: Vec<String>,
    /// Federation parameters
    pub parameters: HashMap<String, String>,
}

impl MockTrustBundleData {
    /// Create a mock trust bundle data
    pub fn new(federation_id: &str, epoch: u64) -> Self {
        Self {
            federation_id: federation_id.to_string(),
            version: 1,
            epoch,
            guardians: vec![
                "did:icn:guardian1".to_string(),
                "did:icn:guardian2".to_string(),
                "did:icn:guardian3".to_string(),
            ],
            parameters: HashMap::new(),
        }
    }
    
    /// Convert to a trust bundle
    pub fn to_trust_bundle(&self) -> TrustBundle {
        TrustBundle {
            id: self.federation_id.clone(),
            epoch: self.epoch,
            threshold: 2, // Default threshold
            guardians: self.guardians.clone(),
            active: true,
            federation_id: self.federation_id.clone(),
            members: vec![], // Changed from HashMap to Vec
            policies: HashMap::new(),
            metadata: HashMap::new(),
            version: self.version as u32, // Convert u64 to u32
            signatures: HashMap::new(),
            valid_until: Some(SystemTime::now() + Duration::from_secs(86400 * 30)), // 30 days
            created_at: chrono::Utc::now(), // Add created_at field
            links: HashMap::new(), // Add links field
        }
    }
}

/// Mock data for the DAG Headers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockDagHeaderData {
    /// CID of the DAG node
    pub cid: String,
    /// The parent CIDs
    pub parents: Vec<String>,
    /// The epoch number
    pub epoch: u64,
    /// The guardian who created this node
    pub creator: String,
    /// Timestamp when this node was created
    pub timestamp: SystemTime,
    /// Type of the DAG node
    pub node_type: String,
}

impl MockDagHeaderData {
    /// Create a mock DAG header
    pub fn new(cid: &str, parents: Vec<String>, epoch: u64) -> Self {
        Self {
            cid: cid.to_string(),
            parents,
            epoch,
            creator: "did:icn:guardian1".to_string(),
            timestamp: SystemTime::now(),
            node_type: "proposal".to_string(),
        }
    }
    
    /// Convert to a DAG node
    pub fn to_dag_node(&self) -> DagNode {
        DagNode {
            data: serde_json::Value::from(serde_json::to_string(&self).unwrap_or_default()),
            links: {
                let mut links = HashMap::new();
                links.insert("self".to_string(), self.cid.clone());
                for (i, parent) in self.parents.iter().enumerate() {
                    links.insert(format!("parent_{}", i), parent.clone());
                }
                links
            },
            signatures: HashMap::new(),
            created_at: chrono::Utc::now(),
        }
    }
}

/// Mock network status for internal use only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncNetworkStatus {
    /// Connection status to federation nodes
    pub is_connected: bool,
    /// Latency to primary federation node in milliseconds
    pub primary_node_latency: Option<u64>,
    /// Last successful sync time
    pub last_successful_sync: Option<SystemTime>,
    /// Number of pending submissions
    pub pending_submissions: usize,
    /// Current federation node in use
    pub active_federation_url: String,
    /// Number of successful operations
    pub successful_operations: usize,
    /// Number of failed operations
    pub failed_operations: usize,
}

impl SyncNetworkStatus {
    /// Convert to the public NetworkStatus type
    pub fn to_network_status(&self) -> wallet_types::network::NetworkStatus {
        wallet_types::network::NetworkStatus {
            online: self.is_connected,
            network_type: "federation".to_string(),
            peer_count: 1, // Placeholder
            block_height: 0, // Placeholder
            latency_ms: self.primary_node_latency.unwrap_or(0),
            sync_percent: 100, // Placeholder
            metadata: {
                let mut map = HashMap::new();
                map.insert("active_url".to_string(), self.active_federation_url.clone());
                map.insert("pending_submissions".to_string(), self.pending_submissions.to_string());
                if let Some(last_sync) = self.last_successful_sync {
                    map.insert("last_sync".to_string(), format!("{:?}", last_sync));
                }
                map
            },
        }
    }
}

/// The SyncManager coordinates synchronization with federation nodes
pub struct SyncManager<S: LocalWalletStore + 'static> {
    /// The identity used for authentication
    identity: IdentityWallet,
    /// HTTP client for network communication
    http_client: HttpClient,
    /// The store for persisting synchronized data
    store: S,
    /// The validator for trust bundles
    trust_validator: TrustBundleValidator,
    /// Configuration for the sync manager
    config: SyncManagerConfig,
    /// Current sync state for each federation
    sync_states: Arc<RwLock<HashMap<String, FederationSyncState>>>,
    /// Lock for sync operations
    sync_lock: Arc<Mutex<()>>,
}

impl<S: LocalWalletStore + 'static> SyncManager<S> {
    /// Create a new SyncManager
    pub fn new(identity: IdentityWallet, store: S, config: Option<SyncManagerConfig>) -> Self {
        let config = config.unwrap_or_default();
        let trust_validator = TrustBundleValidator::new(identity.clone());
        
        // Create an HTTP client with configurable timeout
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .build()
            .unwrap_or_else(|_| HttpClient::new());
        
        Self {
            identity,
            http_client,
            store,
            trust_validator,
            config,
            sync_states: Arc::new(RwLock::new(HashMap::new())),
            sync_lock: Arc::new(Mutex::new(())),
        }
    }
    
    /// Create a new SyncManager from WalletConfig
    pub fn from_wallet_config(identity: IdentityWallet, store: S, config: &WalletConfig) -> Self {
        let sync_config = SyncManagerConfig::from(&config.sync);
        Self::new(identity, store, Some(sync_config))
    }
    
    /// Update the SyncManager configuration
    pub async fn update_config(&mut self, config: SyncManagerConfig) -> SyncResult<()> {
        // Update HTTP client if timeout changed
        if self.config.request_timeout_seconds != config.request_timeout_seconds {
            self.http_client = HttpClient::builder()
                .timeout(Duration::from_secs(config.request_timeout_seconds))
                .build()
                .map_err(|e| SyncError::ConfigurationError(format!("Failed to create HTTP client: {}", e)))?;
        }
        
        // Update config
        self.config = config;
        
        Ok(())
    }
    
    /// Start the sync manager
    pub async fn start(&self) -> SyncResult<()> {
        // Load existing sync state
        self.load_sync_state().await?;
        
        // Perform initial sync if configured
        if self.config.auto_sync_on_startup {
            self.sync_all().await?;
        }
        
        // Start periodic sync if configured
        if self.config.auto_sync_periodic {
            self.start_periodic_sync().await;
        }
        
        Ok(())
    }
    
    /// Load sync state from disk
    async fn load_sync_state(&self) -> SyncResult<()> {
        // For the mock implementation, just use default values
        let mock_state = FederationSyncState {
            federation_id: "default".to_string(),
            federation_url: self.config.federation_urls[0].clone(),
            last_synced_epoch: 0,
            last_sync_time: SystemTime::now() - Duration::from_secs(86400), // 1 day ago
            trust_bundles_count: 0,
            dag_headers_count: 0,
        };
        
        let mut states = self.sync_states.write().await;
        states.insert("default".to_string(), mock_state);
        
        Ok(())
    }
    
    /// Start periodic sync
    async fn start_periodic_sync(&self) {
        let interval_secs = self.config.sync_interval_seconds;
        let sync_states = self.sync_states.clone();
        let sync_lock = self.sync_lock.clone();
        
        // Clone required components for the task
        let federation_urls = self.config.federation_urls.clone();
        let identity = self.identity.clone();
        let store = self.store.clone();
        
        // Spawn a task for periodic sync
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));
            
            loop {
                interval.tick().await;
                
                // Acquire sync lock
                let _lock = sync_lock.lock().await;
                
                // Create a temporary sync manager just for this sync
                let temp_sync_manager = SyncManager::new(
                    identity.clone(),
                    store.clone(),
                    Some(SyncManagerConfig {
                        federation_urls: federation_urls.clone(),
                        sync_interval_seconds: interval_secs,
                        auto_sync_on_startup: false,
                        auto_sync_periodic: false,
                        sync_state_path: PathBuf::from("./storage/sync"),
                        request_timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
                        max_retry_attempts: MAX_RETRY_ATTEMPTS,
                    })
                );
                
                // Perform sync
                if let Err(e) = temp_sync_manager.sync_all().await {
                    eprintln!("Periodic sync failed: {}", e);
                }
            }
        });
    }
    
    /// Synchronize all federation data
    pub async fn sync_all(&self) -> SyncResult<()> {
        // Acquire sync lock
        let _lock = self.sync_lock.lock().await;
        
        // Sync for each federation URL
        for federation_url in &self.config.federation_urls {
            self.sync_federation(federation_url).await?;
        }
        
        Ok(())
    }
    
    /// Synchronize data from a specific federation
    async fn sync_federation(&self, federation_url: &str) -> SyncResult<()> {
        // Get the current state for this federation
        let federation_id = "default"; // In a real impl, we would derive this from the URL
        let last_epoch = {
            let states = self.sync_states.read().await;
            states.get(federation_id)
                .map(|s| s.last_synced_epoch)
                .unwrap_or(0)
        };
        
        // Sync trust bundles
        let trust_bundles = self.sync_trust_bundles(federation_url).await?;
        let bundles_count = trust_bundles.len();
        
        // Process and store trust bundles
        for bundle in trust_bundles {
            // Store the bundle in the wallet store
            self.save_trust_bundle(&bundle).await?;
        }
        
        // Sync DAG headers
        let dag_headers = self.fetch_mock_dag_headers(federation_id, last_epoch).await?;
        let headers_count = dag_headers.len();
        
        // Process and store DAG headers
        for header in dag_headers {
            // Convert to a DAG node
            let node = header.to_dag_node();
            let cid = node.links.get("self").unwrap_or(&String::new()).clone();
            let parents = node.links.iter()
                .filter_map(|(k, v)| {
                    if k.starts_with("parent_") {
                        Some(v.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<String>>();
            let timestamp = chrono::Utc::now().timestamp();
            
            // Store the node
            self.store.save_dag_node(&cid, &node).await
                .map_err(|e| SyncError::CoreError(e))?;
        }
        
        // Update sync state
        let new_epoch = last_epoch + 1; // In a real impl, we would get this from the bundles
        {
            let mut states = self.sync_states.write().await;
            states.insert(federation_id.to_string(), FederationSyncState {
                federation_id: federation_id.to_string(),
                federation_url: federation_url.to_string(),
                last_synced_epoch: new_epoch,
                last_sync_time: SystemTime::now(),
                trust_bundles_count: bundles_count,
                dag_headers_count: headers_count,
            });
        }
        
        Ok(())
    }
    
    /// Fetch the latest trust bundle from the federation with retry logic
    pub async fn fetch_latest_trust_bundle(&self, federation_url: &str) -> SyncResult<TrustBundle> {
        // Check network status first
        let network_status = self.get_network_status().await?;
        if !network_status.online {
            info!("Network is offline, skipping bundle fetching");
            return Err(SyncError::Offline("Network is offline, can't fetch trust bundle".to_string()));
        }

        // Construct the URL for the bundle endpoint
        let url = format!("{}{}", federation_url, DEFAULT_BUNDLE_ENDPOINT);
        debug!("Fetching latest trust bundle from {}", url);
        
        // Define the backoff strategy for retries
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(60)),
            max_interval: Duration::from_secs(10),
            ..ExponentialBackoff::default()
        };
        
        // Create authentication headers with the identity DID
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Identity-DID", 
            self.identity.did.to_string().parse()
                .map_err(|_| SyncError::ProtocolError("Invalid DID for header".to_string()))?);
        
        // Execute the request with retry logic
        let result = retry(backoff, || async {
            let response = self.http_client.get(&url)
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| {
                    let error = SyncError::ConnectionError(format!("Failed to fetch trust bundle: {}", e));
                    // Map network errors to backoff::Error::Transient for retry
                    backoff::Error::Transient { err: error, retry_after: None }
                })?;
                
            // Check status code
            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                let error = SyncError::ProtocolError(format!(
                    "Failed to fetch trust bundle. Status: {}, Error: {}", status, error_text
                ));
                
                // Only retry on server errors (5xx)
                if status.is_server_error() {
                    return Err(backoff::Error::Transient { err: error, retry_after: None });
                } else {
                    return Err(backoff::Error::Permanent(error));
                }
            }
            
            // Parse the response body as JSON
            let bundle: TrustBundle = response.json().await
                .map_err(|e| {
                    let error = SyncError::SerializationError(format!("Failed to parse trust bundle: {}", e));
                    backoff::Error::Permanent(error)
                })?;
                
            Ok(bundle)
        }).await;
        
        match result {
            Ok(bundle) => Ok(bundle),
            Err(backoff::Error::Permanent(e)) => Err(e),
            Err(backoff::Error::Transient { err, .. }) => Err(err),
        }
    }
    
    /// Fetch a specific trust bundle by epoch
    pub async fn fetch_trust_bundle_by_epoch(&self, federation_url: &str, epoch: u64) -> SyncResult<TrustBundle> {
        // Check network status first
        let network_status = self.get_network_status().await?;
        if !network_status.online {
            info!("Network is offline, skipping bundle fetching");
            return Err(SyncError::Offline("Network is offline, can't fetch trust bundle".to_string()));
        }

        // Construct the URL for the bundle endpoint with epoch
        let url = format!("{}/bundles/{}", federation_url, epoch);
        debug!("Fetching trust bundle for epoch {} from {}", epoch, url);
        
        // Define the backoff strategy for retries
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(60)),
            max_interval: Duration::from_secs(10),
            ..ExponentialBackoff::default()
        };
        
        // Create authentication headers with the identity DID
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Identity-DID", 
            self.identity.did.to_string().parse()
                .map_err(|_| SyncError::ProtocolError("Invalid DID for header".to_string()))?);
        
        // Execute the request with retry logic
        let result = retry(backoff, || async {
            let response = self.http_client.get(&url)
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| {
                    let error = SyncError::ConnectionError(format!("Failed to fetch trust bundle: {}", e));
                    // Map network errors to backoff::Error::Transient for retry
                    backoff::Error::Transient { err: error, retry_after: None }
                })?;
                
            // Check status code
            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                let error = match status.as_u16() {
                    404 => SyncError::NotFound(format!("Trust bundle for epoch {} not found", epoch)),
                    _ => SyncError::ProtocolError(format!(
                        "Failed to fetch trust bundle for epoch {}. Status: {}, Error: {}", 
                        epoch, status, error_text
                    )),
                };
                
                // Only retry on server errors (5xx)
                if status.is_server_error() {
                    return Err(backoff::Error::Transient { err: error, retry_after: None });
                } else {
                    return Err(backoff::Error::Permanent(error));
                }
            }
            
            // Parse the response body as JSON
            let bundle: TrustBundle = response.json().await
                .map_err(|e| {
                    let error = SyncError::SerializationError(
                        format!("Failed to parse trust bundle for epoch {}: {}", epoch, e)
                    );
                    backoff::Error::Permanent(error)
                })?;
                
            Ok(bundle)
        }).await;
        
        match result {
            Ok(bundle) => Ok(bundle),
            Err(backoff::Error::Permanent(e)) => Err(e),
            Err(backoff::Error::Transient { err, .. }) => Err(err),
        }
    }
    
    /// Fetch a DAG node by its CID
    pub async fn fetch_dag_node(&self, cid: &str) -> SyncResult<DagNode> {
        // Check network status first
        let network_status = self.get_network_status().await?;
        if !network_status.online {
            info!("Network is offline, skipping DAG node fetching");
            return Err(SyncError::Offline("Network is offline, can't fetch DAG node".to_string()));
        }

        let federation_url = {
            let states = self.sync_states.read().await;
            if let Some(state) = states.values().next() {
                state.federation_url.clone()
            } else {
                self.config.federation_urls[0].clone()
            }
        };

        // Construct the URL for the DAG node endpoint
        let url = format!("{}{}/{}", federation_url, DEFAULT_NODE_ENDPOINT, cid);
        debug!("Fetching DAG node with CID {} from {}", cid, url);
        
        // Define the backoff strategy for retries
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(60)),
            max_interval: Duration::from_secs(10),
            ..ExponentialBackoff::default()
        };
        
        // Create authentication headers with the identity DID
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Identity-DID", 
            self.identity.did.to_string().parse()
                .map_err(|_| SyncError::ProtocolError("Invalid DID for header".to_string()))?);
        
        // Execute the request with retry logic
        let result = retry(backoff, || async {
            let response = self.http_client.get(&url)
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| {
                    let error = SyncError::ConnectionError(format!("Failed to fetch DAG node: {}", e));
                    // Map network errors to backoff::Error::Transient for retry
                    backoff::Error::Transient { err: error, retry_after: None }
                })?;
                
            // Check status code
            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                let error = match status.as_u16() {
                    404 => SyncError::NotFound(format!("DAG node with CID {} not found", cid)),
                    _ => SyncError::ProtocolError(format!(
                        "Failed to fetch DAG node with CID {}. Status: {}, Error: {}", 
                        cid, status, error_text
                    )),
                };
                
                // Only retry on server errors (5xx)
                if status.is_server_error() {
                    return Err(backoff::Error::Transient { err: error, retry_after: None });
                } else {
                    return Err(backoff::Error::Permanent(error));
                }
            }
            
            // Parse the response body as JSON
            let node: DagNode = response.json().await
                .map_err(|e| {
                    let error = SyncError::SerializationError(
                        format!("Failed to parse DAG node with CID {}: {}", cid, e)
                    );
                    backoff::Error::Permanent(error)
                })?;
                
            Ok(node)
        }).await;
        
        let node = match result {
            Ok(node) => node,
            Err(backoff::Error::Permanent(e)) => return Err(e),
            Err(backoff::Error::Transient { err, .. }) => return Err(err),
        };
        
        // Cache the node locally
        self.store.save_dag_node(cid, &node).await
            .map_err(|e| SyncError::CoreError(e))?;
        
        // Update the thread cache if applicable
        self.update_thread_cache_with_node(&node).await?;
        
        // Validate the node
        self.validate_dag_node(&node, Some(cid))?;
        
        Ok(node)
    }
    
    /// Update the thread cache with a new node
    async fn update_thread_cache_with_node(&self, node: &DagNode) -> SyncResult<()> {
        // Attempt to find which thread this node belongs to
        // For this implementation, we'll use a simple heuristic:
        // If the node has a "parent" link, we'll look for a thread containing that parent

        if let Some(parent_cid) = node.links.get("parent") {
            // Find threads containing this parent
            let thread_caches = self.store.list_dag_thread_caches().await
                .map_err(|e| SyncError::CoreError(e))?;
                
            for thread_id in thread_caches {
                match self.store.load_dag_thread_cache(&thread_id).await {
                    Ok(mut cache) => {
                        if cache.node_cids.contains(parent_cid) {
                            // This node belongs to this thread, add it to the cache
                            cache.add_node(&node.links.get("self").unwrap_or(&String::new()).clone());
                            self.store.save_dag_thread_cache(&thread_id, &cache).await
                                .map_err(|e| SyncError::CoreError(e))?;
                            debug!("Added node {} to thread cache {}", node.links.get("self").unwrap_or(&String::new()).clone(), thread_id);
                            return Ok(());
                        }
                    },
                    Err(_) => continue, // Skip errors and try next thread
                }
            }
            
            // If we get here, we couldn't find an existing thread for this node
            // We could create a new thread, but for now we'll just log a warning
            warn!("Could not find a thread for node with CID {} (parent: {})", node.links.get("self").unwrap_or(&String::new()).clone(), parent_cid);
        } else {
            // This might be a root node of a new thread
            // For simplicity, create a new thread for it
            let thread_id = format!("thread-{}", uuid::Uuid::new_v4());
            let thread_type = wallet_core::dag::ThreadType::Custom("unknown".to_string());
            let cache = wallet_core::dag::CachedDagThreadInfo::new(&thread_id, thread_type, &node.links.get("self").unwrap_or(&String::new()).clone());
            
            self.store.save_dag_thread_cache(&thread_id, &cache).await
                .map_err(|e| SyncError::CoreError(e))?;
                
            debug!("Created new thread cache {} for root node {}", thread_id, node.links.get("self").unwrap_or(&String::new()).clone());
        }
        
        Ok(())
    }
    
    /// Fetch information about a DAG thread from the network
    pub async fn fetch_dag_thread_info(&self, thread_id: &str) -> SyncResult<DagThread> {
        // Check network status first
        let network_status = self.get_network_status().await?;
        if !network_status.online {
            info!("Network is offline, skipping thread fetching");
            return Err(SyncError::Offline("Network is offline, can't fetch thread info".to_string()));
        }

        let federation_url = {
            let states = self.sync_states.read().await;
            if let Some(state) = states.values().next() {
                state.federation_url.clone()
            } else {
                self.config.federation_urls[0].clone()
            }
        };

        // Construct the URL for the thread endpoint
        let url = format!("{}{}/{}", federation_url, DEFAULT_THREAD_ENDPOINT, thread_id);
        debug!("Fetching thread info for ID {} from {}", thread_id, url);
        
        // Define the backoff strategy for retries
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(60)),
            max_interval: Duration::from_secs(10),
            ..ExponentialBackoff::default()
        };
        
        // Create authentication headers with the identity DID
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Identity-DID", 
            self.identity.did.to_string().parse()
                .map_err(|_| SyncError::ProtocolError("Invalid DID for header".to_string()))?);
        
        // Execute the request with retry logic
        let result = retry(backoff, || async {
            let response = self.http_client.get(&url)
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| {
                    let error = SyncError::ConnectionError(format!("Failed to fetch thread info: {}", e));
                    // Map network errors to backoff::Error::Transient for retry
                    backoff::Error::Transient { err: error, retry_after: None }
                })?;
                
            // Check status code
            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                let error = match status.as_u16() {
                    404 => SyncError::NotFound(format!("Thread with ID {} not found", thread_id)),
                    _ => SyncError::ProtocolError(format!(
                        "Failed to fetch thread with ID {}. Status: {}, Error: {}", 
                        thread_id, status, error_text
                    )),
                };
                
                // Only retry on server errors (5xx)
                if status.is_server_error() {
                    return Err(backoff::Error::Transient { err: error, retry_after: None });
                } else {
                    return Err(backoff::Error::Permanent(error));
                }
            }
            
            // Parse the response body as JSON
            let thread: DagThread = response.json().await
                .map_err(|e| {
                    let error = SyncError::SerializationError(
                        format!("Failed to parse thread with ID {}: {}", thread_id, e)
                    );
                    backoff::Error::Permanent(error)
                })?;
                
            Ok(thread)
        }).await;
        
        let thread = match result {
            Ok(thread) => thread,
            Err(backoff::Error::Permanent(e)) => return Err(e),
            Err(backoff::Error::Transient { err, .. }) => return Err(err),
        };
        
        // Save the thread to local storage
        self.store.save_dag_thread(thread_id, &thread).await
            .map_err(|e| SyncError::CoreError(e))?;
        
        // Update thread cache with the root and latest nodes
        let mut cache = match self.store.load_dag_thread_cache(thread_id).await {
            Ok(cache) => cache,
            Err(_) => {
                // Create a new cache if it doesn't exist
                wallet_core::dag::CachedDagThreadInfo::new(
                    thread_id, 
                    thread.thread_type.clone(), 
                    &thread.root_cid
                )
            }
        };
        
        // Add the latest CID to the cache if it's not already there
        if !cache.node_cids.contains(&thread.latest_cid) {
            cache.add_node(&thread.latest_cid);
        }
        
        // Save the updated cache
        self.store.save_dag_thread_cache(thread_id, &cache).await
            .map_err(|e| SyncError::CoreError(e))?;
        
        Ok(thread)
    }
    
    /// Sync trust bundles from a federation
    pub async fn sync_trust_bundles(&self, federation_url: &str) -> SyncResult<Vec<TrustBundle>> {
        // Check network status
        let network_status = self.get_network_status().await?;
        if !network_status.online {
            info!("Network is offline, skipping trust bundle sync");
            return Err(SyncError::Offline("Network is offline, can't sync trust bundles".to_string()));
        }
        
        // Fetch the latest trust bundle
        let latest_bundle = self.fetch_latest_trust_bundle(federation_url).await?;
        let latest_epoch = latest_bundle.epoch;
        
        // Perform enhanced validation on the trust bundle
        self.validate_trust_bundle(&latest_bundle)
            .map_err(|e| SyncError::ValidationError(format!("Trust bundle validation failed: {}", e)))?;
        
        // Get current state for this federation
        let federation_id = latest_bundle.id.clone();
        let last_synced_epoch = {
            let states = self.sync_states.read().await;
            states.get(&federation_id)
                .map(|s| s.last_synced_epoch)
                .unwrap_or(0)
        };
        
        // If we're already at the latest epoch, return just the latest bundle
        if last_synced_epoch >= latest_epoch {
            debug!("Already at latest epoch {}, no sync needed", latest_epoch);
            return Ok(vec![latest_bundle]);
        }
        
        // Otherwise, fetch all bundles since the last synced epoch
        let mut bundles = vec![latest_bundle];
        for epoch in (last_synced_epoch + 1)..latest_epoch {
            match self.fetch_trust_bundle_by_epoch(federation_url, epoch).await {
                Ok(bundle) => {
                    // Validate the bundle before adding it
                    match self.validate_trust_bundle(&bundle) {
                        Ok(_) => bundles.push(bundle),
                        Err(e) => {
                            warn!("Trust bundle for epoch {} failed validation: {}", epoch, e);
                            // Skip invalid bundles
                            continue;
                        }
                    }
                },
                Err(SyncError::NotFound(_)) => {
                    // Skip missing epochs
                    warn!("Trust bundle for epoch {} not found", epoch);
                    continue;
                },
                Err(e) => return Err(e),
            }
        }
        
        // Return all fetched and validated bundles
        Ok(bundles)
    }
    
    /// Save a trust bundle to the wallet store
    async fn save_trust_bundle(&self, bundle: &TrustBundle) -> SyncResult<()> {
        // For now, we'll use a simplified storing mechanism
        // In a real implementation, this would use a dedicated method in LocalWalletStore
        let bundle_id = bundle.id.clone();
        let bundle_json = serde_json::to_string(bundle)
            .map_err(|e| SyncError::SerializationError(format!("Failed to serialize bundle: {}", e)))?;
        
        // We'll use a DagNode to store the bundle for now
        let node = DagNode {
            data: serde_json::Value::from(serde_json::to_string(&bundle).unwrap_or_default()),
            links: {
                let mut links = HashMap::new();
                links.insert("self".to_string(), bundle_id.clone());
                links
            },
            signatures: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        
        // Store as a DAG node
        self.store.save_dag_node(&format!("bundle:{}", bundle_id), &node).await
            .map_err(|e| SyncError::CoreError(e))?;
            
        Ok(())
    }
    
    /// Fetch mock DAG headers for testing
    async fn fetch_mock_dag_headers(&self, federation_id: &str, since_epoch: u64) -> SyncResult<Vec<MockDagHeaderData>> {
        // Create mock DAG header data
        let mock_header = MockDagHeaderData::new(
            &format!("bafyreihpcgxa6wjz2cl3mfpxssjcm54chzoj66xtnxekyxuio5h5tsuxsy"),
            vec![],
            since_epoch + 1
        );
        
        Ok(vec![mock_header])
    }
    
    /// Get current sync state for a federation
    pub async fn get_sync_state(&self, federation_id: &str) -> Option<FederationSyncState> {
        let states = self.sync_states.read().await;
        states.get(federation_id).cloned()
    }
    
    /// List stored trust bundles
    pub async fn list_trust_bundles(&self) -> SyncResult<Vec<TrustBundle>> {
        // In a real implementation, this would query the LocalWalletStore
        // For now, we'll just scan for DAG nodes with content_type "trust_bundle"
        
        // Not implemented yet - would need to add a method to list dag nodes by content type
        // For now, return mock data
        let mock_data = MockTrustBundleData::new("default", 1);
        let bundle = mock_data.to_trust_bundle();
        
        Ok(vec![bundle])
    }
    
    /// Get current network status
    pub async fn get_network_status(&self) -> SyncResult<wallet_types::network::NetworkStatus> {
        if self.config.federation_urls.is_empty() {
            return Err(SyncError::ConfigurationError("No federation URLs configured".to_string()));
        }
        
        // Get the current active federation URL
        let active_url = &self.config.federation_urls[0];
        
        // Check connection by pinging the federation health endpoint
        let health_url = format!("{}/health", active_url);
        let is_connected = self.ping_federation(&health_url).await;
        
        // Measure latency if connected
        let primary_node_latency = if is_connected {
            Some(self.measure_latency(&health_url).await)
        } else {
            None
        };
        
        // Get the last successful sync time from sync states
        let last_successful_sync = {
            let states = self.sync_states.read().await;
            states.values()
                .map(|state| state.last_sync_time)
                .max()
        };
        
        // Create the status
        let status = wallet_types::network::NetworkStatus {
            online: is_connected,
            network_type: "federation".to_string(),
            peer_count: 1, // Placeholder
            block_height: 0, // Placeholder
            latency_ms: primary_node_latency.unwrap_or(0),
            sync_percent: 100, // Placeholder
            metadata: {
                let mut map = HashMap::new();
                map.insert("active_url".to_string(), active_url.clone());
                map.insert("pending_submissions".to_string(), self.config.federation_urls.len().to_string());
                if let Some(last_sync) = last_successful_sync {
                    map.insert("last_sync".to_string(), format!("{:?}", last_sync));
                }
                map
            },
        };
        
        Ok(status)
    }
    
    /// Ping federation node to check availability
    async fn ping_federation(&self, health_url: &str) -> bool {
        debug!("Pinging federation node at {}", health_url);
        
        match self.http_client.get(health_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
    
    /// Measure latency to federation node
    async fn measure_latency(&self, url: &str) -> u64 {
        let start = std::time::Instant::now();
        
        let _ = self.http_client.get(url)
            .timeout(Duration::from_secs(5))
            .send()
            .await;
            
        start.elapsed().as_millis() as u64
    }
    
    /// Submit multiple DAG nodes in a batch
    pub async fn submit_dag_nodes_batch(&self, nodes: &[DagNode]) -> SyncResult<Vec<NodeSubmissionResponse>> {
        // Check network status first
        let network_status = self.get_network_status().await?;
        if !network_status.online {
            info!("Network is offline, queueing DAG nodes batch submission for later");
            // In a real implementation, we would queue the nodes for later submission
            
            // Create a response for each node to indicate they are queued
            return Ok(nodes.iter().map(|_| NodeSubmissionResponse {
                success: false,
                id: String::new(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                block_number: None,
                error: Some("Network is offline, submission queued for later".to_string()),
                data: HashMap::new(),
            }).collect());
        }

        // Use the first federation URL by default
        if self.config.federation_urls.is_empty() {
            return Err(SyncError::ConfigurationError("No federation URLs configured".to_string()));
        }
        
        // Use the first federation URL
        let federation_url = &self.config.federation_urls[0];
        let url = format!("{}{}/batch", federation_url, DEFAULT_NODE_ENDPOINT);
        debug!("Batch submitting {} DAG nodes to {}", nodes.len(), url);
        
        // Define the backoff strategy for retries
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(60)),
            max_interval: Duration::from_secs(10),
            ..ExponentialBackoff::default()
        };
        
        // Create authentication headers with the identity DID
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Identity-DID", 
            self.identity.did.to_string().parse()
                .map_err(|_| SyncError::ProtocolError("Invalid DID for header".to_string()))?);
        
        // Execute the request with retry logic
        let result = retry(backoff, || async {
            let response = self.http_client.post(&url)
                .headers(headers.clone())
                .json(nodes)
                .send()
                .await
                .map_err(|e| {
                    let error = SyncError::ConnectionError(format!("Failed to batch submit DAG nodes: {}", e));
                    backoff::Error::Transient { err: error, retry_after: None }
                })?;
                
            // Check status code
            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                let error = SyncError::ProtocolError(format!(
                    "Failed to batch submit DAG nodes. Status: {}, Error: {}", status, error_text
                ));
                
                // Only retry on server errors (5xx)
                if status.is_server_error() {
                    return Err(backoff::Error::Transient { err: error, retry_after: None });
                } else {
                    return Err(backoff::Error::Permanent(error));
                }
            }
            
            // Parse the response body as JSON
            let submission_responses: Vec<NodeSubmissionResponse> = response.json().await
                .map_err(|e| {
                    let error = SyncError::SerializationError(format!(
                        "Failed to parse batch submission responses: {}", e
                    ));
                    backoff::Error::Permanent(error)
                })?;
                
            Ok(submission_responses)
        }).await;
        
        match result {
            Ok(responses) => Ok(responses),
            Err(backoff::Error::Permanent(e)) => Err(e),
            Err(backoff::Error::Transient { err, .. }) => Err(err),
        }
    }
    
    /// Submit a DAG node to the federation
    pub async fn submit_dag_node(&self, node: &DagNode) -> SyncResult<NodeSubmissionResponse> {
        // Check network status first
        let network_status = self.get_network_status().await?;
        if !network_status.online {
            info!("Network is offline, queueing DAG node submission for later");
            // In a real implementation, we would queue the node for later submission
            return Ok(NodeSubmissionResponse {
                success: false,
                id: String::new(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                block_number: None,
                error: Some("Network is offline, submission queued for later".to_string()),
                data: HashMap::new(),
            });
        }

        let federation_url = {
            let states = self.sync_states.read().await;
            if let Some(state) = states.values().next() {
                state.federation_url.clone()
            } else if !self.config.federation_urls.is_empty() {
                self.config.federation_urls[0].clone()
            } else {
                return Err(SyncError::ConfigurationError("No federation URLs configured".to_string()));
            }
        };

        // Construct the URL for the node endpoint
        let url = format!("{}{}", federation_url, DEFAULT_NODE_ENDPOINT);
        debug!("Submitting DAG node to {}", url);
        
        // Define the backoff strategy for retries
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(60)),
            max_interval: Duration::from_secs(10),
            ..ExponentialBackoff::default()
        };
        
        // Create authentication headers with the identity DID
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Identity-DID", 
            self.identity.did.to_string().parse()
                .map_err(|_| SyncError::ProtocolError("Invalid DID for header".to_string()))?);
        
        // Execute the request with retry logic
        let result = retry(backoff, || async {
            let response = self.http_client.post(&url)
                .headers(headers.clone())
                .json(node)
                .send()
                .await
                .map_err(|e| {
                    let error = SyncError::ConnectionError(format!("Failed to submit DAG node: {}", e));
                    // Map network errors to backoff::Error::Transient for retry
                    backoff::Error::Transient { err: error, retry_after: None }
                })?;
                
            // Check status code
            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                let error = SyncError::SubmissionError(format!(
                    "Failed to submit DAG node. Status: {}, Error: {}", status, error_text
                ));
                
                // Only retry on server errors (5xx)
                if status.is_server_error() {
                    return Err(backoff::Error::Transient { err: error, retry_after: None });
                } else {
                    return Err(backoff::Error::Permanent(error));
                }
            }
            
            // Parse the response body as JSON
            let submission_response: NodeSubmissionResponse = response.json().await
                .map_err(|e| {
                    let error = SyncError::SerializationError(format!("Failed to parse submission response: {}", e));
                    backoff::Error::Permanent(error)
                })?;
                
            Ok(submission_response)
        }).await;
        
        let response = match result {
            Ok(response) => response,
            Err(backoff::Error::Permanent(e)) => return Err(e),
            Err(backoff::Error::Transient { err, .. }) => return Err(err),
        };
        
        // If submission was successful and we got a CID, update local cache
        if response.success && !response.id.is_empty() {
            let cid = &response.id;
            
            // Save the node with the assigned CID
            let mut node_copy = node.clone();
            node_copy.links.insert("self".to_string(), cid.clone());
            self.store.save_dag_node(cid, &node_copy).await
                .map_err(|e| SyncError::CoreError(e))?;
                
            // Update thread cache if applicable
            self.update_thread_cache_with_node(&node_copy).await?;
        }
        
        Ok(response)
    }
    
    /// Get cached DAG thread information
    pub async fn get_dag_thread_cache(&self, thread_id: &str) -> SyncResult<wallet_core::dag::CachedDagThreadInfo> {
        self.store.load_dag_thread_cache(thread_id).await
            .map_err(|e| SyncError::CoreError(e))
    }
    
    /// List all cached DAG threads
    pub async fn list_dag_thread_caches(&self) -> SyncResult<Vec<String>> {
        self.store.list_dag_thread_caches().await
            .map_err(|e| SyncError::CoreError(e))
    }
    
    /// Get a summary of a cached DAG thread
    pub async fn get_dag_thread_summary(&self, thread_id: &str) -> SyncResult<HashMap<String, String>> {
        let cache = self.get_dag_thread_cache(thread_id).await?;
        
        let mut summary = HashMap::new();
        summary.insert("thread_id".to_string(), thread_id.to_string());
        summary.insert("node_count".to_string(), cache.node_cids.len().to_string());
        summary.insert("head_cid".to_string(), cache.head_cid.clone());
        summary.insert("tail_cid".to_string(), cache.tail_cid.clone());
        summary.insert("last_updated".to_string(), 
            cache.last_updated.to_rfc3339());
            
        // Add thread type
        match cache.thread_type {
            wallet_core::dag::ThreadType::Proposal => {
                summary.insert("thread_type".to_string(), "Proposal".to_string());
            },
            wallet_core::dag::ThreadType::Vote => {
                summary.insert("thread_type".to_string(), "Vote".to_string());
            },
            wallet_core::dag::ThreadType::Anchor => {
                summary.insert("thread_type".to_string(), "Anchor".to_string());
            },
            wallet_core::dag::ThreadType::Custom(ref custom_type) => {
                summary.insert("thread_type".to_string(), format!("Custom: {}", custom_type));
            },
        }
        
        // Add any available metadata
        for (key, value) in cache.metadata {
            summary.insert(format!("metadata_{}", key), value);
        }
        
        Ok(summary)
    }
    
    /// Fetch and cache all nodes for a DAG thread
    pub async fn fetch_complete_dag_thread(&self, thread_id: &str) -> SyncResult<()> {
        // Get the thread info to find root and latest CIDs
        let thread = self.fetch_dag_thread_info(thread_id).await?;
        
        // Get current cache
        let cache = match self.get_dag_thread_cache(thread_id).await {
            Ok(cache) => cache,
            Err(_) => {
                // Create new cache if it doesn't exist
                let mut cache = wallet_core::dag::CachedDagThreadInfo::new(
                    thread_id, 
                    thread.thread_type.clone(), 
                    &thread.root_cid
                );
                
                if thread.latest_cid != thread.root_cid {
                    cache.add_node(&thread.latest_cid);
                }
                
                cache
            }
        };
        
        // Track which nodes we still need to fetch
        let mut to_fetch = Vec::new();
        let mut fetched = HashSet::new();
        
        // Start with root and latest nodes
        to_fetch.push(thread.root_cid.clone());
        if thread.latest_cid != thread.root_cid {
            to_fetch.push(thread.latest_cid.clone());
        }
        
        // Continue fetching nodes until we have all of them
        while !to_fetch.is_empty() {
            let cid = to_fetch.pop().unwrap();
            
            // Skip if already fetched
            if fetched.contains(&cid) {
                continue;
            }
            
            // Fetch the node
            let node = self.fetch_dag_node(&cid).await?;
            fetched.insert(cid.clone());
            
            // Add parents to the list of nodes to fetch
            for parent_cid in node.links.values() {
                if !fetched.contains(parent_cid) {
                    to_fetch.push(parent_cid.clone());
                }
            }
        }
        
        debug!("Fetched {} nodes for thread {}", fetched.len(), thread_id);
        
        Ok(())
    }

    // Add a new validation method for trust bundles
    fn validate_trust_bundle(&self, bundle: &TrustBundle) -> SyncResult<()> {
        // Basic validation using the trust validator
        self.trust_validator.validate_bundle(bundle)?;

        // Enhanced validations:
        
        // 1. Check timestamp is not in the future
        let now_plus_1min = chrono::Utc::now() + chrono::Duration::seconds(60);
        if bundle.created_at > now_plus_1min {  // Allow 1 minute clock skew
            return Err(SyncError::ValidationError(
                format!("Trust bundle has a future timestamp: {:?}", bundle.created_at)
            ));
        }
        
        // 2. Check threshold is reasonable based on guardian count
        if bundle.threshold < 1 || bundle.threshold as usize > bundle.guardians.len() {
            return Err(SyncError::ValidationError(
                format!("Invalid threshold: {} for {} guardians", bundle.threshold, bundle.guardians.len())
            ));
        }
        
        // 3. Check expiration if set
        if let Some(expires_at) = bundle.valid_until {
            let now = std::time::SystemTime::now();
            if expires_at < now {
                return Err(SyncError::ValidationError(
                    format!("Trust bundle has expired at {:?}", expires_at)
                ));
            }
        }
        
        // 4. Check signature validation
        // In a real implementation, we'd verify signatures from enough guardians to meet threshold
        
        Ok(())
    }

    // Add a validation method for DAG nodes
    fn validate_dag_node(&self, node: &DagNode, expected_cid: Option<&str>) -> SyncResult<()> {
        // 1. Basic structural validation
        if node.links.is_empty() {
            return Err(SyncError::ValidationError("DAG node missing CID".to_string()));
        }
        
        // 2. Verify CID if provided
        if let Some(cid) = expected_cid {
            if node.links.get("self").unwrap_or(&String::new()) != cid {
                return Err(SyncError::ValidationError(
                    format!("CID mismatch: expected {}, got {}", cid, node.links.get("self").unwrap_or(&String::new()).clone())
                ));
            }
        }
        
        // 3. Check timestamp is reasonable
        // Check if created_at is in the future (with a 1 minute allowance for clock skew)
        let now_plus_1min = chrono::Utc::now() + chrono::Duration::seconds(60);
        if node.created_at > now_plus_1min {
            return Err(SyncError::ValidationError(
                format!("DAG node has a future timestamp: {:?}", node.created_at)
            ));
        }
        
        // 4. Check parent references (must be valid CIDs)
        for parent in node.links.values() {
            if parent.is_empty() {
                return Err(SyncError::ValidationError("Empty parent CID reference".to_string()));
            }
            
            // Additional CID format validation could be done here
        }
        
        // 5. Check signature validation
        if node.signatures.is_empty() {
            return Err(SyncError::ValidationError("DAG node has no signatures".to_string()));
        }
        
        // 6. Verify creator exists and is authorized (in real impl)
        if node.links.get("creator").unwrap_or(&String::new()).is_empty() {
            return Err(SyncError::ValidationError("DAG node missing creator".to_string()));
        }
        
        // Additional checks could be implemented based on node type
        
        Ok(())
    }
} 