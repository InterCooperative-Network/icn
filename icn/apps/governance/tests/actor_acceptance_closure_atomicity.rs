//! Residual acceptance-closure atomicity regressions (see issue #1588).
//!
//! PR #1586 landed the primary atomicity fix: execution-required accepted
//! proposals now preflight `emit_accepted_effect` and
//! `mint_and_persist_for_accepted` before `proposal.close()` + `save_proposal`.
//! This file pins the *residual* invariants that follow from moving the
//! Invariant 7 gate + proof-byte persistence in front of terminal save and
//! guarding the redundant post-save emission block:
//!
//! 1. Execution-required accepted proposals (both normal close and force
//!    close) emit their InstitutionalEffectRecord and mandate **exactly
//!    once** — the preflight is the sole writer, the post-save block is
//!    skipped.
//! 2. A preflight failure leaves the proposal in its prior state (Open),
//!    not persisted as Accepted. The caller sees `Err` and the store
//!    agrees.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    sdis::SdisProposal, ForcedOutcome, GovernanceDomainId, GovernanceOps, GovernanceParams,
    MembershipConfig, ProposalId, ProposalPayload, ProposalScope, ProposalState,
    StaticMembershipResolver,
};
use icn_governance_actor::{
    actor::GovernanceActor,
    institutional_effect::InstitutionalEffectRecord,
    receipt_backend::GovernanceReceiptBackend,
    state_store::{GovernanceStateStore, SledGovernanceStateStore},
    GovernanceCommand, GovernanceManager,
};
use icn_identity::IdentityBundle;
use icn_store::SledStore;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

/// Counting receipt backend. Records every call to `put_institutional_effect`,
/// `put_mandate_with_grants`, and `put_mandate`, and supports forcing those
/// writes to fail via a `fail_put_institutional_effect` flag.
struct CountingReceiptBackend {
    effects: Mutex<Vec<InstitutionalEffectRecord>>,
    put_institutional_effect_calls: AtomicUsize,
    list_institutional_effects_calls: AtomicUsize,
    put_mandate_calls: AtomicUsize,
    put_mandate_with_grants_calls: AtomicUsize,
    get_mandate_by_proposal_calls: AtomicUsize,
    fail_put_institutional_effect: bool,
    mandates_by_proposal: Mutex<Vec<icn_governance::Mandate>>,
    grants: Mutex<HashMap<icn_governance::AuthorityGrantId, icn_governance::AuthorityGrant>>,
}

impl CountingReceiptBackend {
    fn new() -> Self {
        Self {
            effects: Mutex::new(vec![]),
            put_institutional_effect_calls: AtomicUsize::new(0),
            list_institutional_effects_calls: AtomicUsize::new(0),
            put_mandate_calls: AtomicUsize::new(0),
            put_mandate_with_grants_calls: AtomicUsize::new(0),
            get_mandate_by_proposal_calls: AtomicUsize::new(0),
            fail_put_institutional_effect: false,
            mandates_by_proposal: Mutex::new(vec![]),
            grants: Mutex::new(HashMap::new()),
        }
    }

    fn new_failing_put_institutional_effect() -> Self {
        let mut b = Self::new();
        b.fail_put_institutional_effect = true;
        b
    }
}

impl GovernanceReceiptBackend for CountingReceiptBackend {
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
    fn put_institutional_effect(&self, record: &InstitutionalEffectRecord) -> Result<(), String> {
        self.put_institutional_effect_calls
            .fetch_add(1, Ordering::SeqCst);
        if self.fail_put_institutional_effect {
            return Err("injected: put_institutional_effect failed".to_string());
        }
        self.effects.lock().unwrap().push(record.clone());
        Ok(())
    }
    fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        self.list_institutional_effects_calls
            .fetch_add(1, Ordering::SeqCst);
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
        self.put_mandate_calls.fetch_add(1, Ordering::SeqCst);
        self.mandates_by_proposal
            .lock()
            .unwrap()
            .push(mandate.clone());
        Ok(())
    }
    fn get_mandate_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<icn_governance::Mandate>, String> {
        self.get_mandate_by_proposal_calls
            .fetch_add(1, Ordering::SeqCst);
        Ok(self
            .mandates_by_proposal
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.decision.proposal_id == proposal_id)
            .cloned())
    }
    fn put_mandate_with_grants(
        &self,
        mandate: &icn_governance::Mandate,
        grants: &[icn_governance::AuthorityGrant],
    ) -> Result<(), String> {
        self.put_mandate_with_grants_calls
            .fetch_add(1, Ordering::SeqCst);
        // Durably record the mandate and every grant so the seam's
        // read-after-write contract holds.
        self.mandates_by_proposal
            .lock()
            .unwrap()
            .push(mandate.clone());
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
        grant_id: &icn_governance::AuthorityGrantId,
    ) -> Result<Option<icn_governance::AuthorityGrant>, String> {
        Ok(self.grants.lock().unwrap().get(grant_id).cloned())
    }
}

