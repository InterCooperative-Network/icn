//! Portable governance proof bundles (icn#2465, first slice), exported from a real close.
//!
//! Every bundle here comes from the production path: a `GovernanceActor` with a node signing
//! key closes a voted proposal, and the close persists a signed `GovernanceProofV2` in the
//! governance store. The producer is then stopped — actor and store dropped, the sled directory
//! reopened with no actor — and the stored bytes are exported. Verification receives only the
//! bundle bytes and a DID the verifier pins; there is no daemon, store or network in that path.
//!
//! The negative cases each break one layer: integrity of the carried bytes, consistency of the
//! decision, authenticity of the signature, and trust in the signer. Several also recompute the
//! bundle checksum after tampering, because the checksum travels with the bundle and so proves
//! nothing on its own; those must still fail, at the decision or signature layer.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::proof_bundle::{
    export_bundle, verify_bundle, BundleVerifyError, GOVERNANCE_PROOF_BUNDLE_SCHEMA_V1,
    VERIFIED_CLAIM,
};
use icn_governance::{
    GovernanceDecisionAttestation, GovernanceDecisionReceipt, GovernanceDomainId, GovernanceOps,
    GovernanceParams, GovernanceProofV2, MembershipConfig, ProofOutcome, ProposalId,
    ProposalPayload, ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{
    actor::GovernanceActor, manager::GovernanceManager, state_store::read_stored_proof_bytes,
};
use icn_identity::IdentityBundle;
use icn_store::SledStore;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// A closed proposal whose producer has been stopped.
struct ClosedDecision {
    _dir: tempfile::TempDir,
    store_path: std::path::PathBuf,
    proposal_id: ProposalId,
    node_did: String,
}

/// Run the production close path to completion, then stop the producer.
///
/// The producer runs on its own tokio runtime, which is shut down after the close: that cancels
/// every task it spawned (the actor included), so their store handles drop — the in-process
/// equivalent of the daemon exiting. Dropping only the handles would leave the actor task alive
/// and holding the store.
fn close_a_real_decision(choice: VoteChoice) -> ClosedDecision {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("producer runtime");
    let closed = runtime.block_on(produce(choice));
    runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    closed
}

async fn produce(choice: VoteChoice) -> ClosedDecision {
    let dir = tempfile::tempdir().expect("tempdir");
    // The daemon's layout: `<data-dir>/store/governance`.
    let store_path = dir.path().join("store").join("governance");
    std::fs::create_dir_all(&store_path).expect("store dir");
    let bundle = IdentityBundle::generate().expect("identity");
    let did = bundle.did().clone();
    let signing_key = Arc::new(ed25519_dalek::SigningKey::from_bytes(
        &bundle.keypair().expect("keypair").to_signing_key_bytes(),
    ));

    let proposal_id = {
        let store = Arc::new(SledStore::open(&store_path).expect("sled"));
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
        .expect("spawn actor");
        let manager = GovernanceManager::with_handle(
            Arc::new(handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
        );
        let domain = GovernanceDomainId("offline-proof".to_string());
        manager
            .create_domain(
                domain.clone(),
                "Offline proof".to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![did.clone()]),
            )
            .await
            .expect("create_domain");
        let proposal_id = manager
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
            .expect("create_proposal");
        manager
            .open_proposal(proposal_id.clone(), 3600)
            .await
            .expect("open_proposal");
        manager
            .cast_vote(proposal_id.clone(), did.clone(), choice, None)
            .await
            .expect("cast_vote");
        manager
            .close_proposal_with_suspension(proposal_id.clone(), None, None, None)
            .await
            .expect("close via the production actor path");
        assert!(
            handle
                .get_proof(&proposal_id)
                .await
                .expect("get_proof")
                .is_some(),
            "precondition: the close persisted a signed GovernanceProofV2"
        );
        icn_store::Store::flush(store.as_ref()).expect("flush");
        proposal_id
        // manager, handle, gossip and store drop here: the producer is gone.
    };

    ClosedDecision {
        _dir: dir,
        store_path,
        proposal_id,
        node_did: did.to_string(),
    }
}

impl ClosedDecision {
    /// Export with no actor: open the store fresh, read the exact stored bytes, bundle them.
    fn export(&self) -> Vec<u8> {
        let store = reopen(&self.store_path);
        let bytes = read_stored_proof_bytes(&store, &self.proposal_id)
            .expect("read")
            .expect("a proof was persisted");
        export_bundle(&self.proposal_id.0, &bytes).expect("export")
    }
}

/// Open the store with the producer stopped.
///
/// sled releases its file lock from a background thread after the last handle drops, so an
/// immediate reopen can briefly see `WouldBlock`; retry within a bound instead of racing it.
fn reopen(path: &std::path::Path) -> SledStore {
    let mut last = None;
    for _ in 0..50 {
        match SledStore::open(path) {
            Ok(store) => return store,
            Err(e) => last = Some(e),
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("store did not reopen with the producer stopped: {last:?}");
}

// --- tampering helpers --------------------------------------------------------------------

fn bundle_json(bundle: &[u8]) -> Value {
    serde_json::from_slice(bundle).unwrap()
}

fn to_bytes(v: &Value) -> Vec<u8> {
    let mut b = serde_json::to_vec(v).unwrap();
    b.push(b'\n');
    b
}

/// Rewrite the carried proof with `f`, then recompute the bundle checksum so that only the
/// decision / signature layers can catch the change.
fn tamper_proof_and_fix_checksum(bundle: &[u8], f: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut b = bundle_json(bundle);
    let proof_bytes = hex::decode(b["proof_bytes_hex"].as_str().unwrap()).unwrap();
    let mut proof: Value = serde_json::from_slice(&proof_bytes).unwrap();
    f(&mut proof);
    let new_bytes = serde_json::to_vec(&proof).unwrap();
    b["proof_bytes_hex"] = Value::String(hex::encode(&new_bytes));
    b["proof_sha256"] = Value::String(hex::encode(Sha256::digest(&new_bytes)));
    to_bytes(&b)
}

fn err(bundle: &[u8], pinned: &str) -> BundleVerifyError {
    verify_bundle(bundle, pinned).expect_err("must be rejected")
}

// --- positive -----------------------------------------------------------------------------

#[test]
fn a_real_accepted_decision_verifies_offline_against_the_pinned_node() {
    let decision = close_a_real_decision(VoteChoice::For);
    let bundle = decision.export();

    let verified = verify_bundle(&bundle, &decision.node_did).expect("verifies offline");
    assert_eq!(verified.schema, GOVERNANCE_PROOF_BUNDLE_SCHEMA_V1);
    assert_eq!(verified.proposal_id, decision.proposal_id.0);
    assert_eq!(verified.domain_id, "offline-proof");
    assert_eq!(verified.outcome, ProofOutcome::Accepted);
    assert_eq!(verified.attested_by, decision.node_did);
    assert_eq!(verified.claim, VERIFIED_CLAIM);
    assert!(
        verified
            .claim
            .contains("not proof that an institution authorized"),
        "a pass must state its bound"
    );

    // The bundle carries the stored bytes verbatim, not a re-serialization.
    let store = reopen(&decision.store_path);
    let stored = read_stored_proof_bytes(&store, &decision.proposal_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        hex::decode(bundle_json(&bundle)["proof_bytes_hex"].as_str().unwrap()).unwrap(),
        stored
    );
}

#[test]
fn a_real_rejected_decision_is_also_verifiable() {
    let decision = close_a_real_decision(VoteChoice::Against);
    let verified = verify_bundle(&decision.export(), &decision.node_did).expect("verifies");
    assert_eq!(verified.outcome, ProofOutcome::Rejected);
}

#[test]
fn export_is_byte_stable() {
    let decision = close_a_real_decision(VoteChoice::For);
    let first = decision.export();
    let second = decision.export();
    assert_eq!(first, second, "same stored proof, same bundle bytes");
    assert!(first.ends_with(b"\n"));
}

// --- integrity ----------------------------------------------------------------------------

#[test]
fn one_flipped_byte_of_committed_content_fails_integrity() {
    let decision = close_a_real_decision(VoteChoice::For);
    let bundle = decision.export();
    let mut b = bundle_json(&bundle);
    let mut proof = hex::decode(b["proof_bytes_hex"].as_str().unwrap()).unwrap();
    // A byte inside the serialized receipt (the proof JSON opens with the receipt object).
    let i = proof.len() / 4;
    proof[i] ^= 0x01;
    b["proof_bytes_hex"] = Value::String(hex::encode(&proof));
    assert_eq!(
        err(&to_bytes(&b), &decision.node_did),
        BundleVerifyError::IntegrityMismatch
    );
    // Restoring the byte restores the pass.
    assert!(verify_bundle(&bundle, &decision.node_did).is_ok());
}

// --- decision consistency (checksum recomputed, so only deeper layers can catch it) ----------

#[test]
fn a_changed_outcome_is_caught_even_with_a_fixed_checksum() {
    let decision = close_a_real_decision(VoteChoice::For);
    let tampered = tamper_proof_and_fix_checksum(&decision.export(), |p| {
        p["receipt"]["outcome"] = Value::String("rejected".into());
    });
    assert_eq!(
        err(&tampered, &decision.node_did),
        BundleVerifyError::DecisionHashMismatch
    );
}

#[test]
fn a_changed_decision_hash_is_caught_even_with_a_fixed_checksum() {
    let decision = close_a_real_decision(VoteChoice::For);
    let tampered = tamper_proof_and_fix_checksum(&decision.export(), |p| {
        let h = p["receipt"]["decision_hash"].as_array_mut().unwrap();
        h[0] = Value::from((h[0].as_u64().unwrap() ^ 1) as u8);
    });
    assert_eq!(
        err(&tampered, &decision.node_did),
        BundleVerifyError::DecisionHashMismatch
    );
}

#[test]
fn a_changed_proposal_id_is_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    let mut b = bundle_json(&decision.export());
    b["proposal_id"] = Value::String("some-other-proposal".into());
    assert!(matches!(
        err(&to_bytes(&b), &decision.node_did),
        BundleVerifyError::ProposalIdMismatch { .. }
    ));
}

#[test]
fn an_attestation_for_another_decision_is_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    let tampered = tamper_proof_and_fix_checksum(&decision.export(), |p| {
        let h = p["attestations"][0]["decision_hash"]
            .as_array_mut()
            .unwrap();
        h[0] = Value::from((h[0].as_u64().unwrap() ^ 1) as u8);
    });
    assert_eq!(
        err(&tampered, &decision.node_did),
        BundleVerifyError::AttestationDecisionMismatch
    );
}

// --- authenticity and trust -----------------------------------------------------------------

#[test]
fn a_forged_signature_is_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    let tampered = tamper_proof_and_fix_checksum(&decision.export(), |p| {
        let s = p["attestations"][0]["signature"].as_array_mut().unwrap();
        s[0] = Value::from((s[0].as_u64().unwrap() ^ 1) as u8);
    });
    assert_eq!(
        err(&tampered, &decision.node_did),
        BundleVerifyError::InvalidSignature
    );
}

