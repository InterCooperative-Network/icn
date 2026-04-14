//! Governance profiles - rules for evaluating votes

use crate::config::{DecisionMode, ProposalThresholds};
use crate::tally::VoteTally;
use crate::GovernanceParams;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Unique identifier for a governance profile
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GovernanceProfileId(pub String);

impl GovernanceProfileId {
    /// Create a built-in profile ID
    pub fn builtin(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Create a contract-based profile ID from a DID
    pub fn contract(did: &str) -> Self {
        Self(format!("contract:{did}"))
    }

    /// Check if this is a contract-based profile
    pub fn is_contract(&self) -> bool {
        self.0.starts_with("contract:")
    }
}

impl std::fmt::Display for GovernanceProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Outcome of evaluating a proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionOutcome {
    /// Proposal accepted
    Accepted,

    /// Proposal rejected
    Rejected,

    /// Quorum not met
    NoQuorum,
}

/// A governance profile defines rules for evaluating votes
///
/// Profiles are extensible via the GovernanceRule trait
pub trait GovernanceRule: Send + Sync {
    /// Evaluate a vote tally and determine the outcome
    fn evaluate(
        &self,
        tally: &VoteTally,
        params: &GovernanceParams,
        eligible_voter_count: usize,
    ) -> Result<DecisionOutcome>;

    /// Get the profile ID
    fn profile_id(&self) -> &GovernanceProfileId;

    /// Get a human-readable description of this profile
    fn description(&self) -> &str;
}

/// The built-in governance profile
pub struct GovernanceProfile {
    id: GovernanceProfileId,
    description: String,
}

impl GovernanceProfile {
    /// Create the default cooperative profile
    ///
    /// Rules:
    /// - 1 member = 1 vote (equal weight)
    /// - Quorum: configurable percentage of eligible voters
    /// - Approval: configurable percentage of votes cast
    pub fn cooperative_default() -> Self {
        Self {
            id: GovernanceProfileId::builtin("cooperative_default"),
            description: "Democratic 1-member-1-vote with quorum and majority approval".to_string(),
        }
    }
}

impl GovernanceProfile {
    /// Evaluate a vote tally with explicit thresholds
    ///
    /// This method allows specifying custom quorum and approval thresholds
    /// instead of using the default params. This is used to enforce
    /// higher thresholds for emergency proposals (freeze, veto, rollback).
    ///
    /// # Arguments
    /// * `tally` - The vote tally to evaluate
    /// * `thresholds` - Custom quorum and approval thresholds
    /// * `eligible_voter_count` - Total number of eligible voters
    ///
    /// # Returns
    /// The decision outcome (Accepted, Rejected, or NoQuorum)
    pub fn evaluate_with_thresholds(
        &self,
        tally: &VoteTally,
        thresholds: ProposalThresholds,
        eligible_voter_count: usize,
    ) -> Result<DecisionOutcome> {
        // Check quorum: did enough people vote?
        let total_votes = tally.total_votes();
        let quorum_required = (eligible_voter_count * thresholds.quorum_percentage as usize) / 100;

        if total_votes < quorum_required {
            return Ok(DecisionOutcome::NoQuorum);
        }

        // Check approval: did enough vote "for"?
        let approval_required = (total_votes * thresholds.approval_percentage as usize) / 100;

        if tally.for_votes > approval_required {
            Ok(DecisionOutcome::Accepted)
        } else {
            Ok(DecisionOutcome::Rejected)
        }
    }

    /// Evaluate a proposal using consent-based decision-making.
    ///
    /// In consent mode a proposal passes unless objections (`against_votes`) exceed
    /// the configured `max_objections` threshold. Quorum still applies: the proposal
    /// must receive enough total votes before the objection count matters.
    ///
    /// # Arguments
    /// * `tally` - The vote tally to evaluate
    /// * `thresholds` - Quorum and objection-limit thresholds
    /// * `eligible_voter_count` - Total number of eligible voters
    pub fn evaluate_consent(
        &self,
        tally: &VoteTally,
        thresholds: ProposalThresholds,
        eligible_voter_count: usize,
    ) -> Result<DecisionOutcome> {
        // Quorum check is identical regardless of decision mode
        let total_votes = tally.total_votes();
        let quorum_required = (eligible_voter_count * thresholds.quorum_percentage as usize) / 100;

        if total_votes < quorum_required {
            return Ok(DecisionOutcome::NoQuorum);
        }

        // In consent mode: accept unless objections exceed threshold
        let max_objections = thresholds.effective_max_objections() as usize;
        if tally.against_votes > max_objections {
            Ok(DecisionOutcome::Rejected)
        } else {
            Ok(DecisionOutcome::Accepted)
        }
    }

