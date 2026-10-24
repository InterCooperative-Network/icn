//! N1-R1: the daemon-owned authority registry over a real data directory.
//!
//! These are the registry's startup and recovery contracts — initialization, refusal, receipt —
//! exercised in-process against real sled stores and real files. They do not exercise a
//! production authority transition: nothing in the runtime produces facts yet, so every fact here
//! is written through [`AuthorityRegistry::ingest`] by the test itself, holding the data-directory
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

/// Write the history through the registry, holding the storage lock for the whole write.
fn seeded(dir: &Path) -> AuthorityReceipt {
    {
        let lock = lock(dir);
        let (mut registry, _) = AuthorityRegistry::open(dir, &lock).unwrap();
        for event in history() {
            registry.ingest(&event).unwrap();
        }
    }
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
fn a_start_that_fails_after_its_record_is_committed_is_not_a_fresh_node_next_time() {
    let dir = tempfile::tempdir().unwrap();
    // Initialization commits the record, then the receipt write fails: the start is refused
    // after the commit point.
    let obstacle = dir.path().join("n1-authority-receipt.json.tmp");
    std::fs::create_dir(&obstacle).unwrap();
    assert!(matches!(
        open(dir.path()),
        Err(RegistryError::ReceiptUnwritable { .. })
    ));
    let record = std::fs::read(init_record_path(dir.path())).unwrap();
    std::fs::remove_dir(&obstacle).unwrap();

    let (registry, receipt) = open(dir.path()).unwrap();
    assert_eq!(
        receipt.startup,
        Startup::Rehydrated,
        "a committed record makes the next start a rehydration, never a fresh initialization"
    );
    assert_eq!(std::fs::read(init_record_path(dir.path())).unwrap(), record);
    assert_eq!(hex::encode(registry.instance()), receipt.instance);
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
    // Removed, so the reopen has to write it again rather than leave the first one in place.
    std::fs::remove_file(receipt_path(dir.path())).unwrap();
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
    // A directory where the temporary goes: removing a stale receipt succeeds, and the write
    // itself is what fails.
    std::fs::create_dir(dir.path().join("n1-authority-receipt.json.tmp")).unwrap();

    let error = open(dir.path()).unwrap_err();
    assert!(
        matches!(error, RegistryError::ReceiptUnwritable { .. }),
        "{error}"
    );
    assert!(
        error.to_string().contains("n1-authority-receipt.json.tmp"),
        "the failure is in writing the receipt: {error}"
    );
    assert!(!receipt_path(dir.path()).exists());
}

#[test]
fn a_refusal_before_the_store_is_opened_still_removes_the_stale_receipt() {
    // Removing the stale receipt is part of opening: an earlier receipt does not survive a start
    // the registry refuses, even when the refusal comes before anything is opened.
    let dir = tempfile::tempdir().unwrap();
    seeded(dir.path());
    std::fs::write(init_record_path(dir.path()), b"damaged").unwrap();

    assert!(matches!(
        open(dir.path()),
        Err(RegistryError::RecordUnreadable { .. })
    ));
    assert!(!receipt_path(dir.path()).exists());
}

// ── file safety ─────────────────────────────────────────────────────────────────────────────────

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777
}

/// Set by [`the_record_and_the_receipt_are_owner_only_whatever_the_umask`] for its child.
const UMASK_CHILD_DIR: &str = "ICN_N1R1_UMASK_CHILD_DIR";

/// Not a test on its own: the child half of the next test, and a no-op without its variable.
/// It creates an ordinary file as a control, then opens a fresh registry.
#[test]
fn umask_child_creates_a_probe_then_opens_a_fresh_registry() {
    let Some(dir) = std::env::var_os(UMASK_CHILD_DIR) else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::File::create(dir.join("probe")).unwrap();
    drop(open(&dir).unwrap());
}

