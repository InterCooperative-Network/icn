//! Attestation Store (Phase F2)
//!
//! Persistent storage for federated trust attestations.
//!
//! Persisted keys retain the exact `Did` spelling that was written, while
//! `Did` equality and hashing identify the decoded cryptographic principal.
//! Reads therefore classify the whole attestation namespace before returning a
//! principal-keyed result. This prevents an alternate valid spelling from
//! selecting a different persisted prefix or making lookup depend on call order.

use crate::attestation::FederatedTrustAttestation;
use crate::error::{FederationError, Result};
use crate::metrics;
use icn_identity::Did;
use icn_store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Storage key prefix
const ATTESTATION_PREFIX: &[u8] = b"federation/attestations/";

struct StoredAttestation {
    key: Vec<u8>,
    attestation: FederatedTrustAttestation,
}

/// Store for federated trust attestations
pub struct AttestationStore {
    store: Arc<dyn Store>,
}

impl AttestationStore {
    /// Create a new attestation store
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// Get the storage key for attestations of a member from a specific coop
    fn attestation_key(member_did: &Did, source_coop_id: &str) -> Vec<u8> {
        let mut key = ATTESTATION_PREFIX.to_vec();
        key.extend(member_did.as_str().as_bytes());
        key.push(b'/');
        key.extend(source_coop_id.as_bytes());
        key
    }

    /// Read and validate every persisted attestation row.
    ///
    /// The namespace is intentionally read as a whole. Persisted rows are keyed
    /// by spelling, while the domain meaning of `member_did` is a principal. A
    /// spelling-prefix read cannot discover another valid spelling of the same
    /// principal, which is the exact ambiguity this store must reject rather
    /// than hide behind a cache entry.
    fn load_checked_rows(&self) -> Result<Vec<StoredAttestation>> {
        let entries = self.store.scan(ATTESTATION_PREFIX)?;
        let mut rows = Vec::with_capacity(entries.len());

        for (key, value) in entries {
            let attestation = serde_json::from_slice::<FederatedTrustAttestation>(&value).map_err(
                |err| FederationError::AttestationStoreUnreadable {
                    reason: err.to_string(),
                },
            )?;

            let expected = Self::attestation_key(
                &attestation.member_did,
                &attestation.source_coop_id,
            );
            if key != expected {
                return Err(FederationError::AttestationStoreKeyValueMismatch {
                    source_coop_id: attestation.source_coop_id.clone(),
                });
            }

            rows.push(StoredAttestation { key, attestation });
        }

        Ok(rows)
    }

    /// Refuse two persisted claims from the same source cooperative about one
    /// principal.
    ///
    /// Attestations from *different* source cooperatives are the existing
    /// federation union and remain valid. Two rows for the same
    /// `(principal, source_coop_id)` may disagree on score, context, evidence,
    /// expiry, or signature. No federation-domain rule authorizes choosing or
    /// combining them, so reads fail closed instead of electing a survivor by
    /// spelling or scan order.
    fn ensure_unique_source_per_principal(rows: &[StoredAttestation]) -> Result<()> {
        let mut counts: HashMap<(Did, String), usize> = HashMap::new();

        for row in rows {
            *counts
                .entry((
                    row.attestation.member_did.clone(),
                    row.attestation.source_coop_id.clone(),
                ))
                .or_insert(0) += 1;
        }

        for ((_, source_coop_id), row_count) in counts {
            if row_count > 1 {
                return Err(FederationError::AttestationStorePrincipalCollision {
                    source_coop_id,
                    row_count,
                });
            }
        }

        Ok(())
    }

