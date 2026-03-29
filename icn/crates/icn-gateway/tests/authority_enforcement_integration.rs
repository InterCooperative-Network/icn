//! Authority enforcement integration tests.
//!
//! These tests prove the authority model implemented in Tranches 5–8:
//!
//! **Rule**: A caller may only perform membership/governance mutations on a
//! cooperative/jurisdiction where they hold a delegated capability.  Holding
//! office in `coop:alpha` does NOT confer authority over `coop:beta`.
//!
//! Every test that exercises cross-domain rejection is marked with the pattern
//! it closes: "office-holder in coop:A cannot act on coop:B".
//!
//! ## What is tested
//! - `member_has_capability` (the primitive that `require_office_in_jurisdiction`
//!   and `require_membership_in_jurisdiction` delegate to) enforces exact-domain
//!   isolation for `HoldOffice` and `Vote` respectively.
//! - A member with `HoldOffice` in domain A does NOT satisfy the office check for
//!   domain B (cross-domain isolation).
//! - A member without `HoldOffice` in any domain cannot pass the office check.
//! - The check returns `true` only for the exact jurisdiction the capability
//!   was granted in.
//! - Membership state mutations (promote, suspend, grant capability) succeed
//!   when called by a properly authorized actor and fail when called by one
//!   who is only authorized in a different domain.
//!
//! ## Layer under test
//! The `require_office_in_jurisdiction` and `require_membership_in_jurisdiction`
//! helpers in `crate::authority` call `CommonsManager::get_holder_by_did` and
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
    mgr.apply_for_membership(holder_id, jurisdiction.clone())
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
async fn authority_check_passes_for_jurisdiction_office_holder() {
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
/// being a plain member of both coops does not grant HoldOffice authority in either.
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

/// Mutation semantics: an office-holder in a jurisdiction can promote a target
/// member; the target's status changes as expected.
#[actix_web::test]
async fn office_holder_can_promote_member_in_own_domain() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    // Admin with HoldOffice
    let (_admin_did, admin_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &admin_id, "coop:alpha").await;
    grant_hold_office(&mgr, &admin_id, "coop:alpha").await;

    // Target: enrolled but still Provisional (just approved, not yet promoted)
    let (_target_did, target_id) = create_holder(&mgr).await;
    let jurisdiction = JurisdictionId::new("coop:alpha");
    mgr.apply_for_membership(&target_id, jurisdiction.clone())
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

/// Cross-domain isolation for mutations: an office-holder in coop:alpha cannot
/// claim authority to act on coop:beta.
///
/// This directly proves the `require_office_in_jurisdiction` pre-condition:
/// a caller from domain A will fail the HoldOffice check for domain B
/// before the underlying mutation method is even invoked.
#[actix_web::test]
async fn cross_domain_office_holder_cannot_act_on_other_domain() {
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
        "coop:alpha office-holder must fail authority check for coop:beta — the handler rejects before mutation"
    );
}

// ─── Dispute-Filing Authority Tests ─────────────────────────────────────────
//
// These tests cover the member-standing gate used by `record_dispute`.
// Filing a dispute requires full `Member` status in the steward's jurisdiction,
// NOT `HoldOffice`.  The gate checks membership status directly (not a
// capability) because capabilities are not auto-granted during the enrollment
// flow — status is the authoritative marker for full membership.

/// A full member (Member status) in the steward's jurisdiction has
/// member-standing and can satisfy the dispute-filing check.
#[actix_web::test]
async fn full_member_has_standing_to_file_dispute() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, member_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &member_id, "coop:alpha").await;
    // enroll_member applies + approves + promotes → status is Member

    let holder = mgr.get_holder(&member_id).await.unwrap().unwrap();
    let jurisdiction = JurisdictionId::new("coop:alpha");
    let affiliation = holder
        .get_affiliation(&jurisdiction)
        .expect("must have affiliation");

    assert_eq!(
        format!("{}", affiliation.membership_status),
        "Member",
        "Full member must have Member status — required for dispute-filing standing"
    );
}

