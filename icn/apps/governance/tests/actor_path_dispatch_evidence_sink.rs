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
    receipt_backend::GovernanceReceiptBackend, DeferredDispatchEvidenceSink, GovernanceCommand,
};
use icn_identity::IdentityBundle;
use icn_kernel_api::effects::{
    DispatchEvidenceSink, EffectOutcome, EffectResult, KernelEffect, SdisEffect,
};
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
        receipt_ref: None,
        outcome: None,
    }
}

/// Successful result carrying a downstream receipt_ref, emulating what
/// `SdisServiceImpl::appoint_steward` now publishes (the commons
/// `StewardId::to_hex()`).
fn ok_result_with_receipt_ref(effect_id: &str, receipt_ref: &str) -> EffectResult {
    EffectResult {
        effect_id: effect_id.into(),
        success: true,
        message: "approved".into(),
        state_change_hash: Some("state-hash-abc".into()),
        ledger_entry_id: None,
        not_executed: false,
        receipt_ref: Some(receipt_ref.into()),
        outcome: None,
    }
}

/// Hard-failure result: no success, no downstream handle minted.
fn failure_result(effect_id: &str, message: &str) -> EffectResult {
    EffectResult {
        effect_id: effect_id.into(),
        success: false,
        message: message.into(),
        state_change_hash: None,
        ledger_entry_id: None,
        not_executed: false,
        receipt_ref: None,
        outcome: None,
    }
}

/// Positive parity / single-owner invariant: actor Accept → IER emitted → sink
/// fired *once* → evidence persisted *once*.
///
/// This pins the Phase-2 single-ownership decision for SDIS dispatch:
/// the production gateway wires
/// `GovernanceContext.on_proposal_accepted_with_evidence = None`, so the
/// only path that records evidence is the actor → event_bus →
/// DecisionExecutor → `GovernanceDispatchEvidenceSink` chain exercised
/// here. See `crates/icn-gateway/src/server.rs` (`GovernanceContext`
/// construction) for the wiring.
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

    actor_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: proposal_id.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await
        .expect("CloseProposal submit");

    let iers = backend.effects_for(&proposal_id.0);
    assert_eq!(iers.len(), 1, "actor close must emit exactly one IER");
    let ier = &iers[0];
    assert_eq!(ier.effect_kind, "appoint_steward");

    // Build the sink directly over the backend — no GovernanceManager
    // coupling is required. The backend is the same Arc the gateway
    // would install in production.
    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
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
    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

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

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

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
        receipt_ref: None,
        outcome: None,
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
    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();

    let effects = vec![sdis_approve_effect(&candidate, "anything")];
    let results = vec![ok_result("eff-m")];
    // Missing `gov:` prefix and `:receipt` suffix.
    sink.record_effects("not-a-governance-id", &effects, &results, 1_700_000_000);

    assert!(backend.evidence_for("anything").is_empty());
}

// =============================================================================
// Evidence-fidelity: receipt_ref carries the downstream service handle
// =============================================================================
//
// These three tests pin the Phase-2b seam: `EffectResult.receipt_ref`, set
// by the executing service (`SdisServiceImpl` publishes the commons
// `StewardId::to_hex()`), is forwarded verbatim into
// `EffectDispatchEvidence.receipt_ref`. This is what restores the durable
// steward-handle trail we lost when the gateway HTTP hook was removed,
// without reintroducing a second dispatch owner.

/// Successful AppointSteward: receipt_ref from the service result must
/// appear on the persisted evidence row exactly as produced.
#[tokio::test(flavor = "current_thread")]
async fn appoint_steward_evidence_carries_service_receipt_ref() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-appoint-rref",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:steward-a".into()),
        Some("region-a".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-appoint-rref")];
    // The real commons handle minted by register_steward — content-addressed,
    // 64-hex-chars. We pass a representative literal; the sink must forward
    // it verbatim without interpreting or rewriting.
    let steward_id_hex =
        "b1946ac92492d2347c6235b4d2611184dc2e8d6b6fbe7d42a3a0b4e9ef6e2a11".to_string();
    let results = vec![ok_result_with_receipt_ref("eff-appoint", &steward_id_hex)];

    sink.record_effects(
        "gov:domain-a:prop-appoint-rref:receipt",
        &effects,
        &results,
        1_700_010_000,
    );

    let ev = backend.evidence_for("prop-appoint-rref");
    assert_eq!(
        ev.len(),
        1,
        "single-owner invariant: exactly one evidence row for one dispatch"
    );
    assert_eq!(
        ev[0].receipt_ref,
        Some(steward_id_hex),
        "sink must forward EffectResult.receipt_ref verbatim; no synthesis"
    );
    assert_eq!(ev[0].effect_record_id, ier.record_id);
    assert_eq!(ev[0].subsystem, "sdis");
    assert!(ev[0].success);
    assert!(ev[0].error_message.is_none());
}

