//! KNOWN LIMITS of the typed monotonicity invariant (#2864 A2).
//!
//! `SledGovernanceStateStore::save_domain` refuses to replace a stored
//! `ContextSubjectV1` domain with a non-Subject one. That is a rule of the TYPED
//! API. Anything holding an unrestricted `dyn Store` (or `sled::Db`) for the same
//! database can still write any key. These tests PIN that, with synthetic stores
//! only (`SledStore::temporary`), so nobody mistakes the typed check for a
//! boundary against raw access:
//!
//! * probe 1 is the control: the typed path refuses and leaves the bytes identical;
//! * probes 2 to 5 are bypasses by a holder of a raw handle (put, delete then
//!   recreate, compare_and_put, `SledStore::import_snapshot`);
//! * probe 6 records the sharing and directory-lock facts the single-writer
//!   assumption rests on.
//!
//! What #2864 A2 REMOVED is the library API that handed such a handle out: the public
//! `SledGovernanceStateStore::as_store()` and the `governance_store` field exported by
//! `init_governance_actor`. Those removals are guarded by `compile_fail` doctests in
//! `state_store.rs` and `init.rs`, not here, because they are about what does NOT
//! compile. A probe asserting that a bypass works is a recorded limitation, not an
//! endorsement; if a future mechanism closes one, update the probe in the same change.
//!
//! This is an integration test, so it sees exactly the public surface a dependent
//! crate sees. Run with `--nocapture` to read the exact stored bytes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_governance::{
    ActorModelError, GovernanceConfig, GovernanceDomain, GovernanceDomainId, HumanActorModel,
    MembershipConfig,
};
use icn_governance_actor::{GovernanceStateStore, SledGovernanceStateStore};
use icn_store::{CompareAndPut, SledStore, Store};

const KEY: &[u8] = b"gov:domain:d";

fn legacy() -> GovernanceDomain {
    let mut config = GovernanceConfig::cooperative_default();
    config.membership = MembershipConfig::static_list(vec![]);
    GovernanceDomain::with_id(GovernanceDomainId::new("d"), "legacy-d".into(), config)
}

fn subject() -> GovernanceDomain {
    legacy()
        .with_human_actor_model(HumanActorModel::ContextSubjectV1)
        .unwrap()
}

/// A typed store over a raw handle that the "attacker" also holds (as any code
/// handed the same `Arc<dyn Store>` would).
fn fixture() -> (Arc<dyn Store>, SledGovernanceStateStore) {
    let raw: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
    let typed = SledGovernanceStateStore::new(raw.clone());
    typed.save_domain(&subject()).unwrap();
    (raw, typed)
}

fn show(label: &str, bytes: Option<Vec<u8>>) -> Option<Vec<u8>> {
    match &bytes {
        Some(b) => eprintln!("  {label}: {}", String::from_utf8_lossy(b)),
        None => eprintln!("  {label}: <absent>"),
    }
    bytes
}

fn model(bytes: &[u8]) -> HumanActorModel {
    serde_json::from_slice::<GovernanceDomain>(bytes)
        .unwrap()
        .human_actor_model()
}

/// 1. CONTROL. The typed path refuses Subject -> legacy and leaves the record
///    byte-identical.
#[test]
fn probe_1_typed_downgrade_is_refused() {
    eprintln!("\n[probe 1] typed Subject -> legacy");
    let (raw, typed) = fixture();
    let before = show("before", raw.get(KEY).unwrap()).unwrap();
    let err = typed.save_domain(&legacy()).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<ActorModelError>(),
        Some(ActorModelError::ModeChangeRefused { .. })
    ));
    let after = show("after ", raw.get(KEY).unwrap()).unwrap();
    assert_eq!(before, after);
    eprintln!("  outcome: REFUSED ({err})");
}

