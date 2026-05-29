//! #1868 — process-authorized `GovernanceDecisionReceiptV3` production emission.
//!
//! Pins the honest emission slice decided in
//! `docs/design/governance/decision-receipt-authority-and-grant-minting.md` §5/§7:
//!
//! - A **scoped** normal democratic close (an authenticated HTTP close that
//!   actually presented a capability scope) emits a `GovernanceDecisionReceiptV3`
//!   carrying `ReceiptMandateAttestation::ProcessAuthorized` and the actual
//!   presented scope — **alongside** the existing v1 receipt / proof.
//! - An **unscoped** close (scheduler/timer auto-close, which presents no
//!   capability) emits **no** v3 — `capability_scope_presented` is evidence and
//!   must never be fabricated.
//! - The v3 receipt is persisted through the fail-closed `put_opaque` seam (the
//!   gateway never imports the typed v3 receipt); a backend without opaque
//!   storage surfaces the gap as an error rather than silently dropping it.
//! - **Forced-accept** (Bootstrap administrative override) emits **no** decision
//!   v2/v3 in this slice — it has no honest `capability_scope_presented` source.
//!
//! Two layers are exercised: the in-memory `GovernanceManager` close path
//! (`close_proposal_inner`, which emits v1 + v3 together) and the actor close
//! path (the production path: `manager → dyn GovernanceOps → CloseProposal`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    ForcedOutcome, GovernanceDecisionReceipt, GovernanceDecisionReceiptV3, GovernanceDomainId,
    GovernanceOps, GovernanceParams, MembershipConfig, ProofOutcome, ProposalId, ProposalPayload,
    ProposalScope, ReceiptMandateAttestation, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{
    actor::GovernanceActor, manager::GovernanceManager, receipt_backend::GovernanceReceiptBackend,
    GovernanceCommand,
};
use icn_identity::{Did, IdentityBundle};
use icn_kernel_api::{AllocationReceipt, Hash};
use icn_store::SledStore;

const DECISION_V3_CLASS: &str = "governance_decision_v3";
const DECISION_V2_CLASS: &str = "governance_decision_v2";

// ============================================================================
// Capturing backend: records v1 receipts and every opaque payload (which is
// how v3 is persisted — the default `put_governance_decision_v3` routes through
// `put_opaque`, so capturing here also proves the opaque-seam routing).
// ============================================================================

#[derive(Default)]
struct CapturingBackend {
    v1: Mutex<Vec<GovernanceDecisionReceipt>>,
    opaque: Mutex<Vec<(String, String, Vec<u8>)>>, // (class, key1, payload)
}

impl CapturingBackend {
    fn v1_count(&self) -> usize {
        self.v1.lock().unwrap().len()
    }

    fn opaque_classes(&self) -> Vec<String> {
        self.opaque
            .lock()
            .unwrap()
            .iter()
            .map(|(c, _, _)| c.clone())
            .collect()
    }

    fn v3_receipts(&self) -> Vec<GovernanceDecisionReceiptV3> {
        self.opaque
            .lock()
            .unwrap()
            .iter()
            .filter(|(class, _, _)| class == DECISION_V3_CLASS)
            .map(|(_, _, payload)| {
                serde_json::from_slice(payload).expect("opaque v3 payload deserializes")
            })
            .collect()
    }
}

