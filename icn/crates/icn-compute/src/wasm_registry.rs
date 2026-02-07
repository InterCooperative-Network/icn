//! WASM Registry - Content-Addressed Storage for WASM Bytecode
//!
//! Enables "deploy once, reference by hash" pattern for WASM modules:
//! 1. Deploy WASM module to registry (gets content hash)
//! 2. Reference module by hash in compute tasks (TaskCode::WasmRef)
//! 3. Executor fetches module from registry before execution
//!
//! ## Storage Schema
//!
//! ```text
//! wasm:<hash>           → Vec<u8> (WASM bytecode)
//! wasm_meta:<hash>      → WasmMetadata (bincode)
//! wasm_owner:<did>      → Vec<[u8; 32]> (list of hashes)
//! ```

use icn_kernel_api::state::{BlobService, StateError};
use icn_kernel_api::types::Namespace;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Content hash for WASM addressing (blake3)
pub type WasmHash = [u8; 32];

/// WASM registry errors
#[derive(Debug, Error)]
pub enum WasmRegistryError {
    #[error("WASM module not found: {0}")]
    NotFound(String),

    #[error("WASM module already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid WASM module: {0}")]
    InvalidModule(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type Result<T> = std::result::Result<T, WasmRegistryError>;

/// Metadata stored alongside WASM modules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmMetadata {
    /// Content hash of the module
    pub code_hash: WasmHash,
    /// Module name (optional, user-provided)
    pub name: Option<String>,
    /// Size in bytes
    pub size_bytes: usize,
    /// Owner/deployer DID
    pub owner: String,
    /// Deployment timestamp (Unix millis)
    pub deployed_at: u64,
    /// Optional description
    pub description: Option<String>,
    /// Required capabilities (user-declared)
    pub capabilities: Vec<String>,
}

impl WasmMetadata {
    /// Create metadata from WASM bytecode
    pub fn new(wasm_bytes: &[u8], owner: &str) -> Self {
        let code_hash = compute_hash(wasm_bytes);
        Self {
            code_hash,
            name: None,
            size_bytes: wasm_bytes.len(),
            owner: owner.to_string(),
            deployed_at: icn_time::current_timestamp_millis(),
            description: None,
            capabilities: Vec::new(),
        }
    }

    /// Set module name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a capability
    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }
}

/// Compute content hash for WASM bytecode
pub fn compute_hash(wasm_bytes: &[u8]) -> WasmHash {
    *blake3::hash(wasm_bytes).as_bytes()
}

/// Validate WASM bytecode has valid magic bytes
fn validate_wasm(wasm_bytes: &[u8]) -> Result<()> {
    // WASM magic number: 0x00 0x61 0x73 0x6D (\0asm)
    const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];

    if wasm_bytes.len() < 8 {
        return Err(WasmRegistryError::InvalidModule(
            "WASM module too short (minimum 8 bytes)".into(),
        ));
    }

    if wasm_bytes[0..4] != WASM_MAGIC {
        return Err(WasmRegistryError::InvalidModule(
            "Invalid WASM magic bytes".into(),
        ));
    }

    Ok(())
}

/// Default namespace for WASM blobs in BlobService.
const WASM_BLOB_NAMESPACE: &str = "wasm";

/// Callback invoked after a WASM module is deployed, to announce availability via gossip.
/// Parameters: (blob_hash, size_bytes)
pub type BlobAnnounceCallback = Arc<dyn Fn([u8; 32], u64) + Send + Sync>;