/// 2. A raw `put` of legacy bytes replaces the Subject record. Nothing in the
///    `Store` trait knows the key is protected.
#[test]
fn probe_2_raw_put_bypasses_the_invariant() {
    eprintln!("\n[probe 2] raw Store::put of legacy bytes over a Subject record");
    let (raw, _typed) = fixture();
    let before = show("before", raw.get(KEY).unwrap()).unwrap();
    assert_eq!(model(&before), HumanActorModel::ContextSubjectV1);
    raw.put(KEY, &serde_json::to_vec(&legacy()).unwrap())
        .unwrap();
    let after = show("after ", raw.get(KEY).unwrap()).unwrap();
    assert_eq!(model(&after), HumanActorModel::LegacyPrincipalV0);
    eprintln!("  outcome: BYPASSED (Subject -> legacy through a raw handle)");
}

/// 3. Raw delete followed by a typed legacy save: the typed store sees an
///    absent key (a "create"), so no history is consulted.
#[test]
fn probe_3_raw_delete_then_typed_recreate_evades_history() {
    eprintln!("\n[probe 3] raw delete, then typed legacy create");
    let (raw, typed) = fixture();
    show("before", raw.get(KEY).unwrap());
    raw.delete(KEY).unwrap();
    show("after raw delete", raw.get(KEY).unwrap());
    typed.save_domain(&legacy()).unwrap();
    let after = show("after typed create", raw.get(KEY).unwrap()).unwrap();
    assert_eq!(model(&after), HumanActorModel::LegacyPrincipalV0);
    eprintln!("  outcome: BYPASSED (the typed store keeps no mode history)");
}

/// 4. A raw `compare_and_put` with the exact stored bytes as `expected`
///    succeeds: the primitive is a correct CAS, not an authorization check.
#[test]
fn probe_4_raw_compare_and_put_bypasses_the_invariant() {
    eprintln!("\n[probe 4] raw Store::compare_and_put(expected = stored bytes)");
    let (raw, _typed) = fixture();
    let before = show("before", raw.get(KEY).unwrap()).unwrap();
    let r = raw
        .compare_and_put(KEY, Some(&before), &serde_json::to_vec(&legacy()).unwrap())
        .unwrap();
    assert_eq!(r, CompareAndPut::Applied);
    let after = show("after ", raw.get(KEY).unwrap()).unwrap();
    assert_eq!(model(&after), HumanActorModel::LegacyPrincipalV0);
    eprintln!("  outcome: BYPASSED");
}

/// 5. `SledStore::import_snapshot` overwrites any key, with no per-key rule.
#[test]
fn probe_5_snapshot_import_replaces_protected_data() {
    eprintln!("\n[probe 5] SledStore::import_snapshot over a Subject record");
    let sled = Arc::new(SledStore::temporary().unwrap());
    let typed = SledGovernanceStateStore::new(sled.clone());
    typed.save_domain(&subject()).unwrap();
    let before = show("before", sled.get(KEY).unwrap()).unwrap();
    assert_eq!(model(&before), HumanActorModel::ContextSubjectV1);
    sled.import_snapshot(&[(KEY.to_vec(), serde_json::to_vec(&legacy()).unwrap())])
        .unwrap();
    let after = show("after ", sled.get(KEY).unwrap()).unwrap();
    assert_eq!(model(&after), HumanActorModel::LegacyPrincipalV0);
    eprintln!("  outcome: BYPASSED (import_snapshot is a public, key-blind overwrite)");
}

/// 6. Single-writer limits: two typed stores over ONE shared handle each pass
///    the typed check independently, and a second sled handle on the same
///    directory is refused by sled's directory lock.
#[test]
fn probe_6_sharing_and_directory_lock() {
    eprintln!("\n[probe 6] shared handle vs second open of the same directory");
    let raw: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
    let a = SledGovernanceStateStore::new(raw.clone());
    let b = SledGovernanceStateStore::new(raw.clone());
    a.save_domain(&subject()).unwrap();
    // Typed writer B is also bound by the invariant (shared handle, same CAS).
    assert!(b.save_domain(&legacy()).is_err());
    eprintln!("  two typed stores on one shared handle: B refused (invariant holds)");

    let dir = tempfile::tempdir().unwrap();
    let _first = SledStore::open(dir.path()).unwrap();
    let second = SledStore::open(dir.path());
    assert!(
        second.is_err(),
        "a second open of the same directory must fail"
    );
    eprintln!(
        "  second SledStore::open(same dir): REFUSED ({})",
        second.err().unwrap()
    );
}
