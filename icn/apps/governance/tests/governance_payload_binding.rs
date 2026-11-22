//! Signed payload binding (Gate 3 minimal capability): a peer holding only the exported proof can
//! verify, offline, WHICH membership change an accepted decision made.
//!
//! Every proof here comes from the production close path (a `GovernanceActor` with a node signing
//! key), stored, the producer stopped, then exported and verified with only the bundle bytes and a
//! pinned signer DID. The binding is the attesting node's statement; it is not institutional
//! authority (who may decide is the domain's membership rule).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::proof_bundle::{
    expect_membership, export_bundle, verify_bundle, BundleVerifyError, VERIFIED_PAYLOAD_CLAIM,
};
use icn_governance::{
    GovernanceDomainId, GovernanceOps, GovernanceParams, GovernancePayloadBinding,
    GovernanceProofV2, MembershipAction, MembershipConfig, ProposalId, ProposalPayload,
    ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{
    actor::GovernanceActor, manager::GovernanceManager, state_store::read_stored_proof_bytes,
};
use icn_identity::{Did, IdentityBundle};
use icn_store::SledStore;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Receipt backend for the actor's execution-closure gate (Invariant 7): membership proposals
/// are execution-required, so the actor refuses to close them without one, exactly as the
/// daemon wires one in. Nothing here is under test — the proof store is real sled.
struct NullReceiptBackend;

impl icn_governance_actor::receipt_backend::GovernanceReceiptBackend for NullReceiptBackend {
    fn put_governance(&self, _: &icn_governance::GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(
        &self,
        _: &icn_kernel_api::AllocationReceipt,
    ) -> Result<icn_kernel_api::Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn put_institutional_effect(
        &self,
        _: &icn_governance_actor::institutional_effect::InstitutionalEffectRecord,
    ) -> Result<(), String> {
        Ok(())
    }
    fn list_institutional_effects_by_proposal(
        &self,
        _: &str,
    ) -> Result<Vec<icn_governance_actor::institutional_effect::InstitutionalEffectRecord>, String>
    {
        Ok(vec![])
    }
}

struct ClosedDecision {
    _dir: tempfile::TempDir,
    store_path: std::path::PathBuf,
    proposal_id: ProposalId,
    node_did: String,
    node_key: ed25519_dalek::SigningKey,
}

fn close(choice: VoteChoice, payload: ProposalPayload) -> ClosedDecision {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("producer runtime");
    let closed = runtime.block_on(produce(choice, payload));
    runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    closed
}

/// A real, independently generated DID for the admitted subject.
fn subject() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

fn membership(action: MembershipAction, member: &Did) -> ProposalPayload {
    ProposalPayload::Membership {
        action,
        member: member.clone(),
    }
}

async fn produce(choice: VoteChoice, payload: ProposalPayload) -> ClosedDecision {
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
            Some(signing_key.clone()),
        )
        .await
        .expect("spawn actor");
        handle
            .install_receipt_store(Arc::new(NullReceiptBackend))
            .await;
        let manager = GovernanceManager::with_handle(
            Arc::new(handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
        );
        let domain = GovernanceDomainId("gate3-domain".to_string());
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
                payload,
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
        node_key: (*signing_key).clone(),
    }
}

impl ClosedDecision {
    fn stored(&self) -> Vec<u8> {
        let store = reopen(&self.store_path);
        read_stored_proof_bytes(&store, &self.proposal_id)
            .expect("read")
            .expect("a proof was persisted")
    }
    fn export(&self) -> Vec<u8> {
        export_bundle(&self.proposal_id.0, &self.stored()).expect("export")
    }
}

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

/// Rewrite the proof inside a bundle and fix the checksum, so only deeper layers can object.
fn with_proof(bundle: &[u8], f: impl FnOnce(&mut GovernanceProofV2)) -> Vec<u8> {
    let mut b: Value = serde_json::from_slice(bundle).unwrap();
    let mut proof: GovernanceProofV2 =
        serde_json::from_slice(&hex::decode(b["proof_bytes_hex"].as_str().unwrap()).unwrap())
            .unwrap();
    f(&mut proof);
    let raw = serde_json::to_vec(&proof).unwrap();
    b["proof_sha256"] = Value::String(hex::encode(Sha256::digest(&raw)));
    b["proof_bytes_hex"] = Value::String(hex::encode(&raw));
    serde_json::to_vec(&b).unwrap()
}

fn spelled_base16(did: &Did) -> String {
    let key = did.to_verifying_key().unwrap().to_bytes();
    // The DID body is a multibase-encoded identifier; re-spell the same bytes in base16.
    let body = did.as_str().strip_prefix("did:icn:").unwrap();
    let (_, bytes) = multibase::decode(body).unwrap();
    let alias = format!(
        "did:icn:{}",
        multibase::encode(multibase::Base::Base16Lower, &bytes)
    );
    let parsed = Did::from_str(&alias).expect("alias parses");
    assert_eq!(
        parsed.to_verifying_key().unwrap().to_bytes(),
        key,
        "same key"
    );
    assert_ne!(alias, did.as_str(), "a different spelling");
    alias
}

// --- the admission a peer can verify -------------------------------------------------------

#[test]
fn an_accepted_admission_binds_the_admitted_member() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let verified = verify_bundle(&d.export(), &d.node_did).expect("verifies");
    assert_eq!(verified.payload_claim, Some(VERIFIED_PAYLOAD_CLAIM));
    assert!(verified.payload_hash.is_some());
    expect_membership(&verified, MembershipAction::Add, b.as_str(), None).expect("B was admitted");
}

