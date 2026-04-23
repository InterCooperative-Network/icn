//! Actor-path receipt-store parity: force-close-accept emits
//! `InstitutionalEffectRecord` after `install_receipt_store`.
//!
//! ## What this pins
//!
//! Before this test existed, the actor-backed governance path could emit
//! `InstitutionalEffectRecord` only if the actor was spawned with a
//! receipt_store wired in at construction. In production wiring the gateway
//! creates the receipt_store *after* the actor is spawned (the gateway opens
//! the sled DB that the receipt store is backed by). The builder-style
//! `GovernanceHandle::with_receipt_store(self, ...)` consumes the handle and
//! is incompatible with that order — so the only writer of
//! `InstitutionalEffectRecord` was the HTTP `close_proposal` handler.
//!
//! Consequences: force-close-accept and deadline-driven auto-close (both of
//! which run *inside* the actor without returning through HTTP) produced no
//! institutional artifact. Audit verify would report "no institutional
//! effect" for any AppointSteward / RevokeSteward that landed via
//! force-close or scheduler — even though the decision was durable.
//!
//! This test pins the parity fix: after installing the receipt backend via
//! the shared-ref setter `install_receipt_store(&self, ...)`, force-close
//! accept of an `AppointSteward` proposal emits a durable
//! `InstitutionalEffectRecord` with `effect_kind == "appoint_steward"`.
//! Same pinning for `RemoveSteward` / `effect_kind == "revoke_steward"`.
//!
//! ## What this does NOT prove
//!
//! - End-to-end dispatch to the commons steward service (that is the
//!   gateway's `on_proposal_accepted_with_evidence` hook — still
//!   gateway-only).
//! - Deadline-driven auto-close (tested separately by the scheduler tests).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    sdis::SdisProposal, ForcedOutcome, GovernanceDomainId, GovernanceOps, GovernanceParams,
    MembershipConfig, ProposalId, ProposalPayload, ProposalScope, StaticMembershipResolver,
};
use icn_governance_actor::{
    actor::{GovernanceActor, GovernanceCommand},
    institutional_effect::InstitutionalEffectRecord,
    manager::GovernanceManager,
    receipt_backend::GovernanceReceiptBackend,
};
use icn_identity::IdentityBundle;
use icn_store::SledStore;
use std::sync::{Arc, Mutex};

/// In-memory receipt backend that records only `InstitutionalEffectRecord`
/// writes — that is the artifact under test.
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

/// Drive a domain + open AppointSteward proposal up to the force-close seam,
/// install the receipt_store *after* open (mirroring production order where
/// the gateway creates the backend after actor spawn), then force-accept and
/// assert the durable `InstitutionalEffectRecord` was written.
#[tokio::test]
async fn actor_force_close_accept_emits_appoint_steward_effect() {
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

    let domain_id = GovernanceDomainId("steward-parity-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );

    manager
        .create_domain(
            domain_id.clone(),
            "Steward Parity Domain".to_string(),
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
            region: "region-north".to_string(),
            bond_amount: 1_000,
            term_length: 3600 * 24 * 30,
        },
    };

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Appoint steward Alice".to_string(),
            "Parity test".to_string(),
            payload,
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    // Install the receipt_store AFTER open — this mirrors production wiring
    // where the gateway opens its sled DB (and builds ReceiptStore) only
    // after the supervisor has already spawned the actor.
    let backend = Arc::new(MemoryReceiptBackend::new());
    actor_handle.install_receipt_store(backend.clone()).await;

    // Force-close accept. The normal close path would also emit, but the
    // specific regression this test pins is the actor-path force-close.
    actor_handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id: proposal_id.clone(),
            forced_outcome: ForcedOutcome::Accept,
            reason: "parity test force-accept".to_string(),
        })
        .await
        .expect("ForceCloseProposal submit");

    let effects = backend.effects_for(&proposal_id.0);
    assert_eq!(
        effects.len(),
        1,
        "force-close-accept must emit exactly one InstitutionalEffectRecord after receipt_store install; got {effects:?}"
    );
    let rec = &effects[0];
    assert_eq!(rec.effect_kind, "appoint_steward");
    assert_eq!(rec.target_did.as_deref(), Some(candidate_did.as_str()));
    assert_eq!(rec.target_ref.as_deref(), Some("region-north"));

    actor_handle.shutdown().await;
}

