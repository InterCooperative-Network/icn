//! Local restart durability for the N1 durable layer (icn#2799).
//!
//! # What this owns, and what it deliberately does not
//!
//! This module persists the two grow-only sets of [`super::AuthorityStore`] to a local medium and
//! reads them back. It owns the **record layout** and the **rehydration rule**. It does not own,
//! and must never acquire, any authority semantics: which body wins, whether a subject is halted,
//! how a fork resolves. Those belong to [`super::derive`] and stay a pure function of the admitted
//! sets.
//!
//! The mechanism that keeps it honest is that [`rehydrate`] has no privileged ingest path. Every
//! record it reads goes through [`super::admissible_bytes`] and then
//! [`super::AuthorityStore::ingest`] — the same gate a body arriving from the network passes. A
//! record that would not have been admitted live is not admitted because it came off a disk.
//!
//! # Why the derived state is identical after a restart
//!
//! Not because this module is careful about ordering. [`super::AuthorityBody`]'s `Ord`, `PartialEq`
//! and `Hash` are all defined over `canonical_bytes()`, so the `BTreeSet` rebuilt from these
//! records is ordered by content, not by the order rows came off the medium. Any backend, in any
//! iteration order, reconstructs the same set. Order-independence is a property of the body type
//! that this module inherits; it is not a promise a backend has to keep.
//!
//! # The record
//!
//! ```text
//! key   = event_id (32) || signature (64)
//! value = canonical_bytes(body)
//! ```
//!
//! The key is the [`super::Witness`] — a `(body digest, signature)` pair — because that is exactly
//! the durable layer's witness element. Two valid signatures over one body are two records, which
//! rehydrate to one body and two witnesses. That is the required outcome: several signatures over
//! one body are a re-signing, not duplicity, and collapsing them would mean *choosing* a signature,
//! which is a content tiebreak the durable lattice forbids (§9.2.1).
//!
//! Carrying the digest in the key rather than deriving it only from the value is what makes a
//! corrupted or substituted value detectable: [`PersistedFact::into_admitted`] recomputes the
//! digest from the decoded body and refuses a record whose key and value disagree.
//!
//! # The claim, stated narrowly
//!
//! Facts **successfully persisted through this store** survive an ordinary process or store restart
//! and rehydrate without semantic change. This is not a claim that a fact which never reached the
//! store, or which disappeared from the medium, can be detected here. Frontier and completeness
//! proofs are a separate concern and are not attempted.

use std::collections::BTreeMap;
use std::sync::RwLock;

use super::admission::{admissible_bytes, AdmissionError};
use super::body::EventId;
use super::store::{AuthorityStore, SignedAuthorityEvent, WitnessSignature};

/// Length of the `event_id` half of a record key.
const KEY_EVENT_ID_LEN: usize = 32;
/// Length of the signature half of a record key.
const KEY_SIGNATURE_LEN: usize = 64;
/// Total record-key length.
pub const FACT_KEY_LEN: usize = KEY_EVENT_ID_LEN + KEY_SIGNATURE_LEN;

/// Why a durable record could not become an admitted fact.
///
/// Every variant is a refusal. There is deliberately no variant meaning "skipped" or "repaired":
/// where ambiguity could affect authority, this layer fails closed and lets the caller decide,
/// because silently dropping one record changes the derived state without anything saying so.
#[derive(Debug, thiserror::Error)]
pub enum FactStoreError {
    /// The backing medium failed.
    #[error("authority fact store backend failed: {0}")]
    Backend(String),

    /// A record's key was not `event_id (32) || signature (64)`.
    #[error("record key is {actual} bytes, expected {FACT_KEY_LEN}")]
    MalformedKey {
        /// The length actually found.
        actual: usize,
    },

    /// The stored value did not decode, or would not have been admitted live.
    ///
    /// This is where corrupt bytes and an unknown protocol version land: `AuthorityBody::decode`
    /// rejects a frame whose version is not `PROTOCOL_VERSION`, so a record written by a future
    /// encoding is refused rather than guessed at.
    #[error("stored fact is not admissible: {0}")]
    Inadmissible(#[from] AdmissionError),

    /// The key names one body and the value decodes to another.
    ///
    /// A substituted or corrupted value that still happens to decode is caught here, and so is a
    /// backend that returned the wrong row for a key.
    #[error("record key names a different body than its value encodes")]
    KeyDisagreesWithValue,
}

/// One durable record: a witness, plus the canonical bytes of the body it witnesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedFact {
    event_id: EventId,
    signature: WitnessSignature,
    canonical_body: Vec<u8>,
}

impl PersistedFact {
    /// The record for an admitted signed event.
    pub fn from_event(event: &SignedAuthorityEvent) -> Self {
        PersistedFact {
            event_id: event.body.event_id(),
            signature: event.signature,
            canonical_body: event.body.canonical_bytes(),
        }
    }