#[test]
fn a_valid_proof_by_an_unpinned_node_is_untrusted() {
    let decision = close_a_real_decision(VoteChoice::For);
    let stranger = IdentityBundle::generate().unwrap().did().to_string();
    assert!(matches!(
        err(&decision.export(), &stranger),
        BundleVerifyError::UntrustedSigner { .. }
    ));
}

#[test]
fn a_proof_with_no_attestation_is_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    let tampered = tamper_proof_and_fix_checksum(&decision.export(), |p| {
        p["attestations"] = Value::Array(vec![]);
    });
    assert_eq!(
        err(&tampered, &decision.node_did),
        BundleVerifyError::NoAttestation
    );
}

#[test]
fn an_unusable_pinned_signer_is_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    assert!(matches!(
        err(&decision.export(), "did:icn:not-a-key"),
        BundleVerifyError::InvalidPinnedSigner(_)
    ));
}

// --- format and version ---------------------------------------------------------------------

#[test]
fn missing_and_unknown_fields_are_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    let bundle = decision.export();

    let mut missing = bundle_json(&bundle);
    missing.as_object_mut().unwrap().remove("proof_sha256");
    assert!(matches!(
        err(&to_bytes(&missing), &decision.node_did),
        BundleVerifyError::Malformed(_)
    ));

    let mut extra = bundle_json(&bundle);
    extra["signer_hint"] = Value::String(decision.node_did.clone());
    assert!(matches!(
        err(&to_bytes(&extra), &decision.node_did),
        BundleVerifyError::Malformed(_)
    ));

    assert!(matches!(
        err(b"not json", &decision.node_did),
        BundleVerifyError::Malformed(_)
    ));
}

