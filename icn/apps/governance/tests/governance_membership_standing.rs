//! Membership standing at a verifier-supplied time, from real persisted governance decisions.
//!
//! One node, one domain, a sequence of real decisions (admission, removal, re-admission) closed
//! at distinct signed decision times. Each is exported from the stopped producer's store and
//! verified under the pinned node; `evaluate_standing_at` then reports what those decisions show
//! at a time the *verifier* chooses — never current membership, never inferred from absence.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::proof_bundle::{
    evaluate_standing_at, export_bundle, verify_bundle, BundleVerifyError, VerifiedGovernanceProof,
    STANDING_CLAIM,
};
use icn_governance::{
    GovernanceDomainId, GovernanceOps, GovernanceParams, GovernanceProofV2, MembershipAction,
    MembershipConfig, ProposalId, ProposalPayload, ProposalScope, StaticMembershipResolver,
    VoteChoice,
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

const DOMAIN: &str = "standing-domain";

/// A node's sequence of closed decisions, producer stopped.
struct History {
    _dir: tempfile::TempDir,
    store_path: std::path::PathBuf,
    node_did: String,
    proposals: Vec<ProposalId>,
}

fn membership(action: MembershipAction, member: &Did) -> ProposalPayload {
    ProposalPayload::Membership {
        action,
        member: member.clone(),
    }
}

/// Close each (payload, vote) in order on ONE node, at least a second apart so the signed
/// decision times differ, then stop the producer.
fn decide(steps: Vec<(ProposalPayload, VoteChoice)>) -> History {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let h = runtime.block_on(produce(steps));
    runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    h
}

async fn produce(steps: Vec<(ProposalPayload, VoteChoice)>) -> History {
    let dir = tempfile::tempdir().unwrap();
    let store_path = dir.path().join("store").join("governance");
    std::fs::create_dir_all(&store_path).unwrap();
    let bundle = IdentityBundle::generate().unwrap();
    let did = bundle.did().clone();
    let key = Arc::new(ed25519_dalek::SigningKey::from_bytes(
        &bundle.keypair().unwrap().to_signing_key_bytes(),
    ));
    let mut proposals = Vec::new();
    {
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
            Some(key),
        )
        .await
        .unwrap();
        handle
            .install_receipt_store(Arc::new(NullReceiptBackend))
            .await;
        let manager = GovernanceManager::with_handle(
            Arc::new(handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
        );
        let domain = GovernanceDomainId(DOMAIN.to_string());
        manager
            .create_domain(
                domain.clone(),
                "Standing".to_string(),
                "cooperative_default".to_string(),
                GovernanceParams::new(50, 50, 3600),
                MembershipConfig::static_list(vec![did.clone()]),
            )
            .await
            .unwrap();
        for (i, (payload, choice)) in steps.into_iter().enumerate() {
            if i > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            }
            let id = manager
                .create_proposal(
                    ProposalId("_ignored".to_string()),
                    domain.clone(),
                    did.clone(),
                    format!("step {i}"),
                    "standing test".to_string(),
                    payload,
                    ProposalScope::Local,
                )
                .await
                .unwrap();
            manager.open_proposal(id.clone(), 3600).await.unwrap();
            manager
                .cast_vote(id.clone(), did.clone(), choice, None)
                .await
                .unwrap();
            manager
                .close_proposal_with_suspension(id.clone(), None, None, None)
                .await
                .unwrap();
            proposals.push(id);
        }
        icn_store::Store::flush(store.as_ref()).unwrap();
    }
    History {
        _dir: dir,
        store_path,
        node_did: did.to_string(),
        proposals,
    }
}

impl History {
    fn bundle(&self, i: usize) -> Vec<u8> {
        let store = reopen(&self.store_path);
        let bytes = read_stored_proof_bytes(&store, &self.proposals[i])
            .unwrap()
            .expect("stored proof");
        export_bundle(&self.proposals[i].0, &bytes).unwrap()
    }
    fn verified(&self, i: usize) -> VerifiedGovernanceProof {
        verify_bundle(&self.bundle(i), &self.node_did).expect("verifies")
    }
}

