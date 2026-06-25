//! Integration tests for `icnctl coop entity-backfill-surrogates` (#2082 lane,
//! PR6).
//!
//! End-to-end: seed a cooperative store with one mappable id and one default
//! `coop:<uuid>` id, run the real binary, and assert the dry-run/apply behavior,
//! idempotency, collision handling, exit codes, and that the original `coop_id`
//! is preserved byte-for-byte. A mapping grants no authority — these tests only
//! exercise the name binding.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use icn_coop::{CoopStore, CoopType, Cooperative};
use icn_entity::{CoopEntityBindingProvenance, CoopEntityMap, SledCoopEntityMap};
use icn_store::SledStore;
use serde_json::Value;
use tempfile::TempDir;

/// Path to the icnctl binary (respects custom target directories).
fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// Cooperative store path used by the daemon and by `icnctl`:
/// `<data_dir>/store/cooperative`.
fn coop_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("cooperative")
}

/// Seed one mappable coop ("real-coop") and one default `coop:<uuid>` coop;
/// return the default (non-mappable) id. Releases the sled lock on return.
fn seed_two_cooperatives(data_dir: &Path) -> String {
    let store = SledStore::open(coop_store_path(data_dir)).unwrap();
    let db = Arc::new(store.db().clone());
    let coop_store = CoopStore::new(db.clone());

    // Mappable: "real-coop" is already a valid EntityId slug.
    let mappable = Cooperative::new_with_id(
        "real-coop".to_string(),
        "Real Coop".to_string(),
        CoopType::Worker,
    );
    coop_store.save_cooperative(&mappable).unwrap();

    // Default: Cooperative::new() generates `coop:<uuid>` (non-mappable).
    let default_coop = Cooperative::new("Default Coop".to_string(), CoopType::Worker);
    let default_id = default_coop.id.clone();
    coop_store.save_cooperative(&default_coop).unwrap();

    db.flush().unwrap();
    // store, db, coop_store drop here -> sled lock released for the binary.
    default_id
}

/// Occupy the surrogate target of `coop_id` with a DIFFERENT cooperative, so a
/// later backfill of `coop_id` would collide on bind. Releases the lock on
/// return.
fn occupy_surrogate_target(data_dir: &Path, coop_id: &str, by: &str) {
    let store = SledStore::open(coop_store_path(data_dir)).unwrap();
    let db = Arc::new(store.db().clone());
    let map = SledCoopEntityMap::new(db.clone());
    let surrogate = icn_entity::propose_surrogate_entity_id(coop_id).unwrap();
    map.bind_exact(by, &surrogate).unwrap();
    db.flush().unwrap();
}

/// Re-open the canonical map (read-only inspection) after the binary has run.
fn open_map(data_dir: &Path) -> (SledStore, SledCoopEntityMap) {
    let store = SledStore::open(coop_store_path(data_dir)).unwrap();
    let db = Arc::new(store.db().clone());
    let map = SledCoopEntityMap::new(db);
    (store, map)
}

/// Run `icnctl coop entity-backfill-surrogates <args...>`.
fn run(data_dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(icnctl_bin());
    cmd.env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("coop")
        .arg("entity-backfill-surrogates");
    for a in args {
        cmd.arg(a);
    }
    cmd.output().unwrap()
}

fn parse_json(output: &Output) -> Value {
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON")
}

// ----------------------------------------------------------------------------
// Dry-run is the default and writes nothing.
// ----------------------------------------------------------------------------

#[test]
fn dry_run_is_default_reports_surrogate_and_writes_nothing() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);

    // No --apply -> dry run by default.
    let output = run(&data_dir, &["--json"]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v = parse_json(&output);
    assert_eq!(v["mode"].as_str(), Some("dry_run"));
    assert_eq!(v["total"].as_u64(), Some(2));
    assert_eq!(v["eligible"].as_u64(), Some(1)); // only the default coop:<uuid>
    assert_eq!(v["applied"].as_u64(), Some(0));
    assert_eq!(v["skipped_mappable_unbound"].as_u64(), Some(1)); // real-coop

    let expected = icn_entity::propose_surrogate_entity_id(&default_id)
        .unwrap()
        .as_str()
        .to_string();
    let entries = v["entries"].as_array().unwrap();
    let def = entries
        .iter()
        .find(|e| e["coop_id"].as_str() == Some(default_id.as_str()))
        .expect("default coop entry present");
    assert_eq!(def["action"].as_str(), Some("would_bind"));
    assert_eq!(def["proposed_entity_id"].as_str(), Some(expected.as_str()));
    assert!(def["reason"].as_str().is_some());

    // Read-only: nothing bound.
    let (_store, map) = open_map(&data_dir);
    assert_eq!(map.entity_for_coop(&default_id).unwrap(), None);
}

