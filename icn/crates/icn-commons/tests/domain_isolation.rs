//! Multi-domain isolation tests for CommonsHandle.
//!
//! These tests verify that domain-scoped list operations on a shared CommonsHandle
//! cannot cross-contaminate data between distinct domains — the core invariant required
//! for a multi-cooperative ICN node.
//!
//! Invariants proven:
//! 1. `list_amendments_by_domain` returns only records whose scope is
//!    `Jurisdiction { domain_id }` with an exact-string match.  A record
//!    belonging to "coop:alpha" is NOT visible to a query for "coop:beta" or
//!    "coop:alpha-extended".
//! 2. `list_appeals_by_domain` has the same exact-match isolation guarantee.
//! 3. Network-scoped amendments are excluded from any domain query.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use ed25519_dalek::SigningKey;
use icn_commons::CommonsHandle;
use icn_governance::{
    Amendment, AmendmentScope, AmendmentType, Appeal, AppealGrounds, AppealRemedy, AppealScope,
    AppealType,
};
use icn_identity::Did;

fn test_did(seed: u8) -> Did {
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    Did::from_public_key(&signing_key.verifying_key())
}

async fn store_jurisdiction_amendment(
    handle: &CommonsHandle,
    proposer: &Did,
    domain_id: &str,
    title: &str,
) {
    let amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: domain_id.to_string(),
        },
        title.to_string(),
        format!("Test amendment for {domain_id}"),
        proposer.clone(),
    );
    handle
        .store_amendment(amendment)
        .await
        .expect("store amendment");
}

async fn store_network_amendment(handle: &CommonsHandle, proposer: &Did) {
    let amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Network,
        "Network constitutional".to_string(),
        "Network-wide amendment".to_string(),
        proposer.clone(),
    );
    handle
        .store_amendment(amendment)
        .await
        .expect("store network amendment");
}

async fn store_jurisdiction_appeal(handle: &CommonsHandle, appellant: &Did, domain_id: &str) {
    let appeal = Appeal::new(
        AppealType::MembershipDenial {
            jurisdiction_id: domain_id.to_string(),
            application_id: None,
        },
        AppealScope::Jurisdiction {
            domain_id: domain_id.to_string(),
        },
        appellant.clone(),
        vec![AppealGrounds::ProceduralError {
            description: "test".to_string(),
        }],
        format!("Appeal for {domain_id}"),
        AppealRemedy::Reinstate,
    );
    handle.store_appeal(appeal).await.expect("store appeal");
}

// ─── Amendment Tests ──────────────────────────────────────────────────────────

/// Two domains on the same handle must not see each other's amendments.
#[tokio::test]
async fn list_amendments_by_domain_isolates_domains() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(10);

    store_jurisdiction_amendment(&handle, &proposer, "coop:alpha", "Alpha change").await;
    store_jurisdiction_amendment(&handle, &proposer, "coop:beta", "Beta change").await;

    let alpha = handle
        .list_amendments_by_domain("coop:alpha")
        .await
        .expect("list alpha");
    assert_eq!(alpha.len(), 1, "coop:alpha sees only its own amendment");
    assert_eq!(alpha[0].title, "Alpha change");

    let beta = handle
        .list_amendments_by_domain("coop:beta")
        .await
        .expect("list beta");
    assert_eq!(beta.len(), 1, "coop:beta sees only its own amendment");
    assert_eq!(beta[0].title, "Beta change");
}

/// A domain-ID prefix must NOT match a longer ID with the same prefix.
/// Substring-based filtering ("coop:alpha") would also return "coop:alpha-test" — we must not.
#[tokio::test]
async fn list_amendments_by_domain_no_prefix_bleed() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(11);

    store_jurisdiction_amendment(&handle, &proposer, "coop:alpha", "Exact match").await;
    store_jurisdiction_amendment(
        &handle,
        &proposer,
        "coop:alpha-extended",
        "Should not bleed",
    )
    .await;

    let results = handle
        .list_amendments_by_domain("coop:alpha")
        .await
        .expect("list");
    assert_eq!(results.len(), 1, "prefix must not match longer IDs");
    assert_eq!(results[0].title, "Exact match");
}

/// Network-scoped amendments must be invisible to domain queries.
#[tokio::test]
async fn list_amendments_by_domain_excludes_network_scope() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(12);

    store_network_amendment(&handle, &proposer).await;
    store_jurisdiction_amendment(&handle, &proposer, "coop:alpha", "Domain-only").await;

    let results = handle
        .list_amendments_by_domain("coop:alpha")
        .await
        .expect("list");
    assert_eq!(
        results.len(),
        1,
        "network-scoped amendments must not appear in domain queries"
    );
    assert_eq!(results[0].title, "Domain-only");
}

// ─── Appeal Tests ─────────────────────────────────────────────────────────────

/// Two domains on the same handle must not see each other's appeals.
#[tokio::test]
async fn list_appeals_by_domain_isolates_domains() {
    let handle = CommonsHandle::new_in_memory();
    let alpha_appellant = test_did(20);
    let beta_appellant = test_did(21);

    store_jurisdiction_appeal(&handle, &alpha_appellant, "coop:alpha").await;
    store_jurisdiction_appeal(&handle, &beta_appellant, "coop:beta").await;

    let alpha = handle
        .list_appeals_by_domain("coop:alpha")
        .await
        .expect("list alpha");
    assert_eq!(alpha.len(), 1, "coop:alpha sees only its own appeal");
    assert_eq!(alpha[0].appellant, alpha_appellant);

    let beta = handle
        .list_appeals_by_domain("coop:beta")
        .await
        .expect("list beta");
    assert_eq!(beta.len(), 1, "coop:beta sees only its own appeal");
    assert_eq!(beta[0].appellant, beta_appellant);
}

/// Prefix bleed check for appeals — same guarantee as amendments.
#[tokio::test]
async fn list_appeals_by_domain_no_prefix_bleed() {
    let handle = CommonsHandle::new_in_memory();
    let appellant = test_did(22);

    store_jurisdiction_appeal(&handle, &appellant, "coop:gamma").await;
    store_jurisdiction_appeal(&handle, &appellant, "coop:gamma-extended").await;

    let results = handle
        .list_appeals_by_domain("coop:gamma")
        .await
        .expect("list");
    assert_eq!(results.len(), 1, "prefix must not match longer domain IDs");
    assert_eq!(
        results[0].scope,
        AppealScope::Jurisdiction {
            domain_id: "coop:gamma".to_string()
        }
    );
}

/// An unknown domain returns an empty list, not an error.
#[tokio::test]
async fn list_by_unknown_domain_returns_empty() {
    let handle = CommonsHandle::new_in_memory();
    let proposer = test_did(30);

    store_jurisdiction_amendment(&handle, &proposer, "coop:known", "Known").await;

    let results = handle
        .list_amendments_by_domain("coop:unknown")
        .await
        .expect("list");
    assert!(results.is_empty(), "unknown domain returns empty list");
}
