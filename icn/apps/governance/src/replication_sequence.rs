//! Durable per-`(author, domain)` sequence source for signed governance emission (#2469).
//!
//! # What `seq` is, and what it is emphatically not
//!
//! `SignedGovernanceOp.seq` orders one author's operations **within one governance domain**.
//! It is a *comparator* for same-key conflicts and never an acceptance gate: a receiver that
//! rejected an operation for carrying a lower `seq` than one already seen would permanently
//! discard legitimate operations, because gossip delivers out of order and anti-entropy can
//! deliver hours late. See `docs/design/authenticated-governance-replication.md` §10.
//!
//! Nothing in this module has a receive-side counterpart, and slice 3 deliberately adds none.
//!
//! # Why this is not `icn_net::SigningSequenceCounter`
//!
//! That counter (#2510) is the pattern this one follows, and it is reused where it fits — but
//! it cannot be used directly here. It is hard-keyed to a single global watermark
//! (`outgoing_signing_seq_reserved_end`) because its sequence space is genuinely per-sender:
//! one node, one stream, consumed by a peer's `ReplayGuard`. Governance needs one independent
//! space per `(author, domain)` pair, so that a vote in one institution never orders against a
//! vote in another. Generalising the `icn-net` type in place would put a keying parameter on
//! the gossip hot path in a kernel crate for the benefit of one domain caller; the invariants
//! are cheap to restate and the coupling is not.
//!
//! Inherited verbatim from #2510:
//!
//! - **Block reservation.** The store holds `reserved_end`, an exclusive upper bound. The
//!   durable invariant is *every sequence ever issued is strictly less than the persisted
//!   `reserved_end`*, and the watermark is written **and flushed** before any sequence in the
//!   new block is handed out. A crash resumes at or above anything that could have reached the
//!   network; the unused remainder of a block is skipped, and gaps are harmless because `seq`
//!   is a comparator.
//! - **Fail closed on persistence failure.** `next_sequence` returns an error rather than a
//!   number it could not record. Issuing an unrecorded sequence is what lets a later
//!   incarnation reuse it.
//! - **Semantic versioning.** An unknown version refuses to open rather than guessing.
//! - **First sequence is 1.** Zero is reserved as "no sequence", so a value of 0 in a signed
//!   envelope is always a construction bug rather than a legitimate first operation.
//!
//! # Key encoding
//!
//! ```text
//! b"gov:repl:seq:v1:" ‖ u32_be(len(author)) ‖ author ‖ u32_be(len(domain)) ‖ domain
//! ```
//!
//! Length-prefixed rather than separator-joined, so that distinct pairs are distinct keys by
//! construction. Under a separator scheme, `(a, "x:y")` and `(a ‖ ":x", "y")` encode
//! identically, which would silently merge two sequence spaces into one.
//!
//! Today `Did::from_str` validates DIDs into `did:icn:<base58>`, which contains no `:` after
//! the prefix, so that particular collision is not currently *reachable* — a `GovernanceDomainId`
//! is an unvalidated `String`, but the author half is not. The encoding does not lean on that.
//! Key separation here would otherwise be a property of a validation rule in another crate,
//! enforced nowhere near this file and free to be relaxed without anyone noticing that a
//! sequence counter depended on it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};

use icn_governance::GovernanceDomainId;
use icn_identity::Did;
use icn_store::Store;

/// Number of sequences claimed per durable reservation.
///
/// One store write per 1,000 emitted operations, and at most 1,000 sequences skipped per
/// restart. Governance emission is far colder than the gossip signing path this is borrowed
/// from, so the block could be smaller; it is kept identical to #2510 so the two counters
/// cannot drift into subtly different durability arguments.
const RESERVATION_BLOCK: u64 = 1_000;

/// Prefix for every per-`(author, domain)` watermark.
const SEQUENCE_KEY_PREFIX: &[u8] = b"gov:repl:seq:v1:";

/// Storage key recording which semantic regime produced the watermarks under
/// [`SEQUENCE_KEY_PREFIX`].
const SEQUENCE_VERSION_KEY: &[u8] = b"gov:repl:seq:semantic_version";

/// Semantic regime this binary writes and can interpret.
///
/// Bump only when the meaning of a persisted watermark changes such that a previous version
/// would misread it. There is no legacy value to migrate from: slice 3 is the first code to
/// write these keys, so an absent version key on a store holding watermarks is impossible in
/// practice and is treated as "fresh" rather than guessed at.
const SEQUENCE_SEMANTIC_VERSION: u32 = 1;

