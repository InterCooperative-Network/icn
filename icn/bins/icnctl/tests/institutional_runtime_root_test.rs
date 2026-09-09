//! A cooperative must be able to come into existence as an institution (#2744).
//!
//! Before this ceremony existed a cooperative could only come into existence as
//! a *string*: `init-coop` wrote a keystore and an `icn.toml` with no
//! `[cooperative]` section, so `CooperativeConfig.treasury_did` was always
//! `None` and `supervisor/lifecycle.rs` fell back to the node's own DID for
//! every governance-authored ledger entry. The treasury *was* the node
//! operator.
//!
//! **What these tests are careful about.**
//!
//! They run the real `icnctl` binary, let the process exit so its sled handles
//! close, and only then reopen the databases *at the paths the daemon uses* —
//! spelled out literally here rather than resolved through the same helper the
//! implementation calls, so a wrong helper cannot make the test agree with the
//! code by construction. That is the lesson of #2718/#2724.
//!
//! Every child process runs with **every** `ICN_DEV_*` variable stripped from
//! its environment, not just `ICN_DEV_SELF_TRUST`. If a dev authority shortcut
//! were what made the positive path pass, the implementation would have failed
//! the issue rather than satisfied it.
//!
//! Scope: this is the **Principal-generation** runtime genesis slice. It does
//! not implement the GEN protocol (#2602), stable Subject-generation (#2694),
//! federation, or Technical Alpha as a whole.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use icn_store::{SledStore, Store};
use tempfile::TempDir;

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// A passphrase supplied the way every scripted `icnctl` call supplies one, so
/// the ceremony runs without a TTY. `rpassword` reads `/dev/tty`, never stdin,
/// so piping a passphrase in is not an option.
const PASSPHRASE: &str = "s1-2744-genesis-fixture-passphrase";

// ---------------------------------------------------------------------------
// Daemon-side paths, spelled out deliberately (see module docs).
// ---------------------------------------------------------------------------

fn daemon_coop_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("cooperative")
}

fn daemon_trust_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("trust")
}