fn reopen(path: &std::path::Path) -> SledStore {
    for _ in 0..50 {
        if let Ok(s) = SledStore::open(path) {
            return s;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("store did not reopen");
}

fn subject() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

// --- boundaries --------------------------------------------------------------------------

#[test]
fn before_the_admission_was_decided_it_is_not_yet_in_effect() {
    let b = subject();
    let h = decide(vec![(
        membership(MembershipAction::Add, &b),
        VoteChoice::For,
    )]);
    let admit = h.verified(0);
    let t = admit.bound_at.unwrap();
    assert_eq!(
        evaluate_standing_at(&admit, &[], b.as_str(), DOMAIN, t - 1),
        Err(BundleVerifyError::NotYetEffective {
            at: t - 1,
            admitted_at: t
        })
    );
    let ok = evaluate_standing_at(&admit, &[], b.as_str(), DOMAIN, t).expect("at the decision");
    assert_eq!(ok.status, "admitted_no_revocation_observed");
    assert_eq!(ok.claim, STANDING_CLAIM);
    assert_eq!(ok.revocations_considered, 0);
}

#[test]
fn a_supplied_removal_revokes_from_its_decision_time_and_not_before() {
    let b = subject();
    let h = decide(vec![
        (membership(MembershipAction::Add, &b), VoteChoice::For),
        (membership(MembershipAction::Remove, &b), VoteChoice::For),
    ]);
    let (admit, removal) = (h.verified(0), h.verified(1));
    let (ta, tr) = (admit.bound_at.unwrap(), removal.bound_at.unwrap());
    assert!(tr > ta, "precondition: distinct decision times");
    // Historically valid between the two decisions, even with the removal in hand.
    let mid = evaluate_standing_at(
        &admit,
        std::slice::from_ref(&removal),
        b.as_str(),
        DOMAIN,
        tr - 1,
    )
    .expect("valid before the removal");
    assert_eq!(mid.revocations_considered, 1);
    assert_eq!(
        evaluate_standing_at(
            &admit,
            std::slice::from_ref(&removal),
            b.as_str(),
            DOMAIN,
            tr
        ),
        Err(BundleVerifyError::RevokedByEvidence {
            at: tr,
            removed_at: tr
        })
    );
    // Without the removal, the verifier cannot know: still only "no revocation observed".
    let blind = evaluate_standing_at(&admit, &[], b.as_str(), DOMAIN, tr + 100).unwrap();
    assert_eq!(blind.status, "admitted_no_revocation_observed");
    assert!(blind.claim.contains("not proof of current membership"));
}

#[test]
fn a_removal_decided_before_a_re_admission_does_not_revoke_it() {
    let b = subject();
    let h = decide(vec![
        (membership(MembershipAction::Add, &b), VoteChoice::For),
        (membership(MembershipAction::Remove, &b), VoteChoice::For),
        (membership(MembershipAction::Add, &b), VoteChoice::For),
    ]);
    let (admit, removal, readmit) = (h.verified(0), h.verified(1), h.verified(2));
    let (tr, t) = (removal.bound_at.unwrap(), readmit.bound_at.unwrap());
    evaluate_standing_at(
        &readmit,
        std::slice::from_ref(&removal),
        b.as_str(),
        DOMAIN,
        t + 10,
    )
    .expect("the earlier removal predates this admission");
    // The OLD admission stays revoked: standing is per admission, and the caller must pass the
    // admission they mean.
    assert_eq!(
        evaluate_standing_at(&admit, &[removal], b.as_str(), DOMAIN, t + 10),
        Err(BundleVerifyError::RevokedByEvidence {
            at: t + 10,
            removed_at: tr
        })
    );
}

// --- evidence that is not what it claims -------------------------------------------------

#[test]
fn a_rejected_removal_is_not_revocation_evidence() {
    let b = subject();
    let h = decide(vec![
        (membership(MembershipAction::Add, &b), VoteChoice::For),
        (
            membership(MembershipAction::Remove, &b),
            VoteChoice::Against,
        ),
    ]);
    let (admit, rejected) = (h.verified(0), h.verified(1));
    assert_eq!(
        evaluate_standing_at(&admit, &[rejected], b.as_str(), DOMAIN, u64::MAX),
        Err(BundleVerifyError::PayloadUnbound),
        "a rejected removal decided nothing; supplying it is a caller error"
    );
}

#[test]
fn removal_evidence_for_someone_else_or_another_domain_is_refused() {
    let b = subject();
    let c = subject();
    let h = decide(vec![
        (membership(MembershipAction::Add, &b), VoteChoice::For),
        (membership(MembershipAction::Remove, &c), VoteChoice::For),
    ]);
    let (admit, other) = (h.verified(0), h.verified(1));
    assert!(matches!(
        evaluate_standing_at(&admit, &[other], b.as_str(), DOMAIN, u64::MAX),
        Err(BundleVerifyError::SubjectMismatch { .. })
    ));
    assert!(matches!(
        evaluate_standing_at(&admit, &[], b.as_str(), "another-domain", u64::MAX),
        Err(BundleVerifyError::SubjectMismatch { .. })
    ));
}

#[test]
fn removal_evidence_from_another_signer_is_refused() {
    let b = subject();
    let ours = decide(vec![(
        membership(MembershipAction::Add, &b),
        VoteChoice::For,
    )]);
    let theirs = decide(vec![(
        membership(MembershipAction::Remove, &b),
        VoteChoice::For,
    )]);
    let removal = theirs.verified(0);
    assert!(matches!(
        evaluate_standing_at(&ours.verified(0), &[removal], b.as_str(), DOMAIN, u64::MAX),
        Err(BundleVerifyError::UntrustedSigner { .. })
    ));
}

#[test]
fn equal_decision_times_are_refused_as_ambiguous() {
    let b = subject();
    let h = decide(vec![
        (membership(MembershipAction::Add, &b), VoteChoice::For),
        (membership(MembershipAction::Remove, &b), VoteChoice::For),
    ]);
    let admit = h.verified(0);
    let mut removal = h.verified(1);
    removal.bound_at = admit.bound_at; // a verified value forced equal: order is unknowable
    assert!(matches!(
        evaluate_standing_at(&admit, &[removal], b.as_str(), DOMAIN, u64::MAX),
        Err(BundleVerifyError::AmbiguousOrder { .. })
    ));
}

#[test]
fn an_altered_decision_time_breaks_the_binding_signature() {
    let b = subject();
    let h = decide(vec![(
        membership(MembershipAction::Add, &b),
        VoteChoice::For,
    )]);
    let mut bundle: Value = serde_json::from_slice(&h.bundle(0)).unwrap();
    let mut proof: GovernanceProofV2 =
        serde_json::from_slice(&hex::decode(bundle["proof_bytes_hex"].as_str().unwrap()).unwrap())
            .unwrap();
    // Backdate the admission so it would look effective earlier; fix the checksum.
    proof.payload_binding.as_mut().unwrap().decided_at -= 86_400;
    let raw = serde_json::to_vec(&proof).unwrap();
    bundle["proof_sha256"] = Value::String(hex::encode(Sha256::digest(&raw)));
    bundle["proof_bytes_hex"] = Value::String(hex::encode(&raw));
    assert_eq!(
        verify_bundle(&serde_json::to_vec(&bundle).unwrap(), &h.node_did).unwrap_err(),
        BundleVerifyError::PayloadBindingInvalidSignature
    );
}

#[test]
fn a_legacy_proof_without_a_binding_gives_no_standing() {
    let b = subject();
    let h = decide(vec![(
        membership(MembershipAction::Add, &b),
        VoteChoice::For,
    )]);
    // What a pre-binding producer exported: the same proof without the binding.
    let mut bundle: Value = serde_json::from_slice(&h.bundle(0)).unwrap();
    let mut proof: GovernanceProofV2 =
        serde_json::from_slice(&hex::decode(bundle["proof_bytes_hex"].as_str().unwrap()).unwrap())
            .unwrap();
    proof.payload_binding = None;
    let raw = serde_json::to_vec(&proof).unwrap();
    bundle["proof_sha256"] = Value::String(hex::encode(Sha256::digest(&raw)));
    bundle["proof_bytes_hex"] = Value::String(hex::encode(&raw));
    let legacy = verify_bundle(&serde_json::to_vec(&bundle).unwrap(), &h.node_did)
        .expect("a legacy proof still verifies as a decision");
    assert!(legacy.bound_at.is_none() && legacy.payload.is_none());
    assert_eq!(
        evaluate_standing_at(&legacy, &[], b.as_str(), DOMAIN, u64::MAX),
        Err(BundleVerifyError::PayloadUnbound),
        "no standing is attributed to an unbound proof"
    );
}