async fn gossip_with_governance_topic(
    did: icn_identity::Did,
) -> Arc<tokio::sync::RwLock<GossipActor>> {
    let gossip = GossipActor::spawn(did, None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".to_string(),
        AccessControl::Public,
    ));
    gossip
}

fn appoint_steward_payload(
    candidate: icn_identity::Did,
    sponsor: icn_identity::Did,
) -> ProposalPayload {
    ProposalPayload::Sdis {
        proposal: SdisProposal::AppointSteward {
            candidate,
            sponsors: vec![sponsor],
            region: "region-atom".to_string(),
            bond_amount: 1_000,
            term_length: 3600 * 24 * 30,
        },
    }
}

/// Execution-required accepted proposals closed via the normal `CloseProposal`
/// path must emit their `InstitutionalEffectRecord` via the preflight and not
/// re-run the post-save emission block. We observe this by asserting the
/// shared helper's `list_institutional_effects_by_proposal` probe runs exactly
/// once — before the guard, it ran twice (once per emit_accepted_effect call).
#[tokio::test(flavor = "current_thread")]
async fn normal_close_execution_required_emits_exactly_once() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate = IdentityBundle::generate().expect("candidate").did().clone();

    let store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("single-emit-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );
    manager
        .create_domain(
            domain_id.clone(),
            "Single-emit domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Appoint steward (single-emit)".to_string(),
            "".to_string(),
            appoint_steward_payload(candidate.clone(), did.clone()),
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");
    manager
        .cast_vote(
            proposal_id.clone(),
            did.clone(),
            icn_governance::VoteChoice::For,
            None,
        )
        .await
        .expect("cast_vote");

    let backend = Arc::new(CountingReceiptBackend::new());
    actor_handle.install_receipt_store(backend.clone()).await;

    actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await
        .expect("CloseProposal submit");

    // Exactly one IER landed in the store.
    assert_eq!(
        backend
            .put_institutional_effect_calls
            .load(Ordering::SeqCst),
        1,
        "execution-required normal close must call put_institutional_effect exactly once"
    );
    // The helper's existence-check probe must not have been called twice —
    // that would mean the post-save emission block ran redundantly.
    assert_eq!(
        backend
            .list_institutional_effects_calls
            .load(Ordering::SeqCst),
        1,
        "execution-required normal close must not re-run the post-save emission helper"
    );
    // Mandate existence-check probe: same invariant.
    assert!(
        backend.get_mandate_by_proposal_calls.load(Ordering::SeqCst) <= 1,
        "execution-required normal close must not re-run the post-save mandate helper; got {}",
        backend.get_mandate_by_proposal_calls.load(Ordering::SeqCst)
    );

    actor_handle.shutdown().await;
}

/// Same invariant for the force-close path: execution-required force-accept
/// must not double-emit through the post-save block.
#[tokio::test(flavor = "current_thread")]
async fn force_close_execution_required_emits_exactly_once() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate = IdentityBundle::generate().expect("candidate").did().clone();

    let store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("single-emit-force-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );
    manager
        .create_domain(
            domain_id.clone(),
            "Single-emit force domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Appoint steward (force, single-emit)".to_string(),
            "".to_string(),
            appoint_steward_payload(candidate.clone(), did.clone()),
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    let backend = Arc::new(CountingReceiptBackend::new());
    actor_handle.install_receipt_store(backend.clone()).await;

    actor_handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id: proposal_id.clone(),
            forced_outcome: ForcedOutcome::Accept,
            reason: "atomicity regression test".to_string(),
        })
        .await
        .expect("ForceCloseProposal submit");

    assert_eq!(
        backend
            .put_institutional_effect_calls
            .load(Ordering::SeqCst),
        1,
        "execution-required force close must call put_institutional_effect exactly once"
    );
    assert_eq!(
        backend
            .list_institutional_effects_calls
            .load(Ordering::SeqCst),
        1,
        "execution-required force close must not re-run the post-save emission helper"
    );

    actor_handle.shutdown().await;
}

