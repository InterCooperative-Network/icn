//! Agreement Storage Layer
//!
//! Persistent storage for inter-cooperative agreements with secondary indexes
//! for efficient lookups by party.

use super::types::{Agreement, AgreementId, AgreementStatus, Amendment};
use crate::error::Result;
use icn_identity::Did;
use icn_store::Store;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{debug, warn};

/// Storage key prefixes
const AGREEMENT_PREFIX: &[u8] = b"federation/agreements/";
const AGREEMENT_PARTY_INDEX: &[u8] = b"idx_agreement_party/";
const AMENDMENT_PREFIX: &[u8] = b"federation/amendments/";

/// Default cache size for agreements
const DEFAULT_CACHE_SIZE: usize = 500;

/// Trait for agreement storage operations
pub trait AgreementStoreOps: Send + Sync {
    /// Store an agreement
    fn store_agreement(&self, agreement: &Agreement) -> Result<()>;

    /// Retrieve an agreement by ID
    fn get_agreement(&self, id: &AgreementId) -> Result<Option<Agreement>>;

    /// Delete an agreement
    fn delete_agreement(&self, id: &AgreementId) -> Result<()>;

    /// List all agreements
    fn list_agreements(&self) -> Result<Vec<Agreement>>;

    /// List agreements for a specific party
    fn list_agreements_for_party(&self, party_did: &Did) -> Result<Vec<Agreement>>;

    /// List agreements by status
    fn list_agreements_by_status(&self, status_type: &str) -> Result<Vec<Agreement>>;

    /// Store an amendment
    fn store_amendment(&self, amendment: &Amendment) -> Result<()>;

    /// Get amendments for an agreement
    fn get_amendments(&self, agreement_id: &AgreementId) -> Result<Vec<Amendment>>;
}

/// Persistent agreement store using Sled
pub struct AgreementStore {
    store: Arc<dyn Store>,
    cache: RwLock<LruCache<AgreementId, Agreement>>,
}

impl AgreementStore {
    /// Create a new agreement store
    pub fn new(store: Arc<dyn Store>) -> Self {
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap();
        Self {
            store,
            cache: RwLock::new(LruCache::new(capacity)),
        }
    }

    /// Create with custom cache size
    pub fn with_cache_size(store: Arc<dyn Store>, cache_size: usize) -> Self {
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(cache_size.max(1)).unwrap();
        Self {
            store,
            cache: RwLock::new(LruCache::new(capacity)),
        }
    }

    /// Get the storage key for an agreement
    fn agreement_key(id: &AgreementId) -> Vec<u8> {
        let mut key = AGREEMENT_PREFIX.to_vec();
        key.extend(id.as_str().as_bytes());
        key
    }

    /// Get the index key for a party-to-agreement mapping
    fn party_index_key(party_did: &Did, agreement_id: &AgreementId) -> Vec<u8> {
        let mut key = AGREEMENT_PARTY_INDEX.to_vec();
        key.extend(party_did.as_str().as_bytes());
        key.push(b'/');
        key.extend(agreement_id.as_str().as_bytes());
        key
    }

    /// Get the prefix for all agreements for a party
    fn party_index_prefix(party_did: &Did) -> Vec<u8> {
        let mut key = AGREEMENT_PARTY_INDEX.to_vec();
        key.extend(party_did.as_str().as_bytes());
        key.push(b'/');
        key
    }

    /// Get the amendment key
    fn amendment_key(agreement_id: &AgreementId, amendment_id: &str) -> Vec<u8> {
        let mut key = AMENDMENT_PREFIX.to_vec();
        key.extend(agreement_id.as_str().as_bytes());
        key.push(b'/');
        key.extend(amendment_id.as_bytes());
        key
    }

    /// Get the prefix for all amendments for an agreement
    fn amendment_prefix(agreement_id: &AgreementId) -> Vec<u8> {
        let mut key = AMENDMENT_PREFIX.to_vec();
        key.extend(agreement_id.as_str().as_bytes());
        key.push(b'/');
        key
    }

    /// Invalidate cache entry
    fn invalidate_cache(&self, id: &AgreementId) {
        self.cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Agreement cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .pop(id);
    }

    /// Update cache with an agreement
    fn update_cache(&self, agreement: &Agreement) {
        self.cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Agreement cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .put(agreement.id.clone(), agreement.clone());
    }

