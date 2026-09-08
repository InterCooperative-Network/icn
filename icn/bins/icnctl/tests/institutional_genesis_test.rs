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
    let text = std::fs::read_to_string(&path).unwrap();
    if text.contains("bootstrap_peers") {
        return;
    }
    let patched = text.replace("[network]\n", "[network]\nbootstrap_peers = []\n");
    assert_ne!(patched, text, "fixture: [network] section must be present");
    std::fs::write(&path, patched).unwrap();
}

fn run_genesis(data_dir: &Path, name: &str) -> Output {
    icnctl(data_dir)
        .args(["institution", "genesis", "create", "--name", name, "--yes"])
        .output()
        .unwrap()
}

fn read_receipt_json(data_dir: &Path) -> (Output, String) {
    let out = icnctl(data_dir)
        .args(["institution", "genesis", "show", "--json"])
        .output()
        .unwrap();
    let raw = String::from_utf8_lossy(&out.stdout).into_owned();
    // `show` now re-verifies the receipt against durable state, and opening the
    // ledger store emits a tracing line on stdout ahead of the payload. That is
    // pre-existing `icnctl` logging behaviour shared by every `--json`
    // subcommand, not something this ceremony introduced, so the fixture slices
    // out the JSON object rather than changing where the whole binary logs.
    let text = match (raw.find('{'), raw.rfind('}')) {
        (Some(a), Some(b)) if b > a => raw[a..=b].to_string(),
        _ => raw,
    };
    (out, text)
}

/// Scan a sled database opened *after* the writing process exited.
fn rows_with_prefix(path: &Path, prefix: &[u8]) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }
    let store = SledStore::open(path).unwrap();
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
    let out = run_genesis(data_dir, "Riverside Bakery Cooperative");
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
        use icn_store::Store;
        let store = SledStore::open(daemon_coop_store_path(data_dir)).unwrap();
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
    let out = run_genesis(data_dir, "Author Gate Cooperative");
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
        std::sync::Arc::new(SledStore::open(daemon_trust_store_path(data_dir)).unwrap());
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
        std::sync::Arc::new(SledStore::open(daemon_ledger_store_path(data_dir)).unwrap());
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
    let out = run_genesis(dir.path(), "No Authority Coop");
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
    assert!(run_genesis(data_dir, "First Cooperative").status.success());

    let (_, first) = read_receipt_json(data_dir);
    let first: serde_json::Value = serde_json::from_str(&first).unwrap();
    let first_treasury = first["treasury_did"].as_str().unwrap().to_string();

    let out = run_genesis(data_dir, "Second Cooperative");
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "a second genesis must refuse:\n{text}"
    );
    // Pin *which* guard refuses. Two independent guards happen to cover this —
    // the prior-ceremony check, and the config linkage check refusing a second
    // `[cooperative]` section — which is real defence in depth, but a test that
    // accepted either could not tell the intended guard from an accident. A
    // mutation removing `refuse_if_already_started` survived until this
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

    let out = run_genesis(data_dir, "Divergent Root Coop");
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
    assert!(run_genesis(data_dir, "Ephemeral Coop").status.success());
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
            "genesis",
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
    use icn_store::Store;

    if let Some((source, target)) = remove {
        let store = SledStore::open(daemon_trust_store_path(data_dir)).unwrap();
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
        std::sync::Arc::new(SledStore::open(daemon_trust_store_path(data_dir)).unwrap());
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
    assert!(run_genesis(dir.path(), name).status.success());
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
    use icn_store::Store;

    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    assert!(init_identity(data_dir).status.success());
    assert!(run_init_coop(data_dir).status.success());
    assert!(run_genesis(data_dir, "Interrupted Cooperative")
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
        let store = SledStore::open(daemon_coop_store_path(data_dir)).unwrap();
        let keys: Vec<_> = store
            .scan(b"genesis:receipt:")
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
    let rerun = run_genesis(data_dir, "Second Attempt");
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
// `institution_genesis.rs`, where the real `run_genesis_inner` is driven to
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
    assert!(run_genesis(dir.path(), name).status.success());
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

    let rerun = run_genesis(dir, "Rerun After Fault");
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
    use icn_store::Store;
    let store = SledStore::open(daemon_coop_store_path(dir)).unwrap();
    for (k, _) in store.scan(b"genesis:receipt:").unwrap() {
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

    // A rerun must not accept this state as a clean slate either.
    let rerun = run_genesis(dir.path(), "Rerun Over Mismatch");
    assert!(
        !rerun.status.success(),
        "a rerun over a tampered configuration must refuse: {}",
        combined(&rerun)
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
    // Every TOML-significant shape at once: a closing quote and a forged key
    // (the injection proper), a newline, a backslash, a comment marker, a table
    // header, and non-ASCII text.
    let hostile = format!(
        "Evil\"\ntreasury_did = \"{}\"\n# comment\n[injected]\nx = 1\nback\\slash \
         Ünïcodé ☭ 你好 name = \"pwned",
        attacker.as_str()
    );

    let out = run_genesis(data_dir, &hostile);
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

    let out = run_genesis(data_dir, "Symlink Coop");
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

    let out = run_genesis(data_dir, "Temp Symlink Coop");
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

    let out = run_genesis(data_dir, "Unloadable Config Coop");
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
    let sled = std::sync::Arc::new(SledStore::open(daemon_coop_store_path(dir.path())).unwrap());
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
                std::sync::Arc::new(SledStore::open(dir.path().join("trust")).unwrap());
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
            std::sync::Arc::new(SledStore::open(dir.path().join("trust")).unwrap());
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

    let created = combined(&run_genesis(data_dir, "Evidence Level Cooperative"));
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
            .args(["institution", "genesis", "show"])
            .output()
            .unwrap(),
    );
    assert!(
        shown.contains("NOT re-verified") && shown.contains("key provenance"),
        "`show` must disclaim the provenance check it did not perform:\n{shown}"
    );
}