fn daemon_ledger_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("ledger")
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/// Build an `icnctl` invocation with every `ICN_DEV_*` authority shortcut
/// removed from the inherited environment.
///
/// Enumerating the parent environment rather than listing known names is the
/// point: a future `ICN_DEV_SOMETHING_ELSE` leaking in from a developer shell
/// or a CI runner must not be able to make the positive path pass.
fn icnctl(data_dir: &Path) -> Command {
    let mut cmd = Command::new(icnctl_bin());
    for (key, _) in std::env::vars() {
        if key.starts_with("ICN_DEV_") {
            cmd.env_remove(&key);
        }
    }
    cmd.env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        // Never touch whatever happens to be listening on a developer machine.
        .env("ICN_GATEWAY", "http://127.0.0.1:1")
        .env_remove("ICN_TOKEN")
        .arg("--data-dir")
        .arg(data_dir);
    cmd
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Provision the node identity the ceremony will run under.
fn init_identity(data_dir: &Path) -> Output {
    icnctl(data_dir).args(["id", "init"]).output().unwrap()
}

/// The node DID, read back out of the keystore through the production binary
/// rather than recomputed here.
fn node_did(data_dir: &Path) -> String {
    let out = icnctl(data_dir).args(["id", "show"]).output().unwrap();
    let text = combined(&out);
    assert!(out.status.success(), "`id show` failed: {text}");
    text.split_whitespace()
        .find(|tok| tok.starts_with("did:icn:"))
        .unwrap_or_else(|| panic!("no DID in `id show` output:\n{text}"))
        .trim_end_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

/// Provision the node the way an operator actually would before founding an
/// institution on it: `init-coop` is what writes the `icn.toml` whose
/// `[cooperative]` section genesis links the treasury into.
///
/// This also means the fixture exercises the storage-root agreement check:
/// `init-coop` writes `data_dir = <the CLI --data-dir>`, so the configuration
/// and the command line agree and genesis proceeds.
fn run_init_coop(data_dir: &Path) -> Output {
    let out = icnctl(data_dir)
        .args(["init-coop", "--name", "Fixture Node", "--yes", "--no-start"])
        .output()
        .unwrap();
    if out.status.success() {
        make_config_daemon_loadable(data_dir);
    }
    out
}

/// Repair the one field that stops `init-coop`'s generated `icn.toml` from
/// loading, so the fixture provisions a node a daemon could actually run.
///
/// This is **not** cosmetic. `init-coop` emits `[network]` without
/// `bootstrap_peers`, which has no serde default, so `Config::from_file` — the
/// loader `icnd --config` uses — rejects the very file `init-coop` tells the
/// operator to run. That is icn#2747: pre-existing, out of scope for #2744, and
/// genesis now refuses such a configuration rather than certifying a genesis
/// the daemon could never consume. The fixture therefore has to hand genesis a
/// loadable config; without this, every positive test below would exercise that
/// refusal instead of the ceremony.
fn make_config_daemon_loadable(data_dir: &Path) {
    let path = data_dir.join("icn.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();

    // (1) `[network] bootstrap_peers` has no serde default, so the file does not
    //     even parse as `Config`.
    if !text.contains("bootstrap_peers") {
        let patched = text.replace("[network]\n", "[network]\nbootstrap_peers = []\n");
        assert_ne!(patched, text, "fixture: [network] section must be present");
        text = patched;
    }

    // (2) The template enables the gateway but leaves `jwt_secret` commented
    //     out, so `Config::validate` fails and the daemon exits at startup. Both
    //     are facets of icn#2747: the configuration `init-coop` generates cannot
    //     run the daemon it tells the operator to start. The ceremony refuses
    //     such a configuration rather than certify a node that cannot start, so
    //     the fixture has to supply what the template omits.
    // Guard on an UNCOMMENTED assignment: the template's own
    // `# jwt_secret = "CHANGE_ME"` line contains the substring, so a naive
    // `contains` check silently skips the patch.
    if !text
        .lines()
        .any(|l| l.trim_start().starts_with("jwt_secret = \""))
    {
        let patched = text.replace(
            "# jwt_secret = \"CHANGE_ME\"  # Set this before starting!",
            "jwt_secret = \"fixture-jwt-secret-not-a-real-credential\"",
        );
        assert_ne!(
            patched, text,
            "fixture: the jwt_secret placeholder must be present"
        );
        text = patched;
    }

    std::fs::write(&path, text).unwrap();
}

fn provision_runtime_root(data_dir: &Path, name: &str) -> Output {
    icnctl(data_dir)
        .args([
            "institution",
            "runtime-root",
            "create",
            "--name",
            name,
            "--yes",
        ])
        .output()
        .unwrap()
}

fn read_receipt_json(data_dir: &Path) -> (Output, String) {
    let out = icnctl(data_dir)
        .args(["institution", "runtime-root", "show", "--json"])
        .output()
        .unwrap();
    // Raw stdout, no substring extraction: `icnctl` writes diagnostics to
    // stderr, so a `--json` document stands alone. Callers here want the
    // receipt itself, which the envelope carries under `receipt`.
    let text = serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .ok()
        .and_then(|v| v.get("receipt").cloned())
        .map(|r| r.to_string())
        .unwrap_or_else(|| String::from_utf8_lossy(&out.stdout).into_owned());
    (out, text)
}

/// Open a sled database, retrying briefly on the exclusive-lock error.
///
/// sled takes a flock and releases it when the last handle drops — but the
/// release can lag a just-dropped handle or a just-exited child process, and
/// this suite opens the same databases repeatedly from both. Failing on the
/// first `WouldBlock` makes tests flaky under parallel load for a reason that
/// has nothing to do with what they assert.
///
/// This retries the *lock* specifically and still fails on any other error, so
/// a genuinely missing or corrupt database is not papered over.
fn open_store(path: impl AsRef<Path>) -> SledStore {
    let path = path.as_ref();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        match SledStore::open(path) {
            Ok(store) => return store,
            Err(e) => {
                let msg = e.to_string();
                let is_lock = msg.contains("could not acquire lock") || msg.contains("WouldBlock");
                if !is_lock || std::time::Instant::now() >= deadline {
                    panic!("failed to open {}: {e}", path.display());
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

/// Scan a sled database opened *after* the writing process exited.
fn rows_with_prefix(path: &Path, prefix: &[u8]) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }
    let store = open_store(path);
    let rows = store
        .scan(prefix)
        .unwrap()
        .into_iter()
        .map(|(k, _)| String::from_utf8_lossy(&k).into_owned())
        .collect();
    drop(store);
    rows
}

// ---------------------------------------------------------------------------
// THE DISCRIMINATOR
// ---------------------------------------------------------------------------

/// The whole ceremony, end to end, through the real binary and real stores.
///
/// Against pre-fix `main` this fails at the first step: there is no genesis
/// ceremony to run. That single failure is the honest one — the later
/// assertions are unreachable rather than independently observable, and this
/// test does not pretend otherwise.
#[test]
fn institutional_genesis_creates_an_institution_distinct_from_the_node() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();

    let id_out = init_identity(data_dir);
    assert!(id_out.status.success(), "fixture: {}", combined(&id_out));
    let node = node_did(data_dir);
    let coop_out = run_init_coop(data_dir);
    assert!(
        coop_out.status.success(),
        "fixture: {}",
        combined(&coop_out)
    );

    // 1. The ceremony runs at all.
    let out = provision_runtime_root(data_dir, "Riverside Bakery Cooperative");
    let text = combined(&out);
    assert!(
        out.status.success(),
        "institutional genesis must be reachable through production `icnctl` \
         (#2744 G5). Got:\n{text}"
    );

    // 2. G1 — the cooperative is durable state, read back through a handle the
    //    ceremony never held.
    let coop_rows = rows_with_prefix(&daemon_coop_store_path(data_dir), b"coop:");
    assert!(
        !coop_rows.is_empty(),
        "G1: a cooperative must exist in the store the daemon opens ({}); a \
         string in a config file is not institutional existence. Found {:?}",
        daemon_coop_store_path(data_dir).display(),
        coop_rows
    );

    // 3. G6 — the receipt is readable back through production code.
    let (show_out, receipt) = read_receipt_json(data_dir);
    assert!(
        show_out.status.success(),
        "G6: the genesis receipt must be readable back through production \
         code:\n{}",
        combined(&show_out)
    );
    let receipt: serde_json::Value = serde_json::from_str(&receipt)
        .unwrap_or_else(|e| panic!("receipt must be structured JSON, not prose: {e}\n{receipt}"));

    let treasury = receipt["treasury_did"]
        .as_str()
        .expect("G6: receipt must name the treasury")
        .to_string();
    let founder = receipt["genesis_authority_did"]
        .as_str()
        .expect("G6: receipt must name the genesis authority")
        .to_string();
    assert!(
        receipt["cooperative_id"].as_str().is_some(),
        "G6: receipt must name the cooperative: {receipt}"
    );
    assert!(
        receipt["schema_version"].as_u64().is_some(),
        "G6: receipt must be versioned: {receipt}"
    );

    // 3b. The trust root is bound to the cooperative on the cooperative's own
    //     durable record — not only in the receipt and not only in the trust
    //     graph. Read through a handle the ceremony never held.
    let trust_root = receipt["trust_root_did"].as_str().unwrap().to_string();
    {
        let store = open_store(daemon_coop_store_path(data_dir));
        let bound = store
            .scan(b"coop:")
            .unwrap()
            .into_iter()
            .any(|(_, v)| String::from_utf8_lossy(&v).contains(&trust_root));
        drop(store);
        assert!(
            bound,
            "the cooperative record must bind its genesis trust root, or the \
             identifier exists only because the receipt says so"
        );
    }
    assert_ne!(trust_root, node, "the trust root must not be the node");
    assert_ne!(
        trust_root, treasury,
        "the trust root and the treasury must be different principals"
    );

    // 4. G2 — the treasury is a principal of its own, not the machine.
    assert_ne!(
        treasury, node,
        "G2: the treasury identity must be distinct from the node DID. \
         Collapsing them is the defect #2744 exists to fix."
    );

    // 5. G2 — and it is backed by real key material, not a fabricated string.
    let treasury_parses = treasury.starts_with("did:icn:")
        && serde_json::from_value::<icn_identity::Did>(serde_json::Value::String(treasury.clone()))
            .is_ok();
    assert!(
        treasury_parses,
        "G2: the treasury DID must be a real keypair-backed DID that survives \
         the daemon's own config parse. `did:icn:treasury:<bs58>` never parses \
         and `Did::from_anchor_id` parses only about half the time; either \
         would be silently replaced by the node DID at \
         supervisor/lifecycle.rs. Got: {treasury}"
    );

    // 6. G3 — the treasury is registered in the ledger and linked to the coop.
    let treasury_rows = rows_with_prefix(&daemon_ledger_store_path(data_dir), b"ledger:treasury:");
    assert!(
        treasury_rows.iter().any(|k| k.contains(&treasury)),
        "G3: the treasury must be registered durably in the ledger store ({}); \
         found {:?}",
        daemon_ledger_store_path(data_dir).display(),
        treasury_rows
    );

    // 7. G4 — institution-rooted trust, not a node self-edge.
    // Edge keys are `trust/edges/{source}:{target}` and a DID CONTAINS colons,
    // so these must be compared against a constructed key. Splitting on the
    // first ':' yields "did" for every key ever written, which would make both
    // assertions below structurally incapable of failing.
    let edges = rows_with_prefix(&daemon_trust_store_path(data_dir), b"trust/edges/");
    let self_edge = format!("trust/edges/{node}:{node}");
    assert!(
        !edges.contains(&self_edge),
        "G4/G5: genesis must never write the node self-edge that \
         ICN_DEV_SELF_TRUST writes ({self_edge}). Found {edges:?}"
    );
    let node_source_prefix = format!("trust/edges/{node}:");
    assert!(
        edges.iter().any(|k| !k.starts_with(&node_source_prefix)),
        "G4: some stored trust edge must be rooted in an authority that is not \
         the node itself, or the treasury's authority is the machine's. \
         Found {edges:?}"
    );
    assert!(
        edges.contains(&format!("trust/edges/{trust_root}:{treasury}")),
        "G4: the trust root's authority edge over the treasury must be present \
         verbatim. Found {edges:?}"
    );

    // 8. The founding authority is named.
    assert!(
        founder.starts_with("did:icn:"),
        "G4: the founding authority must be a real principal: {founder}"
    );
}

// ---------------------------------------------------------------------------
// G4 — the founding authority crosses the REAL ledger author-trust gate
// ---------------------------------------------------------------------------

/// A governance-authored journal entry must append because of the trust facts
/// genesis wrote, with no dev-mode authority anywhere.
///
/// **What makes this the real boundary and not a harness.** The ledger is a
/// real `Ledger` over the ledger store the ceremony wrote. Its trust service is
/// the *production* `TrustServiceImplTokio` built by
/// `icn_trust_app::create_service_tokio` — the same constructor `icnd` calls —
/// over a `TrustGraph` opened at the daemon's own trust path with the node DID
/// as `own_did`, exactly as `icnd` constructs it. `min_trust_for_entry` is left
/// at its production default of 0.1; the sibling test in `witness_trust.rs`
/// sets it to 0.0 to isolate witness checks, and doing that here would make the
/// gate vacuous and the test worthless.
///
/// The entry is built the way `LedgerServiceImpl` builds governance-authored
/// entries: `JournalEntryBuilder::new(treasury_did)` plus governance
/// provenance.
#[tokio::test(flavor = "multi_thread")]
async fn a_governance_authored_append_crosses_the_real_gate_without_dev_self_trust() {
    use icn_ledger::{entry::JournalEntryBuilder, Ledger};

    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();

    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());
    let node = node_did(data_dir);
    let out = provision_runtime_root(data_dir, "Author Gate Cooperative");
    assert!(out.status.success(), "genesis: {}", combined(&out));

    let (_show, receipt_json) = read_receipt_json(data_dir);
    let receipt: serde_json::Value = serde_json::from_str(&receipt_json).unwrap();
    let treasury: icn_identity::Did = receipt["treasury_did"].as_str().unwrap().parse().unwrap();

    // Sanity: the environment this assertion runs under really has no dev
    // authority shortcut. If one leaked in, a pass here would prove nothing.
    for (key, _) in std::env::vars() {
        assert!(
            !key.starts_with("ICN_DEV_"),
            "{key} is set in the test environment; the positive result would be \
             unattributable (#2744 G5)"
        );
    }

    // Rebuild the daemon's own trust plumbing over the ceremony's stores.
    let node_keypair = {
        let mut ks = icn_identity::AgeKeyStore::open(data_dir.join("identity.age")).unwrap();
        icn_identity::KeyStore::unlock(&mut ks, PASSPHRASE.as_bytes()).unwrap();
        icn_identity::KeyStore::get_keypair(&ks).unwrap()
    };
    let trust_store: std::sync::Arc<dyn icn_store::Store> =
        std::sync::Arc::new(open_store(daemon_trust_store_path(data_dir)));
    let node_did_typed: icn_identity::Did = node.parse().unwrap();
    let graph = std::sync::Arc::new(tokio::sync::RwLock::new(icn_trust::TrustGraph::new(
        trust_store.clone(),
        node_did_typed,
    )));
    let trust_service =
        icn_trust_app::create_service_tokio(graph, node_keypair, trust_store.clone());

    // The gate's own view of the author, through the production service.
    let score = { trust_service.trust_score(&treasury.as_str().parse().unwrap()) };
    assert!(
        score >= 0.1,
        "G4: the treasury author must clear the ledger's 0.1 author-trust \
         threshold on the strength of the genesis trust facts alone; scored \
         {score}"
    );

    let ledger_store: std::sync::Arc<dyn icn_store::Store> =
        std::sync::Arc::new(open_store(daemon_ledger_store_path(data_dir)));
    let mut ledger = Ledger::new(ledger_store).unwrap();
    ledger.set_trust_service(trust_service);
    // Deliberately NOT calling `set_min_trust_for_entry`: the production
    // default is the whole point.

    // Balanced double entry, the shape `LedgerServiceImpl::build_account_deltas`
    // produces for a governance-authorised spend: debit the treasury, credit a
    // recipient. The amounts are irrelevant to the author-trust gate but the
    // entry must be intrinsically valid to reach it.
    let recipient = icn_identity::KeyPair::generate().unwrap().did().clone();
    let entry = JournalEntryBuilder::new(treasury.clone())
        .with_governance_provenance("genesis-acceptance-receipt", "genesis-acceptance-hash")
        .add_delta(icn_ledger::AccountDelta::debit(
            treasury.clone(),
            "HOURS".to_string(),
            10,
        ))
        .add_delta(icn_ledger::AccountDelta::credit(
            recipient.clone(),
            "HOURS".to_string(),
            10,
        ))
        .build()
        .unwrap();
    let appended = ledger.append_entry(entry).await;
    assert!(
        appended.is_ok(),
        "G4: a governance-authored entry must append under the institution's \
         own trust facts, with no ICN_DEV_SELF_TRUST: {:?}",
        appended.err()
    );

    // The converse, so the pass above is attributable to the genesis facts and
    // not to a gate that accepts anyone.
    let stranger = icn_identity::KeyPair::generate().unwrap().did().clone();
    let stranger_entry = JournalEntryBuilder::new(stranger.clone())
        .with_governance_provenance("stranger-receipt", "stranger-hash")
        .add_delta(icn_ledger::AccountDelta::debit(
            stranger,
            "HOURS".to_string(),
            10,
        ))
        .add_delta(icn_ledger::AccountDelta::credit(
            recipient,
            "HOURS".to_string(),
            10,
        ))
        .build()
        .unwrap();
    assert!(
        ledger.append_entry(stranger_entry).await.is_err(),
        "G4: the gate must still refuse an author the institution never \
         authorised, or the positive result above proves nothing"
    );
}

// ---------------------------------------------------------------------------
// Negative cases — genesis must refuse rather than collapse or half-succeed
// ---------------------------------------------------------------------------

/// G7: with no node keystore there is no authority to found under.
#[test]
fn genesis_refuses_when_no_genesis_authority_can_be_established() {
    let dir = TempDir::new().unwrap();
    let out = provision_runtime_root(dir.path(), "No Authority Coop");
    assert!(!out.status.success(), "genesis must refuse");
    let text = combined(&out);
    assert!(
        text.contains("no node identity"),
        "the refusal must say the founding authority could not be \
         established, not fail incidentally later:\n{text}"
    );
    assert!(
        !dir.path().join("treasury.age").exists(),
        "G7: no key material may be minted when authority cannot be established"
    );
}

/// G7: a second ceremony must not silently mint a second institution.
#[test]
fn genesis_refuses_a_second_ceremony_over_the_first() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());
    assert!(provision_runtime_root(data_dir, "First Cooperative")
        .status
        .success());

    let (_, first) = read_receipt_json(data_dir);
    let first: serde_json::Value = serde_json::from_str(&first).unwrap();
    let first_treasury = first["treasury_did"].as_str().unwrap().to_string();

    let out = provision_runtime_root(data_dir, "Second Cooperative");
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "a second genesis must refuse:\n{text}"
    );
    // Pin *which* guard refuses. Two independent guards happen to cover this —
    // the prior-ceremony check, and the config linkage check refusing a second
    // `[cooperative]` section — which is real defence in depth, but a test that
    // accepted either could not tell the intended guard from an accident. A
    // mutation removing `refuse_if_already_provisioned` survived until this
    // assertion existed.
    assert!(
        text.contains("already undergone genesis"),
        "the refusal must come from the prior-ceremony check, naming the \
         existing institution, rather than incidentally from a later step:\n{text}"
    );

    let (_, after) = read_receipt_json(data_dir);
    let after: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(
        after["treasury_did"].as_str().unwrap(),
        first_treasury,
        "G7: the first institution's treasury must survive a refused rerun"
    );
}

