//! Agreement Storage Layer
//!
//! Persistent storage for inter-cooperative agreements with a secondary index
//! for lookups by party.
//!
//! # The party index is a projection (N2-A, #2627)
//!
//! `federation/agreements/<id>` holds the canonical serialized [`Agreement`],
//! and its `parties` vector is the only statement of who is a party. Every
//! `idx_agreement_party/<party-did spelling>/<id>` row is derived from it: the
//! key names a party spelling and an agreement, the value repeats the agreement
//! id, and nothing in the row states a fact the canonical row does not already
//! state. The index accelerates discovery of canonical rows and is never
//! consulted as authority:
//!
//! * **A lookup answers by principal, from canonical state.** Since I7 (#2686)
//!   `Did` equality names the decoded principal, while index keys keep the
//!   spelling they were written with and a spelling-prefix read cannot find
//!   another spelling of the same principal. So
//!   [`AgreementStoreOps::list_agreements_for_party`] reads the whole
//!   projection, keeps the rows whose spelling names the queried principal,
//!   loads each candidate canonical row once, and returns only agreements whose
//!   `parties` contain the principal under `Did` equality. A row that points at
//!   a missing agreement, or at one that no longer lists the principal, is a
//!   stale projection row: it is not membership, and it is not an error,
//!   because the write protocol below can leave exactly such a row behind.
//! * **A row this store could never have written is refused.** An index key
//!   that does not parse as `idx_agreement_party/<spelling>/<id>`, a spelling
//!   that names no principal, or a value that names a different agreement than
//!   the key cannot be attributed to any fact. Operations that interpret the
//!   projection fail closed on such a row with
//!   [`FederationError::AgreementPartyIndexMalformed`] instead of skipping it,
//!   and an unreadable canonical row fails every operation that needs it with
//!   [`FederationError::AgreementStoreUnreadable`]. Unreadable state is
//!   evidence, not absence.
//! * **Writes keep the projection a superset of the truth, never a subset.**
//!   [`AgreementStoreOps::store_agreement`] writes the new party rows, then the
//!   canonical row, then removes the rows the previous canonical version
//!   implied and the new one does not.
//!   [`AgreementStoreOps::delete_agreement`] removes the canonical row and then
//!   every projection row naming that agreement under any spelling. A crash at
//!   any point leaves extra rows, which reads filter, and never a canonical row
//!   without its rows. Writers are serialized per store so two replacements of
//!   one agreement cannot interleave their cleanup.
//! * **The projection can be recomputed.**
//!   [`AgreementStore::rebuild_party_index`] derives the expected rows from
//!   every canonical row and makes the projection equal to them, reporting what
//!   it kept, added and removed. It refuses when any canonical row is
//!   unreadable, because the expected set cannot then be known. It never
//!   rewrites, re-keys or normalizes a canonical row.
//!
//! Persisted encodings are unchanged: canonical rows and index rows are the
//! bytes the previous implementation wrote. The N2-A collision scanner
//! registers this prefix (`icn_store::did_collision_scan::n2a_keyspaces`,
//! `icn-federation/agreement_party_index`) as `Equivalent`: two spellings of
//! one party for one agreement are two derivations of one canonical fact, so
//! keeping any one loses nothing. A test below pins that registration.

use super::types::{Agreement, AgreementId, AgreementStatus, Amendment};
use crate::error::{FederationError, Result};
use icn_identity::{identifier_bytes_of_spelling, Did};
use icn_store::Store;
use lru::LruCache;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use tracing::{debug, warn};

/// Storage key prefixes
const AGREEMENT_PREFIX: &[u8] = b"federation/agreements/";
/// Party-index prefix. The N2-A collision scanner scans exactly these bytes;
/// keep the two in step (see `scanner_registry_names_this_keyspace` below).
const AGREEMENT_PARTY_INDEX: &[u8] = b"idx_agreement_party/";
const AMENDMENT_PREFIX: &[u8] = b"federation/amendments/";

/// Default cache size for agreements
const DEFAULT_CACHE_SIZE: usize = 500;

/// Why a value failed to deserialize, without the value.
///
/// `serde_json`'s `Display` can echo input — the `Did` deserializer reports
/// the spelling it rejected — so only the error class and position travel.
fn unreadable_reason(err: &serde_json::Error) -> String {
    format!(
        "{:?} error at line {} column {}",
        err.classify(),
        err.line(),
        err.column()
    )
}

/// Longest agreement id echoed from a canonical key into an error, in characters.
const KEY_ID_ERROR_CAP: usize = 64;

/// The agreement id a canonical key names, escaped and bounded for an error
/// message.
///
/// Taken from the key, never from the value: the key is the locator an
/// operator must inspect. It is still untrusted — this function runs only for
/// rows the store did not write — so every character is escaped
/// (`char::escape_default`: control characters, quotes and non-ASCII become
/// escape sequences) before the cap is applied. The cap counts shown
/// characters and never cuts inside an escape sequence, so what reaches a log
/// is one line, bounded, and lossless for any id the store itself would mint.
fn bounded_key_id(key: &[u8]) -> String {
    let id = key.strip_prefix(AGREEMENT_PREFIX).unwrap_or(key);
    let mut out = String::new();
    let mut shown = 0usize;
    let mut truncated = false;
    for c in String::from_utf8_lossy(id).chars() {
        let escaped: String = c.escape_default().collect();
        let width = escaped.chars().count();
        if shown + width > KEY_ID_ERROR_CAP {
            truncated = true;
            break;
        }
        shown += width;
        out.push_str(&escaped);
    }
    if truncated {
        out.push('…');
    }
    out
}

/// One parsed projection row, attributed to a principal and an agreement.
struct PartyIndexRow {
    key: Vec<u8>,
    /// The 32 identifier bytes the key's spelling decodes to — the same
    /// decode `Did` equality uses, so a match here is a match under I7.
    identifier: [u8; 32],
    agreement_id: AgreementId,
}

/// What [`AgreementStore::rebuild_party_index`] did. Counts only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PartyIndexRebuild {
    /// Canonical agreements the projection was derived from.
    pub agreements: usize,
    /// Rows those agreements imply.
    pub rows_expected: usize,
    /// Expected rows already present, left untouched.
    pub rows_kept: usize,
    /// Expected rows that were missing and have been written.
    pub rows_added: usize,
    /// Well-formed rows no canonical agreement implies, now removed.
    pub rows_removed_stale: usize,
    /// Rows that could not be attributed to any agreement, now removed.
    pub rows_removed_malformed: usize,
}

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
    /// Serializes writers. A replacement reads the previous canonical row to
    /// learn which projection rows it supersedes; two interleaved replacements
    /// of one agreement could otherwise each retire a row the other's canonical
    /// version still implies, leaving a canonical row without its projection.
    write_lock: Mutex<()>,
}

impl AgreementStore {
    /// Create a new agreement store
    pub fn new(store: Arc<dyn Store>) -> Self {
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap();
        Self {
            store,
            cache: RwLock::new(LruCache::new(capacity)),
            write_lock: Mutex::new(()),
        }
    }