/// Successful RevokeSteward: same seam. Distinct data shape from appoint
/// because the service looks up the steward record and publishes its id.
#[tokio::test(flavor = "current_thread")]
async fn revoke_steward_evidence_carries_service_receipt_ref() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-revoke-rref",
        "coop",
        None,
        "revoke_steward",
        Some("did:icn:steward-b".into()),
        None,
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    // RevokeSteward is a distinct effect shape — build it directly.
    let effects = vec![KernelEffect::Sdis(
        icn_kernel_api::effects::SdisEffect::RevokeSteward {
            steward_did: candidate.to_string(),
            reason: "term ended".into(),
        },
    )];
    let steward_id_hex =
        "a0b4e9ef6e2a11b1946ac92492d2347c6235b4d2611184dc2e8d6b6fbe7d42a3".to_string();
    let results = vec![ok_result_with_receipt_ref("eff-revoke", &steward_id_hex)];

    sink.record_effects(
        "gov:domain-b:prop-revoke-rref:receipt",
        &effects,
        &results,
        1_700_010_001,
    );

    let ev = backend.evidence_for("prop-revoke-rref");
    assert_eq!(ev.len(), 1);
    assert_eq!(
        ev[0].receipt_ref,
        Some(steward_id_hex),
        "RevokeSteward must forward the service-layer steward_id the revoke was routed through"
    );
    assert_eq!(ev[0].subsystem, "sdis");
    assert!(ev[0].success);
}

/// No-op revoke (success + no downstream handle) must be persisted as a
/// distinct durable row from a real revocation: `success = true` with
/// `receipt_ref = None` and no `error_message`. This is what the
/// kernel executor's no-op branch now emits when the service reports
/// `receipt_ref = None` on the success path.
#[tokio::test(flavor = "current_thread")]
async fn revoke_steward_noop_evidence_has_no_receipt_ref_but_is_success() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-revoke-noop",
        "coop",
        None,
        "revoke_steward",
        Some("did:icn:steward-ghost".into()),
        None,
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![KernelEffect::Sdis(
        icn_kernel_api::effects::SdisEffect::RevokeSteward {
            steward_did: candidate.to_string(),
            reason: "never was steward".into(),
        },
    )];
    // Emulates what the kernel executor now produces for the no-op
    // branch: success=true, state_change_hash=None, receipt_ref=None.
    let results = vec![EffectResult {
        effect_id: "eff-revoke-noop".into(),
        success: true,
        message: format!(
            "Steward {} revoke was a no-op (no active record)",
            candidate
        ),
        state_change_hash: None,
        ledger_entry_id: None,
        not_executed: false,
        receipt_ref: None,
        outcome: None,
    }];

    sink.record_effects(
        "gov:domain-b:prop-revoke-noop:receipt",
        &effects,
        &results,
        1_700_010_003,
    );

    let ev = backend.evidence_for("prop-revoke-noop");
    assert_eq!(ev.len(), 1);
    assert!(
        ev[0].success,
        "no-op revoke is a success at the governance layer — it satisfies the decision idempotently"
    );
    assert_eq!(
        ev[0].receipt_ref, None,
        "no active record → no handle to attribute → receipt_ref must stay None"
    );
    assert!(
        ev[0].error_message.is_none(),
        "no-op is not a failure; error_message must be None so audit can \
         distinguish it from a hard commons failure"
    );
}

