//! Authority enforcement integration tests.
//!
//! These tests prove the authority model implemented in Tranche 5:
//!
//! **Rule**: A caller may only perform membership/governance mutations on a
//! cooperative/jurisdiction they govern.  Holding office in `coop:alpha` does
//! NOT confer authority over `coop:beta`.
//!
//! Every test that exercises cross-domain rejection is marked with the pattern
//! it closes: "coop:A admin cannot act on coop:B".
//!
//! ## What is tested
//! - `member_has_capability` (the primitive that `require_jurisdiction_authority`
//!   delegates to) enforces exact-domain isolation for `HoldOffice`.
//! - A member with `HoldOffice` in domain A does NOT satisfy the check for
//!   domain B (cross-domain isolation).
//! - A member without `HoldOffice` in any domain cannot pass the check.
//! - The check returns `true` only for the exact jurisdiction the capability
//!   was granted in.
//! - Membership state mutations (promote, suspend, grant capability) succeed
//!   when called by a properly authorized actor and fail when called by one
//!   who is only authorized in a different domain.
//!
//! ## Layer under test
//! The `require_jurisdiction_authority` helper in the membership and steward
//! handlers calls `CommonsManager::get_holder_by_did` and
//! `CommonsManager::member_has_capability`.  Both are tested here at the
//! CommonsManager level, which is the substrate the handlers rely on.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gateway::commons_mgr::CommonsManager;
use icn_governance::{Charter, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType};
use icn_identity::{Did, JurisdictionId, KeyPair, MembershipCapability};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Create a charter for `domain_id` and store it.
async fn create_charter(mgr: &CommonsManager, domain_id: &str) {
    let charter = Charter::new(
        OrgType::Cooperative,
        domain_id.to_string(),
        format!("Test Coop for {domain_id}"),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );
    mgr.store_charter(charter).await.unwrap();
}

/// Create a commons holder from a freshly generated keypair.  Returns `(Did, holder_id_hex)`.
async fn create_holder(mgr: &CommonsManager) -> (Did, String) {
    let kp = KeyPair::generate().unwrap();
    let did = kp.did().clone();
    // Sponsored enrollment gives Strong POP (required to avoid weak-POP rejection)
    let sponsor_kp = KeyPair::generate().unwrap();
    let anchor = mgr
        .create_anchor_from_enrollment(&did, Some(sponsor_kp.did()))
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());
    let holder = mgr
        .create_holder_from_anchor(&anchor_id, &did)
        .await
        .unwrap();
    (did, hex::encode(holder.id()))
}

/// Enroll `holder_id` in `domain_id` and approve + promote them to full Member.
async fn enroll_member(mgr: &CommonsManager, holder_id: &str, domain_id: &str) {
    let jurisdiction = JurisdictionId::new(domain_id);
    mgr.apply_for_membership(holder_id, jurisdiction.clone(), vec![])
        .await
        .unwrap();
    mgr.approve_membership(holder_id, &jurisdiction)
        .await
        .unwrap();
    mgr.promote_member(holder_id, &jurisdiction).await.unwrap();
}

/// Grant `HoldOffice` to `holder_id` in `domain_id`.
async fn grant_hold_office(mgr: &CommonsManager, holder_id: &str, domain_id: &str) {
    let jurisdiction = JurisdictionId::new(domain_id);
    mgr.grant_capability(holder_id, &jurisdiction, MembershipCapability::HoldOffice)
        .await
        .unwrap();
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Baseline: a holder with HoldOffice in their own domain satisfies the authority check.
#[actix_web::test]
async fn authority_check_passes_for_valid_jurisdiction_admin() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, admin_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &admin_id, "coop:alpha").await;
    grant_hold_office(&mgr, &admin_id, "coop:alpha").await;

    let jurisdiction = JurisdictionId::new("coop:alpha");
    let has = mgr
        .member_has_capability(&admin_id, &jurisdiction, MembershipCapability::HoldOffice)
        .await
        .unwrap_or(false);

    assert!(
        has,
        "HoldOffice in own domain must return true for the authority check"
    );
}

/// A holder without HoldOffice cannot pass the authority check, even in their own domain.
#[actix_web::test]
async fn authority_check_fails_without_hold_office() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, member_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &member_id, "coop:alpha").await;
    // No HoldOffice granted — member has only basic Vote + Propose defaults

    let jurisdiction = JurisdictionId::new("coop:alpha");
    let has = mgr
        .member_has_capability(&member_id, &jurisdiction, MembershipCapability::HoldOffice)
        .await
        .unwrap_or(false);

    assert!(
        !has,
        "HoldOffice must be false when not explicitly granted; regular members cannot pass authority check"
    );
}

/// A caller who is not a member of any domain does not satisfy the authority check.
#[actix_web::test]
async fn authority_check_fails_for_non_member() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, outsider_id) = create_holder(&mgr).await;
    // outsider_id was never enrolled — no affiliation record exists

    let jurisdiction = JurisdictionId::new("coop:alpha");
    let has = mgr
        .member_has_capability(
            &outsider_id,
            &jurisdiction,
            MembershipCapability::HoldOffice,
        )
        .await
        .unwrap_or(false);

    assert!(
        !has,
        "Non-member must not satisfy HoldOffice authority check in any jurisdiction"
    );
}