    /// Create with custom cache size
    pub fn with_cache_size(store: Arc<dyn Store>, cache_size: usize) -> Self {
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(cache_size.max(1)).unwrap();
        Self {
            store,
            cache: RwLock::new(LruCache::new(capacity)),
            write_lock: Mutex::new(()),
        }
    }

    fn write_guard(&self) -> MutexGuard<'_, ()> {
        // A writer that panicked while holding the lock has not left the store
        // in a state the lock protects against; later writers re-read the
        // canonical row before acting.
        self.write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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

    /// Decode one canonical row and check that it is the agreement its key
    /// names.
    ///
    /// The key locates the row; the value must be that agreement. A value that
    /// deserializes but carries another agreement's id is one row's value
    /// under another row's key — the fingerprint of a collapsed rebuild's
    /// write-back, or of raw tampering. It is attributed to neither agreement:
    /// returning it would let a replacement retire the *other* agreement's
    /// projection rows, a rebuild call the named agreement's real rows stale,
    /// and a lookup report a party absent from an agreement it never read.
    /// An unreadable value is surfaced without its bytes.
    fn decode_canonical(key: &[u8], value: &[u8]) -> Result<Agreement> {
        let agreement = serde_json::from_slice::<Agreement>(value).map_err(|err| {
            FederationError::AgreementStoreUnreadable {
                key_len: key.len(),
                value_len: value.len(),
                reason: unreadable_reason(&err),
            }
        })?;
        if Self::agreement_key(&agreement.id) != key {
            return Err(FederationError::AgreementStoreKeyValueMismatch {
                key_agreement_id: bounded_key_id(key),
                value_len: value.len(),
            });
        }
        Ok(agreement)
    }

    /// Read one canonical row from the backend, bypassing the cache.
    ///
    /// Writers use this rather than the cache: the cache is a convenience,
    /// and an unreadable canonical row must stop a write rather than be
    /// overwritten by it.
    fn load_canonical(&self, id: &AgreementId) -> Result<Option<Agreement>> {
        let key = Self::agreement_key(id);
        match self.store.get(&key)? {
            Some(value) => Ok(Some(Self::decode_canonical(&key, &value)?)),
            None => Ok(None),
        }
    }

