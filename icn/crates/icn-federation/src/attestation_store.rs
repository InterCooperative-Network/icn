//! Attestation Store (Phase F2)
//!
//! Persistent storage for federated trust attestations.
//!
//! # Principal identity versus persisted spelling (N2-A, #2703)
//!
//! Every row lives under `federation/attestations/<member-did spelling>/<source_coop_id>`.
//! The key carries the DID exactly as it was spelled when the row was written.
//! Since I7 (#2686) `Did` equality and hashing name the decoded *principal*, so
//! one principal may own rows under several spellings while the domain meaning
//! of `member_did` — the member a source cooperative is vouching for — is the
//! principal. A spelling-prefix read cannot discover another valid spelling of
//! the same principal, so this store never answers a principal question from
//! one prefix. Every operation reads the whole namespace, validates every row,
//! and then interprets it by principal equality. The rules are:
//!
//! * **Unreadable state blocks everything.** A row whose value does not
//!   deserialize, or whose key disagrees with the `(member_did, source_coop_id)`
//!   its value carries, cannot be attributed to any principal, so no read or
//!   write proceeds over it. An unreadable attestation is evidence, not an
//!   absent one.
//! * **Rows from different source cooperatives about one principal are the
//!   existing union.** That is what a federation of attestations means and it
//!   is not a collision.
//! * **Two rows for one `(principal, source_coop_id)` are a collision.** They
//!   can only differ by disagreeing — on score, context, expiry, evidence or
//!   signature — and no federation-domain rule authorizes choosing or combining
//!   them. An operation refuses exactly when the rows it interprets or mutates
//!   include such a pair: a lookup for that principal, a listing of that
//!   source, any write to that pair, and the expiry sweep (which would
//!   otherwise elect a survivor by expiry and destroy the evidence an operator
//!   needs). A lookup for an unrelated principal is unaffected, because its
//!   answer does not depend on the pair.
//! * **Revocation removes every row it names, atomically.**
//!   [`AttestationStore::remove_attestation`] deletes the stored keys it read
//!   whose `(principal, source_coop_id)` matches, so a revocation under one
//!   spelling cannot leave the attestation live under another. The deletes go
//!   as one unit, because a revocation interrupted between two alias rows
//!   would leave a lone row that is no longer ambiguous and therefore
//!   *acceptable* — revocation would have made its own evidence valid. It
//!   never elects a survivor, by failure or otherwise.
//! * **Nothing is re-keyed, merged or normalized.** Persisted bytes change only
//!   through the explicit `put`/`delete` the caller asked for, under keys that
//!   were read from the store or built for the caller's own value.
//! * **Errors carry no payload.** A refusal names the keyspace, a truncated
//!   principal fingerprint, a bounded source identifier and counts — never a
//!   spelling of the attested principal and never a stored attestation. The
//!   source identifier is echoed on purpose; see [`bounded_source_id`] for
//!   what that does and does not expose.
//!
//! The N2-A collision scanner registers this namespace under the same prefix
//! (`icn_store::did_collision_scan::n2a_keyspaces`, `icn-federation/attestations`)
//! with the same collision unit, so the offline scan and this store agree on
//! what a collision is. A test below pins that agreement.

use crate::attestation::FederatedTrustAttestation;
use crate::error::{FederationError, Result};
use crate::metrics;
use icn_identity::Did;
use icn_store::Store;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tracing::{debug, info};

/// Storage key prefix.
///
/// The N2-A collision scanner scans exactly these bytes; keep the two in step
/// (see `scanner_registry_names_this_keyspace` below).
pub const ATTESTATION_PREFIX: &[u8] = b"federation/attestations/";

/// Longest source-cooperative identifier echoed into an error, in characters.
const SOURCE_ID_ERROR_CAP: usize = 64;

/// The byte between the member spelling and the source-cooperative identifier.
///
/// Named because the N2-A descriptor declares the same byte as the terminator
/// of its anchored principal segment, and a key builder that disagreed with
/// the descriptor would be a scan that reads a different keyspace than the one
/// this store writes (`scanner_registry_names_this_keyspace` pins it).
const SOURCE_SEPARATOR: u8 = b'/';

/// Serializes every mutation of the namespace within this process.
///
/// A write is a check-then-put: it reads the rows for `(principal, source)` and
/// refuses if a second spelling would result. Two concurrent writers that each
/// saw the other's row absent would otherwise both succeed and persist exactly
/// the collision the check exists to prevent. The lock is process-wide rather
/// than per instance because callers construct a store per request over one
/// shared backend (`icn-rpc`), and a sled database admits one process at a
/// time, so a process-wide lock covers every writer there is.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn write_guard() -> MutexGuard<'static, ()> {
    // A writer that panicked while holding the lock has not left the store in
    // a state this lock protects against; later writers re-read before acting.
    WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One persisted row, validated: its key is exactly the key its value implies.
struct StoredAttestation {
    key: Vec<u8>,
    attestation: FederatedTrustAttestation,
}

/// Eight hex characters of the principal's identifier bytes.
///
/// The same rule the N2-A scanner uses, so an operator can correlate a refusal
/// here with a scan report. Enough to tell principals apart, not enough to
/// reconstruct one.
fn fingerprint(did: &Did) -> String {
    match did.identifier_bytes() {
        Ok(bytes) => hex::encode(&bytes[..4]),
        Err(_) => "unreadable".to_string(),
    }
}