#[test]
fn human_output_is_readable() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_cooperatives(&data_dir);

    let output = run(&data_dir, &[]); // dry-run default, human output
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("coop surrogate backfill"));
    assert!(stdout.contains("dry-run"));
    assert!(stdout.contains("eligible:"));
    assert!(stdout.contains("would_bind"));
}

// ----------------------------------------------------------------------------
// Apply binds the deterministic surrogate, preserving the original coop_id.
// ----------------------------------------------------------------------------

#[test]
fn apply_binds_default_coop_to_deterministic_surrogate_and_preserves_coop_id() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);

    let output = run(&data_dir, &["--apply", "--json"]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v = parse_json(&output);
    assert_eq!(v["mode"].as_str(), Some("apply"));
    assert_eq!(v["applied"].as_u64(), Some(1));
    assert_eq!(v["bind_conflict"].as_u64(), Some(0));
    assert_eq!(v["bind_error"].as_u64(), Some(0));

    let surrogate = icn_entity::propose_surrogate_entity_id(&default_id).unwrap();
    let (_store, map) = open_map(&data_dir);
    // Forward binds to the surrogate...
    assert_eq!(
        map.entity_for_coop(&default_id).unwrap(),
        Some(surrogate.clone())
    );
    // ...and the reverse preserves the original coop_id byte-for-byte.
    assert_eq!(
        map.coop_for_entity(&surrogate).unwrap(),
        Some(default_id.clone())
    );
    // The mappable coop was NOT bound by the surrogate path.
    assert_eq!(map.entity_for_coop("real-coop").unwrap(), None);
}

#[test]
fn apply_records_operator_backfill_provenance_durably() {
    // End-to-end: after `--apply`, the durable Sled binding carries the
    // OperatorBackfill provenance (not the fail-closed UnknownLegacy a plain
    // bind_resolved would leave), so a later store-backed resolver could trust it.
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);

    assert!(run(&data_dir, &["--apply", "--json"]).status.success());

    let surrogate = icn_entity::propose_surrogate_entity_id(&default_id).unwrap();
    let (_store, map) = open_map(&data_dir);
    let binding = map
        .binding_for_coop(&default_id)
        .unwrap()
        .expect("default coop is now bound");
    assert_eq!(binding.coop_id, default_id);
    assert_eq!(binding.entity_id, surrogate);
    assert_eq!(
        binding.provenance,
        CoopEntityBindingProvenance::OperatorBackfill
    );
}

#[test]
fn apply_is_idempotent_when_rerun() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_cooperatives(&data_dir);

    let first = run(&data_dir, &["--apply", "--json"]);
    assert!(first.status.success());
    assert_eq!(parse_json(&first)["applied"].as_u64(), Some(1));

    // Rerun: the default coop is now already-bound, so it is skipped.
    let second = run(&data_dir, &["--apply", "--json"]);
    assert!(second.status.success());
    let v = parse_json(&second);
    assert_eq!(v["applied"].as_u64(), Some(0));
    assert_eq!(v["skipped_already_bound"].as_u64(), Some(1));
    assert_eq!(v["eligible"].as_u64(), Some(0));
}

#[test]
fn dry_run_skips_already_bound_after_apply() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_cooperatives(&data_dir);

    assert!(run(&data_dir, &["--apply", "--json"]).status.success());

    // Explicit --dry-run flag (the default) must also skip the now-bound coop.
    let output = run(&data_dir, &["--dry-run", "--json"]);
    assert!(output.status.success());
    let v = parse_json(&output);
    assert_eq!(v["mode"].as_str(), Some("dry_run"));
    assert_eq!(v["skipped_already_bound"].as_u64(), Some(1));
    assert_eq!(v["eligible"].as_u64(), Some(0));
}

