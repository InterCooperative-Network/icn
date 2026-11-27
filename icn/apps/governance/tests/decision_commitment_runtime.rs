//! Decision commitments produced and served by the real governance runtime.
//!
//! Unlike the component tests (which seed proposal rows and receipts by hand),
//! every record here is written by the governance actor itself: a proposal is
//! created, opened, voted on by an eligible member and closed; the close
//! persists its v1 decision receipt and its ADR-0014 mandate and grant into the
//! receipt store installed on the actor. The commitment is then read back
//! through the actor's own source (`GovernanceHandle::decision_commitments`)
//! and through the mandate gate, across a real sled close-and-reopen of the
//! actor's state store.
//!
//! The receipt store here is an in-memory recorder that outlives the actor
//! (it is handed to the restarted actor), so the restart proves the actor's
//! durable state is re-read, not receipt-store durability; the gateway's sled
//! `ReceiptStore` has its own persistence tests.
//!
//! Scope of the claim: the only existing proposal type that derives grants is
//! SDIS steward appointment, whose grant is Attestation-class. The gate never
//! accepts Attestation for a gated act, so these tests show the commitment
//! stage passing (the refusal that follows is the class check), not a
//! decision-to-execution workflow for any gated act.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    sdis::SdisProposal, GovernanceDecisionReceipt, GovernanceDomainId, GovernanceOps,
    GovernanceParams, MembershipConfig, ProposalId, ProposalPayload, ProposalScope, ProposalState,
    StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{
    actor::GovernanceActor,
    decision_commitment::DecisionCommitment,
    institutional_effect::InstitutionalEffectRecord,
    mandate_gate::{
        DefaultMandateGate, MandateAct, MandateGate, MandateGateError, MandateRejection,
        MandateRequest, MandateTarget,
    },
    receipt_backend::GovernanceReceiptBackend,
    GovernanceCommand, GovernanceHandle,
};
use icn_identity::{Did, IdentityBundle};
use icn_store::SledStore;

// ---------------------------------------------------------------------------
// A receipt store that records exactly what the actor writes.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingReceipts {
    governance: Mutex<Vec<GovernanceDecisionReceipt>>,
    effects: Mutex<Vec<InstitutionalEffectRecord>>,
    mandates: Mutex<Vec<icn_governance::Mandate>>,
    grants: Mutex<HashMap<icn_governance::AuthorityGrantId, icn_governance::AuthorityGrant>>,
}

impl RecordingReceipts {
    fn mandate_for(&self, proposal_id: &str) -> Option<icn_governance::Mandate> {
        self.mandates
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.decision.proposal_id == proposal_id)
            .cloned()
    }
}

