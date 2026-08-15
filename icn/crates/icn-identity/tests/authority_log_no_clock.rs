//! N1 obligation 21 — **no wall clock is read anywhere in the module**.
//!
//! An enforceable guard, not a comment. Authority validation must be a pure function of the
//! durable body set; a receiver clock reintroduces the R4.1/R4.3 divergence that finding F14
//! already produces elsewhere in this crate (`current_timestamp_secs()` returns `0` on clock
//! error, so every expiry check in the identity stack fails **open**).
//!
//! The guard walks `src/authority_log/` at test time rather than using `include_str!`, so a file
//! added to the module later is covered automatically instead of silently escaping the check.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

/// Tokens that would introduce a receiver clock, directly or indirectly.
const FORBIDDEN: &[&str] = &[
    "SystemTime",
    "UNIX_EPOCH",
    "Instant",
    "current_timestamp",
    "try_current_timestamp",
    "icn_time",
    "Utc::now",
    "chrono",
    "Local::now",
    "OffsetDateTime",
    "elapsed(",
    "std::time",
];

fn module_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/authority_log")
}

fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            out.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn module_reads_no_wall_clock() {
    let root = module_root();
    let sources = rust_sources(&root);
    assert!(
        !sources.is_empty(),
        "the guard found no sources under {} — it would pass vacuously",
        root.display()
    );

    let mut violations = Vec::new();
    for path in &sources {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for (index, line) in text.lines().enumerate() {
            for token in FORBIDDEN {
                if line.contains(token) {
                    violations.push(format!(
                        "{}:{}: forbidden clock token `{token}` in `{}`",
                        path.display(),
                        index + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "the authority-log module must not read a receiver clock:\n{}",
        violations.join("\n")
    );
}

/// The guard must actually be able to fail. If this test stops detecting a planted token, the
/// guard above has become vacuous.
#[test]
fn the_guard_can_fail() {
    let planted = "let now = SystemTime::now();";
    let detected = FORBIDDEN.iter().any(|token| planted.contains(token));
    assert!(
        detected,
        "the forbidden-token list no longer detects a clock read"
    );
}

/// `icn-identity` itself is not clock-free — that is the point of scoping the guard to this
/// module. This test documents the boundary so nobody widens the guard by accident and nobody
/// mistakes it for a crate-wide claim.
#[test]
fn the_guard_is_scoped_to_the_authority_log_module() {
    let crate_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let multi_device = crate_src.join("multi_device.rs");
    let text = fs::read_to_string(&multi_device).expect("multi_device.rs must exist");
    assert!(
        text.contains("current_timestamp"),
        "multi_device.rs is expected to read a clock; if that changed, revisit this boundary note"
    );
    assert!(!multi_device.starts_with(module_root()));
}
