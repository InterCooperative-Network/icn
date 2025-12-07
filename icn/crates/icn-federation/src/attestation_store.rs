//! Attestation Store (Phase F2)
//!
//! Persistent storage for federated trust attestations.

use crate::attestation::FederatedTrustAttestation;
use crate::error::Result;
use crate::metrics;
use icn_identity::Did;
use icn_store::Store;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{debug, info};

/// Storage key prefix
const ATTESTATION_PREFIX: &[u8] = b"federation/attestations/";

/// Store for federated trust attestations
pub struct AttestationStore {
    store: Arc<dyn Store>,
    cache: RwLock<LruCache<Did, Vec<FederatedTrustAttestation>>>,
}

impl AttestationStore {
    /// Create a new attestation store
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self {
            store,
            cache: RwLock::new(LruCache::new(NonZeroUsize::new(1000).unwrap())),
        }
    }

    /// Get the storage key for attestations of a member from a specific coop
    fn attestation_key(member_did: &Did, source_coop_id: &str) -> Vec<u8> {
        let mut key = ATTESTATION_PREFIX.to_vec();
        key.extend(member_did.as_str().as_bytes());
        key.push(b'/');
        key.extend(source_coop_id.as_bytes());
        key
    }

    /// Get the prefix for all attestations for a member
    fn member_prefix(member_did: &Did) -> Vec<u8> {
        let mut key = ATTESTATION_PREFIX.to_vec();
        key.extend(member_did.as_str().as_bytes());
        key.push(b'/');
        key
    }

    /// Store an attestation
    pub fn store_attestation(&self, att: FederatedTrustAttestation) -> Result<()> {
        let key = Self::attestation_key(&att.member_did, &att.source_coop_id);
        let value = serde_json::to_vec(&att)?;
        self.store.put(&key, &value)?;

        // Invalidate cache entry
        self.cache.write().unwrap().pop(&att.member_did);

        // Update metrics
        metrics::attestation::stored_inc(&att.source_coop_id, att.trust_context.as_str());

        debug!(
            "Stored attestation for {} from {}",
            att.member_did, att.source_coop_id
        );
        Ok(())
    }

    /// Get all attestations for a member
    pub fn get_attestations_for(&self, member: &Did) -> Result<Vec<FederatedTrustAttestation>> {
        // Check cache first
        if let Some(cached) = self.cache.write().unwrap().get(member) {
            return Ok(cached.clone());
        }

        // Load from storage
        let prefix = Self::member_prefix(member);
        let entries = self.store.scan(&prefix)?;

        let mut attestations = Vec::new();
        for (_key, value) in entries {
            if let Ok(att) = serde_json::from_slice::<FederatedTrustAttestation>(&value) {
                attestations.push(att);
            }
        }

        // Update cache
        if !attestations.is_empty() {
            self.cache
                .write()
                .unwrap()
                .put(member.clone(), attestations.clone());
        }

        Ok(attestations)
    }

    /// Get attestations from a specific cooperative
    pub fn get_attestations_from(&self, coop_id: &str) -> Result<Vec<FederatedTrustAttestation>> {
        let entries = self.store.scan(ATTESTATION_PREFIX)?;

        let mut attestations = Vec::new();
        for (_key, value) in entries {
            if let Ok(att) = serde_json::from_slice::<FederatedTrustAttestation>(&value) {
                if att.source_coop_id == coop_id {
                    attestations.push(att);
                }
            }
        }

        Ok(attestations)
    }

    /// Remove expired attestations
    pub fn remove_expired(&self) -> Result<usize> {
        let entries = self.store.scan(ATTESTATION_PREFIX)?;
        let mut removed = 0;

        for (key, value) in entries {
            if let Ok(att) = serde_json::from_slice::<FederatedTrustAttestation>(&value) {
                if att.is_expired() {
                    self.store.delete(&key)?;
                    self.cache.write().unwrap().pop(&att.member_did);
                    removed += 1;
                    metrics::attestation::expired_inc();
                }
            }
        }

        if removed > 0 {
            info!("Removed {} expired attestations", removed);
        }

        Ok(removed)
    }

    /// Get valid (non-expired) attestations for a member
    pub fn get_valid_attestations_for(
        &self,
        member: &Did,
    ) -> Result<Vec<FederatedTrustAttestation>> {
        let attestations = self.get_attestations_for(member)?;
        Ok(attestations
            .into_iter()
            .filter(|a| !a.is_expired())
            .collect())
    }

    /// Remove a specific attestation
    pub fn remove_attestation(&self, member: &Did, source_coop_id: &str) -> Result<()> {
        let key = Self::attestation_key(member, source_coop_id);
        self.store.delete(&key)?;
        self.cache.write().unwrap().pop(member);
        Ok(())
    }

    /// Count total attestations
    pub fn count(&self) -> Result<usize> {
        let entries = self.store.scan(ATTESTATION_PREFIX)?;
        Ok(entries.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attestation::TrustContext;
    use icn_identity::KeyPair;
    use icn_store::{SledStore, Store};

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_store_and_retrieve() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let att_store = AttestationStore::new(store);

        let source_did = test_did();
        let member_did = test_did();

        let att = FederatedTrustAttestation::new(
            "food-coop".to_string(),
            source_did,
            member_did.clone(),
            0.85,
            TrustContext::Economic,
            30 * 24 * 60 * 60,
        );

        att_store.store_attestation(att.clone()).unwrap();

        let retrieved = att_store.get_attestations_for(&member_did).unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].source_coop_id, "food-coop");
    }

    #[test]
    fn test_get_by_source_coop() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let att_store = AttestationStore::new(store);

        let att = FederatedTrustAttestation::new(
            "food-coop".to_string(),
            test_did(),
            test_did(),
            0.85,
            TrustContext::Economic,
            30 * 24 * 60 * 60,
        );

        att_store.store_attestation(att).unwrap();

        let from_coop = att_store.get_attestations_from("food-coop").unwrap();
        assert_eq!(from_coop.len(), 1);

        let from_other = att_store.get_attestations_from("other-coop").unwrap();
        assert!(from_other.is_empty());
    }
}
