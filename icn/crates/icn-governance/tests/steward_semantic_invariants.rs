//! Semantic invariant tests for stewardship governance.
//!
//! These tests encode the architectural invariants from ADR-0014 and ADR-0015
//! so that future changes that re-introduce bond/slash or financial-punishment
//! semantics will break here immediately.
//!
//! **What these tests protect:**
//! - `StewardPenalty` variants carry only status/authority changes, never financial payloads
//! - `SdisProposal::AppointSteward` requires no financial collateral
//! - `SdisProposal::SanctionSteward` can be constructed with institutional-only data
//! - The governance domain model stays clean of DeFi contamination

#![allow(clippy::unwrap_used)]

use icn_governance::sdis::{JurisdictionTier, SdisProposal, StewardPenalty};
use icn_identity::{Did, KeyPair};

fn test_did() -> Did {
    KeyPair::generate().unwrap().did().clone()
}

// =============================================================================
// ADR-0014: StewardPenalty has no financial fields
// =============================================================================

/// Warning is a unit variant — no amount, no credits, no balance.
#[test]
fn test_warning_penalty_has_no_financial_payload() {
    let penalty = StewardPenalty::Warning;
    // The only way to prove no financial field exists is to construct it without one.
    // If a `bond_slash` or `amount` field were added, this match would need updating
    // and the ADR-0014 boundary would be violated.
    assert!(matches!(penalty, StewardPenalty::Warning));
}

/// Censure is a unit variant — a formal institutional rebuke with no financial mutation.
#[test]
fn test_censure_penalty_has_no_financial_payload() {
    let penalty = StewardPenalty::Censure;
    assert!(matches!(penalty, StewardPenalty::Censure));
}

/// Probation carries only duration + conditions — no `bond_at_risk`, no `collateral`.
#[test]
fn test_probation_carries_only_duration_and_conditions() {
    let penalty = StewardPenalty::Probation {
        duration: 86_400 * 30,
        conditions: vec!["monthly_report".to_string()],
    };
    match &penalty {
        StewardPenalty::Probation {
            duration,
            conditions,
        } => {
            assert!(*duration > 0);
            assert!(!conditions.is_empty());
            // If this match arm ever grows a financial field, add it here and add
            // a failing assertion so the ADR violation is caught at review time.
        }
        _ => panic!("Expected Probation"),
    }
}

/// Suspension carries only duration — no slash amount, no collateral forfeit.
#[test]
fn test_suspension_carries_only_duration() {
    let penalty = StewardPenalty::Suspension {
        duration: 86_400 * 7,
    };
    match &penalty {
        StewardPenalty::Suspension { duration } => {
            assert!(*duration > 0);
        }
        _ => panic!("Expected Suspension"),
    }
}

/// Removal is a unit variant — removal from office, not confiscation.
#[test]
fn test_removal_penalty_has_no_financial_payload() {
    let penalty = StewardPenalty::Removal;
    assert!(matches!(penalty, StewardPenalty::Removal));
}

/// TierDemotion carries only the new tier — no financial forfeiture.
#[test]
fn test_tier_demotion_carries_only_new_tier() {
    let penalty = StewardPenalty::TierDemotion {
        new_tier: JurisdictionTier::Tier1,
    };
    match &penalty {
        StewardPenalty::TierDemotion { new_tier } => {
            assert!(matches!(new_tier, JurisdictionTier::Tier1));
        }
        _ => panic!("Expected TierDemotion"),
    }
}

// =============================================================================
// ADR-0014: AppointSteward requires no financial collateral
// =============================================================================

