//! CommonsHolderRecord Storage
//!
//! Persistent storage for CommonsHolderRecords using the icn-store abstraction.
//!
//! # Storage Layout
//!
//! ```text
//! commons/holders/<holder_id>              -> CommonsHolderRecord (full record)
//! commons/by_did/<did>                     -> holder_id (lookup by DID)
//! commons/by_anchor/<anchor_id>            -> holder_id (lookup by anchor)
//! commons/affiliations/<jurisdiction>/<holder_id> -> timestamp (membership index)
//! commons/status/<status>/<holder_id>      -> timestamp (status index)
//! ```

use crate::commons::{
    Affiliation, CommonsHolderRecord, HolderStatus, JurisdictionId, MembershipStatus,
};
use crate::personhood::POPLevel;
use crate::personhood_store::PersonhoodStore;
use crate::Did;
use anyhow::{Context, Result};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Storage key prefixes
const HOLDER_PREFIX: &[u8] = b"commons/holders/";
const DID_INDEX_PREFIX: &[u8] = b"commons/by_did/";
const ANCHOR_INDEX_PREFIX: &[u8] = b"commons/by_anchor/";
const AFFILIATION_INDEX_PREFIX: &[u8] = b"commons/affiliations/";
const STATUS_INDEX_PREFIX: &[u8] = b"commons/status/";

/// Persistent store for CommonsHolderRecords
pub struct CommonsHolderStore<S: PersonhoodStore> {
    store: Arc<S>,
    cache: RwLock<LruCache<[u8; 32], CommonsHolderRecord>>,
}

impl<S: PersonhoodStore> CommonsHolderStore<S> {
    /// Create a new CommonsHolderStore
    pub fn new(store: Arc<S>) -> Self {
        // SAFETY: 1000 is always non-zero
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(1000).unwrap();
        Self {
            store,
            cache: RwLock::new(LruCache::new(capacity)),
        }
    }

