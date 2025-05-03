use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{Mutex, RwLock};
use wallet_core::identity::IdentityWallet;
use crate::error::{SyncResult, SyncError};
use crate::trust::TrustBundleValidator;
use crate::dag::{DagObject, DagVerifier};
use cid::Cid;
use wallet_types::TrustBundle;
use reqwest::Client as HttpClient;
use backoff::{ExponentialBackoff, Error as BackoffError};
use tracing::{info, warn, error, debug, trace};
use std::sync::Arc;

const DEFAULT_SYNC_SERVERS: [&str; 2] = [
    "https://icn-federation.example.com/api",
    "https://backup-icn.example.org/api",
];

const DEFAULT_SYNC_RETRY_ATTEMPTS: u32 = 3;
const DEFAULT_SYNC_MAX_DELAY_SECONDS: u64 = 60;

/// Sync frequency policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncFrequency {
    /// Sync every N seconds
    Seconds(u64),
    /// Sync on application startup only
    OnStartup,
    /// Only sync when explicitly requested
    Manual,
}

/// Conflict resolution policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictPolicy {
    /// Choose the version with the highest version number
    HighestVersion,
    /// Choose the most recently received version
    MostRecent,
    /// Choose the version from the primary server
    PreferPrimary,
    /// Require manual resolution
    AskUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub servers: Vec<String>,
    pub local_storage: PathBuf,
    pub sync_frequency: SyncFrequency,
    pub auto_sync: bool,
    pub conflict_policy: ConflictPolicy,
    pub max_retry_attempts: u32,
    pub max_retry_delay_seconds: u64,
    pub cache_ttl_seconds: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            servers: DEFAULT_SYNC_SERVERS.iter().map(|s| s.to_string()).collect(),
            local_storage: PathBuf::from("./storage/sync"),
            sync_frequency: SyncFrequency::Seconds(3600), // 1 hour
            auto_sync: true,
            conflict_policy: ConflictPolicy::HighestVersion,
            max_retry_attempts: DEFAULT_SYNC_RETRY_ATTEMPTS,
            max_retry_delay_seconds: DEFAULT_SYNC_MAX_DELAY_SECONDS,
            cache_ttl_seconds: 300, // 5 minutes
        }
    }
}

/// Bundle metadata tracks important sync information
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleMetadata {
    /// Source server from which this bundle was fetched
    source_server: String,
    /// When the bundle was last fetched/updated
    last_updated: SystemTime,
    /// The raw bundle content hash (helps detect actual changes)
    content_hash: String,
}

/// Cache entry with expiration
struct CacheEntry<T> {
    data: T,
    expires_at: Instant,
}

pub struct SyncClient {
    config: Arc<SyncConfig>,
    identity: IdentityWallet,
    http_client: HttpClient,
    trust_validator: TrustBundleValidator,
    dag_verifier: DagVerifier,
    
    /// Cached bundles with access control
    cached_bundles: Arc<RwLock<HashMap<String, TrustBundle>>>,
    
    /// Metadata for bundles to track provenance and timestamps
    bundle_metadata: Arc<RwLock<HashMap<String, BundleMetadata>>>,
    
    /// Tracks the last sync time for incremental syncs
    last_sync_time: Arc<RwLock<Option<SystemTime>>>,
    
    /// Lock for sync operations to prevent concurrent syncs
    sync_lock: Arc<Mutex<()>>,
    
    /// Cache of DAG objects
    dag_cache: Arc<RwLock<HashMap<String, CacheEntry<DagObject>>>>,
}

