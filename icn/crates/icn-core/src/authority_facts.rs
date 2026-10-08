//! The runtime-side persistence adapter for N1 authority facts (icn#2799).
//!
//! `icn-identity` owns the contract ([`AuthorityFactStore`]) and the record layout; it is
//! kernel-class and deliberately carries no dependency on `icn-store`. This module supplies the
//! concrete half, following the pattern the repository already uses for persisted domain facts: a
//! domain crate declares a narrow trait, and an adapter here holds an `Arc<dyn Store>` and maps the
//! domain's records onto keys and values.
//!
//! The adapter moves bytes and nothing else. It does not decode a body, compare two records,
//! prefer one over another, or drop one it cannot read — every such decision belongs to
//! [`icn_identity::authority_log::rehydrate`], which re-admits each record through the same gate a
//! live event passes. An adapter that started interpreting records would be taking over part of
//! the authority derivation.

use std::sync::Arc;

use icn_identity::authority_log::{AuthorityFactStore, FactStoreError, PersistedFact};
use icn_store::Store;

/// Key prefix owning the authority-fact namespace in a shared store.
///
/// Public because tests and audits need to address these rows without re-deriving the spelling,
/// and a second spelling of a namespace is a second owner of it.
pub const AUTHORITY_FACT_PREFIX: &[u8] = b"authority/fact/";

/// A restart-durable [`AuthorityFactStore`] over any [`Store`] backend.
///
/// Named for the backend the runtime actually configures (sled), matching `SledMembershipStore`
/// and its siblings, though it constrains itself to the `Store` trait and so is not sled-specific.
pub struct SledAuthorityFactStore {
    store: Arc<dyn Store>,
}

impl SledAuthorityFactStore {
    /// Wrap a backing store.
    pub fn new(store: Arc<dyn Store>) -> Self {
        SledAuthorityFactStore { store }
    }

    /// The backing key for a record key: the namespace prefix, then the record key verbatim.
    ///
    /// The record key is appended whole rather than re-encoded, so this adapter never becomes a
    /// second definition of the layout `icn-identity` owns.
    fn storage_key(record_key: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(AUTHORITY_FACT_PREFIX.len() + record_key.len());
        key.extend_from_slice(AUTHORITY_FACT_PREFIX);
        key.extend_from_slice(record_key);
        key
    }
}

impl AuthorityFactStore for SledAuthorityFactStore {
    fn put_fact(&self, fact: &PersistedFact) -> Result<(), FactStoreError> {
        self.store
            .put(&Self::storage_key(&fact.key()), fact.value())
            .map_err(|error| FactStoreError::Backend(error.to_string()))
    }

    fn load_facts(&self) -> Result<Vec<PersistedFact>, FactStoreError> {
        let rows = self
            .store
            .scan(AUTHORITY_FACT_PREFIX)
            .map_err(|error| FactStoreError::Backend(error.to_string()))?;

        rows.into_iter()
            .map(|(key, value)| {
                // A row returned under this prefix that does not carry it is a backend fault, not
                // an authority question — surface it rather than trimming blindly, because a
                // wrong-length trim would hand `from_key_value` a shifted key that could still be
                // the right length.
                let record_key = key.strip_prefix(AUTHORITY_FACT_PREFIX).ok_or_else(|| {
                    FactStoreError::Backend(
                        "scan returned a row outside the authority-fact namespace".into(),
                    )
                })?;
                PersistedFact::from_key_value(record_key, &value)
            })
            .collect()
    }
}