    /// Create with custom cache size
    pub fn with_cache_size(store: Arc<S>, cache_size: usize) -> Self {
        // SAFETY: 100 is always non-zero, and cache_size defaults to 100 if zero
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            store,
            cache: RwLock::new(LruCache::new(capacity)),
        }
    }

    // ========================================================================
    // Key Generation
    // ========================================================================

    fn holder_key(holder_id: &[u8; 32]) -> Vec<u8> {
        let mut key = HOLDER_PREFIX.to_vec();
        key.extend_from_slice(holder_id);
        key
    }

    fn did_index_key(did: &Did) -> Vec<u8> {
        let mut key = DID_INDEX_PREFIX.to_vec();
        key.extend_from_slice(did.as_str().as_bytes());
        key
    }

    fn anchor_index_key(anchor_id: &[u8; 32]) -> Vec<u8> {
        let mut key = ANCHOR_INDEX_PREFIX.to_vec();
        key.extend_from_slice(anchor_id);
        key
    }

    fn affiliation_index_key(jurisdiction_id: &JurisdictionId, holder_id: &[u8; 32]) -> Vec<u8> {
        let mut key = AFFILIATION_INDEX_PREFIX.to_vec();
        key.extend_from_slice(jurisdiction_id.as_str().as_bytes());
        key.push(b'/');
        key.extend_from_slice(holder_id);
        key
    }

    fn affiliation_prefix(jurisdiction_id: &JurisdictionId) -> Vec<u8> {
        let mut key = AFFILIATION_INDEX_PREFIX.to_vec();
        key.extend_from_slice(jurisdiction_id.as_str().as_bytes());
        key.push(b'/');
        key
    }

    fn status_index_key(status: &str, holder_id: &[u8; 32]) -> Vec<u8> {
        let mut key = STATUS_INDEX_PREFIX.to_vec();
        key.extend_from_slice(status.as_bytes());
        key.push(b'/');
        key.extend_from_slice(holder_id);
        key
    }

    fn status_prefix(status: &str) -> Vec<u8> {
        let mut key = STATUS_INDEX_PREFIX.to_vec();
        key.extend_from_slice(status.as_bytes());
        key.push(b'/');
        key
    }

    fn status_to_str(status: &HolderStatus) -> &'static str {
        match status {
            HolderStatus::Active => "active",
            HolderStatus::Suspended { .. } => "suspended",
            HolderStatus::Exited { .. } => "exited",
            HolderStatus::Revoked { .. } => "revoked",
        }
    }

    // ========================================================================
    // CRUD Operations
    // ========================================================================

    /// Store a CommonsHolderRecord
    pub fn store(&self, holder: &CommonsHolderRecord) -> Result<()> {
        let holder_id = holder.id();

        // Serialize
        let value =
            serde_json::to_vec(holder).context("Failed to serialize CommonsHolderRecord")?;

        // Store main record
        let key = Self::holder_key(holder_id);
        self.store.put(&key, &value)?;

        // Update DID index
        let did_key = Self::did_index_key(&holder.holder_did);
        self.store.put(&did_key, holder_id)?;

        // Update anchor index
        let anchor_key = Self::anchor_index_key(&holder.anchor_id);
        self.store.put(&anchor_key, holder_id)?;

        // Update affiliation indexes
        for affiliation in &holder.affiliations {
            let aff_key = Self::affiliation_index_key(&affiliation.jurisdiction_id, holder_id);
            let timestamp = holder.updated_at.to_le_bytes();
            self.store.put(&aff_key, &timestamp)?;
        }

        // Update status index
        let status_str = Self::status_to_str(&holder.status);
        let status_key = Self::status_index_key(status_str, holder_id);
        let timestamp = holder.updated_at.to_le_bytes();
        self.store.put(&status_key, &timestamp)?;

        // Update cache
        self.cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Commons holder cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(*holder_id, holder.clone());

        debug!("Stored CommonsHolderRecord: {}", hex::encode(holder_id));
        Ok(())
    }

    /// Get a CommonsHolderRecord by ID
    pub fn get(&self, holder_id: &[u8; 32]) -> Result<Option<CommonsHolderRecord>> {
        // Check cache first
        if let Some(cached) = self
            .cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Commons holder cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(holder_id)
        {
            return Ok(Some(cached.clone()));
        }

        // Load from storage
        let key = Self::holder_key(holder_id);
        if let Some(value) = self.store.get(&key)? {
            let holder: CommonsHolderRecord = serde_json::from_slice(&value)
                .context("Failed to deserialize CommonsHolderRecord")?;

            // Update cache
            self.cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Commons holder cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .put(*holder_id, holder.clone());

            return Ok(Some(holder));
        }

        Ok(None)
    }

    /// Get a CommonsHolderRecord by DID
    pub fn get_by_did(&self, did: &Did) -> Result<Option<CommonsHolderRecord>> {
        let did_key = Self::did_index_key(did);
        if let Some(holder_id_bytes) = self.store.get(&did_key)? {
            if holder_id_bytes.len() == 32 {
                let mut holder_id = [0u8; 32];
                holder_id.copy_from_slice(&holder_id_bytes);
                return self.get(&holder_id);
            }
        }
        Ok(None)
    }

    /// Get a CommonsHolderRecord by anchor ID
    pub fn get_by_anchor(&self, anchor_id: &[u8; 32]) -> Result<Option<CommonsHolderRecord>> {
        let anchor_key = Self::anchor_index_key(anchor_id);
        if let Some(holder_id_bytes) = self.store.get(&anchor_key)? {
            if holder_id_bytes.len() == 32 {
                let mut holder_id = [0u8; 32];
                holder_id.copy_from_slice(&holder_id_bytes);
                return self.get(&holder_id);
            }
        }
        Ok(None)
    }

    /// Delete a CommonsHolderRecord
    pub fn delete(&self, holder_id: &[u8; 32]) -> Result<bool> {
        // Get the holder first to clean up indexes
        if let Some(holder) = self.get(holder_id)? {
            // Delete main record
            let key = Self::holder_key(holder_id);
            self.store.delete(&key)?;

            // Delete DID index
            let did_key = Self::did_index_key(&holder.holder_did);
            self.store.delete(&did_key)?;

            // Delete anchor index
            let anchor_key = Self::anchor_index_key(&holder.anchor_id);
            self.store.delete(&anchor_key)?;

            // Delete affiliation indexes
            for affiliation in &holder.affiliations {
                let aff_key = Self::affiliation_index_key(&affiliation.jurisdiction_id, holder_id);
                self.store.delete(&aff_key)?;
            }

            // Delete status index
            let status_str = Self::status_to_str(&holder.status);
            let status_key = Self::status_index_key(status_str, holder_id);
            self.store.delete(&status_key)?;

            // Remove from cache
            self.cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Commons holder cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .pop(holder_id);

            debug!("Deleted CommonsHolderRecord: {}", hex::encode(holder_id));
            return Ok(true);
        }
        Ok(false)
    }

    // ========================================================================
    // Query Operations
    // ========================================================================

    /// List all CommonsHolderRecords
    pub fn list_all(&self) -> Result<Vec<CommonsHolderRecord>> {
        let entries = self.store.scan(HOLDER_PREFIX)?;
        let mut holders = Vec::new();

        for (_key, value) in entries {
            if let Ok(holder) = serde_json::from_slice::<CommonsHolderRecord>(&value) {
                holders.push(holder);
            }
        }

        Ok(holders)
    }

    /// List holders by status
    pub fn list_by_status(&self, status: &str) -> Result<Vec<CommonsHolderRecord>> {
        let prefix = Self::status_prefix(status);
        let entries = self.store.scan(&prefix)?;
        let mut holders = Vec::new();

        for (key, _value) in entries {
            // Extract holder_id from key (last 32 bytes)
            if key.len() >= 32 {
                let mut holder_id = [0u8; 32];
                holder_id.copy_from_slice(&key[key.len() - 32..]);

                if let Some(holder) = self.get(&holder_id)? {
                    holders.push(holder);
                }
            }
        }

        Ok(holders)
    }

    /// List active holders
    pub fn list_active(&self) -> Result<Vec<CommonsHolderRecord>> {
        self.list_by_status("active")
    }

    /// List suspended holders
    pub fn list_suspended(&self) -> Result<Vec<CommonsHolderRecord>> {
        self.list_by_status("suspended")
    }

    /// List holders affiliated with a jurisdiction
    pub fn list_by_jurisdiction(
        &self,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<Vec<CommonsHolderRecord>> {
        let prefix = Self::affiliation_prefix(jurisdiction_id);
        let entries = self.store.scan(&prefix)?;
        let mut holders = Vec::new();

        for (key, _value) in entries {
            // Extract holder_id from key (last 32 bytes)
            if key.len() >= 32 {
                let mut holder_id = [0u8; 32];
                holder_id.copy_from_slice(&key[key.len() - 32..]);

                if let Some(holder) = self.get(&holder_id)? {
                    holders.push(holder);
                }
            }
        }

        Ok(holders)
    }

    /// List holders by minimum personhood level
    pub fn list_by_min_pop_level(&self, min_level: POPLevel) -> Result<Vec<CommonsHolderRecord>> {
        let all = self.list_all()?;
        Ok(all
            .into_iter()
            .filter(|h| h.personhood_level >= min_level)
            .collect())
    }

    /// Count total holders
    pub fn count(&self) -> Result<usize> {
        let entries = self.store.scan(HOLDER_PREFIX)?;
        Ok(entries.len())
    }

    /// Count by status
    pub fn count_by_status(&self, status: &str) -> Result<usize> {
        let prefix = Self::status_prefix(status);
        let entries = self.store.scan(&prefix)?;
        Ok(entries.len())
    }

    /// Count by jurisdiction
    pub fn count_by_jurisdiction(&self, jurisdiction_id: &JurisdictionId) -> Result<usize> {
        let prefix = Self::affiliation_prefix(jurisdiction_id);
        let entries = self.store.scan(&prefix)?;
        Ok(entries.len())
    }

    // ========================================================================
    // Status Update Operations
    // ========================================================================

    /// Update holder status (handles index updates)
    pub fn update_status(
        &self,
        holder_id: &[u8; 32],
        new_status: HolderStatus,
    ) -> Result<Option<CommonsHolderRecord>> {
        if let Some(mut holder) = self.get(holder_id)? {
            // Get old status for index cleanup
            let old_status_str = Self::status_to_str(&holder.status);

            // Delete old status index
            let old_status_key = Self::status_index_key(old_status_str, holder_id);
            self.store.delete(&old_status_key)?;

            // Update status
            holder.status = new_status;
            holder.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::from_secs(0))
                .as_secs();

            // Store updated holder (will create new status index)
            self.store(&holder)?;

            info!(
                "Updated CommonsHolderRecord {} status to {}",
                hex::encode(holder_id),
                holder.status
            );

            return Ok(Some(holder));
        }
        Ok(None)
    }

    // ========================================================================
    // Affiliation Operations
    // ========================================================================

    /// Add an affiliation to a holder
    pub fn add_affiliation(
        &self,
        holder_id: &[u8; 32],
        affiliation: Affiliation,
    ) -> Result<Option<CommonsHolderRecord>> {
        if let Some(mut holder) = self.get(holder_id)? {
            // Add affiliation index
            let aff_key = Self::affiliation_index_key(&affiliation.jurisdiction_id, holder_id);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::from_secs(0))
                .as_secs();
            self.store.put(&aff_key, &timestamp.to_le_bytes())?;

            // Update holder
            holder.add_affiliation(affiliation);
            self.store(&holder)?;

            return Ok(Some(holder));
        }
        Ok(None)
    }

    /// Remove an affiliation from a holder
    pub fn remove_affiliation(
        &self,
        holder_id: &[u8; 32],
        jurisdiction_id: &JurisdictionId,
    ) -> Result<Option<CommonsHolderRecord>> {
        if let Some(mut holder) = self.get(holder_id)? {
            // Remove affiliation index
            let aff_key = Self::affiliation_index_key(jurisdiction_id, holder_id);
            self.store.delete(&aff_key)?;

            // Update holder
            holder.remove_affiliation(jurisdiction_id);
            holder.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::from_secs(0))
                .as_secs();
            self.store(&holder)?;

            return Ok(Some(holder));
        }
        Ok(None)
    }

    /// Update an affiliation's membership status
    pub fn update_affiliation_status(
        &self,
        holder_id: &[u8; 32],
        jurisdiction_id: &JurisdictionId,
        new_status: MembershipStatus,
    ) -> Result<Option<CommonsHolderRecord>> {
        if let Some(mut holder) = self.get(holder_id)? {
            if let Some(affiliation) = holder.get_affiliation_mut(jurisdiction_id) {
                affiliation.membership_status = new_status;
                holder.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::from_secs(0))
                    .as_secs();
                self.store(&holder)?;
                return Ok(Some(holder));
            }
        }
        Ok(None)
    }

    // ========================================================================
    // Maintenance Operations
    // ========================================================================

    /// Check for holders with expired suspension appeals
    pub fn check_expired_appeals(&self) -> Result<Vec<CommonsHolderRecord>> {
        let suspended = self.list_suspended()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs();

        let mut expired = Vec::new();

        for holder in suspended {
            if let HolderStatus::Suspended {
                appeal_deadline, ..
            } = holder.status
            {
                if now >= appeal_deadline {
                    expired.push(holder);
                }
            }
        }

        Ok(expired)
    }

    /// Check for affiliations with expired memberships
    pub fn check_expired_affiliations(&self) -> Result<Vec<(CommonsHolderRecord, JurisdictionId)>> {
        let all = self.list_all()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs();

        let mut expired = Vec::new();

        for holder in all {
            for affiliation in &holder.affiliations {
                if let Some(expires_at) = affiliation.expires_at {
                    if now >= expires_at {
                        expired.push((holder.clone(), affiliation.jurisdiction_id.clone()));
                    }
                }
            }
        }

        Ok(expired)
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Commons holder cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::Affiliation;
    use crate::personhood_store::InMemoryPersonhoodStore;

    fn test_anchor_id() -> [u8; 32] {
        [42u8; 32]
    }

    fn test_did() -> Did {
        Did::from_anchor_id(&test_anchor_id())
    }

    fn create_test_store() -> CommonsHolderStore<InMemoryPersonhoodStore> {
        let store = Arc::new(InMemoryPersonhoodStore::new());
        CommonsHolderStore::new(store)
    }

    fn create_test_holder() -> CommonsHolderRecord {
        CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong)
    }

    #[test]
    fn test_store_and_get() {
        let store = create_test_store();
        let holder = create_test_holder();

        // Store
        store.store(&holder).unwrap();

        // Get by ID
        let retrieved = store.get(holder.id()).unwrap().unwrap();
        assert_eq!(retrieved.holder_id, holder.holder_id);
        assert_eq!(retrieved.personhood_level, holder.personhood_level);
    }

    #[test]
    fn test_get_by_did() {
        let store = create_test_store();
        let holder = create_test_holder();
        let did = holder.holder_did.clone();

        store.store(&holder).unwrap();

        let retrieved = store.get_by_did(&did).unwrap().unwrap();
        assert_eq!(retrieved.holder_id, holder.holder_id);
    }

    #[test]
    fn test_get_by_anchor() {
        let store = create_test_store();
        let holder = create_test_holder();
        let anchor_id = holder.anchor_id;

        store.store(&holder).unwrap();

        let retrieved = store.get_by_anchor(&anchor_id).unwrap().unwrap();
        assert_eq!(retrieved.holder_id, holder.holder_id);
    }

    #[test]
    fn test_delete() {
        let store = create_test_store();
        let holder = create_test_holder();
        let holder_id = *holder.id();

        store.store(&holder).unwrap();
        assert!(store.get(&holder_id).unwrap().is_some());

        let deleted = store.delete(&holder_id).unwrap();
        assert!(deleted);
        assert!(store.get(&holder_id).unwrap().is_none());
        assert!(store.get_by_did(&holder.holder_did).unwrap().is_none());
    }

    #[test]
    fn test_list_by_status() {
        let store = create_test_store();

        // Create active holder
        let holder1 = create_test_holder();
        store.store(&holder1).unwrap();

        // Create and suspend another holder
        let mut holder2 =
            CommonsHolderRecord::new([2u8; 32], Did::from_anchor_id(&[2u8; 32]), POPLevel::Strong);
        holder2.suspend("test".to_string(), 9999999999);
        store.store(&holder2).unwrap();

        let active = store.list_active().unwrap();
        let suspended = store.list_suspended().unwrap();

        assert_eq!(active.len(), 1);
        assert_eq!(suspended.len(), 1);
    }

    #[test]
    fn test_update_status() {
        let store = create_test_store();
        let holder = create_test_holder();
        let holder_id = *holder.id();

        store.store(&holder).unwrap();
        assert_eq!(store.count_by_status("active").unwrap(), 1);
        assert_eq!(store.count_by_status("suspended").unwrap(), 0);

        // Suspend
        let new_status = HolderStatus::Suspended {
            reason: "test".to_string(),
            appeal_deadline: 9999999999,
            suspended_at: 12345,
        };

        store.update_status(&holder_id, new_status).unwrap();

        assert_eq!(store.count_by_status("active").unwrap(), 0);
        assert_eq!(store.count_by_status("suspended").unwrap(), 1);
    }

    #[test]
    fn test_affiliation_management() {
        let store = create_test_store();
        let holder = create_test_holder();
        let holder_id = *holder.id();

        store.store(&holder).unwrap();

        let coop_id = JurisdictionId::coop("test-coop");
        let affiliation = Affiliation::as_member(coop_id.clone());

        // Add affiliation
        store.add_affiliation(&holder_id, affiliation).unwrap();

        let retrieved = store.get(&holder_id).unwrap().unwrap();
        assert_eq!(retrieved.affiliations.len(), 1);

        // Check jurisdiction index
        let by_jurisdiction = store.list_by_jurisdiction(&coop_id).unwrap();
        assert_eq!(by_jurisdiction.len(), 1);

        // Remove affiliation
        store.remove_affiliation(&holder_id, &coop_id).unwrap();

        let retrieved = store.get(&holder_id).unwrap().unwrap();
        assert!(retrieved.affiliations.is_empty());
    }

    #[test]
    fn test_update_affiliation_status() {
        let store = create_test_store();
        let mut holder = create_test_holder();
        let holder_id = *holder.id();

        let coop_id = JurisdictionId::coop("test-coop");
        holder.add_affiliation(Affiliation::new(coop_id.clone()));
        store.store(&holder).unwrap();

        // Update status
        store
            .update_affiliation_status(&holder_id, &coop_id, MembershipStatus::Member)
            .unwrap();

        let retrieved = store.get(&holder_id).unwrap().unwrap();
        let aff = retrieved.get_affiliation(&coop_id).unwrap();
        assert_eq!(aff.membership_status, MembershipStatus::Member);
    }

    #[test]
    fn test_list_by_min_pop_level() {
        let store = create_test_store();

        // Create holder with Weak level
        let holder1 =
            CommonsHolderRecord::new([1u8; 32], Did::from_anchor_id(&[1u8; 32]), POPLevel::Weak);
        store.store(&holder1).unwrap();

        // Create holder with Strong level
        let holder2 =
            CommonsHolderRecord::new([2u8; 32], Did::from_anchor_id(&[2u8; 32]), POPLevel::Strong);
        store.store(&holder2).unwrap();

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

        store.store(&create_test_holder()).unwrap();

        let holder2 =
            CommonsHolderRecord::new([2u8; 32], Did::from_anchor_id(&[2u8; 32]), POPLevel::Strong);
        store.store(&holder2).unwrap();

        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_count_by_jurisdiction() {
        let store = create_test_store();
        let mut holder = create_test_holder();
        let coop_id = JurisdictionId::coop("test-coop");

        holder.add_affiliation(Affiliation::as_member(coop_id.clone()));
        store.store(&holder).unwrap();

        assert_eq!(store.count_by_jurisdiction(&coop_id).unwrap(), 1);
        assert_eq!(
            store
                .count_by_jurisdiction(&JurisdictionId::coop("other"))
                .unwrap(),
            0
        );
    }

    #[test]
    fn test_cache() {
        let store = create_test_store();
        let holder = create_test_holder();
        let holder_id = *holder.id();

        store.store(&holder).unwrap();

        // First get populates cache
        let _ = store.get(&holder_id).unwrap();

        // Clear underlying store but keep cache
        store.store.data.write().unwrap().clear();

        // Should still get from cache
        let cached = store.get(&holder_id).unwrap();
        assert!(cached.is_some());

        // Clear cache
        store.clear_cache();

        // Now should be gone
        let gone = store.get(&holder_id).unwrap();
        assert!(gone.is_none());
    }

    #[test]
    fn test_check_expired_appeals() {
        let store = create_test_store();
        let mut holder = create_test_holder();

        // Suspend with expired appeal deadline
        holder.status = HolderStatus::Suspended {
            reason: "test".to_string(),
            appeal_deadline: 1, // Already expired
            suspended_at: 0,
        };
        store.store(&holder).unwrap();

        let expired = store.check_expired_appeals().unwrap();
        assert_eq!(expired.len(), 1);
    }

    #[test]
    fn test_check_expired_affiliations() {
        let store = create_test_store();
        let mut holder = create_test_holder();

        // Add affiliation with expired membership
        let coop_id = JurisdictionId::coop("test-coop");
        let mut affiliation = Affiliation::as_member(coop_id.clone());
        affiliation.expires_at = Some(1); // Already expired
        holder.add_affiliation(affiliation);
        store.store(&holder).unwrap();

        let expired = store.check_expired_affiliations().unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].1, coop_id);
    }
}
