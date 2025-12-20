//! Cooperative Registry
//!
//! Manages the registry of known federated cooperatives with persistent storage.
//! The registry tracks cooperative metadata, vouches, and federation status.

use crate::error::{FederationError, Result};
use crate::metrics;
use crate::types::{current_timestamp, CooperativeInfo, FederationPolicy, PolicyResult, Vouch};
use icn_store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{debug, info, warn};

/// Storage key prefixes
const COOP_PREFIX: &[u8] = b"federation/coops/";
const VOUCH_PREFIX: &[u8] = b"federation/vouches/";
const OWN_COOP_KEY: &[u8] = b"federation/own_coop";

/// Registry of federated cooperatives
///
/// Provides CRUD operations for cooperative information with persistence
/// and policy-based federation control.
pub struct CooperativeRegistry {
    /// Persistent storage backend
    store: Arc<dyn Store>,

    /// In-memory cache of cooperatives (coop_id -> info)
    cache: RwLock<HashMap<String, CooperativeInfo>>,

    /// In-memory cache of vouches (target_coop_id -> vec of voucher_coop_ids)
    vouches: RwLock<HashMap<String, Vec<String>>>,

    /// Our own cooperative's ID
    own_coop_id: String,

    /// Our own cooperative's info
    own_coop_info: RwLock<CooperativeInfo>,
}

impl CooperativeRegistry {
    /// Create a new registry with the given store and own cooperative info
    pub fn new(store: Arc<dyn Store>, own_info: CooperativeInfo) -> Result<Self> {
        let own_coop_id = own_info.coop_id.clone();

        let registry = Self {
            store,
            cache: RwLock::new(HashMap::new()),
            vouches: RwLock::new(HashMap::new()),
            own_coop_id,
            own_coop_info: RwLock::new(own_info.clone()),
        };

        // Persist own coop info
        registry.save_own_info(&own_info)?;

        // Load existing data from store
        registry.load_from_store()?;

        Ok(registry)
    }

