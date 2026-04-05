#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Proof: accepted MembershipAction::Remove proposals produce durable sled
//! state mutations with verifiable governance provenance that survive restarts.
//!
//! Chain under test:
//!   ProposalAccepted { canonical_payload_hash }
//!   → create_effect_subscription
//!   → EffectDispatcher::execute_effects
//!   → KernelGovernanceExecutor (MembershipEffect::RemoveMember)
//!   → MembershipServiceImpl::remove_member
//!   → CoopStore::save_member (sled) with provenance in member.metadata
//!   → Cold-cache restart: get_membership_provenance() reads from sled metadata
//!
//! Proves:
//! 1. member.status == Removed after remove proposal
//! 2. gov_decision_receipt_id in member.metadata == "gov:{domain}:{proposal}:receipt"
//! 3. gov_decision_hash in member.metadata == canonical_payload_hash from event
//! 4. Both values survive sled restart (cold-cache path)
//! 5. Legacy (no canonical_hash): blake3 fallback is stored

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

fn open_coop_store(path: &std::path::Path) -> Arc<CoopStore> {
    let db = sled::open(path).expect("Failed to open sled db");
    Arc::new(CoopStore::new(Arc::new(db)))
}

fn seed_active_member(store: &Arc<CoopStore>, domain_id: &str, did: &icn_identity::Did) {
    use icn_coop::{Member, MemberRole, MemberStatus};
    let mut member = Member::new(did.clone(), domain_id.to_string(), MemberRole::Member);
    member.status = MemberStatus::Active;
    store.save_member(&member).expect("seed_active_member");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1 — Remove: canonical hash forwarded, sled-durable, restart-safe
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn remove_member_accepted_proposal_produces_durable_record_with_governance_provenance(
) -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Remove Bridge: ProposalAccepted → durable Removed state ===");

    let tmp = TempDir::new()?;
    let store_path = tmp.path().join("coop-store-remove");

    let member_kp = KeyPair::generate()?;
    let member_did = member_kp.did().clone();

    let domain_id = "test-coop-remove";
    let proposal_id = "remove-member-prop-001";
    let canonical_payload_hash = "sha256:remove-canonical-hash-001".to_string();
    let expected_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");

    // ── Phase A: fire event ───────────────────────────────────────────────────
    {
        let coop_store = open_coop_store(&store_path);
        seed_active_member(&coop_store, domain_id, &member_did);

        let membership_service = Arc::new(MembershipServiceImpl::new(coop_store.clone()));
        let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
            .with_membership_service(membership_service.clone());
        let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));

        let payload = ProposalPayload::Membership {
            action: MembershipAction::Remove,
            member: member_did.clone(),
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
                        "EffectDispatcher processed remove effects"
                    ),
                    Err(e) => tracing::error!(
                        decision_receipt_id = %dr_id,
                        error = %e,
                        "EffectDispatcher error"
                    ),
                }
            });
        });

        subscription(SystemEvent::ProposalAccepted {
            proposal_id: proposal_id.to_string(),
            domain_id: domain_id.to_string(),
            payload: payload_value,
            decided_at: 1_700_000_050,
            canonical_payload_hash: Some(canonical_payload_hash.clone()),
            governance_decision_hash: None,
        });
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let stored = coop_store
            .get_member(domain_id, &member_did)
            .expect("Member should exist after remove (record preserved with Removed status)");

        assert_eq!(
            stored.status,
            icn_coop::MemberStatus::Removed,
            "member.status must be Removed"
        );
        assert_eq!(
            stored
                .metadata
                .get("gov_decision_receipt_id")
                .map(|s| s.as_str()),
            Some(expected_receipt_id.as_str()),
            "gov_decision_receipt_id must match"
        );
        assert_eq!(
            stored.metadata.get("gov_decision_hash").map(|s| s.as_str()),
            Some(canonical_payload_hash.as_str()),
            "gov_decision_hash must equal canonical_payload_hash"
        );
        assert_eq!(
            stored.metadata.get("gov_operation").map(|s| s.as_str()),
            Some("remove"),
            "gov_operation must be 'remove'"
        );

        info!("✅ Phase A: member removed, provenance in metadata");
        // All Arcs drop here → sled lock releases
    }

    // ── Phase B: cold-cache restart ───────────────────────────────────────────
    let reloaded_store = open_coop_store(&store_path);
    let reloaded_service = MembershipServiceImpl::new(reloaded_store);

    let prov = reloaded_service.get_membership_provenance(domain_id, &member_did.to_string());
    assert!(
        prov.is_some(),
        "get_membership_provenance must return Some after restart"
    );
    let (receipt_id_after, hash_after) = prov.unwrap();
    assert_eq!(
        receipt_id_after, expected_receipt_id,
        "receipt_id must match"
    );
    assert_eq!(
        hash_after, canonical_payload_hash,
        "decision_hash must match canonical"
    );

    info!("✅ Phase B: cold-cache restart — remove provenance recovered from sled metadata");

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2 — Remove legacy path (no canonical_hash): blake3 fallback
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn remove_member_without_canonical_hash_falls_back_to_receipt_hash() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    let tmp = TempDir::new()?;
    let store_path = tmp.path().join("coop-store-remove-legacy");

    let member_kp = KeyPair::generate()?;
    let member_did = member_kp.did().clone();

    let domain_id = "test-coop-remove-legacy";
    let proposal_id = "remove-legacy-001";
    let expected_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");

    let coop_store = open_coop_store(&store_path);
    seed_active_member(&coop_store, domain_id, &member_did);

    let membership_service = Arc::new(MembershipServiceImpl::new(coop_store.clone()));
    let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
        .with_membership_service(membership_service.clone());
    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));

    let payload = ProposalPayload::Membership {
        action: MembershipAction::Remove,
        member: member_did.clone(),
    };

    let dispatcher_for_sub = dispatcher.clone();
    let sub = create_effect_subscription(move |effects, dr_id| {
        let d = dispatcher_for_sub.clone();
        let id = dr_id.clone();
        tokio::task::spawn(async move {
            let _ = d.execute_effects(effects, &id).await;
        });
    });

    sub(SystemEvent::ProposalAccepted {
        proposal_id: proposal_id.to_string(),
        domain_id: domain_id.to_string(),
        payload: serde_json::to_value(&payload)?,
        decided_at: 1_700_000_060,
        canonical_payload_hash: None, // legacy: no canonical hash
        governance_decision_hash: None,
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let stored = coop_store
        .get_member(domain_id, &member_did)
        .expect("Member should exist after legacy remove");

    assert_eq!(
        stored.status,
        icn_coop::MemberStatus::Removed,
        "member must be Removed even on legacy path"
    );

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
        "receipt_id must match even on legacy path"
    );
    // blake3 fallback produces a 64-char hex string
    assert_eq!(
        stored_decision_hash.len(),
        64,
        "Fallback decision_hash must be a 64-char blake3 hex string"
    );

    info!(
        receipt_id = %stored_receipt_id,
        decision_hash = %stored_decision_hash,
        "✅ Legacy path: remove record written with blake3 fallback provenance"
    );

    Ok(())
}