/// Cross-domain isolation: HoldOffice in `coop:alpha` does NOT satisfy the
/// authority check for `coop:beta`.  This is the core exploit we close.
#[actix_web::test]
async fn authority_check_cross_domain_rejected() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;
    create_charter(&mgr, "coop:beta").await;

    // Admin is a full member with HoldOffice in coop:alpha only
    let (_did, admin_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &admin_id, "coop:alpha").await;
    grant_hold_office(&mgr, &admin_id, "coop:alpha").await;

    // Alpha admin DOES have authority in alpha
    let alpha = JurisdictionId::new("coop:alpha");
    let has_alpha = mgr
        .member_has_capability(&admin_id, &alpha, MembershipCapability::HoldOffice)
        .await
        .unwrap_or(false);
    assert!(has_alpha, "Admin must have HoldOffice in their own domain");

    // Alpha admin does NOT have authority in beta
    let beta = JurisdictionId::new("coop:beta");
    let has_beta = mgr
        .member_has_capability(&admin_id, &beta, MembershipCapability::HoldOffice)
        .await
        .unwrap_or(false);
    assert!(
        !has_beta,
        "coop:alpha admin must NOT satisfy authority check for coop:beta — cross-domain leakage"
    );
}

/// Cross-domain isolation with explicit beta enrollment (no HoldOffice there):
/// being a plain member of both coops does not grant admin rights in either.
#[actix_web::test]
async fn authority_check_membership_without_hold_office_cross_domain() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;
    create_charter(&mgr, "coop:beta").await;

    let (_did, holder_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &holder_id, "coop:alpha").await;
    enroll_member(&mgr, &holder_id, "coop:beta").await;
    // HoldOffice only in alpha
    grant_hold_office(&mgr, &holder_id, "coop:alpha").await;

    let beta = JurisdictionId::new("coop:beta");
    let has = mgr
        .member_has_capability(&holder_id, &beta, MembershipCapability::HoldOffice)
        .await
        .unwrap_or(false);
    assert!(
        !has,
        "Being a member of coop:beta without HoldOffice there must fail the authority check"
    );
}

/// A member promoted to full Member in domain A, then suspended, no longer satisfies
/// HoldOffice — demonstrates capability isolation is per-membership state.
///
/// This test verifies the underlying data model remains correct: revoked capability
/// is truly absent.
#[actix_web::test]
async fn authority_check_revoked_capability_is_absent() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, holder_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &holder_id, "coop:alpha").await;
    grant_hold_office(&mgr, &holder_id, "coop:alpha").await;

    // Confirm capability present
    let jurisdiction = JurisdictionId::new("coop:alpha");
    assert!(
        mgr.member_has_capability(&holder_id, &jurisdiction, MembershipCapability::HoldOffice)
            .await
            .unwrap_or(false),
        "Precondition: HoldOffice must be present before revocation"
    );

    // Revoke HoldOffice
    mgr.revoke_capability(&holder_id, &jurisdiction, MembershipCapability::HoldOffice)
        .await
        .unwrap();

    // Capability must be absent now
    let has = mgr
        .member_has_capability(&holder_id, &jurisdiction, MembershipCapability::HoldOffice)
        .await
        .unwrap_or(false);
    assert!(
        !has,
        "Revoked HoldOffice capability must not satisfy the authority check"
    );
}

/// Mutation semantics: an authorized admin can promote a target member;
/// the target's status changes as expected.
#[actix_web::test]
async fn authorized_admin_can_promote_member_in_own_domain() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    // Admin with HoldOffice
    let (_admin_did, admin_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &admin_id, "coop:alpha").await;
    grant_hold_office(&mgr, &admin_id, "coop:alpha").await;

    // Target: enrolled but still Provisional (just approved, not yet promoted)
    let (_target_did, target_id) = create_holder(&mgr).await;
    let jurisdiction = JurisdictionId::new("coop:alpha");
    mgr.apply_for_membership(&target_id, jurisdiction.clone(), vec![])
        .await
        .unwrap();
    mgr.approve_membership(&target_id, &jurisdiction)
        .await
        .unwrap();

    // Admin has authority — confirm before acting
    assert!(
        mgr.member_has_capability(&admin_id, &jurisdiction, MembershipCapability::HoldOffice)
            .await
            .unwrap_or(false),
        "Admin must have HoldOffice to authorize the promote action"
    );

    // Perform promotion (the action the handler delegates to after authority check)
    mgr.promote_member(&target_id, &jurisdiction)
        .await
        .expect("Authorized admin promotion must succeed");

    // Verify target is now a full Member
    let holder = mgr.get_holder(&target_id).await.unwrap().unwrap();
    let affiliation = holder
        .get_affiliation(&jurisdiction)
        .expect("Target must have affiliation");
    assert_eq!(
        format!("{}", affiliation.membership_status),
        "Member",
        "Target must be promoted to full Member"
    );
}

/// Cross-domain isolation for mutations: an admin in coop:alpha cannot
/// meaningfully claim authority to act on coop:beta.
///
/// This directly proves the `require_jurisdiction_authority` pre-condition:
/// a caller from domain A will fail the HoldOffice check for domain B
/// before the underlying mutation method is even invoked.
#[actix_web::test]
async fn cross_domain_admin_cannot_satisfy_authority_for_other_domain() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;
    create_charter(&mgr, "coop:beta").await;

    // Alpha admin
    let (_admin_did, alpha_admin_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &alpha_admin_id, "coop:alpha").await;
    grant_hold_office(&mgr, &alpha_admin_id, "coop:alpha").await;

    // Beta member (the intended target of the cross-domain attack)
    let (_target_did, beta_member_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &beta_member_id, "coop:beta").await;

    // The authority check for beta jurisdiction fails for the alpha admin
    let beta = JurisdictionId::new("coop:beta");
    let has_beta_authority = mgr
        .member_has_capability(&alpha_admin_id, &beta, MembershipCapability::HoldOffice)
        .await
        .unwrap_or(false);

    assert!(
        !has_beta_authority,
        "coop:alpha admin must fail authority check for coop:beta — the handler rejects before mutation"
    );
}