/// When the preflight persistence fails on the normal close path, the actor
/// must leave the proposal in its prior state (`Open`) — not persisted as
/// `Accepted`. This is the load-bearing atomicity invariant: caller sees
/// `Err` ⟺ store agrees.
#[tokio::test(flavor = "current_thread")]
async fn normal_close_preflight_failure_leaves_proposal_open() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate = IdentityBundle::generate().expect("candidate").did().clone();

    let store_raw: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store_raw.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("preflight-fail-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );
    manager
        .create_domain(
            domain_id.clone(),
            "Preflight-fail domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Preflight-fail proposal".to_string(),
            "".to_string(),
            appoint_steward_payload(candidate.clone(), did.clone()),
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");
    manager
        .cast_vote(
            proposal_id.clone(),
            did.clone(),
            icn_governance::VoteChoice::For,
            None,
        )
        .await
        .expect("cast_vote");

    // Install a backend that fails put_institutional_effect during preflight.
    let backend = Arc::new(CountingReceiptBackend::new_failing_put_institutional_effect());
    actor_handle.install_receipt_store(backend.clone()).await;

    let result = actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await;
    assert!(result.is_err(), "preflight failure must surface as Err");

    // Re-read the proposal from the same underlying store. State must not
    // have advanced to Accepted.
    let state_store: Arc<dyn GovernanceStateStore> =
        Arc::new(SledGovernanceStateStore::new(store_raw.clone()));
    let persisted = state_store
        .get_proposal(&proposal_id)
        .expect("get_proposal")
        .expect("proposal must still exist");
    assert!(
        matches!(persisted.state, ProposalState::Open { .. }),
        "preflight failure must leave proposal Open; got {:?}",
        persisted.state
    );

    actor_handle.shutdown().await;
}

/// Same invariant for force-close: preflight persistence failure must not
/// leave the proposal persisted as Accepted.
#[tokio::test(flavor = "current_thread")]
async fn force_close_preflight_failure_leaves_proposal_open() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate = IdentityBundle::generate().expect("candidate").did().clone();

    let store_raw: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store_raw.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("preflight-fail-force-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );
    manager
        .create_domain(
            domain_id.clone(),
            "Preflight-fail force domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Preflight-fail proposal (force)".to_string(),
            "".to_string(),
            appoint_steward_payload(candidate.clone(), did.clone()),
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    let backend = Arc::new(CountingReceiptBackend::new_failing_put_institutional_effect());
    actor_handle.install_receipt_store(backend.clone()).await;

    let result = actor_handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id: proposal_id.clone(),
            forced_outcome: ForcedOutcome::Accept,
            reason: "preflight fail test".to_string(),
        })
        .await;
    assert!(
        result.is_err(),
        "force-close preflight failure must surface as Err"
    );

    let state_store: Arc<dyn GovernanceStateStore> =
        Arc::new(SledGovernanceStateStore::new(store_raw.clone()));
    let persisted = state_store
        .get_proposal(&proposal_id)
        .expect("get_proposal")
        .expect("proposal must still exist");
    assert!(
        matches!(persisted.state, ProposalState::Open { .. }),
        "force-close preflight failure must leave proposal Open; got {:?}",
        persisted.state
    );

    actor_handle.shutdown().await;
}

// ============================================================================
// Control-effect replay idempotency (re-dispatch after crash / redelivery).
//
// `ControlEffect::VetoProposal` / `ForceCloseProposal` flow through the
// `ControlServiceImpl` adapter as `GovernanceCommand::VetoProposal` /
// `ForceCloseProposal`. If a control effect is re-dispatched (e.g. effect
// re-delivery after a crash before the dispatch evidence was recorded), the
// second command must be a no-op-with-error: the terminal-state guard in
// `Proposal::veto` / `Proposal::force_close` rejects it *before* any state
// mutation, and the force-accept outcome side effects (institutional-effect
// record + mandate) are not written a second time. These tests pin that the
// re-dispatch path cannot double-apply.
// ============================================================================

