//! Boundary scope integrity tests for CommonsManager.
//!
//! These tests verify that the gateway facade layer enforces semantic
//! consistency from the API boundary to the substrate.  They complement
//! the substrate-level tests in icn-commons/tests/scope_enforcement.rs by
//! covering the gateway manager's own scope-safety guarantees.
//!
//! Invariants proven:
//! 1. `build_governance_dashboard` returns an error for an unknown charter_id
//!    rather than silently falling back to a cross-domain "list all" query.
//! 2. Dashboard data is scoped to the charter's domain — records from other
//!    domains are NOT included.
//! 3. `list_amendments_by_domain` on CommonsManager delegates to the
//!    domain-scoped substrate method (exact-match isolation).
//! 4. `list_appeals_by_domain` on CommonsManager has the same guarantee.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gateway::commons_mgr::CommonsManager;
use icn_governance::{
    Amendment, AmendmentScope, AmendmentType, Appeal, AppealGrounds, AppealRemedy, AppealScope,
    AppealType, Charter, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType,
};
use icn_identity::{Did, KeyPair};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_did() -> Did {
    KeyPair::generate().unwrap().did().clone()
}

/// Create and store a charter for `domain_id`, returning its hex charter_id.
async fn setup_charter(mgr: &CommonsManager, domain_id: &str) -> String {
    let charter = Charter::new(
        OrgType::Cooperative,
        domain_id.to_string(),
        format!("Test Coop for {domain_id}"),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );
    let id = charter.charter_id.to_hex();
    mgr.store_charter(charter).await.expect("store charter");
    id
}

/// Store a jurisdiction-scoped amendment for `domain_id`.
async fn store_amendment(mgr: &CommonsManager, domain_id: &str, title: &str) {
    let amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: domain_id.to_string(),
        },
        title.to_string(),
        format!("Test amendment body for {domain_id}"),
        make_did(),
    );
    mgr.store_amendment(amendment)
        .await
        .expect("store amendment");
}

/// Store a jurisdiction-scoped appeal for `domain_id`.
async fn store_appeal(mgr: &CommonsManager, domain_id: &str) {
    let appeal = Appeal::new(
        AppealType::MembershipDenial {
            jurisdiction_id: domain_id.to_string(),
            application_id: None,
        },
        AppealScope::Jurisdiction {
            domain_id: domain_id.to_string(),
        },
        make_did(),
        vec![AppealGrounds::ProceduralError {
            description: "boundary test".to_string(),
        }],
        format!("Appeal for {domain_id}"),
        AppealRemedy::Reinstate,
    );
    mgr.store_appeal(appeal).await.expect("store appeal");
}

// ─── Dashboard Boundary Tests ─────────────────────────────────────────────────

/// `build_governance_dashboard` must return an error when given a charter_id
/// that does not exist — it must NOT silently widen scope to all records.
#[actix_web::test]
async fn dashboard_unknown_charter_returns_error_not_all_records() {
    let mgr = CommonsManager::new();

    // Store an amendment so we can verify it doesn't appear in the fallback
    setup_charter(&mgr, "coop:real").await;
    store_amendment(&mgr, "coop:real", "Real amendment").await;

    let bogus_hex = "a".repeat(64); // 32 bytes of 0xaa — valid hex, no matching charter
    let result = mgr.build_governance_dashboard(&bogus_hex).await;

    assert!(
        result.is_err(),
        "build_governance_dashboard must error for unknown charter_id, not return all records"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("Charter not found") || msg.contains("not found"),
        "error message should indicate missing charter; got: {msg}"
    );
}