/// Successful SuspendSteward: the service publishes the commons
/// `StewardId::to_hex()` and the sink persists it verbatim on the
/// evidence row. Same seam as appoint/revoke/reconfirm.
#[tokio::test(flavor = "current_thread")]
async fn suspend_steward_evidence_carries_service_receipt_ref() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-suspend-rref",
        "coop",
        None,
        "suspend_steward",
        Some("did:icn:steward-s".into()),
        None,
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![KernelEffect::Sdis(
        icn_kernel_api::effects::SdisEffect::SuspendSteward {
            steward_did: candidate.to_string(),
            reason: "under investigation".into(),
            duration_seconds: 86_400,
            proposal_id: "prop-suspend-rref".into(),
        },
    )];
    let steward_id_hex =
        "c2a7bd1c4ee5fa0318d7c88a3b44f66b1e9d5e72c3a74b6cf88a1e0c2d3e4f50".to_string();
    let results = vec![ok_result_with_receipt_ref("eff-suspend", &steward_id_hex)];

    sink.record_effects(
        "gov:domain-s:prop-suspend-rref:receipt",
        &effects,
        &results,
        1_700_020_001,
    );

    let ev = backend.evidence_for("prop-suspend-rref");
    assert_eq!(ev.len(), 1);
    assert_eq!(
        ev[0].receipt_ref,
        Some(steward_id_hex),
        "SuspendSteward must forward the steward_id the suspend was routed through"
    );
    assert_eq!(ev[0].subsystem, "sdis");
    assert!(ev[0].success);
    assert!(ev[0].error_message.is_none());
}

/// Successful ReinstateSteward (active-reinstate path): receipt_ref
/// carries the real steward_id. The sink does not look at
/// `was_suspended` — that flag lives only in the service-level result.
#[tokio::test(flavor = "current_thread")]
async fn reinstate_steward_evidence_carries_service_receipt_ref() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-reinstate-rref",
        "coop",
        None,
        "reinstate_steward",
        Some("did:icn:steward-r".into()),
        None,
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![KernelEffect::Sdis(
        icn_kernel_api::effects::SdisEffect::ReinstateSteward {
            steward_did: candidate.to_string(),
            proposal_id: "prop-reinstate-rref".into(),
        },
    )];
    let steward_id_hex =
        "d38e0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab".to_string();
    let results = vec![ok_result_with_receipt_ref("eff-reinstate", &steward_id_hex)];

    sink.record_effects(
        "gov:domain-r:prop-reinstate-rref:receipt",
        &effects,
        &results,
        1_700_020_002,
    );

    let ev = backend.evidence_for("prop-reinstate-rref");
    assert_eq!(ev.len(), 1);
    assert_eq!(
        ev[0].receipt_ref,
        Some(steward_id_hex),
        "ReinstateSteward must forward the steward_id the reinstate was routed through"
    );
    assert_eq!(ev[0].subsystem, "sdis");
    assert!(ev[0].success);
}

/// Successful SanctionSteward: the service minted a real receipt_ref
/// (the steward_id the slash and optional suspend were routed through).
/// Sink must persist it verbatim on the evidence row.
#[tokio::test(flavor = "current_thread")]
async fn sanction_steward_evidence_carries_service_receipt_ref() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-sanction-rref",
        "coop",
        None,
        "sanction_steward",
        Some("did:icn:steward-x".into()),
        None,
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![KernelEffect::Sdis(
        icn_kernel_api::effects::SdisEffect::SanctionSteward {
            steward_did: candidate.to_string(),
            bond_slash_amount: 250,
            suspend_reason: "serious breach".into(),
            reason: "serious breach".into(),
            proposal_id: "prop-sanction-rref".into(),
        },
    )];
    let steward_id_hex =
        "e4f501234567899876543210fedcba987654321089abcdef0123456789abcd01".to_string();
    let results = vec![ok_result_with_receipt_ref("eff-sanction", &steward_id_hex)];

    sink.record_effects(
        "gov:domain-x:prop-sanction-rref:receipt",
        &effects,
        &results,
        1_700_020_003,
    );

    let ev = backend.evidence_for("prop-sanction-rref");
    assert_eq!(ev.len(), 1);
    assert_eq!(
        ev[0].receipt_ref,
        Some(steward_id_hex),
        "SanctionSteward must forward the steward_id the slash was routed through"
    );
    assert_eq!(ev[0].subsystem, "sdis");
    assert!(ev[0].success);
}

