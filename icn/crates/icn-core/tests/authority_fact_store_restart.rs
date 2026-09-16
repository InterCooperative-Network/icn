//! N1-D: local restart durability against a real persistent store (icn#2799).
//!
//! The identity crate proves the *contract* against an in-memory implementation. This proves the
//! part that only a medium can prove: that closing the store, dropping every handle, and reopening
//! the same directory yields the same durable N1 facts and therefore the same derived authority.
//!
//! The restart modelled here is an ordinary process or store restart — the handle is dropped and
//! the directory reopened. It is deliberately not a crash-consistency, torn-write, backup or
//! frontier-completeness proof; those belong to the recovery-integrity lane.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_core::authority_facts::{SledAuthorityFactStore, AUTHORITY_FACT_PREFIX};
use icn_identity::authority_log::{
    authorize_event, derive, rehydrate, AuthorityFactStore, AuthorityStore, CapabilitySet,
    ContextNonce, ContinuityRoot, DeviceCapability, PersistedFact, PrincipalKey,
    SignedAuthorityEvent, SubjectId,
};
use icn_store::{SledStore, Store};

/// A deterministic subject: same seed in, same bytes out.
struct Fixture {
    root: ContinuityRoot,
    subject: SubjectId,
    inception: SignedAuthorityEvent,
}

fn fixture(seed: u8) -> Fixture {
    let root = ContinuityRoot::new([seed; 32], ContextNonce::from_bytes([seed ^ 0xa5; 32]), 4);
    let inception = root.incept().expect("inception must construct");
    let subject = inception.body.subject();
    Fixture {
        root,
        subject,
        inception,
    }
}

fn caps() -> CapabilitySet {
    CapabilitySet::new([DeviceCapability::Sign])
}

/// A device principal derived from a real signing key, as the suite's fixtures do: a
/// `PrincipalKey` must be a valid verifying key, not arbitrary bytes.
fn device(seed: u8) -> PrincipalKey {
    let key = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
    PrincipalKey::try_from_verifying_key(key.verifying_key())
        .expect("a real verifying key must convert")
}

fn authorize(
    f: &Fixture,
    position: u64,
    prev: icn_identity::authority_log::EventId,
    dev: u8,
) -> SignedAuthorityEvent {
    authorize_event(
        &f.root.authority_signing_key(0),
        f.subject,
        position,
        prev,
        device(dev),
        caps(),
        None,
    )
}

/// Open a fact store over a sled database at `path`.
fn open_at(path: &std::path::Path) -> SledAuthorityFactStore {
    let sled = SledStore::open(path).expect("sled must open");
    SledAuthorityFactStore::new(Arc::new(sled) as Arc<dyn Store>)
}

#[test]
fn facts_survive_closing_and_reopening_the_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("authority-facts");

    let f = fixture(7);
    let a1 = authorize(&f, 1, f.inception.body.event_id(), 10);
    let a2 = authorize(&f, 2, a1.body.event_id(), 11);
    let events = vec![f.inception.clone(), a1, a2];

    // ── before the restart ───────────────────────────────────────────────────────────────────
    let before = {
        let store = open_at(&path);
        for event in &events {
            store
                .put_fact(&PersistedFact::from_event(event))
                .expect("persisting an admitted event must succeed");
        }
        rehydrate(&store).expect("rehydration must succeed")
    }; // every handle dropped here: the sled database is closed.

    // ── the restart ──────────────────────────────────────────────────────────────────────────
    let after = {
        let reopened = open_at(&path);
        rehydrate(&reopened).expect("rehydration after restart must succeed")
    };

    // The durable facts themselves, then the pure derivation over them. Asserting only the second
    // would let a storage layer that silently altered the sets pass so long as the verdict landed
    // the same way.
    assert_eq!(
        after.bodies(),
        before.bodies(),
        "the durable body set must be unchanged by a restart"
    );
    assert_eq!(
        after.witnesses(),
        before.witnesses(),
        "the durable witness set must be unchanged by a restart"
    );
    assert_eq!(
        derive(f.subject, &after),
        derive(f.subject, &before),
        "the derived authority must be unchanged by a restart"
    );

    // And the state is genuinely non-trivial, so the assertions above are not comparing two
    // empty stores.
    assert_eq!(after.body_count(), 3);
    assert!(matches!(
        derive(f.subject, &after),
        icn_identity::authority_log::AuthorityView::Live { .. }
    ));
}

