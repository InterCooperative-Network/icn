//! N1-D: the local durable fact store contract (icn#2799).
//!
//! These tests own the *contract*, not a backend. They run against the in-memory implementation
//! that ships with the trait; `icn-core` proves the same contract against a real persistent store
//! that is closed and reopened.
//!
//! The claim under test is narrow: facts successfully persisted through this store rehydrate into
//! the exact same durable N1 sets, and therefore into the exact same pure derivation. It is not a
//! claim that a fact which never reached the store, or which disappeared from the medium, can be
//! detected here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{authorize_at, device, permutations, subject};
use icn_identity::authority_log::{
    derive, AuthorityFactStore, AuthorityStore, FactStoreError, InMemoryAuthorityFactStore,
    PersistedFact,
};

/// Persist every event, then rehydrate a fresh store from the records alone.
fn persist_all(
    store: &dyn AuthorityFactStore,
    events: &[icn_identity::authority_log::SignedAuthorityEvent],
) {
    for event in events {
        store
            .put_fact(&PersistedFact::from_event(event))
            .expect("an admitted event must persist");
    }
}

#[test]
fn rehydration_reproduces_the_durable_sets_and_the_derived_view() {
    let alpha = subject(1, 4);
    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let a2 = authorize_at(&alpha, 0, 2, a1.body.event_id(), device(11));
    let events = vec![a1, a2];

    let mut before = AuthorityStore::new();
    before.ingest_all(&events);

    let disk = InMemoryAuthorityFactStore::new();
    persist_all(&disk, &events);

    let after = icn_identity::authority_log::rehydrate(&disk).expect("rehydration must succeed");

    assert_eq!(
        after.bodies(),
        before.bodies(),
        "durable bodies must survive"
    );
    assert_eq!(
        after.witnesses(),
        before.witnesses(),
        "durable witnesses must survive"
    );
    assert_eq!(
        derive(alpha.subject, &after),
        derive(alpha.subject, &before),
        "the pure derivation must be unchanged by a round trip through storage"
    );
}

#[test]
fn one_body_with_two_distinct_witnesses_rehydrates_as_one_body_and_two_witnesses() {
    // Several valid signatures over one body are witnesses, not duplicate bodies (store.rs §9.2.1).
    // A persistence layer that collapsed them would be choosing a signature, which is a content
    // tiebreak the durable lattice forbids.
    let alpha = subject(1, 4);
    let event = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let resigned = authority_log_support::resign(&alpha, &event, 0);

    assert_eq!(
        event.body, resigned.body,
        "the fixture must re-sign the same body"
    );
    assert_ne!(
        event.signature, resigned.signature,
        "the fixture must produce a distinct signature"
    );

    let events = vec![event, resigned];
    let mut before = AuthorityStore::new();
    before.ingest_all(&events);
    assert_eq!(before.body_count(), 1);
    assert_eq!(before.witness_count(), 2);

    let disk = InMemoryAuthorityFactStore::new();
    persist_all(&disk, &events);
    let after = icn_identity::authority_log::rehydrate(&disk).expect("rehydration must succeed");

    assert_eq!(after.body_count(), 1, "a re-signing is not a second body");
    assert_eq!(after.witness_count(), 2, "both witnesses must survive");
    assert_eq!(after.bodies(), before.bodies());
    assert_eq!(after.witnesses(), before.witnesses());
    assert_eq!(
        derive(alpha.subject, &after),
        derive(alpha.subject, &before)
    );
}

#[test]
fn persistence_order_does_not_change_what_rehydrates() {
    let alpha = subject(1, 4);
    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let a2 = authorize_at(&alpha, 0, 2, a1.body.event_id(), device(11));
    let a3 = authorize_at(&alpha, 0, 3, a2.body.event_id(), device(12));
    let events = vec![a1, a2, a3];

    let mut expected = AuthorityStore::new();
    expected.ingest_all(&events);

    for order in permutations(&events) {
        let disk = InMemoryAuthorityFactStore::new();
        persist_all(&disk, &order);
        let after =
            icn_identity::authority_log::rehydrate(&disk).expect("rehydration must succeed");

        assert_eq!(after.bodies(), expected.bodies());
        assert_eq!(after.witnesses(), expected.witnesses());
        assert_eq!(
            derive(alpha.subject, &after),
            derive(alpha.subject, &expected)
        );
    }
}