    /// Store an attestation
    pub fn store_attestation(&self, att: FederatedTrustAttestation) -> Result<()> {
        let rows = self.load_checked_rows()?;
        Self::ensure_unique_source_per_principal(&rows)?;

        let key = Self::attestation_key(&att.member_did, &att.source_coop_id);

        // Updating the exact row already present is ordinary replacement.
        // Writing a second spelling for the same principal/source would create
        // an ambiguity no domain merge rule authorizes, so refuse before write.
        if let Some(existing) = rows.iter().find(|row| {
            row.attestation.member_did == att.member_did
                && row.attestation.source_coop_id == att.source_coop_id
        }) {
            if existing.key != key {
                return Err(FederationError::AttestationStorePrincipalCollision {
                    source_coop_id: att.source_coop_id.clone(),
                    row_count: 2,
                });
            }
        }

        let value = serde_json::to_vec(&att)?;
        self.store.put(&key, &value)?;

        // Update metrics
        metrics::attestation::stored_inc(&att.source_coop_id, att.trust_context.as_str());

        debug!(
            "Stored attestation for {} from {}",
            att.member_did, att.source_coop_id
        );
        Ok(())
    }

    /// Get all attestations for a member principal.
    pub fn get_attestations_for(&self, member: &Did) -> Result<Vec<FederatedTrustAttestation>> {
        let rows = self.load_checked_rows()?;
        Self::ensure_unique_source_per_principal(&rows)?;

        Ok(rows
            .into_iter()
            .filter(|row| row.attestation.member_did == *member)
            .map(|row| row.attestation)
            .collect())
    }

    /// Get attestations from a specific cooperative
    pub fn get_attestations_from(&self, coop_id: &str) -> Result<Vec<FederatedTrustAttestation>> {
        let rows = self.load_checked_rows()?;
        Self::ensure_unique_source_per_principal(&rows)?;

        Ok(rows
            .into_iter()
            .filter(|row| row.attestation.source_coop_id == coop_id)
            .map(|row| row.attestation)
            .collect())
    }