impl GovernanceReceiptBackend for RecordingReceipts {
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String> {
        let mut g = self.governance.lock().unwrap();
        if !g.iter().any(|r| r.decision_hash == receipt.decision_hash) {
            g.push(receipt.clone());
        }
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.proposal_id == proposal_id)
            .cloned())
    }
    fn list_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.proposal_id == proposal_id)
            .cloned()
            .collect())
    }
    fn put_allocation(
        &self,
        _: &icn_kernel_api::AllocationReceipt,
    ) -> Result<icn_kernel_api::Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        h: &icn_kernel_api::Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance
            .lock()
            .unwrap()
            .iter()
            .find(|r| &r.decision_hash == h)
            .cloned())
    }
    fn list_allocations_by_decision(
        &self,
        _: &icn_kernel_api::Hash,
    ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn put_institutional_effect(&self, record: &InstitutionalEffectRecord) -> Result<(), String> {
        self.effects.lock().unwrap().push(record.clone());
        Ok(())
    }
    fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        Ok(self
            .effects
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.proposal_id == proposal_id)
            .cloned()
            .collect())
    }
    fn put_mandate(&self, mandate: &icn_governance::Mandate) -> Result<(), String> {
        self.mandates.lock().unwrap().push(mandate.clone());
        Ok(())
    }
    fn get_mandate_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<icn_governance::Mandate>, String> {
        Ok(self.mandate_for(proposal_id))
    }
    fn list_mandates_by_decision(
        &self,
        h: &icn_kernel_api::Hash,
    ) -> Result<Vec<icn_governance::Mandate>, String> {
        Ok(self
            .mandates
            .lock()
            .unwrap()
            .iter()
            .filter(|m| &m.decision.decision_hash == h)
            .cloned()
            .collect())
    }
    fn put_mandate_with_grants(
        &self,
        mandate: &icn_governance::Mandate,
        grants: &[icn_governance::AuthorityGrant],
    ) -> Result<(), String> {
        self.mandates.lock().unwrap().push(mandate.clone());
        for g in grants {
            self.put_authority_grant(g)?;
        }
        Ok(())
    }
    fn put_authority_grant(&self, grant: &icn_governance::AuthorityGrant) -> Result<(), String> {
        self.grants
            .lock()
            .unwrap()
            .insert(grant.id.clone(), grant.clone());
        Ok(())
    }
    fn get_authority_grant(
        &self,
        id: &icn_governance::AuthorityGrantId,
    ) -> Result<Option<icn_governance::AuthorityGrant>, String> {
        Ok(self.grants.lock().unwrap().get(id).cloned())
    }
    fn list_authority_grants_by_grantee(
        &self,
        grantee: &icn_governance::Grantee,
    ) -> Result<Vec<icn_governance::AuthorityGrant>, String> {
        Ok(self.grants_of(grantee))
    }
    fn list_active_authority_grants_by_grantee(
        &self,
        grantee: &icn_governance::Grantee,
        now: u64,
    ) -> Result<Vec<icn_governance::AuthorityGrant>, String> {
        Ok(self
            .grants_of(grantee)
            .into_iter()
            .filter(|g| g.is_active_at(now))
            .collect())
    }
}

impl RecordingReceipts {
    /// Every grant held for `grantee`, oldest-first by `valid_from`.
    fn grants_of(&self, grantee: &icn_governance::Grantee) -> Vec<icn_governance::AuthorityGrant> {
        let mut out: Vec<_> = self
            .grants
            .lock()
            .unwrap()
            .values()
            .filter(|g| &g.grantee == grantee)
            .cloned()
            .collect();
        out.sort_by_key(|g| g.valid_from);
        out
    }
}

// ---------------------------------------------------------------------------
// Runtime fixture
// ---------------------------------------------------------------------------

const DOMAIN: &str = "commitment-runtime-dom";

fn new_did() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

async fn spawn(
    path: &std::path::Path,
    receipts: Option<Arc<RecordingReceipts>>,
) -> GovernanceHandle {
    let node = new_did();
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        node,
        Arc::new(SledStore::open(path).unwrap()),
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap();
    if let Some(r) = receipts {
        handle.install_receipt_store(r).await;
        handle.recover_incomplete_closes().await;
    }
    handle
}

/// Domain with `member` as its sole voter; an open steward-appointment proposal.
async fn open_proposal(handle: &GovernanceHandle, member: &Did, candidate: &Did) -> ProposalId {
    open_payload(
        handle,
        member,
        ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate: candidate.clone(),
                sponsors: vec![member.clone()],
                region: "region-runtime".into(),
                bond_amount: 1_000,
                term_length: 3600 * 24 * 30,
            },
        },
    )
    .await
}

/// Domain with `member` as its sole voter; an open proposal carrying `payload`.
async fn open_payload(
    handle: &GovernanceHandle,
    member: &Did,
    payload: ProposalPayload,
) -> ProposalId {
    let ops: &dyn GovernanceOps = handle;
    if ops
        .create_domain(
            GovernanceDomainId(DOMAIN.into()),
            "Commitment runtime".into(),
            "cooperative_default".into(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![member.clone()]),
        )
        .await
        .is_err()
    {
        // Already created by an earlier call in the same test.
    }
    let id = ops
        .create_proposal(
            GovernanceDomainId(DOMAIN.into()),
            "Steward proposal".into(),
            "d".into(),
            payload,
            ProposalScope::Local,
        )
        .await
        .unwrap();
    ops.open_proposal(id.clone(), 3600).await.unwrap();
    id
}