/// Callback invoked to fetch a WASM module from a remote peer via blob transfer.
/// Parameters: (blob_hash) -> Result<Vec<u8>>
pub type BlobFetchCallback = Arc<
    dyn Fn(
            [u8; 32],
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = std::result::Result<
                            Vec<u8>,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        > + Send
        + Sync,
>;

/// WASM Registry - stores WASM modules with metadata
///
/// Thread-safe with in-memory caching and optional persistent storage.
/// When a `BlobService` is provided, bytecode is delegated to it (source of truth).
/// Metadata and owner index remain in the local sled store / in-memory cache.
pub struct WasmRegistry {
    /// In-memory module cache
    modules: Arc<RwLock<HashMap<WasmHash, Vec<u8>>>>,
    /// In-memory metadata cache
    metadata: Arc<RwLock<HashMap<WasmHash, WasmMetadata>>>,
    /// Owner → modules index
    owner_index: Arc<RwLock<HashMap<String, Vec<WasmHash>>>>,
    /// Optional persistent store (metadata + fallback bytecode)
    store: Option<sled::Db>,
    /// Optional content-addressed blob storage (bytecode source of truth when present)
    blob_service: Option<Arc<dyn BlobService>>,
    /// Callback to announce blob availability via gossip after deploy (#1073)
    announce_callback: Option<BlobAnnounceCallback>,
    /// Callback to fetch a blob from a remote peer (#1073)
    fetch_callback: Option<BlobFetchCallback>,
}

impl WasmRegistry {
    /// Create a new in-memory registry
    pub fn new() -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            owner_index: Arc::new(RwLock::new(HashMap::new())),
            store: None,
            blob_service: None,
            announce_callback: None,
            fetch_callback: None,
        }
    }

    /// Create a registry with persistent storage
    pub fn with_store(db: sled::Db) -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            owner_index: Arc::new(RwLock::new(HashMap::new())),
            store: Some(db),
            blob_service: None,
            announce_callback: None,
            fetch_callback: None,
        }
    }

    /// Create a registry backed by a BlobService for bytecode storage.
    ///
    /// When a BlobService is provided, it becomes the source of truth for
    /// WASM bytecode. Metadata is still stored in the sled store.
    pub fn with_blob_service(db: sled::Db, blob_service: Arc<dyn BlobService>) -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            owner_index: Arc::new(RwLock::new(HashMap::new())),
            store: Some(db),
            blob_service: Some(blob_service),
            announce_callback: None,
            fetch_callback: None,
        }
    }

    /// Set the gossip announce callback for notifying peers about new deployments.
    pub fn set_announce_callback(&mut self, cb: BlobAnnounceCallback) {
        self.announce_callback = Some(cb);
    }

    /// Set the remote fetch callback for fetching modules from peers.
    pub fn set_fetch_callback(&mut self, cb: BlobFetchCallback) {
        self.fetch_callback = Some(cb);
    }

    /// Open a registry with persistent storage at the given path
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = sled::open(path).map_err(|e| WasmRegistryError::StorageError(e.to_string()))?;
        Ok(Self::with_store(db))
    }

    /// Create a temporary in-memory registry (for testing)
    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?;
        Ok(Self::with_store(db))
    }

    /// Returns the namespace used for WASM blobs in BlobService.
    fn blob_namespace() -> Namespace {
        Namespace::new("icn", WASM_BLOB_NAMESPACE)
    }

    /// Deploy a WASM module to the registry
    ///
    /// Returns the content hash on success.
    pub async fn deploy(
        &self,
        wasm_bytes: Vec<u8>,
        owner: &str,
        metadata_builder: impl FnOnce(WasmMetadata) -> WasmMetadata,
    ) -> Result<WasmHash> {
        self.deploy_sync(wasm_bytes, owner, metadata_builder)
    }

    /// Deploy a WASM module to the registry (sync version)
    ///
    /// Returns the content hash on success.
    pub fn deploy_sync(
        &self,
        wasm_bytes: Vec<u8>,
        owner: &str,
        metadata_builder: impl FnOnce(WasmMetadata) -> WasmMetadata,
    ) -> Result<WasmHash> {
        // Validate WASM
        validate_wasm(&wasm_bytes)?;

        let hash = compute_hash(&wasm_bytes);
        let hash_hex = hex::encode(hash);

        // Check if already exists
        {
            let modules = self
                .modules
                .read()
                .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
            if modules.contains_key(&hash) {
                return Err(WasmRegistryError::AlreadyExists(hash_hex));
            }
        }

        // Check BlobService first, then persistent store
        if let Some(bs) = &self.blob_service {
            if bs.exists(&hash).unwrap_or(false) {
                return Err(WasmRegistryError::AlreadyExists(hash_hex));
            }
        } else if let Some(store) = &self.store {
            let key = format!("wasm:{hash_hex}");
            if store
                .contains_key(key.as_bytes())
                .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?
            {
                return Err(WasmRegistryError::AlreadyExists(hash_hex));
            }
        }

        // Capture size before wasm_bytes is moved
        let wasm_size = wasm_bytes.len() as u64;

        // Build metadata
        let metadata = metadata_builder(WasmMetadata::new(&wasm_bytes, owner));

        // Store bytecode: prefer BlobService, fall back to sled
        if let Some(bs) = &self.blob_service {
            let stored_hash = bs
                .put(&Self::blob_namespace(), &wasm_bytes)
                .map_err(|e| WasmRegistryError::StorageError(format!("BlobService put: {e}")))?;
            // Verify BlobService computed the same hash
            if stored_hash != hash {
                return Err(WasmRegistryError::StorageError(format!(
                    "BlobService hash mismatch: expected {}, got {}",
                    hex::encode(hash),
                    hex::encode(stored_hash),
                )));
            }
        } else if let Some(store) = &self.store {
            let wasm_key = format!("wasm:{hash_hex}");
            store
                .insert(wasm_key.as_bytes(), wasm_bytes.as_slice())
                .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?;
        }

        // Persist metadata to sled (always, regardless of blob storage backend)
        if let Some(store) = &self.store {
            let meta_key = format!("wasm_meta:{hash_hex}");
            let meta_bytes = icn_encoding::encode_versioned(&metadata)
                .map_err(|e| WasmRegistryError::SerializationError(e.to_string()))?;
            store
                .insert(meta_key.as_bytes(), meta_bytes)
                .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?;
        }

        // Update in-memory caches
        {
            let mut modules = self
                .modules
                .write()
                .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
            modules.insert(hash, wasm_bytes);
        }
        {
            let mut meta_cache = self
                .metadata
                .write()
                .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
            meta_cache.insert(hash, metadata);
        }
        {
            let mut owner_index = self
                .owner_index
                .write()
                .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
            owner_index.entry(owner.to_string()).or_default().push(hash);
        }

        tracing::info!(
            hash = %hash_hex,
            owner = %owner,
            "WASM module deployed to registry"
        );

        // Announce availability via gossip (fire and forget)
        if let Some(ref cb) = self.announce_callback {
            tracing::debug!(
                hash = %hash_hex,
                size = wasm_size,
                "Announcing WASM module via gossip"
            );
            cb(hash, wasm_size);
        }

        Ok(hash)
    }

    /// Fetch a WASM module from a remote peer via the blob transfer protocol.
    ///
    /// Calls the configured `fetch_callback` to request the blob from peers,
    /// then verifies the content hash matches before returning.
    ///
    /// Returns an error if no fetch callback is configured or the fetch fails.
    pub async fn fetch_remote(&self, hash: &WasmHash) -> Result<Vec<u8>> {
        let cb = self.fetch_callback.as_ref().ok_or_else(|| {
            WasmRegistryError::StorageError("No remote fetch callback configured".into())
        })?;

        let bytes = (cb)(*hash)
            .await
            .map_err(|e| WasmRegistryError::StorageError(format!("Remote fetch failed: {e}")))?;

        // Verify hash matches
        let actual = compute_hash(&bytes);
        if actual != *hash {
            return Err(WasmRegistryError::StorageError(format!(
                "Remote fetch hash mismatch: expected {}, got {}",
                hex::encode(hash),
                hex::encode(actual),
            )));
        }

        // Validate WASM format
        validate_wasm(&bytes)?;

        Ok(bytes)
    }

    /// Resolve a WASM module by hash, fetching from remote peers if not found locally.
    ///
    /// This is the primary entry point for execute-by-hash (#1074):
    /// 1. Check local cache / BlobService / sled store
    /// 2. If not found locally, attempt remote fetch via gossip
    /// 3. Verify content hash matches after fetch
    /// 4. Store fetched module locally for future use
    ///
    /// Returns `Ok(bytes)` on success, or `Err` if the module cannot be obtained.
    pub async fn resolve_or_fetch(&self, hash: &WasmHash) -> Result<Vec<u8>> {
        // Try local first
        if let Some(bytes) = self.get(hash).await? {
            tracing::debug!(
                hash = %hex::encode(hash),
                "WASM module found locally"
            );
            return Ok(bytes);
        }

        // Not found locally -- attempt remote fetch
        tracing::info!(
            hash = %hex::encode(hash),
            "WASM module not found locally, attempting remote fetch"
        );

        let bytes = self.fetch_remote(hash).await?;

        // fetch_remote already verifies hash and validates WASM magic bytes.
        // Store the fetched module locally for future use.
        // Use deploy_sync which handles dedup (AlreadyExists is not an error here).
        let store_result = self.deploy_sync(bytes.clone(), "remote", |m| m);
        match store_result {
            Ok(_) => {
                tracing::info!(
                    hash = %hex::encode(hash),
                    size = bytes.len(),
                    "Remote WASM module fetched and stored locally"
                );
            }
            Err(WasmRegistryError::AlreadyExists(_)) => {
                // Race condition: another task stored it between our get() and deploy_sync().
                // This is fine.
                tracing::debug!(
                    hash = %hex::encode(hash),
                    "Remote WASM module already stored (race)"
                );
            }
            Err(e) => {
                tracing::warn!(
                    hash = %hex::encode(hash),
                    error = %e,
                    "Failed to store fetched WASM module locally"
                );
                return Err(e);
            }
        }

        Ok(bytes)
    }

    /// Get WASM bytecode by hash (async wrapper)
    pub async fn get(&self, hash: &WasmHash) -> Result<Option<Vec<u8>>> {
        self.get_blocking(hash)
    }

    /// Get WASM bytecode by hash
    ///
    /// This is the main implementation, safe to call from both sync and async contexts.
    /// Hash is verified on every load from storage backends (not cache).
    pub fn get_blocking(&self, hash: &WasmHash) -> Result<Option<Vec<u8>>> {
        // Check cache first (already verified on insertion)
        {
            let modules = self
                .modules
                .read()
                .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
            if let Some(wasm) = modules.get(hash) {
                return Ok(Some(wasm.clone()));
            }
        }

        // Try BlobService first, then sled
        if let Some(bs) = &self.blob_service {
            match bs.get(hash) {
                Ok(wasm_bytes) => {
                    // Verify hash on load
                    let actual_hash = compute_hash(&wasm_bytes);
                    if actual_hash != *hash {
                        return Err(WasmRegistryError::StorageError(format!(
                            "Hash verification failed on load: expected {}, got {}",
                            hex::encode(hash),
                            hex::encode(actual_hash),
                        )));
                    }

                    // Populate cache
                    let mut modules = self.modules.write().map_err(|e| {
                        WasmRegistryError::StorageError(format!("Lock poisoned: {e}"))
                    })?;
                    modules.insert(*hash, wasm_bytes.clone());
                    return Ok(Some(wasm_bytes));
                }
                Err(StateError::BlobNotFound) => {}
                Err(e) => {
                    return Err(WasmRegistryError::StorageError(format!(
                        "BlobService get: {e}"
                    )));
                }
            }
        } else if let Some(store) = &self.store {
            let key = format!("wasm:{}", hex::encode(hash));
            if let Some(bytes) = store
                .get(key.as_bytes())
                .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?
            {
                let wasm_bytes = bytes.to_vec();

                // Verify hash on load
                let actual_hash = compute_hash(&wasm_bytes);
                if actual_hash != *hash {
                    return Err(WasmRegistryError::StorageError(format!(
                        "Hash verification failed on load: expected {}, got {}",
                        hex::encode(hash),
                        hex::encode(actual_hash),
                    )));
                }

                // Populate cache
                let mut modules = self
                    .modules
                    .write()
                    .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
                modules.insert(*hash, wasm_bytes.clone());

                return Ok(Some(wasm_bytes));
            }
        }

        Ok(None)
    }

    /// Get metadata by hash
    pub async fn get_metadata(&self, hash: &WasmHash) -> Result<Option<WasmMetadata>> {
        self.get_metadata_sync(hash)
    }

    /// Get metadata by hash (sync version)
    pub fn get_metadata_sync(&self, hash: &WasmHash) -> Result<Option<WasmMetadata>> {
        // Check cache first
        {
            let metadata = self
                .metadata
                .read()
                .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
            if let Some(meta) = metadata.get(hash) {
                return Ok(Some(meta.clone()));
            }
        }

        // Try persistent store
        if let Some(store) = &self.store {
            let key = format!("wasm_meta:{}", hex::encode(hash));
            if let Some(bytes) = store
                .get(key.as_bytes())
                .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?
            {
                let meta: WasmMetadata = icn_encoding::decode_versioned(&bytes)
                    .map_err(|e| WasmRegistryError::SerializationError(e.to_string()))?;

                // Populate cache
                let mut metadata = self
                    .metadata
                    .write()
                    .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
                metadata.insert(*hash, meta.clone());

                return Ok(Some(meta));
            }
        }

        Ok(None)
    }

    /// Check if a module exists
    pub async fn exists(&self, hash: &WasmHash) -> bool {
        self.exists_sync(hash)
    }

    /// Check if a module exists (sync version)
    pub fn exists_sync(&self, hash: &WasmHash) -> bool {
        let modules = self.modules.read().ok();
        if let Some(modules) = modules {
            if modules.contains_key(hash) {
                return true;
            }
        }

        if let Some(bs) = &self.blob_service {
            if bs.exists(hash).unwrap_or(false) {
                return true;
            }
        }

        if let Some(store) = &self.store {
            let key = format!("wasm:{}", hex::encode(hash));
            store.contains_key(key.as_bytes()).unwrap_or(false)
        } else {
            false
        }
    }

    /// List modules by owner
    pub async fn list_by_owner(&self, owner: &str) -> Result<Vec<WasmMetadata>> {
        self.list_by_owner_sync(owner)
    }

    /// List modules by owner (sync version)
    pub fn list_by_owner_sync(&self, owner: &str) -> Result<Vec<WasmMetadata>> {
        let owner_index = self
            .owner_index
            .read()
            .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
        let metadata = self
            .metadata
            .read()
            .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;

        if let Some(hashes) = owner_index.get(owner) {
            Ok(hashes
                .iter()
                .filter_map(|h| metadata.get(h).cloned())
                .collect())
        } else {
            Ok(vec![])
        }
    }

    /// Get registry statistics
    pub async fn stats(&self) -> WasmRegistryStats {
        self.stats_sync()
    }

    /// Get registry statistics (sync version)
    pub fn stats_sync(&self) -> WasmRegistryStats {
        let modules = self.modules.read().ok();
        let owner_index = self.owner_index.read().ok();

        let (total_modules, total_bytes) = modules
            .map(|m| (m.len(), m.values().map(|v| v.len()).sum()))
            .unwrap_or((0, 0));

        let unique_owners = owner_index.map(|o| o.len()).unwrap_or(0);

        WasmRegistryStats {
            total_modules,
            total_bytes,
            unique_owners,
        }
    }

    /// Load modules from persistent store into cache
    pub async fn load_from_store(&self) -> Result<usize> {
        self.load_from_store_sync()
    }

    /// Load modules from persistent store into cache (sync version)
    ///
    /// When BlobService is configured, scans metadata from sled and fetches
    /// bytecode from BlobService. Otherwise falls back to loading both from sled.
    pub fn load_from_store_sync(&self) -> Result<usize> {
        let Some(store) = &self.store else {
            return Ok(0);
        };

        let mut loaded = 0;

        if self.blob_service.is_some() {
            // BlobService mode: scan metadata keys, fetch bytecode from BlobService
            for item in store.scan_prefix(b"wasm_meta:") {
                let (key, value) =
                    item.map_err(|e| WasmRegistryError::StorageError(e.to_string()))?;
                let key_str = String::from_utf8_lossy(&key);

                if let Some(hash_hex) = key_str.strip_prefix("wasm_meta:") {
                    if let Ok(hash_bytes) = hex::decode(hash_hex) {
                        if hash_bytes.len() == 32 {
                            let mut hash = [0u8; 32];
                            hash.copy_from_slice(&hash_bytes);

                            // Load metadata
                            if let Ok(meta) = icn_encoding::decode_versioned::<WasmMetadata>(&value)
                            {
                                let mut metadata = self.metadata.write().map_err(|e| {
                                    WasmRegistryError::StorageError(format!("Lock poisoned: {e}"))
                                })?;
                                metadata.insert(hash, meta.clone());

                                let mut owner_index = self.owner_index.write().map_err(|e| {
                                    WasmRegistryError::StorageError(format!("Lock poisoned: {e}"))
                                })?;
                                owner_index
                                    .entry(meta.owner.clone())
                                    .or_default()
                                    .push(hash);
                            }
                            // Bytecode not pre-loaded; fetched on demand from BlobService
                            loaded += 1;
                        }
                    }
                }
            }
        } else {
            // Legacy mode: scan wasm: keys for bytecode
            for item in store.scan_prefix(b"wasm:") {
                let (key, value) =
                    item.map_err(|e| WasmRegistryError::StorageError(e.to_string()))?;
                let key_str = String::from_utf8_lossy(&key);

                // Skip metadata keys
                if key_str.starts_with("wasm_meta:") {
                    continue;
                }

                if let Some(hash_hex) = key_str.strip_prefix("wasm:") {
                    if let Ok(hash_bytes) = hex::decode(hash_hex) {
                        if hash_bytes.len() == 32 {
                            let mut hash = [0u8; 32];
                            hash.copy_from_slice(&hash_bytes);

                            let wasm_bytes = value.to_vec();

                            // Load into cache
                            {
                                let mut modules = self.modules.write().map_err(|e| {
                                    WasmRegistryError::StorageError(format!("Lock poisoned: {e}"))
                                })?;
                                modules.insert(hash, wasm_bytes);
                            }

                            // Also load metadata
                            let meta_key = format!("wasm_meta:{hash_hex}");
                            if let Ok(Some(meta_bytes)) = store.get(meta_key.as_bytes()) {
                                if let Ok(meta) =
                                    icn_encoding::decode_versioned::<WasmMetadata>(&meta_bytes)
                                {
                                    let mut metadata = self.metadata.write().map_err(|e| {
                                        WasmRegistryError::StorageError(format!(
                                            "Lock poisoned: {e}"
                                        ))
                                    })?;
                                    metadata.insert(hash, meta.clone());

                                    let mut owner_index =
                                        self.owner_index.write().map_err(|e| {
                                            WasmRegistryError::StorageError(format!(
                                                "Lock poisoned: {e}"
                                            ))
                                        })?;
                                    owner_index
                                        .entry(meta.owner.clone())
                                        .or_default()
                                        .push(hash);
                                }
                            }

                            loaded += 1;
                        }
                    }
                }
            }
        }

        tracing::info!(loaded = loaded, "Loaded WASM modules from persistent store");
        Ok(loaded)
    }
}

