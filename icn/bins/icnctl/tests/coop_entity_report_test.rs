//! Integration tests for `icnctl coop entity-report` (#2082 lane, PR3).
//!
//! End-to-end: seed a cooperative store with one mappable id and one default
//! `coop:<uuid>` id, run the real binary, and assert the read-only inventory
//! classification — and that the report binds nothing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use icn_coop::{CoopStore, CoopType, Cooperative};
use icn_entity::{CoopEntityMap, SledCoopEntityMap};
use icn_store::SledStore;
use serde_json::Value;
use tempfile::TempDir;

/// Path to the icnctl binary (respects custom target directories).
fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// Cooperative store path used by the daemon and by `icnctl`:
/// `<data_dir>/store/cooperative`.
fn coop_store_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("store").join("cooperative")
}

/// Seed the cooperative store with one mappable coop and one default
/// `coop:<uuid>` coop, then release the sled lock by dropping the store.
fn seed_two_cooperatives(data_dir: &std::path::Path) -> String {
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

#[test]
fn entity_report_json_classifies_mappable_and_default_ids() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");

    let default_id = seed_two_cooperatives(&data_dir);
    assert!(
        default_id.starts_with("coop:"),
        "default id should be coop:<uuid>"
    );

    // Run the real binary; RUST_LOG=off keeps stdout pure JSON.
    let output = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .arg("--json")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    assert_eq!(v["total"].as_u64(), Some(2));
    assert_eq!(v["mappable_unbound"].as_u64(), Some(1));
    assert_eq!(v["non_mappable"].as_u64(), Some(1));
    assert_eq!(v["already_bound"].as_u64(), Some(0));
    assert_eq!(v["mappable_reverse_conflict"].as_u64(), Some(0));
    assert_eq!(v["storage_error"].as_u64(), Some(0));

    // Detail entries are present and preserve the original ids.
    let entries = v["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 2);
    let ids: Vec<&str> = entries
        .iter()
        .map(|e| e["coop_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"real-coop"));
    assert!(ids.contains(&default_id.as_str()));
}

#[test]
fn entity_report_human_output_includes_counts() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_cooperatives(&data_dir);

    let output = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("total:"));
    assert!(stdout.contains("non_mappable:"));
    assert!(stdout.contains("mappable_unbound:"));
}

#[test]
fn entity_report_writes_no_bindings() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_cooperatives(&data_dir);

    let status = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .arg("--json")
        .status()
        .unwrap();
    assert!(status.success());

    // Re-open the map: the report must not have created any binding.
    let store = SledStore::open(coop_store_path(&data_dir)).unwrap();
    let db = Arc::new(store.db().clone());
    let map = SledCoopEntityMap::new(db);
    assert_eq!(
        map.entity_for_coop("real-coop").unwrap(),
        None,
        "read-only report must not bind the mappable coop"
    );
}

#[test]
fn entity_report_on_missing_store_reports_empty() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data"); // never created

    let output = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
    assert_eq!(v["total"].as_u64(), Some(0));

    // The read-only command must not have created the store directory.
    assert!(
        !coop_store_path(&data_dir).exists(),
        "report must not create a database when none exists"
    );
}

// ----------------------------------------------------------------------------
// Surrogate preview (#2082 PR4): --preview-surrogates is read-only and additive.
// ----------------------------------------------------------------------------

/// Without the flag, the JSON shape is exactly the pre-surrogate report: no
/// `surrogate_*` aggregates and no per-entry `proposed_surrogate_entity_id`.
#[test]
fn default_json_omits_surrogate_fields() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_two_cooperatives(&data_dir);

    let output = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).unwrap();

    assert!(v.get("surrogate_proposed").is_none());
    assert!(v.get("surrogate_collision").is_none());
    for entry in v["entries"].as_array().unwrap() {
        assert!(
            entry.get("proposed_surrogate_entity_id").is_none(),
            "default report must not include surrogate proposals"
        );
    }
}

/// With `--preview-surrogates --json`, the non-mappable default `coop:<uuid>`
/// carries its deterministic proposed surrogate; the mappable coop does not.
#[test]
fn preview_json_includes_proposed_surrogate_for_default_id() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);

    let output = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .arg("--preview-surrogates")
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).unwrap();

    assert_eq!(v["surrogate_proposed"].as_u64(), Some(1));
    // `surrogate_collision` is omitted when zero (skip_serializing_if), so an
    // absent key means "no collisions" — treat absent as 0.
    assert_eq!(v["surrogate_collision"].as_u64().unwrap_or(0), 0);

    let expected = icn_entity::propose_surrogate_entity_id(&default_id)
        .unwrap()
        .as_str()
        .to_string();

    let entries = v["entries"].as_array().unwrap();
    let default_entry = entries
        .iter()
        .find(|e| e["coop_id"].as_str() == Some(default_id.as_str()))
        .expect("default coop entry present");
    assert_eq!(default_entry["class"].as_str(), Some("non_mappable"));
    assert_eq!(
        default_entry["proposed_surrogate_entity_id"].as_str(),
        Some(expected.as_str())
    );

    let mappable_entry = entries
        .iter()
        .find(|e| e["coop_id"].as_str() == Some("real-coop"))
        .expect("mappable coop entry present");
    assert!(
        mappable_entry.get("proposed_surrogate_entity_id").is_none(),
        "mappable coop must not get a surrogate proposal"
    );
}

/// Human output under `--preview-surrogates` shows the surrogate count and the
/// proposed value.
#[test]
fn preview_human_output_includes_surrogate_count_and_value() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);

    let output = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .arg("--preview-surrogates")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("surrogate_proposed:"));
    let expected = icn_entity::propose_surrogate_entity_id(&default_id)
        .unwrap()
        .as_str()
        .to_string();
    assert!(
        stdout.contains("proposed surrogate:") && stdout.contains(&expected),
        "human preview output missing surrogate value; got:\n{stdout}"
    );
}

/// The preview path is read-only: it must not bind the non-mappable id, its
/// proposed surrogate target, or the mappable id.
#[test]
fn preview_writes_no_bindings() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let default_id = seed_two_cooperatives(&data_dir);

    let status = Command::new(icnctl_bin())
        .env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("coop")
        .arg("entity-report")
        .arg("--preview-surrogates")
        .arg("--json")
        .status()
        .unwrap();
    assert!(status.success());

    let store = SledStore::open(coop_store_path(&data_dir)).unwrap();
    let db = Arc::new(store.db().clone());
    let map = SledCoopEntityMap::new(db);
    assert_eq!(map.entity_for_coop("real-coop").unwrap(), None);
    assert_eq!(map.entity_for_coop(&default_id).unwrap(), None);
    let surrogate = icn_entity::propose_surrogate_entity_id(&default_id).unwrap();
    assert_eq!(map.coop_for_entity(&surrogate).unwrap(), None);
}