async fn vote_and_close(
    handle: &GovernanceHandle,
    id: &ProposalId,
    member: &Did,
    choice: VoteChoice,
) {
    let ops: &dyn GovernanceOps = handle;
    ops.cast_vote(id.clone(), member.clone(), choice, None)
        .await
        .unwrap();
    handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await
        .unwrap();
}

fn steward_request(actor: &Did, id: &ProposalId) -> MandateRequest {
    MandateRequest {
        actor: actor.clone(),
        domain: GovernanceDomainId(DOMAIN.into()),
        act: MandateAct::AppointSteward,
        target: MandateTarget::Proposal(id.clone()),
        at: icn_time::current_timestamp_secs(),
    }
}

fn is_finalized(c: &DecisionCommitment) -> bool {
    matches!(c, DecisionCommitment::FinalizedAccepted(_))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn real_close_commits_and_the_commitment_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    let (member, candidate) = (new_did(), new_did());

    let handle = spawn(tmp.path(), Some(receipts.clone())).await;
    let id = open_proposal(&handle, &member, &candidate).await;
    vote_and_close(&handle, &id, &member, VoteChoice::For).await;
    let state = handle.get_proposal(&id).await.unwrap().unwrap().state;
    assert!(matches!(state, ProposalState::Accepted { .. }), "{state:?}");

    // Everything below was written by the actor's close, not by the test.
    let mandate = receipts
        .mandate_for(&id.0)
        .expect("the close minted a mandate");
    assert_eq!(
        receipts.list_governance_by_proposal(&id.0).unwrap().len(),
        1
    );

    let source = handle.decision_commitments();
    let c = source.decision_commitment(&id.0).unwrap();
    assert!(is_finalized(&c), "{c:?}");
    assert!(
        c.authorizes(&mandate),
        "the real mandate is the committed decision"
    );

    // Through the gate: the commitment stage passes; the refusal that follows
    // is the class check (an Attestation grant authorizes no gated act).
    let err = DefaultMandateGate::new(receipts.clone(), source.as_ref())
        .require(&steward_request(&candidate, &id))
        .unwrap_err();
    assert_ne!(
        err,
        MandateGateError::Rejected(MandateRejection::DecisionNotCommitted)
    );

    // Restart: drop the actor and its sled handles, reopen from disk.
    handle.shutdown().await;
    drop(source);
    drop(handle);
    let handle = spawn(tmp.path(), Some(receipts.clone())).await;
    let source = handle.decision_commitments();
    let c = source.decision_commitment(&id.0).unwrap();
    assert!(is_finalized(&c), "after restart: {c:?}");
    assert!(
        c.authorizes(&mandate),
        "legitimate authority is not lost on restart"
    );
    handle.shutdown().await;
}

#[tokio::test]
async fn source_fails_closed_until_the_receipt_store_is_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let handle = spawn(tmp.path(), None).await;
    let err = handle
        .decision_commitments()
        .decision_commitment("anything");
    assert!(err.is_err(), "no receipt store: an error, never a verdict");
    handle.shutdown().await;
}

#[tokio::test]
async fn incomplete_rejected_and_missing_decisions_do_not_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    let (member, candidate) = (new_did(), new_did());
    let handle = spawn(tmp.path(), Some(receipts.clone())).await;
    let source = handle.decision_commitments();

    // Open, not decided.
    let open = open_proposal(&handle, &member, &candidate).await;
    assert_eq!(
        source.decision_commitment(&open.0).unwrap(),
        DecisionCommitment::Incomplete
    );

    // Decided against: no mandate, and the commitment says Rejected.
    let rejected = open_proposal(&handle, &member, &candidate).await;
    vote_and_close(&handle, &rejected, &member, VoteChoice::Against).await;
    assert!(receipts.mandate_for(&rejected.0).is_none());
    assert_eq!(
        source.decision_commitment(&rejected.0).unwrap(),
        DecisionCommitment::Rejected
    );

    // Unknown proposal.
    assert!(matches!(
        source.decision_commitment("no-such-proposal").unwrap(),
        DecisionCommitment::Unknown(_)
    ));
    handle.shutdown().await;
}