/// Dashboard data is scoped to the charter's domain.  Amendments belonging to
/// a different domain must not appear in the result.
#[actix_web::test]
async fn dashboard_scoped_to_charter_domain_only() {
    let mgr = CommonsManager::new();

    let alpha_id = setup_charter(&mgr, "coop:alpha").await;
    setup_charter(&mgr, "coop:beta").await;

    store_amendment(&mgr, "coop:alpha", "Alpha-only amendment").await;
    store_amendment(&mgr, "coop:beta", "Beta amendment — must not appear").await;

    let dashboard = mgr
        .build_governance_dashboard(&alpha_id)
        .await
        .expect("dashboard for known charter must succeed");

    // Total amendments count must be 1 (alpha only)
    let total = dashboard.amendments_breakdown.draft
        + dashboard.amendments_breakdown.submitted
        + dashboard.amendments_breakdown.voting
        + dashboard.amendments_breakdown.ratified
        + dashboard.amendments_breakdown.rejected
        + dashboard.amendments_breakdown.withdrawn;

    assert_eq!(
        total, 1,
        "dashboard must count only amendments scoped to coop:alpha; got {total}"
    );
}

/// Dashboard is consistent even when no amendments or appeals exist for the domain.
#[actix_web::test]
async fn dashboard_empty_domain_returns_zeroes() {
    let mgr = CommonsManager::new();

    let id = setup_charter(&mgr, "coop:empty").await;

    // Different domain has data — should not leak into coop:empty dashboard
    setup_charter(&mgr, "coop:noisy").await;
    store_amendment(&mgr, "coop:noisy", "Noisy amendment").await;
    store_appeal(&mgr, "coop:noisy").await;

    let dashboard = mgr
        .build_governance_dashboard(&id)
        .await
        .expect("dashboard for known charter must succeed");

    let total_amendments = dashboard.amendments_breakdown.draft
        + dashboard.amendments_breakdown.submitted
        + dashboard.amendments_breakdown.voting
        + dashboard.amendments_breakdown.ratified
        + dashboard.amendments_breakdown.rejected
        + dashboard.amendments_breakdown.withdrawn;

    let total_appeals = dashboard.appeals_breakdown.filed
        + dashboard.appeals_breakdown.under_review
        + dashboard.appeals_breakdown.hearing
        + dashboard.appeals_breakdown.resolved
        + dashboard.appeals_breakdown.dismissed
        + dashboard.appeals_breakdown.withdrawn;

    assert_eq!(
        total_amendments, 0,
        "empty domain dashboard must have zero amendments"
    );
    assert_eq!(
        total_appeals, 0,
        "empty domain dashboard must have zero appeals"
    );
}

// ─── List Delegation Tests ────────────────────────────────────────────────────

/// `CommonsManager::list_amendments_by_domain` must return only records for
/// the requested domain, matching the substrate's exact-match guarantee.
#[actix_web::test]
async fn list_amendments_by_domain_delegates_to_exact_match() {
    let mgr = CommonsManager::new();

    setup_charter(&mgr, "coop:delta").await;
    setup_charter(&mgr, "coop:gamma").await;

    store_amendment(&mgr, "coop:delta", "Delta amendment").await;
    store_amendment(&mgr, "coop:gamma", "Gamma amendment").await;

    let delta = mgr
        .list_amendments_by_domain("coop:delta")
        .await
        .expect("list_amendments_by_domain must succeed");

    assert_eq!(
        delta.len(),
        1,
        "must return exactly one amendment for coop:delta"
    );
    assert_eq!(delta[0].title, "Delta amendment");
}

/// `CommonsManager::list_appeals_by_domain` must return only records for
/// the requested domain, matching the substrate's exact-match guarantee.
#[actix_web::test]
async fn list_appeals_by_domain_delegates_to_exact_match() {
    let mgr = CommonsManager::new();

    setup_charter(&mgr, "coop:epsilon").await;
    setup_charter(&mgr, "coop:zeta").await;

    store_appeal(&mgr, "coop:epsilon").await;
    store_appeal(&mgr, "coop:zeta").await;

    let epsilon = mgr
        .list_appeals_by_domain("coop:epsilon")
        .await
        .expect("list_appeals_by_domain must succeed");

    assert_eq!(
        epsilon.len(),
        1,
        "must return exactly one appeal for coop:epsilon"
    );
    assert!(
        matches!(
            &epsilon[0].scope,
            AppealScope::Jurisdiction { domain_id } if domain_id == "coop:epsilon"
        ),
        "returned appeal must be scoped to coop:epsilon"
    );
}