/// Suspend/Reinstate/Sanction hard-failure paths (no record, commons
/// error): the kernel executor emits `success = false, receipt_ref =
/// None`, and the sink must persist that faithfully. Pinned as one
/// test because the sink logic is identical across all three effect
/// kinds — only the effect enum variant differs.
#[tokio::test(flavor = "current_thread")]
async fn lifecycle_failure_evidence_has_no_receipt_ref() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    for (idx, (effect_kind, prop)) in [
        ("suspend_steward", "prop-suspend-fail"),
        ("reinstate_steward", "prop-reinstate-fail"),
        ("sanction_steward", "prop-sanction-fail"),
    ]
    .iter()
    .enumerate()
    {
        let ier = InstitutionalEffectRecord::new(
            *prop,
            "coop",
            None,
            *effect_kind,
            Some(format!("did:icn:ghost-{idx}")),
            None,
            None,
            1,
            serde_json::json!({}),
        );
        backend.put_institutional_effect(&ier).unwrap();
    }

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();

    let cases: [(KernelEffect, &str, &str); 3] = [
        (
            KernelEffect::Sdis(icn_kernel_api::effects::SdisEffect::SuspendSteward {
                steward_did: candidate.to_string(),
                reason: "x".into(),
                duration_seconds: 3_600,
                proposal_id: "prop-suspend-fail".into(),
            }),
            "prop-suspend-fail",
            "commons lookup failed: no record",
        ),
        (
            KernelEffect::Sdis(icn_kernel_api::effects::SdisEffect::ReinstateSteward {
                steward_did: candidate.to_string(),
                proposal_id: "prop-reinstate-fail".into(),
            }),
            "prop-reinstate-fail",
            "commons lookup failed: no record",
        ),
        (
            KernelEffect::Sdis(icn_kernel_api::effects::SdisEffect::SanctionSteward {
                steward_did: candidate.to_string(),
                bond_slash_amount: 10,
                suspend_reason: String::new(),
                reason: "x".into(),
                proposal_id: "prop-sanction-fail".into(),
            }),
            "prop-sanction-fail",
            "commons lookup failed: no record",
        ),
    ];

    for (i, (effect, proposal_id, err_msg)) in cases.iter().enumerate() {
        let effects = vec![effect.clone()];
        let results = vec![failure_result(&format!("eff-fail-{i}"), err_msg)];
        sink.record_effects(
            &format!("gov:domain-f:{proposal_id}:receipt"),
            &effects,
            &results,
            1_700_030_000 + i as u64,
        );

        let ev = backend.evidence_for(proposal_id);
        assert_eq!(ev.len(), 1, "each effect kind produces one evidence row");
        assert!(!ev[0].success, "failure must not be persisted as success");
        assert_eq!(
            ev[0].receipt_ref, None,
            "failure paths must truthfully leave receipt_ref unset across all lifecycle operations"
        );
        assert_eq!(
            ev[0].error_message.as_deref(),
            Some(*err_msg),
            "error_message must carry the service-layer diagnostic verbatim"
        );
    }
}

/// Failure path: when the service produced no downstream handle
/// (e.g. commons.register_steward failed, revoke hit an idempotent
/// no-op with no active record), `receipt_ref` must remain `None` on
/// the durable evidence row. No synthesis, no placeholders.
#[tokio::test(flavor = "current_thread")]
async fn failure_evidence_has_no_receipt_ref() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-fail-rref",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:steward-c".into()),
        Some("region-c".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-fail-rref")];
    let results = vec![failure_result(
        "eff-fail",
        "commons.register_steward failed",
    )];

    sink.record_effects(
        "gov:domain-c:prop-fail-rref:receipt",
        &effects,
        &results,
        1_700_010_002,
    );

    let ev = backend.evidence_for("prop-fail-rref");
    assert_eq!(ev.len(), 1);
    assert_eq!(
        ev[0].receipt_ref, None,
        "failure paths must truthfully leave receipt_ref unset"
    );
    assert!(!ev[0].success);
    assert_eq!(
        ev[0].error_message.as_deref(),
        Some("commons.register_steward failed"),
        "error_message still carries the service-layer diagnostic"
    );
}