    /// Evaluate a proposal using the decision mode from governance params.
    ///
    /// Routes to either majority or consent evaluation based on the domain's
    /// configured `decision_mode`.
    pub fn evaluate_with_mode(
        &self,
        tally: &VoteTally,
        thresholds: ProposalThresholds,
        eligible_voter_count: usize,
        decision_mode: DecisionMode,
    ) -> Result<DecisionOutcome> {
        match decision_mode {
            DecisionMode::Majority => {
                self.evaluate_with_thresholds(tally, thresholds, eligible_voter_count)
            }
            DecisionMode::Consent => self.evaluate_consent(tally, thresholds, eligible_voter_count),
        }
    }
}

impl GovernanceRule for GovernanceProfile {
    fn evaluate(
        &self,
        tally: &VoteTally,
        params: &GovernanceParams,
        eligible_voter_count: usize,
    ) -> Result<DecisionOutcome> {
        // Delegate to evaluate_with_thresholds with params converted to thresholds
        self.evaluate_with_thresholds(
            tally,
            ProposalThresholds::new(
                params.quorum_percentage,
                params.approval_threshold_percentage,
            ),
            eligible_voter_count,
        )
    }

    fn profile_id(&self) -> &GovernanceProfileId {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_id_builtin() {
        let id = GovernanceProfileId::builtin("cooperative_default");
        assert_eq!(id.0, "cooperative_default");
        assert!(!id.is_contract());
    }

    #[test]
    fn test_profile_id_contract() {
        let id = GovernanceProfileId::contract("did:icn:123");
        assert_eq!(id.0, "contract:did:icn:123");
        assert!(id.is_contract());
    }

    #[test]
    fn test_cooperative_default_quorum_not_met() {
        let profile = GovernanceProfile::cooperative_default();
        let params = GovernanceParams::new(50, 50, 3600);

        // Only 2 out of 10 voters voted (20% < 50% quorum)
        let tally = VoteTally::new(2, 0, 0);
        let outcome = profile.evaluate(&tally, &params, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::NoQuorum);
    }

    #[test]
    fn test_cooperative_default_accepted() {
        let profile = GovernanceProfile::cooperative_default();
        let params = GovernanceParams::new(50, 50, 3600);

        // 6 out of 10 voted (60% >= 50% quorum)
        // 4 voted for, 2 against (67% > 50% approval)
        let tally = VoteTally::new(4, 2, 0);
        let outcome = profile.evaluate(&tally, &params, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::Accepted);
    }

    #[test]
    fn test_cooperative_default_rejected() {
        let profile = GovernanceProfile::cooperative_default();
        let params = GovernanceParams::new(50, 50, 3600);

        // 6 out of 10 voted (60% >= 50% quorum)
        // 2 voted for, 4 against (33% < 50% approval)
        let tally = VoteTally::new(2, 4, 0);
        let outcome = profile.evaluate(&tally, &params, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::Rejected);
    }

    #[test]
    fn test_cooperative_default_exact_threshold() {
        let profile = GovernanceProfile::cooperative_default();
        let params = GovernanceParams::new(50, 50, 3600);

        // 6 out of 10 voted (60% >= 50% quorum)
        // 3 voted for, 3 against (50% == 50% approval threshold, but we need >50%)
        let tally = VoteTally::new(3, 3, 0);
        let outcome = profile.evaluate(&tally, &params, 10).unwrap();

        // With >50% requirement, exactly 50% should be rejected
        assert_eq!(outcome, DecisionOutcome::Rejected);
    }

    // ========== Consent Mode Tests ==========

    #[test]
    fn test_consent_zero_objections_accepted() {
        let profile = GovernanceProfile::cooperative_default();
        let thresholds = ProposalThresholds::new(50, 50); // max_objections defaults to 0

        // 6 out of 10 voted, all in favor (0 objections)
        let tally = VoteTally::new(6, 0, 0);
        let outcome = profile.evaluate_consent(&tally, thresholds, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::Accepted);
    }

    #[test]
    fn test_consent_one_objection_strict_rejected() {
        let profile = GovernanceProfile::cooperative_default();
        // max_objections = 0 (strict consensus: any objection blocks)
        let thresholds = ProposalThresholds::with_max_objections(50, 50, 0);

        // 6 out of 10 voted, 5 for, 1 against
        let tally = VoteTally::new(5, 1, 0);
        let outcome = profile.evaluate_consent(&tally, thresholds, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::Rejected);
    }

    #[test]
    fn test_consent_one_objection_tolerated() {
        let profile = GovernanceProfile::cooperative_default();
        // max_objections = 1 (one objection is allowed)
        let thresholds = ProposalThresholds::with_max_objections(50, 50, 1);

        // 6 out of 10 voted, 5 for, 1 against
        let tally = VoteTally::new(5, 1, 0);
        let outcome = profile.evaluate_consent(&tally, thresholds, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::Accepted);
    }

    #[test]
    fn test_consent_two_objections_with_max_one_rejected() {
        let profile = GovernanceProfile::cooperative_default();
        let thresholds = ProposalThresholds::with_max_objections(50, 50, 1);

        // 6 out of 10 voted, 4 for, 2 against (exceeds max_objections=1)
        let tally = VoteTally::new(4, 2, 0);
        let outcome = profile.evaluate_consent(&tally, thresholds, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::Rejected);
    }

    #[test]
    fn test_consent_quorum_not_met() {
        let profile = GovernanceProfile::cooperative_default();
        let thresholds = ProposalThresholds::new(50, 50);

        // Only 2 out of 10 voted (20% < 50% quorum)
        let tally = VoteTally::new(2, 0, 0);
        let outcome = profile.evaluate_consent(&tally, thresholds, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::NoQuorum);
    }

    #[test]
    fn test_consent_abstentions_count_toward_quorum() {
        let profile = GovernanceProfile::cooperative_default();
        let thresholds = ProposalThresholds::with_max_objections(50, 50, 0);

        // 6 out of 10 voted: 3 for, 0 against, 3 abstain
        // Quorum met (60%), zero objections → accepted
        let tally = VoteTally::new(3, 0, 3);
        let outcome = profile.evaluate_consent(&tally, thresholds, 10).unwrap();

        assert_eq!(outcome, DecisionOutcome::Accepted);
    }

    #[test]
    fn test_evaluate_with_mode_majority() {
        let profile = GovernanceProfile::cooperative_default();
        let thresholds = ProposalThresholds::new(50, 50);

        // Standard majority test
        let tally = VoteTally::new(4, 2, 0);
        let outcome = profile
            .evaluate_with_mode(&tally, thresholds, 10, DecisionMode::Majority)
            .unwrap();

        assert_eq!(outcome, DecisionOutcome::Accepted);
    }

    #[test]
    fn test_evaluate_with_mode_consent() {
        let profile = GovernanceProfile::cooperative_default();
        let thresholds = ProposalThresholds::with_max_objections(50, 50, 0);

        // In consent mode, 5 for + 1 against with max_objections=0 → rejected
        let tally = VoteTally::new(5, 1, 0);
        let outcome = profile
            .evaluate_with_mode(&tally, thresholds, 10, DecisionMode::Consent)
            .unwrap();

        assert_eq!(outcome, DecisionOutcome::Rejected);

        // Same tally in majority mode → accepted (5/6 > 50%)
        let outcome = profile
            .evaluate_with_mode(&tally, thresholds, 10, DecisionMode::Majority)
            .unwrap();

        assert_eq!(outcome, DecisionOutcome::Accepted);
    }

    #[test]
    fn test_majority_mode_regression_unchanged() {
        // Ensure majority mode behavior is identical to before
        let profile = GovernanceProfile::cooperative_default();
        let params = GovernanceParams::new(50, 50, 3600);
        let thresholds = ProposalThresholds::new(
            params.quorum_percentage,
            params.approval_threshold_percentage,
        );

        // Accepted case
        let tally = VoteTally::new(4, 2, 0);
        let old_result = profile.evaluate(&tally, &params, 10).unwrap();
        let new_result = profile
            .evaluate_with_mode(&tally, thresholds, 10, DecisionMode::Majority)
            .unwrap();
        assert_eq!(old_result, new_result);
        assert_eq!(new_result, DecisionOutcome::Accepted);

        // Rejected case
        let tally = VoteTally::new(2, 4, 0);
        let old_result = profile.evaluate(&tally, &params, 10).unwrap();
        let new_result = profile
            .evaluate_with_mode(&tally, thresholds, 10, DecisionMode::Majority)
            .unwrap();
        assert_eq!(old_result, new_result);
        assert_eq!(new_result, DecisionOutcome::Rejected);

        // NoQuorum case
        let tally = VoteTally::new(2, 0, 0);
        let old_result = profile.evaluate(&tally, &params, 10).unwrap();
        let new_result = profile
            .evaluate_with_mode(&tally, thresholds, 10, DecisionMode::Majority)
            .unwrap();
        assert_eq!(old_result, new_result);
        assert_eq!(new_result, DecisionOutcome::NoQuorum);
    }
}
