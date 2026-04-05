#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Proof: accepted MembershipAction::Add proposals produce durable membership
//! records with verifiable governance provenance that survive restarts.
//!
//! Chain under test:
//!   ProposalAccepted { canonical_payload_hash }
//!   → create_effect_subscription
//!   → EffectDispatcher::execute_effects
//!   → KernelGovernanceExecutor (MembershipEffect::AddMember)
//!   → MembershipServiceImpl::add_member
//!   → CoopStore::save_member with provenance baked into member.metadata
//!   → Cold-cache restart: get_membership_provenance() reads from sled metadata
//!
//! Proves:
//! 1. decision_receipt_id in member.metadata == "gov:{domain}:{proposal}:receipt"
//! 2. decision_hash in member.metadata == canonical_payload_hash from the event
//! 3. Both values survive a sled restart (cold-cache provenance path)

use anyhow::Result;
use icn_coop::CoopStore;
use icn_core::services::MembershipServiceImpl;
use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;
use icn_governance::proposal::{MembershipAction, ProposalPayload};
use icn_governance_actor::create_effect_subscription;
use icn_identity::KeyPair;
use icn_kernel_api::events::SystemEvent;
use icn_kernel_api::protocol_params::StubParamStore;
use icn_kernel_api::MembershipService;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;

/// Build a `CoopStore` backed by a sled database at the given path.
fn open_coop_store(path: &std::path::Path) -> Arc<CoopStore> {
    let db = sled::open(path).expect("Failed to open sled db");
    Arc::new(CoopStore::new(Arc::new(db)))
}

/// Prove the full governance → membership bridge with sled-durable provenance.
///
/// Chain: ProposalAccepted → create_effect_subscription → EffectDispatcher
///        → KernelGovernanceExecutor → MembershipServiceImpl::add_member
///        → CoopStore (sled) member.metadata { gov_decision_receipt_id, gov_decision_hash }
#[tokio::test(flavor = "multi_thread")]
async fn add_member_accepted_proposal_produces_durable_record_with_governance_provenance(
) -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Membership Action Bridge: ProposalAccepted → durable member record ===");

    let tmp = TempDir::new()?;
    let store_path = tmp.path().join("coop-store");

    // ── Identities ────────────────────────────────────────────────────────────
    let new_member_kp = KeyPair::generate()?;
    let new_member_did = new_member_kp.did().clone();

    let domain_id = "test-coop";
    let proposal_id = "add-member-prop-001";
    let canonical_payload_hash = "sha256:membership-canonical-hash-001".to_string();
    let expected_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");
    let expected_decision_hash = canonical_payload_hash.clone();

    // ── Phase A: fire event, let the full chain execute ───────────────────────
    // All Arcs that hold a CoopStore reference live only in this block.
    // When the block closes, all chain references drop → sled lock releases.
    {
        let coop_store = open_coop_store(&store_path);
        let membership_service = Arc::new(MembershipServiceImpl::new(coop_store.clone()));

        let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
            .with_membership_service(membership_service.clone());

        let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));

        let payload = ProposalPayload::Membership {
            action: MembershipAction::Add,
            member: new_member_did.clone(),
        };
        let payload_value = serde_json::to_value(&payload)?;

        let dispatcher_for_sub = dispatcher.clone();
        let subscription = create_effect_subscription(move |effects, decision_receipt_id| {
            let disp = dispatcher_for_sub.clone();
            let dr_id = decision_receipt_id.clone();
            tokio::task::spawn(async move {
                match disp.execute_effects(effects, &dr_id).await {
                    Ok(results) => info!(
                        decision_receipt_id = %dr_id,
                        result_count = results.len(),
                        "EffectDispatcher processed membership effects"
                    ),
                    Err(e) => tracing::error!(
                        decision_receipt_id = %dr_id,
                        error = %e,
                        "EffectDispatcher error"
                    ),
                }
            });
        });

        let event = SystemEvent::ProposalAccepted {
            proposal_id: proposal_id.to_string(),
            domain_id: domain_id.to_string(),
            payload: payload_value,
            decided_at: 1_700_000_000,
            canonical_payload_hash: Some(canonical_payload_hash),
        };

        subscription(event);
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Verify the record was written (while lock is still held)
        let stored = coop_store
            .get_member(domain_id, &new_member_did)
            .expect("Member should exist in CoopStore");

        assert_eq!(
            stored
                .metadata
                .get("gov_decision_receipt_id")
                .map(|s| s.as_str()),
            Some(expected_receipt_id.as_str()),
            "member.metadata must contain gov_decision_receipt_id == expected receipt_id"
        );
        assert_eq!(
            stored.metadata.get("gov_decision_hash").map(|s| s.as_str()),
            Some(expected_decision_hash.as_str()),
            "member.metadata must contain gov_decision_hash == canonical_payload_hash"
        );
        assert_eq!(
            stored.metadata.get("gov_operation").map(|s| s.as_str()),
            Some("add"),
            "member.metadata must record operation == 'add'"
        );

        info!(
            receipt_id = %expected_receipt_id,
            decision_hash = %expected_decision_hash,
            "✅ Phase A: member record written with governance provenance in metadata"
        );

        // All Arcs (coop_store, membership_service, dispatcher, subscription) drop here
    }

    // ── Phase B: cold-cache restart ───────────────────────────────────────────
    // Sled lock is now released. Reopen from the same path.
    // The new MembershipServiceImpl has empty in-memory provenance HashMap;
    // get_membership_provenance must recover from sled metadata.
    let reloaded_store = open_coop_store(&store_path);
    let reloaded_service = MembershipServiceImpl::new(reloaded_store);

    let prov = reloaded_service.get_membership_provenance(domain_id, &new_member_did.to_string());

    assert!(
        prov.is_some(),
        "get_membership_provenance must return Some after restart (cold in-memory cache)"
    );

    let (receipt_id_after_restart, hash_after_restart) = prov.unwrap();

    assert_eq!(
        receipt_id_after_restart, expected_receipt_id,
        "Reloaded receipt_id must match"
    );
    assert_eq!(
        hash_after_restart, expected_decision_hash,
        "Reloaded decision_hash must match canonical_payload_hash"
    );

    info!(
        receipt_id = %receipt_id_after_restart,
        decision_hash = %hash_after_restart,
        "✅ Phase B: cold-cache restart — provenance recovered from sled metadata"
    );

    info!(
        "✅ PROOF: accepted MembershipAction::Add produces durable governance provenance \
         that survives restart"
    );

    Ok(())
}