/// #2725 class: authoritative state must never be written to a root the daemon
/// will not read.
#[test]
fn genesis_refuses_when_the_config_names_a_different_storage_root() {
    let dir = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());

    // A configuration that points the daemon somewhere else entirely.
    std::fs::write(
        data_dir.join("icn.toml"),
        format!(
            "# hand-edited\ndata_dir = \"{}\"\n",
            elsewhere.path().display()
        ),
    )
    .unwrap();

    let out = provision_runtime_root(data_dir, "Divergent Root Coop");
    assert!(!out.status.success(), "genesis must refuse");
    let text = combined(&out);
    assert!(
        text.contains(&data_dir.display().to_string())
            && text.contains(&elsewhere.path().display().to_string()),
        "the refusal must name BOTH paths so the operator can see the \
         disagreement:\n{text}"
    );
    assert!(
        !data_dir.join("treasury.age").exists() && !elsewhere.path().join("treasury.age").exists(),
        "nothing may be written under either root when they disagree"
    );
}

/// G6/G7: a receipt must never be reported for an institution whose durable
/// state is not there. The receipt lives in the same sled database as the
/// `coop:` rows precisely so it cannot outlive them.
#[test]
fn a_receipt_cannot_outlive_the_cooperative_store_it_describes() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());
    assert!(provision_runtime_root(data_dir, "Ephemeral Coop")
        .status
        .success());
    assert!(read_receipt_json(data_dir).0.status.success());

    std::fs::remove_dir_all(daemon_coop_store_path(data_dir)).unwrap();

    let (out, _) = read_receipt_json(data_dir);
    assert!(
        !out.status.success(),
        "with the cooperative store gone, `show` must not still report a \
         genesis: {}",
        combined(&out)
    );
}

/// G5: the ceremony must not depend on a dev authority shortcut. Setting the
/// flag must change nothing about the outcome.
#[test]
fn genesis_neither_needs_nor_consults_icn_dev_self_trust() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    let out = Command::new(icnctl_bin())
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .env("ICN_GATEWAY", "http://127.0.0.1:1")
        .env_remove("ICN_TOKEN")
        // Deliberately SET, to show the outcome does not depend on it.
        .env("ICN_DEV_SELF_TRUST", "1")
        .arg("--data-dir")
        .arg(data_dir)
        .args([
            "institution",
            "runtime-root",
            "create",
            "--name",
            "Flag Set Coop",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "genesis: {}", combined(&out));

    let node = node_did(data_dir);
    // Constructed key, not a split — see the note in the main witness.
    let edges = rows_with_prefix(&daemon_trust_store_path(data_dir), b"trust/edges/");
    let self_edge = format!("trust/edges/{node}:{node}");
    assert!(
        !edges.contains(&self_edge),
        "genesis must not write a node self-edge ({self_edge}) even when the \
         dev flag is set — it does not read it. Found {edges:?}"
    );
    assert!(
        !edges.is_empty(),
        "fixture: genesis must have written edges"
    );
}

// ---------------------------------------------------------------------------
// What the two trust facts actually carry
// ---------------------------------------------------------------------------
//
// These two tests exist to pin down the authority semantics of this slice so
// documentation cannot drift into claiming more than the mechanism does. One
// shows the institution-attributed edge is load-bearing; the other shows the
// residual dependence on the node's recognition. Both are true today, and the
// second is the honest limit of the claim.

/// Rebuild the daemon's trust plumbing over the ceremony's stores and score an
/// author exactly as the ledger's gate would.
async fn author_score_after_removing_edge(
    data_dir: &Path,
    remove: Option<(&str, &str)>,
    author: &str,
) -> f64 {
    if let Some((source, target)) = remove {
        let store = open_store(daemon_trust_store_path(data_dir));
        let key = format!("trust/edges/{source}:{target}");
        store.delete(key.as_bytes()).unwrap();
        // Confirm the mutation actually landed. A mutant that silently failed
        // to apply would make the test pass for the wrong reason.
        assert!(
            store.get(key.as_bytes()).unwrap().is_none(),
            "the mutation did not remove {key}"
        );
        drop(store);
    }

    let node = node_did(data_dir);
    let node_keypair = {
        let mut ks = icn_identity::AgeKeyStore::open(data_dir.join("identity.age")).unwrap();
        icn_identity::KeyStore::unlock(&mut ks, PASSPHRASE.as_bytes()).unwrap();
        icn_identity::KeyStore::get_keypair(&ks).unwrap()
    };
    let trust_store: std::sync::Arc<dyn icn_store::Store> =
        std::sync::Arc::new(open_store(daemon_trust_store_path(data_dir)));
    let graph = std::sync::Arc::new(tokio::sync::RwLock::new(icn_trust::TrustGraph::new(
        trust_store.clone(),
        node.parse::<icn_identity::Did>().unwrap(),
    )));
    let svc = icn_trust_app::create_service_tokio(graph, node_keypair, trust_store);
    svc.trust_score(&author.parse().unwrap())
}

/// Set up a genesis and return `(data_dir owner, node, trust_root, treasury)`.
async fn genesis_fixture(name: &str) -> (TempDir, String, String, String) {
    let dir = TempDir::new().unwrap();
    assert!(init_identity(dir.path()).status.success());
    assert!(run_init_coop(dir.path()).status.success());
    assert!(provision_runtime_root(dir.path(), name).status.success());
    let (_, json) = read_receipt_json(dir.path());
    let r: serde_json::Value = serde_json::from_str(&json).unwrap();
    let node = r["node_did"].as_str().unwrap().to_string();
    let trust_root = r["trust_root_did"].as_str().unwrap().to_string();
    let treasury = r["treasury_did"].as_str().unwrap().to_string();
    (dir, node, trust_root, treasury)
}

/// M-authority-1 — the institution-attributed edge is load-bearing.
///
/// Keep `node -> institution`, remove `institution -> treasury`. The treasury
/// author must fall below the ledger's threshold. This is what makes the
/// institution's place in the chain real rather than decorative.
#[tokio::test(flavor = "multi_thread")]
async fn removing_the_institutions_edge_drops_the_treasury_below_the_gate() {
    let (dir, _node, trust_root, treasury) = genesis_fixture("Institution Edge Cooperative").await;

    let before = author_score_after_removing_edge(dir.path(), None, &treasury).await;
    assert!(
        before >= 0.1,
        "fixture: the treasury must clear the gate before the mutation; got {before}"
    );

    let after =
        author_score_after_removing_edge(dir.path(), Some((&trust_root, &treasury)), &treasury)
            .await;
    assert!(
        after < 0.1,
        "M-authority-1: with the institution's own edge removed the treasury \
         must fall below the ledger's 0.1 author-trust threshold, or that edge \
         was not carrying the authority. Scored {after}"
    );
}

/// M-authority-2 — and the residual dependence on the node's recognition.
///
/// Keep `institution -> treasury`, remove `node -> institution`. The treasury
/// still falls below the threshold, because the trust computation is
/// ego-centric from the node DID and cannot reach the institution at all.
///
/// This is the honest limit of what S1 establishes: the machine no longer
/// stands in for the treasury, but the institution's authority is not yet
/// independent of the machine recognising it.
#[tokio::test(flavor = "multi_thread")]
async fn removing_the_nodes_recognition_also_drops_the_treasury_below_the_gate() {
    let (dir, node, trust_root, treasury) = genesis_fixture("Node Recognition Cooperative").await;

    let after =
        author_score_after_removing_edge(dir.path(), Some((&node, &trust_root)), &treasury).await;
    assert!(
        after < 0.1,
        "M-authority-2: the institution-attributed edge alone must NOT be \
         enough — the ledger's trust computation is ego-centric from the node \
         DID, so an institution the node does not recognise is unreachable. If \
         this ever scores above the gate, the trust model changed and the \
         non-claim about institutional independence must be revisited. \
         Scored {after}"
    );
}