// ----------------------------------------------------------------------------
// Collisions: reported, not applied, non-zero exit under --apply.
// ----------------------------------------------------------------------------

#[test]
fn apply_surrogate_collision_is_reported_not_applied_and_exits_nonzero() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);
    // A different coop already occupies the default coop's surrogate target.
    occupy_surrogate_target(&data_dir, &default_id, "alpha-coop");

    let output = run(&data_dir, &["--apply", "--json"]);
    // A collision during --apply means the run did not fully complete: non-zero.
    assert!(
        !output.status.success(),
        "apply with a refused collision must exit non-zero"
    );
    // The JSON report is still emitted on stdout, ahead of the error.
    let v = parse_json(&output);
    assert_eq!(v["surrogate_collision"].as_u64(), Some(1));
    assert_eq!(v["applied"].as_u64(), Some(0));
    let entries = v["entries"].as_array().unwrap();
    let def = entries
        .iter()
        .find(|e| e["coop_id"].as_str() == Some(default_id.as_str()))
        .unwrap();
    assert_eq!(def["action"].as_str(), Some("surrogate_collision"));

    // The default coop is still unbound; its surrogate stays with alpha-coop.
    let surrogate = icn_entity::propose_surrogate_entity_id(&default_id).unwrap();
    let (_store, map) = open_map(&data_dir);
    assert_eq!(map.entity_for_coop(&default_id).unwrap(), None);
    assert_eq!(
        map.coop_for_entity(&surrogate).unwrap(),
        Some("alpha-coop".to_string())
    );
}

#[test]
fn dry_run_reports_collision_without_failing() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);
    occupy_surrogate_target(&data_dir, &default_id, "alpha-coop");

    // A dry run is a preview: collisions are findings, not failures -> exit 0.
    let output = run(&data_dir, &["--json"]);
    assert!(output.status.success());
    let v = parse_json(&output);
    assert_eq!(v["mode"].as_str(), Some("dry_run"));
    assert_eq!(v["surrogate_collision"].as_u64(), Some(1));
    assert_eq!(v["applied"].as_u64(), Some(0));
}

// ----------------------------------------------------------------------------
// Flag handling and empty/missing store.
// ----------------------------------------------------------------------------

#[test]
fn apply_and_dry_run_are_mutually_exclusive() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);

    let output = run(&data_dir, &["--apply", "--dry-run"]);
    assert!(
        !output.status.success(),
        "clap must reject --apply with --dry-run"
    );
    // Nothing was bound (clap errors before the handler runs).
    let (_store, map) = open_map(&data_dir);
    assert_eq!(map.entity_for_coop(&default_id).unwrap(), None);
}

#[test]
fn missing_store_reports_empty_and_creates_nothing_even_with_apply() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data"); // never created

    let output = run(&data_dir, &["--apply", "--json"]);
    assert!(output.status.success());
    let v = parse_json(&output);
    assert_eq!(v["total"].as_u64(), Some(0));
    assert_eq!(v["applied"].as_u64(), Some(0));

    // A read/apply over a missing store must not create a database.
    assert!(
        !coop_store_path(&data_dir).exists(),
        "backfill must not create a store when none exists"
    );
}

// ----------------------------------------------------------------------------
// The PR3/PR4 report still works after an apply.
// ----------------------------------------------------------------------------

#[test]
fn entity_report_preview_surrogates_still_works_after_apply() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);

    assert!(run(&data_dir, &["--apply", "--json"]).status.success());

    // entity-report --json --preview-surrogates still runs, and now classifies
    // the default coop as already-bound (no surrogate proposed for it).
    let output = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .arg("--json")
        .arg("--preview-surrogates")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_str(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    assert_eq!(v["already_bound"].as_u64(), Some(1));
    assert_eq!(v["non_mappable"].as_u64(), Some(0));
    // No non-mappable ids remain, so no surrogate is proposed.
    assert_eq!(v["surrogate_proposed"].as_u64().unwrap_or(0), 0);

    let entries = v["entries"].as_array().unwrap();
    let def = entries
        .iter()
        .find(|e| e["coop_id"].as_str() == Some(default_id.as_str()))
        .unwrap();
    assert_eq!(def["class"].as_str(), Some("already_bound"));
}
