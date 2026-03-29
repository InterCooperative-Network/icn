//! Write-side scope integrity tests for CommonsInner.
//!
//! These tests verify the invariants enforced at the substrate layer when creating
//! governance records. Unlike the read-side isolation tests (domain_isolation.rs),
//! these tests prove that *invalid records can never be created* in the first place.
//!
//! Invariants proven:
//! 1. A domain may have at most one charter — storing a second charter for the
//!    same `domain_id` is rejected with an error.
//! 2. A `Jurisdiction`-scoped amendment must reference a domain whose charter
//!    already exists — references to unknown domains are rejected.
//! 3. A `Network`-scoped amendment requires no charter — network records are
//!    global and bypass domain-existence checks.
//! 4. An amendment whose `charter_id` points to a charter belonging to a
//!    different domain is rejected (cross-domain scope is forbidden).
//! 5. A `Jurisdiction`-scoped appeal must reference a domain whose charter
//!    already exists — references to unknown domains are rejected.
//! 6. A steward registered with a `jurisdiction` that names an unknown domain
//!    is rejected.
//! 7. A steward registered without a `jurisdiction` (None) is always accepted
//!    regardless of what charters exist.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use ed25519_dalek::SigningKey;
use icn_commons::CommonsHandle;
use icn_governance::{
    Amendment, AmendmentScope, AmendmentType, Appeal, AppealGrounds, AppealRemedy, AppealScope,
    AppealType, Charter, GovernanceConfig, OrgType,
};
use icn_identity::Did;

fn test_did(seed: u8) -> Did {
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    Did::from_public_key(&signing_key.verifying_key())
}

/// Create a holder with Strong POP level, suitable for steward registration.
///
/// Uses sponsored enrollment (the sponsor DID acts as voucher), which produces
/// a Strong POP level — the minimum required by `register_steward`.
async fn create_strong_holder(handle: &CommonsHandle, holder_did: &Did, sponsor_did: &Did) {
    let anchor = handle
        .create_anchor_from_enrollment(holder_did, Some(sponsor_did))
        .await
        .expect("create anchor");
    let anchor_id = anchor.id_hex();
    handle
        .get_or_create_holder(&anchor_id, holder_did, None)
        .await
        .expect("create holder");
}

/// Create and store a minimal charter for the given domain_id. Returns the
/// stored charter so callers can inspect its charter_id.
async fn create_charter(handle: &CommonsHandle, domain_id: &str) -> Charter {
    let charter = Charter::new(
        OrgType::Cooperative,
        domain_id.to_string(),
        format!("Test Coop for {domain_id}"),
        GovernanceConfig::cooperative_default(),
        Default::default(),
        Default::default(),
    );
    handle
        .store_charter(charter.clone())
        .await
        .expect("store charter");
    charter
}

// ─── Charter Uniqueness ───────────────────────────────────────────────────────

/// Storing a second charter with the same domain_id must be rejected.
#[tokio::test]
async fn duplicate_charter_domain_is_rejected() {
    let handle = CommonsHandle::new_in_memory();

    create_charter(&handle, "coop:unique").await;

    // Attempt to store another charter for the same domain
    let duplicate = Charter::new(
        OrgType::Cooperative,
        "coop:unique".to_string(),
        "Duplicate Coop".to_string(),
        GovernanceConfig::cooperative_default(),
        Default::default(),
        Default::default(),
    );
    let result = handle.store_charter(duplicate).await;
    assert!(
        result.is_err(),
        "second charter for same domain_id must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("coop:unique"),
        "error should mention the conflicting domain_id; got: {msg}"
    );
}

/// Storing charters for different domain_ids must succeed independently.
#[tokio::test]
async fn distinct_domain_charters_are_allowed() {
    let handle = CommonsHandle::new_in_memory();
    create_charter(&handle, "coop:alpha").await;
    create_charter(&handle, "coop:beta").await;
    // No panic / error means both were accepted
}

// ─── Amendment Scope Enforcement ─────────────────────────────────────────────

/// A jurisdiction-scoped amendment may only be stored after the domain's
/// charter exists.
#[tokio::test]
async fn amendment_with_unknown_domain_is_rejected() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(1);

    let amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:ghost".to_string(),
        },
        "Ghost amendment".to_string(),
        "Scope references a domain with no charter".to_string(),
        proposer,
    );
    let result = handle.store_amendment(amendment).await;
    assert!(
        result.is_err(),
        "amendment for unknown domain must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("coop:ghost"),
        "error should mention the unknown domain; got: {msg}"
    );
}

/// After a charter exists, a jurisdiction-scoped amendment for that domain
/// must be accepted.
#[tokio::test]
async fn amendment_with_known_domain_is_accepted() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(2);

    create_charter(&handle, "coop:valid").await;

    let amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:valid".to_string(),
        },
        "Valid amendment".to_string(),
        "Domain exists".to_string(),
        proposer,
    );
    handle
        .store_amendment(amendment)
        .await
        .expect("amendment for known domain must succeed");
}

/// A `Network`-scoped amendment requires no charter and must always be accepted.
#[tokio::test]
async fn network_scoped_amendment_needs_no_charter() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(3);

    let amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Network,
        "Network-wide change".to_string(),
        "Global scope, no domain required".to_string(),
        proposer,
    );
    handle
        .store_amendment(amendment)
        .await
        .expect("network-scoped amendment must be accepted with no charters present");
}