impl SyncClient {
    pub fn new(identity: IdentityWallet, config: Option<SyncConfig>) -> SyncResult<Self> {
        let config = Arc::new(config.unwrap_or_default());
        
        // Ensure storage directory exists
        fs::create_dir_all(&config.local_storage)
            .map_err(|e| SyncError::IoError(format!("Failed to create storage directory: {}", e)))?;
            
        let trust_validator = TrustBundleValidator::new(identity.clone());
        let dag_verifier = DagVerifier::new();
        
        Ok(Self {
            config,
            identity,
            http_client: HttpClient::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            trust_validator,
            dag_verifier,
            cached_bundles: Arc::new(RwLock::new(HashMap::new())),
            bundle_metadata: Arc::new(RwLock::new(HashMap::new())),
            last_sync_time: Arc::new(RwLock::new(None)),
            sync_lock: Arc::new(Mutex::new(())),
            dag_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Loads all trust bundles from disk
    pub async fn load_trust_bundles_from_disk(&self) -> SyncResult<Vec<TrustBundle>> {
        let bundle_dir = self.config.local_storage.join("bundles");
        
        if !bundle_dir.exists() {
            fs::create_dir_all(&bundle_dir)
                .map_err(|e| SyncError::IoError(format!("Failed to create bundles directory: {}", e)))?;
            return Ok(Vec::new());
        }
        
        let entries = fs::read_dir(&bundle_dir)
            .map_err(|e| SyncError::IoError(format!("Failed to read bundles directory: {}", e)))?;
            
        let mut loaded_bundles = Vec::new();
        let mut metadata = self.bundle_metadata.write().await;
        let mut cached = self.cached_bundles.write().await;
        
        for entry in entries {
            let entry = entry
                .map_err(|e| SyncError::IoError(format!("Failed to read directory entry: {}", e)))?;
                
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                match Self::read_json_file::<TrustBundle>(&path) {
                    Ok(bundle) => {
                        // Calculate hash of the bundle content
                        let content = serde_json::to_string(&bundle)
                            .map_err(|e| SyncError::SerializationError(format!("Failed to serialize bundle: {}", e)))?;
                        let content_hash = format!("{:x}", md5::compute(&content));
                        
                        if let Err(e) = self.trust_validator.validate_bundle(&bundle) {
                            warn!("Invalid trust bundle loaded from disk: {}. Error: {}", bundle.id, e);
                            continue;
                        }
                        
                        // Look for metadata file
                        let metadata_path = path.with_extension("metadata.json");
                        let bundle_metadata = if metadata_path.exists() {
                            match Self::read_json_file::<BundleMetadata>(&metadata_path) {
                                Ok(meta) => meta,
                                Err(e) => {
                                    warn!("Failed to read bundle metadata: {}", e);
                                    // Create default metadata
                                    BundleMetadata {
                                        source_server: "local".to_string(),
                                        last_updated: SystemTime::now(),
                                        content_hash,
                                    }
                                }
                            }
                        } else {
                            // Create default metadata
                            BundleMetadata {
                                source_server: "local".to_string(),
                                last_updated: SystemTime::now(),
                                content_hash,
                            }
                        };
                        
                        // Store in caches
                        metadata.insert(bundle.id.clone(), bundle_metadata);
                        cached.insert(bundle.id.clone(), bundle.clone());
                        loaded_bundles.push(bundle);
                    },
                    Err(e) => {
                        error!("Failed to read trust bundle file {}: {}", path.display(), e);
                    }
                }
            }
        }
        
        info!("Loaded {} trust bundles from disk", loaded_bundles.len());
        Ok(loaded_bundles)
    }
    
    /// Helper to read and parse a JSON file
    fn read_json_file<T: for<'de> serde::Deserialize<'de>>(path: &PathBuf) -> SyncResult<T> {
        let mut file = File::open(path)
            .map_err(|e| SyncError::IoError(format!("Failed to open file {}: {}", path.display(), e)))?;
            
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| SyncError::IoError(format!("Failed to read file {}: {}", path.display(), e)))?;
            
        serde_json::from_str(&content)
            .map_err(|e| SyncError::SerializationError(format!("Failed to parse JSON from {}: {}", path.display(), e)))
    }
    
    /// Synchronizes trust bundles from the federation, with retry logic and conflict resolution
    pub async fn sync_trust_bundles(&self) -> SyncResult<Vec<TrustBundle>> {
        // Acquire lock to prevent concurrent sync operations
        let _lock = self.sync_lock.lock().await;
        info!("Starting trust bundle synchronization");
        
        // Load existing bundles if cache is empty
        {
            let cached = self.cached_bundles.read().await;
            if cached.is_empty() {
                drop(cached); // Release read lock before loading
                self.load_trust_bundles_from_disk().await?;
            }
        }
        
        let mut results = Vec::new();
        let mut errors = Vec::new();
        
        // Record last successful sync time for incremental updates
        let sync_start_time = SystemTime::now();
        
        // Get current state for conflict resolution
        let current_bundles = {
            let bundles = self.cached_bundles.read().await;
            bundles.clone()
        };
        
        let current_metadata = {
            let metadata = self.bundle_metadata.read().await;
            metadata.clone()
        };
        
        // Try each server until successful
        for (server_index, server_url) in self.config.servers.iter().enumerate() {
            let is_primary = server_index == 0;
            
            debug!("Syncing trust bundles from server: {}", server_url);
            
            match self.fetch_trust_bundles_from_server(server_url, is_primary).await {
                Ok(bundles) => {
                    debug!("Received {} trust bundles from {}", bundles.len(), server_url);
                    
                    for bundle in bundles {
                        match self.process_received_bundle(
                            bundle, 
                            server_url,
                            is_primary,
                            &current_bundles,
                            &current_metadata
                        ).await {
                            Ok(processed) => {
                                results.push(processed);
                            },
                            Err(e) => {
                                error!("Failed to process bundle: {}", e);
                                errors.push(e);
                            }
                        }
                    }
                    
                    // If we successfully processed from any server, update last sync time
                    // Continue to try other servers even after success
                    if !results.is_empty() {
                        let mut last_sync = self.last_sync_time.write().await;
                        *last_sync = Some(sync_start_time);
                    }
                },
                Err(e) => {
                    error!("Failed to sync from {}: {}", server_url, e);
                    errors.push(e);
                    // Continue with next server
                }
            }
        }
        
        // If we got results from any server, consider the sync successful
        if !results.is_empty() {
            info!("Successfully synced {} trust bundles", results.len());
            return Ok(results);
        }
        
        // If we're here, all servers failed
        if let Some(first_error) = errors.first() {
            Err(SyncError::ConnectionError(format!("All federation servers failed. First error: {}", first_error)))
        } else {
            // This should never happen if errors is populated correctly
            Err(SyncError::ConnectionError("All federation servers failed with unknown errors".to_string()))
        }
    }
    
    /// Process a received bundle, handling conflicts and validation
    async fn process_received_bundle(
        &self,
        bundle: TrustBundle,
        source_server: &str,
        is_primary: bool,
        current_bundles: &HashMap<String, TrustBundle>,
        current_metadata: &HashMap<String, BundleMetadata>,
    ) -> SyncResult<TrustBundle> {
        // First, validate the bundle
        self.trust_validator.validate_bundle(&bundle)?;
        
        // Generate hash for change detection
        let content = serde_json::to_string(&bundle)
            .map_err(|e| SyncError::SerializationError(format!("Failed to serialize bundle: {}", e)))?;
        let content_hash = format!("{:x}", md5::compute(&content));
        
        // Check if it already exists and handle conflict if needed
        let needs_update = if let Some(existing) = current_bundles.get(&bundle.id) {
            // Bundle already exists, check if it's different
            if let Some(existing_meta) = current_metadata.get(&bundle.id) {
                // If content hash is the same, no need to update
                if existing_meta.content_hash == content_hash {
                    debug!("Bundle {} unchanged, skipping update", bundle.id);
                    false // No update needed
                } else {
                    debug!("Bundle {} has changed, resolving conflict", bundle.id);
                    // Resolve conflict based on policy
                    match self.config.conflict_policy {
                        ConflictPolicy::HighestVersion => {
                            bundle.version > existing.version
                        },
                        ConflictPolicy::MostRecent => {
                            // Always update with most recently received
                            true
                        },
                        ConflictPolicy::PreferPrimary => {
                            // Update if from primary server
                            is_primary
                        },
                        ConflictPolicy::AskUser => {
                            // In a real implementation, this would prompt the user
                            // For now, default to highest version
                            bundle.version > existing.version
                        },
                    }
                }
            } else {
                // We have the bundle but no metadata, assume update needed
                true
            }
        } else {
            // Brand new bundle, always store
            true
        };
        
        if needs_update {
            // Create metadata
            let bundle_metadata = BundleMetadata {
                source_server: source_server.to_string(),
                last_updated: SystemTime::now(),
                content_hash,
            };
            
            // Store updated bundle and metadata
            self.save_bundle_to_disk(&bundle, Some(&bundle_metadata))?;
            
            // Update caches
            {
                let mut cached = self.cached_bundles.write().await;
                cached.insert(bundle.id.clone(), bundle.clone());
            }
            {
                let mut metadata = self.bundle_metadata.write().await;
                metadata.insert(bundle.id.clone(), bundle_metadata);
            }
            
            debug!("Updated bundle {}", bundle.id);
        }
        
        Ok(bundle)
    }
    
    /// Fetch trust bundles from a federation server with retry logic
    async fn fetch_trust_bundles_from_server(&self, server_url: &str, is_primary: bool) -> SyncResult<Vec<TrustBundle>> {
        let url = format!("{}/trust-bundles", server_url);
        
        // Include last sync time for incremental updates if available
        let mut query_params = HashMap::new();
        
        // Only request incremental updates from primary server to avoid conflicts
        if is_primary {
            if let Some(last_sync) = *self.last_sync_time.read().await {
                // Format timestamp as ISO 8601 / RFC 3339
                if let Ok(last_sync_str) = chrono::DateTime::<chrono::Utc>::from(last_sync).to_rfc3339().parse::<String>() {
                    query_params.insert("since", last_sync_str);
                }
            }
        }
        
        // Configure retry logic
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(self.config.max_retry_delay_seconds)),
            max_interval: Duration::from_secs(10),
            max_retries: Some(self.config.max_retry_attempts),
            ..Default::default()
        };
        