#[test]
fn the_expected_member_matches_by_key_not_spelling() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let verified = verify_bundle(&d.export(), &d.node_did).unwrap();
    expect_membership(&verified, MembershipAction::Add, &spelled_base16(&b), None)
        .expect("an alias spelling of B is B");
}

#[test]
fn a_stranger_or_the_wrong_action_is_not_what_was_decided() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let verified = verify_bundle(&d.export(), &d.node_did).unwrap();
    let stranger = subject();
    assert!(matches!(
        expect_membership(&verified, MembershipAction::Add, stranger.as_str(), None),
        Err(BundleVerifyError::SubjectMismatch { .. })
    ));
    assert!(matches!(
        expect_membership(&verified, MembershipAction::Remove, b.as_str(), None),
        Err(BundleVerifyError::SubjectMismatch { .. })
    ));
    assert!(matches!(
        expect_membership(&verified, MembershipAction::Add, "did:icn:not-a-key", None),
        Err(BundleVerifyError::SubjectMismatch { .. })
    ));
}

#[test]
fn a_revocation_binds_remove() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Remove, &b));
    let verified = verify_bundle(&d.export(), &d.node_did).unwrap();
    expect_membership(&verified, MembershipAction::Remove, b.as_str(), None)
        .expect("removal bound");
}

#[test]
fn a_rejected_admission_binds_nothing() {
    let b = subject();
    let d = close(VoteChoice::Against, membership(MembershipAction::Add, &b));
    let stored: GovernanceProofV2 = serde_json::from_slice(&d.stored()).unwrap();
    assert!(
        stored.payload_binding.is_none(),
        "nothing was decided to bind"
    );
    let verified = verify_bundle(&d.export(), &d.node_did).expect("the rejection still verifies");
    assert_eq!(
        expect_membership(&verified, MembershipAction::Add, b.as_str(), None),
        Err(BundleVerifyError::PayloadUnbound)
    );
}

// --- forgeries the checksum fix-up cannot carry --------------------------------------------

#[test]
fn swapping_the_admitted_member_breaks_the_binding_signature() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let mallory = subject();
    let forged = with_proof(&d.export(), |p| {
        let binding = p.payload_binding.as_mut().unwrap();
        binding.payload_json = binding.payload_json.replace(b.as_str(), mallory.as_str());
    });
    assert_eq!(
        verify_bundle(&forged, &d.node_did).unwrap_err(),
        BundleVerifyError::PayloadBindingInvalidSignature
    );
}

#[test]
fn a_binding_moved_onto_another_decision_is_refused() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let forged = with_proof(&d.export(), |p| {
        p.payload_binding.as_mut().unwrap().decision_hash[0] ^= 1;
    });
    assert!(matches!(
        verify_bundle(&forged, &d.node_did).unwrap_err(),
        BundleVerifyError::PayloadBindingMismatch(_)
    ));
}

/// A binding another node signed is not the pinned signer's statement: the decision still
/// verifies, but it is unbound for this verifier, so an expected member is refused.
#[test]
fn a_binding_signed_by_a_stranger_leaves_the_proof_unbound() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let stranger = IdentityBundle::generate().unwrap();
    let key =
        ed25519_dalek::SigningKey::from_bytes(&stranger.keypair().unwrap().to_signing_key_bytes());
    let mallory = subject();
    let forged = with_proof(&d.export(), |p| {
        p.payload_binding = Some(GovernancePayloadBinding::sign(
            p.receipt.decision_hash,
            serde_json::to_string(&membership(MembershipAction::Add, &mallory)).unwrap(),
            1,
            stranger.did().to_string(),
            &key,
        ));
    });
    let verified = verify_bundle(&forged, &d.node_did).expect("the decision itself verifies");
    assert!(
        verified.payload.is_none(),
        "a stranger's binding is not used"
    );
    assert_eq!(
        expect_membership(&verified, MembershipAction::Add, mallory.as_str(), None),
        Err(BundleVerifyError::PayloadUnbound)
    );
}