    /// Load existing cooperatives and vouches from persistent storage
    fn load_from_store(&self) -> Result<()> {
        // Load cooperatives
        let coop_entries = self.store.scan(COOP_PREFIX)?;
        let mut cache = self.cache.write().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned during load, recovering");
            poisoned.into_inner()
        });

        for (key, value) in coop_entries {
            let key_str = String::from_utf8_lossy(&key[COOP_PREFIX.len()..]);
            match serde_json::from_slice::<CooperativeInfo>(&value) {
                Ok(info) => {
                    debug!("Loaded cooperative from store: {}", info.coop_id);
                    cache.insert(info.coop_id.clone(), info);
                }
                Err(e) => {
                    warn!("Failed to deserialize cooperative {}: {}", key_str, e);
                }
            }
        }
        drop(cache);

        // Load vouches
        let vouch_entries = self.store.scan(VOUCH_PREFIX)?;
        let mut vouches = self.vouches.write().unwrap_or_else(|poisoned| {
            warn!("Vouches lock poisoned during load, recovering");
            poisoned.into_inner()
        });

        for (key, value) in vouch_entries {
            let target_coop = String::from_utf8_lossy(&key[VOUCH_PREFIX.len()..]).to_string();
            match serde_json::from_slice::<Vec<String>>(&value) {
                Ok(vouchers) => {
                    debug!(
                        "Loaded {} vouches for cooperative: {}",
                        vouchers.len(),
                        target_coop
                    );
                    vouches.insert(target_coop, vouchers);
                }
                Err(e) => {
                    warn!("Failed to deserialize vouches for {}: {}", target_coop, e);
                }
            }
        }
        drop(vouches);

        // Update metrics
        let cache_len = self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).len();
        metrics::registry::coops_known_set(cache_len);

        info!("Loaded {} cooperatives from storage", cache_len);
        Ok(())
    }

    /// Save own cooperative info to storage
    fn save_own_info(&self, info: &CooperativeInfo) -> Result<()> {
        let value = serde_json::to_vec(info)?;
        self.store.put(OWN_COOP_KEY, &value)?;
        Ok(())
    }

    /// Get the storage key for a cooperative
    fn coop_key(coop_id: &str) -> Vec<u8> {
        let mut key = COOP_PREFIX.to_vec();
        key.extend(coop_id.as_bytes());
        key
    }

    /// Get the storage key for vouches
    fn vouch_key(target_coop_id: &str) -> Vec<u8> {
        let mut key = VOUCH_PREFIX.to_vec();
        key.extend(target_coop_id.as_bytes());
        key
    }

    /// Get our own cooperative ID
    pub fn own_coop_id(&self) -> &str {
        &self.own_coop_id
    }

    /// Get our own cooperative info
    pub fn own_coop_info(&self) -> CooperativeInfo {
        self.own_coop_info
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Own coop info lock poisoned, recovering");
                poisoned.into_inner()
            })
            .clone()
    }

    /// Update our own cooperative info
    pub fn update_own_info(&self, info: CooperativeInfo) -> Result<()> {
        if info.coop_id != self.own_coop_id {
            return Err(FederationError::InvalidCooperativeId(
                "Cannot change own coop_id".to_string(),
            ));
        }
        self.save_own_info(&info)?;
        *self.own_coop_info.write().unwrap_or_else(|poisoned| {
            warn!("Own coop info lock poisoned, recovering");
            poisoned.into_inner()
        }) = info;
        Ok(())
    }

    /// Register a new cooperative
    ///
    /// This checks the federation policy before allowing registration.
    pub fn register(&self, info: CooperativeInfo) -> Result<()> {
        let coop_id = info.coop_id.clone();

        // Don't allow registering ourselves
        if coop_id == self.own_coop_id {
            return Err(FederationError::InvalidCooperativeId(
                "Cannot register own cooperative".to_string(),
            ));
        }

        // Check if already registered
        if self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).contains_key(&coop_id) {
            return Err(FederationError::CooperativeAlreadyExists(coop_id));
        }

        // Check policy
        let policy_result = self.check_policy(&info)?;
        match policy_result {
            PolicyResult::Allowed => {}
            PolicyResult::NeedsVouches { required, current } => {
                return Err(FederationError::InsufficientVouches {
                    required,
                    actual: current,
                });
            }
            PolicyResult::Closed => {
                return Err(FederationError::PolicyViolation(
                    "Federation is closed".to_string(),
                ));
            }
            PolicyResult::Denied(reason) => {
                return Err(FederationError::PolicyViolation(reason));
            }
        }

        // Persist to storage
        let key = Self::coop_key(&coop_id);
        let value = serde_json::to_vec(&info)?;
        self.store.put(&key, &value)?;

        // Update cache
        self.cache.write().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).insert(coop_id.clone(), info);

        // Update metrics
        metrics::registry::coops_registered_inc();
        let cache_len = self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).len();
        metrics::registry::coops_known_set(cache_len);

        info!("Registered cooperative: {}", coop_id);
        Ok(())
    }

    /// Get a cooperative by ID
    pub fn get(&self, coop_id: &str) -> Result<Option<CooperativeInfo>> {
        // Check cache first
        if let Some(info) = self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).get(coop_id) {
            return Ok(Some(info.clone()));
        }

        // Fall back to storage
        let key = Self::coop_key(coop_id);
        if let Some(value) = self.store.get(&key)? {
            let info: CooperativeInfo = serde_json::from_slice(&value)?;
            // Update cache
            self.cache
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Cache lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .insert(coop_id.to_string(), info.clone());
            return Ok(Some(info));
        }

        Ok(None)
    }

    /// List all known cooperatives
    pub fn list(&self) -> Result<Vec<CooperativeInfo>> {
        Ok(self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).values().cloned().collect())
    }

    /// List cooperative IDs only
    pub fn list_ids(&self) -> Vec<String> {
        self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).keys().cloned().collect()
    }

    /// Remove a cooperative from the registry
    pub fn remove(&self, coop_id: &str) -> Result<()> {
        // Don't allow removing ourselves
        if coop_id == self.own_coop_id {
            return Err(FederationError::InvalidCooperativeId(
                "Cannot remove own cooperative".to_string(),
            ));
        }

        // Remove from storage
        let key = Self::coop_key(coop_id);
        self.store.delete(&key)?;

        // Remove from cache
        self.cache.write().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).remove(coop_id);

        // Also remove any vouches for this coop
        let vouch_key = Self::vouch_key(coop_id);
        let _ = self.store.delete(&vouch_key);
        self.vouches.write().unwrap_or_else(|poisoned| {
            warn!("Vouches lock poisoned, recovering");
            poisoned.into_inner()
        }).remove(coop_id);

        // Update metrics
        metrics::registry::coops_removed_inc();
        let cache_len = self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).len();
        metrics::registry::coops_known_set(cache_len);

        info!("Removed cooperative: {}", coop_id);
        Ok(())
    }

    /// Update the last_seen timestamp for a cooperative
    pub fn update_last_seen(&self, coop_id: &str, timestamp: u64) -> Result<()> {
        let key = Self::coop_key(coop_id);

        // Update cache
        {
            let mut cache = self.cache.write().unwrap_or_else(|poisoned| {
                warn!("Cache lock poisoned, recovering");
                poisoned.into_inner()
            });
            if let Some(info) = cache.get_mut(coop_id) {
                info.last_seen = timestamp;
                // Persist updated info
                let value = serde_json::to_vec(info)?;
                self.store.put(&key, &value)?;
                return Ok(());
            }
        }

        Err(FederationError::CooperativeNotFound(coop_id.to_string()))
    }

    /// Check if a cooperative can federate according to our policy
    pub fn check_policy(&self, info: &CooperativeInfo) -> Result<PolicyResult> {
        let own_info = self.own_coop_info.read().unwrap_or_else(|poisoned| {
            warn!("Own coop info lock poisoned, recovering");
            poisoned.into_inner()
        });

        match &own_info.federation_policy {
            FederationPolicy::Open => Ok(PolicyResult::Allowed),

            FederationPolicy::Vouched { min_vouches } => {
                let vouches = self.get_vouches(&info.coop_id)?;

                // Only count vouches from already-federated cooperatives
                let valid_vouches: Vec<_> = vouches
                    .iter()
                    .filter(|voucher| self.is_federated(voucher))
                    .collect();

                if valid_vouches.len() >= *min_vouches as usize {
                    Ok(PolicyResult::Allowed)
                } else {
                    Ok(PolicyResult::NeedsVouches {
                        required: *min_vouches,
                        current: valid_vouches.len() as u8,
                    })
                }
            }

            FederationPolicy::Closed => Ok(PolicyResult::Closed),
        }
    }

    /// Check if a cooperative is already federated with us
    pub fn is_federated(&self, coop_id: &str) -> bool {
        self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).contains_key(coop_id)
    }

    /// Get vouches for a cooperative
    pub fn get_vouches(&self, coop_id: &str) -> Result<Vec<String>> {
        // Check cache first
        if let Some(vouchers) = self.vouches.read().unwrap_or_else(|poisoned| {
            warn!("Vouches lock poisoned, recovering");
            poisoned.into_inner()
        }).get(coop_id) {
            return Ok(vouchers.clone());
        }

        // Fall back to storage
        let key = Self::vouch_key(coop_id);
        if let Some(value) = self.store.get(&key)? {
            let vouchers: Vec<String> = serde_json::from_slice(&value)?;
            // Update cache
            self.vouches
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Vouches lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .insert(coop_id.to_string(), vouchers.clone());
            return Ok(vouchers);
        }

        Ok(Vec::new())
    }

    /// Add a vouch for a cooperative
    ///
    /// The voucher must be an already-federated cooperative.
    pub fn add_vouch(&self, vouch: &Vouch) -> Result<()> {
        let voucher = &vouch.voucher_coop_id;
        let target = &vouch.target_coop_id;

        // Voucher must be federated with us
        if !self.is_federated(voucher) && voucher != &self.own_coop_id {
            return Err(FederationError::VouchNotAllowed(format!(
                "Voucher {voucher} is not a federation partner"
            )));
        }

        // Can't vouch for ourselves
        if target == &self.own_coop_id {
            return Err(FederationError::VouchNotAllowed(
                "Cannot vouch for own cooperative".to_string(),
            ));
        }

        // Check if vouch has expired
        if vouch.is_expired() {
            return Err(FederationError::VouchNotAllowed(
                "Vouch has expired".to_string(),
            ));
        }

        // Get current vouches
        let mut vouches = self.get_vouches(target)?;

        // Don't add duplicate vouches
        if vouches.contains(voucher) {
            return Ok(()); // Already vouched
        }

        // Add the vouch
        vouches.push(voucher.clone());

        // Persist to storage
        let key = Self::vouch_key(target);
        let value = serde_json::to_vec(&vouches)?;
        self.store.put(&key, &value)?;

        // Update cache
        self.vouches
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Vouches lock poisoned, recovering");
                poisoned.into_inner()
            })
            .insert(target.clone(), vouches);

        // Update metrics
        metrics::registry::vouches_received_inc(voucher);

        info!("Added vouch from {} for {}", voucher, target);
        Ok(())
    }

    /// Remove a vouch for a cooperative
    pub fn remove_vouch(&self, voucher: &str, target: &str) -> Result<()> {
        let mut vouches = self.get_vouches(target)?;

        let initial_len = vouches.len();
        vouches.retain(|v| v != voucher);

        if vouches.len() == initial_len {
            return Ok(()); // Vouch didn't exist
        }

        // Persist to storage
        let key = Self::vouch_key(target);
        if vouches.is_empty() {
            self.store.delete(&key)?;
        } else {
            let value = serde_json::to_vec(&vouches)?;
            self.store.put(&key, &value)?;
        }

        // Update cache
        if vouches.is_empty() {
            self.vouches.write().unwrap_or_else(|poisoned| {
                warn!("Vouches lock poisoned, recovering");
                poisoned.into_inner()
            }).remove(target);
        } else {
            self.vouches
                .write()
                .unwrap_or_else(|poisoned| {
                    warn!("Vouches lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .insert(target.to_string(), vouches);
        }

        info!("Removed vouch from {} for {}", voucher, target);
        Ok(())
    }

    /// Get the count of known cooperatives
    pub fn count(&self) -> usize {
        self.cache.read().unwrap_or_else(|poisoned| {
            warn!("Cache lock poisoned, recovering");
            poisoned.into_inner()
        }).len()
    }

    /// Check if any cooperatives are stale (not seen recently)
    pub fn find_stale(&self, max_age_secs: u64) -> Vec<String> {
        let now = current_timestamp();
        let cutoff = now.saturating_sub(max_age_secs);

        self.cache
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .iter()
            .filter_map(|(id, info)| {
                if info.last_seen < cutoff {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get cooperatives that support a specific capability
    pub fn find_by_capability(&self, capability: &str) -> Vec<CooperativeInfo> {
        self.cache
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .values()
            .filter(|info| info.has_capability(capability))
            .cloned()
            .collect()
    }

    /// Get cooperatives that support a specific currency
    pub fn find_by_currency(&self, currency: &str) -> Vec<CooperativeInfo> {
        self.cache
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Cache lock poisoned, recovering");
                poisoned.into_inner()
            })
            .values()
            .filter(|info| info.currencies.iter().any(|c| c.symbol == currency))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::{Did, KeyPair};
    use icn_store::SledStore;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn create_test_registry() -> CooperativeRegistry {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let own_info = CooperativeInfo::new(
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );
        CooperativeRegistry::new(store, own_info).unwrap()
    }

    #[test]
    fn test_register_and_get() {
        let registry = create_test_registry();

        let info = CooperativeInfo::new(
            "food-coop".to_string(),
            "Food Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );

        registry.register(info.clone()).unwrap();

        let retrieved = registry.get("food-coop").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Food Cooperative");
    }

    #[test]
    fn test_cannot_register_self() {
        let registry = create_test_registry();

        let info = CooperativeInfo::new(
            "test-coop".to_string(), // Same as registry's own ID
            "Test Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );

        let result = registry.register(info);
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_register_duplicate() {
        let registry = create_test_registry();

        let info = CooperativeInfo::new(
            "food-coop".to_string(),
            "Food Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );

        registry.register(info.clone()).unwrap();

        let result = registry.register(info);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_cooperatives() {
        let registry = create_test_registry();

        let coop1 = CooperativeInfo::new(
            "coop1".to_string(),
            "Coop 1".to_string(),
            test_did(),
            FederationPolicy::Open,
        );
        let coop2 = CooperativeInfo::new(
            "coop2".to_string(),
            "Coop 2".to_string(),
            test_did(),
            FederationPolicy::Open,
        );

        registry.register(coop1).unwrap();
        registry.register(coop2).unwrap();

        let list = registry.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_remove_cooperative() {
        let registry = create_test_registry();

        let info = CooperativeInfo::new(
            "food-coop".to_string(),
            "Food Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );

        registry.register(info).unwrap();
        assert!(registry.get("food-coop").unwrap().is_some());

        registry.remove("food-coop").unwrap();
        assert!(registry.get("food-coop").unwrap().is_none());
    }

    #[test]
    fn test_vouched_policy() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let own_info = CooperativeInfo::new(
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            test_did(),
            FederationPolicy::Vouched { min_vouches: 2 },
        );
        let registry = CooperativeRegistry::new(store, own_info).unwrap();

        // Register a partner first (will be used as voucher)
        let partner = CooperativeInfo::new(
            "partner-coop".to_string(),
            "Partner Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );
        // Temporarily set policy to open for initial partner
        {
            let mut own = registry.own_coop_info.write().unwrap();
            own.federation_policy = FederationPolicy::Open;
        }
        registry.register(partner).unwrap();
        // Restore vouched policy
        {
            let mut own = registry.own_coop_info.write().unwrap();
            own.federation_policy = FederationPolicy::Vouched { min_vouches: 2 };
        }

        // New coop tries to join
        let new_coop = CooperativeInfo::new(
            "new-coop".to_string(),
            "New Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );

        // Check policy - should need vouches
        let result = registry.check_policy(&new_coop).unwrap();
        match result {
            PolicyResult::NeedsVouches { required, current } => {
                assert_eq!(required, 2);
                assert_eq!(current, 0);
            }
            _ => panic!("Expected NeedsVouches"),
        }
    }

    #[test]
    fn test_vouches() {
        let registry = create_test_registry();

        // Register a partner
        let partner = CooperativeInfo::new(
            "partner-coop".to_string(),
            "Partner Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );
        registry.register(partner).unwrap();

        // Create a vouch with trust score
        let vouch = Vouch::new(
            "partner-coop".to_string(),
            test_did(),
            "new-coop".to_string(),
            0.8, // Trust score
        );

        registry.add_vouch(&vouch).unwrap();

        let vouches = registry.get_vouches("new-coop").unwrap();
        assert_eq!(vouches.len(), 1);
        assert_eq!(vouches[0], "partner-coop");
    }

    #[test]
    fn test_closed_policy() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let own_info = CooperativeInfo::new(
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            test_did(),
            FederationPolicy::Closed,
        );
        let registry = CooperativeRegistry::new(store, own_info).unwrap();

        let info = CooperativeInfo::new(
            "food-coop".to_string(),
            "Food Cooperative".to_string(),
            test_did(),
            FederationPolicy::Open,
        );

        let result = registry.register(info);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_by_capability() {
        let registry = create_test_registry();

        let coop1 = CooperativeInfo::new(
            "coop1".to_string(),
            "Coop 1".to_string(),
            test_did(),
            FederationPolicy::Open,
        )
        .with_capability("clearing");

        let coop2 = CooperativeInfo::new(
            "coop2".to_string(),
            "Coop 2".to_string(),
            test_did(),
            FederationPolicy::Open,
        )
        .with_capability("compute");

        registry.register(coop1).unwrap();
        registry.register(coop2).unwrap();

        let clearing_coops = registry.find_by_capability("clearing");
        assert_eq!(clearing_coops.len(), 1);
        assert_eq!(clearing_coops[0].coop_id, "coop1");
    }

    #[test]
    fn test_persistence() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;

        // Create registry and add a coop
        {
            let own_info = CooperativeInfo::new(
                "test-coop".to_string(),
                "Test Cooperative".to_string(),
                test_did(),
                FederationPolicy::Open,
            );
            let registry = CooperativeRegistry::new(store.clone(), own_info).unwrap();

            let info = CooperativeInfo::new(
                "food-coop".to_string(),
                "Food Cooperative".to_string(),
                test_did(),
                FederationPolicy::Open,
            );
            registry.register(info).unwrap();
        }

        // Create new registry with same store - should load existing data
        {
            let own_info = CooperativeInfo::new(
                "test-coop".to_string(),
                "Test Cooperative".to_string(),
                test_did(),
                FederationPolicy::Open,
            );
            let registry = CooperativeRegistry::new(store, own_info).unwrap();

            let retrieved = registry.get("food-coop").unwrap();
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().name, "Food Cooperative");
        }
    }
}
