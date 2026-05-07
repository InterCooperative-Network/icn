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
// Note: ceremony and enrollment session storage stays in icn-gateway (gateway-local types).

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

    /// Flush pending writes to durable storage. Default is a no-op for in-memory backends.
    fn flush(&self) -> Result<()> {
        Ok(())
    }
}

/// Allow `Arc<dyn CommonsStoreBackend>` to be used as a backend directly.
/// This enables type-erased `CommonsManager` construction without changing handler signatures.
impl CommonsStoreBackend for Arc<dyn CommonsStoreBackend> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.as_ref().get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.as_ref().put(key, value)
    }
    fn delete(&self, key: &[u8]) -> Result<()> {
        self.as_ref().delete(key)
    }
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.as_ref().scan(prefix)
    }
    fn flush(&self) -> Result<()> {
        self.as_ref().flush()
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
/// Sled is an unconditional dependency of `icn-commons`. No feature flag is
/// required to use this backend.
pub struct SledCommonsStore {
    db: sled::Db,
}

/// Returns `true` when a `sled::Error` indicates that the OS file lock
/// for the database is currently held by another process (or the
/// previous in-process flusher hasn't exited yet).
///
/// Two shapes are matched, both representing the same underlying race:
///
/// 1. **Bare `WouldBlock`.** Some platforms / future sled versions
///    can surface the kernel's `EAGAIN` from `flock(LOCK_NB)`
///    directly as `Error::Io(io::Error::new(ErrorKind::WouldBlock, ...))`.
/// 2. **Sled 0.34's wrapped form.** sled 0.34.7 wraps the
///    `try_lock_exclusive` failure as
///    `Error::Io(io::Error::new(ErrorKind::Other, format!("could not
///    acquire lock on {:?}: {:?}", path, e)))`. The outer `kind()` is
///    `Other`, not `WouldBlock`. The classifier must detect this
///    wrapped form by message substring, otherwise the retry loop in
///    `SledCommonsStore::open` would never trigger for the actual
///    real-world race this primitive is meant to defend against.
///    See `sled-0.34.7/src/config.rs::Config::try_lock` for the wrap
///    site.
///
/// Other I/O errors (`NotFound`, `PermissionDenied`, etc.) and
/// non-I/O sled errors do not match.
fn is_sled_lock_contention(e: &sled::Error) -> bool {
    match e {
        sled::Error::Io(io_err) => match io_err.kind() {
            // Direct `EAGAIN` surface (some platforms / future sled).
            std::io::ErrorKind::WouldBlock => true,
            // Sled 0.34's wrap: `kind = Other` with a message that
            // begins with "could not acquire lock". The substring
            // match is intentionally narrow; any other `Other` I/O
            // error must NOT be retried.
            std::io::ErrorKind::Other => {
                let msg = io_err.to_string();
                msg.contains("could not acquire lock")
            }
            _ => false,
        },
        _ => false,
    }
}

impl SledCommonsStore {
    /// Open a persistent Sled database at the given path.
    ///
    /// Retries briefly on lock contention. sled 0.34 spawns a
    /// background flusher thread that holds the OS `flock(LOCK_EX)` on
    /// the database directory's lockfile. On `Db::drop`, sled signals
    /// the flusher to stop but does **not** synchronously join it —
    /// the flock is released only when the flusher actually exits.
    /// Without a retry, an immediate-reopen pattern (drop a manager,
    /// open a new one against the same path) races against sled's
    /// internal shutdown. sled 0.34.7 surfaces the `flock(LOCK_NB)`
    /// failure as a wrapped `io::Error` with `kind = Other` and a
    /// message beginning with "could not acquire lock"; see
    /// `is_sled_lock_contention` for the exact match. This is
    /// load-dependent and primarily fires on busy CI runners but is
    /// also reachable in production daemon-restart scenarios.
    ///
    /// The retry loop:
    /// - retries only on lock-contention errors per
    ///   `is_sled_lock_contention`; every other error path is
    ///   surfaced immediately (`NotFound`, `PermissionDenied`,
    ///   deserialization, corruption, etc.);
    /// - performs at most one cold-open attempt + `MAX_RETRIES`
    ///   sleeping retries (so up to `1 + MAX_RETRIES` total
    ///   `sled::open()` calls — currently up to 9);
    /// - uses exponential backoff capped at a `MAX_TOTAL_BACKOFF_MS`
    ///   total sleep budget (500ms) so a genuinely-stuck lockfile
    ///   fails fast;
    /// - is a no-op on the cold-open path (single attempt, no sleep).
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        // Number of *retries* after the initial cold-open attempt.
        // Total `sled::open()` calls in the worst case is therefore
        // `1 + MAX_RETRIES`. Naming this MAX_RETRIES (rather than
        // MAX_ATTEMPTS) matches the loop semantics: `attempt` counts
        // how many retries have already slept.
        const MAX_RETRIES: u32 = 8;
        const INITIAL_BACKOFF_MS: u64 = 10;
        const MAX_TOTAL_BACKOFF_MS: u64 = 500;

        let path = path.as_ref();
        let mut retries: u32 = 0;
        let mut total_slept_ms: u64 = 0;
        loop {
            match sled::open(path) {
                Ok(db) => return Ok(SledCommonsStore { db }),
                Err(e)
                    if is_sled_lock_contention(&e)
                        && retries < MAX_RETRIES
                        && total_slept_ms < MAX_TOTAL_BACKOFF_MS =>
                {
                    let remaining_budget = MAX_TOTAL_BACKOFF_MS - total_slept_ms;
                    let backoff_ms = (INITIAL_BACKOFF_MS << retries).min(remaining_budget);
                    debug!(
                        retry = retries,
                        backoff_ms,
                        path = ?path,
                        "Sled open hit lock-contention; backing off and retrying"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                    total_slept_ms += backoff_ms;
                    retries += 1;
                }
                Err(e) => return Err(e).context("Failed to open Sled database"),
            }
        }
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

    fn flush(&self) -> Result<()> {
        self.db.flush().context("Failed to flush Sled database")?;
        Ok(())
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
        }
    }
}

/// High-level Commons storage with caching.
///
/// `S` is the storage backend. When used from `CommonsInner`, `S` is
/// `Arc<dyn CommonsStoreBackend>` (a single Arc wrapping the backend). The
/// backend is stored as `S` directly — not wrapped in an additional `Arc` —
/// so callers that already pass an `Arc` do not end up with a nested
/// `Arc<Arc<...>>`.
pub struct CommonsStore<S: CommonsStoreBackend> {
    store: S,
    // Caches for frequently accessed records
    anchor_cache: RwLock<LruCache<String, PersonhoodAnchor>>,
    holder_cache: RwLock<LruCache<String, CommonsHolderRecord>>,
    charter_cache: RwLock<LruCache<String, Charter>>,
    steward_cache: RwLock<LruCache<String, StewardRecord>>,
    amendment_cache: RwLock<LruCache<String, Amendment>>,
    appeal_cache: RwLock<LruCache<String, Appeal>>,
}

impl<S: CommonsStoreBackend> CommonsStore<S> {
    /// Create a new CommonsStore with default cache sizes.
    ///
    /// Pass the backend directly. If the backend is already an `Arc<B>`,
    /// pass the `Arc` — it will not be double-wrapped.
    pub fn new(store: S) -> Self {
        Self::with_config(store, CacheConfig::default())
    }

    /// Create with custom cache configuration
    pub fn with_config(store: S, config: CacheConfig) -> Self {
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
        }
    }

    /// Get reference to underlying backend
    pub fn backend(&self) -> &S {
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
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;

    /// Deterministic end-to-end test of the retry path through real
    /// sled lock contention.
    ///
    /// A "blocker" `sled::Db` is opened first, holding the OS
    /// `flock(LOCK_EX)` on the database directory's lockfile. A
    /// background thread is scheduled to drop the blocker after a
    /// short delay, simulating the previous instance's shutdown
    /// (including sled's own flusher-thread shutdown delay). The main
    /// thread then calls `SledCommonsStore::open` against the same
    /// path. Without the retry loop this would surface sled 0.34's
    /// wrapped lock-contention error immediately. With the retry
    /// loop, subsequent attempts succeed once the blocker thread has
    /// dropped the prior `Db` and sled has released the kernel lock.
    ///
    /// This test is deterministic by construction: the blocker is a
    /// real sled handle (not a synthetic error), the timing is
    /// well within the configured retry budget (`MAX_TOTAL_BACKOFF_MS`
    /// = 500ms), and we assert the blocker actually dropped before
    /// the main thread observed success.
    #[test]
    fn sled_open_retries_through_flock_contention_to_success() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("commons.sled");

        // Open the blocker first — this acquires the OS flock.
        let blocker = sled::open(&path).expect("blocker open should succeed");

        // Confirm the lock is currently held: a second sled::open()
        // against the same path must fail with a lock-contention
        // error that our classifier recognises. We test this through
        // the inherent `sled::open()` (not `SledCommonsStore::open`)
        // so the assertion is independent of the retry loop being
        // tested.
        let immediate_attempt = sled::open(&path);
        match immediate_attempt {
            Err(e) => assert!(
                is_sled_lock_contention(&e),
                "while blocker holds the flock, the immediate-open \
                 attempt must surface a lock-contention error our \
                 classifier recognises; got: {e:?}"
            ),
            Ok(_unexpected) => panic!(
                "while blocker holds the flock, sled::open must fail; \
                 got Ok unexpectedly"
            ),
        }

        // Schedule the blocker drop on a background thread. The
        // delay is comfortably less than the retry budget but
        // greater than the initial backoff so the retry loop is
        // forced to spin at least once.
        let blocker_dropped = Arc::new(AtomicBool::new(false));
        let bd_clone = blocker_dropped.clone();
        let blocker_handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(blocker);
            bd_clone.store(true, Ordering::SeqCst);
        });

        // Call into the retry loop. This must succeed within the
        // configured retry budget.
        let store = SledCommonsStore::open(&path).expect(
            "SledCommonsStore::open must succeed via the retry loop \
             once the blocker thread releases the flock",
        );

        // The blocker thread must have actually run and dropped
        // before the main thread observed success.
        blocker_handle.join().expect("blocker thread panicked");
        assert!(
            blocker_dropped.load(Ordering::SeqCst),
            "blocker drop must have executed before SledCommonsStore::open returned"
        );

        drop(store);
    }

    /// The lock-contention classifier must match both shapes the
    /// real sled stack can surface for a held flock:
    /// (a) bare `io::ErrorKind::WouldBlock` (some platforms / future
    ///     sled versions / direct `EAGAIN` surface), and
    /// (b) sled 0.34's wrapped form: `kind = Other` with a message
    ///     that begins with `"could not acquire lock"` (the actual
    ///     production case in this workspace, see
    ///     `sled-0.34.7/src/config.rs::Config::try_lock`).
    ///
    /// All other I/O errors (`NotFound`, `PermissionDenied`,
    /// generic `Other` without the lock prefix, etc.) and non-I/O
    /// sled errors must NOT match — otherwise the retry loop would
    /// mask genuine failures.
    #[test]
    fn is_sled_lock_contention_classifier_matches_both_shapes() {
        // Shape (a): bare WouldBlock.
        let would_block = sled::Error::Io(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "simulated EAGAIN from flock",
        ));
        assert!(
            is_sled_lock_contention(&would_block),
            "bare WouldBlock must be classified as lock contention"
        );

        // Shape (b): sled 0.34's wrap. Reproduces the exact format
        // string from `sled-0.34.7/src/config.rs` so the substring
        // match is pinned to real production wording.
        let sled_wrapped = sled::Error::Io(std::io::Error::other(
            "could not acquire lock on \"/tmp/.tmpXXXXXX/db\": \
             Os { code: 11, kind: WouldBlock, message: \"Resource temporarily unavailable\" }",
        ));
        assert!(
            is_sled_lock_contention(&sled_wrapped),
            "sled 0.34's wrapped lock-contention error must be \
             classified as lock contention"
        );

        // Negative: another `Other`-kind I/O error without the lock
        // prefix must NOT match — the substring check is intentionally
        // narrow.
        let other_unrelated = sled::Error::Io(std::io::Error::other("some unrelated I/O failure"));
        assert!(
            !is_sled_lock_contention(&other_unrelated),
            "generic `Other` I/O errors without the lock prefix must \
             NOT be classified as lock contention"
        );

        // Negative: PermissionDenied.
        let permission_denied = sled::Error::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "simulated EACCES",
        ));
        assert!(
            !is_sled_lock_contention(&permission_denied),
            "PermissionDenied must NOT be classified as lock contention"
        );

        // Negative: NotFound.
        let not_found = sled::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "simulated ENOENT",
        ));
        assert!(
            !is_sled_lock_contention(&not_found),
            "NotFound must NOT be classified as lock contention"
        );
    }
}