/// Same pin for `RemoveSteward` / `effect_kind == "revoke_steward"` — this
/// is the other half of the steward-authority slice called out in the
/// parity directive.
#[tokio::test]
async fn actor_force_close_accept_emits_revoke_steward_effect() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let did = bundle.did().clone();
    let steward_bundle = IdentityBundle::generate().expect("steward keypair");
    let steward_did = steward_bundle.did().clone();

    let store = Arc::new(SledStore::open(&db_path).expect("SledStore::open"));
    let gossip = gossip_with_governance_topic(did.clone()).await;
    let resolver = Arc::new(StaticMembershipResolver::new());

    let actor_handle =
        GovernanceActor::spawn(did.clone(), store.clone(), gossip, resolver, None, None)
            .await
            .expect("GovernanceActor::spawn");

    let domain_id = GovernanceDomainId("revoke-parity-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );

    manager
        .create_domain(
            domain_id.clone(),
            "Revoke Parity Domain".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![did.clone()]),
        )
        .await
        .expect("create_domain");

    let payload = ProposalPayload::Sdis {
        proposal: SdisProposal::RemoveSteward {
            steward: steward_did.clone(),
            reason: "breach of duty".to_string(),
            return_bond: false,
        },
    };

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "Remove steward Bob".to_string(),
            "Parity test".to_string(),
            payload,
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    let backend = Arc::new(MemoryReceiptBackend::new());
    actor_handle.install_receipt_store(backend.clone()).await;

    actor_handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id: proposal_id.clone(),
            forced_outcome: ForcedOutcome::Accept,
            reason: "parity test force-accept".to_string(),
        })
        .await
        .expect("ForceCloseProposal submit");

    let effects = backend.effects_for(&proposal_id.0);
    assert_eq!(
        effects.len(),
        1,
        "force-close-accept must emit exactly one InstitutionalEffectRecord after receipt_store install; got {effects:?}"
    );
    let rec = &effects[0];
    assert_eq!(rec.effect_kind, "revoke_steward");
    assert_eq!(rec.target_did.as_deref(), Some(steward_did.as_str()));
    assert_eq!(rec.reason.as_deref(), Some("breach of duty"));

    actor_handle.shutdown().await;
}

/// Execution-required proposals cannot be force-accepted without a receipt_store.
/// The actor must reject the terminal transition rather than silently accepting
/// an operationally unlinked decision.
#[tokio::test]
async fn actor_force_close_accept_without_receipt_store_is_rejected() {
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

    let domain_id = GovernanceDomainId("nostore-domain".to_string());
    let manager = GovernanceManager::with_handle(
        Arc::new(actor_handle.clone()) as Arc<dyn GovernanceOps + Send + Sync>
    );

    manager
        .create_domain(
            domain_id.clone(),
            "No Store Domain".to_string(),
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
            region: "r".to_string(),
            bond_amount: 1,
            term_length: 3600,
        },
    };

    let proposal_id = manager
        .create_proposal(
            ProposalId("_ignored".to_string()),
            domain_id.clone(),
            did.clone(),
            "No receipt store".to_string(),
            "".to_string(),
            payload,
            ProposalScope::Local,
        )
        .await
        .expect("create_proposal");

    manager
        .open_proposal(proposal_id.clone(), 3600)
        .await
        .expect("open_proposal");

    // NOTE: no install_receipt_store call.
    let result = actor_handle
        .submit(GovernanceCommand::ForceCloseProposal {
            proposal_id: proposal_id.clone(),
            forced_outcome: ForcedOutcome::Accept,
            reason: "no backend".to_string(),
        })
        .await;
    assert!(
        result
            .as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("requires execution closure")),
        "force-accept without receipt_store must be rejected with closure error; got {result:?}"
    );

    actor_handle.shutdown().await;
}
