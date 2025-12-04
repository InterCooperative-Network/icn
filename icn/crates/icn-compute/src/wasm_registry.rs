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
            deployed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
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

/// WASM Registry - stores WASM modules with metadata
///
/// Thread-safe with in-memory caching and optional persistent storage.
pub struct WasmRegistry {
    /// In-memory module cache
    modules: Arc<RwLock<HashMap<WasmHash, Vec<u8>>>>,
    /// In-memory metadata cache
    metadata: Arc<RwLock<HashMap<WasmHash, WasmMetadata>>>,
    /// Owner → modules index
    owner_index: Arc<RwLock<HashMap<String, Vec<WasmHash>>>>,
    /// Optional persistent store
    store: Option<sled::Db>,
}

impl WasmRegistry {
    /// Create a new in-memory registry
    pub fn new() -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            owner_index: Arc::new(RwLock::new(HashMap::new())),
            store: None,
        }
    }

    /// Create a registry with persistent storage
    pub fn with_store(db: sled::Db) -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            owner_index: Arc::new(RwLock::new(HashMap::new())),
            store: Some(db),
        }
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

        // Check persistent store
        if let Some(store) = &self.store {
            let key = format!("wasm:{hash_hex}");
            if store
                .contains_key(key.as_bytes())
                .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?
            {
                return Err(WasmRegistryError::AlreadyExists(hash_hex));
            }
        }

        // Build metadata
        let metadata = metadata_builder(WasmMetadata::new(&wasm_bytes, owner));

        // Persist to store if available
        if let Some(store) = &self.store {
            let wasm_key = format!("wasm:{hash_hex}");
            store
                .insert(wasm_key.as_bytes(), wasm_bytes.as_slice())
                .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?;

            let meta_key = format!("wasm_meta:{hash_hex}");
            let meta_bytes = bincode::serialize(&metadata)
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

        Ok(hash)
    }

    /// Get WASM bytecode by hash (async wrapper)
    pub async fn get(&self, hash: &WasmHash) -> Result<Option<Vec<u8>>> {
        self.get_blocking(hash)
    }

    /// Get WASM bytecode by hash
    ///
    /// This is the main implementation, safe to call from both sync and async contexts.
    pub fn get_blocking(&self, hash: &WasmHash) -> Result<Option<Vec<u8>>> {
        // Check cache first
        {
            let modules = self
                .modules
                .read()
                .map_err(|e| WasmRegistryError::StorageError(format!("Lock poisoned: {e}")))?;
            if let Some(wasm) = modules.get(hash) {
                return Ok(Some(wasm.clone()));
            }
        }

        // Try persistent store
        if let Some(store) = &self.store {
            let key = format!("wasm:{}", hex::encode(hash));
            if let Some(bytes) = store
                .get(key.as_bytes())
                .map_err(|e| WasmRegistryError::StorageError(e.to_string()))?
            {
                let wasm_bytes = bytes.to_vec();

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
                let meta: WasmMetadata = bincode::deserialize(&bytes)
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
    pub fn load_from_store_sync(&self) -> Result<usize> {
        let Some(store) = &self.store else {
            return Ok(0);
        };

        let mut loaded = 0;

        // Scan for wasm keys
        for item in store.scan_prefix(b"wasm:") {
            let (key, value) = item.map_err(|e| WasmRegistryError::StorageError(e.to_string()))?;
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
                            if let Ok(meta) = bincode::deserialize::<WasmMetadata>(&meta_bytes) {
                                let mut metadata = self.metadata.write().map_err(|e| {
                                    WasmRegistryError::StorageError(format!("Lock poisoned: {e}"))
                                })?;
                                metadata.insert(hash, meta.clone());

                                // Rebuild owner index
                                let mut owner_index = self.owner_index.write().map_err(|e| {
                                    WasmRegistryError::StorageError(format!("Lock poisoned: {e}"))
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
}