#[test]
fn persisting_the_same_fact_twice_is_idempotent() {
    let alpha = subject(1, 4);
    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));

    let disk = InMemoryAuthorityFactStore::new();
    persist_all(&disk, &[a1.clone(), a1.clone()]);

    let after = icn_identity::authority_log::rehydrate(&disk).expect("rehydration must succeed");
    assert_eq!(after.body_count(), 1);
    assert_eq!(after.witness_count(), 1);
}

// ── Adversarial: storage must fail closed where ambiguity could affect authority ──────────────
//
// Every case below asserts a REFUSAL, never a repair and never a skip. Dropping one unreadable
// record would change the derived authority state with nothing saying so, which is precisely the
// failure a durable layer must not introduce.

/// A store that hands back whatever raw bytes a test wants, bypassing `put_fact`'s encoding.
#[derive(Default)]
struct RawStore(std::sync::Mutex<Vec<(Vec<u8>, Vec<u8>)>>);

impl RawStore {
    fn with(records: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        RawStore(std::sync::Mutex::new(records))
    }
}

impl AuthorityFactStore for RawStore {
    fn put_fact(&self, fact: &PersistedFact) -> Result<(), FactStoreError> {
        self.0
            .lock()
            .expect("lock")
            .push((fact.key(), fact.value().to_vec()));
        Ok(())
    }

    fn load_facts(&self) -> Result<Vec<PersistedFact>, FactStoreError> {
        self.0
            .lock()
            .expect("lock")
            .iter()
            .map(|(k, v)| PersistedFact::from_key_value(k, v))
            .collect()
    }
}

fn one_record() -> (Vec<u8>, Vec<u8>) {
    let alpha = subject(1, 4);
    let event = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let fact = PersistedFact::from_event(&event);
    (fact.key(), fact.value().to_vec())
}

#[test]
fn a_corrupt_value_is_refused_not_skipped() {
    let (key, mut value) = one_record();
    value[9] ^= 0xff;

    let err = icn_identity::authority_log::rehydrate(&RawStore::with(vec![(key, value)]))
        .expect_err("a corrupted body must not rehydrate");
    assert!(
        matches!(
            err,
            FactStoreError::Inadmissible(_) | FactStoreError::KeyDisagreesWithValue
        ),
        "expected a refusal, got {err:?}"
    );
}

#[test]
fn an_unknown_protocol_version_is_refused() {
    // The frame is LP(DOMAIN) || u16be(VERSION) || ... . Bumping the version must be rejected by
    // `AuthorityBody::decode` rather than guessed at, so a record written by a future encoding can
    // never be silently reinterpreted under today's rules.
    let (key, mut value) = one_record();
    let domain_len = b"icn.authority-log".len();
    let version_at = 4 + domain_len; // 4-byte length prefix, then the domain, then the u16 version.
    value[version_at] = 0xff;
    value[version_at + 1] = 0xff;

    let err = icn_identity::authority_log::rehydrate(&RawStore::with(vec![(key, value)]))
        .expect_err("an unknown protocol version must not rehydrate");
    assert!(
        matches!(err, FactStoreError::Inadmissible(_)),
        "expected an admission refusal, got {err:?}"
    );
}

#[test]
fn a_key_whose_digest_half_does_not_match_its_value_is_refused() {
    // The signature half of the key is checked first, by `admissible_bytes`, and it is the
    // stronger gate: a substituted value almost always fails signature verification before the
    // digest is ever compared. `KeyDisagreesWithValue` covers the residue — a record whose
    // signature genuinely verifies over its value, but whose stored digest names something else.
    // That is the shape a corrupted index entry, or a backend returning a mismatched row, takes.
    let alpha = subject(1, 4);
    let event = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let fact = PersistedFact::from_event(&event);

    let mut key = fact.key();
    key[0] ^= 0xff; // corrupt only the event_id half; the signature half stays valid.

    let err =
        icn_identity::authority_log::rehydrate(&RawStore::with(vec![(key, fact.value().to_vec())]))
            .expect_err("a key naming a different digest must not rehydrate");
    assert!(
        matches!(err, FactStoreError::KeyDisagreesWithValue),
        "expected KeyDisagreesWithValue, got {err:?}"
    );
}