    /// Update party indexes for an agreement
    fn update_party_indexes(&self, agreement: &Agreement) -> Result<()> {
        // Add index entry for each party
        for party in &agreement.parties {
            let index_key = Self::party_index_key(&party.did, &agreement.id);
            self.store
                .put(&index_key, agreement.id.as_str().as_bytes())?;
        }
        Ok(())
    }

    /// Remove party indexes for an agreement
    fn remove_party_indexes(&self, agreement: &Agreement) -> Result<()> {
        for party in &agreement.parties {
            let index_key = Self::party_index_key(&party.did, &agreement.id);
            self.store.delete(&index_key)?;
        }
        Ok(())
    }

    /// Get status type string for filtering
    fn status_type(status: &AgreementStatus) -> &'static str {
        match status {
            AgreementStatus::Draft => "draft",
            AgreementStatus::Proposed { .. } => "proposed",
            AgreementStatus::Active { .. } => "active",
            AgreementStatus::Suspended { .. } => "suspended",
            AgreementStatus::Terminated { .. } => "terminated",
        }
    }
}

impl AgreementStoreOps for AgreementStore {
    fn store_agreement(&self, agreement: &Agreement) -> Result<()> {
        let key = Self::agreement_key(&agreement.id);
        let value = serde_json::to_vec(agreement)?;

        // Store the agreement
        self.store.put(&key, &value)?;

        // Update indexes
        self.update_party_indexes(agreement)?;

        // Update cache
        self.update_cache(agreement);

        debug!(
            "Stored agreement {} (status: {:?})",
            agreement.id, agreement.status
        );
        Ok(())
    }

    fn get_agreement(&self, id: &AgreementId) -> Result<Option<Agreement>> {
        // Check cache first
        if let Some(cached) = self
            .cache
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Agreement cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(id)
        {
            return Ok(Some(cached.clone()));
        }

        // Load from storage
        let key = Self::agreement_key(id);
        match self.store.get(&key)? {
            Some(value) => {
                let agreement: Agreement = serde_json::from_slice(&value)?;
                self.update_cache(&agreement);
                Ok(Some(agreement))
            }
            None => Ok(None),
        }
    }

    fn delete_agreement(&self, id: &AgreementId) -> Result<()> {
        // Load agreement first to get party list for index cleanup
        if let Some(agreement) = self.get_agreement(id)? {
            // Remove party indexes
            self.remove_party_indexes(&agreement)?;

            // Remove any amendments
            let amendment_prefix = Self::amendment_prefix(id);
            let amendments = self.store.scan(&amendment_prefix)?;
            for (key, _) in amendments {
                self.store.delete(&key)?;
            }
        }

        // Remove the agreement itself
        let key = Self::agreement_key(id);
        self.store.delete(&key)?;

        // Invalidate cache
        self.invalidate_cache(id);

        debug!("Deleted agreement {}", id);
        Ok(())
    }

    fn list_agreements(&self) -> Result<Vec<Agreement>> {
        let entries = self.store.scan(AGREEMENT_PREFIX)?;
        let mut agreements = Vec::new();

        for (_key, value) in entries {
            if let Ok(agreement) = serde_json::from_slice::<Agreement>(&value) {
                agreements.push(agreement);
            }
        }

        // Sort by created_at descending (most recent first)
        agreements.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(agreements)
    }

    fn list_agreements_for_party(&self, party_did: &Did) -> Result<Vec<Agreement>> {
        let prefix = Self::party_index_prefix(party_did);
        let index_entries = self.store.scan(&prefix)?;

        let mut agreements = Vec::new();
        for (_key, value) in index_entries {
            // Value contains the agreement ID
            if let Ok(id_str) = String::from_utf8(value) {
                let id = AgreementId::new(id_str);
                if let Some(agreement) = self.get_agreement(&id)? {
                    agreements.push(agreement);
                }
            }
        }

        // Sort by created_at descending
        agreements.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(agreements)
    }

    fn list_agreements_by_status(&self, status_type: &str) -> Result<Vec<Agreement>> {
        let all = self.list_agreements()?;
        Ok(all
            .into_iter()
            .filter(|a| Self::status_type(&a.status) == status_type)
            .collect())
    }

    fn store_amendment(&self, amendment: &Amendment) -> Result<()> {
        let key = Self::amendment_key(&amendment.agreement_id, &amendment.id);
        let value = serde_json::to_vec(amendment)?;
        self.store.put(&key, &value)?;

        // Invalidate agreement cache since amendments affect the agreement
        self.invalidate_cache(&amendment.agreement_id);

        debug!(
            "Stored amendment {} for agreement {}",
            amendment.id, amendment.agreement_id
        );
        Ok(())
    }