/// A source-cooperative identifier bounded for an error message.
///
/// Deliberately echoed rather than fingerprinted, unlike the member principal.
/// A refusal has to name which `(principal, source)` pair stopped the
/// operation, and the source is the half an operator can act on: it is
/// federation key structure a cooperative chose and publishes in order to be
/// attributed at all, not a subject's identity. The attested principal — the
/// half a report must not reconstruct — is always a truncated fingerprint.
///
/// One consequence is worth stating rather than discovering: nothing in the
/// federation domain forbids a `source_coop_id` that *is* a `did:icn:`
/// spelling, and this store must not invent a grammar that does (#2704). Such
/// an id therefore appears in a refusal in full, within the cap. That is the
/// source cooperative's own published identifier, never the attested member's.
/// Redacting it would be a federation-domain diagnostics decision, and it
/// would cost the refusal the one field that makes it actionable.
fn bounded_source_id(source_coop_id: &str) -> String {
    if source_coop_id.chars().count() <= SOURCE_ID_ERROR_CAP {
        return source_coop_id.to_string();
    }
    let mut out: String = source_coop_id.chars().take(SOURCE_ID_ERROR_CAP).collect();
    out.push('…');
    out
}

/// Why a value failed to deserialize, without the value.
///
/// `serde_json`'s `Display` can echo input — the `Did` deserializer, for one,
/// reports the spelling it rejected — so only the error class and position
/// are carried.
fn unreadable_reason(err: &serde_json::Error) -> String {
    format!(
        "{:?} error at line {} column {}",
        err.classify(),
        err.line(),
        err.column()
    )
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
        key.push(SOURCE_SEPARATOR);
        key.extend(source_coop_id.as_bytes());
        key
    }

    /// Read and validate every persisted attestation row.
    ///
    /// The namespace is read as a whole: persisted rows are keyed by spelling,
    /// while the domain meaning of `member_did` is a principal, and a
    /// spelling-prefix read cannot discover another valid spelling of the same
    /// principal. Every row must deserialize and must live under exactly the
    /// key its own `(member_did, source_coop_id)` implies; a row that fails
    /// either check cannot be attributed to a principal and stops the read.
    fn load_checked_rows(&self) -> Result<Vec<StoredAttestation>> {
        let entries = self.store.scan(ATTESTATION_PREFIX)?;
        let mut rows = Vec::with_capacity(entries.len());

        for (key, value) in entries {
            let attestation =
                serde_json::from_slice::<FederatedTrustAttestation>(&value).map_err(|err| {
                    FederationError::AttestationStoreUnreadable {
                        key_len: key.len(),
                        value_len: value.len(),
                        reason: unreadable_reason(&err),
                    }
                })?;

            let expected =
                Self::attestation_key(&attestation.member_did, &attestation.source_coop_id);
            if key != expected {
                return Err(FederationError::AttestationStoreKeyValueMismatch {
                    principal_fingerprint: fingerprint(&attestation.member_did),
                    source_coop_id: bounded_source_id(&attestation.source_coop_id),
                    key_len: key.len(),
                });
            }

            rows.push(StoredAttestation { key, attestation });
        }

        Ok(rows)
    }

    /// Refuse if `rows` hold two persisted claims from one source cooperative
    /// about one principal.
    ///
    /// `rows` is the set the calling operation is about to interpret or mutate,
    /// not necessarily the whole namespace. Attestations from *different*
    /// sources about one principal are the federation union and pass. Two rows
    /// for the same `(principal, source_coop_id)` may disagree on score,
    /// context, evidence, expiry, or signature; no federation-domain rule
    /// authorizes choosing or combining them, so the operation fails closed
    /// instead of electing a survivor by spelling or scan order.
    fn ensure_unambiguous(rows: &[StoredAttestation]) -> Result<()> {
        // Grouped by `Did` equality — the I7 rule itself, not a reimplementation.
        let mut groups: HashMap<(&Did, &str), usize> = HashMap::new();
        for row in rows {
            *groups
                .entry((
                    &row.attestation.member_did,
                    row.attestation.source_coop_id.as_str(),
                ))
                .or_insert(0) += 1;
        }

        let mut colliding: Vec<(String, String, usize)> = groups
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|((did, source), count)| (fingerprint(did), bounded_source_id(source), count))
            .collect();
        if colliding.is_empty() {
            return Ok(());
        }

        // The pair named in the error is the least by (fingerprint, source),
        // never whichever one a hash map happened to yield first: the same
        // store must produce the same refusal every time.
        colliding.sort();
        let colliding_pairs = colliding.len();
        let (principal_fingerprint, source_coop_id, row_count) = colliding.swap_remove(0);
        Err(FederationError::AttestationStorePrincipalCollision {
            principal_fingerprint,
            source_coop_id,
            row_count,
            colliding_pairs,
        })
    }

    /// Store an attestation.
    ///
    /// Replacing the exact row already present for this `(principal, source)`
    /// is ordinary. Writing a second spelling for a pair that is already
    /// persisted would create an ambiguity no domain merge rule authorizes, so
    /// the write is refused before it happens; a pair that is already ambiguous
    /// cannot be replaced either, because there is no single row to replace.
    pub fn store_attestation(&self, att: FederatedTrustAttestation) -> Result<()> {
        let _guard = write_guard();

        let rows = self.load_checked_rows()?;
        let pair: Vec<StoredAttestation> = rows
            .into_iter()
            .filter(|row| {
                row.attestation.member_did == att.member_did
                    && row.attestation.source_coop_id == att.source_coop_id
            })
            .collect();
        Self::ensure_unambiguous(&pair)?;

        let key = Self::attestation_key(&att.member_did, &att.source_coop_id);
        if let Some(existing) = pair.first() {
            if existing.key != key {
                return Err(FederationError::AttestationStoreAliasWriteRefused {
                    principal_fingerprint: fingerprint(&att.member_did),
                    source_coop_id: bounded_source_id(&att.source_coop_id),
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

    /// Get all attestations for a member principal, under any spelling.
    ///
    /// The result is the same whichever spelling of the principal is asked
    /// for, and does not depend on what was asked before.
    pub fn get_attestations_for(&self, member: &Did) -> Result<Vec<FederatedTrustAttestation>> {
        let rows = self.load_checked_rows()?;
        let mine: Vec<StoredAttestation> = rows
            .into_iter()
            .filter(|row| row.attestation.member_did == *member)
            .collect();
        Self::ensure_unambiguous(&mine)?;

        Ok(mine.into_iter().map(|row| row.attestation).collect())
    }

    /// Get attestations from a specific cooperative
    pub fn get_attestations_from(&self, coop_id: &str) -> Result<Vec<FederatedTrustAttestation>> {
        let rows = self.load_checked_rows()?;
        let theirs: Vec<StoredAttestation> = rows
            .into_iter()
            .filter(|row| row.attestation.source_coop_id == coop_id)
            .collect();
        Self::ensure_unambiguous(&theirs)?;

        Ok(theirs.into_iter().map(|row| row.attestation).collect())
    }

    /// Remove expired attestations.
    ///
    /// The sweep interprets the whole namespace, so the whole namespace must be
    /// unambiguous first: deleting the expired half of a colliding pair would
    /// elect the other half as survivor and destroy the evidence an operator
    /// needs to disposition the pair. A refused sweep deletes nothing.
    ///
    /// The deletes are therefore *not* one atomic unit, and need not be: past
    /// that refusal no two rows share a `(principal, source_coop_id)`, so
    /// there is no alias survivor for an interrupted sweep to elect. A sweep
    /// that stops partway has removed some expired rows and left others, which
    /// is the state a sweep that had not run yet was already in. That is the
    /// difference from [`AttestationStore::remove_attestation`], which acts on
    /// a pair that may be ambiguous by construction.
    pub fn remove_expired(&self) -> Result<usize> {
        let _guard = write_guard();

        let rows = self.load_checked_rows()?;
        Self::ensure_unambiguous(&rows)?;
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
    /// Removal is principal-wide for this source. Deleting only the caller's
    /// spelling would make revocation representation-dependent: the attestation
    /// would stay live under any other spelling already persisted. This path
    /// reads and validates every row, then deletes the exact keys it read whose
    /// `(principal, source_coop_id)` matches. If that pair happens to be
    /// ambiguous, every row of it goes — revocation is the one operation for
    /// which removing more can never be the unsafe direction. It does not
    /// merge, re-key, or choose one value to keep.
    ///
    /// The deletion is **one atomic unit**, not one per row. Deleting alias
    /// rows separately would put a failure boundary between them, and the
    /// state on the far side of that boundary is the dangerous one: a lone
    /// surviving row is no longer ambiguous, so the next read accepts the
    /// exact attestation the revocation was called to remove. Revoking must
    /// not be able to elect a survivor, so [`icn_store::Store::delete_atomic`]
    /// carries the whole pair or none of it, and a failure leaves the pair as
    /// it was — still ambiguous, still refused, and still there as the
    /// evidence an operator needs.
    ///
    /// Retry is the remedy for a failure and is safe: a revocation that did
    /// not happen leaves nothing to undo, and one that did leaves nothing to
    /// match, so a repeat is a no-op rather than an error.
    pub fn remove_attestation(&self, member: &Did, source_coop_id: &str) -> Result<()> {
        let _guard = write_guard();

        let doomed: Vec<Vec<u8>> = self
            .load_checked_rows()?
            .into_iter()
            .filter(|row| {
                row.attestation.member_did == *member
                    && row.attestation.source_coop_id == source_coop_id
            })
            .map(|row| row.key)
            .collect();

        if doomed.is_empty() {
            return Ok(());
        }
        self.store.delete_atomic(&doomed)?;

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
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    /// A second valid spelling of the same principal: base16 instead of the
    /// base58 `KeyPair` produces. Distinct string, equal `Did`.
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

    fn expired_attestation(source: &str, member_did: Did) -> FederatedTrustAttestation {
        let mut att = attestation(source, member_did, 0.5);
        att.issued_at = 1;
        att.expires_at = 2;
        assert!(att.is_expired());
        att
    }

    /// Write a row exactly as the production key builder would, bypassing the
    /// store's own write guard — the shape of a row that predates the guard.
    fn put_raw(raw: &SledStore, member: &Did, att: &FederatedTrustAttestation) {
        raw.put(
            &AttestationStore::attestation_key(member, &att.source_coop_id),
            &serde_json::to_vec(att).unwrap(),
        )
        .unwrap();
    }

    fn open_pair() -> (Arc<SledStore>, AttestationStore) {
        let raw = Arc::new(SledStore::temporary().unwrap());
        let att_store = AttestationStore::new(raw.clone() as Arc<dyn Store>);
        (raw, att_store)
    }

    fn sources_of(atts: &[FederatedTrustAttestation]) -> Vec<String> {
        let mut out: Vec<String> = atts.iter().map(|a| a.source_coop_id.clone()).collect();
        out.sort();
        out
    }

    fn snapshot(raw: &SledStore) -> Vec<(Vec<u8>, Vec<u8>)> {
        raw.scan(ATTESTATION_PREFIX).unwrap()
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

    #[test]
    fn two_distinct_principals_are_not_conflated() {
        let (_, att_store) = open_pair();

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
        assert_eq!(
            att_store.get_attestations_from("food-coop").unwrap().len(),
            2
        );
    }

    #[test]
    fn lookup_is_principal_correct_in_both_alias_query_orders() {
        // The #2703 regression case: rows under two spellings of one principal,
        // from two sources. Whichever spelling is asked first, and whichever is
        // asked second, the answer is the union for the principal.
        let (raw, att_store) = open_pair();

        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &member,
            &attestation("food-coop", member.clone(), 0.85),
        );
        put_raw(
            &raw,
            &alias,
            &attestation("housing-coop", alias.clone(), 0.75),
        );
        assert_eq!(snapshot(&raw).len(), 2, "two distinct persisted keys");

        let expected = vec!["food-coop".to_string(), "housing-coop".to_string()];

        // Order A: canonical first, then alias.
        assert_eq!(
            sources_of(&att_store.get_attestations_for(&member).unwrap()),
            expected
        );
        assert_eq!(
            sources_of(&att_store.get_attestations_for(&alias).unwrap()),
            expected
        );

        // Order B on a fresh handle over the same rows: alias first.
        let fresh = AttestationStore::new(raw.clone() as Arc<dyn Store>);
        assert_eq!(
            sources_of(&fresh.get_attestations_for(&alias).unwrap()),
            expected
        );
        assert_eq!(
            sources_of(&fresh.get_attestations_for(&member).unwrap()),
            expected
        );
        assert_eq!(
            sources_of(&fresh.get_valid_attestations_for(&alias).unwrap()),
            expected
        );
    }

    #[test]
    fn reversed_insertion_order_gives_the_same_answer() {
        let member = test_did();
        let alias = alias_spelling(&member);

        let answer = |first: &Did, first_src: &str, second: &Did, second_src: &str| {
            let (raw, att_store) = open_pair();
            put_raw(&raw, first, &attestation(first_src, first.clone(), 0.85));
            put_raw(&raw, second, &attestation(second_src, second.clone(), 0.75));
            (
                sources_of(&att_store.get_attestations_for(&member).unwrap()),
                sources_of(&att_store.get_attestations_for(&alias).unwrap()),
            )
        };

        let forward = answer(&member, "food-coop", &alias, "housing-coop");
        let reversed = answer(&alias, "housing-coop", &member, "food-coop");
        assert_eq!(forward, reversed);
        assert_eq!(forward.0, forward.1);
        assert_eq!(forward.0, vec!["food-coop", "housing-coop"]);
    }

    #[test]
    fn a_reopened_store_answers_the_same() {
        // No cache survives a restart, so a restart must change nothing. The
        // rows are written under the alias by one process lifetime and read
        // under the canonical spelling by the next.
        let dir = tempfile::tempdir().unwrap();
        let member = test_did();
        let alias = alias_spelling(&member);

        {
            let raw = Arc::new(SledStore::open(dir.path()).unwrap());
            let att_store = AttestationStore::new(raw.clone() as Arc<dyn Store>);
            att_store
                .store_attestation(attestation("food-coop", alias.clone(), 0.85))
                .unwrap();
            assert_eq!(att_store.get_attestations_for(&member).unwrap().len(), 1);
        }

        let raw = Arc::new(SledStore::open(dir.path()).unwrap());
        let att_store = AttestationStore::new(raw.clone() as Arc<dyn Store>);
        assert_eq!(
            sources_of(&att_store.get_attestations_for(&member).unwrap()),
            vec!["food-coop"]
        );
        assert_eq!(
            sources_of(&att_store.get_attestations_for(&alias).unwrap()),
            vec!["food-coop"]
        );
    }

    #[test]
    fn same_principal_same_source_alias_rows_fail_closed() {
        let (raw, att_store) = open_pair();

        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &member,
            &attestation("food-coop", member.clone(), 0.85),
        );
        put_raw(&raw, &alias, &attestation("food-coop", alias.clone(), 0.25));
        let before = snapshot(&raw);
        assert_eq!(before.len(), 2);

        for spelling in [&member, &alias] {
            let err = att_store.get_attestations_for(spelling).unwrap_err();
            match err {
                FederationError::AttestationStorePrincipalCollision {
                    ref principal_fingerprint,
                    ref source_coop_id,
                    row_count,
                    colliding_pairs,
                } => {
                    assert_eq!(principal_fingerprint, &fingerprint(&member));
                    assert_eq!(source_coop_id, "food-coop");
                    assert_eq!(row_count, 2);
                    assert_eq!(colliding_pairs, 1);
                }
                other => panic!("expected a principal collision, got {other:?}"),
            }
            assert!(matches!(
                att_store.get_valid_attestations_for(spelling),
                Err(FederationError::AttestationStorePrincipalCollision { .. })
            ));
        }

        // The listing for that source is ambiguous too.
        assert!(matches!(
            att_store.get_attestations_from("food-coop"),
            Err(FederationError::AttestationStorePrincipalCollision { .. })
        ));

        // A refused read mutates nothing.
        assert_eq!(snapshot(&raw), before);
    }

    #[test]
    fn a_collision_elsewhere_does_not_block_an_unrelated_principal() {
        // Refusal is scoped to what an operation interprets. Q's ambiguity is
        // not evidence about P, and P's rows are exactly as readable as before.
        let (raw, att_store) = open_pair();

        let p = test_did();
        let q = test_did();
        let q_alias = alias_spelling(&q);
        put_raw(&raw, &p, &attestation("food-coop", p.clone(), 0.9));
        put_raw(&raw, &q, &attestation("food-coop", q.clone(), 0.8));
        put_raw(
            &raw,
            &q_alias,
            &attestation("food-coop", q_alias.clone(), 0.2),
        );

        assert_eq!(att_store.get_attestations_for(&p).unwrap().len(), 1);
        assert!(matches!(
            att_store.get_attestations_for(&q),
            Err(FederationError::AttestationStorePrincipalCollision { .. })
        ));
        // The source listing includes Q's pair, so it is ambiguous.
        assert!(matches!(
            att_store.get_attestations_from("food-coop"),
            Err(FederationError::AttestationStorePrincipalCollision { .. })
        ));
        // A different source is unaffected.
        assert!(att_store
            .get_attestations_from("housing-coop")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_reported_collision_is_deterministic() {
        // Two colliding pairs. Whichever order the rows were written, and
        // whatever order a hash map yields them, the refusal names the same
        // pair and the same counts.
        let a = test_did();
        let b = test_did();
        let (lo, hi) = if fingerprint(&a) <= fingerprint(&b) {
            (a, b)
        } else {
            (b, a)
        };

        let refusal = |order: &[&Did]| {
            let (raw, att_store) = open_pair();
            for did in order {
                put_raw(&raw, did, &attestation("food-coop", (*did).clone(), 0.5));
                let alias = alias_spelling(did);
                put_raw(&raw, &alias, &attestation("food-coop", alias.clone(), 0.5));
            }
            att_store.remove_expired().unwrap_err().to_string()
        };

        let first = refusal(&[&lo, &hi]);
        let second = refusal(&[&hi, &lo]);
        assert_eq!(first, second);
        assert!(first.contains(&fingerprint(&lo)), "{first}");
        assert!(first.contains("2 colliding pair"), "{first}");
    }

    #[test]
    fn refusals_carry_no_full_identifier() {
        let (raw, att_store) = open_pair();

        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &member,
            &attestation("food-coop", member.clone(), 0.85),
        );
        put_raw(&raw, &alias, &attestation("food-coop", alias.clone(), 0.25));

        let message = att_store
            .get_attestations_for(&member)
            .unwrap_err()
            .to_string();
        assert!(!message.contains(member.as_str()), "{message}");
        assert!(!message.contains(alias.as_str()), "{message}");
        assert!(message.contains(&fingerprint(&member)), "{message}");
        assert!(message.contains("federation/attestations"), "{message}");
    }

    #[test]
    fn the_second_spelling_of_a_persisted_pair_cannot_be_written() {
        let (raw, att_store) = open_pair();

        let member = test_did();
        let alias = alias_spelling(&member);
        att_store
            .store_attestation(attestation("food-coop", member.clone(), 0.85))
            .unwrap();
        let before = snapshot(&raw);

        let err = att_store
            .store_attestation(attestation("food-coop", alias.clone(), 0.25))
            .unwrap_err();
        assert!(
            matches!(
                err,
                FederationError::AttestationStoreAliasWriteRefused { .. }
            ),
            "{err:?}"
        );
        assert_eq!(snapshot(&raw), before, "a refused write changes no byte");

        // The same pair under the same spelling is an ordinary replacement,
        // and a different source under the alias is the ordinary union.
        att_store
            .store_attestation(attestation("food-coop", member.clone(), 0.9))
            .unwrap();
        att_store
            .store_attestation(attestation("housing-coop", alias.clone(), 0.7))
            .unwrap();
        let rows = att_store.get_attestations_for(&alias).unwrap();
        assert_eq!(sources_of(&rows), vec!["food-coop", "housing-coop"]);
        assert_eq!(
            rows.iter()
                .find(|a| a.source_coop_id == "food-coop")
                .unwrap()
                .trust_score,
            0.9
        );
    }

    #[test]
    fn an_already_ambiguous_pair_cannot_be_replaced() {
        let (raw, att_store) = open_pair();

        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &member,
            &attestation("food-coop", member.clone(), 0.85),
        );
        put_raw(&raw, &alias, &attestation("food-coop", alias.clone(), 0.25));
        let before = snapshot(&raw);

        let err = att_store
            .store_attestation(attestation("food-coop", member.clone(), 0.5))
            .unwrap_err();
        assert!(
            matches!(
                err,
                FederationError::AttestationStorePrincipalCollision { .. }
            ),
            "{err:?}"
        );
        assert_eq!(snapshot(&raw), before);
    }

    #[test]
    fn a_collision_elsewhere_does_not_block_a_write_to_an_unrelated_pair() {
        let (raw, att_store) = open_pair();

        let q = test_did();
        let q_alias = alias_spelling(&q);
        put_raw(&raw, &q, &attestation("food-coop", q.clone(), 0.8));
        put_raw(
            &raw,
            &q_alias,
            &attestation("food-coop", q_alias.clone(), 0.2),
        );

        let p = test_did();
        att_store
            .store_attestation(attestation("food-coop", p.clone(), 0.9))
            .unwrap();
        assert_eq!(att_store.get_attestations_for(&p).unwrap().len(), 1);
    }

    #[test]
    fn malformed_persisted_value_is_surfaced_without_its_contents() {
        let (raw, att_store) = open_pair();
        let member = test_did();
        att_store
            .store_attestation(attestation("food-coop", member.clone(), 0.85))
            .unwrap();

        // A value that is not JSON at all.
        raw.put(b"federation/attestations/malformed", b"{not-json")
            .unwrap();
        let err = att_store.get_attestations_for(&member).unwrap_err();
        assert!(
            matches!(err, FederationError::AttestationStoreUnreadable { .. }),
            "{err:?}"
        );
        raw.delete(b"federation/attestations/malformed").unwrap();

        // A value that is JSON but whose member DID is junk. The `Did`
        // deserializer echoes what it rejected; the refusal must not.
        let junk = "did:example:NOT-AN-ICN-PRINCIPAL-9f8e7d";
        let mut doc = serde_json::to_value(attestation("food-coop", member.clone(), 0.5)).unwrap();
        doc["member_did"] = serde_json::Value::String(junk.to_string());
        raw.put(
            b"federation/attestations/junk/food-coop",
            &serde_json::to_vec(&doc).unwrap(),
        )
        .unwrap();
        let before = snapshot(&raw);

        for result in [
            att_store.get_attestations_for(&member),
            att_store.get_attestations_from("food-coop"),
        ] {
            let err = result.unwrap_err();
            assert!(
                matches!(err, FederationError::AttestationStoreUnreadable { .. }),
                "{err:?}"
            );
            let message = err.to_string();
            assert!(!message.contains(junk), "{message}");
            assert!(!message.contains("NOT-AN-ICN"), "{message}");
        }
        assert!(matches!(
            att_store.remove_expired(),
            Err(FederationError::AttestationStoreUnreadable { .. })
        ));
        assert!(matches!(
            att_store.store_attestation(attestation("housing-coop", member.clone(), 0.5)),
            Err(FederationError::AttestationStoreUnreadable { .. })
        ));
        assert_eq!(snapshot(&raw), before, "nothing was dropped or rewritten");
    }

    #[test]
    fn a_key_that_disagrees_with_its_value_is_surfaced() {
        let (raw, att_store) = open_pair();
        let member = test_did();
        let att = attestation("food-coop", member.clone(), 0.85);

        // Valid document, stored under a key naming a different source.
        raw.put(
            &AttestationStore::attestation_key(&member, "housing-coop"),
            &serde_json::to_vec(&att).unwrap(),
        )
        .unwrap();
        let before = snapshot(&raw);

        let err = att_store.get_attestations_for(&member).unwrap_err();
        match err {
            FederationError::AttestationStoreKeyValueMismatch {
                ref principal_fingerprint,
                ref source_coop_id,
                key_len,
            } => {
                assert_eq!(principal_fingerprint, &fingerprint(&member));
                assert_eq!(source_coop_id, "food-coop");
                assert_eq!(key_len, before[0].0.len());
            }
            other => panic!("expected a key/value mismatch, got {other:?}"),
        }
        let message = att_store
            .get_attestations_from("food-coop")
            .unwrap_err()
            .to_string();
        assert!(!message.contains(member.as_str()), "{message}");

        // Valid document under a key with no spelling at all.
        raw.delete(&before[0].0).unwrap();
        raw.put(
            b"federation/attestations/no-did-here",
            &serde_json::to_vec(&att).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            att_store.get_attestations_for(&member),
            Err(FederationError::AttestationStoreKeyValueMismatch { .. })
        ));
    }

    #[test]
    fn removal_through_the_alternate_spelling_revokes_every_alias_row() {
        let (raw, att_store) = open_pair();

        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &member,
            &attestation("food-coop", member.clone(), 0.85),
        );
        put_raw(&raw, &alias, &attestation("food-coop", alias.clone(), 0.25));
        // A different source for the same principal must survive the revocation.
        put_raw(
            &raw,
            &alias,
            &attestation("housing-coop", alias.clone(), 0.7),
        );
        assert_eq!(snapshot(&raw).len(), 3);

        // Revoke naming the spelling that is *not* the one the caller wrote.
        att_store.remove_attestation(&alias, "food-coop").unwrap();

        let remaining = snapshot(&raw);
        assert_eq!(remaining.len(), 1, "both food-coop rows are gone");
        assert!(
            remaining[0].0.ends_with(b"/housing-coop"),
            "the other source's row is untouched"
        );
        // And the attestation does not reappear under either spelling.
        for spelling in [&member, &alias] {
            let rows = att_store.get_attestations_for(spelling).unwrap();
            assert_eq!(sources_of(&rows), vec!["housing-coop"]);
        }
    }

    #[test]
    fn removal_names_exactly_one_source() {
        let (raw, att_store) = open_pair();
        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &member,
            &attestation("food-coop", member.clone(), 0.85),
        );
        put_raw(
            &raw,
            &alias,
            &attestation("housing-coop", alias.clone(), 0.7),
        );

        att_store
            .remove_attestation(&member, "housing-coop")
            .unwrap();

        let rows = att_store.get_attestations_for(&alias).unwrap();
        assert_eq!(sources_of(&rows), vec!["food-coop"]);
    }

    #[test]
    fn the_expiry_sweep_refuses_over_an_ambiguous_pair_and_deletes_nothing() {
        // Deleting the expired half of a colliding pair would elect the other
        // half. The sweep must stop before its first delete — including an
        // expired row that has nothing to do with the collision.
        let (raw, att_store) = open_pair();

        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &member,
            &expired_attestation("food-coop", member.clone()),
        );
        put_raw(&raw, &alias, &attestation("food-coop", alias.clone(), 0.9));
        let other = test_did();
        put_raw(
            &raw,
            &other,
            &expired_attestation("food-coop", other.clone()),
        );
        let before = snapshot(&raw);
        assert_eq!(before.len(), 3);

        assert!(matches!(
            att_store.remove_expired(),
            Err(FederationError::AttestationStorePrincipalCollision { .. })
        ));
        assert_eq!(snapshot(&raw), before, "a refused sweep deletes nothing");

        // Control: once the pair is revoked, the sweep proceeds normally.
        att_store.remove_attestation(&member, "food-coop").unwrap();
        assert_eq!(att_store.remove_expired().unwrap(), 1);
        assert!(snapshot(&raw).is_empty());
    }

    #[test]
    fn the_expiry_sweep_removes_expired_rows_under_any_spelling() {
        let (raw, att_store) = open_pair();
        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &alias,
            &expired_attestation("food-coop", alias.clone()),
        );
        put_raw(
            &raw,
            &member,
            &attestation("housing-coop", member.clone(), 0.9),
        );

        assert_eq!(att_store.remove_expired().unwrap(), 1);
        let rows = att_store.get_attestations_for(&member).unwrap();
        assert_eq!(sources_of(&rows), vec!["housing-coop"]);
    }

    #[test]
    fn scanner_registry_names_this_keyspace() {
        // The N2-A collision scanner must scan exactly the bytes this store
        // writes, with the layout this store uses: the member spelling is
        // anchored right after the prefix and ends at the `/`, the source is
        // an opaque discriminator this store compares as exact bytes and the
        // scan must not parse, and a collision is one `(principal, source)`.
        use icn_store::did_collision_scan::{n2a_keyspaces, MergeDisposition, PrincipalRegion};

        let descriptor = n2a_keyspaces()
            .into_iter()
            .find(|d| d.prefix == ATTESTATION_PREFIX)
            .expect("federation/attestations/ must be a registered N2-A keyspace (#2703)");
        assert_eq!(descriptor.name, "icn-federation/attestations");
        assert_eq!(
            descriptor.principal_region,
            PrincipalRegion::AnchoredThenOpaque {
                terminator: SOURCE_SEPARATOR
            },
            "the scan must read the member segment and nothing else"
        );
        assert_eq!(descriptor.disposition, MergeDisposition::FailClosed);
    }

    #[test]
    fn scanner_and_store_agree_on_what_a_collision_is() {
        // The same rows, written by the production key builder: the store
        // refuses exactly the pair the scanner reports, and passes exactly the
        // pair the scanner does not.
        use icn_store::did_collision_scan::{n2a_keyspaces, scan_keyspace};

        let descriptor = n2a_keyspaces()
            .into_iter()
            .find(|d| d.prefix == ATTESTATION_PREFIX)
            .unwrap();

        // Different sources: union for the store, no group for the scanner.
        let (raw, att_store) = open_pair();
        let member = test_did();
        let alias = alias_spelling(&member);
        put_raw(
            &raw,
            &member,
            &attestation("food-coop", member.clone(), 0.85),
        );
        put_raw(
            &raw,
            &alias,
            &attestation("housing-coop", alias.clone(), 0.7),
        );
        let report = scan_keyspace(&*raw as &dyn Store, &descriptor).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(report.rows_unreadable, 0);
        assert!(report.collision_groups.is_empty());
        assert!(!report.must_fail_closed());
        assert_eq!(att_store.get_attestations_for(&member).unwrap().len(), 2);

        // Same source: refusal for the store, one blocking group for the scanner.
        put_raw(&raw, &alias, &attestation("food-coop", alias.clone(), 0.2));
        let report = scan_keyspace(&*raw as &dyn Store, &descriptor).unwrap();
        assert_eq!(report.collision_groups.len(), 1);
        assert_eq!(report.collision_groups[0].rows.len(), 2);
        assert_eq!(
            report.collision_groups[0].principal_fingerprints,
            vec![fingerprint(&member)],
            "same fingerprint rule as this store's refusals"
        );
        assert!(report.must_fail_closed());
        assert!(matches!(
            att_store.get_attestations_for(&member),
            Err(FederationError::AttestationStorePrincipalCollision { .. })
        ));
    }

    // ----- revocation atomicity (#2704 review, P1) --------------------------
    //
    // A revocation that deleted one alias row and then failed would leave the
    // other row as the only row for its `(principal, source)` pair — and a
    // pair of one is not a collision, so the next read would *accept* the
    // exact attestation the revocation was called to remove. These fixtures
    // interrupt a revocation where a crash or an I/O error would interrupt it
    // and then reopen the database from disk, because the property is about
    // persisted state and an in-process handle cannot speak for it.

    /// A store that hands every call to a real sled database but fails the
    /// `fail_on`-th mutation, and counts how many mutations it was asked for.
    ///
    /// Counting matters as much as failing: "the revocation is atomic" is the
    /// claim that there is no boundary *between* two mutations to be
    /// interrupted at, and a count of one is what proves the boundary is gone.
    struct InterruptAfter {
        inner: Arc<SledStore>,
        mutations: AtomicUsize,
        fail_on: usize,
    }

    impl InterruptAfter {
        /// Fail the `fail_on`-th mutation (1-based); `usize::MAX` never fails.
        fn new(inner: Arc<SledStore>, fail_on: usize) -> Self {
            Self {
                inner,
                mutations: AtomicUsize::new(0),
                fail_on,
            }
        }

        fn mutations(&self) -> usize {
            self.mutations.load(Ordering::SeqCst)
        }

        /// Record one mutation and say whether this is the one that fails.
        fn arm(&self) -> anyhow::Result<()> {
            if self.mutations.fetch_add(1, Ordering::SeqCst) + 1 == self.fail_on {
                anyhow::bail!("injected storage failure");
            }
            Ok(())
        }
    }

    impl Store for InterruptAfter {
        fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            self.inner.get(key)
        }

        fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
            self.arm()?;
            self.inner.put(key, value)
        }

        fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
            self.arm()?;
            self.inner.delete(key)
        }

        fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
            self.arm()?;
            self.inner.delete_atomic(keys)
        }

        fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan(prefix)
        }

        fn flush(&self) -> anyhow::Result<()> {
            Store::flush(self.inner.as_ref())
        }

        fn get_replica_metadata(
            &self,
            hash: &icn_store::ContentHash,
        ) -> anyhow::Result<Option<icn_store::ReplicaMetadata>> {
            self.inner.get_replica_metadata(hash)
        }

        fn put_replica_metadata(&self, meta: &icn_store::ReplicaMetadata) -> anyhow::Result<()> {
            self.inner.put_replica_metadata(meta)
        }

        fn list_replica_hashes(&self) -> anyhow::Result<Vec<icn_store::ContentHash>> {
            self.inner.list_replica_hashes()
        }
    }

    /// Write an alias pair for one `(principal, source)` into a database at
    /// `root`, plus a second source that must survive, then close it.
    fn seed_alias_pair(root: &std::path::Path, member: &Did, alias: &Did) {
        let raw = SledStore::open(root).unwrap();
        put_raw(
            &raw,
            member,
            &attestation("food-coop", member.clone(), 0.85),
        );
        put_raw(&raw, alias, &attestation("food-coop", alias.clone(), 0.25));
        put_raw(
            &raw,
            alias,
            &attestation("housing-coop", alias.clone(), 0.7),
        );
        raw.flush().unwrap();
    }

    /// Open `root` afresh and answer both questions a later start would ask.
    fn reopen_and_classify(
        root: &std::path::Path,
        member: &Did,
    ) -> (usize, Result<Vec<FederatedTrustAttestation>>) {
        let raw = Arc::new(SledStore::open(root).unwrap());
        let rows = raw.scan(ATTESTATION_PREFIX).unwrap().len();
        let att_store = AttestationStore::new(raw.clone() as Arc<dyn Store>);
        (rows, att_store.get_attestations_for(member))
    }

    #[test]
    fn an_interrupted_revocation_never_elects_an_alias_survivor() {
        // The defect this fixture was written for. Interrupt the revocation
        // where a per-row loop has its boundary: after the first row is gone.
        // A loop leaves the second row as the only row for
        // `(principal, food-coop)` — and a pair of one is not a collision, so
        // a fresh store hands that attestation back as valid. One atomic
        // deletion has no such boundary: there is no second mutation to fail.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("federation");
        let member = test_did();
        let alias = alias_spelling(&member);
        seed_alias_pair(&root, &member, &alias);

        let mutations = {
            let inner = Arc::new(SledStore::open(&root).unwrap());
            let faulty = Arc::new(InterruptAfter::new(inner.clone(), 2));
            let att_store = AttestationStore::new(faulty.clone() as Arc<dyn Store>);

            att_store
                .remove_attestation(&alias, "food-coop")
                .expect("a two-row revocation must not have a second mutation to fail at");
            inner.flush().unwrap();
            faulty.mutations()
        };
        assert_eq!(
            mutations, 1,
            "a two-row revocation must ask the store for exactly one mutation"
        );

        // Reopened from disk, so nothing an in-process handle remembered can
        // stand in for what was actually persisted.
        let (rows, lookup) = reopen_and_classify(&root, &member);
        assert_eq!(rows, 1, "both food-coop rows are gone");
        assert_eq!(
            sources_of(&lookup.unwrap()),
            vec!["housing-coop"],
            "a surviving food-coop row would be the revoked attestation, \
             accepted because it no longer has an alias to collide with"
        );
    }

    #[test]
    fn a_failed_revocation_leaves_the_pair_intact_and_still_refused() {
        // The other side of the invariant. When the one mutation does fail,
        // the persisted state is the state revocation started from: both alias
        // rows present and still ambiguous, so nothing has become acceptable.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("federation");
        let member = test_did();
        let alias = alias_spelling(&member);
        seed_alias_pair(&root, &member, &alias);

        {
            let inner = Arc::new(SledStore::open(&root).unwrap());
            let faulty = Arc::new(InterruptAfter::new(inner.clone(), 1));
            let att_store = AttestationStore::new(faulty.clone() as Arc<dyn Store>);

            let err = att_store
                .remove_attestation(&alias, "food-coop")
                .expect_err("the injected failure must surface, not be swallowed");
            assert!(
                matches!(&err, FederationError::Internal(m) if m.contains("injected")),
                "expected the storage failure to propagate, got {err:?}"
            );
            assert_eq!(faulty.mutations(), 1);
            inner.flush().unwrap();
        }

        let (rows, lookup) = reopen_and_classify(&root, &member);
        assert_eq!(rows, 3, "a refused revocation deletes nothing");
        assert!(
            matches!(
                lookup,
                Err(FederationError::AttestationStorePrincipalCollision { .. })
            ),
            "the pair must still be ambiguous: {lookup:?}"
        );
    }

    #[test]
    fn retrying_an_interrupted_revocation_converges() {
        // A revocation that failed is a revocation that did not happen, so the
        // caller's remedy is to ask again. The retry must be ordinary: same
        // arguments, same outcome as if the first attempt had never run.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("federation");
        let member = test_did();
        let alias = alias_spelling(&member);
        seed_alias_pair(&root, &member, &alias);

        {
            let inner = Arc::new(SledStore::open(&root).unwrap());
            let faulty = Arc::new(InterruptAfter::new(inner.clone(), 1));
            let att_store = AttestationStore::new(faulty as Arc<dyn Store>);
            att_store
                .remove_attestation(&member, "food-coop")
                .unwrap_err();

            // Same handle, no injected failure this time.
            let att_store = AttestationStore::new(inner.clone() as Arc<dyn Store>);
            att_store.remove_attestation(&member, "food-coop").unwrap();
            // And a third call over an already-revoked pair is a no-op, not an
            // error: retry must be safe to repeat.
            att_store.remove_attestation(&member, "food-coop").unwrap();
            inner.flush().unwrap();
        }

        let (rows, lookup) = reopen_and_classify(&root, &member);
        assert_eq!(rows, 1, "both food-coop rows are gone");
        assert_eq!(
            sources_of(&lookup.unwrap()),
            vec!["housing-coop"],
            "the other source's attestation is untouched"
        );
    }

    #[test]
    fn an_ordinary_single_row_revocation_still_removes_exactly_its_row() {
        // The atomic path must not need an alias pair to work: one row is the
        // ordinary case, and it is still one mutation.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("federation");
        let member = test_did();

        {
            let raw = Arc::new(SledStore::open(&root).unwrap());
            let att_store = AttestationStore::new(raw.clone() as Arc<dyn Store>);
            att_store
                .store_attestation(attestation("food-coop", member.clone(), 0.85))
                .unwrap();
            att_store
                .store_attestation(attestation("housing-coop", member.clone(), 0.7))
                .unwrap();

            let counted = Arc::new(InterruptAfter::new(raw.clone(), usize::MAX));
            let counting_store = AttestationStore::new(counted.clone() as Arc<dyn Store>);
            counting_store
                .remove_attestation(&member, "food-coop")
                .unwrap();
            assert_eq!(counted.mutations(), 1);
            raw.flush().unwrap();
        }

        let (rows, lookup) = reopen_and_classify(&root, &member);
        assert_eq!(rows, 1);
        assert_eq!(sources_of(&lookup.unwrap()), vec!["housing-coop"]);
    }

    #[test]
    fn revoking_a_pair_that_is_not_there_asks_the_store_for_nothing() {
        // Nothing to delete must not become an empty mutation: a store that
        // was handed a batch would have a write to fail at, and there is no
        // revocation here to make durable.
        let raw = Arc::new(SledStore::temporary().unwrap());
        let counted = Arc::new(InterruptAfter::new(raw.clone(), 1));
        let att_store = AttestationStore::new(counted.clone() as Arc<dyn Store>);

        att_store
            .remove_attestation(&test_did(), "food-coop")
            .unwrap();

        assert_eq!(counted.mutations(), 0);
    }
}