    /// Read every canonical row. Fails closed on the first unreadable one:
    /// a listing that silently omitted it would turn unreadable state into
    /// absent state.
    fn load_all_canonical(&self) -> Result<Vec<Agreement>> {
        let entries = self.store.scan(AGREEMENT_PREFIX)?;
        let mut agreements = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            agreements.push(Self::decode_canonical(&key, &value)?);
        }
        Ok(agreements)
    }

    /// Parse one projection row: `idx_agreement_party/<spelling>/<id>` whose
    /// value is `<id>`.
    ///
    /// The split is anchored on the value rather than on a separator: a
    /// base64 spelling legitimately contains `/`, so the only sound reading is
    /// that the key must end with `/` followed by exactly the agreement id its
    /// value names. Anything else is a row this store never wrote.
    fn parse_party_index_row(
        key: &[u8],
        value: &[u8],
    ) -> std::result::Result<PartyIndexRow, String> {
        let rest = key
            .strip_prefix(AGREEMENT_PARTY_INDEX)
            .ok_or_else(|| "key lacks the projection prefix".to_string())?;
        let id = std::str::from_utf8(value)
            .map_err(|_| format!("{}-byte value is not UTF-8", value.len()))?;
        if id.is_empty() {
            return Err("empty agreement id".to_string());
        }
        let spelling = rest
            .strip_suffix(id.as_bytes())
            .and_then(|s| s.strip_suffix(b"/"))
            .ok_or_else(|| {
                format!(
                    "{}-byte key does not end with the {}-byte agreement id its value names",
                    key.len(),
                    id.len()
                )
            })?;
        let spelling = std::str::from_utf8(spelling)
            .map_err(|_| format!("{}-byte spelling is not UTF-8", spelling.len()))?;
        let identifier = identifier_bytes_of_spelling(spelling)
            .map_err(|_| format!("{}-byte spelling names no principal", spelling.len()))?;
        Ok(PartyIndexRow {
            key: key.to_vec(),
            identifier,
            agreement_id: AgreementId::new(id),
        })
    }

    /// Read and attribute the whole projection.
    ///
    /// Every row is classified before anything is returned, so the refusal
    /// counts all malformed rows rather than the first. A malformed row is
    /// refused rather than skipped: it is a row this store could not have
    /// written, and reading around it would let the projection say something
    /// canonical state does not.
    fn load_party_index(&self) -> Result<Vec<PartyIndexRow>> {
        let entries = self.store.scan(AGREEMENT_PARTY_INDEX)?;
        let mut rows = Vec::with_capacity(entries.len());
        let mut malformed = 0usize;
        let mut first_reason: Option<String> = None;
        for (key, value) in entries {
            match Self::parse_party_index_row(&key, &value) {
                Ok(row) => rows.push(row),
                Err(reason) => {
                    malformed += 1;
                    first_reason.get_or_insert(reason);
                }
            }
        }
        if malformed > 0 {
            return Err(FederationError::AgreementPartyIndexMalformed {
                rows: malformed,
                first_reason: first_reason.unwrap_or_default(),
            });
        }
        Ok(rows)
    }

    /// The projection rows one canonical agreement implies: one per party,
    /// under the spelling the canonical row carries, valued by the agreement
    /// id. This is the whole derivation; nothing else feeds the index.
    fn implied_rows(agreement: &Agreement) -> BTreeMap<Vec<u8>, Vec<u8>> {
        agreement
            .parties
            .iter()
            .map(|party| {
                (
                    Self::party_index_key(&party.did, &agreement.id),
                    agreement.id.as_str().as_bytes().to_vec(),
                )
            })
            .collect()
    }

    /// Newest first, then by id, so the order is a function of the data and
    /// not of scan order, spelling or cache state.
    fn sort_newest_first(agreements: &mut [Agreement]) {
        agreements.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
    }

    /// Recompute the party index from the canonical agreement rows.
    ///
    /// The expected projection is exactly the rows every canonical agreement
    /// implies. Rows already present and expected are kept; expected rows
    /// that are missing are written; well-formed rows nothing implies are
    /// removed as stale; rows that cannot be attributed to any agreement are
    /// removed as malformed. The report says how many of each, so the
    /// operation leaves a record of what it changed.
    ///
    /// This is a projection recomputation, not a data clean-up: no row here
    /// states a fact the canonical rows do not, so removing one discards no
    /// institutional evidence, and no canonical byte is touched. It refuses
    /// before mutating anything if a canonical row is unreadable, because the
    /// expected set cannot then be known.
    pub fn rebuild_party_index(&self) -> Result<PartyIndexRebuild> {
        let _guard = self.write_guard();

        let agreements = self.load_all_canonical()?;
        let mut expected: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
        for agreement in &agreements {
            expected.extend(Self::implied_rows(agreement));
        }

        let mut report = PartyIndexRebuild {
            agreements: agreements.len(),
            rows_expected: expected.len(),
            ..PartyIndexRebuild::default()
        };

        let mut present: BTreeSet<Vec<u8>> = BTreeSet::new();
        for (key, value) in self.store.scan(AGREEMENT_PARTY_INDEX)? {
            match Self::parse_party_index_row(&key, &value) {
                Ok(_) if expected.contains_key(&key) => {
                    present.insert(key);
                    report.rows_kept += 1;
                }
                Ok(_) => {
                    self.store.delete(&key)?;
                    report.rows_removed_stale += 1;
                }
                Err(_) => {
                    self.store.delete(&key)?;
                    report.rows_removed_malformed += 1;
                }
            }
        }

        for (key, value) in &expected {
            if !present.contains(key) {
                self.store.put(key, value)?;
                report.rows_added += 1;
            }
        }

        debug!(?report, "Rebuilt the agreement party index");
        Ok(report)
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
        let _guard = self.write_guard();

        let key = Self::agreement_key(&agreement.id);
        let value = serde_json::to_vec(agreement)?;

        // Which projection rows did the version being replaced imply? Read
        // from the backend: an unreadable canonical row stops the write here,
        // before any byte moves, rather than being overwritten.
        let previous = self.load_canonical(&agreement.id)?;
        let new_rows = Self::implied_rows(agreement);

        // Projection first, then the canonical row, so a crash between the
        // two leaves extra projection rows and never a canonical row without
        // its rows. Reads filter extra rows; they cannot recover missing ones.
        for (index_key, index_value) in &new_rows {
            self.store.put(index_key, index_value)?;
        }
        self.store.put(&key, &value)?;

        // Retire the rows the previous version implied and this one does not:
        // a removed party, or a party the new version spells differently.
        if let Some(previous) = previous {
            for (index_key, _) in Self::implied_rows(&previous) {
                if !new_rows.contains_key(&index_key) {
                    self.store.delete(&index_key)?;
                }
            }
        }

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

        let loaded = self.load_canonical(id)?;
        if let Some(agreement) = &loaded {
            self.update_cache(agreement);
        }
        Ok(loaded)
    }

    fn delete_agreement(&self, id: &AgreementId) -> Result<()> {
        let _guard = self.write_guard();

        // An unreadable canonical row stops the delete before any byte moves:
        // deleting it would destroy the evidence an operator needs.
        let _existing = self.load_canonical(id)?;

        // Every projection row naming this agreement, under any spelling —
        // including rows earlier versions left behind. The whole projection is
        // attributed first, so a malformed projection refuses the operation
        // rather than letting an unattributable row survive a deletion.
        let mine: Vec<Vec<u8>> = self
            .load_party_index()?
            .into_iter()
            .filter(|row| row.agreement_id == *id)
            .map(|row| row.key)
            .collect();
        let amendments = self.store.scan_keys(&Self::amendment_prefix(id))?;

        // Canonical row first, then its dependents, so a crash leaves extra
        // projection rows and never a canonical row without its rows.
        self.store.delete(&Self::agreement_key(id))?;
        for amendment_key in amendments {
            self.store.delete(&amendment_key)?;
        }
        for index_key in mine {
            self.store.delete(&index_key)?;
        }

        self.invalidate_cache(id);

        debug!("Deleted agreement {}", id);
        Ok(())
    }

    fn list_agreements(&self) -> Result<Vec<Agreement>> {
        let mut agreements = self.load_all_canonical()?;
        Self::sort_newest_first(&mut agreements);
        Ok(agreements)
    }

    fn list_agreements_for_party(&self, party_did: &Did) -> Result<Vec<Agreement>> {
        // The projection is read and attributed before the query is even
        // looked at, so a malformed row is refused whatever is asked: refusal
        // is a property of the persisted state, not of the query.
        let rows = self.load_party_index()?;

        // A `Did` that names no principal is equal only to its own spelling,
        // and every attributed projection row names a principal, so it can
        // match none of them — the same answer `Did` equality gives. No `Did`
        // a caller can construct reaches this arm (`from_str`, `from_public_key`
        // and deserialization all validate); it is kept so the arm states its
        // own answer rather than relying on that.
        let Ok(wanted) = party_did.identifier_bytes() else {
            return Ok(Vec::new());
        };

        // Candidates: every projection row whose spelling names the principal,
        // under any spelling, de-duplicated by agreement id.
        let candidates: BTreeSet<String> = rows
            .into_iter()
            .filter(|row| row.identifier == wanted)
            .map(|row| row.agreement_id.as_str().to_string())
            .collect();

        // Membership is decided by the canonical row, under `Did` equality.
        let mut agreements = Vec::with_capacity(candidates.len());
        for id in candidates {
            let id = AgreementId::new(id);
            match self.get_agreement(&id)? {
                Some(agreement) if agreement.parties.iter().any(|p| p.did == *party_did) => {
                    agreements.push(agreement);
                }
                Some(_) => debug!(
                    "Stale party-index row: agreement {} no longer lists the queried party",
                    id
                ),
                None => debug!("Stale party-index row: agreement {} does not exist", id),
            }
        }

        Self::sort_newest_first(&mut agreements);
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
        amendments.sort_by_key(|a| a.proposed_at);

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
        AgreementStore::sort_newest_first(&mut agreements);
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
        AgreementStore::sort_newest_first(&mut agreements);
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
        amendments.sort_by_key(|a| a.proposed_at);
        Ok(amendments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agreement::{AgreementParty, AgreementType, PartyRole};
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

    /// Helper to open a SledStore with retry logic to handle OS-level file lock contention.
    ///
    /// Sled uses file locks that may not release deterministically on drop. This helper
    /// retries with exponential backoff to handle intermittent lock contention in tests.
    ///
    /// # Arguments
    /// * `store_path` - Path to the Sled database
    /// * `max_attempts` - Maximum number of attempts (1 = no retries)
    ///
    /// # Returns
    /// Arc<icn_store::SledStore> on success, panics after max_attempts failures.
    fn retry_open_sled(
        store_path: &std::path::Path,
        max_attempts: usize,
    ) -> Arc<icn_store::SledStore> {
        let mut attempt = 1;
        loop {
            match icn_store::SledStore::open(store_path) {
                Ok(store) => return Arc::new(store),
                Err(e) if attempt < max_attempts => {
                    eprintln!(
                        "Attempt {}/{} to open store failed: {}. Retrying...",
                        attempt, max_attempts, e
                    );
                    // Exponential backoff: 100ms, 200ms, 400ms, 800ms, ...
                    let delay_ms = 100 * (1u64 << (attempt - 1));
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    attempt += 1;
                }
                Err(e) => panic!(
                    "Failed to open store after {} attempts: {}",
                    max_attempts, e
                ),
            }
        }
    }

    #[test]
    fn test_persistent_store_survives_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store_path = temp_dir.path().join("agreements");

        // First "session" - create and store agreement
        let agreement_id;
        let party_did;
        {
            // Keep concrete Arc<SledStore> so we can call flush() explicitly,
            // then cast to trait object for AgreementStore.
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
        let sled_store = retry_open_sled(&store_path, 5);

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
            // Keep concrete Arc<SledStore> so we can call flush() explicitly,
            // then cast to trait object for AgreementStore.
            let sled_store = Arc::new(icn_store::SledStore::open(&store_path).unwrap());
            let store = AgreementStore::new(sled_store.clone() as Arc<dyn Store>);

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

            // Explicitly flush and drop in correct order to release file lock
            sled_store.flush().unwrap();
            drop(store);
            drop(sled_store);
        }

        // Wait for OS-level lock release
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Second session - reopen with retry logic to handle lock contention
        let sled_store = retry_open_sled(&store_path, 5);

        {
            let store = AgreementStore::new(sled_store as Arc<dyn Store>);

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

    // ----- N2-A: the party index is a projection of canonical rows (#2627) -----
    //
    // Fixtures run against a real `SledStore`. They write raw rows where the
    // hazard is a row the store itself would never write, because the store's
    // own API cannot produce a malformed projection row.

    fn open_pair() -> (Arc<icn_store::SledStore>, AgreementStore) {
        let raw = Arc::new(icn_store::SledStore::temporary().unwrap());
        let store = AgreementStore::new(raw.clone() as Arc<dyn Store>);
        (raw, store)
    }

    /// The same principal under another accepted multibase spelling.
    fn respell(did: &Did, base: multibase::Base) -> Did {
        let bytes = did.identifier_bytes().unwrap();
        let alias = Did::from_str(&format!("did:icn:{}", multibase::encode(base, bytes))).unwrap();
        assert_ne!(
            alias.as_str(),
            did.as_str(),
            "fixture must produce a distinct spelling"
        );
        assert_eq!(alias, *did, "fixture spellings must name one principal");
        alias
    }

    fn two_party_agreement(a: &Did, b: &Did) -> Agreement {
        Agreement::new(
            "Projection",
            "N2-A party index",
            AgreementType::Credit {
                credit_limit: 1,
                interest_rate_bps: 1,
                currency: "ICN".to_string(),
            },
        )
        .with_party(a.clone(), "coop-a", PartyRole::Proposer)
        .with_party(b.clone(), "coop-b", PartyRole::Counterparty)
    }

    fn index_keys(raw: &icn_store::SledStore) -> Vec<Vec<u8>> {
        raw.scan_keys(AGREEMENT_PARTY_INDEX).unwrap()
    }

    fn ids_of(agreements: &[Agreement]) -> Vec<String> {
        let mut ids: Vec<String> = agreements
            .iter()
            .map(|a| a.id.as_str().to_string())
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn lookup_under_an_alternate_spelling_finds_the_same_agreements() {
        let (_raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();

        let by_stored = store.list_agreements_for_party(&a).unwrap();
        let by_alias = store
            .list_agreements_for_party(&respell(&a, multibase::Base::Base16Lower))
            .unwrap();
        let by_other_alias = store
            .list_agreements_for_party(&respell(&a, multibase::Base::Base32Lower))
            .unwrap();

        assert_eq!(ids_of(&by_stored), vec![agreement.id.as_str().to_string()]);
        assert_eq!(ids_of(&by_alias), ids_of(&by_stored));
        assert_eq!(ids_of(&by_other_alias), ids_of(&by_stored));
    }

    #[test]
    fn a_removed_party_no_longer_lists_the_agreement() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let mut agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();
        assert_eq!(store.list_agreements_for_party(&b).unwrap().len(), 1);

        // The party set changes after first persistence, as a ratified
        // `RemoveParty` amendment or a gossip sync replacement makes it.
        agreement.parties.retain(|p| p.did != b);
        store.store_agreement(&agreement).unwrap();

        assert!(
            store.list_agreements_for_party(&b).unwrap().is_empty(),
            "the projection must not preserve membership canonical state no longer contains"
        );
        assert_eq!(store.list_agreements_for_party(&a).unwrap().len(), 1);
        assert!(
            !index_keys(&raw).contains(&AgreementStore::party_index_key(&b, &agreement.id)),
            "the superseded row is retired, not merely filtered"
        );
    }

    #[test]
    fn a_malformed_index_value_is_surfaced_not_skipped() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        store.store_agreement(&two_party_agreement(&a, &b)).unwrap();

        let mut key = AGREEMENT_PARTY_INDEX.to_vec();
        key.extend(a.as_str().as_bytes());
        key.extend(b"/agr-x");
        raw.put(&key, &[0xff, 0xfe]).unwrap();

        assert!(
            store.list_agreements_for_party(&a).is_err(),
            "a projection row the store could never have written is evidence, not absence"
        );
    }

    #[test]
    fn an_index_row_cannot_manufacture_membership() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let outsider = test_did();
        let agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();

        // Well-formed row, wrong fact: says `outsider` is a party. Canonical
        // state says otherwise, and canonical state wins.
        let stale_key = AgreementStore::party_index_key(&outsider, &agreement.id);
        raw.put(&stale_key, agreement.id.as_str().as_bytes())
            .unwrap();
        assert!(
            store
                .list_agreements_for_party(&outsider)
                .unwrap()
                .is_empty(),
            "an index row is never sufficient to establish party membership"
        );

        // Key and value disagree about which agreement the row names.
        let mut mismatched = AGREEMENT_PARTY_INDEX.to_vec();
        mismatched.extend(outsider.as_str().as_bytes());
        mismatched.extend(b"/agr-not-this-one");
        raw.put(&mismatched, agreement.id.as_str().as_bytes())
            .unwrap();
        assert!(
            store.list_agreements_for_party(&outsider).is_err(),
            "a key that disagrees with its value cannot be attributed and must be surfaced"
        );
    }

    #[test]
    fn an_index_row_pointing_at_a_missing_agreement_is_tolerated_as_stale() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();

        let dangling = AgreementStore::party_index_key(&a, &AgreementId::new("agr-gone"));
        raw.put(&dangling, b"agr-gone").unwrap();

        // The write protocol can leave exactly this row behind (a crash between
        // the canonical delete and the projection delete), so it is not
        // corruption; it is simply not membership.
        assert_eq!(
            ids_of(&store.list_agreements_for_party(&a).unwrap()),
            vec![agreement.id.as_str().to_string()]
        );
    }

    #[test]
    fn duplicate_alias_index_rows_do_not_duplicate_the_agreement() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();

        // A second spelling of `a` for the same agreement, as an older writer
        // could have left it.
        let alias = respell(&a, multibase::Base::Base16Lower);
        raw.put(
            &AgreementStore::party_index_key(&alias, &agreement.id),
            agreement.id.as_str().as_bytes(),
        )
        .unwrap();
        assert_eq!(index_keys(&raw).len(), 3);

        for spelling in [&a, &alias, &respell(&a, multibase::Base::Base32Upper)] {
            let found = store.list_agreements_for_party(spelling).unwrap();
            assert_eq!(
                found.len(),
                1,
                "one canonical agreement, however many projection rows"
            );
            assert_eq!(found[0].id, agreement.id);
        }
    }

    #[test]
    fn deleting_an_agreement_removes_every_projection_row() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let agreement = two_party_agreement(&a, &b);
        let other = two_party_agreement(&b, &test_did());
        store.store_agreement(&agreement).unwrap();
        store.store_agreement(&other).unwrap();

        // A stale alias row for the agreement being deleted, plus rows for an
        // unrelated agreement that must survive untouched.
        let alias = respell(&a, multibase::Base::Base16Lower);
        raw.put(
            &AgreementStore::party_index_key(&alias, &agreement.id),
            agreement.id.as_str().as_bytes(),
        )
        .unwrap();
        let before: std::collections::BTreeSet<Vec<u8>> = index_keys(&raw).into_iter().collect();
        assert_eq!(before.len(), 5);

        store.delete_agreement(&agreement.id).unwrap();

        let after: std::collections::BTreeSet<Vec<u8>> = index_keys(&raw).into_iter().collect();
        let survivors: Vec<&Vec<u8>> = after.iter().collect();
        assert_eq!(
            survivors.len(),
            2,
            "only the unrelated agreement's rows remain: {survivors:?}"
        );
        for key in &after {
            assert!(key.ends_with(other.id.as_str().as_bytes()));
            assert!(before.contains(key), "no unrelated row was rewritten");
        }
        assert!(store.get_agreement(&agreement.id).unwrap().is_none());
        assert_eq!(store.list_agreements_for_party(&b).unwrap().len(), 1);
    }

    #[test]
    fn a_malformed_canonical_row_fails_listing_closed() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        store.store_agreement(&two_party_agreement(&a, &b)).unwrap();
        raw.put(
            &AgreementStore::agreement_key(&AgreementId::new("agr-bad")),
            b"{not json",
        )
        .unwrap();

        assert!(
            store.list_agreements().is_err(),
            "unreadable canonical state is evidence, not absence"
        );
        assert!(store.get_agreement(&AgreementId::new("agr-bad")).is_err());
        // A lookup that must load the unreadable row fails closed too.
        raw.put(
            &AgreementStore::party_index_key(&a, &AgreementId::new("agr-bad")),
            b"agr-bad",
        )
        .unwrap();
        assert!(store.list_agreements_for_party(&a).is_err());
    }

    // ----- N2-A: properties the projection design must hold ----------------------

    #[test]
    fn two_distinct_principals_stay_distinct() {
        let (_raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let c = test_did();
        let ab = two_party_agreement(&a, &b);
        let bc = two_party_agreement(&b, &c);
        store.store_agreement(&ab).unwrap();
        store.store_agreement(&bc).unwrap();

        assert_eq!(
            ids_of(&store.list_agreements_for_party(&a).unwrap()),
            ids_of(std::slice::from_ref(&ab))
        );
        assert_eq!(
            ids_of(&store.list_agreements_for_party(&c).unwrap()),
            ids_of(std::slice::from_ref(&bc))
        );
        assert_eq!(
            ids_of(&store.list_agreements_for_party(&b).unwrap()),
            ids_of(&[ab, bc])
        );
        assert!(store
            .list_agreements_for_party(&test_did())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn one_principal_in_several_agreements_under_several_spellings() {
        let (_raw, store) = open_pair();
        let a = test_did();
        let alias = respell(&a, multibase::Base::Base16Lower);
        let first = two_party_agreement(&a, &test_did());
        // The same principal enters the second agreement under another spelling —
        // as a gossip peer that spells it differently would write it.
        let second = two_party_agreement(&alias, &test_did());
        store.store_agreement(&first).unwrap();
        store.store_agreement(&second).unwrap();

        let expected = ids_of(&[first, second]);
        assert_eq!(
            ids_of(&store.list_agreements_for_party(&a).unwrap()),
            expected
        );
        assert_eq!(
            ids_of(&store.list_agreements_for_party(&alias).unwrap()),
            expected
        );
        assert_eq!(
            ids_of(
                &store
                    .list_agreements_for_party(&respell(&a, multibase::Base::Base64))
                    .unwrap()
            ),
            expected
        );
    }

    #[test]
    fn insertion_order_does_not_change_the_answer() {
        let a = test_did();
        let b = test_did();
        let x = two_party_agreement(&a, &b);
        let y = two_party_agreement(&respell(&a, multibase::Base::Base16Lower), &b);

        let (_r1, forward) = open_pair();
        forward.store_agreement(&x).unwrap();
        forward.store_agreement(&y).unwrap();
        let (_r2, reversed) = open_pair();
        reversed.store_agreement(&y).unwrap();
        reversed.store_agreement(&x).unwrap();

        for spelling in [&a, &respell(&a, multibase::Base::Base32Lower), &b] {
            let f: Vec<String> = forward
                .list_agreements_for_party(spelling)
                .unwrap()
                .iter()
                .map(|g| g.id.as_str().to_string())
                .collect();
            let r: Vec<String> = reversed
                .list_agreements_for_party(spelling)
                .unwrap()
                .iter()
                .map(|g| g.id.as_str().to_string())
                .collect();
            assert_eq!(
                f, r,
                "order and content are a function of the data, not of history"
            );
            assert_eq!(f.len(), 2);
        }
    }

    #[test]
    fn a_fresh_handle_over_the_same_backend_agrees_with_a_warm_one() {
        let (raw, warm) = open_pair();
        let a = test_did();
        let b = test_did();
        let mut agreement = two_party_agreement(&a, &b);
        warm.store_agreement(&agreement).unwrap();
        // Warm the cache, then change the party set through the same handle.
        assert_eq!(warm.list_agreements_for_party(&b).unwrap().len(), 1);
        agreement.parties.retain(|p| p.did != b);
        warm.store_agreement(&agreement).unwrap();

        let cold = AgreementStore::new(raw as Arc<dyn Store>);
        for store in [&warm, &cold] {
            assert!(store.list_agreements_for_party(&b).unwrap().is_empty());
            assert_eq!(
                store
                    .list_agreements_for_party(&respell(&a, multibase::Base::Base16Lower))
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn replacement_that_adds_a_party_is_visible_under_any_spelling() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let c = test_did();
        let mut agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();
        assert!(store.list_agreements_for_party(&c).unwrap().is_empty());

        agreement.parties.push(AgreementParty::new(
            c.clone(),
            "coop-c",
            PartyRole::Guarantor,
        ));
        store.store_agreement(&agreement).unwrap();

        assert_eq!(store.list_agreements_for_party(&c).unwrap().len(), 1);
        assert_eq!(
            store
                .list_agreements_for_party(&respell(&c, multibase::Base::Base16Lower))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(index_keys(&raw).len(), 3);
    }

    #[test]
    fn replacement_that_only_respells_a_party_retires_the_old_row() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let mut agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();
        let before = index_keys(&raw);
        assert_eq!(before.len(), 2);

        // Same principals, one spelled differently — a sync from a peer that
        // spells `a` in another base.
        let alias = respell(&a, multibase::Base::Base16Lower);
        agreement.parties[0].did = alias.clone();
        store.store_agreement(&agreement).unwrap();

        let after = index_keys(&raw);
        assert_eq!(
            after.len(),
            2,
            "exactly the rows the canonical row implies: {after:?}"
        );
        assert!(after.contains(&AgreementStore::party_index_key(&alias, &agreement.id)));
        assert!(!after.contains(&AgreementStore::party_index_key(&a, &agreement.id)));
        for spelling in [&a, &alias] {
            assert_eq!(store.list_agreements_for_party(spelling).unwrap().len(), 1);
        }
    }

    #[test]
    fn a_malformed_index_key_is_surfaced_with_a_count_and_no_bytes() {
        let (raw, store) = open_pair();
        let a = test_did();
        store
            .store_agreement(&two_party_agreement(&a, &test_did()))
            .unwrap();

        // A spelling that names no principal, and a key with no `/` at all.
        raw.put(
            b"idx_agreement_party/did:icn:znotaprincipal/agr-1",
            b"agr-1",
        )
        .unwrap();
        raw.put(b"idx_agreement_party/garbage", b"agr-1").unwrap();

        // Refusal is a property of the persisted state, not of the query:
        // a principal that matches no row is refused exactly the same way.
        assert!(matches!(
            store.list_agreements_for_party(&test_did()),
            Err(FederationError::AgreementPartyIndexMalformed { .. })
        ));
        let err = store.list_agreements_for_party(&a).unwrap_err();
        match &err {
            FederationError::AgreementPartyIndexMalformed { rows, first_reason } => {
                assert_eq!(*rows, 2, "every malformed row is counted before refusing");
                assert!(
                    !first_reason.contains("did:icn:"),
                    "no spelling in the error: {first_reason}"
                );
            }
            other => panic!("expected a malformed-projection refusal, got {other:?}"),
        }
        assert!(!err.to_string().contains(a.as_str()));
        // Deletion interprets the projection too, and refuses without moving a byte.
        let before = raw.scan(b"").unwrap();
        assert!(matches!(
            store.delete_agreement(&AgreementId::new("agr-1")),
            Err(FederationError::AgreementPartyIndexMalformed { .. })
        ));
        assert_eq!(raw.scan(b"").unwrap(), before);
    }

    #[test]
    fn rebuild_makes_the_projection_equal_to_what_canonical_rows_imply() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let c = test_did();
        let ab = two_party_agreement(&a, &b);
        let bc = two_party_agreement(&b, &c);
        store.store_agreement(&ab).unwrap();
        store.store_agreement(&bc).unwrap();

        // Corrupt the projection every way the protocol or a bug could:
        // a missing row, a stale alias row, a dangling row, a row for a party
        // canonical state does not contain, and two malformed rows.
        raw.delete(&AgreementStore::party_index_key(&c, &bc.id))
            .unwrap();
        raw.put(
            &AgreementStore::party_index_key(&respell(&a, multibase::Base::Base16Lower), &ab.id),
            ab.id.as_str().as_bytes(),
        )
        .unwrap();
        raw.put(
            &AgreementStore::party_index_key(&a, &AgreementId::new("agr-gone")),
            b"agr-gone",
        )
        .unwrap();
        raw.put(
            &AgreementStore::party_index_key(&test_did(), &ab.id),
            ab.id.as_str().as_bytes(),
        )
        .unwrap();
        raw.put(b"idx_agreement_party/garbage", b"agr-1").unwrap();
        raw.put(&AgreementStore::party_index_key(&a, &ab.id), b"agr-other")
            .unwrap(); // value disagrees with key
        let canonical_before = raw.scan(AGREEMENT_PREFIX).unwrap();

        let report = store.rebuild_party_index().unwrap();
        assert_eq!(
            report,
            PartyIndexRebuild {
                agreements: 2,
                rows_expected: 4,
                rows_kept: 2,
                rows_added: 2,
                rows_removed_stale: 3,
                rows_removed_malformed: 2,
            }
        );

        let mut expected: Vec<Vec<u8>> = [(&a, &ab), (&b, &ab), (&b, &bc), (&c, &bc)]
            .iter()
            .map(|(did, agreement)| AgreementStore::party_index_key(did, &agreement.id))
            .collect();
        expected.sort();
        let mut actual = index_keys(&raw);
        actual.sort();
        assert_eq!(actual, expected);
        assert_eq!(
            raw.scan(AGREEMENT_PREFIX).unwrap(),
            canonical_before,
            "no canonical byte moved"
        );

        // And a second rebuild is a no-op.
        assert_eq!(
            store.rebuild_party_index().unwrap(),
            PartyIndexRebuild {
                agreements: 2,
                rows_expected: 4,
                rows_kept: 4,
                ..PartyIndexRebuild::default()
            }
        );
    }

    #[test]
    fn rebuild_refuses_over_an_unreadable_canonical_row_and_moves_nothing() {
        let (raw, store) = open_pair();
        store
            .store_agreement(&two_party_agreement(&test_did(), &test_did()))
            .unwrap();
        raw.put(
            &AgreementStore::agreement_key(&AgreementId::new("agr-bad")),
            b"{",
        )
        .unwrap();
        raw.put(b"idx_agreement_party/garbage", b"x").unwrap();
        let before = raw.scan(b"").unwrap();

        assert!(matches!(
            store.rebuild_party_index(),
            Err(FederationError::AgreementStoreUnreadable { .. })
        ));
        assert_eq!(
            raw.scan(b"").unwrap(),
            before,
            "the expected set is unknown, so nothing moves"
        );
    }

    #[test]
    fn a_torn_write_leaves_extra_rows_never_missing_ones() {
        // Simulate the two crash points the protocol admits and show that a
        // read filters the residue while every canonical row keeps its rows.
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();

        // Crash after the canonical delete, before the projection delete.
        raw.delete(&AgreementStore::agreement_key(&agreement.id))
            .unwrap();
        let cold = AgreementStore::new(raw.clone() as Arc<dyn Store>);
        assert!(cold.list_agreements_for_party(&a).unwrap().is_empty());
        assert!(cold.list_agreements_for_party(&b).unwrap().is_empty());
        assert_eq!(
            index_keys(&raw).len(),
            2,
            "residue is tolerated, not read as membership"
        );

        // Crash after the new projection rows, before the canonical replacement:
        // the old canonical row is still the truth and still has its rows.
        let (raw2, store2) = open_pair();
        store2.store_agreement(&agreement).unwrap();
        let c = test_did();
        raw2.put(
            &AgreementStore::party_index_key(&c, &agreement.id),
            agreement.id.as_str().as_bytes(),
        )
        .unwrap();
        let cold2 = AgreementStore::new(raw2.clone() as Arc<dyn Store>);
        assert!(cold2.list_agreements_for_party(&c).unwrap().is_empty());
        assert_eq!(cold2.list_agreements_for_party(&a).unwrap().len(), 1);
        assert_eq!(cold2.list_agreements_for_party(&b).unwrap().len(), 1);
    }

    #[test]
    fn concurrent_replacements_of_one_agreement_never_strand_a_canonical_party() {
        use std::sync::Barrier;
        let (raw, store) = open_pair();
        let store = Arc::new(store);
        let a = test_did();
        let b = test_did();
        let c = test_did();
        let base = two_party_agreement(&a, &b);
        store.store_agreement(&base).unwrap();

        // Two writers race to replace the same agreement with different party
        // sets; whichever canonical row wins must have every row it implies.
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [vec![a.clone(), c.clone()], vec![b.clone(), c.clone()]]
            .into_iter()
            .map(|parties| {
                let store = store.clone();
                let barrier = barrier.clone();
                let mut candidate = base.clone();
                candidate.parties = parties
                    .into_iter()
                    .map(|did| AgreementParty::new(did, "coop", PartyRole::Counterparty))
                    .collect();
                std::thread::spawn(move || {
                    barrier.wait();
                    store.store_agreement(&candidate).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let winner = store.get_agreement(&base.id).unwrap().unwrap();
        let keys = index_keys(&raw);
        for party in &winner.parties {
            assert!(keys.contains(&AgreementStore::party_index_key(&party.did, &base.id)));
        }
        for did in [&a, &b, &c] {
            let listed = !store.list_agreements_for_party(did).unwrap().is_empty();
            let canonical = winner.parties.iter().any(|p| p.did == *did);
            assert_eq!(listed, canonical, "lookup agrees with canonical membership");
        }
    }

    #[test]
    fn reopened_store_answers_the_same_under_any_spelling() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store_path = temp_dir.path().join("agreements");
        let a = test_did();
        let b = test_did();
        let agreement = two_party_agreement(&a, &b);
        let mut expected_keys;
        {
            let sled_store = Arc::new(icn_store::SledStore::open(&store_path).unwrap());
            let store = AgreementStore::new(sled_store.clone() as Arc<dyn Store>);
            let mut replaced = agreement.clone();
            store.store_agreement(&replaced).unwrap();
            replaced.parties.retain(|p| p.did != b);
            store.store_agreement(&replaced).unwrap();
            expected_keys = sled_store.scan_keys(AGREEMENT_PARTY_INDEX).unwrap();
            expected_keys.sort();
            sled_store.flush().unwrap();
            drop(store);
            drop(sled_store);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        let sled_store = retry_open_sled(&store_path, 5);
        let store = AgreementStore::new(sled_store.clone() as Arc<dyn Store>);
        let mut keys = sled_store.scan_keys(AGREEMENT_PARTY_INDEX).unwrap();
        keys.sort();
        assert_eq!(keys, expected_keys);
        assert!(store.list_agreements_for_party(&b).unwrap().is_empty());
        assert!(store
            .list_agreements_for_party(&respell(&b, multibase::Base::Base16Lower))
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .list_agreements_for_party(&respell(&a, multibase::Base::Base16Lower))
                .unwrap()[0]
                .id,
            agreement.id
        );
    }

    #[test]
    fn scanner_registry_names_this_keyspace_as_an_equivalent_projection() {
        use icn_store::did_collision_scan::{n2a_keyspaces, MergeDisposition, RuleBasis};
        let descriptor = n2a_keyspaces()
            .into_iter()
            .find(|d| d.prefix == AGREEMENT_PARTY_INDEX)
            .expect("the scanner registry must cover idx_agreement_party/");
        assert_eq!(descriptor.name, "icn-federation/agreement_party_index");
        assert_eq!(descriptor.disposition, MergeDisposition::Equivalent);
        assert_eq!(descriptor.basis, RuleBasis::Established);
        assert!(descriptor.slash_ends_did, "the spelling is followed by `/`");
        assert!(
            !descriptor.did_ends_key,
            "the agreement id follows the spelling"
        );
    }

    #[test]
    fn scanner_and_store_agree_on_what_a_projection_row_is() {
        use icn_store::did_collision_scan::{n2a_keyspaces, scan_keyspace};
        let (raw, store) = open_pair();
        let a = test_did();
        let agreement = two_party_agreement(&a, &test_did());
        store.store_agreement(&agreement).unwrap();
        raw.put(
            &AgreementStore::party_index_key(
                &respell(&a, multibase::Base::Base16Lower),
                &agreement.id,
            ),
            agreement.id.as_str().as_bytes(),
        )
        .unwrap();

        let descriptor = n2a_keyspaces()
            .into_iter()
            .find(|d| d.prefix == AGREEMENT_PARTY_INDEX)
            .unwrap();
        let report = scan_keyspace(raw.as_ref(), &descriptor).unwrap();
        assert_eq!(report.rows_scanned, 3);
        assert_eq!(
            report.rows_unreadable, 0,
            "every row the store wrote is readable to the scan"
        );
        assert_eq!(
            report.collision_groups.len(),
            1,
            "the alias pair is one group"
        );
        assert!(report.is_automatable());
        // The store reads the same three rows as one principal plus one other.
        assert_eq!(store.list_agreements_for_party(&a).unwrap().len(), 1);
    }

    /// A `Store` that records the order of its mutations, so the write
    /// protocol can be pinned rather than assumed.
    struct RecordingStore {
        inner: Arc<icn_store::SledStore>,
        log: Mutex<Vec<(&'static str, Vec<u8>)>>,
    }

    impl Store for RecordingStore {
        fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            self.inner.get(key)
        }
        fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
            self.log.lock().unwrap().push(("put", key.to_vec()));
            self.inner.put(key, value)
        }
        fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
            self.log.lock().unwrap().push(("delete", key.to_vec()));
            self.inner.delete(key)
        }
        fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan(prefix)
        }
        fn get_replica_metadata(
            &self,
            content_hash: &icn_store::ContentHash,
        ) -> anyhow::Result<Option<icn_store::ReplicaMetadata>> {
            self.inner.get_replica_metadata(content_hash)
        }
        fn put_replica_metadata(
            &self,
            metadata: &icn_store::ReplicaMetadata,
        ) -> anyhow::Result<()> {
            self.inner.put_replica_metadata(metadata)
        }
        fn list_replica_hashes(&self) -> anyhow::Result<Vec<icn_store::ContentHash>> {
            self.inner.list_replica_hashes()
        }
    }

    #[test]
    fn the_write_protocol_keeps_the_projection_a_superset_at_every_step() {
        let recording = Arc::new(RecordingStore {
            inner: Arc::new(icn_store::SledStore::temporary().unwrap()),
            log: Mutex::new(Vec::new()),
        });
        let store = AgreementStore::new(recording.clone() as Arc<dyn Store>);
        let a = test_did();
        let b = test_did();
        let c = test_did();
        let mut agreement = two_party_agreement(&a, &b);
        store.store_agreement(&agreement).unwrap();

        // Replacement: drop `b`, add `c`.
        agreement.parties.retain(|p| p.did != b);
        agreement.parties.push(AgreementParty::new(
            c.clone(),
            "coop-c",
            PartyRole::Guarantor,
        ));
        recording.log.lock().unwrap().clear();
        store.store_agreement(&agreement).unwrap();

        let log = recording.log.lock().unwrap().clone();
        let canonical = AgreementStore::agreement_key(&agreement.id);
        let position = |op: &str, key: &[u8]| {
            log.iter()
                .position(|(o, k)| *o == op && k == key)
                .unwrap_or_else(|| panic!("expected {op} of {}", String::from_utf8_lossy(key)))
        };
        let canonical_put = position("put", &canonical);
        for did in [&a, &c] {
            let row = AgreementStore::party_index_key(did, &agreement.id);
            assert!(
                position("put", &row) < canonical_put,
                "every row the new canonical version implies is written before it"
            );
        }
        let retired = AgreementStore::party_index_key(&b, &agreement.id);
        assert!(
            position("delete", &retired) > canonical_put,
            "a superseded row is retired only after the new canonical row is visible"
        );

        // Deletion: the canonical row goes first, then its projection rows.
        recording.log.lock().unwrap().clear();
        store.delete_agreement(&agreement.id).unwrap();
        let log = recording.log.lock().unwrap().clone();
        let canonical_delete = log
            .iter()
            .position(|(o, k)| *o == "delete" && *k == canonical)
            .unwrap();
        for did in [&a, &c] {
            let row = AgreementStore::party_index_key(did, &agreement.id);
            let row_delete = log
                .iter()
                .position(|(o, k)| *o == "delete" && *k == row)
                .unwrap();
            assert!(row_delete > canonical_delete);
        }
    }

    // ----- canonical key/value identity (#2707 review) ---------------------------

    /// Store `other`'s serialized value under `victim`'s canonical key: one
    /// row's value under another row's key, the shape a collapsed rebuild's
    /// write-back leaves behind, and the shape an adversary with raw store
    /// access would choose to make one agreement wear another's identity.
    fn misfile(raw: &icn_store::SledStore, victim: &AgreementId, other: &Agreement) {
        raw.put(
            &AgreementStore::agreement_key(victim),
            &serde_json::to_vec(other).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn a_canonical_row_whose_value_names_another_agreement_is_attributed_to_neither() {
        let (raw, warm) = open_pair();
        let a = test_did();
        let b = test_did();
        let c = test_did();
        let victim = two_party_agreement(&a, &b);
        let other = two_party_agreement(&c, &test_did());
        warm.store_agreement(&victim).unwrap();
        warm.store_agreement(&other).unwrap();
        misfile(&raw, &victim.id, &other);
        // A cold handle, so the warm cache cannot mask what is on disk.
        let store = AgreementStore::new(raw.clone() as Arc<dyn Store>);

        assert!(
            matches!(
                store.get_agreement(&victim.id),
                Err(FederationError::AgreementStoreKeyValueMismatch { .. })
            ),
            "the row is neither `victim` nor `other`; it is unusable"
        );
        assert!(matches!(
            store.list_agreements(),
            Err(FederationError::AgreementStoreKeyValueMismatch { .. })
        ));
        // A party lookup that must load the row fails closed rather than
        // reporting the party absent from an agreement it cannot read.
        assert!(matches!(
            store.list_agreements_for_party(&a),
            Err(FederationError::AgreementStoreKeyValueMismatch { .. })
        ));
        // A lookup that does not need the row still answers, from its own
        // intact canonical row.
        assert_eq!(store.list_agreements_for_party(&c).unwrap()[0].id, other.id);
        assert_eq!(
            store.get_agreement(&other.id).unwrap().unwrap().id,
            other.id
        );
        // The refusal carries no stored bytes.
        let text = store.get_agreement(&victim.id).unwrap_err().to_string();
        assert!(
            !text.contains(a.as_str()) && !text.contains(c.as_str()),
            "{text}"
        );
    }

    #[test]
    fn a_canonical_key_value_disagreement_refuses_every_mutation_before_a_byte_moves() {
        let (raw, store) = open_pair();
        let a = test_did();
        let b = test_did();
        let c = test_did();
        let mut victim = two_party_agreement(&a, &b);
        let other = two_party_agreement(&c, &test_did());
        store.store_agreement(&victim).unwrap();
        store.store_agreement(&other).unwrap();
        misfile(&raw, &victim.id, &other);
        let snapshot = raw.scan(b"").unwrap();

        // Replacement would otherwise take `other`'s parties as the previous
        // party set and retire `other`'s projection rows.
        victim.parties.retain(|p| p.did != b);
        assert!(matches!(
            store.store_agreement(&victim),
            Err(FederationError::AgreementStoreKeyValueMismatch { .. })
        ));
        assert_eq!(
            raw.scan(b"").unwrap(),
            snapshot,
            "store_agreement moved a byte"
        );

        // Deletion would otherwise destroy the evidence.
        assert!(matches!(
            store.delete_agreement(&victim.id),
            Err(FederationError::AgreementStoreKeyValueMismatch { .. })
        ));
        assert_eq!(
            raw.scan(b"").unwrap(),
            snapshot,
            "delete_agreement moved a byte"
        );

        // Rebuild would otherwise derive the expected set from `other` twice,
        // call `victim`'s real rows stale, and remove them.
        assert!(matches!(
            store.rebuild_party_index(),
            Err(FederationError::AgreementStoreKeyValueMismatch { .. })
        ));
        assert_eq!(
            raw.scan(b"").unwrap(),
            snapshot,
            "rebuild_party_index moved a byte"
        );

        // An agreement that does not touch the row can still be written.
        let fresh = two_party_agreement(&test_did(), &test_did());
        store.store_agreement(&fresh).unwrap();
        assert_eq!(
            store.get_agreement(&fresh.id).unwrap().unwrap().id,
            fresh.id
        );
    }

    #[test]
    fn a_tampered_canonical_key_is_echoed_escaped_and_bounded() {
        let (raw, store) = open_pair();
        let other = two_party_agreement(&test_did(), &test_did());
        store.store_agreement(&other).unwrap();

        // A key an adversary with raw access chose: control characters, a
        // fake log line, and far more bytes than any agreement id has.
        let mut key = AGREEMENT_PREFIX.to_vec();
        key.extend(b"agr-evil\n[ERROR] forged line\r\x1b[31m");
        key.extend(std::iter::repeat_n(b'x', 500));
        raw.put(&key, &serde_json::to_vec(&other).unwrap()).unwrap();

        let text = store.list_agreements().unwrap_err().to_string();
        assert!(matches!(
            store.list_agreements(),
            Err(FederationError::AgreementStoreKeyValueMismatch { .. })
        ));
        assert!(
            !text.chars().any(|c| c.is_control()),
            "control characters must not reach a loggable error: {text:?}"
        );
        assert!(
            text.contains("agr-evil\\n[ERROR]"),
            "escaped, not dropped: {text}"
        );
        assert!(
            text.chars().count() < 400,
            "bounded: {} chars",
            text.chars().count()
        );
    }
}
