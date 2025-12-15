//! PersonhoodAnchor Storage
//!
//! Persistent storage for PersonhoodAnchors using the icn-store abstraction.
//!
//! # Storage Layout
//!
//! ```text
//! personhood/anchors/<anchor_id>           -> PersonhoodAnchor (full record)
//! personhood/by_did/<did>                  -> anchor_id (lookup by DID)
//! personhood/by_key/<pubkey>               -> anchor_id (lookup by current key)
//! personhood/attestations/<anchor_id>/<att_id> -> POPAttestation
//! personhood/status/<status>/<anchor_id>   -> timestamp (index by status)
//! ```

use crate::personhood::{AnchorStatus, POPAttestation, POPLevel, PersonhoodAnchor};
use crate::Did;
use anyhow::{Context, Result};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Storage key prefixes
const ANCHOR_PREFIX: &[u8] = b"personhood/anchors/";
const DID_INDEX_PREFIX: &[u8] = b"personhood/by_did/";
const KEY_INDEX_PREFIX: &[u8] = b"personhood/by_key/";
const STATUS_INDEX_PREFIX: &[u8] = b"personhood/status/";

/// Store trait (simplified from icn-store for this module)
/// In production, this would use icn_store::Store
pub trait PersonhoodStore: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
}

/// In-memory store implementation for testing
#[derive(Default)]
pub struct InMemoryPersonhoodStore {
    /// Internal data storage (pub(crate) for testing)
    pub(crate) data: RwLock<std::collections::BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl InMemoryPersonhoodStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PersonhoodStore for InMemoryPersonhoodStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.read().unwrap().get(key).cloned())
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.data
            .write()
            .unwrap()
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.data.write().unwrap().remove(key);
        Ok(())
    }

    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let data = self.data.read().unwrap();
        Ok(data
            .range(prefix.to_vec()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

/// Persistent store for PersonhoodAnchors
pub struct PersonhoodAnchorStore<S: PersonhoodStore> {
    store: Arc<S>,
    cache: RwLock<LruCache<[u8; 32], PersonhoodAnchor>>,
}

impl<S: PersonhoodStore> PersonhoodAnchorStore<S> {
    /// Create a new PersonhoodAnchorStore
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            cache: RwLock::new(LruCache::new(NonZeroUsize::new(1000).unwrap())),
        }
    }

    /// Create with custom cache size
    pub fn with_cache_size(store: Arc<S>, cache_size: usize) -> Self {
        Self {
            store,
            cache: RwLock::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(100).unwrap()),
            )),
        }
    }

    // ========================================================================
    // Key Generation
    // ========================================================================

    fn anchor_key(anchor_id: &[u8; 32]) -> Vec<u8> {
        let mut key = ANCHOR_PREFIX.to_vec();
        key.extend_from_slice(anchor_id);
        key
    }

    fn did_index_key(did: &Did) -> Vec<u8> {
        let mut key = DID_INDEX_PREFIX.to_vec();
        key.extend_from_slice(did.as_str().as_bytes());
        key
    }

    fn key_index_key(pubkey: &[u8; 32]) -> Vec<u8> {
        let mut key = KEY_INDEX_PREFIX.to_vec();
        key.extend_from_slice(pubkey);
        key
    }

    fn status_index_key(status: &str, anchor_id: &[u8; 32]) -> Vec<u8> {
        let mut key = STATUS_INDEX_PREFIX.to_vec();
        key.extend_from_slice(status.as_bytes());
        key.push(b'/');
        key.extend_from_slice(anchor_id);
        key
    }

    fn status_prefix(status: &str) -> Vec<u8> {
        let mut key = STATUS_INDEX_PREFIX.to_vec();
        key.extend_from_slice(status.as_bytes());
        key.push(b'/');
        key
    }

    // ========================================================================
    // CRUD Operations
    // ========================================================================

    /// Store a PersonhoodAnchor
    pub fn store(&self, anchor: &PersonhoodAnchor) -> Result<()> {
        let anchor_id = anchor.id();

        // Serialize
        let value = serde_json::to_vec(anchor).context("Failed to serialize PersonhoodAnchor")?;

        // Store main record
        let key = Self::anchor_key(anchor_id);
        self.store.put(&key, &value)?;

        // Update DID index
        let did = anchor.to_did();
        let did_key = Self::did_index_key(&did);
        self.store.put(&did_key, anchor_id)?;

        // Update key index (if current_key is set)
        if let Some(current_key) = &anchor.current_key {
            let key_index = Self::key_index_key(current_key);
            self.store.put(&key_index, anchor_id)?;
        }

        // Update status index
        let status_str = match &anchor.status {
            AnchorStatus::Active => "active",
            AnchorStatus::Suspended { .. } => "suspended",
            AnchorStatus::Revoked { .. } => "revoked",
        };
        let status_key = Self::status_index_key(status_str, anchor_id);
        let timestamp = anchor.updated_at.to_le_bytes();
        self.store.put(&status_key, &timestamp)?;

        // Update cache
        self.cache.write().unwrap().put(*anchor_id, anchor.clone());

        debug!("Stored PersonhoodAnchor: {}", hex::encode(anchor_id));
        Ok(())
    }

    /// Get a PersonhoodAnchor by ID
    pub fn get(&self, anchor_id: &[u8; 32]) -> Result<Option<PersonhoodAnchor>> {
        // Check cache first
        if let Some(cached) = self.cache.write().unwrap().get(anchor_id) {
            return Ok(Some(cached.clone()));
        }

        // Load from storage
        let key = Self::anchor_key(anchor_id);
        if let Some(value) = self.store.get(&key)? {
            let anchor: PersonhoodAnchor =
                serde_json::from_slice(&value).context("Failed to deserialize PersonhoodAnchor")?;

            // Update cache
            self.cache.write().unwrap().put(*anchor_id, anchor.clone());

            return Ok(Some(anchor));
        }

        Ok(None)
    }

    /// Get a PersonhoodAnchor by DID
    pub fn get_by_did(&self, did: &Did) -> Result<Option<PersonhoodAnchor>> {
        let did_key = Self::did_index_key(did);
        if let Some(anchor_id_bytes) = self.store.get(&did_key)? {
            if anchor_id_bytes.len() == 32 {
                let mut anchor_id = [0u8; 32];
                anchor_id.copy_from_slice(&anchor_id_bytes);
                return self.get(&anchor_id);
            }
        }
        Ok(None)
    }

    /// Get a PersonhoodAnchor by current public key
    pub fn get_by_key(&self, pubkey: &[u8; 32]) -> Result<Option<PersonhoodAnchor>> {
        let key_index = Self::key_index_key(pubkey);
        if let Some(anchor_id_bytes) = self.store.get(&key_index)? {
            if anchor_id_bytes.len() == 32 {
                let mut anchor_id = [0u8; 32];
                anchor_id.copy_from_slice(&anchor_id_bytes);
                return self.get(&anchor_id);
            }
        }
        Ok(None)
    }

    /// Delete a PersonhoodAnchor
    pub fn delete(&self, anchor_id: &[u8; 32]) -> Result<bool> {
        // Get the anchor first to clean up indexes
        if let Some(anchor) = self.get(anchor_id)? {
            // Delete main record
            let key = Self::anchor_key(anchor_id);
            self.store.delete(&key)?;

            // Delete DID index
            let did = anchor.to_did();
            let did_key = Self::did_index_key(&did);
            self.store.delete(&did_key)?;

            // Delete key index
            if let Some(current_key) = &anchor.current_key {
                let key_index = Self::key_index_key(current_key);
                self.store.delete(&key_index)?;
            }

            // Delete status index
            let status_str = match &anchor.status {
                AnchorStatus::Active => "active",
                AnchorStatus::Suspended { .. } => "suspended",
                AnchorStatus::Revoked { .. } => "revoked",
            };
            let status_key = Self::status_index_key(status_str, anchor_id);
            self.store.delete(&status_key)?;

            // Remove from cache
            self.cache.write().unwrap().pop(anchor_id);

            debug!("Deleted PersonhoodAnchor: {}", hex::encode(anchor_id));
            return Ok(true);
        }
        Ok(false)
    }

    // ========================================================================
    // Query Operations
    // ========================================================================

    /// List all PersonhoodAnchors
    pub fn list_all(&self) -> Result<Vec<PersonhoodAnchor>> {
        let entries = self.store.scan(ANCHOR_PREFIX)?;
        let mut anchors = Vec::new();

        for (_key, value) in entries {
            if let Ok(anchor) = serde_json::from_slice::<PersonhoodAnchor>(&value) {
                anchors.push(anchor);
            }
        }

        Ok(anchors)
    }

    /// List PersonhoodAnchors by status
    pub fn list_by_status(&self, status: &str) -> Result<Vec<PersonhoodAnchor>> {
        let prefix = Self::status_prefix(status);
        let entries = self.store.scan(&prefix)?;
        let mut anchors = Vec::new();

        for (key, _value) in entries {
            // Extract anchor_id from key (last 32 bytes)
            if key.len() >= 32 {
                let mut anchor_id = [0u8; 32];
                anchor_id.copy_from_slice(&key[key.len() - 32..]);

                if let Some(anchor) = self.get(&anchor_id)? {
                    anchors.push(anchor);
                }
            }
        }

        Ok(anchors)
    }

    /// List active PersonhoodAnchors
    pub fn list_active(&self) -> Result<Vec<PersonhoodAnchor>> {
        self.list_by_status("active")
    }

    /// List suspended PersonhoodAnchors
    pub fn list_suspended(&self) -> Result<Vec<PersonhoodAnchor>> {
        self.list_by_status("suspended")
    }

    /// List revoked PersonhoodAnchors
    pub fn list_revoked(&self) -> Result<Vec<PersonhoodAnchor>> {
        self.list_by_status("revoked")
    }

    /// Count total PersonhoodAnchors
    pub fn count(&self) -> Result<usize> {
        let entries = self.store.scan(ANCHOR_PREFIX)?;
        Ok(entries.len())
    }

    /// Count by status
    pub fn count_by_status(&self, status: &str) -> Result<usize> {
        let prefix = Self::status_prefix(status);
        let entries = self.store.scan(&prefix)?;
        Ok(entries.len())
    }

    // ========================================================================
    // Status Update Operations
    // ========================================================================

    /// Update anchor status (handles index updates)
    pub fn update_status(
        &self,
        anchor_id: &[u8; 32],
        new_status: AnchorStatus,
    ) -> Result<Option<PersonhoodAnchor>> {
        if let Some(mut anchor) = self.get(anchor_id)? {
            // Get old status for index cleanup
            let old_status_str = match &anchor.status {
                AnchorStatus::Active => "active",
                AnchorStatus::Suspended { .. } => "suspended",
                AnchorStatus::Revoked { .. } => "revoked",
            };

            // Delete old status index
            let old_status_key = Self::status_index_key(old_status_str, anchor_id);
            self.store.delete(&old_status_key)?;

            // Update status
            anchor.status = new_status;
            anchor.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // Store updated anchor (will create new status index)
            self.store(&anchor)?;

            info!(
                "Updated PersonhoodAnchor {} status to {}",
                hex::encode(anchor_id),
                anchor.status
            );

            return Ok(Some(anchor));
        }
        Ok(None)
    }

    // ========================================================================
    // Attestation Operations
    // ========================================================================

    /// Add a POP attestation to an anchor
    pub fn add_attestation(
        &self,
        anchor_id: &[u8; 32],
        attestation: POPAttestation,
    ) -> Result<Option<PersonhoodAnchor>> {
        if let Some(mut anchor) = self.get(anchor_id)? {
            anchor.add_attestation(attestation);
            self.store(&anchor)?;
            return Ok(Some(anchor));
        }
        Ok(None)
    }

    /// Get anchors by minimum POP level
    pub fn list_by_min_pop_level(&self, min_level: POPLevel) -> Result<Vec<PersonhoodAnchor>> {
        let all = self.list_all()?;
        Ok(all
            .into_iter()
            .filter(|a| a.meets_pop_level(min_level))
            .collect())
    }

    // ========================================================================
    // Key Rotation Support
    // ========================================================================

    /// Update the current key for an anchor (handles index updates)
    pub fn update_current_key(
        &self,
        anchor_id: &[u8; 32],
        old_key: Option<[u8; 32]>,
        new_key: [u8; 32],
    ) -> Result<bool> {
        if let Some(mut anchor) = self.get(anchor_id)? {
            // Delete old key index if present
            if let Some(old) = old_key {
                let old_key_index = Self::key_index_key(&old);
                self.store.delete(&old_key_index)?;
            }

            // Update current key
            anchor.current_key = Some(new_key);
            anchor.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // Store updated anchor (will create new key index)
            self.store(&anchor)?;

            debug!(
                "Updated current key for PersonhoodAnchor {}",
                hex::encode(anchor_id)
            );

            return Ok(true);
        }
        Ok(false)
    }

    // ========================================================================
    // Maintenance Operations
    // ========================================================================

    /// Check for auto-reinstatable suspended anchors
    pub fn check_auto_reinstate(&self) -> Result<Vec<PersonhoodAnchor>> {
        let suspended = self.list_suspended()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut reinstated = Vec::new();

        for anchor in suspended {
            if let AnchorStatus::Suspended {
                until: Some(until), ..
            } = anchor.status
            {
                if now >= until {
                    if let Some(updated) = self.update_status(anchor.id(), AnchorStatus::Active)? {
                        info!(
                            "Auto-reinstated PersonhoodAnchor {}",
                            hex::encode(anchor.id())
                        );
                        reinstated.push(updated);
                    }
                }
            }
        }

        Ok(reinstated)
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cache.write().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personhood::{POPMethod, PersonhoodAnchor};

    fn test_key() -> [u8; 32] {
        [42u8; 32]
    }

    fn create_test_store() -> PersonhoodAnchorStore<InMemoryPersonhoodStore> {
        let store = Arc::new(InMemoryPersonhoodStore::new());
        PersonhoodAnchorStore::new(store)
    }

    #[test]
    fn test_store_and_get() {
        let store = create_test_store();
        let anchor = PersonhoodAnchor::genesis("test", test_key());

        // Store
        store.store(&anchor).unwrap();

        // Get by ID
        let retrieved = store.get(anchor.id()).unwrap().unwrap();
        assert_eq!(retrieved.anchor.id, anchor.anchor.id);
        assert_eq!(retrieved.current_key, anchor.current_key);
    }

    #[test]
    fn test_get_by_did() {
        let store = create_test_store();
        let anchor = PersonhoodAnchor::genesis("test", test_key());
        let did = anchor.to_did();

        store.store(&anchor).unwrap();

        let retrieved = store.get_by_did(&did).unwrap().unwrap();
        assert_eq!(retrieved.anchor.id, anchor.anchor.id);
    }

    #[test]
    fn test_get_by_key() {
        let store = create_test_store();
        let key = test_key();
        let anchor = PersonhoodAnchor::genesis("test", key);

        store.store(&anchor).unwrap();

        let retrieved = store.get_by_key(&key).unwrap().unwrap();
        assert_eq!(retrieved.anchor.id, anchor.anchor.id);
    }

    #[test]
    fn test_delete() {
        let store = create_test_store();
        let anchor = PersonhoodAnchor::genesis("test", test_key());
        let anchor_id = *anchor.id();

        store.store(&anchor).unwrap();
        assert!(store.get(&anchor_id).unwrap().is_some());

        let deleted = store.delete(&anchor_id).unwrap();
        assert!(deleted);
        assert!(store.get(&anchor_id).unwrap().is_none());

        // Indexes should also be cleaned up
        assert!(store.get_by_did(&anchor.to_did()).unwrap().is_none());
    }

    #[test]
    fn test_list_by_status() {
        let store = create_test_store();

        // Create active anchor
        let anchor1 = PersonhoodAnchor::genesis("test1", [1u8; 32]);
        store.store(&anchor1).unwrap();

        // Create and suspend another anchor
        let mut anchor2 = PersonhoodAnchor::genesis("test2", [2u8; 32]);
        let authority = Did::from_anchor_id(&[99u8; 32]);
        anchor2.suspend("test suspension".to_string(), authority, None);
        store.store(&anchor2).unwrap();

        // Check counts
        let active = store.list_active().unwrap();
        let suspended = store.list_suspended().unwrap();

        assert_eq!(active.len(), 1);
        assert_eq!(suspended.len(), 1);
    }

    #[test]
    fn test_update_status() {
        let store = create_test_store();
        let anchor = PersonhoodAnchor::genesis("test", test_key());
        let anchor_id = *anchor.id();

        store.store(&anchor).unwrap();
        assert_eq!(store.count_by_status("active").unwrap(), 1);
        assert_eq!(store.count_by_status("suspended").unwrap(), 0);

        // Suspend
        let authority = Did::from_anchor_id(&[99u8; 32]);
        let new_status = AnchorStatus::Suspended {
            reason: "test".to_string(),
            suspended_at: 12345,
            until: None,
            authority,
        };

        store.update_status(&anchor_id, new_status).unwrap();

        assert_eq!(store.count_by_status("active").unwrap(), 0);
        assert_eq!(store.count_by_status("suspended").unwrap(), 1);

        let updated = store.get(&anchor_id).unwrap().unwrap();
        assert!(updated.is_suspended());
    }

    #[test]
    fn test_add_attestation() {
        let store = create_test_store();
        let anchor = PersonhoodAnchor::genesis("test", test_key());
        let anchor_id = *anchor.id();

        store.store(&anchor).unwrap();

        let issuer = Did::from_anchor_id(&[99u8; 32]);
        let attestation = POPAttestation::new(
            &anchor_id,
            issuer,
            POPMethod::InPerson {
                location_hash: None,
            },
            POPLevel::Strong,
            vec![1, 2, 3],
            None,
        );

        let updated = store
            .add_attestation(&anchor_id, attestation)
            .unwrap()
            .unwrap();
        assert_eq!(updated.pop_attestations.len(), 2); // Genesis + new one
        assert!(updated.meets_pop_level(POPLevel::Strong));
    }

    #[test]
    fn test_update_current_key() {
        let store = create_test_store();
        let old_key = [1u8; 32];
        let new_key = [2u8; 32];
        let anchor = PersonhoodAnchor::genesis("test", old_key);
        let anchor_id = *anchor.id();

        store.store(&anchor).unwrap();

        // Should find by old key
        assert!(store.get_by_key(&old_key).unwrap().is_some());

        // Update key
        store
            .update_current_key(&anchor_id, Some(old_key), new_key)
            .unwrap();

        // Should find by new key, not old
        assert!(store.get_by_key(&old_key).unwrap().is_none());
        assert!(store.get_by_key(&new_key).unwrap().is_some());
    }

    #[test]
    fn test_list_by_min_pop_level() {
        let store = create_test_store();

        // Create anchor with only genesis (Weak)
        let anchor1 = PersonhoodAnchor::genesis("test1", [1u8; 32]);
        store.store(&anchor1).unwrap();

        // Create anchor and add Strong attestation
        let mut anchor2 = PersonhoodAnchor::genesis("test2", [2u8; 32]);
        let issuer = Did::from_anchor_id(&[99u8; 32]);
        let attestation = POPAttestation::new(
            anchor2.id(),
            issuer,
            POPMethod::InPerson {
                location_hash: None,
            },
            POPLevel::Strong,
            vec![],
            None,
        );
        anchor2.add_attestation(attestation);
        store.store(&anchor2).unwrap();

        // Query
        let weak_or_above = store.list_by_min_pop_level(POPLevel::Weak).unwrap();
        let strong_or_above = store.list_by_min_pop_level(POPLevel::Strong).unwrap();
        let verified = store.list_by_min_pop_level(POPLevel::Verified).unwrap();

        assert_eq!(weak_or_above.len(), 2);
        assert_eq!(strong_or_above.len(), 1);
        assert_eq!(verified.len(), 0);
    }

    #[test]
    fn test_count() {
        let store = create_test_store();

        assert_eq!(store.count().unwrap(), 0);

        store
            .store(&PersonhoodAnchor::genesis("test1", [1u8; 32]))
            .unwrap();
        store
            .store(&PersonhoodAnchor::genesis("test2", [2u8; 32]))
            .unwrap();

        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_cache() {
        let store = create_test_store();
        let anchor = PersonhoodAnchor::genesis("test", test_key());
        let anchor_id = *anchor.id();

        store.store(&anchor).unwrap();

        // First get populates cache
        let _ = store.get(&anchor_id).unwrap();

        // Clear underlying store but keep cache
        store.store.data.write().unwrap().clear();

        // Should still get from cache
        let cached = store.get(&anchor_id).unwrap();
        assert!(cached.is_some());

        // Clear cache
        store.clear_cache();

        // Now should be gone
        let gone = store.get(&anchor_id).unwrap();
        assert!(gone.is_none());
    }

    #[test]
    fn test_auto_reinstate() {
        let store = create_test_store();
        let mut anchor = PersonhoodAnchor::genesis("test", test_key());
        let authority = Did::from_anchor_id(&[99u8; 32]);

        // Suspend with expiry in the past
        anchor.status = AnchorStatus::Suspended {
            reason: "test".to_string(),
            suspended_at: 0,
            until: Some(1), // Expired
            authority,
        };
        store.store(&anchor).unwrap();

        // Run auto-reinstate
        let reinstated = store.check_auto_reinstate().unwrap();

        assert_eq!(reinstated.len(), 1);
        assert!(reinstated[0].is_active());

        // Verify in store
        let updated = store.get(anchor.id()).unwrap().unwrap();
        assert!(updated.is_active());
    }
}