impl Default for WasmRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmRegistryStats {
    pub total_modules: usize,
    pub total_bytes: usize,
    pub unique_owners: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::state::BlobMetadata;
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex;

    /// In-memory BlobService implementation for testing.
    #[allow(clippy::type_complexity)]
    struct MockBlobService {
        blobs: Mutex<StdHashMap<[u8; 32], (Vec<u8>, Namespace)>>,
    }

    impl MockBlobService {
        fn new() -> Self {
            Self {
                blobs: Mutex::new(StdHashMap::new()),
            }
        }
    }

    impl BlobService for MockBlobService {
        fn put(
            &self,
            namespace: &Namespace,
            data: &[u8],
        ) -> std::result::Result<[u8; 32], StateError> {
            let hash = *blake3::hash(data).as_bytes();
            let mut blobs = self.blobs.lock().unwrap();
            blobs.insert(hash, (data.to_vec(), namespace.clone()));
            Ok(hash)
        }

        fn get(&self, hash: &[u8; 32]) -> std::result::Result<Vec<u8>, StateError> {
            let blobs = self.blobs.lock().unwrap();
            blobs
                .get(hash)
                .map(|(data, _)| data.clone())
                .ok_or(StateError::BlobNotFound)
        }

        fn exists(&self, hash: &[u8; 32]) -> std::result::Result<bool, StateError> {
            let blobs = self.blobs.lock().unwrap();
            Ok(blobs.contains_key(hash))
        }