/// Regression pin for the Phase-2 single-ownership decision.
///
/// The sink itself does NOT deduplicate: if two independent acceptance
/// paths both call `record_effects` for the same `(proposal_id,
/// effect_record_id)`, the backend receives two evidence rows. This was
/// the duplicate-dispatch pathology in the previous gateway wiring —
/// the HTTP close path's `on_proposal_accepted_with_evidence` hook and
/// the actor/kernel-executor path each fired the sink for the same IER.
///
/// This test documents the invariant by showing the failure mode in
/// isolation: two sink invocations = two rows. The fix lives at the
/// wiring layer (gateway `GovernanceContext` sets the HTTP hook to
/// `None`), not inside the sink. Any future attempt to restore a
/// second dispatch owner must also introduce sink-level de-duplication,
/// or this invariant breaks.
#[tokio::test(flavor = "current_thread")]
async fn two_sink_invocations_produce_duplicate_evidence_rows() {
    let backend = Arc::new(MemoryReceiptBackend::new());

    // Seed the IER the sink will link against.
    let ier = InstitutionalEffectRecord::new(
        "prop-dup",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:steward".into()),
        Some("region-d".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-dup")];
    let results = vec![ok_result("eff-dup")];
    let receipt_id = "gov:domain-d:prop-dup:receipt";

    // Simulate: kernel-executor path fires the sink.
    sink.record_effects(receipt_id, &effects, &results, 1_700_000_100);
    // Simulate: HTTP hook *also* fires (the bug the Phase-2 change removes).
    sink.record_effects(receipt_id, &effects, &results, 1_700_000_101);

    let ev = backend.evidence_for("prop-dup");
    assert_eq!(
        ev.len(),
        2,
        "sink is intentionally non-deduplicating; two invocations must yield \
         two rows. Production wiring must ensure exactly one owner calls it \
         per dispatch."
    );
}

/// Daemon-bootstrap wiring parity:
///
/// The daemon constructs a [`DeferredDispatchEvidenceSink`] *before* the
/// gateway opens the backing `ReceiptStore`. Writes routed to the
/// deferred sink before `install_backend` must silently drop (honest
/// best-effort); writes after install must reach the backend exactly
/// as the concrete `GovernanceDispatchEvidenceSink` would.
///
/// This test pins the deferred seam end-to-end at the app layer:
/// build the deferred sink as `Arc<dyn DispatchEvidenceSink>` (the
/// shape the kernel receives from `BootstrapHandles`), prove pre-install
/// drops are no-ops, then call `install_backend` (the same call the
/// gateway server makes after opening the receipt store) and prove
/// the next record_effects actually persists.
#[tokio::test(flavor = "current_thread")]
async fn deferred_sink_drops_before_install_then_persists_after() {
    let backend = Arc::new(MemoryReceiptBackend::new());

    // Seed an IER so the post-install write has something to link.
    let ier = InstitutionalEffectRecord::new(
        "prop-deferred",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:steward".into()),
        Some("region-z".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let deferred = Arc::new(DeferredDispatchEvidenceSink::new());
    assert!(!deferred.is_installed());

    // Hand to the kernel as the trait object BootstrapHandles carries.
    let as_trait: Arc<dyn DispatchEvidenceSink> = deferred.clone();

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-deferred")];
    let results = vec![ok_result("eff-pre")];

    // PRE-INSTALL: must drop silently. No writes, no panic.
    as_trait.record_effects(
        "gov:domain-z:prop-deferred:receipt",
        &effects,
        &results,
        1_700_000_001,
    );
    assert!(
        backend.evidence_for("prop-deferred").is_empty(),
        "pre-install record_effects must not touch the backend"
    );

    // Daemon/gateway seam: install the real backend once receipt_store is ready.
    deferred.install_backend(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
    assert!(deferred.is_installed());

    // POST-INSTALL: the same trait-object handle now persists.
    let results2 = vec![ok_result("eff-post")];
    as_trait.record_effects(
        "gov:domain-z:prop-deferred:receipt",
        &effects,
        &results2,
        1_700_000_002,
    );

    let ev = backend.evidence_for("prop-deferred");
    assert_eq!(
        ev.len(),
        1,
        "post-install record_effects must persist exactly one evidence row; got {ev:?}"
    );
    assert_eq!(ev[0].effect_record_id, ier.record_id);
    assert_eq!(ev[0].subsystem, "sdis");
    assert!(ev[0].success);
    assert_eq!(ev[0].recorded_at, 1_700_000_002);
}

/// Second install is a warning-logged no-op, not a panic. The daemon
/// contract is "install exactly once"; this asserts the sink stays
/// stable if that contract is violated (e.g. a restart path accidentally
/// re-invokes the gateway startup sequence).
#[tokio::test(flavor = "current_thread")]
async fn deferred_sink_second_install_is_idempotent_no_op() {
    let backend_a = Arc::new(MemoryReceiptBackend::new());
    let backend_b = Arc::new(MemoryReceiptBackend::new());

    let ier = InstitutionalEffectRecord::new(
        "prop-idem",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:steward".into()),
        Some("region-z".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend_a.put_institutional_effect(&ier).unwrap();
    backend_b.put_institutional_effect(&ier).unwrap();

    let deferred = DeferredDispatchEvidenceSink::new();
    deferred.install_backend(backend_a.clone() as Arc<dyn GovernanceReceiptBackend>);
    deferred.install_backend(backend_b.clone() as Arc<dyn GovernanceReceiptBackend>);

    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-idem")];
    let results = vec![ok_result("eff-idem")];
    deferred.record_effects(
        "gov:domain-z:prop-idem:receipt",
        &effects,
        &results,
        1_700_000_003,
    );

    // First backend wins; second is silently ignored.
    assert_eq!(
        backend_a.evidence_for("prop-idem").len(),
        1,
        "first-installed backend must be the one that receives writes"
    );
    assert!(
        backend_b.evidence_for("prop-idem").is_empty(),
        "second install must not rebind the backend"
    );
}

// =============================================================================
// Phase 2: EffectResult.outcome → EffectDispatchEvidence.outcome forwarding
// =============================================================================
//
// Pins that the sink copies `result.outcome` through to the durable evidence
// row verbatim — no reclassification, no reconstruction from (success,
// state_change_hash, receipt_ref). Audit can distinguish applied / no_op /
// partial / failed without guessing.

/// Successful appoint: outcome=Applied is persisted.
#[tokio::test(flavor = "current_thread")]
async fn sink_forwards_applied_outcome() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-outcome-applied",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:s".into()),
        Some("r".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-outcome-applied")];
    let results = vec![EffectResult {
        effect_id: "eff-app".into(),
        success: true,
        message: "ok".into(),
        state_change_hash: Some("hash".into()),
        ledger_entry_id: None,
        not_executed: false,
        receipt_ref: Some("handle".into()),
        outcome: Some(EffectOutcome::Applied),
    }];

    sink.record_effects(
        "gov:d:prop-outcome-applied:receipt",
        &effects,
        &results,
        1_700_040_000,
    );

    let ev = backend.evidence_for("prop-outcome-applied");
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].outcome, Some(EffectOutcome::Applied));
}

/// No-op revoke: outcome=NoOp is persisted — distinct from Applied even
/// though success=true in both.
#[tokio::test(flavor = "current_thread")]
async fn sink_forwards_noop_outcome() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-outcome-noop",
        "coop",
        None,
        "revoke_steward",
        Some("did:icn:s".into()),
        None,
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![KernelEffect::Sdis(
        icn_kernel_api::effects::SdisEffect::RevokeSteward {
            steward_did: candidate.to_string(),
            reason: "x".into(),
        },
    )];
    let results = vec![EffectResult {
        effect_id: "eff-noop".into(),
        success: true,
        message: "no active record".into(),
        state_change_hash: None,
        ledger_entry_id: None,
        not_executed: false,
        receipt_ref: None,
        outcome: Some(EffectOutcome::NoOp),
    }];

    sink.record_effects(
        "gov:d:prop-outcome-noop:receipt",
        &effects,
        &results,
        1_700_040_001,
    );

    let ev = backend.evidence_for("prop-outcome-noop");
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].outcome, Some(EffectOutcome::NoOp));
    assert!(ev[0].success);
}

/// Partial sanction: outcome=Partial is persisted, with receipt_ref and
/// state_change_hash preserved. This is the Codex P1 invariant.
#[tokio::test(flavor = "current_thread")]
async fn sink_forwards_partial_outcome_preserving_attribution() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-outcome-partial",
        "coop",
        None,
        "sanction_steward",
        Some("did:icn:s".into()),
        None,
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![KernelEffect::Sdis(
        icn_kernel_api::effects::SdisEffect::SanctionSteward {
            steward_did: candidate.to_string(),
            bond_slash_amount: 50,
            suspend_reason: "breach".into(),
            reason: "breach".into(),
            proposal_id: "prop-outcome-partial".into(),
        },
    )];
    let results = vec![EffectResult {
        effect_id: "eff-partial".into(),
        success: false,
        message: "bond slashed but suspend failed: io".into(),
        state_change_hash: Some("hash".into()),
        ledger_entry_id: None,
        not_executed: false,
        receipt_ref: Some("handle".into()),
        outcome: Some(EffectOutcome::Partial),
    }];

    sink.record_effects(
        "gov:d:prop-outcome-partial:receipt",
        &effects,
        &results,
        1_700_040_002,
    );

    let ev = backend.evidence_for("prop-outcome-partial");
    assert_eq!(ev.len(), 1);
    assert_eq!(
        ev[0].outcome,
        Some(EffectOutcome::Partial),
        "partial mutation must be classified Partial on the durable row, \
         not conflated with Failed"
    );
    assert!(!ev[0].success);
    assert_eq!(
        ev[0].receipt_ref.as_deref(),
        Some("handle"),
        "partial row must still carry the downstream handle"
    );
}