#[tokio::test]
async fn a_conflicting_second_decision_record_is_contradictory() {
    let tmp = tempfile::tempdir().unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    let (member, candidate) = (new_did(), new_did());
    let handle = spawn(tmp.path(), Some(receipts.clone())).await;
    let id = open_proposal(&handle, &member, &candidate).await;
    vote_and_close(&handle, &id, &member, VoteChoice::For).await;
    let mandate = receipts.mandate_for(&id.0).unwrap();

    // Another writer records a different decision for the same proposal. Its
    // hash is independent of the mandate under test.
    let mut other = receipts.list_governance_by_proposal(&id.0).unwrap()[0].clone();
    other.decision_hash = [0xEE; 32];
    receipts.put_governance(&other).unwrap();

    let source = handle.decision_commitments();
    let c = source.decision_commitment(&id.0).unwrap();
    assert!(matches!(c, DecisionCommitment::Contradictory(_)), "{c:?}");
    assert!(!c.authorizes(&mandate));
    let err = DefaultMandateGate::new(receipts.clone(), source.as_ref())
        .require(&steward_request(&candidate, &id))
        .unwrap_err();
    assert_eq!(
        err,
        MandateGateError::Rejected(MandateRejection::DecisionNotCommitted)
    );
    handle.shutdown().await;
}

#[tokio::test]
async fn an_actor_backed_manager_answers_only_from_the_wired_actor_source() {
    use icn_governance_actor::decision_commitment::{
        DecisionCommitmentSource, SharedManagerCommitments,
    };
    use icn_governance_actor::GovernanceManager;

    let tmp = tempfile::tempdir().unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    let (member, candidate) = (new_did(), new_did());
    let handle = spawn(tmp.path(), Some(receipts.clone())).await;
    let id = open_proposal(&handle, &member, &candidate).await;
    vote_and_close(&handle, &id, &member, VoteChoice::For).await;
    let ops: Arc<dyn GovernanceOps + Send + Sync> = Arc::new(handle.clone());

    // Actor-backed, no source wired: an error, never its empty standalone view.
    let unwired =
        Arc::new(GovernanceManager::with_handle(ops.clone()).with_receipt_store(receipts.clone()));
    assert!(SharedManagerCommitments::new(unwired)
        .decision_commitment(&id.0)
        .is_err());

    // Wired with the actor's source: the actor's committed decision.
    let wired = Arc::new(
        GovernanceManager::with_handle(ops)
            .with_receipt_store(receipts.clone())
            .with_decision_commitments(handle.decision_commitments()),
    );
    let c = SharedManagerCommitments::new(wired)
        .decision_commitment(&id.0)
        .unwrap();
    assert!(is_finalized(&c), "{c:?}");
    assert!(c.authorizes(&receipts.mandate_for(&id.0).unwrap()));
    handle.shutdown().await;
}

// ---------------------------------------------------------------------------
// Derived authority: reconfirmation / reinstatement templates
// ---------------------------------------------------------------------------

/// A grant written straight into the receipt store: in-domain grantor and a
/// `granted_by` naming a decision this runtime never made.
fn planted_grant(steward: &Did, revoked_at: Option<u64>) -> icn_governance::AuthorityGrant {
    let now = icn_time::current_timestamp_secs();
    icn_governance::AuthorityGrant {
        id: icn_governance::AuthorityGrantId::new(),
        class: icn_governance::AuthorityClass::Execution,
        grantor: icn_governance::GrantorEntityId(DOMAIN.into()),
        grantee: icn_governance::Grantee::Person(steward.clone()),
        scope: icn_governance::TypedScope {
            domain: Some(GovernanceDomainId(DOMAIN.into())),
            proposal_class: vec!["Sdis".into(), "Treasury".into()],
            ..icn_governance::TypedScope::default()
        },
        granted_by: Some(icn_governance::DecisionProvenance {
            proposal_id: "prop-never-decided".into(),
            decision_hash: [0xD7; 32],
        }),
        valid_from: now.saturating_sub(100),
        valid_until: Some(now + 1_000_000),
        revoked_at,
    }
}

