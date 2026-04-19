//! Actor-path dispatch-evidence parity: once an `InstitutionalEffectRecord`
//! has been emitted by the actor's acceptance path, the
//! `GovernanceDispatchEvidenceSink` (the kernel-neutral seam added in the
//! same PR) must persist `EffectDispatchEvidence` linking the per-effect
//! `EffectResult` back to that IER — the same durable artifact the
//! gateway-close path writes via `on_proposal_accepted_with_evidence`.
//!
//! ## What this pins
//!
//! - Positive: after actor-path Accept emits an IER, the sink writes
//!   exactly one `EffectDispatchEvidence` with `effect_record_id ==
//!   IER.record_id`, `proposal_id == IER.proposal_id`, `subsystem == "sdis"`,
//!   and `success == true`.
//! - Negative (missing IER): when no IER was emitted for the effect_kind
//!   (e.g. receipt_store wasn't installed at close time), the sink
//!   records nothing and does not panic.
//! - Negative (non-evidenced effect): `KernelEffect::NoOp` skips the
//!   write silently even when an IER exists for the proposal.
//! - Non-governance receipt_id: ill-formed receipt_id strings are
//!   rejected by the sink's parser and produce no writes.
//!
//! ## What this does NOT pin
//!
//! - Full supervisor wiring: this test exercises the sink directly
//!   rather than through `create_decision_executor_callback_with_sink`
//!   in `icn-core`. That callback is tested separately in
//!   `crates/icn-core/tests/decision_executor_runtime_test.rs`.
//! - The failure path of `record_dispatch_evidence` itself (I/O
//!   failures inside the receipt backend) — the sink logs and returns.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    sdis::SdisProposal, GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig,
    ProposalId, ProposalPayload, ProposalScope, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::{
    actor::GovernanceActor, dispatch_evidence::EffectDispatchEvidence,
    dispatch_evidence_sink::GovernanceDispatchEvidenceSink,
    institutional_effect::InstitutionalEffectRecord, manager::GovernanceManager,
    receipt_backend::GovernanceReceiptBackend, GovernanceCommand,
};
use icn_identity::IdentityBundle;
use icn_kernel_api::effects::{DispatchEvidenceSink, EffectResult, KernelEffect, SdisEffect};
use icn_store::SledStore;
use std::sync::{Arc, Mutex};

/// In-memory backend that records both `InstitutionalEffectRecord` and
/// `EffectDispatchEvidence` writes. Other trait methods stub out because
/// this test never exercises them.
struct MemoryReceiptBackend {
    effects: Mutex<Vec<InstitutionalEffectRecord>>,
    evidence: Mutex<Vec<EffectDispatchEvidence>>,
}

impl MemoryReceiptBackend {
    fn new() -> Self {
        Self {
            effects: Mutex::new(vec![]),
            evidence: Mutex::new(vec![]),
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

    fn evidence_for(&self, proposal_id: &str) -> Vec<EffectDispatchEvidence> {
        self.evidence
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.proposal_id == proposal_id)
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
    fn put_effect_dispatch_evidence(&self, ev: &EffectDispatchEvidence) -> Result<(), String> {
        self.evidence.lock().unwrap().push(ev.clone());
        Ok(())
    }
    fn list_effect_dispatch_evidence_by_record(
        &self,
        effect_record_id: &str,
    ) -> Result<Vec<EffectDispatchEvidence>, String> {
        Ok(self
            .evidence
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.effect_record_id == effect_record_id)
            .cloned()
            .collect())
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

/// Build a synthetic `SdisEffect::ApproveSteward` matching the AppointSteward
/// IER the actor-path acceptance just emitted.
fn sdis_approve_effect(candidate: &icn_identity::Did, proposal_id: &str) -> KernelEffect {
    KernelEffect::Sdis(SdisEffect::ApproveSteward {
        steward_did: candidate.to_string(),
        jurisdiction_id: "deadline-parity-domain".to_string(),
        term_length_seconds: 3600 * 24 * 30,
        bond_amount: 2_000,
        region: Some("region-south".to_string()),
        proposal_id: proposal_id.to_string(),
        capabilities_hash: String::new(),
    })
}

fn ok_result(effect_id: &str) -> EffectResult {
    EffectResult {
        effect_id: effect_id.into(),
        success: true,
        message: "approved".into(),
        state_change_hash: Some("state-hash-abc".into()),
        ledger_entry_id: None,
        not_executed: false,
    }
}

/// Positive parity: actor Accept → IER emitted → sink fired → evidence persisted.
#[tokio::test(flavor = "current_thread")]
async fn sink_records_dispatch_evidence_after_actor_accept() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let candidate_bundle = IdentityBundle::generate().expect("candidate keypair");
    let candidate_did = candidate_bundle.did().clone();

    let store = Arc::new(SledStore::open(tmp.path()).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("deadline-parity-domain".to_string());
    let manager = Arc::new(GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>,
    ));

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
            "Appoint steward via actor close".to_string(),
            "dispatch-evidence sink parity test".to_string(),
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

    // Install backend, close via actor — produces the IER.
    let backend = Arc::new(MemoryReceiptBackend::new());
    actor_handle.install_receipt_store(backend.clone()).await;
    // Install backend into the manager too so the sink can look it up.
    let manager_with_store = Arc::new(
        GovernanceManager::with_handle(
            Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
        )
        .with_receipt_store(backend.clone() as Arc<dyn GovernanceReceiptBackend>),
    );

    actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
        })
        .await
        .expect("CloseProposal submit");