    fn get_amendments(&self, agreement_id: &AgreementId) -> Result<Vec<Amendment>> {
        let prefix = Self::amendment_prefix(agreement_id);
        let entries = self.store.scan(&prefix)?;

        let mut amendments = Vec::new();
        for (_key, value) in entries {
            if let Ok(amendment) = serde_json::from_slice::<Amendment>(&value) {
                amendments.push(amendment);
            }
        }

        // Sort by proposed_at
        amendments.sort_by(|a, b| a.proposed_at.cmp(&b.proposed_at));

        Ok(amendments)
    }
}

/// In-memory agreement store for testing
pub struct InMemoryAgreementStore {
    agreements: RwLock<std::collections::HashMap<AgreementId, Agreement>>,
    amendments: RwLock<std::collections::HashMap<String, Amendment>>,
}

impl InMemoryAgreementStore {
    /// Create a new in-memory store
    pub fn new() -> Self {
        Self {
            agreements: RwLock::new(std::collections::HashMap::new()),
            amendments: RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryAgreementStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AgreementStoreOps for InMemoryAgreementStore {
    fn store_agreement(&self, agreement: &Agreement) -> Result<()> {
        let mut guard = self.agreements.write().unwrap_or_else(|p| {
            warn!("Agreement store lock poisoned, recovering");
            p.into_inner()
        });
        guard.insert(agreement.id.clone(), agreement.clone());
        Ok(())
    }

    fn get_agreement(&self, id: &AgreementId) -> Result<Option<Agreement>> {
        let guard = self.agreements.read().unwrap_or_else(|p| {
            warn!("Agreement store lock poisoned, recovering");
            p.into_inner()
        });
        Ok(guard.get(id).cloned())
    }

    fn delete_agreement(&self, id: &AgreementId) -> Result<()> {
        let mut guard = self.agreements.write().unwrap_or_else(|p| {
            warn!("Agreement store lock poisoned, recovering");
            p.into_inner()
        });
        guard.remove(id);

        // Also remove amendments
        let mut amend_guard = self.amendments.write().unwrap_or_else(|p| {
            warn!("Amendment store lock poisoned, recovering");
            p.into_inner()
        });
        amend_guard.retain(|_, a| &a.agreement_id != id);

        Ok(())
    }

    fn list_agreements(&self) -> Result<Vec<Agreement>> {
        let guard = self.agreements.read().unwrap_or_else(|p| {
            warn!("Agreement store lock poisoned, recovering");
            p.into_inner()
        });
        let mut agreements: Vec<_> = guard.values().cloned().collect();
        agreements.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(agreements)
    }

    fn list_agreements_for_party(&self, party_did: &Did) -> Result<Vec<Agreement>> {
        let guard = self.agreements.read().unwrap_or_else(|p| {
            warn!("Agreement store lock poisoned, recovering");
            p.into_inner()
        });
        let mut agreements: Vec<_> = guard
            .values()
            .filter(|a| a.parties.iter().any(|p| &p.did == party_did))
            .cloned()
            .collect();
        agreements.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(agreements)
    }

    fn list_agreements_by_status(&self, status_type: &str) -> Result<Vec<Agreement>> {
        let guard = self.agreements.read().unwrap_or_else(|p| {
            warn!("Agreement store lock poisoned, recovering");
            p.into_inner()
        });

        let status_matches = |status: &AgreementStatus| -> bool {
            matches!(
                (status, status_type),
                (AgreementStatus::Draft, "draft")
                    | (AgreementStatus::Proposed { .. }, "proposed")
                    | (AgreementStatus::Active { .. }, "active")
                    | (AgreementStatus::Suspended { .. }, "suspended")
                    | (AgreementStatus::Terminated { .. }, "terminated")
            )
        };

        let agreements: Vec<_> = guard
            .values()
            .filter(|a| status_matches(&a.status))
            .cloned()
            .collect();
        Ok(agreements)
    }

    fn store_amendment(&self, amendment: &Amendment) -> Result<()> {
        let mut guard = self.amendments.write().unwrap_or_else(|p| {
            warn!("Amendment store lock poisoned, recovering");
            p.into_inner()
        });
        guard.insert(amendment.id.clone(), amendment.clone());
        Ok(())
    }

    fn get_amendments(&self, agreement_id: &AgreementId) -> Result<Vec<Amendment>> {
        let guard = self.amendments.read().unwrap_or_else(|p| {
            warn!("Amendment store lock poisoned, recovering");
            p.into_inner()
        });
        let mut amendments: Vec<_> = guard
            .values()
            .filter(|a| &a.agreement_id == agreement_id)
            .cloned()
            .collect();
        amendments.sort_by(|a, b| a.proposed_at.cmp(&b.proposed_at));
        Ok(amendments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agreement::{AgreementType, PartyRole};
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn create_test_agreement() -> Agreement {
        let proposer = test_did();
        let counterparty = test_did();

        Agreement::new(
            "Test Agreement",
            "Test description",
            AgreementType::Credit {
                credit_limit: 5000,
                interest_rate_bps: 300,
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer, "coop-a", PartyRole::Proposer)
        .with_party(counterparty, "coop-b", PartyRole::Counterparty)
    }

    #[test]
    fn test_in_memory_store_crud() {
        let store = InMemoryAgreementStore::new();

        // Create
        let agreement = create_test_agreement();
        let id = agreement.id.clone();
        store.store_agreement(&agreement).unwrap();

        // Read
        let loaded = store.get_agreement(&id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().title, "Test Agreement");

        // List
        let all = store.list_agreements().unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        store.delete_agreement(&id).unwrap();
        assert!(store.get_agreement(&id).unwrap().is_none());
    }

    #[test]
    fn test_list_by_party() {
        let store = InMemoryAgreementStore::new();

        let proposer = test_did();
        let counterparty1 = test_did();
        let counterparty2 = test_did();

        // Agreement 1: proposer + counterparty1
        let agreement1 = Agreement::new(
            "Agreement 1",
            "First agreement",
            AgreementType::Credit {
                credit_limit: 5000,
                interest_rate_bps: 300,
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer.clone(), "coop-a", PartyRole::Proposer)
        .with_party(counterparty1.clone(), "coop-b", PartyRole::Counterparty);

        // Agreement 2: proposer + counterparty2
        let agreement2 = Agreement::new(
            "Agreement 2",
            "Second agreement",
            AgreementType::Credit {
                credit_limit: 10000,
                interest_rate_bps: 400,
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer.clone(), "coop-a", PartyRole::Proposer)
        .with_party(counterparty2.clone(), "coop-c", PartyRole::Counterparty);

        store.store_agreement(&agreement1).unwrap();
        store.store_agreement(&agreement2).unwrap();

        // Proposer should see both
        let proposer_agreements = store.list_agreements_for_party(&proposer).unwrap();
        assert_eq!(proposer_agreements.len(), 2);

        // Each counterparty should see only one
        let cp1_agreements = store.list_agreements_for_party(&counterparty1).unwrap();
        assert_eq!(cp1_agreements.len(), 1);
        assert_eq!(cp1_agreements[0].title, "Agreement 1");

        let cp2_agreements = store.list_agreements_for_party(&counterparty2).unwrap();
        assert_eq!(cp2_agreements.len(), 1);
        assert_eq!(cp2_agreements[0].title, "Agreement 2");
    }

    #[test]
    fn test_list_by_status() {
        let store = InMemoryAgreementStore::new();

        let agreement1 = create_test_agreement();
        let mut agreement2 = create_test_agreement();

        // Keep agreement1 as draft
        store.store_agreement(&agreement1).unwrap();

        // Propose agreement2
        agreement2.propose().unwrap();
        store.store_agreement(&agreement2).unwrap();

        // List by status
        let drafts = store.list_agreements_by_status("draft").unwrap();
        assert_eq!(drafts.len(), 1);

        let proposed = store.list_agreements_by_status("proposed").unwrap();
        assert_eq!(proposed.len(), 1);

        let active = store.list_agreements_by_status("active").unwrap();
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn test_amendments() {
        let store = InMemoryAgreementStore::new();

        let agreement = create_test_agreement();
        let agreement_id = agreement.id.clone();
        store.store_agreement(&agreement).unwrap();

        // Create amendments
        let amendment1 = Amendment::new(agreement_id.clone(), "First amendment", test_did());
        let amendment2 = Amendment::new(agreement_id.clone(), "Second amendment", test_did());

        store.store_amendment(&amendment1).unwrap();
        store.store_amendment(&amendment2).unwrap();

        // Get amendments
        let amendments = store.get_amendments(&agreement_id).unwrap();
        assert_eq!(amendments.len(), 2);
    }

    #[test]
    fn test_persistent_store_crud() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store_path = temp_dir.path().join("agreements");

        let sled_store: Arc<dyn Store> = Arc::new(icn_store::SledStore::open(&store_path).unwrap());
        let store = AgreementStore::new(sled_store);

        // Create
        let agreement = create_test_agreement();
        let id = agreement.id.clone();
        store.store_agreement(&agreement).unwrap();

        // Read
        let loaded = store.get_agreement(&id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().title, "Test Agreement");

        // List
        let all = store.list_agreements().unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        store.delete_agreement(&id).unwrap();
        assert!(store.get_agreement(&id).unwrap().is_none());
    }

    #[test]
    fn test_persistent_store_survives_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store_path = temp_dir.path().join("agreements");

        // First "session" - create and store agreement
        let agreement_id;
        let party_did;
        {
            let sled_store = Arc::new(icn_store::SledStore::open(&store_path).unwrap());
            let store = AgreementStore::new(sled_store.clone() as Arc<dyn Store>);

            let proposer = test_did();
            party_did = proposer.clone();
            let counterparty = test_did();

            let agreement = Agreement::new(
                "Persistent Agreement",
                "This should survive restart",
                AgreementType::Credit {
                    credit_limit: 10000,
                    interest_rate_bps: 500,
                    currency: "ICN".to_string(),
                },
            )
            .with_party(proposer, "coop-persisted", PartyRole::Proposer)
            .with_party(counterparty, "coop-other", PartyRole::Counterparty);

            agreement_id = agreement.id.clone();
            store.store_agreement(&agreement).unwrap();

            // Verify it's stored
            assert!(store.get_agreement(&agreement_id).unwrap().is_some());

            // Explicitly flush to disk and drop in correct order
            sled_store.flush().unwrap();
            drop(store);
            drop(sled_store);
        }

        // Wait for OS-level lock release (Sled uses file locks)
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Second "session" - reopen with retry logic to handle lock contention
        let sled_store = {
            let mut attempts = 0;
            let max_attempts = 5;
            loop {
                match icn_store::SledStore::open(&store_path) {
                    Ok(store) => break Arc::new(store),
                    Err(e) if attempts < max_attempts => {
                        attempts += 1;
                        eprintln!(
                            "Attempt {}/{} to open store failed: {}. Retrying...",
                            attempts, max_attempts, e
                        );
                        std::thread::sleep(std::time::Duration::from_millis(100 * attempts));
                    }
                    Err(e) => panic!(
                        "Failed to reopen store after {} attempts: {}",
                        max_attempts, e
                    ),
                }
            }
        };

        {
            let store = AgreementStore::new(sled_store as Arc<dyn Store>);

            // Agreement should still exist
            let loaded = store.get_agreement(&agreement_id).unwrap();
            assert!(loaded.is_some(), "Agreement should persist across restart");

            let agreement = loaded.unwrap();
            assert_eq!(agreement.title, "Persistent Agreement");
            assert_eq!(agreement.description, "This should survive restart");

            // Party index should also persist
            let party_agreements = store.list_agreements_for_party(&party_did).unwrap();
            assert_eq!(party_agreements.len(), 1);
            assert_eq!(party_agreements[0].id, agreement_id);

            // List should return the agreement
            let all = store.list_agreements().unwrap();
            assert_eq!(all.len(), 1);
        }
    }

    #[test]
    fn test_persistent_store_amendments_survive_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store_path = temp_dir.path().join("agreements");

        let agreement_id;
        let amendment_id;

        // First session - create agreement and amendment
        {
            let sled_store: Arc<dyn Store> =
                Arc::new(icn_store::SledStore::open(&store_path).unwrap());
            let store = AgreementStore::new(sled_store);

            let agreement = create_test_agreement();
            agreement_id = agreement.id.clone();
            store.store_agreement(&agreement).unwrap();

            let amendment =
                Amendment::new(agreement_id.clone(), "Persistent amendment", test_did());
            amendment_id = amendment.id.clone();
            store.store_amendment(&amendment).unwrap();

            // Verify
            let amendments = store.get_amendments(&agreement_id).unwrap();
            assert_eq!(amendments.len(), 1);
        }

        // Second session - verify amendments persist
        {
            let sled_store: Arc<dyn Store> =
                Arc::new(icn_store::SledStore::open(&store_path).unwrap());
            let store = AgreementStore::new(sled_store);

            let amendments = store.get_amendments(&agreement_id).unwrap();
            assert_eq!(
                amendments.len(),
                1,
                "Amendment should persist across restart"
            );
            assert_eq!(amendments[0].id, amendment_id);
            assert_eq!(amendments[0].description, "Persistent amendment");
        }
    }
}
