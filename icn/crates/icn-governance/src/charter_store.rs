//! Charter Storage
//!
//! Persistent storage for Charters using a key-value store abstraction.
//!
//! # Storage Layout
//!
//! ```text
//! charters/<charter_id>                    -> Charter (full record)
//! charters/by_domain/<domain_id>           -> charter_id (lookup by domain)
//! charters/by_type/<org_type>/<charter_id> -> timestamp (type index)
//! charters/status/<status>/<charter_id>    -> timestamp (status index)
//! ```

use crate::charter::{Charter, CharterId, CharterStatus, OrgType};
use anyhow::{Context, Result};
use lru::LruCache;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Storage key prefixes
const CHARTER_PREFIX: &[u8] = b"charters/";
const DOMAIN_INDEX_PREFIX: &[u8] = b"charters/by_domain/";
const TYPE_INDEX_PREFIX: &[u8] = b"charters/by_type/";
const STATUS_INDEX_PREFIX: &[u8] = b"charters/status/";

/// Store trait for charter persistence
pub trait CharterStoreBackend: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
}

/// In-memory store implementation for testing
#[derive(Default)]
pub struct InMemoryCharterStore {
    pub(crate) data: RwLock<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl InMemoryCharterStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CharterStoreBackend for InMemoryCharterStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.read().unwrap().get(key).cloned())
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.data.write().unwrap().insert(key.to_vec(), value.to_vec());
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

/// Persistent store for Charters
pub struct CharterStore<S: CharterStoreBackend> {
    store: Arc<S>,
    cache: RwLock<LruCache<[u8; 32], Charter>>,
}

impl<S: CharterStoreBackend> CharterStore<S> {
    /// Create a new CharterStore
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            cache: RwLock::new(LruCache::new(NonZeroUsize::new(500).unwrap())),
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

    fn charter_key(charter_id: &CharterId) -> Vec<u8> {
        let mut key = CHARTER_PREFIX.to_vec();
        key.extend_from_slice(charter_id.as_bytes());
        key
    }

    fn domain_index_key(domain_id: &str) -> Vec<u8> {
        let mut key = DOMAIN_INDEX_PREFIX.to_vec();
        key.extend_from_slice(domain_id.as_bytes());
        key
    }

    fn type_index_key(org_type: OrgType, charter_id: &CharterId) -> Vec<u8> {
        let mut key = TYPE_INDEX_PREFIX.to_vec();
        key.extend_from_slice(Self::org_type_str(org_type).as_bytes());
        key.push(b'/');
        key.extend_from_slice(charter_id.as_bytes());
        key
    }

    fn type_prefix(org_type: OrgType) -> Vec<u8> {
        let mut key = TYPE_INDEX_PREFIX.to_vec();
        key.extend_from_slice(Self::org_type_str(org_type).as_bytes());
        key.push(b'/');
        key
    }

    fn status_index_key(status: &str, charter_id: &CharterId) -> Vec<u8> {
        let mut key = STATUS_INDEX_PREFIX.to_vec();
        key.extend_from_slice(status.as_bytes());
        key.push(b'/');
        key.extend_from_slice(charter_id.as_bytes());
        key
    }

    fn status_prefix(status: &str) -> Vec<u8> {
        let mut key = STATUS_INDEX_PREFIX.to_vec();
        key.extend_from_slice(status.as_bytes());
        key.push(b'/');
        key
    }