/// A ceremony that died before its commit point must be reported as INCOMPLETE,
/// and a rerun must neither resume it nor mint fresh identities over it.
///
/// The crash is simulated by removing the receipt row — the commit point — from
/// otherwise-complete durable state. That is precisely the state a process kill
/// between step 11 and step 12 would leave.
#[test]
fn partial_genesis_is_reported_incomplete_and_never_silently_completed() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());
    assert!(provision_runtime_root(data_dir, "Interrupted Cooperative")
        .status
        .success());

    let (_, json) = read_receipt_json(data_dir);
    let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
    let treasury_before = receipt["treasury_did"].as_str().unwrap().to_string();
    let keys_before: Vec<Vec<u8>> = ["genesis-trust-root.age", "treasury.age"]
        .iter()
        .map(|f| std::fs::read(data_dir.join(f)).unwrap())
        .collect();

    // Simulate the crash: drop the commit point, keep everything else.
    {
        let store = open_store(daemon_coop_store_path(data_dir));
        let keys: Vec<_> = store
            .scan(b"runtimeroot:receipt:")
            .unwrap()
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert!(
            !keys.is_empty(),
            "fixture: there must be a receipt to remove"
        );
        for k in keys {
            store.delete(&k).unwrap();
        }
        store.flush().unwrap();
    }

    // `show` must say INCOMPLETE, not "nothing happened" and not a receipt.
    let (show, _) = read_receipt_json(data_dir);
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "show must not report a genesis: {text}"
    );
    assert!(
        text.contains("INCOMPLETE"),
        "an interrupted ceremony must be reported as incomplete, not as an \
         untouched directory:\n{text}"
    );

    // A rerun must refuse, and must not mint anything.
    let rerun = provision_runtime_root(data_dir, "Second Attempt");
    let rerun_text = combined(&rerun);
    assert!(!rerun.status.success(), "rerun must refuse: {rerun_text}");
    assert!(
        rerun_text.contains("INCOMPLETE"),
        "the refusal must name the partial state rather than merely saying \
         something already exists:\n{rerun_text}"
    );

    let keys_after: Vec<Vec<u8>> = ["genesis-trust-root.age", "treasury.age"]
        .iter()
        .map(|f| std::fs::read(data_dir.join(f)).unwrap())
        .collect();
    assert_eq!(
        keys_before, keys_after,
        "G7: a refused rerun must not mint or overwrite key material"
    );

    // And the treasury the stores still name must be the original one.
    let coop_rows = rows_with_prefix(&daemon_coop_store_path(data_dir), b"coop:");
    assert_eq!(
        coop_rows.len(),
        1,
        "no second cooperative may have appeared"
    );
    assert!(
        rows_with_prefix(&daemon_ledger_store_path(data_dir), b"ledger:treasury:")
            .iter()
            .any(|k| k.contains(&treasury_before)),
        "the original treasury registration must be untouched"
    );
}

// ---------------------------------------------------------------------------
// Post-hoc removal and tampering
// ---------------------------------------------------------------------------
//
// **These are consistency tests, not crash-boundary tests.** Each takes a
// COMPLETED genesis and removes or alters a component afterwards. That proves
// the inspector detects a state that has stopped being true — a real property,
// since a receipt must not outlive what it claims — but it says nothing about
// what the ceremony writes, or in what order.
//
// Crash-boundary evidence lives in the `failpoint_tests` module inside
// `institution_runtime_root.rs`, where the real `provision_runtime_root_inner` is driven to
// each mutation boundary and the resulting component report is asserted
// exactly. Those pin the ordering; these pin the detection. Reordering two
// writes fails the former and not the latter, which is why both exist.
//
// No claim of transactionality is made either way: three sled databases, two
// keystore files and a config file, with no cross-store transaction.

fn genesis_dir(name: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    assert!(init_identity(dir.path()).status.success());
    assert!(run_init_coop(dir.path()).status.success());
    assert!(provision_runtime_root(dir.path(), name).status.success());
    dir
}

/// Strip the completion marker, then apply a fault, then assert nothing reports
/// a completed genesis and a rerun refuses without minting.
fn assert_partial_state_is_never_complete(dir: &Path, label: &str) {
    let keys_before: Vec<Vec<u8>> = ["genesis-trust-root.age", "treasury.age"]
        .iter()
        .filter(|f| dir.join(f).exists())
        .map(|f| std::fs::read(dir.join(f)).unwrap())
        .collect();

    let (show, _) = read_receipt_json(dir);
    assert!(
        !show.status.success(),
        "{label}: `show` must not report a completed genesis over partial \
         state:\n{}",
        combined(&show)
    );

    let rerun = provision_runtime_root(dir, "Rerun After Fault");
    let text = combined(&rerun);
    assert!(
        !rerun.status.success(),
        "{label}: a rerun over partial state must refuse:\n{text}"
    );

    let keys_after: Vec<Vec<u8>> = ["genesis-trust-root.age", "treasury.age"]
        .iter()
        .filter(|f| dir.join(f).exists())
        .map(|f| std::fs::read(dir.join(f)).unwrap())
        .collect();
    assert_eq!(
        keys_before, keys_after,
        "{label}: a refused rerun must not mint or overwrite key material"
    );
}

fn drop_receipt(dir: &Path) {
    let store = open_store(daemon_coop_store_path(dir));
    for (k, _) in store.scan(b"runtimeroot:receipt:").unwrap() {
        store.delete(&k).unwrap();
    }
    store.flush().unwrap();
}

/// The cooperative store is removed after a completed genesis.
#[test]
fn removing_the_cooperative_store_after_genesis_is_detected() {
    let dir = genesis_dir("Boundary A Coop");
    drop_receipt(dir.path());
    std::fs::remove_dir_all(daemon_coop_store_path(dir.path())).unwrap();
    assert_partial_state_is_never_complete(dir.path(), "cooperative store removed");
}

/// The treasury registration is removed after a completed genesis.
#[test]
fn removing_the_treasury_registration_after_genesis_is_detected() {
    let dir = genesis_dir("Boundary B Coop");
    // The receipt is KEPT so that verification's treasury readback is exercised
    // in the failing direction. Dropping it first would short-circuit
    // `genesis_state` on the artefact scan and leave that check uncovered.
    std::fs::remove_dir_all(daemon_ledger_store_path(dir.path())).unwrap();
    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "boundary B must not be complete:\n{text}"
    );
    assert!(
        text.contains("INCONSISTENT"),
        "the missing treasury registration must be detected on readback:\n{text}"
    );
    drop_receipt(dir.path());
    assert_partial_state_is_never_complete(dir.path(), "treasury registration removed");
}

/// The trust facts are removed after a completed genesis.
#[test]
fn removing_the_trust_facts_after_genesis_is_detected() {
    let dir = genesis_dir("Boundary C Coop");
    // Receipt KEPT, as in boundary B, so the trust-edge readback is exercised.
    std::fs::remove_dir_all(daemon_trust_store_path(dir.path())).unwrap();
    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "boundary C must not be complete:\n{text}"
    );
    assert!(
        text.contains("INCONSISTENT"),
        "the missing trust facts must be detected on readback:\n{text}"
    );
    drop_receipt(dir.path());
    assert_partial_state_is_never_complete(dir.path(), "trust facts removed");
}

/// Boundary D — everything stored, configuration never linked.
///
/// This is the case that motivated moving the commit marker after the config
/// write. If the receipt were still written before the linkage, this state
/// would report COMPLETE while the daemon still fell back to the node DID.
#[test]
fn removing_the_config_linkage_after_genesis_is_detected() {
    let dir = genesis_dir("Boundary D Coop");
    // NOTE: the receipt is deliberately NOT removed here. This is the case that
    // discriminates the commit ordering: if the completion marker were written
    // before the configuration linkage, this state — receipt present, linkage
    // absent — is exactly what a crash during publication would leave, and
    // reporting it as COMPLETE would certify a genesis the daemon cannot
    // consume. `show` re-verifies, so it reports INCONSISTENT instead.
    // Remove the [cooperative] linkage the ceremony published.
    let cfg = dir.path().join("icn.toml");
    let text = std::fs::read_to_string(&cfg).unwrap();
    let stripped: String = text.split("\n[cooperative]\n").next().unwrap().to_string();
    std::fs::write(&cfg, stripped).unwrap();
    assert!(
        !std::fs::read_to_string(&cfg)
            .unwrap()
            .contains("[cooperative]"),
        "fixture: the linkage must actually be gone"
    );

    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "config linkage removed: a receipt must not certify a genesis whose configuration \
         linkage is absent — the daemon would fall back to the node DID:\n{text}"
    );
    assert!(
        text.contains("INCONSISTENT"),
        "config linkage removed: the state must be reported as inconsistent, naming the \
         disagreement, not merely as a missing receipt:\n{text}"
    );
    assert_partial_state_is_never_complete(dir.path(), "config linkage removed");
}

/// A committed receipt must never coexist with a configuration naming a
/// different treasury. This is the invariant that makes the receipt evidence
/// rather than an assertion.
#[test]
fn a_config_naming_a_different_treasury_is_detected_on_readback() {
    let dir = genesis_dir("Config Mismatch Coop");
    let cfg = dir.path().join("icn.toml");
    let text = std::fs::read_to_string(&cfg).unwrap();
    let stranger = icn_identity::KeyPair::generate().unwrap().did().clone();
    let (_, json) = read_receipt_json(dir.path());
    let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
    let real = receipt["treasury_did"].as_str().unwrap();
    let tampered = text.replace(real, stranger.as_str());
    assert_ne!(
        tampered, text,
        "fixture: the treasury must appear in the config"
    );
    std::fs::write(&cfg, tampered).unwrap();

    // `show` must report the disagreement itself. Asserting only that a rerun
    // refuses is not enough: a rerun refuses on "already undergone genesis"
    // whether or not the treasury comparison exists, so deleting that check
    // would leave this test green.
    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "a config naming a different treasury must not read as COMPLETE:\n{text}"
    );
    assert!(
        text.contains("INCONSISTENT") && text.contains("resolves treasury"),
        "the report must name the treasury disagreement, not merely fail:\n{text}"
    );

    // A rerun must not accept this state as a clean slate either.
    let rerun = provision_runtime_root(dir.path(), "Rerun Over Mismatch");
    assert!(
        !rerun.status.success(),
        "a rerun over a tampered configuration must refuse: {}",
        combined(&rerun)
    );
}

