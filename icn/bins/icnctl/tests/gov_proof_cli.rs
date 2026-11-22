//! `icnctl gov proof export|verify` against a real persisted decision (icn#2465, first slice).
//!
//! The decision is produced by the production close path (a `GovernanceActor` with a node
//! signing key) into `<data-dir>/store/governance`, and the producer is stopped before the real
//! `icnctl` binary exports it. Verification then runs with no usable data directory, the RPC
//! endpoint aimed at a closed port and proxy variables aimed at nothing: any dependency on a
//! store, daemon, gateway or network would surface as a failure there.
//!
//! Export is held to the same contract as the other offline commands (see
//! `n2a_gate_coverage_test.rs`): it refuses while the daemon holds the store, and refuses a data
//! directory the N2-A gate refuses — writing nothing in either case.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig, ProposalId,
    ProposalPayload, ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{actor::GovernanceActor, manager::GovernanceManager};
use icn_identity::{Did, IdentityBundle};
use icn_store::{SledStore, Store};

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

fn icnctl(data_dir: &Path, args: &[&str]) -> Output {
    Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(data_dir)
        .args(args)
        .output()
        .unwrap()
}

/// Verify with no usable data directory, the RPC endpoint on a closed port and proxies pointing
/// nowhere — so a pass cannot have come from a store, a daemon or the network.
fn verify_offline(bundle: &Path, signer: &str) -> Output {
    Command::new(icnctl_bin())
        .args(["--data-dir", "/nonexistent/icn-offline-verify"])
        .args(["--endpoint", "127.0.0.1:9"])
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .args(["gov", "proof", "verify", "--bundle"])
        .arg(bundle)
        .args(["--expect-signer", signer])
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Run the production close path into `<data_dir>/store/governance`, then stop the producer.
///
/// The producer gets its own runtime, shut down after the close, so the actor task and every
/// store handle it held are gone before the binary runs — as when the daemon has exited.
fn real_decision(data_dir: &Path) -> (ProposalId, Did) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let out = runtime.block_on(produce(data_dir));
    runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    let store_path = data_dir.join("store").join("governance");
    for _ in 0..50 {
        if SledStore::open(&store_path).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    out
}

async fn produce(data_dir: &Path) -> (ProposalId, Did) {
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();
    let signing_key = Arc::new(ed25519_dalek::SigningKey::from_bytes(
        &bundle.keypair().unwrap().to_signing_key_bytes(),
    ));
    let store_path = data_dir.join("store").join("governance");
    std::fs::create_dir_all(&store_path).unwrap();
    let proposal_id = {
        let store = Arc::new(SledStore::open(&store_path).unwrap());
        let gossip = GossipActor::spawn(did.clone(), None);
        gossip.write().await.create_topic(Topic::new(
            "governance:proposal".to_string(),
            AccessControl::Public,
        ));
        let handle = GovernanceActor::spawn(
            did.clone(),
            store.clone(),
            gossip,
            Arc::new(StaticMembershipResolver::new()),
            None,
            Some(signing_key),
        )
        .await
        .unwrap();
        let manager = GovernanceManager::with_handle(
            Arc::new(handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
        );
        let domain = GovernanceDomainId("cli-offline-proof".to_string());
        manager
            .create_domain(
                domain.clone(),
                "CLI offline proof".to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![did.clone()]),
            )
            .await
            .unwrap();
        let id = manager
            .create_proposal(
                ProposalId("_ignored".to_string()),
                domain,
                did.clone(),
                "Adopt a fictional statement".to_string(),
                "A declarative text proposal.".to_string(),
                ProposalPayload::Text {
                    body: "We adopt this fictional statement.".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();
        manager.open_proposal(id.clone(), 3600).await.unwrap();
        manager
            .cast_vote(id.clone(), did.clone(), VoteChoice::For, None)
            .await
            .unwrap();
        manager
            .close_proposal_with_suspension(id.clone(), None, None, None)
            .await
            .unwrap();
        assert!(handle.get_proof(&id).await.unwrap().is_some());
        Store::flush(store.as_ref()).unwrap();
        id
    };
    (proposal_id, did)
}

/// BLOCKED end to end: after a real close the N2-A gate refuses the data directory (the uncovered
/// `gov:repl:seq:v1` keyspace; see `the_startup_gate_is_clear_after_a_real_close` in
/// icn-governance-actor), so `export` correctly refuses. Kept as the contract for when the
/// keyspace is dispositioned; `verify_exit_codes_offline` covers the verifier meanwhile.
#[test]
#[ignore = "BLOCKED: N2-A gate refuses gov:repl:seq:v1 after a real close — owner disposition pending (#2627)"]
fn export_then_verify_offline_with_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let (id, did) = real_decision(dir.path());
    let out_a = dir.path().join("a.bundle.json");
    let out_b = dir.path().join("b.bundle.json");

    let exp = icnctl(
        dir.path(),
        &[
            "gov",
            "proof",
            "export",
            "--proposal-id",
            &id.0,
            "--out",
            out_a.to_str().unwrap(),
        ],
    );
    assert!(exp.status.success(), "export: {}", text(&exp));
    let exp2 = icnctl(
        dir.path(),
        &[
            "gov",
            "proof",
            "export",
            "--proposal-id",
            &id.0,
            "--out",
            out_b.to_str().unwrap(),
        ],
    );
    assert!(exp2.status.success(), "second export: {}", text(&exp2));
    let bundle = std::fs::read(&out_a).unwrap();
    assert_eq!(
        bundle,
        std::fs::read(&out_b).unwrap(),
        "export is byte-stable"
    );

    let ok = verify_offline(&out_a, did.as_str());
    assert_eq!(ok.status.code(), Some(0), "verify: {}", text(&ok));
    let v: serde_json::Value = serde_json::from_slice(&ok.stdout).unwrap();
    assert_eq!(v["verified"], true);
    assert_eq!(v["outcome"], "accepted");
    assert_eq!(v["proposal_id"], id.0.as_str());

    // One flipped byte of the carried proof: integrity (exit 2).
    let mut b: serde_json::Value = serde_json::from_slice(&bundle).unwrap();
    let mut proof = hex::decode(b["proof_bytes_hex"].as_str().unwrap()).unwrap();
    let i = proof.len() / 4;
    proof[i] ^= 1;
    b["proof_bytes_hex"] = serde_json::Value::String(hex::encode(&proof));
    let tampered = dir.path().join("tampered.json");
    std::fs::write(&tampered, serde_json::to_vec(&b).unwrap()).unwrap();
    let t = verify_offline(&tampered, did.as_str());
    assert_eq!(t.status.code(), Some(2), "tampered: {}", text(&t));
    assert!(text(&t).contains("integrity_mismatch"));

    // A different pinned node: untrusted (exit 5).
    let stranger = IdentityBundle::generate().unwrap().did().to_string();
    let u = verify_offline(&out_a, &stranger);
    assert_eq!(u.status.code(), Some(5), "untrusted: {}", text(&u));

    // Another schema version: refused as unsupported (exit 6).
    let mut s: serde_json::Value = serde_json::from_slice(&bundle).unwrap();
    s["schema"] = serde_json::Value::String("urn:icn:contract:governance-proof-bundle:v2".into());
    let v2 = dir.path().join("v2.json");
    std::fs::write(&v2, serde_json::to_vec(&s).unwrap()).unwrap();
    let sv = verify_offline(&v2, did.as_str());
    assert_eq!(sv.status.code(), Some(6), "schema: {}", text(&sv));

    // The original, untouched, still verifies.
    assert_eq!(verify_offline(&out_a, did.as_str()).status.code(), Some(0));
}

/// A fresh data directory whose governance store holds only the proof row a real close wrote.
///
/// A real post-close directory is refused by the N2-A gate today (see the ignored test above),
/// so tests of the export command itself — its lock, store read and file write — need a
/// directory the gate clears. The proof bytes are still the production ones, copied verbatim.
fn proof_only_data_dir() -> (tempfile::TempDir, ProposalId, Did) {
    let src = tempfile::tempdir().unwrap();
    let (id, did) = real_decision(src.path());
    let key = icn_governance::proof_bundle::stored_proof_key(&id.0);
    let bytes = {
        let store = SledStore::open(src.path().join("store").join("governance")).unwrap();
        store.get(&key).unwrap().expect("a proof was persisted")
    };
    let dir = tempfile::tempdir().unwrap();
    let store_path = dir.path().join("store").join("governance");
    std::fs::create_dir_all(&store_path).unwrap();
    {
        let store = SledStore::open(&store_path).unwrap();
        store.put(&key, &bytes).unwrap();
        store.flush().unwrap();
    }
    (dir, id, did)
}

/// The export command end to end on a directory the gate clears, then the verifier offline.
#[test]
fn export_command_writes_a_bundle_that_verifies_offline() {
    let (dir, id, did) = proof_only_data_dir();
    let out_a = dir.path().join("a.json");
    let out_b = dir.path().join("b.json");
    for out in [&out_a, &out_b] {
        let exp = icnctl(
            dir.path(),
            &[
                "gov",
                "proof",
                "export",
                "--proposal-id",
                &id.0,
                "--out",
                out.to_str().unwrap(),
            ],
        );
        assert!(exp.status.success(), "export: {}", text(&exp));
    }
    assert_eq!(
        std::fs::read(&out_a).unwrap(),
        std::fs::read(&out_b).unwrap(),
        "export is byte-stable"
    );
    let ok = verify_offline(&out_a, did.as_str());
    assert_eq!(ok.status.code(), Some(0), "verify: {}", text(&ok));

    // An unknown proposal is an error, not an empty bundle.
    let missing = dir.path().join("missing.json");
    let exp = icnctl(
        dir.path(),
        &[
            "gov",
            "proof",
            "export",
            "--proposal-id",
            "no-such-proposal",
            "--out",
            missing.to_str().unwrap(),
        ],
    );
    assert!(!exp.status.success());
    assert!(
        text(&exp).contains("no decision proof is stored"),
        "{}",
        text(&exp)
    );
    assert!(!missing.exists());
}

#[test]
fn export_refuses_while_the_store_is_held_and_writes_nothing() {
    // A directory the gate clears, so the refusal can only come from the held store.
    let (dir, id, _did) = proof_only_data_dir();
    // Stand in for a running daemon: hold the governance store open.
    let held = SledStore::open(dir.path().join("store").join("governance")).unwrap();
    let out = dir.path().join("x.json");
    let exp = icnctl(
        dir.path(),
        &[
            "gov",
            "proof",
            "export",
            "--proposal-id",
            &id.0,
            "--out",
            out.to_str().unwrap(),
        ],
    );
    drop(held);
    assert!(
        !exp.status.success(),
        "must refuse while the store is held: {}",
        text(&exp)
    );
    // The gate is the first thing to open the store, so it is what meets the held lock. The
    // pin is on the lock, which separates this from the gate's UNCOVERED-keyspace refusal.
    assert!(
        text(&exp).contains("could not acquire lock") && !text(&exp).contains("UNCOVERED"),
        "refused for the held store, not something else: {}",
        text(&exp)
    );
    assert!(!out.exists(), "a refused export writes nothing");
}

#[test]
fn export_refuses_a_data_directory_the_gate_refuses() {
    // The alias-collision fixture `n2a_gate_coverage_test.rs` uses for its siblings.
    let dir = tempfile::tempdir().unwrap();
    let a = icn_identity::KeyPair::generate().unwrap().did().clone();
    let alias: Did = format!("did:icn:f{}", hex::encode(a.identifier_bytes().unwrap()))
        .parse()
        .unwrap();
    assert_eq!(a, alias);
    let peer = icn_identity::KeyPair::generate().unwrap().did().clone();
    {
        let store = SledStore::open(dir.path().join("store")).unwrap();
        for src in [&a, &alias] {
            let key = format!("trust/edges/{}:{}", src.as_str(), peer.as_str());
            store.put(key.as_bytes(), b"{}").unwrap();
        }
        store.db().flush().unwrap();
    }
    let out = dir.path().join("x.json");
    let exp = icnctl(
        dir.path(),
        &[
            "gov",
            "proof",
            "export",
            "--proposal-id",
            "p",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert!(!exp.status.success(), "must refuse: {}", text(&exp));
    assert!(
        text(&exp).contains("N2-A"),
        "refusal names the gate: {}",
        text(&exp)
    );
    assert!(!out.exists());
}

/// The verifier CLI on a bundle exported from a real persisted decision, offline.
///
/// The bundle is exported by the library from the real store (the step the gate currently
/// blocks in the CLI); everything after that is the real binary with no store, daemon or network.
#[test]
fn verify_exit_codes_offline() {
    let dir = tempfile::tempdir().unwrap();
    let (id, did) = real_decision(dir.path());
    let stored = {
        let store = SledStore::open(dir.path().join("store").join("governance")).unwrap();
        store
            .get(&icn_governance::proof_bundle::stored_proof_key(&id.0))
            .unwrap()
            .expect("a proof was persisted")
    };
    let bundle = icn_governance::proof_bundle::export_bundle(&id.0, &stored).unwrap();
    let path = dir.path().join("bundle.json");
    std::fs::write(&path, &bundle).unwrap();

    let ok = verify_offline(&path, did.as_str());
    assert_eq!(ok.status.code(), Some(0), "verify: {}", text(&ok));
    let v: serde_json::Value = serde_json::from_slice(&ok.stdout).unwrap();
    assert_eq!(v["verified"], true);
    assert_eq!(v["outcome"], "accepted");

    let mut b: serde_json::Value = serde_json::from_slice(&bundle).unwrap();
    let mut proof = hex::decode(b["proof_bytes_hex"].as_str().unwrap()).unwrap();
    let i = proof.len() / 4;
    proof[i] ^= 1;
    b["proof_bytes_hex"] = serde_json::Value::String(hex::encode(&proof));
    let tampered = dir.path().join("tampered.json");
    std::fs::write(&tampered, serde_json::to_vec(&b).unwrap()).unwrap();
    assert_eq!(
        verify_offline(&tampered, did.as_str()).status.code(),
        Some(2)
    );

    let stranger = IdentityBundle::generate().unwrap().did().to_string();
    assert_eq!(verify_offline(&path, &stranger).status.code(), Some(5));
    assert_eq!(
        verify_offline(&path, "did:icn:not-a-key").status.code(),
        Some(7)
    );
    assert_eq!(verify_offline(&path, did.as_str()).status.code(), Some(0));
}
