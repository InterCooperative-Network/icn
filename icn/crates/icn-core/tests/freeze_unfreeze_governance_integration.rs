#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Proof: accepted FreezeMember / UnfreezeMember proposals produce durable sled
//! state mutations with verifiable governance provenance that survive restarts.
//!
//! Chain under test:
//!   ProposalAccepted { canonical_payload_hash }
//!   → create_effect_subscription
//!   → EffectDispatcher::execute_effects
//!   → KernelGovernanceExecutor (MembershipEffect::FreezeMember / UnfreezeMember)
//!   → MembershipServiceImpl::freeze_member / unfreeze_member
//!   → CoopStore::save_member (sled) with provenance in member.metadata
//!   → Cold-cache restart: get_membership_provenance() reads from sled metadata
//!
//! Proves:
//! 1. member.status == Suspended after freeze (Active after unfreeze)
//! 2. gov_decision_receipt_id in member.metadata == "gov:{domain}:{proposal}:receipt"
//! 3. gov_decision_hash in member.metadata == canonical_payload_hash from event
//! 4. Both provenance values survive a sled restart (cold-cache path)
//! 5. Legacy (no canonical_hash): fallback blake3 hash is stored

use anyhow::Result;
use icn_coop::CoopStore;
use icn_core::services::MembershipServiceImpl;
use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;
use icn_governance::proposal::ProposalPayload;
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

