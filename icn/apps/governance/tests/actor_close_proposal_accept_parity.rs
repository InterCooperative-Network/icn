//! Actor-path `CloseProposal::Accept` parity: when the actor receives a
//! `GovernanceCommand::CloseProposal` (as the deadline-driven scheduler task
//! issues when a voting period expires) for an open proposal that meets
//! quorum, the accept branch must emit a durable `InstitutionalEffectRecord`
//! through the installed receipt backend — the same artifact the HTTP close
//! path produces.
//!
//! ## What this pins
//!
//! [`actor_receipt_store_parity`] pins the `ForceCloseProposal::Accept`
//! branch. This file pins the *other* actor-internal acceptance path:
//! `CloseProposal::Accept`. This is the command the scheduler task submits
//! at `apps/governance/src/actor.rs:1335` when `ScheduledEvent::CloseVoting`
//! fires. Before PR #1565 that path emitted no institutional artifact in
//! daemon mode because the receipt_store was only wired at the gateway-close
//! handler. With `install_receipt_store` in place the command now produces
//! the IER through the same `emit_accepted_effect` shared helper that
//! gateway-close and force-close use.
//!
//! ## Why direct-submit instead of scheduler-driven
//!
//! The scheduler task stores wake-up times as `std::time::Instant` (see
//! `ScheduledGovernanceEvent.at`), but its poll loop uses
//! `tokio::time::interval(SCHEDULER_INTERVAL)` for its 10s tick. Those are
//! two different clocks: pausing tokio time stops the interval but not
//! `std::time::Instant::now()`, so a test that advances paused tokio time
//! will never satisfy the scheduler's `at <= now` check, and a test that
//! waits real seconds still has to wait for the interval tick. There is no
//! hermetic way to drive the scheduler end-to-end without flake. The
//! scheduler's only job after the tick fires is exactly this submit:
//! `handle_clone.submit(GovernanceCommand::CloseProposal { .. }).await`.
//! So the honest, stable test is to submit that command directly with the
//! proposal in the same state the scheduler would find it in (Open,
//! quorum-meeting votes cast, receipt_store installed) and assert the
//! accept branch emits the IER.
//!
//! ## What this does NOT prove
//!
//! - End-to-end dispatch evidence to the commons steward service
//!   (`on_proposal_accepted_with_evidence` / `EffectDispatchEvidence`). The
//!   actor path executes the downstream effect via
//!   `SystemEvent::ProposalAccepted` → `EffectDispatcher` →
//!   `KernelGovernanceExecutor` → `SdisServiceImpl::register_steward`, but
//!   no `EffectDispatchEvidence` row is persisted linking that result back
//!   to the IER. Closing that gap requires a `DispatchEvidenceSink` trait
//!   in `icn-kernel-api` + an app-side impl + plumbing through
//!   `BootstrapHandles` / `DecisionExecutor` callback signatures, which is
//!   a separate, larger PR.
//! - The scheduler's timer firing itself. Scheduler-task scheduling is
//!   covered by the existing scheduler tests in this crate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    sdis::SdisProposal, GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig,
    ProposalId, ProposalPayload, ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{
    actor::GovernanceActor, institutional_effect::InstitutionalEffectRecord,
    manager::GovernanceManager, receipt_backend::GovernanceReceiptBackend, GovernanceCommand,
};
use icn_identity::IdentityBundle;
use icn_store::SledStore;
use std::sync::{Arc, Mutex};

/// Minimal in-memory receipt backend that only records
/// `InstitutionalEffectRecord` writes — that is the durable artifact under
/// test. Other backend methods are stubbed to compile but return empty /
/// zero values; the test never exercises them.
struct MemoryReceiptBackend {
    effects: Mutex<Vec<InstitutionalEffectRecord>>,
}

impl MemoryReceiptBackend {
    fn new() -> Self {
        Self {
            effects: Mutex::new(vec![]),
        }
    }

    fn effects_for(&self, proposal_id: &str) -> Vec<InstitutionalEffectRecord> {
        self.effects
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.proposal_id == proposal_id)
            .cloned()
            .collect()
    }
}

