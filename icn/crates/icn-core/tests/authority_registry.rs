//! N1-R1: the daemon-owned authority registry over a real data directory.
//!
//! These are the registry's startup and recovery contracts — initialization, refusal, receipt —
//! exercised in-process against real sled stores and real files. They do not exercise a
//! production authority transition: nothing in the runtime produces facts yet, so every fact here
//! is written through [`AuthorityRegistry::ingest`] by the test itself, under the data-directory
//! lock. The daemon-level versions of the same claims, against the real `icnd` binary, are in
//! `bins/icnd/tests/authority_restart_test.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use icn_core::authority_facts::AUTHORITY_FACT_PREFIX;
use icn_core::authority_registry::{
    init_record_path, receipt_path, AuthorityReceipt, AuthorityRegistry, RegistryError, Startup,
    ViewReceipt, INSTANCE_KEY,
};
use icn_core::config::authority_fact_store_path;
use icn_core::DataDirLock;
use icn_identity::authority_log::{
    authorize_event, derive, AuthorityStore, CapabilitySet, ContextNonce, ContinuityRoot,
    DeviceCapability, PrincipalKey, SignedAuthorityEvent,
};
use icn_store::{SledStore, Store};

// ── fixtures ────────────────────────────────────────────────────────────────────────────────────

fn device(seed: u8) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key(),
    )
    .unwrap()
}

fn root(seed: u8) -> ContinuityRoot {
    ContinuityRoot::new([seed; 32], ContextNonce::from_bytes([seed ^ 0xa5; 32]), 2)
}

fn grant(
    signer: &ContinuityRoot,
    subject_inception: &SignedAuthorityEvent,
    position: u64,
    prev: &SignedAuthorityEvent,
    dev: u8,
) -> SignedAuthorityEvent {
    authorize_event(
        &signer.authority_signing_key(0),
        subject_inception.body.subject(),
        position,
        prev.body.event_id(),
        device(dev),
        CapabilitySet::new([DeviceCapability::Sign]),
        None,
    )
}

/// Two subjects whose derived decisions differ in kind:
///
/// - subject 1 is `Live` at frontier 2 with exactly device 10. It also holds a well-formed grant
///   for device 12 signed by subject 2's key — admitted, persisted, and ignored by `derive`.
/// - subject 2 is `Halted` at position 1: two equally-authorized grants compete there.
fn history() -> Vec<SignedAuthorityEvent> {
    let (r1, r2) = (root(1), root(2));
    let i1 = r1.incept().unwrap();
    let i2 = r2.incept().unwrap();
    let a1 = grant(&r1, &i1, 1, &i1, 10);
    let stranger = grant(&r2, &i1, 2, &a1, 12);
    let c1 = grant(&r2, &i2, 1, &i2, 20);
    let c2 = grant(&r2, &i2, 1, &i2, 21);
    vec![i1, a1, stranger, i2, c1, c2]
}

fn independent_store(events: &[SignedAuthorityEvent]) -> AuthorityStore {
    let mut store = AuthorityStore::new();
    for event in events {
        store.ingest(event).unwrap();
    }
    store
}

fn lock(dir: &Path) -> DataDirLock {
    DataDirLock::acquire(dir, "the authority registry test").unwrap()
}

fn open(dir: &Path) -> Result<(AuthorityRegistry, AuthorityReceipt), RegistryError> {
    let lock = lock(dir);
    AuthorityRegistry::open(dir, &lock)
}

fn read_receipt(dir: &Path) -> AuthorityReceipt {
    serde_json::from_slice(&std::fs::read(receipt_path(dir)).unwrap()).unwrap()
}

fn seeded(dir: &Path) -> AuthorityReceipt {
    let (mut registry, _) = open(dir).unwrap();
    for event in history() {
        registry.ingest(&event).unwrap();
    }
    drop(registry);
    let (_, receipt) = open(dir).unwrap();
    receipt
}

// ── initialization (D1) ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_fresh_node_initializes_records_it_and_reopens_as_the_same_instance() {
    let dir = tempfile::tempdir().unwrap();

    let (registry, first) = open(dir.path()).unwrap();
    assert_eq!(first.startup, Startup::Initialized);
    assert_eq!(first.body_count, 0);
    assert!(authority_fact_store_path(dir.path()).is_dir());
    assert!(init_record_path(dir.path()).is_file());
    assert_eq!(
        read_receipt(dir.path()),
        first,
        "the file is the receipt returned"
    );
    drop(registry);

    let (_, second) = open(dir.path()).unwrap();
    assert_eq!(second.startup, Startup::Rehydrated);
    assert_eq!(second.instance, first.instance);
}

