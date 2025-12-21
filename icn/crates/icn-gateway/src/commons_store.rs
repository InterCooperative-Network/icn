//! Commons Storage Backend
//!
//! Persistent storage abstraction for CommonsManager data using a key-value store.
//!
//! # Storage Layout
//!
//! ```text
//! commons/anchors/<anchor_id>              -> PersonhoodAnchor
//! commons/anchors/by_did/<did>             -> anchor_id
//! commons/holders/<holder_id>              -> CommonsHolderRecord
//! commons/holders/by_did/<did>             -> holder_id
//! commons/holders/by_anchor/<anchor_id>    -> holder_id
//! commons/charters/<charter_id>            -> Charter
//! commons/charters/by_domain/<domain_id>   -> charter_id
//! commons/stewards/<steward_id>            -> StewardRecord
//! commons/stewards/by_did/<did>            -> steward_id
//! commons/amendments/<amendment_id>        -> Amendment
//! commons/appeals/<appeal_id>              -> Appeal
//! commons/revocations/<revocation_id>      -> RevocationRecord
//! ```

use anyhow::{Context, Result};
use icn_governance::{Amendment, Appeal, Charter, StewardRecord};
use icn_identity::{CommonsHolderRecord, PersonhoodAnchor, RevocationRecord};
use lru::LruCache;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

// ============================================================================
// Storage Key Prefixes
// ============================================================================

pub const ANCHOR_PREFIX: &[u8] = b"commons/anchors/";
pub const ANCHOR_BY_DID_PREFIX: &[u8] = b"commons/anchors/by_did/";
pub const HOLDER_PREFIX: &[u8] = b"commons/holders/";
pub const HOLDER_BY_DID_PREFIX: &[u8] = b"commons/holders/by_did/";
pub const HOLDER_BY_ANCHOR_PREFIX: &[u8] = b"commons/holders/by_anchor/";
pub const CHARTER_PREFIX: &[u8] = b"commons/charters/";
pub const CHARTER_BY_DOMAIN_PREFIX: &[u8] = b"commons/charters/by_domain/";
pub const STEWARD_PREFIX: &[u8] = b"commons/stewards/";
pub const STEWARD_BY_DID_PREFIX: &[u8] = b"commons/stewards/by_did/";
pub const AMENDMENT_PREFIX: &[u8] = b"commons/amendments/";
pub const APPEAL_PREFIX: &[u8] = b"commons/appeals/";
pub const REVOCATION_PREFIX: &[u8] = b"commons/revocations/";
pub const CEREMONY_PREFIX: &[u8] = b"commons/ceremonies/";
pub const ENROLLMENT_SESSION_PREFIX: &[u8] = b"commons/enrollment_sessions/";

// ============================================================================
// Storage Backend Trait
// ============================================================================

/// Low-level key-value store backend trait
pub trait CommonsStoreBackend: Send + Sync {
    /// Get a value by key
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Store a key-value pair
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;

    /// Delete a key
    fn delete(&self, key: &[u8]) -> Result<()>;

    /// Scan all keys with a given prefix
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Check if a key exists
    fn exists(&self, key: &[u8]) -> Result<bool> {
        Ok(self.get(key)?.is_some())
    }
}

// ============================================================================
// In-Memory Implementation
// ============================================================================

/// In-memory store implementation for testing and development
#[derive(Default)]
pub struct InMemoryCommonsStore {
    pub(crate) data: RwLock<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl InMemoryCommonsStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total entry count
    pub fn len(&self) -> usize {
        self.data
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Data lock poisoned, recovering");
                poisoned.into_inner()
            })
            .len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Data lock poisoned, recovering");
                poisoned.into_inner()
            })
            .is_empty()
    }

    /// Clear all data
    pub fn clear(&self) {
        self.data
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Data lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
    }
}

impl CommonsStoreBackend for InMemoryCommonsStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self
            .data
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Data lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(key)
            .cloned())
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.data
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Data lock poisoned, recovering");
                poisoned.into_inner()
            })
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.data
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Data lock poisoned, recovering");
                poisoned.into_inner()
            })
            .remove(key);
        Ok(())
    }

    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let data = self.data.read().unwrap_or_else(|poisoned| {
            warn!("Data lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(data
            .range(prefix.to_vec()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

// ============================================================================
// Sled Implementation (Feature-gated)
// ============================================================================

/// Sled-based persistent store implementation
///
/// Enable with the `sled-storage` feature flag.
pub struct SledCommonsStore {
    db: sled::Db,
}

impl SledCommonsStore {
    /// Open a persistent Sled database at the given path
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = sled::open(path).context("Failed to open Sled database")?;
        Ok(SledCommonsStore { db })
    }

    /// Create a temporary Sled database (deleted on drop)
    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .context("Failed to create temporary Sled database")?;
        Ok(SledCommonsStore { db })
    }

    /// Flush all pending writes to disk
    pub fn flush(&self) -> Result<()> {
        self.db.flush().context("Failed to flush Sled database")?;
        Ok(())
    }

    /// Get approximate database size in bytes
    pub fn size_on_disk(&self) -> Result<u64> {
        Ok(self.db.size_on_disk().unwrap_or(0))
    }
}

impl CommonsStoreBackend for SledCommonsStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get(key)?.map(|v| v.to_vec()))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.insert(key, value)?;
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.db.remove(key)?;
        Ok(())
    }

    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();
        for item in self.db.scan_prefix(prefix) {
            let (k, v) = item?;
            results.push((k.to_vec(), v.to_vec()));
        }
        Ok(results)
    }
}

