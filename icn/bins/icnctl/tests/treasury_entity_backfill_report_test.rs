//! Integration tests for `icnctl treasury entity-backfill-report` (#2082 lane).
//!
//! End-to-end: seed the ledger store with one legacy treasury whose `coop_id`
//! has a trusted cooperative binding (would-populate) and one with no binding
//! (skipped-no-mapping), run the real binary, and assert the read-only plan — and
//! that the report writes nothing, populates no `entity_id`, and creates no store.
//!
//! `entries` order is not deterministic (`list_treasuries` iterates a `HashMap`),
//! so assertions search entries by `action` and compare counters, never fixed
//! indices or raw stdout.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use icn_entity::{CoopEntityBindingProvenance, CoopEntityMap, EntityId, SledCoopEntityMap};
use icn_identity::KeyPair;
use icn_ledger::TreasuryManager;
use icn_store::{SledStore, Store};
use serde_json::Value;
use tempfile::TempDir;

/// Path to the icnctl binary (respects custom target directories).
fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// Ledger store path used by the daemon and by `icnctl`: `<data_dir>/store/ledger`.
fn ledger_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("ledger")
}

/// Cooperative store path: `<data_dir>/store/cooperative`.
fn coop_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("cooperative")
}

/// Seed the ledger store with two legacy treasuries (`entity_id: None`): one for
/// `food-coop` (will be eligible once a trusted binding exists) and one for
/// `no-map-coop` (no binding → skipped). Releases the sled lock on return.
fn seed_two_legacy_treasuries(data_dir: &Path) {
    let sled = Arc::new(SledStore::open(ledger_store_path(data_dir)).unwrap());
    let store: Arc<dyn Store> = sled.clone();
    let mut mgr = TreasuryManager::with_store(store).unwrap();

    let creator = KeyPair::generate().unwrap().did().clone();
    let food_did = KeyPair::generate().unwrap().did().clone();
    let no_map_did = KeyPair::generate().unwrap().did().clone();

    mgr.register_treasury(
        food_did,
        "food-coop".to_string(),
        "USD".to_string(),
        creator.clone(),
        None,
    )
    .unwrap();
    mgr.register_treasury(
        no_map_did,
        "no-map-coop".to_string(),
        "USD".to_string(),
        creator,
        None,
    )
    .unwrap();

    sled.db().flush().unwrap();
    // mgr, store, sled drop here -> sled lock released for the binary.
}

/// Seed the cooperative store with a trusted activation binding
/// `food-coop -> cooperative:food-coop`. Releases the sled lock on return.
fn seed_trusted_binding(data_dir: &Path) {
    let store = SledStore::open(coop_store_path(data_dir)).unwrap();
    let db = Arc::new(store.db().clone());
    let map = SledCoopEntityMap::new(db.clone());
    map.bind_resolved_with_provenance(
        "food-coop",
        &EntityId::cooperative("food-coop").unwrap(),
        CoopEntityBindingProvenance::Activation,
    )
    .unwrap();
    db.flush().unwrap();
    // store, db, map drop here -> sled lock released for the binary.
}

/// Run `icnctl --data-dir <dir> treasury entity-backfill-report [--json]`.
fn run_report(data_dir: &Path, json: bool) -> std::process::Output {
    let mut cmd = Command::new(icnctl_bin());
    cmd.env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("treasury")
        .arg("entity-backfill-report");
    if json {
        cmd.arg("--json");
    }
    cmd.output().unwrap()
}

/// Find the single entry whose `action` matches, or fail.
fn entry_with_action<'a>(v: &'a Value, action: &str) -> &'a Value {
    v["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .find(|e| e["action"].as_str() == Some(action))
        .unwrap_or_else(|| panic!("no entry with action {action}"))
}

#[test]
fn report_json_classifies_would_populate_and_skip() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");

    seed_two_legacy_treasuries(&data_dir);
    seed_trusted_binding(&data_dir);

    let output = run_report(&data_dir, true);
    assert!(
        output.status.success(),
        "command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    // Counters partition the total.
    assert_eq!(v["total"].as_u64(), Some(2));
    assert_eq!(v["would_populate"].as_u64(), Some(1));
    assert_eq!(v["skipped_no_mapping"].as_u64(), Some(1));
    assert_eq!(v["skipped_already_has_entity_id"].as_u64(), Some(0));
    assert_eq!(v["skipped_untrusted_provenance"].as_u64(), Some(0));
    assert_eq!(v["skipped_non_cooperative_entity"].as_u64(), Some(0));
    assert_eq!(v["skipped_ambiguous_binding"].as_u64(), Some(0));
    assert_eq!(v["skipped_storage_error"].as_u64(), Some(0));
    assert_eq!(v["entries"].as_array().map(|e| e.len()), Some(2));

    // The eligible row is food-coop with a resolved cooperative candidate.
    let would = entry_with_action(&v, "would_populate");
    assert_eq!(would["coop_id"].as_str(), Some("food-coop"));
    assert!(
        !would["resolved_entity_id"].is_null(),
        "would_populate entry must surface its resolved candidate entity_id"
    );

    // The skipped row is the unbound coop.
    let skipped = entry_with_action(&v, "skipped_no_mapping");
    assert_eq!(skipped["coop_id"].as_str(), Some("no-map-coop"));
}

