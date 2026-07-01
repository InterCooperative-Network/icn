//! Integration tests for `icnctl coop entity-unknown-legacy-report` (#2082 lane).
//!
//! End-to-end: seed a cooperative store with a trusted (Activation-provenance)
//! binding, an untrusted `UnknownLegacy` binding (plain `bind_resolved`, no
//! provenance recorded), and an unbound cooperative; run the real binary; and
//! assert the read-only repair-candidate classification — and that the report
//! upgrades nothing, binds nothing, and creates no store.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use icn_coop::{CoopStore, CoopType, Cooperative};
use icn_entity::{project_coop_id, CoopEntityBindingProvenance, CoopEntityMap, SledCoopEntityMap};
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

/// Seed the cooperative store and the canonical map (sharing one Db, as the
/// daemon does) with three rows:
/// - `trusted-coop`: bound with `Activation` provenance (trusted; not a
///   candidate),
/// - `legacy-coop`: bound via plain `bind_resolved` — no provenance recorded, so
///   it reads back `UnknownLegacy` (an untrusted repair candidate),
/// - `unbound-coop`: present in the coop store but never bound (`NotBound`).
///
/// Drops the store/db to release the sled lock for the binary.
fn seed_mixed_bindings(data_dir: &std::path::Path) {
    let store = SledStore::open(coop_store_path(data_dir)).unwrap();
    let db = Arc::new(store.db().clone());
    let coop_store = CoopStore::new(db.clone());
    let map = SledCoopEntityMap::new(db.clone());

    for id in ["trusted-coop", "legacy-coop", "unbound-coop"] {
        let coop = Cooperative::new_with_id(id.to_string(), id.to_string(), CoopType::Worker);
        coop_store.save_cooperative(&coop).unwrap();
    }

    // Trusted: record an accountable Activation provenance.
    let trusted_entity = project_coop_id("trusted-coop").unwrap();
    map.bind_resolved_with_provenance(
        "trusted-coop",
        &trusted_entity,
        CoopEntityBindingProvenance::Activation,
    )
    .unwrap();

    // Untrusted: a plain bind with no provenance reads back as UnknownLegacy.
    let legacy_entity = project_coop_id("legacy-coop").unwrap();
    map.bind_resolved("legacy-coop", &legacy_entity).unwrap();

    // `unbound-coop` is intentionally left unbound.

    db.flush().unwrap();
    // store, db, coop_store, map drop here -> sled lock released for the binary.
}

fn run_report(data_dir: &std::path::Path, json: bool) -> std::process::Output {
    let mut cmd = Command::new(icnctl_bin());
    cmd.env("RUST_LOG", "off")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("coop")
        .arg("entity-unknown-legacy-report");
    if json {
        cmd.arg("--json");
    }
    cmd.output().unwrap()
}

#[test]
fn unknown_legacy_report_json_classifies_trusted_untrusted_and_unbound() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_mixed_bindings(&data_dir);

    let output = run_report(&data_dir, true);
    assert!(
        output.status.success(),
        "command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    assert_eq!(v["total"].as_u64(), Some(3));
    assert_eq!(v["trusted"].as_u64(), Some(1));
    assert_eq!(v["untrusted_provenance"].as_u64(), Some(1));
    assert_eq!(v["not_bound"].as_u64(), Some(1));
    assert_eq!(v["reverse_mismatch"].as_u64(), Some(0));
    assert_eq!(v["malformed_target"].as_u64(), Some(0));
    assert_eq!(v["storage_error"].as_u64(), Some(0));
    assert_eq!(v["repair_candidates"].as_u64(), Some(1));

    let entries = v["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 3);

    let legacy = entries
        .iter()
        .find(|e| e["coop_id"].as_str() == Some("legacy-coop"))
        .expect("legacy-coop entry present");
    assert_eq!(legacy["status"].as_str(), Some("untrusted_provenance"));
    assert_eq!(
        legacy["required_evidence"].as_str(),
        Some("accountable_provenance_attestation")
    );
    // Untrusted, fail-closed — the provenance must be reported as-is, never upgraded.
    assert_eq!(
        legacy["provenance_observed"]["kind"].as_str(),
        Some("UnknownLegacy")
    );

    let trusted = entries
        .iter()
        .find(|e| e["coop_id"].as_str() == Some("trusted-coop"))
        .expect("trusted-coop entry present");
    assert_eq!(trusted["status"].as_str(), Some("trusted"));
    assert_eq!(trusted["required_evidence"].as_str(), Some("none"));
}

#[test]
fn unknown_legacy_report_human_output_is_readable_and_claims_no_authority() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    seed_mixed_bindings(&data_dir);

    let output = run_report(&data_dir, false);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Readable counts.
    assert!(stdout.contains("repair_candidates") || stdout.contains("repair candidates"));
    assert!(stdout.contains("legacy-coop"), "candidate row surfaced");
    // Zero-authority discipline: an explicit disclaimer, and no claim that the
    // report confers authority or production/operator readiness.
    assert!(
        stdout.contains("grants no authority"),
        "must carry the zero-authority disclaimer; got:\n{stdout}"
    );
    let lowered = stdout.to_lowercase();
    for forbidden in ["authoritative", "production ready", "ready for production"] {
        assert!(
            !lowered.contains(forbidden),
            "human output must not claim authority/readiness (found {forbidden:?})"
        );
    }
}

#[test]
fn unknown_legacy_report_missing_store_reports_empty_and_creates_nothing() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    let store_path = coop_store_path(&data_dir);
    assert!(!store_path.exists());

    let output = run_report(&data_dir, true);
    assert!(
        output.status.success(),
        "command failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert_eq!(v["total"].as_u64(), Some(0));
    assert_eq!(v["repair_candidates"].as_u64(), Some(0));

    // Read-only contract: a missing store is reported empty, never materialized.
    assert!(
        !store_path.exists(),
        "the report must not create the cooperative store"
    );
}