// ============================================================================
// Commons Store (High-Level Interface)
// ============================================================================

/// Cache configuration
#[derive(Clone)]
pub struct CacheConfig {
    pub anchor_cache_size: usize,
    pub holder_cache_size: usize,
    pub charter_cache_size: usize,
    pub steward_cache_size: usize,
    pub amendment_cache_size: usize,
    pub appeal_cache_size: usize,
    pub ceremony_cache_size: usize,
    pub enrollment_session_cache_size: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            anchor_cache_size: 1000,
            holder_cache_size: 1000,
            charter_cache_size: 500,
            steward_cache_size: 200,
            amendment_cache_size: 500,
            appeal_cache_size: 500,
            ceremony_cache_size: 500,
            enrollment_session_cache_size: 500,
        }
    }
}

/// High-level Commons storage with caching
pub struct CommonsStore<S: CommonsStoreBackend> {
    store: Arc<S>,
    // Caches for frequently accessed records
    anchor_cache: RwLock<LruCache<String, PersonhoodAnchor>>,
    holder_cache: RwLock<LruCache<String, CommonsHolderRecord>>,
    charter_cache: RwLock<LruCache<String, Charter>>,
    steward_cache: RwLock<LruCache<String, StewardRecord>>,
    amendment_cache: RwLock<LruCache<String, Amendment>>,
    appeal_cache: RwLock<LruCache<String, Appeal>>,
    ceremony_cache: RwLock<LruCache<String, crate::api::sdis::enrollment::EnrollmentCeremony>>,
    enrollment_session_cache:
        RwLock<LruCache<String, crate::api::sdis::simple_enrollment::EnrollmentSession>>,
}

impl<S: CommonsStoreBackend> CommonsStore<S> {
    /// Create a new CommonsStore with default cache sizes
    pub fn new(store: Arc<S>) -> Self {
        Self::with_config(store, CacheConfig::default())
    }

    /// Create with custom cache configuration
    pub fn with_config(store: Arc<S>, config: CacheConfig) -> Self {
        // Helper to ensure cache size is at least 1
        // SAFETY: .max(1) ensures the value is always non-zero
        #[allow(clippy::unwrap_used)]
        fn cache_size(size: usize) -> NonZeroUsize {
            NonZeroUsize::new(size.max(1)).unwrap()
        }

        Self {
            store,
            anchor_cache: RwLock::new(LruCache::new(cache_size(config.anchor_cache_size))),
            holder_cache: RwLock::new(LruCache::new(cache_size(config.holder_cache_size))),
            charter_cache: RwLock::new(LruCache::new(cache_size(config.charter_cache_size))),
            steward_cache: RwLock::new(LruCache::new(cache_size(config.steward_cache_size))),
            amendment_cache: RwLock::new(LruCache::new(cache_size(config.amendment_cache_size))),
            appeal_cache: RwLock::new(LruCache::new(cache_size(config.appeal_cache_size))),
            ceremony_cache: RwLock::new(LruCache::new(cache_size(config.ceremony_cache_size))),
            enrollment_session_cache: RwLock::new(LruCache::new(cache_size(
                config.enrollment_session_cache_size,
            ))),
        }
    }

    /// Get reference to underlying backend
    pub fn backend(&self) -> &Arc<S> {
        &self.store
    }

    // ========================================================================
    // Generic Helpers
    // ========================================================================

    fn make_key(prefix: &[u8], id: &str) -> Vec<u8> {
        let mut key = prefix.to_vec();
        key.extend_from_slice(id.as_bytes());
        key
    }

    fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(value).context("Serialization failed")
    }

    fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
        serde_json::from_slice(data).context("Deserialization failed")
    }

    // ========================================================================
    // PersonhoodAnchor Operations
    // ========================================================================

    /// Store a PersonhoodAnchor
    pub fn put_anchor(&self, anchor: &PersonhoodAnchor) -> Result<()> {
        let id = hex::encode(anchor.id());
        let did = anchor.to_did().to_string();

        // Store main record
        let key = Self::make_key(ANCHOR_PREFIX, &id);
        let value = Self::serialize(anchor)?;
        self.store.put(&key, &value)?;

        // Update DID index
        let did_key = Self::make_key(ANCHOR_BY_DID_PREFIX, &did);
        self.store.put(&did_key, id.as_bytes())?;

        // Update cache
        self.anchor_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Anchor cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(id.clone(), anchor.clone());

        debug!("Stored anchor: {}", id);
        Ok(())
    }

    /// Get a PersonhoodAnchor by ID
    pub fn get_anchor(&self, id: &str) -> Result<Option<PersonhoodAnchor>> {
        // Check cache
        if let Some(cached) = self
            .anchor_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Anchor cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        // Load from storage
        let key = Self::make_key(ANCHOR_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let anchor: PersonhoodAnchor = Self::deserialize(&value)?;
            self.anchor_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Anchor cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(id.to_string(), anchor.clone());
            return Ok(Some(anchor));
        }

        Ok(None)
    }

    /// Get anchor by DID
    pub fn get_anchor_by_did(&self, did: &str) -> Result<Option<PersonhoodAnchor>> {
        let did_key = Self::make_key(ANCHOR_BY_DID_PREFIX, did);
        if let Some(id_bytes) = self.store.get(&did_key)? {
            let id = String::from_utf8(id_bytes).context("Invalid anchor ID")?;
            return self.get_anchor(&id);
        }
        Ok(None)
    }

    /// Add a DID -> anchor_id index entry
    ///
    /// This is used when the enrollment DID differs from the anchor's internal DID.
    pub fn put_anchor_did_index(&self, did: &str, anchor_id: &str) -> Result<()> {
        let did_key = Self::make_key(ANCHOR_BY_DID_PREFIX, did);
        self.store.put(&did_key, anchor_id.as_bytes())?;
        Ok(())
    }

    /// Delete an anchor
    pub fn delete_anchor(&self, id: &str) -> Result<bool> {
        if let Some(anchor) = self.get_anchor(id)? {
            let did = anchor.to_did().to_string();

            // Delete main record
            let key = Self::make_key(ANCHOR_PREFIX, id);
            self.store.delete(&key)?;

            // Delete DID index
            let did_key = Self::make_key(ANCHOR_BY_DID_PREFIX, &did);
            self.store.delete(&did_key)?;

            // Remove from cache
            self.anchor_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Anchor cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .pop(id);

            debug!("Deleted anchor: {}", id);
            return Ok(true);
        }
        Ok(false)
    }

    /// List all anchors
    pub fn list_anchors(&self) -> Result<Vec<PersonhoodAnchor>> {
        let entries = self.store.scan(ANCHOR_PREFIX)?;
        let mut anchors = Vec::new();

        for (key, value) in entries {
            // Skip index entries
            if !key.starts_with(ANCHOR_BY_DID_PREFIX) {
                if let Ok(anchor) = Self::deserialize::<PersonhoodAnchor>(&value) {
                    anchors.push(anchor);
                }
            }
        }

        Ok(anchors)
    }

    // ========================================================================
    // CommonsHolderRecord Operations
    // ========================================================================

    /// Store a CommonsHolderRecord
    pub fn put_holder(&self, holder: &CommonsHolderRecord) -> Result<()> {
        let id = hex::encode(holder.id());
        let did = holder.holder_did.to_string();
        let anchor_id = hex::encode(holder.anchor_id);

        // Store main record
        let key = Self::make_key(HOLDER_PREFIX, &id);
        let value = Self::serialize(holder)?;
        self.store.put(&key, &value)?;

        // Update DID index
        let did_key = Self::make_key(HOLDER_BY_DID_PREFIX, &did);
        self.store.put(&did_key, id.as_bytes())?;

        // Update anchor index
        let anchor_key = Self::make_key(HOLDER_BY_ANCHOR_PREFIX, &anchor_id);
        self.store.put(&anchor_key, id.as_bytes())?;

        // Update cache
        self.holder_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Holder cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(id.clone(), holder.clone());

        debug!("Stored holder: {}", id);
        Ok(())
    }

    /// Get a CommonsHolderRecord by ID
    pub fn get_holder(&self, id: &str) -> Result<Option<CommonsHolderRecord>> {
        // Check cache
        if let Some(cached) = self
            .holder_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Holder cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        // Load from storage
        let key = Self::make_key(HOLDER_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let holder: CommonsHolderRecord = Self::deserialize(&value)?;
            self.holder_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Holder cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(id.to_string(), holder.clone());
            return Ok(Some(holder));
        }

        Ok(None)
    }

    /// Get holder by DID
    pub fn get_holder_by_did(&self, did: &str) -> Result<Option<CommonsHolderRecord>> {
        let did_key = Self::make_key(HOLDER_BY_DID_PREFIX, did);
        if let Some(id_bytes) = self.store.get(&did_key)? {
            let id = String::from_utf8(id_bytes).context("Invalid holder ID")?;
            return self.get_holder(&id);
        }
        Ok(None)
    }

    /// Get holder by anchor ID
    pub fn get_holder_by_anchor(&self, anchor_id: &str) -> Result<Option<CommonsHolderRecord>> {
        let anchor_key = Self::make_key(HOLDER_BY_ANCHOR_PREFIX, anchor_id);
        if let Some(id_bytes) = self.store.get(&anchor_key)? {
            let id = String::from_utf8(id_bytes).context("Invalid holder ID")?;
            return self.get_holder(&id);
        }
        Ok(None)
    }

    /// Delete a holder
    pub fn delete_holder(&self, id: &str) -> Result<bool> {
        if let Some(holder) = self.get_holder(id)? {
            let did = holder.holder_did.to_string();
            let anchor_id = hex::encode(holder.anchor_id);

            // Delete main record
            let key = Self::make_key(HOLDER_PREFIX, id);
            self.store.delete(&key)?;

            // Delete indexes
            let did_key = Self::make_key(HOLDER_BY_DID_PREFIX, &did);
            self.store.delete(&did_key)?;

            let anchor_key = Self::make_key(HOLDER_BY_ANCHOR_PREFIX, &anchor_id);
            self.store.delete(&anchor_key)?;

            // Remove from cache
            self.holder_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Holder cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .pop(id);

            debug!("Deleted holder: {}", id);
            return Ok(true);
        }
        Ok(false)
    }

    /// List all holders
    pub fn list_holders(&self) -> Result<Vec<CommonsHolderRecord>> {
        let entries = self.store.scan(HOLDER_PREFIX)?;
        let mut holders = Vec::new();

        for (key, value) in entries {
            // Skip index entries
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.contains("/by_did/") && !key_str.contains("/by_anchor/") {
                if let Ok(holder) = Self::deserialize::<CommonsHolderRecord>(&value) {
                    holders.push(holder);
                }
            }
        }

        Ok(holders)
    }

    // ========================================================================
    // Charter Operations
    // ========================================================================

    /// Store a Charter
    pub fn put_charter(&self, charter: &Charter) -> Result<()> {
        let id = charter.charter_id.to_hex();
        // Use domain_id directly (not full_domain_id which adds an extra prefix)
        let domain_id = &charter.domain_id;

        // Store main record
        let key = Self::make_key(CHARTER_PREFIX, &id);
        let value = Self::serialize(charter)?;
        self.store.put(&key, &value)?;

        // Update domain index
        let domain_key = Self::make_key(CHARTER_BY_DOMAIN_PREFIX, domain_id);
        self.store.put(&domain_key, id.as_bytes())?;

        // Update cache
        self.charter_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Charter cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(id.clone(), charter.clone());

        debug!("Stored charter: {}", id);
        Ok(())
    }

    /// Get a Charter by ID
    pub fn get_charter(&self, id: &str) -> Result<Option<Charter>> {
        // Check cache
        if let Some(cached) = self
            .charter_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Charter cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        // Load from storage
        let key = Self::make_key(CHARTER_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let charter: Charter = Self::deserialize(&value)?;
            self.charter_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Charter cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(id.to_string(), charter.clone());
            return Ok(Some(charter));
        }

        Ok(None)
    }

    /// Get charter by domain
    pub fn get_charter_by_domain(&self, domain_id: &str) -> Result<Option<Charter>> {
        let domain_key = Self::make_key(CHARTER_BY_DOMAIN_PREFIX, domain_id);
        if let Some(id_bytes) = self.store.get(&domain_key)? {
            let id = String::from_utf8(id_bytes).context("Invalid charter ID")?;
            return self.get_charter(&id);
        }
        Ok(None)
    }

    /// List all charters
    pub fn list_charters(&self) -> Result<Vec<Charter>> {
        let entries = self.store.scan(CHARTER_PREFIX)?;
        let mut charters = Vec::new();

        for (key, value) in entries {
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.contains("/by_domain/") {
                if let Ok(charter) = Self::deserialize::<Charter>(&value) {
                    charters.push(charter);
                }
            }
        }

        Ok(charters)
    }

    // ========================================================================
    // StewardRecord Operations
    // ========================================================================

    /// Store a StewardRecord
    pub fn put_steward(&self, steward: &StewardRecord) -> Result<()> {
        let id = steward.id().to_hex();
        let did = steward.holder_did.to_string();

        // Store main record
        let key = Self::make_key(STEWARD_PREFIX, &id);
        let value = Self::serialize(steward)?;
        self.store.put(&key, &value)?;

        // Update DID index
        let did_key = Self::make_key(STEWARD_BY_DID_PREFIX, &did);
        self.store.put(&did_key, id.as_bytes())?;

        // Update cache
        self.steward_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Steward cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(id.clone(), steward.clone());

        debug!("Stored steward: {}", id);
        Ok(())
    }

    /// Get a StewardRecord by ID
    pub fn get_steward(&self, id: &str) -> Result<Option<StewardRecord>> {
        // Check cache
        if let Some(cached) = self
            .steward_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Steward cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        // Load from storage
        let key = Self::make_key(STEWARD_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let steward: StewardRecord = Self::deserialize(&value)?;
            self.steward_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Steward cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(id.to_string(), steward.clone());
            return Ok(Some(steward));
        }

        Ok(None)
    }

    /// Get steward by DID
    pub fn get_steward_by_did(&self, did: &str) -> Result<Option<StewardRecord>> {
        let did_key = Self::make_key(STEWARD_BY_DID_PREFIX, did);
        if let Some(id_bytes) = self.store.get(&did_key)? {
            let id = String::from_utf8(id_bytes).context("Invalid steward ID")?;
            return self.get_steward(&id);
        }
        Ok(None)
    }

    /// List all stewards
    pub fn list_stewards(&self) -> Result<Vec<StewardRecord>> {
        let entries = self.store.scan(STEWARD_PREFIX)?;
        let mut stewards = Vec::new();

        for (key, value) in entries {
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.contains("/by_did/") {
                if let Ok(steward) = Self::deserialize::<StewardRecord>(&value) {
                    stewards.push(steward);
                }
            }
        }

        Ok(stewards)
    }

    // ========================================================================
    // Amendment Operations
    // ========================================================================

    /// Store an Amendment
    pub fn put_amendment(&self, amendment: &Amendment) -> Result<()> {
        let id = amendment.id.to_hex();

        let key = Self::make_key(AMENDMENT_PREFIX, &id);
        let value = Self::serialize(amendment)?;
        self.store.put(&key, &value)?;

        self.amendment_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Amendment cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(id.clone(), amendment.clone());

        debug!("Stored amendment: {}", id);
        Ok(())
    }

    /// Get an Amendment by ID
    pub fn get_amendment(&self, id: &str) -> Result<Option<Amendment>> {
        if let Some(cached) = self
            .amendment_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Amendment cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        let key = Self::make_key(AMENDMENT_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let amendment: Amendment = Self::deserialize(&value)?;
            self.amendment_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Amendment cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(id.to_string(), amendment.clone());
            return Ok(Some(amendment));
        }

        Ok(None)
    }

    /// List all amendments
    pub fn list_amendments(&self) -> Result<Vec<Amendment>> {
        let entries = self.store.scan(AMENDMENT_PREFIX)?;
        let mut amendments = Vec::new();

        for (_key, value) in entries {
            if let Ok(amendment) = Self::deserialize::<Amendment>(&value) {
                amendments.push(amendment);
            }
        }

        Ok(amendments)
    }

    // ========================================================================
    // Appeal Operations
    // ========================================================================

    /// Store an Appeal
    pub fn put_appeal(&self, appeal: &Appeal) -> Result<()> {
        let id = appeal.id.to_hex();

        let key = Self::make_key(APPEAL_PREFIX, &id);
        let value = Self::serialize(appeal)?;
        self.store.put(&key, &value)?;

        self.appeal_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Appeal cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(id.clone(), appeal.clone());

        debug!("Stored appeal: {}", id);
        Ok(())
    }

    /// Get an Appeal by ID
    pub fn get_appeal(&self, id: &str) -> Result<Option<Appeal>> {
        if let Some(cached) = self
            .appeal_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Appeal cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        let key = Self::make_key(APPEAL_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let appeal: Appeal = Self::deserialize(&value)?;
            self.appeal_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Appeal cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(id.to_string(), appeal.clone());
            return Ok(Some(appeal));
        }

        Ok(None)
    }

    /// List all appeals
    pub fn list_appeals(&self) -> Result<Vec<Appeal>> {
        let entries = self.store.scan(APPEAL_PREFIX)?;
        let mut appeals = Vec::new();

        for (_key, value) in entries {
            if let Ok(appeal) = Self::deserialize::<Appeal>(&value) {
                appeals.push(appeal);
            }
        }

        Ok(appeals)
    }

    // ========================================================================
    // Revocation Operations
    // ========================================================================

    /// Store a RevocationRecord
    pub fn put_revocation(&self, revocation: &RevocationRecord) -> Result<()> {
        let id = hex::encode(revocation.revocation_id);

        let key = Self::make_key(REVOCATION_PREFIX, &id);
        let value = Self::serialize(revocation)?;
        self.store.put(&key, &value)?;

        debug!("Stored revocation: {}", id);
        Ok(())
    }

    /// Get a RevocationRecord by ID
    pub fn get_revocation(&self, id: &str) -> Result<Option<RevocationRecord>> {
        let key = Self::make_key(REVOCATION_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let revocation: RevocationRecord = Self::deserialize(&value)?;
            return Ok(Some(revocation));
        }
        Ok(None)
    }

    /// List all revocations
    pub fn list_revocations(&self) -> Result<Vec<RevocationRecord>> {
        let entries = self.store.scan(REVOCATION_PREFIX)?;
        let mut revocations = Vec::new();

        for (_key, value) in entries {
            if let Ok(revocation) = Self::deserialize::<RevocationRecord>(&value) {
                revocations.push(revocation);
            }
        }

        Ok(revocations)
    }

    // ========================================================================
    // Cache Management
    // ========================================================================

    /// Clear all caches
    pub fn clear_caches(&self) {
        self.anchor_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Anchor cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
        self.holder_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Holder cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
        self.charter_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Charter cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
        self.steward_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Steward cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
        self.amendment_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Amendment cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
        self.appeal_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Appeal cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
        self.ceremony_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Ceremony cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
        self.enrollment_session_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Enrollment session cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
    }

    // ========================================================================
    // EnrollmentCeremony Operations
    // ========================================================================

    /// Store an EnrollmentCeremony
    pub fn put_ceremony(
        &self,
        id: &str,
        ceremony: &crate::api::sdis::enrollment::EnrollmentCeremony,
    ) -> Result<()> {
        let key = Self::make_key(CEREMONY_PREFIX, id);
        let value = Self::serialize(ceremony)?;
        self.store.put(&key, &value)?;

        self.ceremony_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Ceremony cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(id.to_string(), ceremony.clone());

        debug!("Stored ceremony: {}", id);
        Ok(())
    }

    /// Get an EnrollmentCeremony by ID
    pub fn get_ceremony(
        &self,
        id: &str,
    ) -> Result<Option<crate::api::sdis::enrollment::EnrollmentCeremony>> {
        if let Some(cached) = self
            .ceremony_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Ceremony cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        let key = Self::make_key(CEREMONY_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let ceremony: crate::api::sdis::enrollment::EnrollmentCeremony =
                Self::deserialize(&value)?;
            self.ceremony_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Ceremony cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(id.to_string(), ceremony.clone());
            return Ok(Some(ceremony));
        }

        Ok(None)
    }

    /// Update an existing ceremony
    pub fn update_ceremony(
        &self,
        id: &str,
        ceremony: &crate::api::sdis::enrollment::EnrollmentCeremony,
    ) -> Result<()> {
        self.put_ceremony(id, ceremony)
    }

    /// Delete a ceremony
    pub fn delete_ceremony(&self, id: &str) -> Result<bool> {
        let key = Self::make_key(CEREMONY_PREFIX, id);
        if self.store.exists(&key)? {
            self.store.delete(&key)?;
            self.ceremony_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Ceremony cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .pop(id);
            debug!("Deleted ceremony: {}", id);
            return Ok(true);
        }
        Ok(false)
    }

    /// List all ceremonies
    pub fn list_ceremonies(
        &self,
    ) -> Result<Vec<(String, crate::api::sdis::enrollment::EnrollmentCeremony)>> {
        let entries = self.store.scan(CEREMONY_PREFIX)?;
        let mut ceremonies = Vec::new();

        for (key, value) in entries {
            // Extract ID from key
            let key_str = String::from_utf8_lossy(&key);
            if let Some(id) = key_str.strip_prefix("commons/ceremonies/") {
                if let Ok(ceremony) =
                    Self::deserialize::<crate::api::sdis::enrollment::EnrollmentCeremony>(&value)
                {
                    ceremonies.push((id.to_string(), ceremony));
                }
            }
        }

        Ok(ceremonies)
    }

    // ========================================================================
    // EnrollmentSession Operations (Simple Enrollment)
    // ========================================================================

    /// Store an EnrollmentSession
    pub fn put_enrollment_session(
        &self,
        id: &str,
        session: &crate::api::sdis::simple_enrollment::EnrollmentSession,
    ) -> Result<()> {
        let key = Self::make_key(ENROLLMENT_SESSION_PREFIX, id);
        let value = Self::serialize(session)?;
        self.store.put(&key, &value)?;

        self.enrollment_session_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Enrollment session cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(id.to_string(), session.clone());

        debug!("Stored enrollment session: {}", id);
        Ok(())
    }

    /// Get an EnrollmentSession by ID
    pub fn get_enrollment_session(
        &self,
        id: &str,
    ) -> Result<Option<crate::api::sdis::simple_enrollment::EnrollmentSession>> {
        if let Some(cached) = self
            .enrollment_session_cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Enrollment session cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        let key = Self::make_key(ENROLLMENT_SESSION_PREFIX, id);
        if let Some(value) = self.store.get(&key)? {
            let session: crate::api::sdis::simple_enrollment::EnrollmentSession =
                Self::deserialize(&value)?;
            self.enrollment_session_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Enrollment session cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(id.to_string(), session.clone());
            return Ok(Some(session));
        }

        Ok(None)
    }

    /// Update an existing enrollment session
    pub fn update_enrollment_session(
        &self,
        id: &str,
        session: &crate::api::sdis::simple_enrollment::EnrollmentSession,
    ) -> Result<()> {
        self.put_enrollment_session(id, session)
    }

    /// Delete an enrollment session
    pub fn delete_enrollment_session(&self, id: &str) -> Result<bool> {
        let key = Self::make_key(ENROLLMENT_SESSION_PREFIX, id);
        if self.store.exists(&key)? {
            self.store.delete(&key)?;
            self.enrollment_session_cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Enrollment session cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .pop(id);
            debug!("Deleted enrollment session: {}", id);
            return Ok(true);
        }
        Ok(false)
    }

    /// List all enrollment sessions
    pub fn list_enrollment_sessions(
        &self,
    ) -> Result<
        Vec<(
            String,
            crate::api::sdis::simple_enrollment::EnrollmentSession,
        )>,
    > {
        let entries = self.store.scan(ENROLLMENT_SESSION_PREFIX)?;
        let mut sessions = Vec::new();

        for (key, value) in entries {
            let key_str = String::from_utf8_lossy(&key);
            if let Some(id) = key_str.strip_prefix("commons/enrollment_sessions/") {
                if let Ok(session) = Self::deserialize::<
                    crate::api::sdis::simple_enrollment::EnrollmentSession,
                >(&value)
                {
                    sessions.push((id.to_string(), session));
                }
            }
        }

        Ok(sessions)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn create_test_store() -> CommonsStore<InMemoryCommonsStore> {
        let backend = Arc::new(InMemoryCommonsStore::new());
        CommonsStore::new(backend)
    }

    #[test]
    fn test_in_memory_backend() {
        let backend = InMemoryCommonsStore::new();

        // Put and get
        backend.put(b"key1", b"value1").unwrap();
        let value = backend.get(b"key1").unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));

        // Delete
        backend.delete(b"key1").unwrap();
        let value = backend.get(b"key1").unwrap();
        assert!(value.is_none());

        // Scan
        backend.put(b"prefix/a", b"1").unwrap();
        backend.put(b"prefix/b", b"2").unwrap();
        backend.put(b"other/c", b"3").unwrap();

        let results = backend.scan(b"prefix/").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_holder_operations() {
        use icn_identity::{CommonsHolderRecord, CommonsRights, HolderStatus, POPLevel};

        let store = create_test_store();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let holder = CommonsHolderRecord {
            holder_id: [1u8; 32],
            anchor_id: [2u8; 32],
            holder_did: did.clone(),
            display_name: Some("Test User".to_string()),
            status: HolderStatus::Active,
            personhood_level: POPLevel::Strong,
            affiliations: Vec::new(),
            baseline_rights: CommonsRights::default(),
            created_at: 12345,
            last_review_at: None,
            updated_at: 12345,
        };

        // Store
        store.put_holder(&holder).unwrap();

        // Get by ID
        let id = hex::encode(holder.id());
        let retrieved = store.get_holder(&id).unwrap().unwrap();
        assert_eq!(retrieved.id(), holder.id());

        // Get by DID
        let by_did = store.get_holder_by_did(&did.to_string()).unwrap().unwrap();
        assert_eq!(by_did.id(), holder.id());

        // Get by anchor
        let anchor_id = hex::encode(holder.anchor_id);
        let by_anchor = store.get_holder_by_anchor(&anchor_id).unwrap().unwrap();
        assert_eq!(by_anchor.id(), holder.id());

        // List
        let holders = store.list_holders().unwrap();
        assert_eq!(holders.len(), 1);

        // Delete
        let deleted = store.delete_holder(&id).unwrap();
        assert!(deleted);
        assert!(store.get_holder(&id).unwrap().is_none());
    }

    #[test]
    fn test_charter_operations() {
        use icn_governance::{DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType};

        let store = create_test_store();

        let charter = Charter::new(
            OrgType::Cooperative,
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            GovernanceConfig::cooperative_default(),
            MembershipPolicy::default(),
            DisputePolicy::default(),
        );

        let id = charter.charter_id.to_hex();
        let domain = &charter.domain_id;

        // Store
        store.put_charter(&charter).unwrap();

        // Get by ID
        let retrieved = store.get_charter(&id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Test Cooperative");

        // Get by domain (uses domain_id, not full_domain_id)
        let by_domain = store.get_charter_by_domain(domain).unwrap().unwrap();
        assert_eq!(by_domain.charter_id.to_hex(), id);

        // List
        let charters = store.list_charters().unwrap();
        assert_eq!(charters.len(), 1);
    }

    #[test]
    fn test_cache_behavior() {
        use icn_governance::{DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType};

        let backend = Arc::new(InMemoryCommonsStore::new());
        let store = CommonsStore::new(backend.clone());

        let charter = Charter::new(
            OrgType::Cooperative,
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            GovernanceConfig::cooperative_default(),
            MembershipPolicy::default(),
            DisputePolicy::default(),
        );
        let id = charter.charter_id.to_hex();

        // Store (populates cache)
        store.put_charter(&charter).unwrap();

        // First get (from cache or storage)
        let _ = store.get_charter(&id).unwrap();

        // Clear backend data but keep cache
        backend.clear();

        // Should still get from cache
        let cached = store.get_charter(&id).unwrap();
        assert!(cached.is_some());

        // Clear cache
        store.clear_caches();

        // Now should be gone
        let gone = store.get_charter(&id).unwrap();
        assert!(gone.is_none());
    }

    // ========================================================================
    // Sled Backend Tests (feature-gated)
    // ========================================================================
    // Enable with: cargo test -p icn-gateway --features sled-storage
    #[cfg(feature = "sled-storage")]
    mod sled_tests {
        use super::*;
        use crate::commons_store::SledCommonsStore;

        fn create_sled_store() -> CommonsStore<SledCommonsStore> {
            let backend = Arc::new(SledCommonsStore::temporary().unwrap());
            CommonsStore::new(backend)
        }

        #[test]
        fn test_sled_backend_basic() {
            let backend = SledCommonsStore::temporary().unwrap();

            // Put and get
            backend.put(b"key1", b"value1").unwrap();
            let value = backend.get(b"key1").unwrap();
            assert_eq!(value, Some(b"value1".to_vec()));

            // Delete
            backend.delete(b"key1").unwrap();
            let value = backend.get(b"key1").unwrap();
            assert!(value.is_none());

            // Scan
            backend.put(b"prefix/a", b"1").unwrap();
            backend.put(b"prefix/b", b"2").unwrap();
            backend.put(b"other/c", b"3").unwrap();

            let results = backend.scan(b"prefix/").unwrap();
            assert_eq!(results.len(), 2);
        }

        #[test]
        fn test_sled_holder_operations() {
            use icn_identity::{CommonsHolderRecord, CommonsRights, HolderStatus, POPLevel};

            let store = create_sled_store();
            let keypair = KeyPair::generate().unwrap();
            let did = keypair.did().clone();

            let holder = CommonsHolderRecord {
                holder_id: [1u8; 32],
                anchor_id: [2u8; 32],
                holder_did: did.clone(),
                display_name: Some("Sled Test User".to_string()),
                status: HolderStatus::Active,
                personhood_level: POPLevel::Strong,
                affiliations: Vec::new(),
                baseline_rights: CommonsRights::default(),
                created_at: 12345,
                last_review_at: None,
                updated_at: 12345,
            };

            // Store
            store.put_holder(&holder).unwrap();

            // Get by ID
            let id = hex::encode(holder.id());
            let retrieved = store.get_holder(&id).unwrap().unwrap();
            assert_eq!(retrieved.id(), holder.id());

            // Get by DID
            let by_did = store.get_holder_by_did(&did.to_string()).unwrap().unwrap();
            assert_eq!(by_did.id(), holder.id());

            // Get by anchor
            let anchor_id = hex::encode(holder.anchor_id);
            let by_anchor = store.get_holder_by_anchor(&anchor_id).unwrap().unwrap();
            assert_eq!(by_anchor.id(), holder.id());

            // List
            let holders = store.list_holders().unwrap();
            assert_eq!(holders.len(), 1);

            // Delete
            let deleted = store.delete_holder(&id).unwrap();
            assert!(deleted);
            assert!(store.get_holder(&id).unwrap().is_none());
        }

        #[test]
        fn test_sled_charter_operations() {
            use icn_governance::{DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType};

            let store = create_sled_store();

            let charter = Charter::new(
                OrgType::Cooperative,
                "sled-test-coop".to_string(),
                "Sled Test Cooperative".to_string(),
                GovernanceConfig::cooperative_default(),
                MembershipPolicy::default(),
                DisputePolicy::default(),
            );

            let id = charter.charter_id.to_hex();
            let domain = &charter.domain_id;

            // Store
            store.put_charter(&charter).unwrap();

            // Get by ID
            let retrieved = store.get_charter(&id).unwrap().unwrap();
            assert_eq!(retrieved.name, "Sled Test Cooperative");

            // Get by domain
            let by_domain = store.get_charter_by_domain(domain).unwrap().unwrap();
            assert_eq!(by_domain.charter_id.to_hex(), id);

            // List
            let charters = store.list_charters().unwrap();
            assert_eq!(charters.len(), 1);
        }

        #[test]
        fn test_sled_persistence() {
            use icn_governance::{DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType};
            use std::path::PathBuf;
            use tempfile::tempdir;

            // Create a temporary directory for the database
            let temp_dir = tempdir().unwrap();
            let db_path: PathBuf = temp_dir.path().join("commons_test.db");

            let charter_id: String;
            let charter_name = "Persistent Coop";

            // First: Create database and store data
            {
                let backend = Arc::new(SledCommonsStore::open(&db_path).unwrap());
                let store = CommonsStore::new(backend.clone());

                let charter = Charter::new(
                    OrgType::Cooperative,
                    "persistent-coop".to_string(),
                    charter_name.to_string(),
                    GovernanceConfig::cooperative_default(),
                    MembershipPolicy::default(),
                    DisputePolicy::default(),
                );
                charter_id = charter.charter_id.to_hex();

                store.put_charter(&charter).unwrap();

                // Explicitly flush
                backend.flush().unwrap();
            } // Store and backend dropped here

            // Second: Reopen database and verify data persisted
            {
                let backend = Arc::new(SledCommonsStore::open(&db_path).unwrap());
                let store = CommonsStore::new(backend);

                let retrieved = store.get_charter(&charter_id).unwrap();
                assert!(
                    retrieved.is_some(),
                    "Charter should persist across restarts"
                );
                assert_eq!(retrieved.unwrap().name, charter_name);
            }
        }

        #[test]
        fn test_sled_size_on_disk() {
            let backend = SledCommonsStore::temporary().unwrap();

            // Add some data
            for i in 0..100 {
                let key = format!("key_{i}");
                let value = format!("value_{i}");
                backend.put(key.as_bytes(), value.as_bytes()).unwrap();
            }

            backend.flush().unwrap();

            // Should report some size
            let size = backend.size_on_disk().unwrap();
            assert!(size > 0, "Database should have non-zero size after writes");
        }
    }
}