/// The cooperative record naming a different treasury than the receipt must be
/// detected. Same shape as the config case, different stored fact.
#[test]
fn a_cooperative_record_naming_a_different_treasury_is_detected_on_readback() {
    let dir = genesis_dir("Record Mismatch Coop");
    let (_, json) = read_receipt_json(dir.path());
    let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
    let coop_id = receipt["cooperative_id"].as_str().unwrap().to_string();

    // Rewrite the stored cooperative to name a different treasury.
    {
        let sled = std::sync::Arc::new(open_store(daemon_coop_store_path(dir.path())));
        let store = icn_coop::CoopStore::new(std::sync::Arc::new(sled.db().clone()));
        let mut coop = store.get_cooperative(&coop_id).unwrap();
        coop.treasury_did = Some(
            icn_identity::KeyPair::generate()
                .unwrap()
                .did()
                .as_str()
                .to_string(),
        );
        store.save_cooperative(&coop).unwrap();
        sled.flush().unwrap();
    }

    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "a cooperative naming a different treasury must not read as COMPLETE:\n{text}"
    );
    // The distinctive substring, not just "INCONSISTENT": asserting only the
    // state word would let a refusal from a *different* guard satisfy this
    // test, which is the exact failure mode two earlier mutants exposed.
    assert!(
        text.contains("names treasury"),
        "the report must name the stored-cooperative treasury disagreement \
         specifically:\n{text}"
    );
}

/// The trust-root binding disagreeing with the receipt must be detected.
#[test]
fn a_cooperative_record_binding_a_different_trust_root_is_detected_on_readback() {
    let dir = genesis_dir("Binding Mismatch Coop");
    let (_, json) = read_receipt_json(dir.path());
    let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
    let coop_id = receipt["cooperative_id"].as_str().unwrap().to_string();

    {
        let sled = std::sync::Arc::new(open_store(daemon_coop_store_path(dir.path())));
        let store = icn_coop::CoopStore::new(std::sync::Arc::new(sled.db().clone()));
        let mut coop = store.get_cooperative(&coop_id).unwrap();
        coop.metadata.insert(
            "genesis.trust_root_did".to_string(),
            icn_identity::KeyPair::generate()
                .unwrap()
                .did()
                .as_str()
                .to_string(),
        );
        store.save_cooperative(&coop).unwrap();
        sled.flush().unwrap();
    }

    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "a rebound trust root must be detected:\n{text}"
    );
    assert!(
        text.contains("binds trust root"),
        "the report must name the trust-root binding disagreement \
         specifically, not merely report some inconsistency:\n{text}"
    );
}

/// A cooperative name is operator-supplied text and must not be able to inject
/// configuration.
///
/// The name here closes the TOML string and tries to append a second
/// `treasury_did` pointing at an attacker-chosen principal. If the writer
/// interpolated the name into a quoted string, the published config would carry
/// that second key and the daemon would resolve the wrong treasury.
#[test]
fn a_hostile_cooperative_name_cannot_inject_configuration() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    let attacker = icn_identity::KeyPair::generate().unwrap().did().clone();
    // Newline-based injection is refused outright by input validation now — a
    // cooperative name containing a control character never reaches the config.
    // Assert that first, then prove the *remaining* TOML-significant characters
    // are escaped rather than merely absent.
    let with_newline = format!("Evil\"\ntreasury_did = \"{}\"", attacker.as_str());
    let refused = combined(&provision_runtime_root(data_dir, &with_newline));
    assert!(
        refused.contains("control characters"),
        "a name containing a newline must be refused before it reaches the \
         configuration:\n{refused}"
    );

    // Everything else TOML-significant, on one line: a closing quote and a
    // forged key assignment, a backslash, a comment marker, a table header, and
    // non-ASCII text.
    let hostile = format!(
        "Evil\" treasury_did = \"{}\" # comment [injected] x = 1 back\\slash \
         Ünïcodé ☭ 你好 name = \"pwned",
        attacker.as_str()
    );

    let out = provision_runtime_root(data_dir, &hostile);
    assert!(
        out.status.success(),
        "a name with TOML metacharacters must be handled, not rejected \
         incidentally: {}",
        combined(&out)
    );

    let (_, json) = read_receipt_json(data_dir);
    let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
    let real_treasury = receipt["treasury_did"].as_str().unwrap();

    let cfg = std::fs::read_to_string(data_dir.join("icn.toml")).unwrap();
    // Parse it the way the daemon's config layer would and check the value that
    // actually resolves — counting occurrences in the text would miss an
    // injection that wins by being parsed last.
    let doc: toml::Value = toml::from_str(&cfg).expect("published config must still parse");
    let resolved = doc
        .get("cooperative")
        .and_then(|c| c.get("treasury_did"))
        .and_then(|v| v.as_str())
        .expect("the [cooperative] section must carry a treasury_did");
    assert_eq!(
        resolved, real_treasury,
        "the resolved treasury must be the one genesis created, not one \
         injected through the cooperative name"
    );
    // The attacker's DID *does* appear in the file — as inert text inside the
    // escaped `name` value, which is exactly what correct escaping looks like.
    // The property that matters is that it parses as part of the name and not
    // as a key. Asserting its mere absence would be testing the wrong thing.
    let name = doc
        .get("cooperative")
        .and_then(|c| c.get("name"))
        .and_then(|v| v.as_str())
        .expect("the [cooperative] section must carry a name");
    assert_eq!(
        name, hostile,
        "the hostile name must round-trip as a single string value"
    );
}

// ---------------------------------------------------------------------------
// Path containment
// ---------------------------------------------------------------------------
//
// A privileged ceremony that writes key material must not be redirectable by
// what it finds on disk.
//
// **What actually protects this, established by mutation.** The N2-A startup
// gate — which the ceremony crosses before it opens or writes anything —
// already refuses a data directory containing a symlink, because sled
// discovery "cannot decide whether it names a database inside or outside this
// data directory". Reverting the ceremony's own `symlink_metadata` check to a
// plain `exists()` did NOT make these tests fail: the gate refuses first.
//
// So these assert the gate's refusal, which is the real boundary. The
// ceremony's own checks remain as defence in depth for the case the gate does
// not cover — `exists()` follows symlinks and returns `false` for a DANGLING
// one, so an existence check written that way would permit a
// create-through-symlink if it were ever reached first. Attributing the
// protection to the wrong layer is how a later refactor moves a write past the
// thing that was actually guarding it.

/// A dangling `treasury.age` symlink must not let genesis write key material
/// outside the data directory.
#[test]
fn a_dangling_treasury_keystore_symlink_cannot_redirect_key_material() {
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    let target = outside.path().join("stolen-treasury.age");
    assert!(!target.exists(), "fixture: the link must dangle");
    std::os::unix::fs::symlink(&target, data_dir.join("treasury.age")).unwrap();
    // The trap, stated as an assertion so the fixture cannot silently stop
    // exercising it: a plain existence check sees nothing here.
    assert!(
        !data_dir.join("treasury.age").exists(),
        "fixture: a dangling symlink must be invisible to `exists()`"
    );

    let out = provision_runtime_root(data_dir, "Symlink Coop");
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "genesis must refuse over a symlinked treasury keystore path:\n{text}"
    );
    assert!(
        text.contains("N2-A startup gate") && text.contains("symlink"),
        "the refusal must come from the containment boundary that actually \
         guards this — the N2-A gate, before any write — and must name the \
         symlink:\n{text}"
    );
    assert!(
        !target.exists(),
        "no key material may be written through the symlink to {}",
        target.display()
    );
}

/// A symlink at the temporary config path must not redirect the configuration
/// write.
#[test]
fn a_symlink_at_the_temp_config_path_cannot_redirect_the_write() {
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    let target = outside.path().join("clobbered.toml");
    std::fs::write(&target, "# untouched\n").unwrap();
    std::os::unix::fs::symlink(&target, data_dir.join("icn.toml.genesis-tmp")).unwrap();

    let out = provision_runtime_root(data_dir, "Temp Symlink Coop");
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "genesis must refuse rather than publish through a symlinked temp \
         path:\n{text}"
    );
    assert!(
        text.contains("N2-A startup gate") && text.contains("symlink"),
        "as above, the refusal is the gate's and must name the symlink:\n{text}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "# untouched\n",
        "the file behind the symlink must not have been written"
    );
}

/// Genesis must refuse a configuration the daemon cannot load, rather than
/// certifying a genesis that would never be consumed.
///
/// This is the case that made a COMPLETE receipt dishonest: `icnd --config`
/// parses the whole file with `Config::from_file`, and `icnd --data-dir` — its
/// only other form — takes `Config::default()`, whose `treasury_did` is `None`,
/// so the node authors as itself. Certifying either as a completed genesis
/// would assert exactly the collapse this work exists to prevent.
///
/// The unloadable config here is the one `init-coop` really generates
/// (icn#2747), reproduced by removing the field the fixture adds.
#[test]
fn genesis_refuses_a_configuration_the_daemon_cannot_load() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    // Put the config back into the state `init-coop` actually leaves it in.
    let path = data_dir.join("icn.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("bootstrap_peers = []\n", "")).unwrap();

    let out = provision_runtime_root(data_dir, "Unloadable Config Coop");
    let msg = combined(&out);
    assert!(
        !out.status.success(),
        "genesis must refuse a configuration the daemon cannot load:\n{msg}"
    );
    assert!(
        msg.contains("not loadable by the daemon") && msg.contains("2747"),
        "the refusal must say the daemon could not load it and name the \
         pre-existing defect:\n{msg}"
    );
    // Refused before anything was written.
    assert!(
        !data_dir.join("treasury.age").exists()
            && !data_dir.join("genesis-trust-root.age").exists(),
        "no key material may be minted when the configuration is unusable"
    );
}