/// Re-dispatching a `VetoProposal` command for an already-vetoed proposal must
/// surface as `Err`, preserve the terminal `Vetoed` state, and leave the
/// *original* veto reason in place — the replayed command's reason must not
/// overwrite it.
#[tokio::test(flavor = "current_thread")]
async fn veto_redispatch_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();

    let store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("veto-redispatch-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );
    manager
        .create_domain(
            domain_id.clone(),
            "Veto redispatch domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Veto redispatch proposal".to_string(),
            "".to_string(),
            ProposalPayload::Text {
                body: "body".to_string(),
            },
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    // First veto succeeds and records the original reason.
    actor_handle
        .submit(GovernanceCommand::VetoProposal {
            proposal_id: proposal_id.clone(),
            reason: "original veto".to_string(),
        })
        .await
        .expect("first VetoProposal submit");

    // Replay the same control effect with a different reason.
    let replay = actor_handle
        .submit(GovernanceCommand::VetoProposal {
            proposal_id: proposal_id.clone(),
            reason: "replayed veto".to_string(),
        })
        .await;
    assert!(
        replay.is_err(),
        "re-dispatched veto on a terminal proposal must surface as Err"
    );

    // Persisted state is still Vetoed with the *original* reason — the replay
    // neither double-applied nor overwrote anything.
    let state_store: Arc<dyn GovernanceStateStore> =
        Arc::new(SledGovernanceStateStore::new(store.clone()));
    let persisted = state_store
        .get_proposal(&proposal_id)
        .expect("get_proposal")
        .expect("proposal must still exist");
    assert!(
        matches!(
            persisted.state,
            ProposalState::Vetoed { ref reason, .. } if reason == "original veto"
        ),
        "veto replay must preserve the original Vetoed record; got {:?}",
        persisted.state
    );

    actor_handle.shutdown().await;
}

/// Re-dispatching a `ForceCloseProposal` (force-accept of an execution-required
/// proposal) for an already-force-closed proposal must surface as `Err`,
/// preserve the terminal `ForceClosed` state, and NOT re-write the durable
/// outcome side effects. The force-accept preflight re-runs on replay, but its
/// institutional-effect and mandate writes are idempotent, so the durable write
/// counts stay exactly where the first dispatch left them.
#[tokio::test(flavor = "current_thread")]
async fn force_close_redispatch_does_not_repeat_effects() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate = IdentityBundle::generate().expect("candidate").did().clone();

    let store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("force-close-redispatch-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );
    manager
        .create_domain(
            domain_id.clone(),
            "Force-close redispatch domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Appoint steward (force-close redispatch)".to_string(),
            "".to_string(),
            appoint_steward_payload(candidate.clone(), did.clone()),
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    let backend = Arc::new(CountingReceiptBackend::new());
    actor_handle.install_receipt_store(backend.clone()).await;

    // First force-close (force-accept) succeeds and emits the outcome effect once.
    actor_handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id: proposal_id.clone(),
            forced_outcome: ForcedOutcome::Accept,
            reason: "first force-close".to_string(),
        })
        .await
        .expect("first ForceCloseProposal submit");

    assert_eq!(
        backend
            .put_institutional_effect_calls
            .load(Ordering::SeqCst),
        1,
        "first force-accept must write exactly one institutional-effect record"
    );
    let mandate_writes_after_first = backend.put_mandate_with_grants_calls.load(Ordering::SeqCst);

    let state_store: Arc<dyn GovernanceStateStore> =
        Arc::new(SledGovernanceStateStore::new(store.clone()));
    let closed_at_after_first = match state_store
        .get_proposal(&proposal_id)
        .expect("get_proposal")
        .expect("proposal must exist")
        .state
    {
        ProposalState::ForceClosed { closed_at, .. } => closed_at,
        other => panic!("expected ForceClosed after first force-close, got {other:?}"),
    };

    // Replay the same control effect. The terminal-state guard rejects it.
    let replay = actor_handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id: proposal_id.clone(),
            forced_outcome: ForcedOutcome::Accept,
            reason: "replayed force-close".to_string(),
        })
        .await;
    assert!(
        replay.is_err(),
        "re-dispatched force-close on a terminal proposal must surface as Err"
    );

    // The durable outcome side effects were not written a second time.
    assert_eq!(
        backend
            .put_institutional_effect_calls
            .load(Ordering::SeqCst),
        1,
        "force-close replay must not write a second institutional-effect record"
    );
    assert_eq!(
        backend.put_mandate_with_grants_calls.load(Ordering::SeqCst),
        mandate_writes_after_first,
        "force-close replay must not mint/persist the mandate again"
    );

    // Terminal state is byte-for-byte preserved (same closed_at).
    let persisted = state_store
        .get_proposal(&proposal_id)
        .expect("get_proposal")
        .expect("proposal must still exist");
    match persisted.state {
        ProposalState::ForceClosed { closed_at, .. } => {
            assert_eq!(
                closed_at, closed_at_after_first,
                "force-close replay must preserve the original ForceClosed record"
            );
        }
        other => panic!("state mutated after force-close replay: {other:?}"),
    }

    actor_handle.shutdown().await;
}
