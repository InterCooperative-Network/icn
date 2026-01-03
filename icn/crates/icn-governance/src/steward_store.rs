//! Steward storage layer
//!
//! Provides persistent storage for StewardRecords with:
//! - Full CRUD operations
//! - Indexes (by DID, holder, jurisdiction, status)
//! - LRU caching for performance
//! - Lifecycle operations (suspend, reinstate, retire, revoke)
//!
//! ## Lock Poisoning Handling
//!
//! This module uses two patterns for lock poison recovery:
//!
//! - **Backend operations** (get, set, delete, list_keys): Fail-fast with error.
//!   Poisoned locks indicate a prior panic and possibly corrupt state.
//!   Silent recovery would risk data integrity issues.
//!
//! - **Cache operations**: Graceful degradation. Cache is non-critical; poison
//!   is logged but the operation continues using the backend directly.

use crate::steward::{StewardId, StewardRecord, StewardStatus};
use async_trait::async_trait;
use icn_identity::Did;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Storage key prefixes
const PREFIX_STEWARD: &[u8] = b"steward/records/";
const PREFIX_BY_DID: &[u8] = b"steward/by_did/";
const PREFIX_BY_HOLDER: &[u8] = b"steward/by_holder/";
const PREFIX_BY_JURISDICTION: &[u8] = b"steward/by_jurisdiction/";
const PREFIX_BY_STATUS: &[u8] = b"steward/status/";

/// Backend trait for steward storage
#[async_trait]
pub trait StewardStoreBackend: Send + Sync {
    /// Get a value by key
    async fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>>;

    /// Set a key-value pair
    async fn set(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()>;

    /// Delete a key
    async fn delete(&self, key: &[u8]) -> anyhow::Result<()>;

    /// List keys with a given prefix
    async fn list_keys(&self, prefix: &[u8]) -> anyhow::Result<Vec<Vec<u8>>>;
}

/// In-memory backend for testing
pub struct InMemoryStewardStore {
    data: RwLock<std::collections::BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl InMemoryStewardStore {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(std::collections::BTreeMap::new()),
        }
    }
}

impl Default for InMemoryStewardStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StewardStoreBackend for InMemoryStewardStore {
    async fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        let data = self.data.read().map_err(|_| {
            error!("Lock poisoned in steward store get - previous holder panicked");
            anyhow::anyhow!("Lock poisoned - store may contain corrupt state")
        })?;
        Ok(data.get(key).cloned())
    }

    async fn set(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        let mut data = self.data.write().map_err(|_| {
            error!("Lock poisoned in steward store set - previous holder panicked");
            anyhow::anyhow!("Lock poisoned - store may contain corrupt state")
        })?;
        data.insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    async fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        let mut data = self.data.write().map_err(|_| {
            error!("Lock poisoned in steward store delete - previous holder panicked");
            anyhow::anyhow!("Lock poisoned - store may contain corrupt state")
        })?;
        data.remove(key);
        Ok(())
    }

    async fn list_keys(&self, prefix: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
        let data = self.data.read().map_err(|_| {
            error!("Lock poisoned in steward store list_keys - previous holder panicked");
            anyhow::anyhow!("Lock poisoned - store may contain corrupt state")
        })?;
        let keys: Vec<Vec<u8>> = data
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }
}

/// Cached index entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    steward_id: [u8; 32],
}

/// Steward storage with caching and indexes
pub struct StewardStore<B: StewardStoreBackend> {
    backend: B,
    cache: RwLock<LruCache<[u8; 32], StewardRecord>>,
}

impl<B: StewardStoreBackend> StewardStore<B> {
    /// Create a new StewardStore with the given backend and cache size
    pub fn new(backend: B, cache_size: usize) -> Self {
        // SAFETY: .max(1) ensures the value is always non-zero
        #[allow(clippy::unwrap_used)]
        let cache_size = NonZeroUsize::new(cache_size.max(1)).unwrap();
        Self {
            backend,
            cache: RwLock::new(LruCache::new(cache_size)),
        }
    }

    /// Create with default cache size (1000 entries)
    pub fn with_default_cache(backend: B) -> Self {
        Self::new(backend, 1000)
    }