/// A principal named by the receipt must still have key material behind it.
///
/// Without this check, deleting `treasury.age` after a completed ceremony left
/// the receipt reporting COMPLETE for a treasury nothing could ever act as.
#[test]
fn a_receipt_cannot_survive_the_key_material_it_names() {
    let dir = genesis_dir("Key Material Coop");
    assert!(read_receipt_json(dir.path()).0.status.success());

    std::fs::remove_file(dir.path().join("treasury.age")).unwrap();

    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "a receipt must not certify a treasury whose key is gone:\n{text}"
    );
    assert!(
        text.contains("key material is missing"),
        "the failure must name the missing key material:\n{text}"
    );
}

/// The cooperative record must carry its genesis trust root through the
/// production read path, not merely somewhere in the receipt.
///
/// The witness above checks the trust root appears in the stored bytes; this
/// reads the field back the way the daemon's cooperative store does, after the
/// writing process exited, and asserts the exact key. Without this, the trust
/// root could exist only because the receipt asserts it — the thing the design
/// explicitly refuses to do.
#[test]
fn the_cooperative_record_binds_its_trust_root_through_the_production_read_path() {
    let dir = genesis_dir("Binding Readback Cooperative");
    let (_, json) = read_receipt_json(dir.path());
    let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
    let trust_root = receipt["trust_root_did"].as_str().unwrap();
    let treasury = receipt["treasury_did"].as_str().unwrap();
    let coop_id = receipt["cooperative_id"].as_str().unwrap();

    // Opened fresh, after the ceremony's process exited.
    let sled = std::sync::Arc::new(open_store(daemon_coop_store_path(dir.path())));
    let store = icn_coop::CoopStore::new(std::sync::Arc::new(sled.db().clone()));
    let coop = store
        .get_cooperative(coop_id)
        .expect("the cooperative must be readable through CoopStore");

    assert_eq!(
        coop.metadata
            .get("genesis.trust_root_did")
            .map(String::as_str),
        Some(trust_root),
        "the cooperative record must bind its genesis trust root under the \
         namespaced key, readable through the production store"
    );
    assert_eq!(
        coop.treasury_did.as_deref(),
        Some(treasury),
        "and must name the same treasury the receipt does"
    );
    assert_eq!(coop.id, coop_id);
}

// ---------------------------------------------------------------------------
// What TrustGraphType actually does to the ledger's author-trust query
// ---------------------------------------------------------------------------

/// The ledger's author-trust query does not filter on `TrustGraphType`.
///
/// Established by execution rather than inferred from API names. The storage
/// key is `trust/edges/{source}:{target}` with no type component, so one edge
/// exists per ordered pair regardless of type; `computation.rs` never mentions
/// `graph_type`; and the score cache keys on the DID alone. The type is
/// serialized into the edge value and then never consulted by this path.
///
/// All four combinations of `Social` / `EconomicReliability` across the two
/// genesis edges therefore reach the same score. This is a **guard, not an
/// endorsement**: it exists so that introducing a typed query — which would be
/// a reasonable thing to want — cannot silently stop genesis-written facts from
/// reaching the gate. If this test starts failing, the trust model gained type
/// filtering and genesis must declare the graph the ledger actually consumes.
#[tokio::test(flavor = "multi_thread")]
async fn the_ledger_author_trust_query_does_not_filter_on_graph_type() {
    use icn_trust::{TrustEdge, TrustGraph, TrustGraphType, TrustScore};

    let social = TrustGraphType::Social;
    let economic = TrustGraphType::EconomicReliability;

    for (recognition, authority, label) in [
        (social, social, "A: Social / Social"),
        (economic, economic, "B: Economic / Economic"),
        (social, economic, "C: Social / Economic"),
        (economic, social, "D: Economic / Social"),
    ] {
        let dir = TempDir::new().unwrap();
        let node_kp = icn_identity::KeyPair::generate().unwrap();
        let node = node_kp.did().clone();
        let trust_root = icn_identity::KeyPair::generate().unwrap().did().clone();
        let treasury = icn_identity::KeyPair::generate().unwrap().did().clone();
        let full = TrustScore::new(1.0).unwrap();

        // Write, then DROP the writer, then reopen — so the score below is
        // computed from persisted state and not from the writer's own caches.
        {
            let store: std::sync::Arc<dyn icn_store::Store> =
                std::sync::Arc::new(open_store(dir.path().join("trust")));
            let mut graph = TrustGraph::new(store, node.clone());
            graph
                .add_edge(TrustEdge::new_typed(
                    node.clone(),
                    trust_root.clone(),
                    full,
                    recognition,
                ))
                .unwrap();
            graph
                .add_edge(TrustEdge::new_typed(
                    trust_root.clone(),
                    treasury.clone(),
                    full,
                    authority,
                ))
                .unwrap();
        }

        let store: std::sync::Arc<dyn icn_store::Store> =
            std::sync::Arc::new(open_store(dir.path().join("trust")));
        let graph = std::sync::Arc::new(tokio::sync::RwLock::new(TrustGraph::new(
            store.clone(),
            node.clone(),
        )));
        let svc = icn_trust_app::create_service_tokio(graph, node_kp, store);
        let score = svc.trust_score(&treasury.as_str().parse().unwrap());

        assert!(
            score >= 0.1,
            "{label}: the ledger's author-trust threshold must be reached \
             regardless of graph type; scored {score}"
        );
    }
}

/// `show` must state which evidence level it actually reached.
///
/// The two paths verify different things — the ceremony unlocks both keystores
/// and confirms they derive the recorded DIDs; `show` cannot, because it must
/// not prompt for a passphrase. Printing the same confident receipt from both
/// would claim stronger evidence than the read-only path establishes.
#[test]
fn show_says_that_it_did_not_re_verify_key_provenance() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    let created = combined(&provision_runtime_root(
        data_dir,
        "Evidence Level Cooperative",
    ));
    assert!(
        created.contains("key provenance"),
        "the ceremony must say it verified key provenance:\n{created}"
    );
    assert!(
        !created.contains("NOT re-verified"),
        "the ceremony DID verify provenance, so it must not disclaim it:\n{created}"
    );

    let shown = combined(
        &icnctl(data_dir)
            .args(["institution", "runtime-root", "show"])
            .output()
            .unwrap(),
    );
    assert!(
        shown.contains("NOT re-verified") && shown.contains("key provenance"),
        "`show` must disclaim the provenance check it did not perform:\n{shown}"
    );
}

/// The exact boundary of the G4 claim, pinned so the PR cannot overstate it.
///
/// `a_governance_authored_append_crosses_the_real_gate_without_dev_self_trust`
/// opens a fresh `TrustGraph` and scores immediately, which is what a
/// just-started `icnd` does — and it passes. But `TrustGraph`'s reachability
/// bloom filter starts empty, is populated only by in-process `add_edge`, and
/// is never built from storage (`rebuild_reachability_filter` has no production
/// caller). So from the first runtime edge onward, a principal reachable only
/// through *persisted* edges short-circuits to `0.0`.
///
/// That is icn#2750: pre-existing, affecting every out-of-process trust write
/// including `init-coop`'s own bootstrap edges, and NOT introduced here. This
/// test exists so the limit is a measured fact rather than a footnote, and so
/// that the day icn#2750 is fixed this test fails and the claim can be widened
/// deliberately.
///
/// Note the inversion worth remembering: `icnd`'s `ICN_DEV_SELF_TRUST` seed is
/// itself a runtime `add_edge`, so enabling the dev flag would *cause* this
/// failure rather than paper over it.
#[test]
fn persisted_genesis_trust_facts_are_zeroed_once_any_edge_is_added_in_process() {
    let dir = TempDir::new().unwrap();
    let node = icn_identity::KeyPair::generate().unwrap().did().clone();
    let trust_root = icn_identity::KeyPair::generate().unwrap().did().clone();
    let treasury = icn_identity::KeyPair::generate().unwrap().did().clone();
    let full = icn_trust::TrustScore::new(1.0).unwrap();
    let path = dir.path().join("trust");

    // The genesis shape: two edges written, then the writer goes away.
    {
        let store: std::sync::Arc<dyn icn_store::Store> = std::sync::Arc::new(open_store(&path));
        let mut g = icn_trust::TrustGraph::new(store, node.clone());
        g.add_edge(icn_trust::TrustEdge::new(
            node.clone(),
            trust_root.clone(),
            full,
        ))
        .unwrap();
        g.add_edge(icn_trust::TrustEdge::new(
            trust_root,
            treasury.clone(),
            full,
        ))
        .unwrap();
    }

    // A freshly started daemon that has added nothing of its own: correct.
    {
        let store: std::sync::Arc<dyn icn_store::Store> = std::sync::Arc::new(open_store(&path));
        let g = icn_trust::TrustGraph::new(store, node.clone());
        assert!(
            g.compute_trust_score(&treasury).unwrap() >= 0.1,
            "a daemon that has added no edge of its own must see the genesis facts"
        );
    }

    // The same daemon after one unrelated runtime edge, scored for the first
    // time (no cache entry to mask it).
    {
        let store: std::sync::Arc<dyn icn_store::Store> = std::sync::Arc::new(open_store(&path));
        let mut g = icn_trust::TrustGraph::new(store, node.clone());
        let stranger = icn_identity::KeyPair::generate().unwrap().did().clone();
        g.add_edge(icn_trust::TrustEdge::new(node, stranger, full))
            .unwrap();

        let score = g.compute_trust_score(&treasury).unwrap();
        assert_eq!(
            score, 0.0,
            "icn#2750: persisted edges are expected to be zeroed here today. If \
             this now scores {score}, icn#2750 has been fixed — delete this test \
             and widen the G4 claim in the PR body accordingly."
        );
    }
}