impl GovernanceReceiptBackend for CapturingBackend {
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String> {
        self.v1.lock().unwrap().push(receipt.clone());
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(&self, _: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(&self, _: &Hash) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }
    fn put_opaque(
        &self,
        class: &str,
        key1: &str,
        _key2: Option<&str>,
        _recorded_at: u64,
        _record_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<(), String> {
        self.opaque
            .lock()
            .unwrap()
            .push((class.to_string(), key1.to_string(), payload.to_vec()));
        Ok(())
    }
}

// A backend that does NOT implement opaque storage. It inherits the
// fail-closed `put_opaque` default (`opaque_storage_not_implemented`), so a v3
// emission through it must error rather than drop silently.
#[derive(Default)]
struct NoOpaqueBackend;

impl GovernanceReceiptBackend for NoOpaqueBackend {
    fn put_governance(&self, _: &GovernanceDecisionReceipt) -> Result<(), String> {
        Ok(())
    }
    fn get_governance_by_proposal(
        &self,
        _: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn put_allocation(&self, _: &AllocationReceipt) -> Result<Hash, String> {
        Ok([0u8; 32])
    }
    fn get_governance_by_decision(
        &self,
        _: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(None)
    }
    fn list_allocations_by_decision(&self, _: &Hash) -> Result<Vec<AllocationReceipt>, String> {
        Ok(vec![])
    }
    // `put_opaque` intentionally NOT overridden → fail-closed default.
}

// ============================================================================
// In-memory `GovernanceManager` path (close_proposal_inner emits v1 + v3).
// ============================================================================

async fn seeded_in_memory_manager(
    backend: Arc<dyn GovernanceReceiptBackend>,
) -> (GovernanceManager, GovernanceDomainId, ProposalId, Did) {
    let bundle = IdentityBundle::generate().expect("identity");
    let did = bundle.did().clone();
    let mgr = GovernanceManager::new().with_receipt_store(backend);
    let domain = GovernanceDomainId("v3-emit-inmem".to_string());
    mgr.create_domain(
        domain.clone(),
        "V3 Emit (in-memory)".to_string(),
        "cooperative_default".to_string(),
        GovernanceParams::new(50, 50, 3600),
        MembershipConfig::static_list(vec![did.clone()]),
    )
    .await
    .expect("create_domain");

    let proposal_id = ProposalId("v3-inmem-prop".to_string());
    mgr.create_proposal(
        proposal_id.clone(),
        domain.clone(),
        did.clone(),
        "Endorse a fictional statement".to_string(),
        "A declarative text proposal — no execution closure.".to_string(),
        ProposalPayload::Text {
            body: "We endorse this fictional statement.".to_string(),
        },
        ProposalScope::Local,
    )
    .await
    .expect("create_proposal");
    mgr.open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");
    mgr.cast_vote(proposal_id.clone(), did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");
    (mgr, domain, proposal_id, did)
}

#[tokio::test(flavor = "current_thread")]
async fn in_memory_scoped_close_emits_v1_and_process_authorized_v3() {
    let backend = Arc::new(CapturingBackend::default());
    let (mgr, _domain, proposal_id, _did) =
        seeded_in_memory_manager(backend.clone() as Arc<dyn GovernanceReceiptBackend>).await;

    mgr.close_proposal_with_suspension(
        proposal_id.clone(),
        None,
        None,
        Some("governance:write".to_string()),
    )
    .await
    .expect("scoped close");

    // v1 receipt still emitted (behavior preserved).
    assert_eq!(
        backend.v1_count(),
        1,
        "v1 GovernanceDecisionReceipt must still be emitted alongside v3"
    );

    // Exactly one v3, persisted through the opaque seam.
    let v3s = backend.v3_receipts();
    assert_eq!(v3s.len(), 1, "expected exactly one v3 decision receipt");
    let v3 = &v3s[0];
    assert_eq!(v3.proposal_id, proposal_id.0);
    assert_eq!(
        v3.outcome,
        ProofOutcome::Accepted,
        "single-member For vote crosses 50/50 thresholds"
    );
    assert_eq!(
        v3.capability_scope_presented, "governance:write",
        "v3 must record the actual presented scope, not a constant"
    );
    assert!(
        matches!(
            v3.mandate_attestation,
            ReceiptMandateAttestation::ProcessAuthorized
        ),
        "a normal democratic close is process-authorized, got {:?}",
        v3.mandate_attestation
    );
    assert!(v3.verify(), "emitted v3 receipt must verify");
}

#[tokio::test(flavor = "current_thread")]
async fn in_memory_unscoped_close_emits_v1_only_no_v3() {
    let backend = Arc::new(CapturingBackend::default());
    let (mgr, _domain, proposal_id, _did) =
        seeded_in_memory_manager(backend.clone() as Arc<dyn GovernanceReceiptBackend>).await;

    // No capability scope presented (unscoped entry point).
    mgr.close_proposal_with_suspension(proposal_id.clone(), None, None, None)
        .await
        .expect("unscoped close");

    assert_eq!(backend.v1_count(), 1, "v1 receipt still emitted");
    assert!(
        backend.v3_receipts().is_empty(),
        "no v3 may be emitted when no capability scope was presented"
    );
}

// ============================================================================
// Actor path (production): manager → dyn GovernanceOps → CloseProposal.
// ============================================================================

async fn gossip_with_governance_topic(did: Did) -> Arc<tokio::sync::RwLock<GossipActor>> {
    let gossip = GossipActor::spawn(did, None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".to_string(),
        AccessControl::Public,
    ));
    gossip
}

/// Spawn an actor + single-member domain with one open, For-voted Text
/// proposal, with `backend` installed. Returns the manager (handle-backed),
/// the actor handle, and the proposal id.
async fn seeded_actor(
    backend: Arc<dyn GovernanceReceiptBackend>,
) -> (
    GovernanceManager,
    icn_governance_actor::actor::GovernanceHandle,
    ProposalId,
) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle = IdentityBundle::generate().expect("identity");
    let did = bundle.did().clone();
    let store = Arc::new(SledStore::open(tmp.path()).expect("sled"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("spawn actor");
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );

    let domain = GovernanceDomainId("v3-emit-actor".to_string());
    manager
        .create_domain(
            domain.clone(),
            "V3 Emit (actor)".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    // The actor assigns its own proposal id; use the returned value, not the
    // placeholder we pass in (mirrors the accept-parity test pattern).
    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain,
            did.clone(),
            "Endorse a fictional statement".to_string(),
            "A declarative text proposal — no execution closure.".to_string(),
            ProposalPayload::Text {
                body: "We endorse this fictional statement.".to_string(),
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
        .cast_vote(proposal_id.clone(), did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");

    actor_handle.install_receipt_store(backend).await;
    (manager, actor_handle, proposal_id)
}

#[tokio::test(flavor = "current_thread")]
async fn actor_scoped_close_emits_process_authorized_v3() {
    let backend = Arc::new(CapturingBackend::default());
    let (manager, _handle, proposal_id) =
        seeded_actor(backend.clone() as Arc<dyn GovernanceReceiptBackend>).await;

    manager
        .close_proposal_with_suspension(
            proposal_id.clone(),
            None,
            None,
            Some("governance:write".to_string()),
        )
        .await
        .expect("scoped close via production actor path");

    let v3s = backend.v3_receipts();
    assert_eq!(v3s.len(), 1, "actor path must emit exactly one v3");
    let v3 = &v3s[0];
    assert_eq!(v3.proposal_id, proposal_id.0);
    assert_eq!(v3.outcome, ProofOutcome::Accepted);
    assert_eq!(v3.capability_scope_presented, "governance:write");
    assert!(matches!(
        v3.mandate_attestation,
        ReceiptMandateAttestation::ProcessAuthorized
    ));
    assert!(v3.verify(), "emitted v3 receipt must verify");
}

#[tokio::test(flavor = "current_thread")]
async fn actor_unscoped_close_emits_no_v3() {
    let backend = Arc::new(CapturingBackend::default());
    let (_manager, handle, proposal_id) =
        seeded_actor(backend.clone() as Arc<dyn GovernanceReceiptBackend>).await;

    // The exact command the scheduler/timer auto-close submits: no scope.
    handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id,
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await
        .expect("timer-shaped close");

    assert!(
        backend.v3_receipts().is_empty(),
        "scheduler/timer auto-close presents no scope → no v3"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn actor_force_accept_emits_no_decision_v2_or_v3() {
    let backend = Arc::new(CapturingBackend::default());
    let (_manager, handle, proposal_id) =
        seeded_actor(backend.clone() as Arc<dyn GovernanceReceiptBackend>).await;

    handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id,
            forced_outcome: ForcedOutcome::Accept,
            reason: "admin override".to_string(),
        })
        .await
        .expect("force close");

    let classes = backend.opaque_classes();
    assert!(
        !classes.iter().any(|c| c == DECISION_V3_CLASS),
        "forced-accept must emit no decision v3 in this slice; classes: {classes:?}"
    );
    assert!(
        !classes.iter().any(|c| c == DECISION_V2_CLASS),
        "forced-accept must emit no decision v2 Bootstrap in this slice; classes: {classes:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn actor_scoped_close_v3_persistence_failure_is_fail_closed() {
    // Backend without opaque storage → the v3 emission hits the fail-closed
    // `put_opaque` default. The actor persists v3 before the terminal close, so
    // the whole close must fail rather than commit without its decision evidence.
    let backend = Arc::new(NoOpaqueBackend);
    let (manager, _handle, proposal_id) =
        seeded_actor(backend as Arc<dyn GovernanceReceiptBackend>).await;

    let result = manager
        .close_proposal_with_suspension(
            proposal_id,
            None,
            None,
            Some("governance:write".to_string()),
        )
        .await;

    assert!(
        result.is_err(),
        "a v3 persistence failure must fail the close closed, not drop the evidence silently"
    );
}