        fn delete(&self, hash: &[u8; 32]) -> std::result::Result<(), StateError> {
            let mut blobs = self.blobs.lock().unwrap();
            blobs.remove(hash);
            Ok(())
        }

        fn metadata(&self, hash: &[u8; 32]) -> std::result::Result<BlobMetadata, StateError> {
            let blobs = self.blobs.lock().unwrap();
            let (data, ns) = blobs.get(hash).ok_or(StateError::BlobNotFound)?;
            Ok(BlobMetadata {
                size: data.len() as u64,
                namespace: ns.clone(),
                created_at: 0,
                content_type: None,
            })
        }
    }

    fn sample_wasm() -> Vec<u8> {
        // Minimal valid WASM module (just magic + version)
        vec![
            0x00, 0x61, 0x73, 0x6D, // magic: \0asm
            0x01, 0x00, 0x00, 0x00, // version: 1
        ]
    }

    #[tokio::test]
    async fn test_deploy_and_get() {
        let registry = WasmRegistry::new();
        let wasm = sample_wasm();

        let hash = registry
            .deploy(wasm.clone(), "did:icn:owner", |m| m)
            .await
            .unwrap();

        let retrieved = registry.get(&hash).await.unwrap().unwrap();
        assert_eq!(retrieved, wasm);
    }