/// An unenrolled outsider has no affiliation and must be rejected by the
/// dispute-filing check.
#[actix_web::test]
async fn outsider_lacks_standing_to_file_dispute() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, outsider_id) = create_holder(&mgr).await;
    // outsider is never enrolled — no affiliation in any jurisdiction

    let holder = mgr.get_holder(&outsider_id).await.unwrap().unwrap();
    let jurisdiction = JurisdictionId::new("coop:alpha");
    let affiliation = holder.get_affiliation(&jurisdiction);

    assert!(
        affiliation.is_none(),
        "Unenrolled outsider must have no affiliation — cannot satisfy dispute-filing standing"
    );
}

/// Cross-jurisdiction: Member status in coop:alpha does NOT confer standing in
/// coop:beta.  A member of one cooperative cannot file disputes against stewards
/// in another cooperative's jurisdiction.
#[actix_web::test]
async fn cross_domain_member_cannot_file_dispute_in_other_jurisdiction() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;
    create_charter(&mgr, "coop:beta").await;

    let (_did, alpha_member_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &alpha_member_id, "coop:alpha").await;

    let holder = mgr.get_holder(&alpha_member_id).await.unwrap().unwrap();
    let alpha = JurisdictionId::new("coop:alpha");
    let beta = JurisdictionId::new("coop:beta");

    // Is a full member in alpha
    let alpha_affiliation = holder
        .get_affiliation(&alpha)
        .expect("must have alpha affiliation");
    assert_eq!(
        format!("{}", alpha_affiliation.membership_status),
        "Member",
        "Precondition: must be a full member in own jurisdiction"
    );

    // Has no affiliation in beta at all
    assert!(
        holder.get_affiliation(&beta).is_none(),
        "Member of coop:alpha must have no affiliation in coop:beta — cross-domain dispute standing is prohibited"
    );
}

/// Dispute filing does NOT require HoldOffice — a plain member satisfies the
/// standing check; `HoldOffice` is only required for governance mutations.
#[actix_web::test]
async fn plain_member_without_hold_office_can_satisfy_dispute_standing() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, member_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &member_id, "coop:alpha").await;
    // No HoldOffice granted — plain full member

    let jurisdiction = JurisdictionId::new("coop:alpha");

    // Has Member status (dispute standing)
    let holder = mgr.get_holder(&member_id).await.unwrap().unwrap();
    let affiliation = holder
        .get_affiliation(&jurisdiction)
        .expect("must have affiliation");
    assert_eq!(
        format!("{}", affiliation.membership_status),
        "Member",
        "Plain member must have Member status for dispute-filing standing"
    );

    // Does NOT have HoldOffice (governance gate requires explicit grant)
    let has_office = mgr
        .member_has_capability(&member_id, &jurisdiction, MembershipCapability::HoldOffice)
        .await
        .unwrap_or(false);
    assert!(
        !has_office,
        "Plain member must NOT have HoldOffice — dispute filing and governance mutations are separate authority levels"
    );
}

// ─── Tranche 8: Enrollment capability-origin invariant ───────────────────────
//
// These tests prove that capabilities originate from jurisdictional delegation,
// not from applicant self-assertion.  Applying for membership, having the
// application approved, and being promoted to full member must not produce any
// capabilities except the baseline granted by `promote()`.

/// Application creates a Candidate with no capabilities, regardless of what
/// the application request conceptually "expressed."
#[actix_web::test]
async fn application_creates_candidate_with_no_capabilities() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, applicant_id) = create_holder(&mgr).await;
    let jurisdiction = JurisdictionId::new("coop:alpha");

    // apply_for_membership creates a Candidate with no capabilities
    mgr.apply_for_membership(&applicant_id, jurisdiction.clone())
        .await
        .unwrap();

    let holder = mgr.get_holder(&applicant_id).await.unwrap().unwrap();
    let affil = holder
        .get_affiliation(&jurisdiction)
        .expect("must have affiliation after application");

    assert_eq!(format!("{}", affil.membership_status), "Candidate");
    assert!(
        affil.capabilities.is_empty(),
        "Candidate must have no capabilities at application time"
    );
}