/// `show --json` must carry the evidence level on every arm, not only success.
///
/// Without this, reverting the envelope to a bare receipt left the whole suite
/// green — the fixture's unwrapper falls through to the raw text when there is
/// no `receipt` key, so every downstream field read still resolved. A machine
/// surface nothing asserts on is a machine surface that can silently regress.
#[test]
fn show_json_reports_the_evidence_level_on_every_arm() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();

    // Parse RAW stdout. Slicing from the first `{` to the last `}` would prove
    // only that JSON is *somewhere* in the output, which is not what a caller
    // piping into a parser gets. `icnctl` now writes diagnostics to stderr, so
    // stdout must be a document on its own.
    let json_show = |d: &Path| -> (bool, serde_json::Value) {
        let out = icnctl(d)
            .args(["institution", "runtime-root", "show", "--json"])
            .output()
            .unwrap();
        let parsed = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "`show --json` stdout must be parseable JSON with nothing else in it: {e}\n\
                 stdout was: {:?}",
                String::from_utf8_lossy(&out.stdout)
            )
        });
        (out.status.success(), parsed)
    };

    // Nothing has happened here yet.
    assert!(init_identity(data_dir).status.success());
    let (ok, v) = json_show(data_dir);
    assert!(!ok);
    assert_eq!(v["state"], "NOT_STARTED");

    // A completed ceremony: the envelope must state what was and was not checked.
    assert!(run_init_coop(data_dir).status.success());
    assert!(provision_runtime_root(data_dir, "Envelope Cooperative")
        .status
        .success());
    let (ok, v) = json_show(data_dir);
    assert!(ok, "a provisioned runtime root must succeed");
    assert_eq!(v["state"], "READY");
    assert_eq!(v["kind"], "institutional_runtime_root");
    // Tri-state, not booleans: "not_reverified" is a different claim from
    // "mismatch", and a boolean `false` would conflate them.
    assert_eq!(
        v["evidence"]["trust_root_key_provenance"], "not_reverified",
        "`show` does not prompt for a passphrase, so provenance is UNKNOWN here \
         — not known-false"
    );
    assert_eq!(v["evidence"]["treasury_key_provenance"], "not_reverified");
    assert_eq!(v["evidence"]["node_identity"], "not_reverified");
    assert_eq!(v["evidence"]["durable_state"], "verified");
    assert_eq!(v["evidence"]["config_linkage"], "verified");
    assert_eq!(
        v["evidence"]["trust_score_above_ledger_threshold"],
        "verified"
    );
    assert!(
        v["receipt"]["treasury_did"].as_str().is_some(),
        "the receipt must remain available under `receipt`"
    );

    // And a state that stopped being true reports as such, in JSON.
    let cfg = data_dir.join("icn.toml");
    let text = std::fs::read_to_string(&cfg).unwrap();
    std::fs::write(&cfg, text.split("\n[cooperative]\n").next().unwrap()).unwrap();
    let (ok, v) = json_show(data_dir);
    assert!(!ok);
    assert_eq!(v["state"], "INCONSISTENT");
    // INCOMPLETE, via the same commit-marker removal the tampering tests use.
    // The failpoint module remains the evidence for *ordering*; this only pins
    // that the envelope reports the state and carries the component report.
    {
        let d2 = TempDir::new().unwrap();
        assert!(init_identity(d2.path()).status.success());
        assert!(run_init_coop(d2.path()).status.success());
        assert!(
            provision_runtime_root(d2.path(), "Incomplete Envelope Coop")
                .status
                .success()
        );
        drop_receipt(d2.path());

        let (ok, v) = json_show(d2.path());
        assert!(!ok);
        assert_eq!(v["state"], "INCOMPLETE");
        assert_eq!(
            v["components"]["receipt"], false,
            "the component report must show the missing commit marker"
        );
        assert_eq!(
            v["components"]["treasury_key"], true,
            "and must show the key material that is still there"
        );
        assert!(
            v["components"]["trust_store_edges"].as_u64().is_some(),
            "the edge count must be present: {v}"
        );
    }

    // Removing the linkage makes resolution fall back to the node DID, so the
    // diagnosis is the treasury-resolution one — and it correctly names the node
    // as the treasury the daemon would have used. Assert that specific guard,
    // not merely that something went wrong.
    let problem = v["problem"].as_str().unwrap_or_default().to_string();
    assert!(
        problem.contains("resolves treasury"),
        "the JSON arm must carry the same diagnosis the human arm does: {v}"
    );
    let node = v["receipt"]["node_did"].as_str().unwrap();
    assert!(
        problem.contains(node),
        "the diagnosis must name the node DID the daemon would have fallen back \
         to, which is the collapse this work exists to prevent: {problem}"
    );
}