/// First sequence a fresh counter issues.
///
/// Zero is reserved to mean "no sequence assigned", so that a zero reaching a signed envelope
/// is unambiguously a bug rather than a legitimate first operation.
const FIRST_SEQUENCE: u64 = 1;

/// Mutable per-pair state. Sequences in `[next, reserved_end)` are safe to issue without
/// touching disk.
#[derive(Clone, Copy)]
struct PairState {
    next: u64,
    reserved_end: u64,
}

/// Durable monotonic sequence source keyed by `(author DID, governance domain)`.
///
/// Each pair advances independently: a counter for `(alice, coop-a)` is untouched by
/// operations from `(alice, coop-b)` or `(bob, coop-a)`.
pub struct GovernanceSequenceCounter {
    /// `None` for the in-memory test/ephemeral counter.
    store: Option<Arc<dyn Store>>,
    /// Cached reservations, keyed by the same encoding used on disk.
    state: Mutex<HashMap<Vec<u8>, PairState>>,
}

impl GovernanceSequenceCounter {
    /// Open a durable counter over `store`.
    ///
    /// Watermarks are read lazily, per pair, on first use — a node may hold many domains and
    /// there is no reason to scan them all at startup.
    pub fn new(store: Arc<dyn Store>) -> Result<Self> {
        let version = Self::reconcile_semantic_version(&store)?;
        tracing::debug!(
            semantic_version = version,
            block = RESERVATION_BLOCK,
            "Opened durable governance replication sequence counter"
        );
        Ok(Self {
            store: Some(store),
            state: Mutex::new(HashMap::new()),
        })
    }