#[test]
fn another_schema_version_is_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    let mut b = bundle_json(&decision.export());
    b["schema"] = Value::String("urn:icn:contract:governance-proof-bundle:v2".into());
    assert!(matches!(
        err(&to_bytes(&b), &decision.node_did),
        BundleVerifyError::SchemaMismatch { .. }
    ));
}

// --- review follow-ups: the forgeries a checksum fix-up alone cannot express -----------------

/// The realistic forgery: change the outcome AND recompute the receipt's decision hash, so the
/// receipt is internally consistent. Only the attestation binding catches it.
#[test]
fn a_changed_outcome_with_a_recomputed_decision_hash_is_caught_by_the_attestation() {
    let decision = close_a_real_decision(VoteChoice::For);
    let tampered = tamper_proof_and_fix_checksum(&decision.export(), |p| {
        let mut proof: GovernanceProofV2 = serde_json::from_value(p.clone()).unwrap();
        proof.receipt.outcome = ProofOutcome::Rejected;
        proof.receipt.decision_hash = GovernanceDecisionReceipt::compute_decision_hash(
            &proof.receipt.proposal_id,
            &proof.receipt.domain_id,
            proof.receipt.outcome,
            &proof.receipt.vote_tally,
            &proof.receipt.vote_hash,
        );
        assert!(
            proof.verify_receipt(),
            "the forged receipt is self-consistent"
        );
        *p = serde_json::to_value(&proof).unwrap();
    });
    assert_eq!(
        err(&tampered, &decision.node_did),
        BundleVerifyError::AttestationDecisionMismatch
    );
}