    /// Remove expired attestations
    pub fn remove_expired(&self) -> Result<usize> {
        let rows = self.load_checked_rows()?;
        Self::ensure_unique_source_per_principal(&rows)?;
        let mut removed = 0;

        for row in rows {
            if row.attestation.is_expired() {
                self.store.delete(&row.key)?;
                removed += 1;
                metrics::attestation::expired_inc();
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

    /// Remove a specific source cooperative's attestation for a member
    /// principal.
    ///
    /// Removal is intentionally principal-wide for this source. If historical
    /// data already contains alias-spelled rows, deleting only the caller's
    /// spelling would make revocation representation-dependent. This path first
    /// reads and validates each stored row, then deletes the exact keys it read
    /// whose `(principal, source_coop_id)` matches the requested revocation. It
    /// does not merge, re-key, or choose one conflicting value.
    pub fn remove_attestation(&self, member: &Did, source_coop_id: &str) -> Result<()> {
        let rows = self.load_checked_rows()?;

        for row in rows {
            if row.attestation.member_did == *member
                && row.attestation.source_coop_id == source_coop_id
            {
                self.store.delete(&row.key)?;
            }
        }

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
    use std::str::FromStr;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn alias_spelling(did: &Did) -> Did {
        let bytes = did.identifier_bytes().expect("test DID must decode");
        let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes)))
            .expect("base16 spelling must parse");
        assert_ne!(did.as_str(), alias.as_str());
        assert_eq!(did, &alias);
        alias
    }

    fn attestation(source: &str, member_did: Did, score: f64) -> FederatedTrustAttestation {
        FederatedTrustAttestation::new(
            source.to_string(),
            test_did(),
            member_did,
            score,
            TrustContext::Economic,
            30 * 24 * 60 * 60,
        )
    }

    #[test]
    fn test_store_and_retrieve() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let att_store = AttestationStore::new(store);

        let member_did = test_did();
        let att = attestation("food-coop", member_did.clone(), 0.85);

        att_store.store_attestation(att.clone()).unwrap();

        let retrieved = att_store.get_attestations_for(&member_did).unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].source_coop_id, "food-coop");
    }

    #[test]
    fn lookup_is_principal_correct_in_both_alias_query_orders() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let att_store = AttestationStore::new(store);

        let member = test_did();
        let alias = alias_spelling(&member);

        att_store
            .store_attestation(attestation("food-coop", member.clone(), 0.85))
            .unwrap();
        att_store
            .store_attestation(attestation("housing-coop", alias.clone(), 0.75))
            .unwrap();

        let mut first = att_store
            .get_attestations_for(&member)
            .unwrap()
            .into_iter()
            .map(|att| att.source_coop_id)
            .collect::<Vec<_>>();
        first.sort();

        let mut second = att_store
            .get_attestations_for(&alias)
            .unwrap()
            .into_iter()
            .map(|att| att.source_coop_id)
            .collect::<Vec<_>>();
        second.sort();

        assert_eq!(first, vec!["food-coop", "housing-coop"]);
        assert_eq!(second, first);
    }

    #[test]
    fn same_principal_same_source_alias_rows_fail_closed() {
        let raw = Arc::new(SledStore::temporary().unwrap());
        let att_store = AttestationStore::new(raw.clone() as Arc<dyn Store>);

        let member = test_did();
        let alias = alias_spelling(&member);
        let first = attestation("food-coop", member.clone(), 0.85);
        let second = attestation("food-coop", alias.clone(), 0.25);

        raw.put(
            &AttestationStore::attestation_key(&member, "food-coop"),
            &serde_json::to_vec(&first).unwrap(),
        )
        .unwrap();
        raw.put(
            &AttestationStore::attestation_key(&alias, "food-coop"),
            &serde_json::to_vec(&second).unwrap(),
        )
        .unwrap();

        let err = att_store.get_attestations_for(&member).unwrap_err();
        assert!(matches!(
            err,
            FederationError::AttestationStorePrincipalCollision {
                source_coop_id,
                row_count: 2
            } if source_coop_id == "food-coop"
        ));
    }

    #[test]
    fn two_distinct_principals_are_not_conflated() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let att_store = AttestationStore::new(store);

        let first = test_did();
        let second = test_did();
        assert_ne!(first, second);

        att_store
            .store_attestation(attestation("food-coop", first.clone(), 0.85))
            .unwrap();
        att_store
            .store_attestation(attestation("food-coop", second.clone(), 0.65))
            .unwrap();

        assert_eq!(att_store.get_attestations_for(&first).unwrap().len(), 1);
        assert_eq!(att_store.get_attestations_for(&second).unwrap().len(), 1);
    }

    #[test]
    fn malformed_persisted_row_is_surfaced() {
        let raw = Arc::new(SledStore::temporary().unwrap());
        let att_store = AttestationStore::new(raw.clone() as Arc<dyn Store>);
        let member = test_did();

        raw.put(b"federation/attestations/malformed", b"{not-json")
            .unwrap();

        let err = att_store.get_attestations_for(&member).unwrap_err();
        assert!(matches!(
            err,
            FederationError::AttestationStoreUnreadable { .. }
        ));
    }

    #[test]
    fn removal_clears_all_alias_rows_for_principal_and_source() {
        let raw = Arc::new(SledStore::temporary().unwrap());
        let att_store = AttestationStore::new(raw.clone() as Arc<dyn Store>);

        let member = test_did();
        let alias = alias_spelling(&member);
        let first = attestation("food-coop", member.clone(), 0.85);
        let second = attestation("food-coop", alias.clone(), 0.25);

        raw.put(
            &AttestationStore::attestation_key(&member, "food-coop"),
            &serde_json::to_vec(&first).unwrap(),
        )
        .unwrap();
        raw.put(
            &AttestationStore::attestation_key(&alias, "food-coop"),
            &serde_json::to_vec(&second).unwrap(),
        )
        .unwrap();

        att_store.remove_attestation(&member, "food-coop").unwrap();
        assert!(raw.scan(ATTESTATION_PREFIX).unwrap().is_empty());
    }

    #[test]
    fn test_get_by_source_coop() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let att_store = AttestationStore::new(store);

        let att = attestation("food-coop", test_did(), 0.85);
        att_store.store_attestation(att).unwrap();

        let from_coop = att_store.get_attestations_from("food-coop").unwrap();
        assert_eq!(from_coop.len(), 1);

        let from_other = att_store.get_attestations_from("other-coop").unwrap();
        assert!(from_other.is_empty());
    }
}