    /// The storage key: `event_id || signature`.
    ///
    /// Two records share a key exactly when they are the same witness over the same body, so a
    /// backend that overwrites on duplicate keys gets replay idempotence for free.
    pub fn key(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(FACT_KEY_LEN);
        key.extend_from_slice(self.event_id.as_bytes());
        key.extend_from_slice(self.signature.as_bytes());
        key
    }

    /// The storage value: the body's canonical encoding.
    pub fn value(&self) -> &[u8] {
        &self.canonical_body
    }

    /// Rebuild a record from what a backend returned, without interpreting it.
    ///
    /// Key *shape* is checked here because a malformed key is a backend fault, not an authority
    /// question. Whether the record is admissible, and whether key and value agree, are decided by
    /// [`PersistedFact::into_admitted`].
    pub fn from_key_value(key: &[u8], value: &[u8]) -> Result<Self, FactStoreError> {
        if key.len() != FACT_KEY_LEN {
            return Err(FactStoreError::MalformedKey { actual: key.len() });
        }
        let mut event_id = [0u8; KEY_EVENT_ID_LEN];
        event_id.copy_from_slice(&key[..KEY_EVENT_ID_LEN]);
        let mut signature = [0u8; KEY_SIGNATURE_LEN];
        signature.copy_from_slice(&key[KEY_EVENT_ID_LEN..]);

        Ok(PersistedFact {
            event_id: EventId::from_bytes(event_id),
            signature: WitnessSignature::from_bytes(signature),
            canonical_body: value.to_vec(),
        })
    }

    /// Decode and re-admit this record, or refuse it.
    ///
    /// The admission check is [`super::admissible_bytes`], the same free function of body and
    /// signature that gates a live event. Storage therefore cannot widen what is admissible.
    pub fn into_admitted(self) -> Result<SignedAuthorityEvent, FactStoreError> {
        let body = admissible_bytes(&self.canonical_body, &self.signature)?;
        if body.event_id() != self.event_id {
            return Err(FactStoreError::KeyDisagreesWithValue);
        }
        Ok(SignedAuthorityEvent::new(body, self.signature))
    }
}

/// A local, restart-durable home for N1 authority facts.
///
/// Narrow on purpose. It moves records; it never decides anything about them. An implementation
/// that inspected a body, preferred one record over another, or dropped a record it could not
/// parse would be taking over part of [`super::derive`]'s job.
pub trait AuthorityFactStore: Send + Sync {
    /// Persist one record. Writing a record that is already present is a no-op, not an error.
    fn put_fact(&self, fact: &PersistedFact) -> Result<(), FactStoreError>;

    /// Every record currently held, in any order.
    ///
    /// Order is unconstrained because [`rehydrate`] cannot be made order-sensitive: the set it
    /// builds is ordered by body content.
    fn load_facts(&self) -> Result<Vec<PersistedFact>, FactStoreError>;
}

/// Rebuild the durable sets from a store's records.
///
/// Fails closed on the first record it cannot admit. A partially-rehydrated store is a different
/// subject history than the one that was persisted, and returning one would let a single corrupt
/// row silently change a derived authority state.
pub fn rehydrate(store: &dyn AuthorityFactStore) -> Result<AuthorityStore, FactStoreError> {
    let mut rebuilt = AuthorityStore::new();
    for fact in store.load_facts()? {
        let event = fact.into_admitted()?;
        rebuilt.ingest(&event)?;
    }
    Ok(rebuilt)
}

/// An in-memory [`AuthorityFactStore`] for tests and for callers that want no medium at all.
///
/// This is not a cache and not a fallback for a failed backend. It exists so the contract can be
/// tested without a filesystem, and so a caller can compose against the trait before choosing one.
#[derive(Debug, Default)]
pub struct InMemoryAuthorityFactStore {
    records: RwLock<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl InMemoryAuthorityFactStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many records are held.
    pub fn len(&self) -> usize {
        self.records
            .read()
            .map(|records| records.len())
            .unwrap_or(0)
    }

    /// Whether the store holds no records.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AuthorityFactStore for InMemoryAuthorityFactStore {
    fn put_fact(&self, fact: &PersistedFact) -> Result<(), FactStoreError> {
        let mut records = self
            .records
            .write()
            .map_err(|_| FactStoreError::Backend("in-memory store lock poisoned".into()))?;
        records.insert(fact.key(), fact.value().to_vec());
        Ok(())
    }

    fn load_facts(&self) -> Result<Vec<PersistedFact>, FactStoreError> {
        let records = self
            .records
            .read()
            .map_err(|_| FactStoreError::Backend("in-memory store lock poisoned".into()))?;
        records
            .iter()
            .map(|(key, value)| PersistedFact::from_key_value(key, value))
            .collect()
    }
}