    fn org_type_str(org_type: OrgType) -> &'static str {
        match org_type {
            OrgType::Cooperative => "coop",
            OrgType::Community => "community",
            OrgType::Federation => "federation",
        }
    }

    fn status_to_str(status: &CharterStatus) -> &'static str {
        match status {
            CharterStatus::Draft => "draft",
            CharterStatus::Active => "active",
            CharterStatus::Suspended { .. } => "suspended",
            CharterStatus::Dissolved { .. } => "dissolved",
        }
    }

    // ========================================================================
    // CRUD Operations
    // ========================================================================

    /// Store a Charter
    pub fn store(&self, charter: &Charter) -> Result<()> {
        let charter_id = &charter.charter_id;

        // Serialize
        let value = serde_json::to_vec(charter).context("Failed to serialize Charter")?;

        // Store main record
        let key = Self::charter_key(charter_id);
        self.store.put(&key, &value)?;

        // Update domain index
        let domain_key = Self::domain_index_key(&charter.full_domain_id());
        self.store.put(&domain_key, charter_id.as_bytes())?;

        // Update type index
        let type_key = Self::type_index_key(charter.org_type, charter_id);
        let timestamp = charter.created_at.to_le_bytes();
        self.store.put(&type_key, &timestamp)?;

        // Update status index
        let status_str = Self::status_to_str(&charter.status);
        let status_key = Self::status_index_key(status_str, charter_id);
        self.store.put(&status_key, &timestamp)?;

        // Update cache
        self.cache
            .write()
            .unwrap()
            .put(*charter_id.as_bytes(), charter.clone());

        debug!("Stored Charter: {}", charter_id);
        Ok(())
    }

    /// Get a Charter by ID
    pub fn get(&self, charter_id: &CharterId) -> Result<Option<Charter>> {
        // Check cache first
        if let Some(cached) = self.cache.write().unwrap().get(charter_id.as_bytes()) {
            return Ok(Some(cached.clone()));
        }

        // Load from storage
        let key = Self::charter_key(charter_id);
        if let Some(value) = self.store.get(&key)? {
            let charter: Charter =
                serde_json::from_slice(&value).context("Failed to deserialize Charter")?;

            // Update cache
            self.cache
                .write()
                .unwrap()
                .put(*charter_id.as_bytes(), charter.clone());

            return Ok(Some(charter));
        }

        Ok(None)
    }

    /// Get a Charter by domain ID
    pub fn get_by_domain(&self, domain_id: &str) -> Result<Option<Charter>> {
        let domain_key = Self::domain_index_key(domain_id);
        if let Some(charter_id_bytes) = self.store.get(&domain_key)? {
            if charter_id_bytes.len() == 32 {
                let mut id = [0u8; 32];
                id.copy_from_slice(&charter_id_bytes);
                return self.get(&CharterId::new(id));
            }
        }
        Ok(None)
    }

    /// Delete a Charter
    pub fn delete(&self, charter_id: &CharterId) -> Result<bool> {
        if let Some(charter) = self.get(charter_id)? {
            // Delete main record
            let key = Self::charter_key(charter_id);
            self.store.delete(&key)?;

            // Delete domain index
            let domain_key = Self::domain_index_key(&charter.full_domain_id());
            self.store.delete(&domain_key)?;

            // Delete type index
            let type_key = Self::type_index_key(charter.org_type, charter_id);
            self.store.delete(&type_key)?;

            // Delete status index
            let status_str = Self::status_to_str(&charter.status);
            let status_key = Self::status_index_key(status_str, charter_id);
            self.store.delete(&status_key)?;

            // Remove from cache
            self.cache.write().unwrap().pop(charter_id.as_bytes());

            debug!("Deleted Charter: {}", charter_id);
            return Ok(true);
        }
        Ok(false)
    }

    // ========================================================================
    // Query Operations
    // ========================================================================

    /// List all Charters
    pub fn list_all(&self) -> Result<Vec<Charter>> {
        let entries = self.store.scan(CHARTER_PREFIX)?;
        let mut charters = Vec::new();

        for (key, value) in entries {
            // Skip index entries (they contain '/')
            if key.len() == CHARTER_PREFIX.len() + 32 {
                if let Ok(charter) = serde_json::from_slice::<Charter>(&value) {
                    charters.push(charter);
                }
            }
        }

        Ok(charters)
    }

    /// List Charters by organization type
    pub fn list_by_type(&self, org_type: OrgType) -> Result<Vec<Charter>> {
        let prefix = Self::type_prefix(org_type);
        let entries = self.store.scan(&prefix)?;
        let mut charters = Vec::new();

        for (key, _value) in entries {
            if key.len() >= 32 {
                let mut id = [0u8; 32];
                id.copy_from_slice(&key[key.len() - 32..]);
                if let Some(charter) = self.get(&CharterId::new(id))? {
                    charters.push(charter);
                }
            }
        }

        Ok(charters)
    }

    /// List Charters by status
    pub fn list_by_status(&self, status: &str) -> Result<Vec<Charter>> {
        let prefix = Self::status_prefix(status);
        let entries = self.store.scan(&prefix)?;
        let mut charters = Vec::new();

        for (key, _value) in entries {
            if key.len() >= 32 {
                let mut id = [0u8; 32];
                id.copy_from_slice(&key[key.len() - 32..]);
                if let Some(charter) = self.get(&CharterId::new(id))? {
                    charters.push(charter);
                }
            }
        }

        Ok(charters)
    }

    /// List active Charters
    pub fn list_active(&self) -> Result<Vec<Charter>> {
        self.list_by_status("active")
    }

    /// List draft Charters
    pub fn list_drafts(&self) -> Result<Vec<Charter>> {
        self.list_by_status("draft")
    }

    /// List cooperatives
    pub fn list_cooperatives(&self) -> Result<Vec<Charter>> {
        self.list_by_type(OrgType::Cooperative)
    }

    /// List communities
    pub fn list_communities(&self) -> Result<Vec<Charter>> {
        self.list_by_type(OrgType::Community)
    }

    /// List federations
    pub fn list_federations(&self) -> Result<Vec<Charter>> {
        self.list_by_type(OrgType::Federation)
    }

    /// Count total Charters
    pub fn count(&self) -> Result<usize> {
        Ok(self.list_all()?.len())
    }

    /// Count by status
    pub fn count_by_status(&self, status: &str) -> Result<usize> {
        let prefix = Self::status_prefix(status);
        let entries = self.store.scan(&prefix)?;
        Ok(entries.len())
    }

    /// Count by type
    pub fn count_by_type(&self, org_type: OrgType) -> Result<usize> {
        let prefix = Self::type_prefix(org_type);
        let entries = self.store.scan(&prefix)?;
        Ok(entries.len())
    }

    // ========================================================================
    // Status Update Operations
    // ========================================================================

    /// Update charter status (handles index updates)
    pub fn update_status(
        &self,
        charter_id: &CharterId,
        new_status: CharterStatus,
    ) -> Result<Option<Charter>> {
        if let Some(mut charter) = self.get(charter_id)? {
            // Get old status for index cleanup
            let old_status_str = Self::status_to_str(&charter.status);

            // Delete old status index
            let old_status_key = Self::status_index_key(old_status_str, charter_id);
            self.store.delete(&old_status_key)?;

            // Update status
            charter.status = new_status;

            // Store updated charter (will create new status index)
            self.store(&charter)?;

            info!("Updated Charter {} status to {}", charter_id, charter.status);

            return Ok(Some(charter));
        }
        Ok(None)
    }

    /// Ratify a draft charter
    pub fn ratify(&self, charter_id: &CharterId) -> Result<Option<Charter>> {
        if let Some(mut charter) = self.get(charter_id)? {
            if charter.ratify().is_ok() {
                // Delete old draft status index
                let old_status_key = Self::status_index_key("draft", charter_id);
                self.store.delete(&old_status_key)?;

                // Store with new active status
                self.store(&charter)?;

                info!("Ratified Charter {}", charter_id);
                return Ok(Some(charter));
            }
        }
        Ok(None)
    }

    /// Suspend an active charter
    pub fn suspend(&self, charter_id: &CharterId, reason: String) -> Result<Option<Charter>> {
        self.update_status(charter_id, CharterStatus::Suspended { reason })
    }

    /// Dissolve a charter
    pub fn dissolve(
        &self,
        charter_id: &CharterId,
        reason: String,
        governance_ref: String,
    ) -> Result<Option<Charter>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.update_status(
            charter_id,
            CharterStatus::Dissolved {
                reason,
                dissolved_at: now,
                governance_ref,
            },
        )
    }

    // ========================================================================
    // Maintenance Operations
    // ========================================================================

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cache.write().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GovernanceParams;
    use crate::membership::MembershipConfig;
    use crate::profile::GovernanceProfileId;
    use crate::{GovernanceConfig, MembershipPolicy, DisputePolicy, FounderSignature};
    use icn_identity::Did;

    fn test_governance_config() -> GovernanceConfig {
        GovernanceConfig::new(
            GovernanceProfileId::builtin("cooperative_default"),
            MembershipConfig::trust_threshold(0.3),
            GovernanceParams::new(50, 50, 7 * 24 * 60 * 60),
        )
    }

    fn create_test_store() -> CharterStore<InMemoryCharterStore> {
        let store = Arc::new(InMemoryCharterStore::new());
        CharterStore::new(store)
    }

    fn create_test_charter(name: &str, org_type: OrgType) -> Charter {
        Charter::new(
            org_type,
            name.to_string(),
            format!("{} Organization", name),
            test_governance_config(),
            MembershipPolicy::default(),
            DisputePolicy::default(),
        )
    }

    #[test]
    fn test_store_and_get() {
        let store = create_test_store();
        let charter = create_test_charter("test-coop", OrgType::Cooperative);
        let charter_id = charter.charter_id.clone();

        store.store(&charter).unwrap();

        let retrieved = store.get(&charter_id).unwrap().unwrap();
        assert_eq!(retrieved.name, charter.name);
        assert_eq!(retrieved.org_type, charter.org_type);
    }

    #[test]
    fn test_get_by_domain() {
        let store = create_test_store();
        let charter = create_test_charter("food-portland", OrgType::Cooperative);

        store.store(&charter).unwrap();

        let domain_id = charter.full_domain_id();
        let retrieved = store.get_by_domain(&domain_id).unwrap().unwrap();
        assert_eq!(retrieved.name, charter.name);
    }

    #[test]
    fn test_delete() {
        let store = create_test_store();
        let charter = create_test_charter("test", OrgType::Community);
        let charter_id = charter.charter_id.clone();

        store.store(&charter).unwrap();
        assert!(store.get(&charter_id).unwrap().is_some());

        let deleted = store.delete(&charter_id).unwrap();
        assert!(deleted);
        assert!(store.get(&charter_id).unwrap().is_none());
    }

    #[test]
    fn test_list_by_type() {
        let store = create_test_store();

        store
            .store(&create_test_charter("coop1", OrgType::Cooperative))
            .unwrap();
        store
            .store(&create_test_charter("coop2", OrgType::Cooperative))
            .unwrap();
        store
            .store(&create_test_charter("community1", OrgType::Community))
            .unwrap();

        let coops = store.list_cooperatives().unwrap();
        let communities = store.list_communities().unwrap();

        assert_eq!(coops.len(), 2);
        assert_eq!(communities.len(), 1);
    }

    #[test]
    fn test_list_by_status() {
        let store = create_test_store();

        // Create and store draft charter
        let charter1 = create_test_charter("draft1", OrgType::Cooperative);
        store.store(&charter1).unwrap();

        // Create, ratify, and store active charter
        let mut charter2 = Charter::cooperative(
            "active1".to_string(),
            "Active Coop".to_string(),
            test_governance_config(),
            "hours".to_string(),
        );
        for i in 0..3 {
            charter2.add_founder(FounderSignature::new(
                Did::from_anchor_id(&[i as u8; 32]),
                vec![0u8; 64],
            ));
        }
        charter2.ratify().unwrap();
        store.store(&charter2).unwrap();

        let drafts = store.list_drafts().unwrap();
        let active = store.list_active().unwrap();

        assert_eq!(drafts.len(), 1);
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_update_status() {
        let store = create_test_store();
        let charter = create_test_charter("test", OrgType::Community);
        let charter_id = charter.charter_id.clone();

        store.store(&charter).unwrap();
        assert_eq!(store.count_by_status("draft").unwrap(), 1);

        // Suspend
        store
            .suspend(&charter_id, "Policy review".to_string())
            .unwrap();

        assert_eq!(store.count_by_status("draft").unwrap(), 0);
        assert_eq!(store.count_by_status("suspended").unwrap(), 1);
    }

    #[test]
    fn test_ratify() {
        let store = create_test_store();
        let mut charter = Charter::cooperative(
            "test".to_string(),
            "Test Coop".to_string(),
            test_governance_config(),
            "hours".to_string(),
        );
        for i in 0..3 {
            charter.add_founder(FounderSignature::new(
                Did::from_anchor_id(&[i as u8; 32]),
                vec![0u8; 64],
            ));
        }
        let charter_id = charter.charter_id.clone();

        store.store(&charter).unwrap();
        assert_eq!(store.count_by_status("draft").unwrap(), 1);

        store.ratify(&charter_id).unwrap();

        assert_eq!(store.count_by_status("draft").unwrap(), 0);
        assert_eq!(store.count_by_status("active").unwrap(), 1);
    }

    #[test]
    fn test_dissolve() {
        let store = create_test_store();
        let mut charter = Charter::cooperative(
            "test".to_string(),
            "Test Coop".to_string(),
            test_governance_config(),
            "hours".to_string(),
        );
        for i in 0..3 {
            charter.add_founder(FounderSignature::new(
                Did::from_anchor_id(&[i as u8; 32]),
                vec![0u8; 64],
            ));
        }
        charter.ratify().unwrap();
        let charter_id = charter.charter_id.clone();

        store.store(&charter).unwrap();

        store
            .dissolve(
                &charter_id,
                "Members voted to dissolve".to_string(),
                "proposal-123".to_string(),
            )
            .unwrap();

        let dissolved = store.get(&charter_id).unwrap().unwrap();
        assert!(matches!(dissolved.status, CharterStatus::Dissolved { .. }));
    }

    #[test]
    fn test_count() {
        let store = create_test_store();

        assert_eq!(store.count().unwrap(), 0);

        store
            .store(&create_test_charter("test1", OrgType::Cooperative))
            .unwrap();
        store
            .store(&create_test_charter("test2", OrgType::Community))
            .unwrap();

        assert_eq!(store.count().unwrap(), 2);
        assert_eq!(store.count_by_type(OrgType::Cooperative).unwrap(), 1);
        assert_eq!(store.count_by_type(OrgType::Community).unwrap(), 1);
    }

    #[test]
    fn test_cache() {
        let store = create_test_store();
        let charter = create_test_charter("test", OrgType::Cooperative);
        let charter_id = charter.charter_id.clone();

        store.store(&charter).unwrap();

        // First get populates cache
        let _ = store.get(&charter_id).unwrap();

        // Clear underlying store but keep cache
        store.store.data.write().unwrap().clear();

        // Should still get from cache
        let cached = store.get(&charter_id).unwrap();
        assert!(cached.is_some());

        // Clear cache
        store.clear_cache();

        // Now should be gone
        let gone = store.get(&charter_id).unwrap();
        assert!(gone.is_none());
    }
}
