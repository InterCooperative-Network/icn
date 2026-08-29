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
        assert_eq!(
            doc["store"].as_str().expect("store is a string"),
            dir.display().to_string(),
            "path must survive encoding for {name:?}"
        );
        assert!(doc["clear"].is_boolean());
        assert!(doc["keyspaces"].is_array());
    }
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