    /// Create a non-durable counter for tests and genuinely ephemeral nodes.
    ///
    /// **Warning**: restarts reset every pair to [`FIRST_SEQUENCE`], reissuing sequences that
    /// have already been signed and published under this author's key. Never use on a node
    /// with persistent storage.
    pub fn in_memory() -> Self {
        Self {
            store: None,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Establish which semantic regime this store's watermarks belong to, stamping it if
    /// absent.
    ///
    /// An unknown version means the store was written by a binary that knows something this
    /// one does not. Refuse to open: a sender that guesses wrong reissues sequences under a
    /// signature that already carried them.
    fn reconcile_semantic_version(store: &Arc<dyn Store>) -> Result<u32> {
        let found = match store
            .get(SEQUENCE_VERSION_KEY)
            .context("Failed to read governance replication sequence semantic version")?
        {
            Some(bytes) => {
                let raw: [u8; 4] = bytes.as_slice().try_into().map_err(|_| {
                    anyhow!(
                        "Corrupt governance replication sequence semantic version: \
                         expected 4 bytes, found {}",
                        bytes.len()
                    )
                })?;
                Some(u32::from_be_bytes(raw))
            }
            None => None,
        };

        match found {
            Some(v) if v == SEQUENCE_SEMANTIC_VERSION => Ok(v),
            Some(v) => Err(anyhow!(
                "Governance replication sequence store has semantic version {v}, but this \
                 binary understands only {SEQUENCE_SEMANTIC_VERSION}; refusing to reinterpret \
                 it, because guessing here reissues sequences that peers have already seen \
                 signed"
            )),
            None => {
                store
                    .put(
                        SEQUENCE_VERSION_KEY,
                        &SEQUENCE_SEMANTIC_VERSION.to_be_bytes(),
                    )
                    .context("Failed to persist governance replication sequence version")?;
                store
                    .flush()
                    .context("Failed to flush governance replication sequence version")?;
                Ok(SEQUENCE_SEMANTIC_VERSION)
            }
        }
    }

    /// Issue the next sequence for `(author, domain)`, extending the durable reservation if
    /// the cached block is exhausted.
    ///
    /// # Errors
    ///
    /// Returns an error if the reservation cannot be persisted. The caller **must** treat that
    /// as fatal for the operation: emitting a signed envelope carrying a sequence that was
    /// never recorded is precisely the ambiguity this counter exists to prevent, and unlike a
    /// dropped message it cannot be detected after the fact.
    ///
    /// # Why the store write happens under the lock
    ///
    /// Correctness depends on "persist, then issue from the new block" being atomic per pair.
    /// Releasing the lock around a blocking write would let two callers each observe an
    /// exhausted reservation and both extend it from the same base, reissuing the overlap.
    /// The cost is amortised to one write per [`RESERVATION_BLOCK`] operations.
    pub fn next_sequence(&self, author: &Did, domain: &GovernanceDomainId) -> Result<u64> {
        let key = sequence_key(author, domain);

        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Governance replication sequence counter mutex poisoned"))?;

        let entry = match state.get(&key).copied() {
            Some(cached) => cached,
            None => {
                // First use of this pair in this process: resume from the persisted watermark.
                let reserved_end = self.load_watermark(&key)?;
                PairState {
                    next: reserved_end.max(FIRST_SEQUENCE),
                    reserved_end,
                }
            }
        };
        let mut entry = entry;

        if entry.next >= entry.reserved_end {
            let new_end = entry.next.checked_add(RESERVATION_BLOCK).context(
                "Governance replication sequence space exhausted for this (author, domain) \
                 pair (requires 2^64 operations)",
            )?;

            // Persist BEFORE issuing anything from the new block, so a crash between here and
            // the publish resumes above every sequence that could have escaped. The flush is
            // what makes that claim true rather than aspirational.
            if let Some(ref store) = self.store {
                store
                    .put(&key, &new_end.to_be_bytes())
                    .context("Failed to persist governance replication sequence reservation")?;
                store
                    .flush()
                    .context("Failed to flush governance replication sequence reservation")?;
            }

            entry.reserved_end = new_end;
        }

        let sequence = entry.next;
        entry.next += 1;
        state.insert(key, entry);
        Ok(sequence)
    }

    /// Read the persisted exclusive upper bound for one pair, or 0 if it has never been used.
    fn load_watermark(&self, key: &[u8]) -> Result<u64> {
        let Some(ref store) = self.store else {
            return Ok(0);
        };
        match store
            .get(key)
            .context("Failed to read governance replication sequence watermark")?
        {
            Some(bytes) => {
                let raw: [u8; 8] = bytes.as_slice().try_into().map_err(|_| {
                    anyhow!(
                        "Corrupt governance replication sequence watermark: \
                         expected 8 bytes, found {}",
                        bytes.len()
                    )
                })?;
                Ok(u64::from_be_bytes(raw))
            }
            None => Ok(0),
        }
    }
}

/// Length-prefixed storage key for one `(author, domain)` pair.
///
/// See the module docs: length prefixing makes distinct pairs distinct keys without relying
/// on any character being absent from a DID or a domain id.
fn sequence_key(author: &Did, domain: &GovernanceDomainId) -> Vec<u8> {
    let author = author.as_str().as_bytes();
    let domain = domain.0.as_bytes();

    let mut key = Vec::with_capacity(SEQUENCE_KEY_PREFIX.len() + 8 + author.len() + domain.len());
    key.extend_from_slice(SEQUENCE_KEY_PREFIX);
    key.extend_from_slice(&(author.len() as u32).to_be_bytes());
    key.extend_from_slice(author);
    key.extend_from_slice(&(domain.len() as u32).to_be_bytes());
    key.extend_from_slice(domain);
    key
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    fn author() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn domain(s: &str) -> GovernanceDomainId {
        GovernanceDomainId::new(s)
    }

    fn temp_store() -> (tempfile::TempDir, Arc<dyn Store>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store: Arc<dyn Store> = Arc::new(SledStore::open(tmp.path()).expect("sled"));
        (tmp, store)
    }

    #[test]
    fn sequences_start_at_one_and_advance() {
        let counter = GovernanceSequenceCounter::in_memory();
        let (a, d) = (author(), domain("coop-a"));

        assert_eq!(counter.next_sequence(&a, &d).unwrap(), 1);
        assert_eq!(counter.next_sequence(&a, &d).unwrap(), 2);
        assert_eq!(counter.next_sequence(&a, &d).unwrap(), 3);
    }

    #[test]
    fn domains_advance_independently() {
        let counter = GovernanceSequenceCounter::in_memory();
        let alice = author();

        assert_eq!(counter.next_sequence(&alice, &domain("coop-a")).unwrap(), 1);
        assert_eq!(counter.next_sequence(&alice, &domain("coop-a")).unwrap(), 2);

        // A different domain is a different space, not a continuation of the first.
        assert_eq!(counter.next_sequence(&alice, &domain("coop-b")).unwrap(), 1);
        assert_eq!(counter.next_sequence(&alice, &domain("coop-a")).unwrap(), 3);
    }

    #[test]
    fn authors_never_share_sequence_state() {
        let counter = GovernanceSequenceCounter::in_memory();
        let d = domain("coop-a");

        let alice = author();
        let bob = author();
        assert_eq!(counter.next_sequence(&alice, &d).unwrap(), 1);
        assert_eq!(counter.next_sequence(&alice, &d).unwrap(), 2);
        assert_eq!(counter.next_sequence(&bob, &d).unwrap(), 1);
    }

    /// The property the length-prefixed key exists for: the encoding is injective over
    /// `(author, domain)` without depending on any byte being absent from either half.
    ///
    /// A `GovernanceDomainId` is an unvalidated `String`, so the domain half genuinely may
    /// contain the separator a naive key would have used.
    #[test]
    fn adversarial_domain_ids_produce_distinct_keys() {
        let alice = author();
        let bob = author();

        let keys = [
            sequence_key(&alice, &domain("a:b")),
            sequence_key(&alice, &domain("a")),
            sequence_key(&alice, &domain("b")),
            sequence_key(&alice, &domain("")),
            sequence_key(&bob, &domain("a:b")),
            sequence_key(&bob, &domain("a")),
        ];

        let mut unique = keys.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), keys.len(), "sequence keys must be injective");
    }