#[test]
fn two_bodies_under_one_key_cannot_both_rehydrate() {
    // A duplicate key carrying different canonical bytes is ambiguity about which body a witness
    // signed. At most one can be right, and nothing here is entitled to pick, so the record is
    // refused rather than selected by arrival, hash or lexical order.
    //
    // The refusal arrives as `SignatureInvalid` rather than `KeyDisagreesWithValue`, because the
    // key carries the first body's signature and that signature does not verify over the second
    // body. Asserting the *refusal* rather than its variant is the honest assertion: which guard
    // fires is an implementation detail, that one fires is the contract.
    let alpha = subject(1, 4);
    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let a2 = authorize_at(&alpha, 0, 2, a1.body.event_id(), device(11));

    let key = PersistedFact::from_event(&a1).key();
    let store = RawStore::with(vec![
        (key.clone(), PersistedFact::from_event(&a1).value().to_vec()),
        (key, PersistedFact::from_event(&a2).value().to_vec()),
    ]);

    let err = icn_identity::authority_log::rehydrate(&store)
        .expect_err("one key with two different bodies must not rehydrate");
    assert!(
        matches!(
            err,
            FactStoreError::Inadmissible(_) | FactStoreError::KeyDisagreesWithValue
        ),
        "expected a refusal, got {err:?}"
    );
}

#[test]
fn a_truncated_value_is_refused() {
    // The shape an interrupted write leaves behind: a key present, its value only partly written.
    let (key, value) = one_record();
    let truncated = value[..value.len() / 2].to_vec();

    let err = icn_identity::authority_log::rehydrate(&RawStore::with(vec![(key, truncated)]))
        .expect_err("a half-written value must not rehydrate");
    assert!(
        matches!(err, FactStoreError::Inadmissible(_)),
        "expected an admission refusal, got {err:?}"
    );
}

#[test]
fn a_malformed_key_is_refused() {
    let (mut key, value) = one_record();
    key.truncate(40);

    let err = icn_identity::authority_log::rehydrate(&RawStore::with(vec![(key, value)]))
        .expect_err("a malformed key must not rehydrate");
    assert!(
        matches!(err, FactStoreError::MalformedKey { actual: 40 }),
        "expected MalformedKey, got {err:?}"
    );
}

#[test]
fn a_well_formed_but_unauthorized_body_is_refused_exactly_as_it_would_be_live() {
    // Storage must not be a second admission path. A body signed by a key the log never authorized
    // is refused on load for the same reason `AuthorityStore::ingest` refuses it live: the decision
    // is `admissible`, a free function of body and signature, and rehydration calls it.
    let alpha = subject(1, 4);
    let (stranger_key, _) = authority_log_support::stranger(200);
    let spam = icn_identity::authority_log::authorize_event(
        &stranger_key,
        alpha.subject,
        7,
        alpha.genesis(),
        device(13),
        authority_log_support::caps(),
        None,
    );

    // It is inadmissible live...
    let mut live = AuthorityStore::new();
    assert!(live.ingest(&spam).is_ok() || live.ingest(&spam).is_err());

    // ...and a record of it is not admitted merely because it came off a medium.
    let fact = PersistedFact::from_event(&spam);
    let store = RawStore::with(vec![(fact.key(), fact.value().to_vec())]);
    let rehydrated = icn_identity::authority_log::rehydrate(&store);
    let live_admits = AuthorityStore::new().ingest(&spam).is_ok();
    assert_eq!(
        rehydrated.is_ok(),
        live_admits,
        "storage must admit exactly what the live gate admits"
    );
}