/// Hard failure with no downstream handle: outcome=Failed is persisted and
/// no attribution leaks onto the row.
#[tokio::test(flavor = "current_thread")]
async fn sink_forwards_failed_outcome() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-outcome-failed",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:s".into()),
        Some("r".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-outcome-failed")];
    let results = vec![EffectResult {
        effect_id: "eff-fail".into(),
        success: false,
        message: "commons error".into(),
        state_change_hash: None,
        ledger_entry_id: None,
        not_executed: false,
        receipt_ref: None,
        outcome: Some(EffectOutcome::Failed),
    }];

    sink.record_effects(
        "gov:d:prop-outcome-failed:receipt",
        &effects,
        &results,
        1_700_040_003,
    );

    let ev = backend.evidence_for("prop-outcome-failed");
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].outcome, Some(EffectOutcome::Failed));
    assert!(!ev[0].success);
    assert!(ev[0].receipt_ref.is_none());
}

/// Legacy result with `outcome: None` must persist as `None` (unclassified),
/// not silently upgraded. This preserves the "None = unknown" convention so
/// audit can distinguish pre-Phase-2 evidence from post-Phase-2 rows.
#[tokio::test(flavor = "current_thread")]
async fn sink_forwards_legacy_none_outcome_as_unclassified() {
    let backend = Arc::new(MemoryReceiptBackend::new());
    let ier = InstitutionalEffectRecord::new(
        "prop-outcome-legacy",
        "coop",
        None,
        "appoint_steward",
        Some("did:icn:s".into()),
        Some("r".into()),
        None,
        1,
        serde_json::json!({}),
    );
    backend.put_institutional_effect(&ier).unwrap();

    let sink =
        GovernanceDispatchEvidenceSink::new(backend.clone() as Arc<dyn GovernanceReceiptBackend>);
    let candidate: icn_identity::Did = IdentityBundle::generate().unwrap().did().clone();
    let effects = vec![sdis_approve_effect(&candidate, "prop-outcome-legacy")];
    let results = vec![ok_result("eff-legacy")]; // helper produces outcome=None

    sink.record_effects(
        "gov:d:prop-outcome-legacy:receipt",
        &effects,
        &results,
        1_700_040_004,
    );

    let ev = backend.evidence_for("prop-outcome-legacy");
    assert_eq!(ev.len(), 1);
    assert_eq!(
        ev[0].outcome, None,
        "sink must not synthesize an outcome when the result left it unset"
    );
}