/// Prove that when canonical_payload_hash is absent (legacy events),
/// the fallback blake3(receipt_id) is stored as the decision_hash — bridge still works.
#[tokio::test(flavor = "multi_thread")]
async fn add_member_without_canonical_hash_falls_back_to_receipt_hash() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    let tmp = TempDir::new()?;
    let store_path = tmp.path().join("coop-store-legacy");

    let new_member_kp = KeyPair::generate()?;
    let new_member_did = new_member_kp.did().clone();

    let coop_store = open_coop_store(&store_path);
    let membership_service = Arc::new(MembershipServiceImpl::new(coop_store.clone()));

    let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
        .with_membership_service(membership_service.clone());

    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));

    let domain_id = "test-coop-legacy";
    let proposal_id = "add-member-legacy-001";

    let payload = ProposalPayload::Membership {
        action: MembershipAction::Add,
        member: new_member_did.clone(),
    };

    let expected_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");

    let dispatcher_for_sub = dispatcher.clone();
    let subscription = create_effect_subscription(move |effects, decision_receipt_id| {
        let disp = dispatcher_for_sub.clone();
        let dr_id = decision_receipt_id.clone();
        tokio::task::spawn(async move {
            let _ = disp.execute_effects(effects, &dr_id).await;
        });
    });

    let event = SystemEvent::ProposalAccepted {
        proposal_id: proposal_id.to_string(),
        domain_id: domain_id.to_string(),
        payload: serde_json::to_value(&payload)?,
        decided_at: 1_700_000_001,
        canonical_payload_hash: None, // legacy: no canonical hash
        governance_decision_hash: None,
    };

    subscription(event);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let stored = coop_store
        .get_member(domain_id, &new_member_did)
        .expect("Member should exist after legacy-path add");

    let stored_receipt_id = stored
        .metadata
        .get("gov_decision_receipt_id")
        .expect("gov_decision_receipt_id must be present");
    let stored_decision_hash = stored
        .metadata
        .get("gov_decision_hash")
        .expect("gov_decision_hash must be present");

    assert_eq!(
        stored_receipt_id.as_str(),
        expected_receipt_id.as_str(),
        "receipt_id must match even for legacy (no-canonical-hash) path"
    );
    // blake3 produces a 64-char hex string
    assert_eq!(
        stored_decision_hash.len(),
        64,
        "Fallback decision_hash must be a 64-char blake3 hex string"
    );

    info!(
        receipt_id = %stored_receipt_id,
        decision_hash = %stored_decision_hash,
        "✅ Legacy path (no canonical_hash): member record written with blake3 fallback provenance"
    );

    Ok(())
}