        let result = backoff::future::retry(backoff, || async {
            match self.http_client.get(&url)
                .query(&query_params)
                .header("Authorization", format!("DID {}", self.identity.did))
                .send().await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<Vec<TrustBundle>>().await {
                            Ok(bundles) => Ok(bundles),
                            Err(e) => {
                                error!("Failed to parse trust bundles from {}: {}", server_url, e);
                                Err(BackoffError::Permanent(SyncError::SerializationError(
                                    format!("Failed to parse trust bundles: {}", e)
                                )))
                            }
                        }
                    } else {
                        let status = response.status();
                        let error_text = match response.text().await {
                            Ok(text) => text,
                            Err(_) => "[Could not read error body]".to_string(),
                        };
                        
                        let error = SyncError::ProtocolError(format!(
                            "Server returned error: {} - {}", status, error_text
                        ));
                        
                        // Only retry on server errors or too many requests
                        if status.as_u16() >= 500 || status.as_u16() == 429 {
                            warn!("Retryable error from {}: {}", server_url, error);
                            Err(BackoffError::Transient(error))
                        } else {
                            error!("Non-retryable error from {}: {}", server_url, error);
                            Err(BackoffError::Permanent(error))
                        }
                    }
                },
                Err(e) => {
                    if e.is_timeout() || e.is_connect() {
                        warn!("Network error fetching trust bundles from {}, will retry: {}", server_url, e);
                        Err(BackoffError::Transient(SyncError::ConnectionError(
                            format!("Network error: {}", e)
                        )))
                    } else {
                        error!("Request error fetching trust bundles from {}: {}", server_url, e);
                        Err(BackoffError::Permanent(SyncError::ConnectionError(
                            format!("Request error: {}", e)
                        )))
                    }
                }
            }
        }).await;
        
        match result {
            Ok(bundles) => Ok(bundles),
            Err(e) => match e {
                BackoffError::Permanent(e) | BackoffError::Transient(e) => Err(e),
            },
        }
    }
    
    /// Fetch a DAG object by CID with caching and retry logic
    pub async fn fetch_dag_object(&self, cid: &str) -> SyncResult<DagObject> {
        // Validate CID format
        Cid::try_from(cid)
            .map_err(|e| SyncError::CidError(format!("Invalid CID format: {}", e)))?;
            
        // Check cache first
        {
            let cache = self.dag_cache.read().await;
            if let Some(entry) = cache.get(cid) {
                if Instant::now() < entry.expires_at {
                    trace!("DAG object cache hit for CID: {}", cid);
                    return Ok(entry.data.clone());
                }
            }
        }
        
        // Check if we have it cached locally on disk
        let local_path = self.get_dag_object_path(cid);
        if local_path.exists() {
            match self.load_dag_object_from_disk(cid) {
                Ok(obj) => {
                    // Add to memory cache
                    let mut cache = self.dag_cache.write().await;
                    cache.insert(cid.to_string(), CacheEntry {
                        data: obj.clone(),
                        expires_at: Instant::now() + Duration::from_secs(self.config.cache_ttl_seconds),
                    });
                    return Ok(obj);
                },
                Err(e) => {
                    warn!("Failed to load DAG object from disk: {}", e);
                    // Continue to fetch from network
                }
            }
        }
        
        // Try fetching from remote servers with retry logic
        for server_url in &self.config.servers {
            let url = format!("{}/dag/{}", server_url, cid);
            
            let backoff = ExponentialBackoff {
                max_elapsed_time: Some(Duration::from_secs(30)),
                max_interval: Duration::from_secs(5),
                max_retries: Some(self.config.max_retry_attempts),
                ..Default::default()
            };
            
            let result = backoff::future::retry(backoff, || async {
                match self.http_client.get(&url)
                    .header("Authorization", format!("DID {}", self.identity.did))
                    .send().await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            match response.bytes().await {
                                Ok(bytes) => {
                                    match serde_json::from_slice::<DagObject>(&bytes) {
                                        Ok(obj) => Ok(obj),
                                        Err(e) => Err(BackoffError::Permanent(SyncError::SerializationError(
                                            format!("Invalid DAG object: {}", e)
                                        ))),
                                    }
                                },
                                Err(e) => Err(BackoffError::Transient(SyncError::HttpError(
                                    format!("Failed to read response: {}", e)
                                ))),
                            }
                        } else if response.status().as_u16() == 404 {
                            // Object not found, try next server
                            Err(BackoffError::Permanent(SyncError::ResourceNotFound(
                                format!("DAG object not found: {}", cid)
                            )))
                        } else {
                            // Server error, might be retryable
                            let status = response.status();
                            Err(if status.as_u16() >= 500 || status.as_u16() == 429 {
                                BackoffError::Transient(SyncError::ProtocolError(
                                    format!("Server error: {}", status)
                                ))
                            } else {
                                BackoffError::Permanent(SyncError::ProtocolError(
                                    format!("Server returned error: {}", status)
                                ))
                            })
                        }
                    },
                    Err(e) => {
                        if e.is_timeout() || e.is_connect() {
                            Err(BackoffError::Transient(SyncError::ConnectionError(
                                format!("Network error: {}", e)
                            )))
                        } else {
                            Err(BackoffError::Permanent(SyncError::ConnectionError(
                                format!("Request error: {}", e)
                            )))
                        }
                    }
                }
            }).await;
            
            match result {
                Ok(obj) => {
                    // Verify the object
                    if self.dag_verifier.verify_object(&obj, cid)? {
                        // Save to disk
                        self.save_dag_object_to_disk(&obj, cid)?;
                        
                        // Add to memory cache
                        let mut cache = self.dag_cache.write().await;
                        cache.insert(cid.to_string(), CacheEntry {
                            data: obj.clone(),
                            expires_at: Instant::now() + Duration::from_secs(self.config.cache_ttl_seconds),
                        });
                        
                        return Ok(obj);
                    } else {
                        return Err(SyncError::VerificationError(
                            format!("DAG object verification failed: {}", cid)
                        ));
                    }
                },
                Err(BackoffError::Permanent(SyncError::ResourceNotFound(_))) => {
                    // Not found on this server, try next server
                    continue;
                },
                Err(e) => {
                    match e {
                        BackoffError::Permanent(e) | BackoffError::Transient(e) => {
                            warn!("Failed to fetch DAG object from {}: {}", server_url, e);
                            // Try next server
                        }
                    }
                }
            }
        }
        
        // If we're here, all servers failed
        Err(SyncError::ConnectionError(format!("Failed to fetch DAG object from all servers: {}", cid)))
    }
    
    /// Verify if a DID has a valid guardian mandate
    pub async fn verify_guardian_mandate(&self, did: &str) -> SyncResult<bool> {
        // In a real implementation, this would fetch the Guardian Mandate proof
        // from the DAG and verify it cryptographically
        
        // For this implementation, we'll check if the DID is in any active trust bundle
        let cached = self.cached_bundles.read().await;
        
        for bundle in cached.values() {
            if bundle.active && bundle.guardians.contains(&did.to_string()) {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    // Helper methods
    fn get_bundle_path(&self, bundle_id: &str) -> PathBuf {
        self.config.local_storage.join("bundles").join(format!("{}.json", bundle_id))
    }
    
    fn get_bundle_metadata_path(&self, bundle_id: &str) -> PathBuf {
        self.config.local_storage.join("bundles").join(format!("{}.metadata.json", bundle_id))
    }
    
    fn get_dag_object_path(&self, cid: &str) -> PathBuf {
        self.config.local_storage.join("dag").join(format!("{}.json", cid))
    }
    
    fn save_bundle_to_disk(&self, bundle: &TrustBundle, metadata: Option<&BundleMetadata>) -> SyncResult<()> {
        let bundle_dir = self.config.local_storage.join("bundles");
        fs::create_dir_all(&bundle_dir)
            .map_err(|e| SyncError::IoError(format!("Failed to create bundles directory: {}", e)))?;
            
        let path = self.get_bundle_path(&bundle.id);
        
        let content = serde_json::to_string_pretty(bundle)
            .map_err(|e| SyncError::SerializationError(format!("Failed to serialize bundle: {}", e)))?;
            
        let mut file = File::create(&path)
            .map_err(|e| SyncError::IoError(format!("Failed to create bundle file: {}", e)))?;
            
        file.write_all(content.as_bytes())
            .map_err(|e| SyncError::IoError(format!("Failed to write bundle file: {}", e)))?;
            
        // Save metadata if provided
        if let Some(meta) = metadata {
            let meta_path = self.get_bundle_metadata_path(&bundle.id);
            
            let meta_content = serde_json::to_string_pretty(meta)
                .map_err(|e| SyncError::SerializationError(format!("Failed to serialize metadata: {}", e)))?;
                
            let mut meta_file = File::create(&meta_path)
                .map_err(|e| SyncError::IoError(format!("Failed to create metadata file: {}", e)))?;
                
            meta_file.write_all(meta_content.as_bytes())
                .map_err(|e| SyncError::IoError(format!("Failed to write metadata file: {}", e)))?;
        }
        
        Ok(())
    }
    
    fn save_dag_object_to_disk(&self, obj: &DagObject, cid: &str) -> SyncResult<()> {
        let dag_dir = self.config.local_storage.join("dag");
        fs::create_dir_all(&dag_dir)
            .map_err(|e| SyncError::IoError(format!("Failed to create DAG directory: {}", e)))?;
            
        let path = self.get_dag_object_path(cid);
        
        let content = serde_json::to_string_pretty(obj)
            .map_err(|e| SyncError::SerializationError(format!("Failed to serialize DAG object: {}", e)))?;
            
        let mut file = File::create(path)
            .map_err(|e| SyncError::IoError(format!("Failed to create DAG file: {}", e)))?;
            
        file.write_all(content.as_bytes())
            .map_err(|e| SyncError::IoError(format!("Failed to write DAG file: {}", e)))?;
            
        Ok(())
    }
    
    fn load_dag_object_from_disk(&self, cid: &str) -> SyncResult<DagObject> {
        let path = self.get_dag_object_path(cid);
        
        let mut file = File::open(&path)
            .map_err(|e| SyncError::IoError(format!("Failed to open DAG file: {}", e)))?;
            
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| SyncError::IoError(format!("Failed to read DAG file: {}", e)))?;
            
        let obj: DagObject = serde_json::from_str(&content)
            .map_err(|e| SyncError::SerializationError(format!("Failed to deserialize DAG object: {}", e)))?;
            
        Ok(obj)
    }
    
    /// Clear caches to force fresh data to be fetched
    pub async fn clear_caches(&self) {
        // Clear memory caches
        {
            let mut dag_cache = self.dag_cache.write().await;
            dag_cache.clear();
        }
        
        debug!("Cleared all sync caches");
    }
    
    /// Check server connectivity
    pub async fn check_connection(&self) -> SyncResult<bool> {
        for server_url in &self.config.servers {
            let url = format!("{}/health", server_url);
            
            match self.http_client.get(&url)
                .timeout(Duration::from_secs(5))
                .send().await 
            {
                Ok(response) => {
                    if response.status().is_success() {
                        return Ok(true);
                    }
                },
                Err(_) => {
                    // Try next server
                }
            }
        }
        
        // No servers responded
        Ok(false)
    }
} 