    /// Store a steward record
    pub async fn store(&self, record: &StewardRecord) -> anyhow::Result<()> {
        let steward_id = record.steward_id.as_bytes();

        // Serialize the record
        let data = serde_json::to_vec(record)?;

        // Store main record
        let mut key = PREFIX_STEWARD.to_vec();
        key.extend_from_slice(steward_id);
        self.backend.set(&key, &data).await?;

        // Update indexes
        self.update_indexes(record).await?;

        // Update cache (non-critical - skip on poison)
        if let Ok(mut cache) = self.cache.write() {
            cache.put(*steward_id, record.clone());
        } else {
            warn!("Cache lock poisoned in store - skipping cache update");
        }

        debug!("Stored steward record: {}", record.steward_id);
        Ok(())
    }

    /// Get a steward by ID
    pub async fn get(&self, steward_id: &StewardId) -> anyhow::Result<Option<StewardRecord>> {
        let id_bytes = steward_id.as_bytes();

        // Check cache first (non-critical - skip on poison)
        if let Ok(mut cache) = self.cache.write() {
            if let Some(record) = cache.get(id_bytes) {
                return Ok(Some(record.clone()));
            }
        } else {
            warn!("Cache lock poisoned in get - falling back to backend");
        }

        // Load from backend
        let mut key = PREFIX_STEWARD.to_vec();
        key.extend_from_slice(id_bytes);

        if let Some(data) = self.backend.get(&key).await? {
            let record: StewardRecord = serde_json::from_slice(&data)?;

            // Update cache (non-critical - skip on poison)
            if let Ok(mut cache) = self.cache.write() {
                cache.put(*id_bytes, record.clone());
            } else {
                warn!("Cache lock poisoned in get - skipping cache update");
            }

            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    /// Get a steward by their operational DID
    pub async fn get_by_did(&self, did: &Did) -> anyhow::Result<Option<StewardRecord>> {
        let mut key = PREFIX_BY_DID.to_vec();
        key.extend_from_slice(did.as_str().as_bytes());

        if let Some(data) = self.backend.get(&key).await? {
            let entry: IndexEntry = serde_json::from_slice(&data)?;
            let steward_id = StewardId::new(entry.steward_id);
            self.get(&steward_id).await
        } else {
            Ok(None)
        }
    }

    /// Get a steward by their holder DID
    pub async fn get_by_holder(&self, holder_did: &Did) -> anyhow::Result<Option<StewardRecord>> {
        let mut key = PREFIX_BY_HOLDER.to_vec();
        key.extend_from_slice(holder_did.as_str().as_bytes());

        if let Some(data) = self.backend.get(&key).await? {
            let entry: IndexEntry = serde_json::from_slice(&data)?;
            let steward_id = StewardId::new(entry.steward_id);
            self.get(&steward_id).await
        } else {
            Ok(None)
        }
    }

    /// List stewards by jurisdiction
    pub async fn list_by_jurisdiction(
        &self,
        jurisdiction: &str,
    ) -> anyhow::Result<Vec<StewardRecord>> {
        let mut prefix = PREFIX_BY_JURISDICTION.to_vec();
        prefix.extend_from_slice(jurisdiction.as_bytes());
        prefix.push(b'/');

        let keys = self.backend.list_keys(&prefix).await?;
        let mut records = Vec::new();

        for key in keys {
            if let Some(data) = self.backend.get(&key).await? {
                let entry: IndexEntry = serde_json::from_slice(&data)?;
                let steward_id = StewardId::new(entry.steward_id);
                if let Some(record) = self.get(&steward_id).await? {
                    records.push(record);
                }
            }
        }

        Ok(records)
    }

    /// List stewards by status
    pub async fn list_by_status(&self, status: &str) -> anyhow::Result<Vec<StewardRecord>> {
        let mut prefix = PREFIX_BY_STATUS.to_vec();
        prefix.extend_from_slice(status.as_bytes());
        prefix.push(b'/');

        let keys = self.backend.list_keys(&prefix).await?;
        let mut records = Vec::new();

        for key in keys {
            if let Some(data) = self.backend.get(&key).await? {
                let entry: IndexEntry = serde_json::from_slice(&data)?;
                let steward_id = StewardId::new(entry.steward_id);
                if let Some(record) = self.get(&steward_id).await? {
                    records.push(record);
                }
            }
        }

        Ok(records)
    }

    /// List all active stewards
    pub async fn list_active(&self) -> anyhow::Result<Vec<StewardRecord>> {
        self.list_by_status("Active").await
    }

    /// List stewards with active attestation capability
    pub async fn list_attesters(&self) -> anyhow::Result<Vec<StewardRecord>> {
        let active = self.list_active().await?;
        Ok(active.into_iter().filter(|s| s.can_attest()).collect())
    }

    /// Update a steward record
    pub async fn update(&self, record: &StewardRecord) -> anyhow::Result<()> {
        // Get the old record to update indexes
        if let Some(old_record) = self.get(&record.steward_id).await? {
            // Remove old indexes if status changed
            if std::mem::discriminant(&old_record.status) != std::mem::discriminant(&record.status)
            {
                self.remove_status_index(&old_record).await?;
            }
            if old_record.jurisdiction != record.jurisdiction {
                self.remove_jurisdiction_index(&old_record).await?;
            }
        }

        // Store the updated record
        self.store(record).await
    }

    /// Suspend a steward
    pub async fn suspend(&self, steward_id: &StewardId, reason: String) -> anyhow::Result<bool> {
        if let Some(mut record) = self.get(steward_id).await? {
            record.suspend(reason);
            self.update(&record).await?;
            info!("Suspended steward: {steward_id}");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Reinstate a suspended steward
    pub async fn reinstate(&self, steward_id: &StewardId) -> anyhow::Result<bool> {
        if let Some(mut record) = self.get(steward_id).await? {
            if record.reinstate() {
                self.update(&record).await?;
                info!("Reinstated steward: {steward_id}");
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Retire a steward
    pub async fn retire(&self, steward_id: &StewardId) -> anyhow::Result<bool> {
        if let Some(mut record) = self.get(steward_id).await? {
            record.retire();
            self.update(&record).await?;
            info!("Retired steward: {steward_id}");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Revoke a steward for cause
    pub async fn revoke(
        &self,
        steward_id: &StewardId,
        reason: String,
        evidence: Vec<[u8; 32]>,
    ) -> anyhow::Result<bool> {
        if let Some(mut record) = self.get(steward_id).await? {
            record.revoke(reason, evidence);
            self.update(&record).await?;
            info!("Revoked steward: {steward_id}");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Record an attestation issued by a steward
    pub async fn record_attestation(&self, steward_id: &StewardId) -> anyhow::Result<bool> {
        if let Some(mut record) = self.get(steward_id).await? {
            record.record_attestation();
            self.update(&record).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Record a dispute against a steward
    pub async fn record_dispute(&self, steward_id: &StewardId) -> anyhow::Result<bool> {
        if let Some(mut record) = self.get(steward_id).await? {
            record.record_dispute();
            self.update(&record).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Record a dispute won by a steward
    pub async fn record_dispute_won(&self, steward_id: &StewardId) -> anyhow::Result<bool> {
        if let Some(mut record) = self.get(steward_id).await? {
            record.record_dispute_won();
            self.update(&record).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Delete a steward record (for testing/maintenance)
    pub async fn delete(&self, steward_id: &StewardId) -> anyhow::Result<bool> {
        if let Some(record) = self.get(steward_id).await? {
            // Remove indexes
            self.remove_all_indexes(&record).await?;

            // Remove main record
            let mut key = PREFIX_STEWARD.to_vec();
            key.extend_from_slice(steward_id.as_bytes());
            self.backend.delete(&key).await?;

            // Remove from cache (non-critical - skip on poison)
            if let Ok(mut cache) = self.cache.write() {
                cache.pop(steward_id.as_bytes());
            } else {
                warn!("Cache lock poisoned in delete - skipping cache update");
            }

            debug!("Deleted steward record: {steward_id}");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Count total stewards
    pub async fn count(&self) -> anyhow::Result<usize> {
        let keys = self.backend.list_keys(PREFIX_STEWARD).await?;
        Ok(keys.len())
    }

    /// Count active stewards
    pub async fn count_active(&self) -> anyhow::Result<usize> {
        let active = self.list_active().await?;
        Ok(active.len())
    }

    // ========== Private helpers ==========

    async fn update_indexes(&self, record: &StewardRecord) -> anyhow::Result<()> {
        let steward_id = record.steward_id.as_bytes();
        let entry = IndexEntry {
            steward_id: *steward_id,
        };
        let entry_data = serde_json::to_vec(&entry)?;

        // Index by DID
        let mut key = PREFIX_BY_DID.to_vec();
        key.extend_from_slice(record.steward_did.as_str().as_bytes());
        self.backend.set(&key, &entry_data).await?;

        // Index by holder DID
        let mut key = PREFIX_BY_HOLDER.to_vec();
        key.extend_from_slice(record.holder_did.as_str().as_bytes());
        self.backend.set(&key, &entry_data).await?;

        // Index by jurisdiction (if set)
        if let Some(ref jurisdiction) = record.jurisdiction {
            let mut key = PREFIX_BY_JURISDICTION.to_vec();
            key.extend_from_slice(jurisdiction.as_bytes());
            key.push(b'/');
            key.extend_from_slice(steward_id);
            self.backend.set(&key, &entry_data).await?;
        }

        // Index by status
        let status_key = status_to_key(&record.status);
        let mut key = PREFIX_BY_STATUS.to_vec();
        key.extend_from_slice(status_key.as_bytes());
        key.push(b'/');
        key.extend_from_slice(steward_id);
        self.backend.set(&key, &entry_data).await?;

        Ok(())
    }

    async fn remove_status_index(&self, record: &StewardRecord) -> anyhow::Result<()> {
        let status_key = status_to_key(&record.status);
        let mut key = PREFIX_BY_STATUS.to_vec();
        key.extend_from_slice(status_key.as_bytes());
        key.push(b'/');
        key.extend_from_slice(record.steward_id.as_bytes());
        self.backend.delete(&key).await?;
        Ok(())
    }

    async fn remove_jurisdiction_index(&self, record: &StewardRecord) -> anyhow::Result<()> {
        if let Some(ref jurisdiction) = record.jurisdiction {
            let mut key = PREFIX_BY_JURISDICTION.to_vec();
            key.extend_from_slice(jurisdiction.as_bytes());
            key.push(b'/');
            key.extend_from_slice(record.steward_id.as_bytes());
            self.backend.delete(&key).await?;
        }
        Ok(())
    }

    async fn remove_all_indexes(&self, record: &StewardRecord) -> anyhow::Result<()> {
        // Remove DID index
        let mut key = PREFIX_BY_DID.to_vec();
        key.extend_from_slice(record.steward_did.as_str().as_bytes());
        self.backend.delete(&key).await?;

        // Remove holder index
        let mut key = PREFIX_BY_HOLDER.to_vec();
        key.extend_from_slice(record.holder_did.as_str().as_bytes());
        self.backend.delete(&key).await?;

        // Remove jurisdiction index
        self.remove_jurisdiction_index(record).await?;

        // Remove status index
        self.remove_status_index(record).await?;

        Ok(())
    }
}

fn status_to_key(status: &StewardStatus) -> &'static str {
    match status {
        StewardStatus::Active => "Active",
        StewardStatus::Suspended { .. } => "Suspended",
        StewardStatus::Retired => "Retired",
        StewardStatus::Revoked { .. } => "Revoked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Valid test DIDs (proper multibase-encoded Ed25519 public keys)
    /// Generated from deterministic seeds
    const TEST_DID_1: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"; // seed [1u8; 32]
    const TEST_DID_2: &str = "did:icn:z9hSR6S7WPtxmTojgo6GG3k4yDPecgJY292j7xrsUGWBu"; // seed [2u8; 32]
    const TEST_DID_3: &str = "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse"; // seed [3u8; 32]
    const TEST_DID_4: &str = "did:icn:zEdmxWPmx2WH6WgFfTdu9xfkYf3k1g5wD1zccTVySEEh1"; // seed [4u8; 32]
    const TEST_DID_5: &str = "did:icn:z8SFqwqnq4whPhs8icwHA2hQg3hUoN1qrCLK1SBx3WKwe"; // seed [5u8; 32]
    const TEST_DID_6: &str = "did:icn:zAKkzLhjhyFtM9j7WAhbaqYpFe49cXeJBg2kzLRC2PnNa"; // seed [6u8; 32]

    fn test_did() -> Did {
        TEST_DID_1.parse().expect("valid test DID")
    }

    fn test_holder_did() -> Did {
        TEST_DID_2.parse().expect("valid test DID")
    }

    fn create_test_record() -> StewardRecord {
        let now = icn_time::current_timestamp_secs();

        StewardRecord::new(
            test_did(),
            test_holder_did(),
            now,
            now + 365 * 24 * 60 * 60,
            1000,
            "proposal-001".to_string(),
        )
    }

    #[tokio::test]
    async fn test_store_and_get() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        store.store(&record).await.unwrap();

        let retrieved = store.get(&record.steward_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().steward_id.0, record.steward_id.0);
    }

    #[tokio::test]
    async fn test_get_by_did() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        store.store(&record).await.unwrap();

        let retrieved = store.get_by_did(&record.steward_did).await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_get_by_holder() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        store.store(&record).await.unwrap();

        let retrieved = store.get_by_holder(&record.holder_did).await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_list_by_jurisdiction() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let mut record1 = create_test_record();
        record1.jurisdiction = Some("federation:pacific-nw".to_string());
        store.store(&record1).await.unwrap();

        let mut record2 = StewardRecord::new(
            Did::from_anchor_id(&[44u8; 32]),
            Did::from_anchor_id(&[45u8; 32]),
            1000,
            2000,
            500,
            "proposal-002".to_string(),
        );
        record2.jurisdiction = Some("federation:pacific-nw".to_string());
        store.store(&record2).await.unwrap();

        let records = store
            .list_by_jurisdiction("federation:pacific-nw")
            .await
            .unwrap();
        assert_eq!(records.len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        store.store(&record).await.unwrap();

        let active = store.list_by_status("Active").await.unwrap();
        assert_eq!(active.len(), 1);

        let suspended = store.list_by_status("Suspended").await.unwrap();
        assert_eq!(suspended.len(), 0);
    }

    #[tokio::test]
    async fn test_suspend_and_reinstate() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        let steward_id = record.steward_id.clone();
        store.store(&record).await.unwrap();

        // Suspend
        assert!(store
            .suspend(&steward_id, "Under review".to_string())
            .await
            .unwrap());

        let retrieved = store.get(&steward_id).await.unwrap().unwrap();
        assert!(retrieved.is_suspended());

        // Reinstate
        assert!(store.reinstate(&steward_id).await.unwrap());

        let retrieved = store.get(&steward_id).await.unwrap().unwrap();
        assert!(retrieved.is_active());
    }

    #[tokio::test]
    async fn test_retire() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        let steward_id = record.steward_id.clone();
        store.store(&record).await.unwrap();

        assert!(store.retire(&steward_id).await.unwrap());

        let retrieved = store.get(&steward_id).await.unwrap().unwrap();
        assert!(matches!(retrieved.status, StewardStatus::Retired));
    }

    #[tokio::test]
    async fn test_revoke() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        let steward_id = record.steward_id.clone();
        store.store(&record).await.unwrap();

        let evidence = vec![[1u8; 32], [2u8; 32]];
        assert!(store
            .revoke(&steward_id, "Fraud".to_string(), evidence)
            .await
            .unwrap());

        let retrieved = store.get(&steward_id).await.unwrap().unwrap();
        assert!(matches!(retrieved.status, StewardStatus::Revoked { .. }));
    }

    #[tokio::test]
    async fn test_record_attestation() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        let steward_id = record.steward_id.clone();
        store.store(&record).await.unwrap();

        assert!(store.record_attestation(&steward_id).await.unwrap());

        let retrieved = store.get(&steward_id).await.unwrap().unwrap();
        assert_eq!(retrieved.attestations_issued, 1);
    }

    #[tokio::test]
    async fn test_delete() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        let steward_id = record.steward_id.clone();
        store.store(&record).await.unwrap();

        assert!(store.delete(&steward_id).await.unwrap());
        assert!(store.get(&steward_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_count() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        assert_eq!(store.count().await.unwrap(), 0);

        let record = create_test_record();
        store.store(&record).await.unwrap();

        assert_eq!(store.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_list_active() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        let record = create_test_record();
        let steward_id = record.steward_id.clone();
        store.store(&record).await.unwrap();

        let active = store.list_active().await.unwrap();
        assert_eq!(active.len(), 1);

        store.retire(&steward_id).await.unwrap();

        let active = store.list_active().await.unwrap();
        assert_eq!(active.len(), 0);
    }

    #[tokio::test]
    async fn test_cache() {
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::new(backend, 2); // Small cache

        let record1 = create_test_record();
        let record2 = StewardRecord::new(
            TEST_DID_3.parse().expect("valid test DID"),
            TEST_DID_4.parse().expect("valid test DID"),
            1000,
            2000,
            500,
            "proposal-002".to_string(),
        );
        let record3 = StewardRecord::new(
            TEST_DID_5.parse().expect("valid test DID"),
            TEST_DID_6.parse().expect("valid test DID"),
            1000,
            2000,
            500,
            "proposal-003".to_string(),
        );

        store.store(&record1).await.unwrap();
        store.store(&record2).await.unwrap();
        store.store(&record3).await.unwrap();

        // All should still be retrievable from backend
        assert!(store.get(&record1.steward_id).await.unwrap().is_some());
        assert!(store.get(&record2.steward_id).await.unwrap().is_some());
        assert!(store.get(&record3.steward_id).await.unwrap().is_some());
    }

    // ========== Lock Poisoning Tests ==========

    /// A backend that can simulate poison/failure states for testing
    struct FailingBackend {
        inner: InMemoryStewardStore,
        should_fail: AtomicBool,
    }

    impl FailingBackend {
        fn new() -> Self {
            Self {
                inner: InMemoryStewardStore::new(),
                should_fail: AtomicBool::new(false),
            }
        }

        fn set_failing(&self, fail: bool) {
            self.should_fail.store(fail, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl StewardStoreBackend for FailingBackend {
        async fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            if self.should_fail.load(Ordering::SeqCst) {
                anyhow::bail!("Lock poisoned - store may contain corrupt state")
            }
            self.inner.get(key).await
        }

        async fn set(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
            if self.should_fail.load(Ordering::SeqCst) {
                anyhow::bail!("Lock poisoned - store may contain corrupt state")
            }
            self.inner.set(key, value).await
        }

        async fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
            if self.should_fail.load(Ordering::SeqCst) {
                anyhow::bail!("Lock poisoned - store may contain corrupt state")
            }
            self.inner.delete(key).await
        }

        async fn list_keys(&self, prefix: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
            if self.should_fail.load(Ordering::SeqCst) {
                anyhow::bail!("Lock poisoned - store may contain corrupt state")
            }
            self.inner.list_keys(prefix).await
        }
    }

    #[tokio::test]
    async fn test_backend_poison_fails_fast() {
        // Test that backend failures propagate as errors (fail-fast behavior)
        let backend = FailingBackend::new();
        let store = StewardStore::with_default_cache(backend);

        // Now simulate backend poison BEFORE any operations
        store.backend.set_failing(true);

        // Store operation should fail
        let record = create_test_record();
        let result = store.store(&record).await;
        assert!(result.is_err(), "store should fail when backend is poisoned");
        assert!(
            result.unwrap_err().to_string().contains("Lock poisoned"),
            "error should mention lock poisoning"
        );

        // Get should fail (nothing in cache, backend is poisoned)
        let result = store.get(&record.steward_id).await;
        assert!(result.is_err(), "get should fail when backend is poisoned");

        // list_keys (via count) should fail
        let result = store.count().await;
        assert!(result.is_err(), "count should fail when backend is poisoned");

        // Verify that when backend is restored, operations succeed
        store.backend.set_failing(false);
        let result = store.store(&record).await;
        assert!(result.is_ok(), "store should succeed when backend is restored");
    }

    #[tokio::test]
    async fn test_cache_poison_graceful_degradation() {
        // Test that cache failures don't break the store (graceful degradation)
        // We simulate this by poisoning the cache and verifying operations still succeed
        let backend = InMemoryStewardStore::new();
        let store = StewardStore::with_default_cache(backend);

        // Store a record
        let record = create_test_record();
        store.store(&record).await.expect("store should succeed");

        // Poison the cache by panicking while holding the lock
        // We use catch_unwind to safely panic and poison the lock
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = store.cache.write().unwrap();
            panic!("intentional panic to poison cache");
        }));

        // Verify the cache is actually poisoned
        assert!(
            store.cache.write().is_err(),
            "cache should be poisoned after panic"
        );

        // The cache is now poisoned, but operations should still work via backend
        // Store another record - should succeed (cache update is non-critical)
        let mut record2 = create_test_record();
        record2.steward_did = TEST_DID_3.parse().unwrap();
        record2.holder_did = TEST_DID_4.parse().unwrap();
        let result = store.store(&record2).await;
        assert!(result.is_ok(), "store should succeed even with poisoned cache");

        // Get should work (falls back to backend when cache is poisoned)
        let result = store.get(&record.steward_id).await;
        assert!(result.is_ok(), "get should succeed even with poisoned cache");
        assert!(result.unwrap().is_some(), "should retrieve record from backend");

        // Delete should work (cache update is non-critical)
        let result = store.delete(&record2.steward_id).await;
        assert!(result.is_ok(), "delete should succeed even with poisoned cache");
    }
}
