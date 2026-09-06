//! A trust edge `init-coop` reports creating must be visible to the daemon (#2718).
//!
//! `icnctl init-coop` step 7 opened `<data_dir>/store` and wrote trust edges
//! there; `icnd` reads trust state from `<data_dir>/store/trust`. Those are two
//! different sled databases, so the wizard printed "✓ Trust edges created" and
//! the node then started with an empty trust graph. Trust gates rate limiting,
//! connection admission and federated placement, so a freshly initialised
//! cooperative came up with none of its intended bootstrap trust.
//!
//! **The proof has to cross the writer/reader boundary.** A test that writes an
//! edge and reads it back through the same handle — or through the same wrong
//! path — passes on the defect and proves nothing. So these tests run the real
//! `icnctl` binary, let the process exit (closing its sled handles), and then
//! open the database *at the path the daemon uses*, resolved the way `icnd`
//! resolves it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use icn_identity::Did;
use icn_store::{SledStore, Store};
use tempfile::TempDir;

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// The path `icnd` opens for trust state (`icnd/src/main.rs`, via
/// `Config::store_path()`): `<data_dir>/store/trust`.
fn daemon_trust_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("trust")
}

/// The directory that *contains* the per-domain stores. `init-coop` used to
/// materialise a sled database here, nesting a root inside the tree the N2-A
/// startup gate walks.
fn store_root(data_dir: &Path) -> PathBuf {
    data_dir.join("store")
}

fn fresh_did() -> Did {
    icn_identity::KeyPair::generate().unwrap().did().clone()
}

/// A passphrase supplied the way `icnd` and every scripted `icnctl` call supply
/// one, so the wizard runs without a TTY.
const PASSPHRASE: &str = "m4d-2718-fixture-passphrase";

/// Create the keystore first, so `init-coop` takes its *existing identity*
/// branch.
///
/// The wizard's new-identity branch calls `rpassword` directly instead of the
/// crate's `read_passphrase`/`confirm_passphrase` helpers, so it ignores
/// `ICN_KEYSTORE_PASSPHRASE` and cannot run unattended. That is a separate
/// defect from the one under test here and is recorded rather than fixed; this
/// fixture sidesteps it by provisioning the identity through `id init`, which
/// does honour the variable.
fn init_identity(data_dir: &Path) -> Output {
    Command::new(icnctl_bin())
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .arg("--data-dir")
        .arg(data_dir)
        .args(["id", "init"])
        .output()
        .unwrap()
}

fn run_init_coop(data_dir: &Path, member: &Did) -> Output {
    Command::new(icnctl_bin())
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        // Step 6 probes `$ICN_GATEWAY/v1/health` (default `localhost:8080`) and,
        // if `ICN_TOKEN` is set, POSTs a governance domain to it. A unit test
        // must not create real state on whatever happens to be listening on a
        // developer machine or a CI runner, so the gateway is pointed at a port
        // nothing can be serving and the token is removed from the inherited
        // environment. Step 7 — the step under test — runs either way, and this
        // also drops the 3s health-check timeout from every run.
        .env("ICN_GATEWAY", "http://127.0.0.1:1")
        .env_remove("ICN_TOKEN")
        .arg("--data-dir")
        .arg(data_dir)
        .args([
            "init-coop",
            "--name",
            "Trust Path Coop",
            "--members",
            member.as_str(),
            "--yes",
            "--no-start",
        ])
        .output()
        .unwrap()
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Read `trust/edges/` rows through a freshly opened handle at `path`.
///
/// Opening here — after the `icnctl` process has exited — is what makes this a
/// cross-process, cross-handle read rather than a round-trip through the
/// writer's own state.
fn trust_edge_rows(path: &Path) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }
    let store = SledStore::open(path).unwrap();
    let rows = store
        .scan(b"trust/edges/")
        .unwrap()
        .into_iter()
        .map(|(k, _)| String::from_utf8_lossy(&k).into_owned())
        .collect();
    drop(store);
    rows
}

#[test]
fn an_init_coop_trust_edge_is_readable_through_the_daemons_trust_store() {
    let dir = TempDir::new().unwrap();
    let member = fresh_did();
    let id_out = init_identity(dir.path());
    assert!(
        id_out.status.success(),
        "fixture setup: {}",
        combined(&id_out)
    );

    let out = run_init_coop(dir.path(), &member);
    let text = combined(&out);
    assert!(
        text.contains("Trust edges created"),
        "the wizard must report creating trust edges — otherwise this test is \
         not exercising the claim it exists to check:\n{text}"
    );

    // The whole point: read where the DAEMON reads, from a handle the wizard
    // never held.
    let seen = trust_edge_rows(&daemon_trust_store_path(dir.path()));
    assert!(
        seen.iter().any(|k| k.contains(member.as_str())),
        "the edge `init-coop` reported creating must be present in the store \
         `icnd` opens ({}); found {:?}\n{text}",
        daemon_trust_store_path(dir.path()).display(),
        seen
    );
}

#[test]
fn init_coop_does_not_materialise_a_sled_database_at_the_store_root() {
    // `<data_dir>/store` is the directory that holds `trust/`, `ledger/`,
    // `governance/` and the rest. A sled database opened *at* it nests a root
    // inside the tree `find_sled_roots` walks for the N2-A startup gate.
    let dir = TempDir::new().unwrap();
    let member = fresh_did();
    assert!(init_identity(dir.path()).status.success());

    let out = run_init_coop(dir.path(), &member);
    let text = combined(&out);
    // Guard, for the same reason the sibling test has one: if the wizard failed
    // before step 7 it would write nothing under `<data_dir>/store` at all, and
    // both assertions below would pass while proving nothing.
    assert!(
        text.contains("Trust edges created"),
        "the wizard must reach step 7 for this test to mean anything:\n{text}"
    );

    let root = store_root(dir.path());
    for artifact in ["db", "conf"] {
        assert!(
            !root.join(artifact).exists(),
            "a sled `{artifact}` file at {} means a database was opened on the \
             store root itself",
            root.display()
        );
    }
}
