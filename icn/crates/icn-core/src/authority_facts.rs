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
use icn_store::{SledStore, Store};

/// Key prefix owning the authority-fact namespace in a shared store.
///
/// Public because tests and audits need to address these rows without re-deriving the spelling,
/// and a second spelling of a namespace is a second owner of it.
pub const AUTHORITY_FACT_PREFIX: &[u8] = b"authority/fact/";

/// A [`Store`] that can make its accepted writes durable on demand.
///
/// [`Store::flush`] defaults to a no-op, which is right for in-memory backends and wrong for
/// anything that promises restart durability. [`SledAuthorityFactStore`] acknowledges a fact only
/// after this trait's barrier returns, so over a backend whose barrier did nothing it would
/// acknowledge durability it never provided.
///
/// Two things keep that from happening by accident. A backend is eligible only through an explicit
/// `impl`, and that `impl` must write [`DurableStore::sync_durable`] itself: there is no default to
/// inherit, so a no-op barrier can exist only where someone wrote one and a reviewer can see it.
/// This is a claim, not a proof. Nothing here can check that a backend's barrier really reaches
/// stable storage.
pub trait DurableStore: Store {
    /// Return only once every write this store accepted before the call is on stable storage, or
    /// fail. Must not return `Ok` for a write that a crash could still lose.
    fn sync_durable(&self) -> anyhow::Result<()>;
}

impl DurableStore for SledStore {
    /// Sled's flush writes its buffered log and page cache out and syncs them
    /// ([`SledStore::flush`]).
    fn sync_durable(&self) -> anyhow::Result<()> {
        SledStore::flush(self).map(|_| ())
    }
}

/// A restart-durable [`AuthorityFactStore`] over a [`DurableStore`].
///
/// Named for the backend the runtime actually configures (sled), matching `SledMembershipStore`
/// and its siblings, though it constrains itself to [`DurableStore`] and so is not sled-specific.
pub struct SledAuthorityFactStore {
    store: Arc<dyn DurableStore>,
}

impl SledAuthorityFactStore {
    /// Wrap a backing store.
    pub fn new(store: Arc<dyn DurableStore>) -> Self {
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
    /// Persist one record and force it durable before returning.
    ///
    /// The barrier is not optional and not a caller's responsibility. [`Store::put`] writes
    /// through the backend but does not sync; [`DurableStore::sync_durable`] is the backend's
    /// explicit promise to do so, the same barrier the governance close journal forces before any
    /// cross-store artifact is written.
    ///
    /// Without it this type would return `Ok(())` for a fact that a crash or power loss can still
    /// erase, and the contract it implements says facts *successfully persisted through this store*
    /// survive a restart. A put that is not durable makes that claim false rather than merely
    /// weaker. Dropping a sled handle happens to flush, so a graceful reopen would have hidden this
    /// — which is exactly why the guarantee cannot rest on how the process happened to exit.
    ///
    /// On `Err` the fact may or may not be on the medium. It was not acknowledged, and writing it
    /// again is safe because a record's key is its identity.
    ///
    /// The cost is an fsync per fact. That is the honest price of the guarantee at this scale: N1
    /// events are rare, bounded per subject, and written at authority transitions rather than in a
    /// hot path. A batched `put_facts` that flushes once per batch is the right optimisation if a
    /// caller ever needs it, and belongs with that caller's evidence, not ahead of it.
    fn put_fact(&self, fact: &PersistedFact) -> Result<(), FactStoreError> {
        self.store
            .put(&Self::storage_key(&fact.key()), fact.value())
            .map_err(|error| FactStoreError::Backend(error.to_string()))?;
        self.store
            .sync_durable()
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