#[test]
fn an_initialized_node_whose_store_is_missing_refuses_and_does_not_recreate_it() {
    let dir = tempfile::tempdir().unwrap();
    seeded(dir.path());
    std::fs::remove_dir_all(authority_fact_store_path(dir.path())).unwrap();

    let error = open(dir.path()).unwrap_err();
    assert!(
        matches!(error, RegistryError::StoreMissing { .. }),
        "{error}"
    );
    assert!(
        !authority_fact_store_path(dir.path()).exists(),
        "a refusal must not leave an empty store for the next start to accept"
    );
    assert!(!receipt_path(dir.path()).exists());
}

#[test]
fn an_initialized_node_whose_store_was_recreated_empty_refuses() {
    let dir = tempfile::tempdir().unwrap();
    seeded(dir.path());
    let store = authority_fact_store_path(dir.path());
    std::fs::remove_dir_all(&store).unwrap();
    drop(SledStore::open(&store).unwrap());

    let error = open(dir.path()).unwrap_err();
    assert!(
        matches!(error, RegistryError::StoreReplaced { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("found none"), "{error}");
}

#[test]
fn a_store_from_another_node_refuses() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    seeded(a.path());
    drop(open(b.path()).unwrap());
    // Give `a` the record of `b`: same layout, different instance.
    std::fs::copy(init_record_path(b.path()), init_record_path(a.path())).unwrap();

    let error = open(a.path()).unwrap_err();
    assert!(
        matches!(error, RegistryError::StoreReplaced { .. }),
        "{error}"
    );
}

#[test]
fn a_store_holding_facts_without_its_record_refuses() {
    let dir = tempfile::tempdir().unwrap();
    seeded(dir.path());
    std::fs::remove_file(init_record_path(dir.path())).unwrap();

    let error = open(dir.path()).unwrap_err();
    match error {
        RegistryError::RecordMissing { facts, .. } => assert_eq!(facts, history().len()),
        other => panic!("expected RecordMissing, got {other}"),
    }
    assert!(
        !init_record_path(dir.path()).exists(),
        "no record is invented"
    );
}

#[test]
fn an_initialization_interrupted_before_its_instance_row_completes() {
    let dir = tempfile::tempdir().unwrap();
    // The crash window between creating the database and writing anything into it.
    drop(SledStore::open(authority_fact_store_path(dir.path())).unwrap());

    let (_, receipt) = open(dir.path()).unwrap();
    assert_eq!(receipt.startup, Startup::CompletedInterruptedInitialization);
    assert!(init_record_path(dir.path()).is_file());
}

#[test]
fn an_initialization_interrupted_before_its_record_adopts_the_stores_instance() {
    let dir = tempfile::tempdir().unwrap();
    // The crash window between the durable instance row and the record that names it.
    let instance = [0x5a; 16];
    {
        let store = SledStore::open(authority_fact_store_path(dir.path())).unwrap();
        store.put(INSTANCE_KEY, &instance).unwrap();
        store.flush().unwrap();
    }

    let (registry, receipt) = open(dir.path()).unwrap();
    assert_eq!(receipt.startup, Startup::CompletedInterruptedInitialization);
    assert_eq!(registry.instance(), instance);
    drop(registry);
    assert_eq!(open(dir.path()).unwrap().1.startup, Startup::Rehydrated);
}

#[test]
fn an_unreadable_record_refuses_before_anything_is_opened() {
    for (name, bytes) in [
        ("empty", b"".as_slice()),
        ("torn", b"{\"format\":\"icn.n1.authority-store-in".as_slice()),
        (
            "future version",
            br#"{"format":"icn.n1.authority-store-initialized","version":2,"instance":"00000000000000000000000000000000","store":"store/authority"}"#
                .as_slice(),
        ),
        (
            "other store",
            br#"{"format":"icn.n1.authority-store-initialized","version":1,"instance":"00000000000000000000000000000000","store":"store/ledger"}"#
                .as_slice(),
        ),
        (
            "short instance",
            br#"{"format":"icn.n1.authority-store-initialized","version":1,"instance":"00","store":"store/authority"}"#
                .as_slice(),
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(init_record_path(dir.path()), bytes).unwrap();

        let error = open(dir.path()).unwrap_err();
        assert!(
            matches!(error, RegistryError::RecordUnreadable { .. }),
            "{name}: {error}"
        );
        assert!(
            !authority_fact_store_path(dir.path()).exists(),
            "{name}: a damaged record must not lead to a fresh store"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_store_that_cannot_be_inspected_is_not_mistaken_for_a_fresh_node() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let parent = icn_core::config::store_path(dir.path());
    std::fs::create_dir(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::symlink_metadata(authority_fact_store_path(dir.path())).is_ok()
        || std::fs::read_dir(&parent).is_ok()
    {
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        eprintln!("SKIPPED: this process can see through a 0000 directory (root?)");
        return;
    }

    let result = open(dir.path());
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();

    let error = result.unwrap_err();
    assert!(matches!(error, RegistryError::Store { .. }), "{error}");
    assert!(
        !init_record_path(dir.path()).exists(),
        "an unobservable store must not be initialized over"
    );
}

#[test]
fn a_leftover_temporary_record_is_never_read_and_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().join("n1-authority-initialized.json.tmp");
    std::fs::write(&tmp, b"garbage from an interrupted write").unwrap();

    let (_, receipt) = open(dir.path()).unwrap();
    assert_eq!(receipt.startup, Startup::Initialized);
    assert!(!tmp.exists(), "the next write renames over the temporary");
}

// ── rehydration and the receipt (D2) ────────────────────────────────────────────────────────────

#[test]
fn the_receipt_is_the_derivation_over_what_was_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let receipt = seeded(dir.path());
    let expected =
        AuthorityReceipt::of(&independent_store(&history()), [0; 16], Startup::Rehydrated);

    assert_eq!(receipt.startup, Startup::Rehydrated);
    assert_eq!(receipt.body_count, 6);
    assert_eq!(receipt.witness_count, 6);
    assert_eq!(receipt.subjects, expected.subjects);
    assert_eq!(receipt.decisions_digest, expected.decisions_digest);
    assert_eq!(read_receipt(dir.path()), receipt);

    // And the decisions themselves, checked against `derive` rather than against the receipt
    // code that produced them.
    let events = history();
    let store = independent_store(&events);
    let s1 = events[0].body.subject();
    let s2 = events[3].body.subject();
    assert!(!derive(s1, &store).is_halted());
    assert!(derive(s2, &store).is_halted());

    let by_subject = |subject: icn_identity::authority_log::SubjectId| {
        receipt
            .subjects
            .iter()
            .find(|s| s.subject == hex::encode(subject.as_bytes()))
            .unwrap()
            .view
            .clone()
    };
    match by_subject(s1) {
        ViewReceipt::Live { state, frontier } => {
            assert_eq!(frontier, 2);
            let devices: Vec<_> = state.devices.iter().map(|d| d.device.clone()).collect();
            assert_eq!(devices, vec![hex::encode(device(10).as_bytes())]);
            assert_eq!(state.devices[0].capabilities, vec!["sign".to_string()]);
        }
        other => panic!("subject 1 must be Live, got {other:?}"),
    }
    match by_subject(s2) {
        ViewReceipt::Halted {
            disputed_at,
            candidates,
            ..
        } => {
            assert_eq!(disputed_at, 1);
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("subject 2 must be Halted, got {other:?}"),
    }
}

#[test]
fn reopening_reproduces_the_receipt_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    seeded(dir.path());
    let first = std::fs::read(receipt_path(dir.path())).unwrap();
    drop(open(dir.path()).unwrap());
    let second = std::fs::read(receipt_path(dir.path())).unwrap();
    assert_eq!(first, second);
}

#[test]
fn a_corrupt_fact_refuses_the_whole_store_and_leaves_no_receipt() {
    let dir = tempfile::tempdir().unwrap();
    seeded(dir.path());
    assert!(receipt_path(dir.path()).exists());
    {
        let store = SledStore::open(authority_fact_store_path(dir.path())).unwrap();
        let (key, mut value) = store
            .scan(AUTHORITY_FACT_PREFIX)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let last = value.len() - 1;
        value[last] ^= 0xff;
        store.put(&key, &value).unwrap();
        store.flush().unwrap();
    }

    let error = open(dir.path()).unwrap_err();
    assert!(matches!(error, RegistryError::Rehydrate { .. }), "{error}");
    assert!(
        !receipt_path(dir.path()).exists(),
        "a refused start must not leave the previous start's receipt describing a state"
    );
}

#[test]
fn a_receipt_that_cannot_be_written_refuses_the_start() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(receipt_path(dir.path())).unwrap();

    let error = open(dir.path()).unwrap_err();
    assert!(
        matches!(error, RegistryError::ReceiptUnwritable { .. }),
        "{error}"
    );
}

// ── the lock ────────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_lock_on_another_directory_is_refused_and_nothing_is_written() {
    let (target, other) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let other_lock = lock(other.path());

    let error = AuthorityRegistry::open(target.path(), &other_lock).unwrap_err();
    assert!(matches!(error, RegistryError::Lock { .. }), "{error}");
    assert!(!authority_fact_store_path(target.path()).exists());
    assert!(!init_record_path(target.path()).exists());
}

#[test]
fn the_configuration_lock_is_not_the_storage_lock() {
    let dir = tempfile::tempdir().unwrap();
    let config_lock = DataDirLock::acquire_config(dir.path(), "a test").unwrap();

    let error = AuthorityRegistry::open(dir.path(), &config_lock).unwrap_err();
    assert!(matches!(error, RegistryError::Lock { .. }), "{error}");
    assert!(!authority_fact_store_path(dir.path()).exists());
}