#[test]
fn a_reopened_store_matches_one_that_never_restarted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("authority-facts");

    let f = fixture(9);
    let a1 = authorize(&f, 1, f.inception.body.event_id(), 20);
    let events = vec![f.inception.clone(), a1];

    {
        let store = open_at(&path);
        for event in &events {
            store.put_fact(&PersistedFact::from_event(event)).unwrap();
        }
    }

    let reference = {
        let mut in_ram = AuthorityStore::new();
        in_ram.ingest_all(&events);
        in_ram
    };
    let reopened = rehydrate(&open_at(&path)).expect("rehydration must succeed");

    assert_eq!(
        reopened.body_count(),
        events.len(),
        "the comparison must not be between two empty stores"
    );
    assert_eq!(reopened.bodies(), reference.bodies());
    assert_eq!(reopened.witnesses(), reference.witnesses());
    assert_eq!(
        derive(f.subject, &reopened),
        derive(f.subject, &reference),
        "persisting and reloading must be indistinguishable from never having persisted"
    );
}

#[test]
fn write_order_does_not_survive_into_the_reopened_state() {
    let dir = tempfile::tempdir().expect("tempdir");

    let f = fixture(11);
    let a1 = authorize(&f, 1, f.inception.body.event_id(), 30);
    let a2 = authorize(&f, 2, a1.body.event_id(), 31);
    let forward = vec![f.inception.clone(), a1, a2];
    let mut reverse = forward.clone();
    reverse.reverse();

    let persist = |name: &str, events: &[SignedAuthorityEvent]| {
        let path = dir.path().join(name);
        {
            let store = open_at(&path);
            for event in events {
                store.put_fact(&PersistedFact::from_event(event)).unwrap();
            }
        }
        rehydrate(&open_at(&path)).expect("rehydration must succeed")
    };

    let a = persist("forward", &forward);
    let b = persist("reverse", &reverse);

    // Non-emptiness first. Two stores that persisted nothing also compare equal, so without this
    // the whole test passes against an adapter whose `put_fact` is a no-op — confirmed by
    // mutating exactly that, which left this test green while the substantive ones failed.
    assert_eq!(
        a.body_count(),
        forward.len(),
        "the fixture must actually persist"
    );
    assert_eq!(
        b.body_count(),
        forward.len(),
        "the fixture must actually persist"
    );

    assert_eq!(a.bodies(), b.bodies());
    assert_eq!(a.witnesses(), b.witnesses());
    assert_eq!(derive(f.subject, &a), derive(f.subject, &b));
}

#[test]
fn a_corrupt_row_on_disk_is_refused_after_restart() {
    // Storage fails closed: a damaged row is a refusal, not a silently smaller history. Returning
    // a partially-rehydrated store would be handing back a different subject history than the one
    // that was persisted, with nothing saying so.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("authority-facts");

    let f = fixture(13);
    let a1 = authorize(&f, 1, f.inception.body.event_id(), 40);
    let fact = PersistedFact::from_event(&a1);

    {
        let store = open_at(&path);
        store
            .put_fact(&PersistedFact::from_event(&f.inception))
            .unwrap();
        store.put_fact(&fact).unwrap();
    }

    // Damage one row underneath the adapter, exactly as a bad sector would.
    {
        let sled = SledStore::open(&path).expect("sled must open");
        let mut key = AUTHORITY_FACT_PREFIX.to_vec();
        key.extend_from_slice(&fact.key());
        let mut value = fact.value().to_vec();
        value[7] ^= 0xff;
        sled.put(&key, &value).expect("corrupting write must land");
    }

    let err = rehydrate(&open_at(&path)).expect_err("a corrupt row must not rehydrate");
    // The refusal is what matters, not which guard fired.
    let message = err.to_string();
    assert!(
        !message.is_empty(),
        "a refusal must carry a diagnosis, got {err:?}"
    );
}