/// Seed a member as Active so freeze/unfreeze have something to act on.
fn seed_active_member(store: &Arc<CoopStore>, domain_id: &str, did: &icn_identity::Did) {
    use icn_coop::{Member, MemberRole, MemberStatus};
    let mut member = Member::new(did.clone(), domain_id.to_string(), MemberRole::Member);
    member.status = MemberStatus::Active;
    store.save_member(&member).expect("seed_active_member");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1 — Freeze: canonical hash forwarded, sled-durable, restart-safe
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn freeze_member_accepted_proposal_produces_durable_record_with_governance_provenance(
) -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Freeze Bridge: ProposalAccepted → durable Suspended state ===");

    let tmp = TempDir::new()?;
    let store_path = tmp.path().join("coop-store-freeze");

    let member_kp = KeyPair::generate()?;
    let member_did = member_kp.did().clone();

    let domain_id = "test-coop-freeze";
    let proposal_id = "freeze-member-prop-001";
    let canonical_payload_hash = "sha256:freeze-canonical-hash-001".to_string();
    let expected_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");

    // ── Phase A: fire event ───────────────────────────────────────────────────
    {
        let coop_store = open_coop_store(&store_path);
        seed_active_member(&coop_store, domain_id, &member_did);

        let membership_service = Arc::new(MembershipServiceImpl::new(coop_store.clone()));
        let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
            .with_membership_service(membership_service.clone());
        let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));

        let payload = ProposalPayload::FreezeMember {
            member: member_did.clone(),
            reason: "policy violation".to_string(),
            duration_seconds: Some(3600),
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
                        "EffectDispatcher processed freeze effects"
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
            decided_at: 1_700_000_010,
            canonical_payload_hash: Some(canonical_payload_hash.clone()),
            governance_decision_hash: None,
        };

        subscription(event);
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let stored = coop_store
            .get_member(domain_id, &member_did)
            .expect("Member should exist after freeze");

        // Status must be Suspended
        assert_eq!(
            stored.status,
            icn_coop::MemberStatus::Suspended,
            "member.status must be Suspended after freeze"
        );
        // Governance provenance in metadata
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
            Some("freeze"),
            "gov_operation must be 'freeze'"
        );

        info!("✅ Phase A: member frozen, provenance in metadata");
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

    info!("✅ Phase B: cold-cache restart — freeze provenance recovered from sled metadata");

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2 — Unfreeze: canonical hash forwarded, sled-durable, restart-safe
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn unfreeze_member_accepted_proposal_produces_durable_record_with_governance_provenance(
) -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Unfreeze Bridge: ProposalAccepted → durable Active state ===");

    let tmp = TempDir::new()?;
    let store_path = tmp.path().join("coop-store-unfreeze");

    let member_kp = KeyPair::generate()?;
    let member_did = member_kp.did().clone();

    let domain_id = "test-coop-unfreeze";
    let freeze_proposal_id = "freeze-prop-for-unfreeze-001";
    let unfreeze_proposal_id = "unfreeze-member-prop-001";
    let unfreeze_canonical_hash = "sha256:unfreeze-canonical-hash-001".to_string();
    let expected_unfreeze_receipt_id = format!("gov:{domain_id}:{unfreeze_proposal_id}:receipt");

    // ── Phase A: seed + freeze + unfreeze ─────────────────────────────────────
    {
        let coop_store = open_coop_store(&store_path);
        seed_active_member(&coop_store, domain_id, &member_did);

        let membership_service = Arc::new(MembershipServiceImpl::new(coop_store.clone()));
        let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
            .with_membership_service(membership_service.clone());
        let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));

        // Step 1: freeze the member first (so unfreeze has a Suspended member to act on)
        {
            let freeze_payload = ProposalPayload::FreezeMember {
                member: member_did.clone(),
                reason: "initial freeze".to_string(),
                duration_seconds: None,
            };
            let disp = dispatcher.clone();
            let sub = create_effect_subscription(move |effects, dr_id| {
                let d = disp.clone();
                let id = dr_id.clone();
                tokio::task::spawn(async move {
                    let _ = d.execute_effects(effects, &id).await;
                });
            });
            sub(SystemEvent::ProposalAccepted {
                proposal_id: freeze_proposal_id.to_string(),
                domain_id: domain_id.to_string(),
                payload: serde_json::to_value(&freeze_payload).unwrap(),
                decided_at: 1_700_000_020,
                canonical_payload_hash: Some("sha256:freeze-pre-hash".to_string()),
                governance_decision_hash: None,
            });
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;

            let after_freeze = coop_store.get_member(domain_id, &member_did).unwrap();
            assert_eq!(after_freeze.status, icn_coop::MemberStatus::Suspended);
        }

        // Step 2: unfreeze
        let unfreeze_payload = ProposalPayload::UnfreezeMember {
            member: member_did.clone(),
            reason: "resolved".to_string(),
        };
        let payload_value = serde_json::to_value(&unfreeze_payload)?;

        let dispatcher_for_sub = dispatcher.clone();
        let subscription = create_effect_subscription(move |effects, decision_receipt_id| {
            let disp = dispatcher_for_sub.clone();
            let dr_id = decision_receipt_id.clone();
            tokio::task::spawn(async move {
                match disp.execute_effects(effects, &dr_id).await {
                    Ok(results) => info!(
                        decision_receipt_id = %dr_id,
                        result_count = results.len(),
                        "EffectDispatcher processed unfreeze effects"
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
            proposal_id: unfreeze_proposal_id.to_string(),
            domain_id: domain_id.to_string(),
            payload: payload_value,
            decided_at: 1_700_000_030,
            canonical_payload_hash: Some(unfreeze_canonical_hash.clone()),
            governance_decision_hash: None,
        });
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let stored = coop_store
            .get_member(domain_id, &member_did)
            .expect("Member should exist after unfreeze");

        // Status back to Active
        assert_eq!(
            stored.status,
            icn_coop::MemberStatus::Active,
            "member.status must be Active after unfreeze"
        );
        assert_eq!(
            stored
                .metadata
                .get("gov_decision_receipt_id")
                .map(|s| s.as_str()),
            Some(expected_unfreeze_receipt_id.as_str()),
            "gov_decision_receipt_id must reflect unfreeze receipt"
        );
        assert_eq!(
            stored.metadata.get("gov_decision_hash").map(|s| s.as_str()),
            Some(unfreeze_canonical_hash.as_str()),
            "gov_decision_hash must equal canonical_payload_hash"
        );
        assert_eq!(
            stored.metadata.get("gov_operation").map(|s| s.as_str()),
            Some("unfreeze"),
            "gov_operation must be 'unfreeze'"
        );

        info!("✅ Phase A: member unfrozen, provenance in metadata");
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
        receipt_id_after, expected_unfreeze_receipt_id,
        "receipt_id must match unfreeze receipt"
    );
    assert_eq!(
        hash_after, unfreeze_canonical_hash,
        "decision_hash must match unfreeze canonical"
    );

    info!("✅ Phase B: cold-cache restart — unfreeze provenance recovered from sled metadata");

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3 — Freeze legacy path (no canonical_hash): blake3 fallback
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn freeze_without_canonical_hash_falls_back_to_receipt_hash() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    let tmp = TempDir::new()?;
    let store_path = tmp.path().join("coop-store-freeze-legacy");

    let member_kp = KeyPair::generate()?;
    let member_did = member_kp.did().clone();

    let domain_id = "test-coop-freeze-legacy";
    let proposal_id = "freeze-legacy-001";
    let expected_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");

    let coop_store = open_coop_store(&store_path);
    seed_active_member(&coop_store, domain_id, &member_did);

    let membership_service = Arc::new(MembershipServiceImpl::new(coop_store.clone()));
    let kernel_executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
        .with_membership_service(membership_service.clone());
    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));

    let payload = ProposalPayload::FreezeMember {
        member: member_did.clone(),
        reason: "legacy freeze".to_string(),
        duration_seconds: None,
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
        decided_at: 1_700_000_040,
        canonical_payload_hash: None, // legacy: no canonical hash
        governance_decision_hash: None,
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let stored = coop_store
        .get_member(domain_id, &member_did)
        .expect("Member should exist after legacy freeze");

    assert_eq!(
        stored.status,
        icn_coop::MemberStatus::Suspended,
        "member must be Suspended even on legacy path"
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
        "✅ Legacy path: freeze record written with blake3 fallback provenance"
    );

    Ok(())
}