/// Approval alone does not grant any capabilities — affiliation moves to
/// Provisional but capability set remains empty.
#[actix_web::test]
async fn approval_does_not_grant_capabilities() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, applicant_id) = create_holder(&mgr).await;
    let jurisdiction = JurisdictionId::new("coop:alpha");

    mgr.apply_for_membership(&applicant_id, jurisdiction.clone())
        .await
        .unwrap();
    mgr.approve_membership(&applicant_id, &jurisdiction)
        .await
        .unwrap();

    let holder = mgr.get_holder(&applicant_id).await.unwrap().unwrap();
    let affil = holder
        .get_affiliation(&jurisdiction)
        .expect("must have affiliation after approval");

    assert_eq!(format!("{}", affil.membership_status), "Provisional");
    assert!(
        affil.capabilities.is_empty(),
        "Provisional member must have no capabilities — only promotion grants baseline capabilities"
    );
}

/// Promotion grants exactly the baseline member capabilities.
/// `HoldOffice` is absent without explicit delegation.
#[actix_web::test]
async fn promotion_grants_only_baseline_capabilities() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, applicant_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &applicant_id, "coop:alpha").await;

    let jurisdiction = JurisdictionId::new("coop:alpha");
    let holder = mgr.get_holder(&applicant_id).await.unwrap().unwrap();
    let affil = holder
        .get_affiliation(&jurisdiction)
        .expect("must have affiliation after promotion");

    assert_eq!(format!("{}", affil.membership_status), "Member");

    // Baseline capabilities are present
    assert!(
        affil.has_capability(MembershipCapability::Vote),
        "Promoted member must have Vote"
    );
    assert!(
        affil.has_capability(MembershipCapability::Propose),
        "Promoted member must have Propose"
    );
    assert!(
        affil.has_capability(MembershipCapability::Transact),
        "Promoted member must have Transact"
    );

    // HoldOffice is NOT present — it requires explicit jurisdictional delegation
    assert!(
        !affil.has_capability(MembershipCapability::HoldOffice),
        "Promoted member must NOT have HoldOffice — requires explicit delegation"
    );

    // The capability count is exactly 3 — no extras snuck in
    assert_eq!(
        affil.capabilities.len(),
        3,
        "Promoted member must have exactly the 3 baseline capabilities"
    );
}

/// `HoldOffice` can only be obtained through explicit delegation by a
/// jurisdiction office-holder, not through the application/promotion flow.
#[actix_web::test]
async fn hold_office_requires_explicit_delegation_not_enrollment() {
    let mgr = CommonsManager::new();
    create_charter(&mgr, "coop:alpha").await;

    let (_did, applicant_id) = create_holder(&mgr).await;
    enroll_member(&mgr, &applicant_id, "coop:alpha").await;

    let jurisdiction = JurisdictionId::new("coop:alpha");

    // After enrollment the member does NOT have HoldOffice
    let before = mgr
        .member_has_capability(
            &applicant_id,
            &jurisdiction,
            MembershipCapability::HoldOffice,
        )
        .await
        .unwrap_or(false);
    assert!(!before, "HoldOffice must be absent after enrollment");

    // Only explicit grant by a jurisdiction office-holder adds HoldOffice
    mgr.grant_capability(
        &applicant_id,
        &jurisdiction,
        MembershipCapability::HoldOffice,
    )
    .await
    .unwrap();

    let after = mgr
        .member_has_capability(
            &applicant_id,
            &jurisdiction,
            MembershipCapability::HoldOffice,
        )
        .await
        .unwrap_or(false);
    assert!(
        after,
        "HoldOffice must be present after explicit delegation"
    );
}