    let iers = backend.effects_for(&proposal_id.0);
    assert_eq!(iers.len(), 1, "actor close must emit exactly one IER");
    let ier = &iers[0];
    assert_eq!(ier.effect_kind, "appoint_steward");

    // Build the sink over the manager that has the receipt store wired.
    let sink = GovernanceDispatchEvidenceSink::new(manager_with_store);
    let effects = vec![sdis_approve_effect(&candidate_did, &proposal_id.0)];
    let results = vec![ok_result("eff-1")];
    let decision_receipt_id = format!("gov:{}:{}:receipt", domain_id.0, proposal_id.0);
    sink.record_effects(&decision_receipt_id, &effects, &results, 1_700_000_123);

    let evidence = backend.evidence_for(&proposal_id.0);
    assert_eq!(
        evidence.len(),
        1,
        "sink must persist exactly one EffectDispatchEvidence row; got {evidence:?}"
    );
    let ev = &evidence[0];
    assert_eq!(ev.effect_record_id, ier.record_id);
    assert_eq!(ev.proposal_id, proposal_id.0);
    assert_eq!(ev.subsystem, "sdis");
    assert!(ev.success);
    assert!(ev.error_message.is_none());
    assert_eq!(ev.recorded_at, 1_700_000_123);

    actor_handle.shutdown().await;
}

/// Negative: no IER present → sink is a silent no-op, no panic.
#[tokio::test(flavor = "current_thread")]
async fn sink_no_op_when_no_institutional_record_exists() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let manager = Arc::new(
        GovernanceManager::new()
            .with_receipt_store(backend.clone() as Arc<dyn GovernanceReceiptBackend>),
    );
    let sink = GovernanceDispatchEvidenceSink::new(manager);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-no-ier")];
    let results = vec![ok_result("eff-a")];
    sink.record_effects(
        "gov:domain-x:prop-no-ier:receipt",
        &effects,
        &results,
        1_700_000_000,
    );

    assert!(
        backend.evidence_for("prop-no-ier").is_empty(),
        "sink must not persist evidence when no IER exists for the proposal"
    );
}

/// Negative: NoOp effect skipped even when an IER exists.
#[tokio::test(flavor = "current_thread")]
async fn sink_skips_noop_effects() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    // Seed an IER so we can confirm the skip is the effect-kind check,
    // not the IER lookup.
    let ier = InstitutionalEffectRecord::new(
        "prop-noop",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:s".into()),
        Some("region-x".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let manager = Arc::new(
        GovernanceManager::new()
            .with_receipt_store(backend.clone() as Arc<dyn GovernanceReceiptBackend>),
    );
    let sink = GovernanceDispatchEvidenceSink::new(manager);

    let effects = vec![KernelEffect::NoOp {
        reason: "non-executable".into(),
    }];
    let results = vec![EffectResult {
        effect_id: "eff-noop".into(),
        success: false,
        message: "non-executable".into(),
        state_change_hash: None,
        ledger_entry_id: None,
        not_executed: true,
    }];
    sink.record_effects(
        "gov:domain-x:prop-noop:receipt",
        &effects,
        &results,
        1_700_000_000,
    );

    assert!(
        backend.evidence_for("prop-noop").is_empty(),
        "NoOp effects must not produce dispatch evidence"
    );
}

/// Negative: non-governance receipt_id (wrong prefix/suffix) → sink drops.
#[tokio::test(flavor = "current_thread")]
async fn sink_rejects_malformed_receipt_id() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let manager = Arc::new(
        GovernanceManager::new()
            .with_receipt_store(backend.clone() as Arc<dyn GovernanceReceiptBackend>),
    );
    let sink = GovernanceDispatchEvidenceSink::new(manager);
    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();

    let effects = vec![sdis_approve_effect(&candidate, "anything")];
    let results = vec![ok_result("eff-m")];
    // Missing `gov:` prefix and `:receipt` suffix.
    sink.record_effects("not-a-governance-id", &effects, &results, 1_700_000_000);

    assert!(backend.evidence_for("anything").is_empty());
}