/// The realistic graft: the same node genuinely signs a binding for a different decision, and
/// it is moved onto this one. Only decision-hash equality can refuse it — the signature is valid.
#[test]
fn a_genuinely_signed_binding_from_another_decision_is_refused() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let mallory = subject();
    let mut other_decision = [0u8; 32];
    other_decision[0] = 7;
    let genuine = GovernancePayloadBinding::sign(
        other_decision,
        serde_json::to_string(&membership(MembershipAction::Add, &mallory)).unwrap(),
        1,
        d.node_did.clone(),
        &d.node_key,
    );
    assert!(
        genuine.verify(&d.node_key.verifying_key()),
        "precondition: validly signed"
    );
    let forged = with_proof(&d.export(), |p| p.payload_binding = Some(genuine));
    assert!(matches!(
        verify_bundle(&forged, &d.node_did).unwrap_err(),
        BundleVerifyError::PayloadBindingMismatch(_)
    ));
}

/// Signed payload text with a duplicate `member` key: a lenient parser keeps the last one, a
/// strict one refuses. The verifier must refuse, so no reader can find a different member.
#[test]
fn a_bound_payload_with_duplicate_keys_is_refused() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let mallory = subject();
    let forged = with_proof(&d.export(), |p| {
        let json = format!(
            r#"{{"Membership":{{"action":"Add","member":"{}","member":"{}"}}}}"#,
            b.as_str(),
            mallory.as_str()
        );
        p.payload_binding = Some(GovernancePayloadBinding::sign(
            p.receipt.decision_hash,
            json,
            1,
            d.node_did.clone(),
            &d.node_key,
        ));
    });
    assert!(matches!(
        verify_bundle(&forged, &d.node_did).unwrap_err(),
        BundleVerifyError::ProofMalformed(_)
    ));
}

#[test]
fn the_accepting_domain_can_be_pinned() {
    let b = subject();
    let d = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let verified = verify_bundle(&d.export(), &d.node_did).unwrap();
    expect_membership(
        &verified,
        MembershipAction::Add,
        b.as_str(),
        Some("gate3-domain"),
    )
    .expect("decided in gate3-domain");
    assert!(matches!(
        expect_membership(
            &verified,
            MembershipAction::Add,
            b.as_str(),
            Some("sandbox")
        ),
        Err(BundleVerifyError::SubjectMismatch { .. })
    ));
}

#[test]
fn a_non_membership_decision_is_not_an_admission() {
    let b = subject();
    let d = close(
        VoteChoice::For,
        ProposalPayload::Text {
            body: format!("We welcome {b}"),
        },
    );
    let verified = verify_bundle(&d.export(), &d.node_did).unwrap();
    assert!(verified.payload.is_some(), "the text payload is bound");
    assert!(matches!(
        expect_membership(&verified, MembershipAction::Add, b.as_str(), None),
        Err(BundleVerifyError::SubjectMismatch { .. })
    ));
}

#[test]
fn a_binding_on_a_rejected_decision_is_refused() {
    let b = subject();
    let accepted = close(VoteChoice::For, membership(MembershipAction::Add, &b));
    let rejected = close(VoteChoice::Against, membership(MembershipAction::Add, &b));
    // Graft an accepted decision's binding onto a rejection, pointing it at the rejection's
    // decision hash: the outcome check refuses it before any signature is consulted.
    let binding: GovernanceProofV2 = serde_json::from_slice(&accepted.stored()).unwrap();
    let forged = with_proof(&rejected.export(), |p| {
        let mut moved = binding.payload_binding.clone().unwrap();
        moved.decision_hash = p.receipt.decision_hash;
        p.payload_binding = Some(moved);
    });
    assert!(matches!(
        verify_bundle(&forged, &rejected.node_did).unwrap_err(),
        BundleVerifyError::PayloadBindingMismatch(_)
    ));
}

// --- compatibility -------------------------------------------------------------------------

#[test]
fn proofs_without_a_binding_keep_their_exact_bytes() {
    let b = subject();
    let d = close(VoteChoice::Against, membership(MembershipAction::Add, &b));
    let stored = d.stored();
    let parsed: GovernanceProofV2 = serde_json::from_slice(&stored).unwrap();
    assert_eq!(
        serde_json::to_vec(&parsed).unwrap(),
        stored,
        "no new field is written"
    );
    assert!(!String::from_utf8(stored)
        .unwrap()
        .contains("payload_binding"));
}