/// An amendment whose `charter_id` points to a charter belonging to a
/// different domain must be rejected (cross-domain scope is forbidden).
#[tokio::test]
async fn amendment_charter_id_domain_mismatch_is_rejected() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(4);

    // Create two distinct charters
    let alpha_charter = create_charter(&handle, "coop:alpha").await;
    create_charter(&handle, "coop:beta").await;

    // Build an amendment scoped to "coop:beta" but referencing alpha's charter_id
    let mut amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:beta".to_string(),
        },
        "Cross-domain mismatch".to_string(),
        "charter_id belongs to alpha but scope says beta".to_string(),
        proposer,
    );
    amendment.charter_id = Some(*alpha_charter.charter_id.as_bytes());

    let result = handle.store_amendment(amendment).await;
    assert!(
        result.is_err(),
        "amendment with cross-domain charter_id must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("coop:alpha") || msg.contains("coop:beta"),
        "error should mention the conflicting domains; got: {msg}"
    );
}

/// An amendment with a `charter_id` that matches the scope domain must be accepted.
#[tokio::test]
async fn amendment_charter_id_matching_domain_is_accepted() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(5);

    let charter = create_charter(&handle, "coop:consistent").await;

    let mut amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:consistent".to_string(),
        },
        "Consistent amendment".to_string(),
        "charter_id domain matches scope domain".to_string(),
        proposer,
    );
    amendment.charter_id = Some(*charter.charter_id.as_bytes());

    handle
        .store_amendment(amendment)
        .await
        .expect("amendment with matching charter_id must be accepted");
}

// ─── Appeal Scope Enforcement ─────────────────────────────────────────────────

/// A jurisdiction-scoped appeal must be rejected when no charter exists for
/// the referenced domain.
#[tokio::test]
async fn appeal_with_unknown_domain_is_rejected() {
    let handle = CommonsHandle::new_in_memory();
    let appellant = test_did(10);

    let appeal = Appeal::new(
        AppealType::MembershipDenial {
            jurisdiction_id: "coop:void".to_string(),
            application_id: None,
        },
        AppealScope::Jurisdiction {
            domain_id: "coop:void".to_string(),
        },
        appellant,
        vec![AppealGrounds::ProceduralError {
            description: "test".to_string(),
        }],
        "Appeal to the void".to_string(),
        AppealRemedy::Reinstate,
    );

    let result = handle.store_appeal(appeal).await;
    assert!(
        result.is_err(),
        "appeal for unknown domain must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("coop:void"),
        "error should mention the unknown domain; got: {msg}"
    );
}

/// After a charter exists, an appeal for that domain must be accepted.
#[tokio::test]
async fn appeal_with_known_domain_is_accepted() {
    let handle = CommonsHandle::new_in_memory();
    let appellant = test_did(11);

    create_charter(&handle, "coop:chartered").await;

    let appeal = Appeal::new(
        AppealType::MembershipDenial {
            jurisdiction_id: "coop:chartered".to_string(),
            application_id: None,
        },
        AppealScope::Jurisdiction {
            domain_id: "coop:chartered".to_string(),
        },
        appellant,
        vec![AppealGrounds::ProceduralError {
            description: "test".to_string(),
        }],
        "Valid appeal".to_string(),
        AppealRemedy::Reinstate,
    );

    handle
        .store_appeal(appeal)
        .await
        .expect("appeal for known domain must succeed");
}

// ─── Steward Jurisdiction Enforcement ────────────────────────────────────────

/// A steward with a `jurisdiction` naming an unknown domain must be rejected.
#[tokio::test]
async fn steward_with_unknown_jurisdiction_is_rejected() {
    let handle = CommonsHandle::new_in_memory();
    let holder = test_did(20);
    let sponsor = test_did(21); // acts as POP voucher
    let steward = test_did(22);

    create_strong_holder(&handle, &holder, &sponsor).await;

    let result = handle
        .register_steward(
            &holder,
            &steward,
            365,
            1000,
            "governance_approval_token".to_string(),
            Some("coop:nowhere".to_string()),
            vec![],
        )
        .await;

    assert!(
        result.is_err(),
        "steward with unknown jurisdiction must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("coop:nowhere"),
        "error should mention the unknown jurisdiction; got: {msg}"
    );
}

/// A steward registered without a jurisdiction (None) must always be accepted.
#[tokio::test]
async fn steward_without_jurisdiction_is_always_accepted() {
    let handle = CommonsHandle::new_in_memory();
    let holder = test_did(23);
    let sponsor = test_did(24);
    let steward = test_did(25);

    create_strong_holder(&handle, &holder, &sponsor).await;

    // No charters exist — should not matter for jurisdiction-less stewards
    handle
        .register_steward(
            &holder,
            &steward,
            365,
            1000,
            "governance_approval_token".to_string(),
            None,
            vec![],
        )
        .await
        .expect("steward without jurisdiction must always be accepted");
}

/// A steward with a jurisdiction that names a known domain must be accepted.
#[tokio::test]
async fn steward_with_known_jurisdiction_is_accepted() {
    let handle = CommonsHandle::new_in_memory();
    let holder = test_did(26);
    let sponsor = test_did(27);
    let steward = test_did(28);

    create_strong_holder(&handle, &holder, &sponsor).await;
    create_charter(&handle, "coop:governed").await;

    handle
        .register_steward(
            &holder,
            &steward,
            365,
            1000,
            "governance_approval_token".to_string(),
            Some("coop:governed".to_string()),
            vec![],
        )
        .await
        .expect("steward with known jurisdiction must be accepted");
}