fn reconfirm(steward: &Did) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::ReconfirmSteward {
            steward: steward.clone(),
            new_term_end: icn_time::current_timestamp_secs() + 3600 * 24 * 90,
            performance_notes: None,
        },
    }
}

fn steward_grants(
    receipts: &RecordingReceipts,
    steward: &Did,
) -> Vec<icn_governance::AuthorityGrant> {
    receipts.grants_of(&icn_governance::Grantee::Person(steward.clone()))
}

#[tokio::test]
async fn daemon_reconfirm_does_not_carry_forward_an_orphan_active_template() {
    let tmp = tempfile::tempdir().unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    let (member, steward) = (new_did(), new_did());
    let handle = spawn(tmp.path(), Some(receipts.clone())).await;
    let orphan = planted_grant(&steward, None);
    receipts.put_authority_grant(&orphan).unwrap();

    let id = open_payload(&handle, &member, reconfirm(&steward)).await;
    vote_and_close(&handle, &id, &member, VoteChoice::For).await;
    let state = handle.get_proposal(&id).await.unwrap().unwrap().state;
    assert!(matches!(state, ProposalState::Accepted { .. }), "{state:?}");

    let mandate = receipts
        .mandate_for(&id.0)
        .expect("the close records a mandate");
    assert!(mandate.grants.is_empty(), "no derived grant: {mandate:?}");
    assert_eq!(steward_grants(&receipts, &steward), vec![orphan]);
    handle.shutdown().await;
}

#[tokio::test]
async fn daemon_reinstate_does_not_carry_forward_an_orphan_revoked_template() {
    let tmp = tempfile::tempdir().unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    let (member, steward) = (new_did(), new_did());
    let handle = spawn(tmp.path(), Some(receipts.clone())).await;
    let revoked_at = icn_time::current_timestamp_secs().saturating_sub(10);
    let orphan = planted_grant(&steward, Some(revoked_at));
    receipts.put_authority_grant(&orphan).unwrap();

    let payload = ProposalPayload::Sdis {
        proposal: SdisProposal::ReinstateSteward {
            steward: steward.clone(),
            reason: "appeal upheld".into(),
        },
    };
    let id = open_payload(&handle, &member, payload).await;
    vote_and_close(&handle, &id, &member, VoteChoice::For).await;
    let state = handle.get_proposal(&id).await.unwrap().unwrap().state;
    assert!(matches!(state, ProposalState::Accepted { .. }), "{state:?}");

    let mandate = receipts
        .mandate_for(&id.0)
        .expect("the close records a mandate");
    assert!(mandate.grants.is_empty(), "no derived grant: {mandate:?}");
    assert_eq!(steward_grants(&receipts, &steward), vec![orphan]);
    handle.shutdown().await;
}

/// Control: a template the runtime itself committed (a real appointment) is
/// carried forward by a real reconfirmation close.
#[tokio::test]
async fn daemon_reconfirm_carries_forward_a_committed_appointment() {
    let tmp = tempfile::tempdir().unwrap();
    let receipts = Arc::new(RecordingReceipts::default());
    let (member, steward) = (new_did(), new_did());
    let handle = spawn(tmp.path(), Some(receipts.clone())).await;
    let appoint = open_proposal(&handle, &member, &steward).await;
    vote_and_close(&handle, &appoint, &member, VoteChoice::For).await;
    let appointed = steward_grants(&receipts, &steward);
    assert_eq!(appointed.len(), 1, "the appointment minted one grant");

    let id = open_payload(&handle, &member, reconfirm(&steward)).await;
    vote_and_close(&handle, &id, &member, VoteChoice::For).await;
    let mandate = receipts
        .mandate_for(&id.0)
        .expect("the close records a mandate");
    assert_eq!(mandate.grants.len(), 1, "{mandate:?}");
    let all = steward_grants(&receipts, &steward);
    assert_eq!(all.len(), 2, "{all:?}");
    let fresh = all.iter().find(|g| g.id == mandate.grants[0]).unwrap();
    assert_eq!(fresh.class, appointed[0].class);
    assert_eq!(fresh.scope, appointed[0].scope);
    assert_eq!(fresh.granted_by, Some(mandate.decision.clone()));
    handle.shutdown().await;
}