/// Key material replaced by a symlink must be reported as inconsistent, even
/// though `show` never unlocks anything.
///
/// Containment and provenance are different properties. Provenance needs the
/// passphrase and `show` deliberately does not prompt — but "is this a regular
/// file inside the data directory" needs no secret at all, and `is_file()`
/// answers it wrongly because it follows links. `show` does not cross the N2-A
/// gate that refuses symlinks for the create path, so nothing else catches this.
#[test]
fn a_symlinked_treasury_keystore_is_reported_inconsistent_by_show() {
    let dir = genesis_dir("Symlink Presence Coop");
    let outside = TempDir::new().unwrap();

    // A *valid* keystore elsewhere, so this is not merely "the file is missing".
    let elsewhere = outside.path().join("decoy.age");
    std::fs::copy(dir.path().join("treasury.age"), &elsewhere).unwrap();
    std::fs::remove_file(dir.path().join("treasury.age")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, dir.path().join("treasury.age")).unwrap();
    assert!(
        dir.path().join("treasury.age").is_file(),
        "fixture: `is_file()` must be fooled by the link — that is the defect"
    );

    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "a symlinked keystore must not read as READY:\n{text}"
    );
    assert!(
        text.contains("is a symlink"),
        "the report must name the containment violation specifically:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// Concurrency: one ceremony may own a data directory at a time
// ---------------------------------------------------------------------------

/// Two real `icnctl` processes racing on one fresh data root.
///
/// Deterministic because the lock is taken as the *first* state-sensitive step,
/// before the passphrase is read and long before key minting — which takes
/// seconds. Two processes started microseconds apart therefore always overlap at
/// the lock, so exactly one acquires it and the other is refused immediately.
///
/// Without the lock, both would pass preflight against an untouched directory
/// and then race through `AgeKeyStore::init`'s non-atomic existence-check/write,
/// letting one overwrite key material the other generated — and a receipt could
/// be committed for keys no longer on disk.
#[test]
fn two_concurrent_ceremonies_cannot_both_own_a_data_directory() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    let spawn = |name: &str| {
        icnctl(data_dir)
            .args([
                "institution",
                "runtime-root",
                "create",
                "--name",
                name,
                "--yes",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };

    let a = spawn("Racer A");
    // Start the second process *after* the first is certainly past the lock
    // acquisition but still mid-ceremony. Minting two keystores takes seconds,
    // so this margin is large.
    //
    // Starting them simultaneously would not discriminate: both would contend
    // at the same instant, and a lock that was acquired and released
    // immediately would still refuse one of them by luck. This ordering makes
    // the test fail if the lock is not *held* across the ceremony, which is the
    // property that matters.
    std::thread::sleep(std::time::Duration::from_millis(600));
    let b = spawn("Racer B");
    let ra = a.wait_with_output().unwrap();
    let rb = b.wait_with_output().unwrap();

    let succeeded = [&ra, &rb].iter().filter(|o| o.status.success()).count();
    let texts: Vec<String> = [&ra, &rb]
        .iter()
        .map(|o| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        })
        .collect();

    assert_eq!(
        succeeded, 1,
        "exactly one ceremony may own the directory; got {succeeded} successes.\nA:\n{}\nB:\n{}",
        texts[0], texts[1]
    );

    let loser = if ra.status.success() {
        &texts[1]
    } else {
        &texts[0]
    };
    assert!(
        loser.contains("already holds"),
        "the loser must be refused because another ceremony owns the root, not \
         for some incidental later reason:\n{loser}"
    );

    // Exactly one runtime root exists, and its key material backs its receipt —
    // so the loser wrote nothing that could shadow the winner's principals.
    let (show, json) = read_receipt_json(data_dir);
    assert!(show.status.success(), "the winner's root must be READY");
    let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
    let treasury = receipt["treasury_did"].as_str().unwrap();

    let rows = rows_with_prefix(&daemon_coop_store_path(data_dir), b"runtimeroot:receipt:");
    assert_eq!(rows.len(), 1, "exactly one receipt may exist: {rows:?}");

    let ledger_rows = rows_with_prefix(&daemon_ledger_store_path(data_dir), b"ledger:treasury:");
    let primaries: Vec<&String> = ledger_rows
        .iter()
        .filter(|k| !k.starts_with("ledger:treasury:idx:"))
        .collect();
    assert_eq!(
        primaries.len(),
        1,
        "exactly one treasury may have been registered: {primaries:?}"
    );
    assert!(
        primaries[0].contains(treasury),
        "and it must be the one the surviving receipt names"
    );
}

/// The generated cooperative ID must be one the rest of the system accepts.
///
/// The gateway's `validate_coop_id` permits only alphanumerics, hyphens and
/// underscores, so a colon-bearing ID is rejected by `/v1/auth/verify` before
/// authentication — and the default path of `institution bootstrap apply` could
/// then never obtain a token for the cooperative this ceremony just founded.
#[test]
fn the_generated_cooperative_id_is_accepted_by_the_gateway_validator() {
    let dir = genesis_dir("Gateway Valid Id Coop");
    let (_, json) = read_receipt_json(dir.path());
    let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
    let coop_id = receipt["cooperative_id"].as_str().unwrap();

    assert!(
        coop_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
        "the cooperative ID must satisfy the gateway's character rule; got {coop_id:?}"
    );
    assert!(
        !coop_id.contains(':'),
        "a colon is exactly what the gateway rejects: {coop_id:?}"
    );
}

/// A configuration edited to point at a different storage root must stop
/// reading as READY, even though the treasury DID in it is unchanged.
#[test]
fn a_config_repointed_at_another_storage_root_is_detected_on_readback() {
    let dir = genesis_dir("Repointed Root Coop");
    let elsewhere = TempDir::new().unwrap();
    let cfg = dir.path().join("icn.toml");
    let text = std::fs::read_to_string(&cfg).unwrap();
    let patched = text.replace(
        &format!("data_dir = \"{}\"", dir.path().display()),
        &format!("data_dir = \"{}\"", elsewhere.path().display()),
    );
    assert_ne!(patched, text, "fixture: the data_dir line must be present");
    std::fs::write(&cfg, patched).unwrap();

    let (show, _) = read_receipt_json(dir.path());
    let text = combined(&show);
    assert!(
        !show.status.success(),
        "a repointed storage root must not read as READY:\n{text}"
    );
    assert!(
        text.contains("storage root"),
        "the report must name the storage-root disagreement:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// Cross-process exclusion: real processes, real primitive
// ---------------------------------------------------------------------------
//
// The child is this same test binary, re-invoked as a separate process and
// holding the real `DataDirLock`. That makes these witnesses about the process
// invariant — and about the exact primitive the daemon and the ceremony call —
// rather than about Rust's `Drop` or about a foreign lock implementation.

fn lock_file_for(data_dir: &Path) -> PathBuf {
    icn_core::DataDirLock::lock_path(data_dir)
}

/// The child half of the process-death witness.
///
/// Re-invoked as a separate process by the test below. Without the environment
/// variable it returns immediately, so an ordinary `cargo test` run is
/// unaffected.
///
/// This deliberately uses the real `DataDirLock`, not `flock(1)`: the mechanism
/// under test is the Rust primitive both the daemon and the ceremony call, and
/// an earlier witness built on `flock FILE COMMAND` proved nothing because that
/// tool *forks* the command, leaving a grandchild holding the inherited
/// descriptor after the parent was killed.
#[test]
fn data_dir_lock_holder_child() {
    let Ok(root) = std::env::var("ICN_TEST_LOCK_HOLDER_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let _guard = icn_core::DataDirLock::acquire(&root, "test child")
        .expect("the child must acquire the lock");
    std::fs::write(root.join(".holder-ready"), b"ready").unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Spawn that child and wait until it has *established* ownership.
fn spawn_lock_holder(data_dir: &Path) -> std::process::Child {
    let ready = data_dir.join(".holder-ready");
    let _ = std::fs::remove_file(&ready);

    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "data_dir_lock_holder_child", "--nocapture"])
        .env("ICN_TEST_LOCK_HOLDER_ROOT", data_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the test binary must be re-invokable as a child");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !ready.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the lock holder never signalled readiness"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    child
}

/// C4 — a *killed* process releases ownership; the kernel does it, not `Drop`.
///
/// This is the property that makes an advisory lock safe here and a
/// `create_new` marker file or a PID file unsafe: nothing has to run on the
/// dying side, and no operator has to delete anything afterwards.
#[test]
fn killing_the_lock_holder_releases_ownership_without_manual_cleanup() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    let mut holder = spawn_lock_holder(data_dir);

    // Contention is observable from a genuinely separate process.
    let blocked = provision_runtime_root(data_dir, "Blocked While Held");
    assert!(
        !blocked.status.success(),
        "the ceremony must refuse while another process owns the root"
    );
    assert!(
        combined(&blocked).contains("already holds"),
        "and must say so: {}",
        combined(&blocked)
    );

    // Kill it — no cooperative cleanup, no Drop, no unlink.
    holder.kill().unwrap();
    holder.wait().unwrap();

    assert!(
        lock_file_for(data_dir).exists(),
        "the lock file itself survives; only ownership is released"
    );
    let after = provision_runtime_root(data_dir, "Proceeds After Death");
    assert!(
        after.status.success(),
        "a killed holder must not strand the data directory: {}",
        combined(&after)
    );
}

/// `show --json` must emit exactly one JSON document for EVERY outcome —
/// including the failures automation most needs to distinguish.
///
/// Classification itself can fail before any output arm is reached: a receipt
/// schema this binary does not understand, several receipts where there may be
/// one, a store it cannot read. Returning prose there means the machine surface
/// is missing precisely when it matters.
///
/// Every assertion parses the complete stdout bytes. No brace hunting, no
/// tracing-prefix tolerance — `icnctl` writes diagnostics to stderr, so stdout
/// is a document or the contract is broken.
#[test]
fn show_json_emits_one_document_for_every_outcome_including_load_failures() {
    use icn_store::Store;

    let parse = |out: &Output| -> serde_json::Value {
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout must be exactly one JSON document: {e}\nstdout was: {:?}",
                String::from_utf8_lossy(&out.stdout)
            )
        })
    };
    let show = |d: &Path| -> Output {
        icnctl(d)
            .args(["institution", "runtime-root", "show", "--json"])
            .output()
            .unwrap()
    };

    // --- multiple receipts -------------------------------------------------
    {
        let dir = genesis_dir("Two Receipts Coop");
        let (_, json) = read_receipt_json(dir.path());
        let receipt: serde_json::Value = serde_json::from_str(&json).unwrap();
        let store = open_store(daemon_coop_store_path(dir.path()));
        let mut second = receipt.clone();
        second["cooperative_id"] = serde_json::Value::String("coop-second".into());
        store
            .put(
                b"runtimeroot:receipt:coop-second",
                &serde_json::to_vec(&second).unwrap(),
            )
            .unwrap();
        store.flush().unwrap();
        drop(store);

        let out = show(dir.path());
        let v = parse(&out);
        assert!(!out.status.success());
        assert_eq!(v["state"], "ERROR");
        assert_eq!(v["kind"], "institutional_runtime_root");
        assert_eq!(v["error"]["code"], "multiple_receipts");
    }

    // --- unsupported receipt schema ----------------------------------------
    {
        let dir = genesis_dir("Future Schema Coop");
        let store = open_store(daemon_coop_store_path(dir.path()));
        let (key, raw) = store
            .scan(b"runtimeroot:receipt:")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let mut receipt: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        receipt["schema_version"] = serde_json::Value::from(999u64);
        store
            .put(&key, &serde_json::to_vec(&receipt).unwrap())
            .unwrap();
        store.flush().unwrap();
        drop(store);

        let out = show(dir.path());
        let v = parse(&out);
        assert!(!out.status.success());
        assert_eq!(v["error"]["code"], "receipt_schema_unsupported");
    }

    // --- malformed receipt payload -----------------------------------------
    {
        let dir = genesis_dir("Malformed Receipt Coop");
        let store = open_store(daemon_coop_store_path(dir.path()));
        let (key, _) = store
            .scan(b"runtimeroot:receipt:")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        store.put(&key, b"{not json").unwrap();
        store.flush().unwrap();
        drop(store);

        let out = show(dir.path());
        let v = parse(&out);
        assert!(!out.status.success());
        assert_eq!(v["error"]["code"], "receipt_unreadable");
    }

    // --- the ordinary states still hold ------------------------------------
    {
        let dir = TempDir::new().unwrap();
        assert!(init_identity(dir.path()).status.success());
        let v = parse(&show(dir.path()));
        assert_eq!(v["state"], "NOT_STARTED");
        assert_eq!(v["kind"], "institutional_runtime_root");
    }
    {
        let dir = genesis_dir("Ready Envelope Coop");
        let out = show(dir.path());
        let v = parse(&out);
        assert!(out.status.success());
        assert_eq!(v["state"], "READY");
        // Unknown is not false: `show` did not unlock anything.
        assert_eq!(v["evidence"]["node_identity"], "not_reverified");
        assert_eq!(v["evidence"]["treasury_key_provenance"], "not_reverified");
        assert_eq!(v["evidence"]["durable_state"], "verified");
    }
}

/// Key material this ceremony mints is not world-readable.
///
/// `AgeKeyStore` writes at the process umask, so a keystore normally lands 0664.
/// The content is encrypted at rest, so that exposes ciphertext rather than
/// keys — defence-in-depth, not a plaintext leak — but a privileged founding
/// operation should not create new secret files that way.
///
/// Scoped to what this ceremony owns: the node's own `identity.age` predates it
/// and is left alone, because changing `AgeKeyStore` for every caller is
/// icn#2748's to own.
#[test]
fn minted_key_material_is_not_world_readable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = genesis_dir("Key Mode Coop");
    for name in ["treasury.age", "genesis-trust-root.age"] {
        let mode = std::fs::metadata(dir.path().join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "{name} must be owner-only; got {mode:o}");
    }

    // The pre-existing node keystore is deliberately untouched — that is
    // icn#2748's scope, not this ceremony's.
    assert!(
        dir.path().join("identity.age").exists(),
        "fixture: the node keystore must exist"
    );
}

/// The gateway secret may come from the environment, exactly as it does for the
/// daemon — provisioning must not be stricter than the process it validates for.
///
/// `icnd` applies `--gateway-jwt-secret`, then `ICN_GATEWAY_JWT_SECRET`, then
/// the file, and only then validates. Validating the raw file would reject the
/// standard `init-coop` flow — whose own instructions recommend the environment
/// variable — and push an operator into persisting a plaintext secret purely to
/// satisfy a provisioning check.
#[test]
fn the_gateway_secret_may_come_from_the_environment_as_it_does_for_the_daemon() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());

    // Put the configuration back to the template's shape: gateway enabled with
    // the secret only commented out.
    let cfg = data_dir.join("icn.toml");
    let text = std::fs::read_to_string(&cfg).unwrap();
    let without = text.replace(
        "jwt_secret = \"fixture-jwt-secret-not-a-real-credential\"",
        "# jwt_secret = \"CHANGE_ME\"  # Set this before starting!",
    );
    assert_ne!(without, text, "fixture: the secret line must be present");
    std::fs::write(&cfg, without).unwrap();

    // Without the environment variable, provisioning must refuse — the daemon
    // would not start either.
    let refused = combined(&provision_runtime_root(data_dir, "No Secret Coop"));
    assert!(
        refused.contains("jwt_secret"),
        "an absent gateway secret must be refused, as the daemon refuses it:\n{refused}"
    );

    // With it supplied the way the daemon accepts it, provisioning proceeds.
    let out = icnctl(data_dir)
        .env("ICN_GATEWAY_JWT_SECRET", "supplied-through-the-environment")
        .args([
            "institution",
            "runtime-root",
            "create",
            "--name",
            "Env Secret Coop",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "the environment-supplied secret must be honoured:\n{}",
        combined(&out)
    );

    // And the plaintext secret was never written into the configuration.
    let after = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        !after.contains("supplied-through-the-environment"),
        "the secret must not be persisted into the configuration:\n{after}"
    );
}
