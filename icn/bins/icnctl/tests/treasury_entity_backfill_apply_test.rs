//! Integration tests for `icnctl treasury entity-backfill-apply` (ADR-0084, #2082
//! lane).
//!
//! End-to-end: seed the ledger store with one legacy treasury whose `coop_id`
//! has a trusted cooperative binding (would-populate) and one with no binding
//! (skipped-no-mapping), run the real binary, and assert the controlled apply
//! contract:
//! - dry-run is the default and writes nothing;
//! - `--apply` alone refuses to mutate when rows would be populated (a second
//!   `--confirm-apply` is required);
//! - a confirmed apply populates **only** the eligible row, preserves `coop_id`,
//!   never writes the map, and is idempotent on re-run;
//! - a missing store is reported, never created (apply included).
//!
//! `entries` order is not deterministic (`list_treasuries` iterates a `HashMap`),
//! so assertions compare counters and look treasuries up by `coop_id`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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

/// Ledger store path: `<data_dir>/store/ledger`.
fn ledger_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("ledger")
}

/// Cooperative store path: `<data_dir>/store/cooperative`.
fn coop_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("cooperative")
}

/// Seed two legacy treasuries (`entity_id: None`): `food-coop` (eligible once a
/// trusted binding exists) and `no-map-coop` (no binding → skipped). Releases the
/// sled lock on return.
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

/// Run `icnctl --data-dir <dir> treasury entity-backfill-apply [--apply]
/// [--confirm-apply] [--json]`.
fn run_apply(data_dir: &Path, apply: bool, confirm: bool, json: bool) -> Output {
    let mut cmd = Command::new(icnctl_bin());
    cmd.env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("treasury")
        .arg("entity-backfill-apply");
    if apply {
        cmd.arg("--apply");
    }
    if confirm {
        cmd.arg("--confirm-apply");
    }
    if json {
        cmd.arg("--json");
    }
    cmd.output().unwrap()
}

/// Re-open the ledger store and return the persisted `entity_id` of the treasury
/// for `coop_id` (the treasury must exist). Proves both persistence and the
/// byte-for-byte `coop_id` (the lookup is by exact `coop_id`).
fn persisted_entity_id(data_dir: &Path, coop_id: &str) -> Option<EntityId> {
    let sled = Arc::new(SledStore::open(ledger_store_path(data_dir)).unwrap());
    let store: Arc<dyn Store> = sled.clone();
    let mgr = TreasuryManager::with_store(store).unwrap();
    mgr.list_treasuries()
        .into_iter()
        .find(|t| t.coop_id == coop_id)
        .expect("treasury persists")
        .entity_id()
        .cloned()
}

#[test]
fn apply_dry_run_is_the_default_and_writes_nothing() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_legacy_treasuries(&data_dir);
    seed_trusted_binding(&data_dir);

    let output = run_apply(&data_dir, false, false, true);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["mode"].as_str(), Some("dry-run"));
    assert_eq!(v["plan"]["would_populate"].as_u64(), Some(1));

    // Default writes nothing: no entity_id was populated.
    assert_eq!(persisted_entity_id(&data_dir, "food-coop"), None);
}

#[test]
fn apply_requires_confirmation_when_rows_would_populate() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_legacy_treasuries(&data_dir);
    seed_trusted_binding(&data_dir);

    // --apply WITHOUT --confirm-apply must refuse to mutate (non-zero exit).
    let output = run_apply(&data_dir, true, false, true);
    assert!(
        !output.status.success(),
        "apply without --confirm-apply must fail closed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--confirm-apply"),
        "stderr must instruct the operator to pass --confirm-apply: {stderr}"
    );

    // Fail closed: nothing was populated.
    assert_eq!(persisted_entity_id(&data_dir, "food-coop"), None);
}

#[test]
fn apply_with_confirmation_populates_only_eligible_rows() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_legacy_treasuries(&data_dir);
    seed_trusted_binding(&data_dir);

    let output = run_apply(&data_dir, true, true, true);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["mode"].as_str(), Some("apply"));
    assert_eq!(v["report"]["applied"].as_u64(), Some(1));
    assert_eq!(v["report"]["failed"].as_u64(), Some(0));
    // The embedded plan carries the full classification.
    assert_eq!(v["report"]["plan"]["would_populate"].as_u64(), Some(1));

    // The eligible row is populated + persisted; the unmapped row is untouched.
    assert_eq!(
        persisted_entity_id(&data_dir, "food-coop"),
        Some(EntityId::cooperative("food-coop").unwrap())
    );
    assert_eq!(persisted_entity_id(&data_dir, "no-map-coop"), None);

    // The map is untouched (apply never writes the coop_id <-> EntityId map).
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
        "apply must not bind the unmapped coop"
    );
}

#[test]
fn apply_is_idempotent_across_runs() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_legacy_treasuries(&data_dir);
    seed_trusted_binding(&data_dir);

    let first = run_apply(&data_dir, true, true, true);
    assert!(first.status.success());
    let v1: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(v1["report"]["applied"].as_u64(), Some(1));

    // Re-run: the row now classifies skipped_already_has_entity_id; a no-op.
    let second = run_apply(&data_dir, true, true, true);
    assert!(second.status.success());
    let v2: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(v2["report"]["applied"].as_u64(), Some(0));
    assert_eq!(v2["report"]["failed"].as_u64(), Some(0));
    assert_eq!(
        v2["report"]["plan"]["skipped_already_has_entity_id"].as_u64(),
        Some(1)
    );

    assert_eq!(
        persisted_entity_id(&data_dir, "food-coop"),
        Some(EntityId::cooperative("food-coop").unwrap())
    );
}

#[test]
fn apply_with_no_eligible_rows_needs_no_confirmation() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    // Treasuries exist but there is NO cooperative map -> would_populate == 0.
    seed_two_legacy_treasuries(&data_dir);
    assert!(!coop_store_path(&data_dir).exists());

    // --apply without --confirm-apply: nothing would be populated, so no second
    // confirmation is required and the command succeeds as a no-op.
    let output = run_apply(&data_dir, true, false, true);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["mode"].as_str(), Some("apply"));
    assert_eq!(v["report"]["applied"].as_u64(), Some(0));
    // Persisted treasuries must NOT be hidden behind total: 0.
    assert_eq!(v["report"]["plan"]["total"].as_u64(), Some(2));
    assert_eq!(v["report"]["plan"]["skipped_no_mapping"].as_u64(), Some(2));

    // No cooperative store was materialized.
    assert!(
        !coop_store_path(&data_dir).exists(),
        "apply must not create the cooperative store"
    );
}

#[test]
fn apply_on_missing_ledger_store_creates_nothing() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data"); // never created

    let output = run_apply(&data_dir, true, true, true);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["plan"]["total"].as_u64(), Some(0));

    // Even with --apply --confirm-apply, a missing store must not be created.
    assert!(
        !ledger_store_path(&data_dir).exists(),
        "apply on a missing store must not materialize the ledger store"
    );
}