#[test]
fn report_human_output_includes_counts() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");

    seed_two_legacy_treasuries(&data_dir);
    seed_trusted_binding(&data_dir);

    let output = run_report(&data_dir, false);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("total:"));
    assert!(stdout.contains("would_populate:"));
    assert!(stdout.contains("skipped_no_mapping:"));
    assert!(stdout.contains("a mapping grants no authority"));
}

#[test]
fn report_writes_nothing() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");

    seed_two_legacy_treasuries(&data_dir);
    seed_trusted_binding(&data_dir);

    let output = run_report(&data_dir, true);
    assert!(output.status.success());

    // Re-open the ledger store: no treasury may have gained an entity_id.
    let sled = Arc::new(SledStore::open(ledger_store_path(&data_dir)).unwrap());
    let store: Arc<dyn Store> = sled.clone();
    let mgr = TreasuryManager::with_store(store).unwrap();
    let food = mgr
        .list_treasuries()
        .into_iter()
        .find(|t| t.coop_id == "food-coop")
        .expect("food-coop treasury persists");
    assert_eq!(
        food.entity_id, None,
        "read-only report must not populate treasury.entity_id"
    );
    drop(mgr);
    drop(sled);

    // Re-open the map: the binding is unchanged and no new binding was created.
    let coop_store = SledStore::open(coop_store_path(&data_dir)).unwrap();
    let db = Arc::new(coop_store.db().clone());
    let map = SledCoopEntityMap::new(db);
    assert_eq!(
        map.entity_for_coop("food-coop").unwrap(),
        Some(EntityId::cooperative("food-coop").unwrap()),
        "the pre-existing trusted binding must be untouched"
    );
    assert_eq!(
        map.entity_for_coop("no-map-coop").unwrap(),
        None,
        "read-only report must not bind the unmapped coop"
    );
}

#[test]
fn report_on_missing_ledger_store_reports_empty() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data"); // never created

    let output = run_report(&data_dir, true);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
    assert_eq!(v["total"].as_u64(), Some(0));

    // The read-only command must not have materialized the ledger store.
    assert!(
        !ledger_store_path(&data_dir).exists(),
        "missing-store report must not create the ledger store"
    );
}

#[test]
fn report_missing_coop_store_classifies_all_as_no_mapping() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");

    // Ledger has treasuries, but no cooperative map store exists.
    seed_two_legacy_treasuries(&data_dir);
    assert!(!coop_store_path(&data_dir).exists());

    let output = run_report(&data_dir, true);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    // Persisted treasuries must NOT be hidden behind total: 0.
    assert_eq!(v["total"].as_u64(), Some(2));
    assert_eq!(v["skipped_no_mapping"].as_u64(), Some(2));
    assert_eq!(v["would_populate"].as_u64(), Some(0));

    // The command must not have materialized the cooperative store.
    assert!(
        !coop_store_path(&data_dir).exists(),
        "missing cooperative store must not be created by the report"
    );
}

#[test]
fn report_repeat_run_is_stable() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");

    seed_two_legacy_treasuries(&data_dir);
    seed_trusted_binding(&data_dir);

    let first = run_report(&data_dir, true);
    let second = run_report(&data_dir, true);
    assert!(first.status.success() && second.status.success());

    let v1: Value = serde_json::from_slice(&first.stdout).unwrap();
    let v2: Value = serde_json::from_slice(&second.stdout).unwrap();

    // Counters are stable across runs (entry order is not guaranteed).
    for field in [
        "total",
        "would_populate",
        "skipped_no_mapping",
        "skipped_already_has_entity_id",
        "skipped_untrusted_provenance",
        "skipped_non_cooperative_entity",
        "skipped_ambiguous_binding",
        "skipped_storage_error",
    ] {
        assert_eq!(v1[field], v2[field], "counter {field} drifted across runs");
    }
}