/// AppointSteward can be constructed without any bond or collateral field.
///
/// If `bond_amount` were re-added to this proposal type, this test would require
/// that field and the reviewer would see the ADR-0014 violation immediately.
#[test]
fn test_appoint_steward_requires_no_financial_collateral() {
    let candidate = test_did();
    let sponsors = vec![test_did(), test_did()];

    let proposal = SdisProposal::AppointSteward {
        candidate: candidate.clone(),
        region: "northeast".to_string(),
        term_length: 365 * 86_400,
        sponsors: sponsors.clone(),
    };

    match &proposal {
        SdisProposal::AppointSteward {
            candidate: c,
            region,
            term_length,
            sponsors: s,
        } => {
            assert_eq!(c, &candidate);
            assert_eq!(region, "northeast");
            assert!(*term_length > 0);
            assert_eq!(s.len(), 2);
            // If bond_amount were here, it would appear in this destructure.
            // The absence of any financial field in this match arm is the assertion.
        }
        _ => panic!("Expected AppointSteward"),
    }
}

// =============================================================================
// ADR-0014: SanctionSteward can be constructed with institutional-only data
// =============================================================================

/// SanctionSteward with a Censure penalty carries no financial payload.
#[test]
fn test_sanction_steward_censure_is_institutional_only() {
    let steward = test_did();
    let proposal = SdisProposal::SanctionSteward {
        steward: steward.clone(),
        penalty: StewardPenalty::Censure,
        reason: "Missed three consecutive governance meetings".to_string(),
        evidence: vec![],
    };

    match &proposal {
        SdisProposal::SanctionSteward {
            steward: s,
            penalty,
            reason,
            evidence,
        } => {
            assert_eq!(s, &steward);
            assert!(matches!(penalty, StewardPenalty::Censure));
            assert!(!reason.is_empty());
            assert!(evidence.is_empty());
        }
        _ => panic!("Expected SanctionSteward"),
    }
}

/// SanctionSteward with a Warning penalty carries no financial payload.
#[test]
fn test_sanction_steward_warning_is_institutional_only() {
    let steward = test_did();
    let proposal = SdisProposal::SanctionSteward {
        steward: steward.clone(),
        penalty: StewardPenalty::Warning,
        reason: "First late response to attestation request".to_string(),
        evidence: vec![],
    };
    assert!(matches!(
        proposal,
        SdisProposal::SanctionSteward {
            penalty: StewardPenalty::Warning,
            ..
        }
    ));
}

// =============================================================================
// ADR-0015: All StewardPenalty variants are exhaustive and institutionally scoped
// =============================================================================

/// Exhaustive match on ALL StewardPenalty variants.
///
/// This test serves as a compile-time guardrail: if a new variant is added
/// without being reviewed against ADR-0014/ADR-0015, this test will fail to
/// compile (non-exhaustive match), forcing the author to explicitly handle it
/// and consider whether it violates the financial-punishment prohibition.
#[test]
fn test_all_steward_penalty_variants_are_institutional() {
    let penalties = [
        StewardPenalty::Warning,
        StewardPenalty::Censure,
        StewardPenalty::Suspension { duration: 1 },
        StewardPenalty::TierDemotion {
            new_tier: JurisdictionTier::Tier2,
        },
        StewardPenalty::Removal,
        StewardPenalty::Probation {
            duration: 1,
            conditions: vec![],
        },
    ];

    for penalty in &penalties {
        // Each penalty must be classifiable as either:
        // - record-only (Warning, Censure, Probation)
        // - status-change (Suspension, TierDemotion, Removal)
        //
        // None are financial.
        let is_institutional = match penalty {
            StewardPenalty::Warning => true,
            StewardPenalty::Censure => true,
            StewardPenalty::Suspension { .. } => true,
            StewardPenalty::TierDemotion { .. } => true,
            StewardPenalty::Removal => true,
            StewardPenalty::Probation { .. } => true,
            // If you add a variant here that has financial fields (e.g.,
            // `FinancialPenalty { amount: i64 }`), set this to `false` and
            // this test will fail, prompting review against ADR-0014.
        };
        assert!(
            is_institutional,
            "StewardPenalty variant {:?} violates ADR-0014: penalties must be \
             institutional status/authority changes, not financial punishments",
            penalty
        );
    }
}
