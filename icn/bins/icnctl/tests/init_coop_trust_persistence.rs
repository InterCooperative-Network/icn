//! `icnctl init-coop` must seed trust the daemon can actually read (#2718).
//!
//! The wizard printed `✓ Trust edges created for N member(s)` while writing them
//! into the sled database at `<data_dir>/store`, and `icnd` opened
//! `<data_dir>/store/trust`. Two different databases, so every bootstrap trust
//! relationship was invisible to the node it configured — a bootstrap that
//! reports success is not successful unless the consumer can observe the bytes.
//!
//! These tests drive the **real `icnctl` binary** and read back through the
//! **daemon's own path constructor** (`Config::trust_store_path`). Nothing here
//! spells a store path itself: a test that recomputed the layout could agree
//! with a wrong writer and stay green, which is exactly the failure being fixed.
//!
//! The pre-fix binary fails `edge_written_by_init_coop_is_visible_at_the_daemon_path`
//! because `store/trust/` does not exist at all, and fails
//! `init_coop_does_not_leave_a_sled_database_at_the_store_root` because `store/`
//! is itself a sled root.

// Integration test: a failed assumption should abort the test loudly, which is what
// `expect`/`unwrap` do. Same posture as the other `icnctl` integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use icn_core::config::Config;
use icn_identity::Did;
use icn_store::SledStore;
use icn_trust::TrustGraph;

/// A second party for the coop. Any valid `did:icn:` identifier works; this one
/// is a fixed 32-byte key so the test does not depend on generating an identity
/// twice.
const MEMBER_DID: &str = "did:icn:z2mc9LnC3ic2berctfjr5xTLU8bibTgYJ3gFB5oTYcu1p";

const PASSPHRASE: &str = "init-coop-trust-test-passphrase";

/// Run the real wizard unattended.
///
/// `--yes` covers the confirmation prompt and `ICN_KEYSTORE_PASSPHRASE` covers
/// the keystore prompt, which is the same variable `icnd` reads.
fn run_init_coop(data_dir: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_icnctl"))
        .arg("--data-dir")
        .arg(data_dir)
        .args([
            "init-coop",
            "--name",
            "TrustPersistenceCoop",
            "--members",
            MEMBER_DID,
            "--yes",
            "--no-start",
        ])
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .output()
        .expect("failed to spawn icnctl");

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "init-coop failed (status {:?})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status.code()
    );
    assert!(
        stdout.contains("Trust edges created"),
        "init-coop did not report creating trust edges; it may have skipped step 7\n{stdout}"
    );
    stdout
}

/// The DID the wizard generated for this data directory, taken from its own
/// output rather than recomputed.
fn own_did_from(stdout: &str) -> Did {
    let line = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("Your DID: "))
        .or_else(|| stdout.lines().find_map(|l| l.trim().strip_prefix("DID: ")))
        .expect("init-coop did not print the DID it created");
    line.trim()
        .parse()
        .expect("init-coop printed an unparseable DID")
}

/// The path `icnd` opens. Resolved through the daemon's own constructor so the
/// test cannot drift from the daemon.
fn daemon_trust_path(data_dir: &Path) -> std::path::PathBuf {
    let config = Config {
        data_dir: data_dir.to_path_buf(),
        ..Default::default()
    };
    config.trust_store_path()
}

#[test]
fn edge_written_by_init_coop_is_visible_at_the_daemon_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path();

    // Writer: the real CLI, in its own process. It exits, so every sled handle
    // it held is closed before anything below opens the database.
    let stdout = run_init_coop(data_dir);
    let own_did = own_did_from(&stdout);
    let member: Did = MEMBER_DID.parse().expect("member DID");

    // Reader: the daemon's path, a fresh process-local handle, no writer state.
    let trust_path = daemon_trust_path(data_dir);
    assert!(
        trust_path.exists(),
        "the daemon's trust store was never created at {}; init-coop wrote somewhere else",
        trust_path.display()
    );

    let store = Arc::new(SledStore::open(&trust_path).expect("open daemon trust store"));
    let graph = TrustGraph::new(store, own_did.clone());

    let edge = graph
        .get_edge(&own_did, &member)
        .expect("trust store read failed")
        .unwrap_or_else(|| {
            panic!(
                "the daemon path holds no edge {own_did} -> {member}; \
                 init-coop reported success but the daemon sees nothing"
            )
        });

    assert_eq!(
        edge.source, own_did,
        "edge source is not the wizard's own DID"
    );
    assert_eq!(edge.target, member, "edge target is not the member DID");
    assert!(
        (edge.score.value() - 0.5).abs() < f64::EPSILON,
        "bootstrap trust score was not preserved across the reopen: {}",
        edge.score.value()
    );
}

/// MUST-FAIL control for the defect's signature.
///
/// Pre-fix, `init-coop` opened `<data_dir>/store` *itself* as a sled database,
/// leaving sled's `db`/`conf` files at the store root and nesting every other
/// domain store inside a live database directory. If that reappears, the layout
/// has regressed even if the test above somehow still passes.
#[test]
fn init_coop_does_not_leave_a_sled_database_at_the_store_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path();
    run_init_coop(data_dir);

    let store_root = data_dir.join("store");
    for marker in ["db", "conf"] {
        assert!(
            !store_root.join(marker).exists(),
            "sled marker `{marker}` at {} means the store root is itself a database, \
             which is the #2718 layout defect",
            store_root.display()
        );
    }

    // ...and the trust database is one level down, where the daemon looks.
    let trust_path = daemon_trust_path(data_dir);
    assert!(
        trust_path.join("db").exists() || trust_path.join("conf").exists(),
        "no sled database at the daemon's trust path {}",
        trust_path.display()
    );
}

/// Pins the canonical trust subdirectory constant.
///
/// Unlike the two tests above, this one does **not** discriminate the #2718
/// writer defect: it observes only the daemon side, so it passed even against
/// the pre-fix binary during mutation testing. It is here to catch a rename or
/// re-spelling of the canonical subdirectory, not to prove the writer agrees.
/// The writer/reader agreement is proven by
/// `edge_written_by_init_coop_is_visible_at_the_daemon_path`.
#[test]
fn the_wizard_and_the_daemon_agree_on_where_trust_lives() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path();
    run_init_coop(data_dir);

    let daemon_path = daemon_trust_path(data_dir);

    // The only directory under `store/` that init-coop created for trust must be
    // the one the daemon opens.
    assert!(
        daemon_path.starts_with(data_dir.join("store")),
        "the daemon trust path escaped the store root: {}",
        daemon_path.display()
    );
    assert_eq!(
        daemon_path.file_name().and_then(|s| s.to_str()),
        Some(icn_core::config::TRUST_STORE_SUBDIR),
        "the daemon's trust subdirectory is not the declared canonical one"
    );
}