    #[tokio::test]
    async fn test_get_metadata() {
        let registry = WasmRegistry::new();
        let wasm = sample_wasm();

        let hash = registry
            .deploy(wasm, "did:icn:alice", |m| {
                m.with_name("test-module").with_description("Test WASM")
            })
            .await
            .unwrap();

        let meta = registry.get_metadata(&hash).await.unwrap().unwrap();
        assert_eq!(meta.name, Some("test-module".to_string()));
        assert_eq!(meta.description, Some("Test WASM".to_string()));
        assert_eq!(meta.owner, "did:icn:alice");
    }

    #[tokio::test]
    async fn test_duplicate_rejected() {
        let registry = WasmRegistry::new();
        let wasm = sample_wasm();

        // First deploy succeeds
        registry
            .deploy(wasm.clone(), "did:icn:owner", |m| m)
            .await
            .unwrap();

        // Second deploy fails
        let result = registry.deploy(wasm, "did:icn:owner", |m| m).await;
        assert!(matches!(result, Err(WasmRegistryError::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_invalid_wasm_rejected() {
        let registry = WasmRegistry::new();
        let invalid = vec![0x00, 0x00, 0x00, 0x00]; // Not valid WASM

        let result = registry.deploy(invalid, "did:icn:owner", |m| m).await;
        assert!(matches!(result, Err(WasmRegistryError::InvalidModule(_))));
    }

    #[tokio::test]
    async fn test_not_found() {
        let registry = WasmRegistry::new();
        let fake_hash = [0xAA; 32];

        let result = registry.get(&fake_hash).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_exists() {
        let registry = WasmRegistry::new();
        let wasm = sample_wasm();
        let fake_hash = [0xAA; 32];

        let hash = registry.deploy(wasm, "did:icn:owner", |m| m).await.unwrap();

        assert!(registry.exists(&hash).await);
        assert!(!registry.exists(&fake_hash).await);
    }

    #[tokio::test]
    async fn test_list_by_owner() {
        let registry = WasmRegistry::new();

        // Deploy from owner1
        let wasm1 = sample_wasm();
        registry
            .deploy(wasm1, "did:icn:owner1", |m| m.with_name("mod1"))
            .await
            .unwrap();

        // Deploy from owner2 (different WASM to get different hash)
        let mut wasm2 = sample_wasm();
        wasm2.push(0x00); // Make it different
        registry
            .deploy(wasm2, "did:icn:owner2", |m| m.with_name("mod2"))
            .await
            .unwrap();

        // List owner1's modules
        let owner1_mods = registry.list_by_owner("did:icn:owner1").await.unwrap();
        assert_eq!(owner1_mods.len(), 1);
        assert_eq!(owner1_mods[0].name, Some("mod1".to_string()));

        // List owner2's modules
        let owner2_mods = registry.list_by_owner("did:icn:owner2").await.unwrap();
        assert_eq!(owner2_mods.len(), 1);
        assert_eq!(owner2_mods[0].name, Some("mod2".to_string()));
    }

    #[tokio::test]
    async fn test_stats() {
        let registry = WasmRegistry::new();
        let wasm = sample_wasm();
        let size = wasm.len();

        registry.deploy(wasm, "did:icn:owner", |m| m).await.unwrap();

        let stats = registry.stats().await;
        assert_eq!(stats.total_modules, 1);
        assert_eq!(stats.total_bytes, size);
        assert_eq!(stats.unique_owners, 1);
    }

    #[tokio::test]
    async fn test_persistent_storage() {
        let registry = WasmRegistry::temporary().unwrap();
        let wasm = sample_wasm();

        let hash = registry
            .deploy(wasm.clone(), "did:icn:owner", |m| m)
            .await
            .unwrap();

        // Clear in-memory cache
        {
            let mut modules = registry.modules.write().unwrap();
            modules.clear();
        }

        // Should still be retrievable from persistent store
        let retrieved = registry.get(&hash).await.unwrap().unwrap();
        assert_eq!(retrieved, wasm);
    }

    #[test]
    fn test_compute_hash() {
        let wasm = sample_wasm();
        let hash1 = compute_hash(&wasm);
        let hash2 = compute_hash(&wasm);

        // Same input produces same hash
        assert_eq!(hash1, hash2);

        // Different input produces different hash
        let mut different = wasm.clone();
        different.push(0xFF);
        let hash3 = compute_hash(&different);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_validate_wasm() {
        // Valid WASM
        let valid = sample_wasm();
        assert!(validate_wasm(&valid).is_ok());

        // Too short
        let short = vec![0x00, 0x61];
        assert!(validate_wasm(&short).is_err());

        // Wrong magic
        let wrong_magic = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x00];
        assert!(validate_wasm(&wrong_magic).is_err());
    }

    // --- BlobService integration tests ---

    fn blob_registry() -> WasmRegistry {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let bs = Arc::new(MockBlobService::new());
        WasmRegistry::with_blob_service(db, bs)
    }

    #[tokio::test]
    async fn test_blob_deploy_and_get() {
        let registry = blob_registry();
        let wasm = sample_wasm();

        let hash = registry
            .deploy(wasm.clone(), "did:icn:owner", |m| m)
            .await
            .unwrap();

        let retrieved = registry.get(&hash).await.unwrap().unwrap();
        assert_eq!(retrieved, wasm);
    }

    #[tokio::test]
    async fn test_blob_deploy_metadata_persists() {
        let registry = blob_registry();
        let wasm = sample_wasm();

        let hash = registry
            .deploy(wasm, "did:icn:alice", |m| {
                m.with_name("blob-mod").with_description("Blob test")
            })
            .await
            .unwrap();

        let meta = registry.get_metadata(&hash).await.unwrap().unwrap();
        assert_eq!(meta.name, Some("blob-mod".to_string()));
        assert_eq!(meta.owner, "did:icn:alice");
    }

    #[tokio::test]
    async fn test_blob_duplicate_rejected() {
        let registry = blob_registry();
        let wasm = sample_wasm();

        registry
            .deploy(wasm.clone(), "did:icn:owner", |m| m)
            .await
            .unwrap();

        let result = registry.deploy(wasm, "did:icn:owner", |m| m).await;
        assert!(matches!(result, Err(WasmRegistryError::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_blob_hash_verified_on_load() {
        let registry = blob_registry();
        let wasm = sample_wasm();

        let hash = registry
            .deploy(wasm.clone(), "did:icn:owner", |m| m)
            .await
            .unwrap();

        // Clear in-memory cache to force load from BlobService
        {
            let mut modules = registry.modules.write().unwrap();
            modules.clear();
        }

        // Get should succeed and return correct data
        let retrieved = registry.get(&hash).await.unwrap().unwrap();
        assert_eq!(retrieved, wasm);
        assert_eq!(compute_hash(&retrieved), hash);
    }

    #[tokio::test]
    async fn test_blob_exists() {
        let registry = blob_registry();
        let wasm = sample_wasm();
        let fake_hash = [0xAA; 32];

        let hash = registry.deploy(wasm, "did:icn:owner", |m| m).await.unwrap();

        assert!(registry.exists(&hash).await);
        assert!(!registry.exists(&fake_hash).await);
    }

    #[tokio::test]
    async fn test_blob_load_from_store() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let bs = Arc::new(MockBlobService::new());
        let registry = WasmRegistry::with_blob_service(db.clone(), bs.clone());
        let wasm = sample_wasm();

        let hash = registry
            .deploy(wasm.clone(), "did:icn:alice", |m| {
                m.with_name("persist-test")
            })
            .await
            .unwrap();

        // Create a new registry pointing to same db + blob_service
        let registry2 = WasmRegistry::with_blob_service(db, bs);
        let loaded = registry2.load_from_store_sync().unwrap();
        assert_eq!(loaded, 1);

        // Metadata should be loaded
        let meta = registry2.get_metadata(&hash).await.unwrap().unwrap();
        assert_eq!(meta.name, Some("persist-test".to_string()));

        // Bytecode should be fetchable from BlobService
        let retrieved = registry2.get(&hash).await.unwrap().unwrap();
        assert_eq!(retrieved, wasm);
    }

    // --- Gossip announce tests (#1073) ---

    #[tokio::test]
    async fn test_deploy_emits_announce() {
        #[allow(clippy::type_complexity)]
        let announced: Arc<Mutex<Vec<([u8; 32], u64)>>> = Arc::new(Mutex::new(Vec::new()));
        let announced_clone = announced.clone();

        let mut registry = blob_registry();
        registry.set_announce_callback(Arc::new(move |hash, size| {
            announced_clone.lock().unwrap().push((hash, size));
        }));

        let wasm = sample_wasm();
        let wasm_size = wasm.len() as u64;
        let hash = registry.deploy(wasm, "did:icn:owner", |m| m).await.unwrap();

        let announces = announced.lock().unwrap();
        assert_eq!(announces.len(), 1, "exactly one announce after deploy");
        assert_eq!(
            announces[0].0, hash,
            "announced hash must match deployed hash"
        );
        assert_eq!(announces[0].1, wasm_size, "announced size must match");
    }

    #[tokio::test]
    async fn test_no_announce_without_callback() {
        let registry = blob_registry();
        let wasm = sample_wasm();

        // Should succeed without announce callback
        let result = registry.deploy(wasm, "did:icn:owner", |m| m).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_remote_success() {
        let wasm = sample_wasm();
        let expected_hash = compute_hash(&wasm);
        let wasm_clone = wasm.clone();

        let mut registry = blob_registry();
        registry.set_fetch_callback(Arc::new(move |hash| {
            let data = wasm_clone.clone();
            let h = hash;
            Box::pin(async move {
                // Simulate remote fetch: return data if hash matches
                if h == compute_hash(&data) {
                    Ok(data)
                } else {
                    Err("not found".into())
                }
            })
        }));

        let fetched = registry.fetch_remote(&expected_hash).await.unwrap();
        assert_eq!(fetched, wasm);
    }

    #[tokio::test]
    async fn test_fetch_remote_hash_mismatch() {
        let _wasm = sample_wasm();

        let mut registry = blob_registry();
        registry.set_fetch_callback(Arc::new(move |_hash| {
            // Return wrong data (doesn't match requested hash)
            Box::pin(async move { Ok(vec![0xFF; 100]) })
        }));

        let fake_hash = [0xAA; 32];
        let result = registry.fetch_remote(&fake_hash).await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("mismatch") || err_msg.contains("Invalid"));
    }

    #[tokio::test]
    async fn test_fetch_remote_no_callback() {
        let registry = blob_registry();
        let result = registry.fetch_remote(&[0xBB; 32]).await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("No remote fetch callback"));
    }

    // --- resolve_or_fetch tests (#1074) ---

    #[tokio::test]
    async fn test_resolve_or_fetch_local_hit() {
        let registry = blob_registry();
        let wasm = sample_wasm();

        // Deploy locally first
        let hash = registry
            .deploy(wasm.clone(), "did:icn:owner", |m| m)
            .await
            .unwrap();

        // resolve_or_fetch should return local bytes without needing fetch callback
        let resolved = registry.resolve_or_fetch(&hash).await.unwrap();
        assert_eq!(resolved, wasm);
    }

    #[tokio::test]
    async fn test_resolve_or_fetch_remote_success() {
        let wasm = sample_wasm();
        let expected_hash = compute_hash(&wasm);
        let wasm_clone = wasm.clone();

        let mut registry = blob_registry();
        registry.set_fetch_callback(Arc::new(move |hash| {
            let data = wasm_clone.clone();
            let h = hash;
            Box::pin(async move {
                if h == compute_hash(&data) {
                    Ok(data)
                } else {
                    Err("not found".into())
                }
            })
        }));

        // Module is NOT deployed locally -- resolve_or_fetch should fetch remotely
        let resolved = registry.resolve_or_fetch(&expected_hash).await.unwrap();
        assert_eq!(resolved, wasm);

        // After fetch, module should be stored locally
        assert!(registry.exists(&expected_hash).await);
    }

    #[tokio::test]
    async fn test_resolve_or_fetch_remote_failure() {
        let mut registry = blob_registry();
        registry.set_fetch_callback(Arc::new(move |_hash| {
            Box::pin(async move { Err("peer unreachable".into()) })
        }));

        let fake_hash = [0xCC; 32];
        let result = registry.resolve_or_fetch(&fake_hash).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("Remote fetch failed"),
            "Expected 'Remote fetch failed', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_resolve_or_fetch_no_callback_no_local() {
        let registry = blob_registry();
        // No fetch callback, module not deployed locally
        let fake_hash = [0xDD; 32];
        let result = registry.resolve_or_fetch(&fake_hash).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("No remote fetch callback"),
            "Expected 'No remote fetch callback', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_resolve_or_fetch_hash_mismatch() {
        let mut registry = blob_registry();
        // Return wrong data that doesn't match the requested hash
        registry.set_fetch_callback(Arc::new(move |_hash| {
            Box::pin(async move {
                // Return valid WASM but with wrong content
                Ok(vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0xFF])
            })
        }));

        let fake_hash = [0xEE; 32];
        let result = registry.resolve_or_fetch(&fake_hash).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("mismatch"),
            "Expected hash mismatch error, got: {err_msg}"
        );
    }
}
