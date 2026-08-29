//! `--json` mode must emit parseable JSON for any legal filesystem path.
//!
//! Driven through the real binary rather than a library call, because the
//! defect this guards against lived in the output formatting: a report whose
//! fields were fine but whose rendering produced invalid JSON the moment a path
//! contained a quote, a backslash or a newline.

// Test-only: assertions and fixture setup panic on failure by design, as in
// this crate's other integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

/// Create an empty sled store at `dir` by letting sled initialise it.
fn make_store(dir: &Path) {
    let db = sled::open(dir).expect("open scratch store");
    db.flush().expect("flush");
}

fn scan_json(store: &Path) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_did-collision-scan"))
        .arg(store)
        .arg("--json")
        .output()
        .expect("run did-collision-scan");

    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "--json emitted invalid JSON for {}: {e}\n{stdout}",
            store.display()
        )
    })
}

#[test]
fn json_output_is_valid_for_paths_containing_json_special_characters() {
    let base = tempfile::tempdir().expect("tempdir");

    // Each name is legal on Linux and hostile to hand-rolled JSON.
    for name in [
        r#"quote"in-name"#,
        r#"back\slash"#,
        "line\nbreak",
        "ünïcode-Ω-😀",
        "plain",
    ] {
        let dir = base.path().join(name);
        std::fs::create_dir_all(&dir).expect("create store dir");
        make_store(&dir);

        let doc = scan_json(&dir);

        // Parsed successfully; now confirm the path round-tripped intact
        // rather than being mangled into something that merely parses.
        let store = &doc["stores"][0];
        assert_eq!(
            store["store"].as_str().expect("store is a string"),
            dir.display().to_string(),
            "path must survive encoding for {name:?}"
        );
        assert!(doc["clear"].is_boolean());
        assert!(store["keyspaces"].is_array());
    }
}

/// Multiple stores must produce ONE document, not one per path.
///
/// Each per-store object parsed on its own before this, so the whole stdout
/// looked fine line by line while being invalid JSON as a whole — exactly the
/// shape of defect a single-path test cannot see.
#[test]
fn multi_store_json_is_one_valid_document() {
    let base = tempfile::tempdir().expect("tempdir");
    let mut dirs = Vec::new();
    for name in ["one", "two", "three"] {
        let dir = base.path().join(name);
        std::fs::create_dir_all(&dir).expect("create");
        make_store(&dir);
        dirs.push(dir);
    }

    let out = Command::new(env!("CARGO_BIN_EXE_did-collision-scan"))
        .args(&dirs)
        .arg("--json")
        .output()
        .expect("run");

    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let doc: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("multi-store --json must be one document: {e}\n{stdout}"));

    let stores = doc["stores"].as_array().expect("stores is an array");
    assert_eq!(stores.len(), 3, "every scanned store must appear once");
    assert!(doc["clear"].is_boolean(), "run-level verdict present");
}

/// A path one level above the databases must fail, not report CLEAR.
///
/// `sled::open` creates a database when the directory is not one, so pointing
/// the scan at `/data` — which is exactly what the documented `kubectl cp`
/// produces — would otherwise yield an empty database in the scratch copy, zero
/// rows, and exit 0 without ever looking at the stores underneath.
#[test]
fn a_path_above_the_databases_is_rejected_not_reported_clear() {
    let base = tempfile::tempdir().expect("tempdir");
    let parent = base.path().join("data");
    let nested = parent.join("store").join("ledger");
    std::fs::create_dir_all(&nested).expect("create");
    make_store(&nested);

    let out = Command::new(env!("CARGO_BIN_EXE_did-collision-scan"))
        .arg(&parent)
        .output()
        .expect("run");

    assert!(!out.status.success(), "a wrong-level path must not exit 0");
    let err = String::from_utf8(out.stderr).expect("utf-8");
    assert!(
        err.contains("not a sled database"),
        "the error must say why: {err}"
    );
    assert!(
        err.contains("ledger"),
        "the error must name the database to scan instead: {err}"
    );
}

/// An empty directory with no database anywhere beneath it is also refused.
#[test]
fn a_directory_with_no_database_is_rejected() {
    let base = tempfile::tempdir().expect("tempdir");
    let empty = base.path().join("empty");
    std::fs::create_dir_all(&empty).expect("create");

    let out = Command::new(env!("CARGO_BIN_EXE_did-collision-scan"))
        .arg(&empty)
        .output()
        .expect("run");

    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).expect("utf-8");
    assert!(err.contains("no database was found"), "{err}");
}

/// A symlinked store must be refused, not silently copied as an empty one.
///
/// `ensure_sled_root` follows links when it checks for `conf`, so a source
/// represented as a symlink farm — a backup, a restored snapshot — passed
/// validation while the copy skipped every linked artifact. `SledStore::open`
/// then initialised a fresh empty database and the gate reported CLEAR having
/// scanned nothing.
#[test]
fn a_symlinked_store_artifact_is_refused_not_skipped() {
    let base = tempfile::tempdir().expect("tempdir");
    let real = base.path().join("real");
    std::fs::create_dir_all(&real).expect("create");
    make_store(&real);

    let farm = base.path().join("farm");
    std::fs::create_dir_all(&farm).expect("create");
    for entry in std::fs::read_dir(&real).expect("read real") {
        let entry = entry.expect("entry");
        if entry.file_type().expect("ft").is_file() {
            std::os::unix::fs::symlink(entry.path(), farm.join(entry.file_name()))
                .expect("symlink");
        }
    }

    // The farm looks like a database: `conf` resolves through the link.
    assert!(farm.join("conf").is_file());

    let out = Command::new(env!("CARGO_BIN_EXE_did-collision-scan"))
        .arg(&farm)
        .output()
        .expect("run");

    assert!(
        !out.status.success(),
        "a symlinked store must not produce a CLEAR verdict"
    );
    let err = String::from_utf8(out.stderr).expect("utf-8");
    assert!(err.contains("symlink"), "the error must say why: {err}");
}

#[test]
fn json_verdict_agrees_with_the_process_exit_status() {
    // The human text, the JSON verdict and the exit code are three renderings
    // of one decision. A gate whose exit status disagreed with its report would
    // be worse than one that simply failed.
    let base = tempfile::tempdir().expect("tempdir");
    let dir = base.path().join("store");
    std::fs::create_dir_all(&dir).expect("create store dir");
    make_store(&dir);

    let out = Command::new(env!("CARGO_BIN_EXE_did-collision-scan"))
        .arg(&dir)
        .arg("--json")
        .output()
        .expect("run");
    let doc: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).expect("valid json");

    let clear = doc["clear"].as_bool().expect("clear is a bool");
    assert_eq!(
        clear,
        out.status.success(),
        "JSON verdict and exit status must agree"
    );
}