/// The receipt names subjects, authority keys and device grants: not secrets, but metadata a node
/// has no reason to show other local accounts. Both files are created owner-only, so a lax umask
/// cannot widen them (it only ever removes bits).
///
/// The mask is set explicitly, to a permissive 022, in a child process, so the result cannot
/// depend on whatever mask this test inherited. The child's probe shows the mask really applied.
#[cfg(unix)]
#[test]
fn the_record_and_the_receipt_are_owner_only_whatever_the_umask() {
    let dir = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"umask 022 && exec "$0" --exact umask_child_creates_a_probe_then_opens_a_fresh_registry --test-threads=1 -q"#)
        .arg(std::env::current_exe().unwrap())
        .env(UMASK_CHILD_DIR, dir.path())
        .status()
        .unwrap();
    assert!(status.success(), "the child must open the registry");
    assert_eq!(
        mode(&dir.path().join("probe")),
        0o644,
        "control: the child ran, under umask 022"
    );
    assert_eq!(mode(&init_record_path(dir.path())), 0o600);
    assert_eq!(mode(&receipt_path(dir.path())), 0o600);
}

/// Whoever can place a link at the temporary's name must not be able to direct the write to a
/// file outside the data directory.
#[cfg(unix)]
#[test]
fn a_link_planted_at_a_temporary_name_is_not_written_through() {
    for name in [
        "n1-authority-initialized.json.tmp",
        "n1-authority-receipt.json.tmp",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("victim");
        std::fs::write(&victim, b"untouched").unwrap();
        std::os::unix::fs::symlink(&victim, dir.path().join(name)).unwrap();

        drop(open(dir.path()).unwrap());
        assert_eq!(std::fs::read(&victim).unwrap(), b"untouched", "{name}");
        for path in [init_record_path(dir.path()), receipt_path(dir.path())] {
            assert!(
                std::fs::symlink_metadata(&path)
                    .unwrap()
                    .file_type()
                    .is_file(),
                "{name}: {} must be a regular file",
                path.display()
            );
        }
    }
}

/// The record this module writes is a regular file. Anything else at its name was put there by
/// something else, and is refused rather than followed.
#[cfg(unix)]
#[test]
fn a_record_that_is_a_link_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    seeded(dir.path());
    let elsewhere = tempfile::tempdir().unwrap();
    let moved = elsewhere.path().join("record.json");
    std::fs::rename(init_record_path(dir.path()), &moved).unwrap();
    std::os::unix::fs::symlink(&moved, init_record_path(dir.path())).unwrap();

    let error = open(dir.path()).unwrap_err();
    assert!(
        matches!(error, RegistryError::RecordUnreadable { .. }),
        "{error}"
    );
}

#[test]
fn discarding_a_receipt_needs_this_directorys_lock() {
    let (dir, other) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    seeded(dir.path());
    let wrong = lock(other.path());
    assert!(AuthorityRegistry::discard_receipt(dir.path(), &wrong).is_err());
    assert!(receipt_path(dir.path()).exists());

    let right = lock(dir.path());
    AuthorityRegistry::discard_receipt(dir.path(), &right).unwrap();
    assert!(!receipt_path(dir.path()).exists());
    // Idempotent: nothing to discard is not an error.
    AuthorityRegistry::discard_receipt(dir.path(), &right).unwrap();
}

// ── the lock ────────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_lock_that_no_longer_covers_its_path_is_refused_and_nothing_is_written() {
    // icn#2758: rename the locked root away and put a fresh directory at its path. The lock still
    // names that path, but it excludes nobody there.
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let held = lock(&root);
    std::fs::rename(&root, parent.path().join("moved")).unwrap();
    std::fs::create_dir(&root).unwrap();

    let error = AuthorityRegistry::open(&root, &held).unwrap_err();
    assert!(matches!(error, RegistryError::Lock { .. }), "{error}");
    assert!(!authority_fact_store_path(&root).exists());
    assert!(!init_record_path(&root).exists());
}

#[test]
fn a_storage_lock_taken_past_the_restore_marker_still_cannot_open_the_store() {
    // `acquire_for_restore_recovery` is a storage lock that deliberately skips the marker. The
    // registry asks again instead of trusting the lock to have asked.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(icn_core::restore_marker_path(dir.path()), b"").unwrap();
    let seen = icn_core::marker_seen(dir.path()).unwrap().unwrap();
    let recovery =
        DataDirLock::acquire_for_restore_recovery(dir.path(), "a recovery", &seen).unwrap();

    let error = AuthorityRegistry::open(dir.path(), &recovery).unwrap_err();
    assert!(error.to_string().contains("did not finish"), "{error}");
    assert!(!authority_fact_store_path(dir.path()).exists());
    assert!(!init_record_path(dir.path()).exists());
}

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