    /// A domain id may itself look like a key suffix. Length prefixing means the parse is
    /// unambiguous regardless.
    #[test]
    fn a_domain_id_cannot_impersonate_another_pair() {
        let alice = author();
        let bob = author();

        // Domain id that literally spells out another author's key tail.
        let impersonating = domain(&format!("{}{}", bob.as_str(), "coop-a"));

        assert_ne!(
            sequence_key(&alice, &impersonating),
            sequence_key(&bob, &domain("coop-a")),
        );
    }

    #[test]
    fn sequences_survive_restart() {
        let (_tmp, store) = temp_store();
        let (a, d) = (author(), domain("coop-a"));

        let issued = {
            let counter = GovernanceSequenceCounter::new(store.clone()).unwrap();
            let first = counter.next_sequence(&a, &d).unwrap();
            let second = counter.next_sequence(&a, &d).unwrap();
            assert_eq!((first, second), (1, 2));
            second
        };

        // Reopen: a fresh process over the same store.
        let reopened = GovernanceSequenceCounter::new(store).unwrap();
        let after_restart = reopened.next_sequence(&a, &d).unwrap();

        assert!(
            after_restart > issued,
            "restart reissued sequence {after_restart} at or below the already-issued {issued}"
        );
    }

    #[test]
    fn restart_isolation_is_per_pair() {
        let (_tmp, store) = temp_store();
        let alice = author();

        {
            let counter = GovernanceSequenceCounter::new(store.clone()).unwrap();
            counter.next_sequence(&alice, &domain("coop-a")).unwrap();
        }

        let reopened = GovernanceSequenceCounter::new(store).unwrap();
        // coop-a advanced past its block; coop-b was never used and must still start fresh.
        assert!(reopened.next_sequence(&alice, &domain("coop-a")).unwrap() > 1);
        assert_eq!(
            reopened.next_sequence(&alice, &domain("coop-b")).unwrap(),
            1
        );
    }

    #[test]
    fn unknown_semantic_version_refuses_to_open() {
        let (_tmp, store) = temp_store();
        store
            .put(
                SEQUENCE_VERSION_KEY,
                &(SEQUENCE_SEMANTIC_VERSION + 1).to_be_bytes(),
            )
            .unwrap();

        assert!(
            GovernanceSequenceCounter::new(store).is_err(),
            "a store from a newer binary must not be reinterpreted"
        );
    }

    #[test]
    fn corrupt_watermark_is_an_error_not_a_reset() {
        let (_tmp, store) = temp_store();
        let (a, d) = (author(), domain("coop-a"));

        GovernanceSequenceCounter::new(store.clone()).unwrap();
        store.put(&sequence_key(&a, &d), b"short").unwrap();

        let counter = GovernanceSequenceCounter::new(store).unwrap();
        assert!(
            counter.next_sequence(&a, &d).is_err(),
            "a corrupt watermark must fail closed, not silently restart the sequence"
        );
    }
}