impl GovernanceReceiptBackend for MemoryReceiptBackend {
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
        self.effects.lock().unwrap().push(record.clone());
        Ok(())
    }
    fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        Ok(self.effects_for(proposal_id))
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

/// Drive open → vote → direct `CloseProposal` submit and assert the actor's
/// `CloseProposal::Accept` branch emitted a durable `InstitutionalEffectRecord`
/// via the installed receipt backend.
#[tokio::test(flavor = "current_thread")]
async fn actor_close_proposal_accept_emits_appoint_steward_effect() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate_bundle = IdentityBundle::generate().expect("candidate keypair");
    let candidate_did = candidate_bundle.did().clone();

    let store = Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("deadline-parity-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );

    // Single-member domain: one For vote = 100% participation & approval,
    // crossing the 50/50 thresholds set below → DecisionOutcome::Accepted.
    manager
        .create_domain(
            domain_id.clone(),
            "Deadline Parity Domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let payload = ProposalPayload::Sdis {
        proposal: SdisProposal::AppointSteward {
            candidate: candidate_did.clone(),
            sponsors: vec![did.clone()],
            region: "region-south".to_string(),
            bond_amount: 2_000,
            term_length: 3600 * 24 * 30,
        },
    };

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Appoint steward via CloseProposal".to_string(),
            "CloseProposal::Accept parity test".to_string(),
            payload,
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    // Open the proposal. Voting period is long (3600s) so the scheduler
    // will not race us; only a direct submit below will close it.
    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    // Cast the single eligible For vote so the close hits the
    // DecisionOutcome::Accepted branch that emits the IER.
    manager
        .cast_vote(proposal_id.clone(), did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");

    // Install the receipt backend AFTER the actor is spawned and the proposal
    // is open — mirrors production ordering where the gateway builds the
    // receipt store only after supervisor hands off.
    let backend = Arc::new(MemoryReceiptBackend::new());
    actor_handle.install_receipt_store(backend.clone()).await;

    // Directly submit the exact command the scheduler submits when
    // `ScheduledEvent::CloseVoting` fires (see actor.rs:1335). This pins
    // the CloseProposal::Accept emission path without depending on the
    // scheduler's interval timer (whose `std::time::Instant` wake-up time
    // is not controllable by `tokio::time::pause`).
    actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
        })
        .await
        .expect("CloseProposal submit");

    let effects = backend.effects_for(&proposal_id.0);
    assert_eq!(
        effects.len(),
        1,
        "CloseProposal::Accept must emit exactly one InstitutionalEffectRecord; got {effects:?}"
    );
    let rec = &effects[0];
    assert_eq!(
        rec.effect_kind, "appoint_steward",
        "CloseProposal::Accept of an AppointSteward proposal must emit effect_kind=appoint_steward",
    );
    assert_eq!(rec.target_did.as_deref(), Some(candidate_did.as_str()));
    assert_eq!(rec.target_ref.as_deref(), Some("region-south"));

    actor_handle.shutdown().await;
}

/// Execution-required proposals cannot terminate Accepted on actor path when
/// `receipt_store` is absent.
#[tokio::test(flavor = "current_thread")]
async fn actor_close_proposal_accept_without_receipt_store_is_rejected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate_bundle = IdentityBundle::generate().expect("candidate keypair");
    let candidate_did = candidate_bundle.did().clone();

    let store = Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("deadline-parity-noop-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );

    manager
        .create_domain(
            domain_id.clone(),
            "Deadline Parity Noop Domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let payload = ProposalPayload::Sdis {
        proposal: SdisProposal::AppointSteward {
            candidate: candidate_did.clone(),
            sponsors: vec![did.clone()],
            region: "region-south".to_string(),
            bond_amount: 2_000,
            term_length: 3600 * 24 * 30,
        },
    };

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "No receipt store".to_string(),
            "Noop parity".to_string(),
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
        .cast_vote(proposal_id.clone(), did.clone(), VoteChoice::For, None)
        .await
        .expect("cast_vote");

    // NOTE: no install_receipt_store call before close.
    let result = actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
        })
        .await;
    assert!(
        result
            .as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("requires execution closure")),
        "CloseProposal::Accept without receipt_store must be rejected for execution-required payloads; got {result:?}"
    );

    actor_handle.shutdown().await;
}