/// With several attestations, the pinned signer's is the one checked, wherever it sits.
#[test]
fn the_pinned_attestation_is_found_behind_a_strangers() {
    let decision = close_a_real_decision(VoteChoice::For);
    let stranger = IdentityBundle::generate().unwrap();
    let stranger_key =
        ed25519_dalek::SigningKey::from_bytes(&stranger.keypair().unwrap().to_signing_key_bytes());
    let reordered = tamper_proof_and_fix_checksum(&decision.export(), |p| {
        let mut proof: GovernanceProofV2 = serde_json::from_value(p.clone()).unwrap();
        let theirs = GovernanceDecisionAttestation::sign(
            proof.receipt.decision_hash,
            stranger.did().to_string(),
            1,
            &stranger_key,
        );
        proof.attestations.insert(0, theirs);
        *p = serde_json::to_value(&proof).unwrap();
    });
    let verified = verify_bundle(&reordered, &decision.node_did).expect("pinned one is second");
    assert_eq!(verified.attested_by, decision.node_did);
    assert_ne!(
        verified.attested_at, 1,
        "the stranger's attestation was not the one used"
    );
}

/// A file carrying a key twice is refused, so no two verifiers can read different bundles from it.
#[test]
fn duplicate_bundle_keys_are_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    let bundle = decision.export();
    let text = String::from_utf8(bundle).unwrap();
    let dup = text.replacen('{', "{\"proposal_id\":\"other\",", 1);
    assert!(matches!(
        err(dup.as_bytes(), &decision.node_did),
        BundleVerifyError::Malformed(_)
    ));
}

/// One decision has exactly one bundle byte form: uppercase hex is refused, not normalized.
#[test]
fn uppercase_hex_is_refused() {
    let decision = close_a_real_decision(VoteChoice::For);
    let mut b = bundle_json(&decision.export());
    let up = b["proof_bytes_hex"].as_str().unwrap().to_uppercase();
    b["proof_bytes_hex"] = Value::String(up);
    assert_eq!(
        err(&to_bytes(&b), &decision.node_did),
        BundleVerifyError::ProofEncoding
    );
    let mut b = bundle_json(&decision.export());
    let up = b["proof_sha256"].as_str().unwrap().to_uppercase();
    b["proof_sha256"] = Value::String(up);
    assert!(matches!(
        err(&to_bytes(&b), &decision.node_did),
        BundleVerifyError::Malformed(_)
    ));
}

// --- the N2-A startup gate after a real close ------------------------------------------------

/// RED: a node that has closed one proposal must still pass its own N2-A startup gate.
///
/// It does not. The close path writes the durable replication-sequence row
/// `gov:repl:seq:v1:` ‖ len ‖ author DID ‖ len ‖ domain (apps/governance/src/replication_sequence.rs),
/// a principal-bearing keyspace that `icn-store`'s N2-A registry neither covers nor defers. The
/// gate therefore refuses the data directory: `icnd` refuses to start again, and the offline
/// `icnctl gov proof export` (which runs the same gate) refuses too. Reproduced with the release
/// daemon. Choosing the keyspace's disposition belongs to its owner (#2627 registry), so this
/// slice records the contract rather than registering the keyspace itself.
#[test]
#[ignore = "RED: N2-A registry does not cover gov:repl:seq:v1 — owner disposition pending (#2627)"]
fn the_startup_gate_is_clear_after_a_real_close() {
    let decision = close_a_real_decision(VoteChoice::For);
    let data_dir = decision.store_path.parent().unwrap().parent().unwrap();
    let verdict = icn_store::n2a_startup_gate::enforce(data_dir, std::time::SystemTime::now());
    assert!(
        verdict.is_ok(),
        "a node that closed one proposal must still pass its startup gate: {verdict:?}"
    );
}
