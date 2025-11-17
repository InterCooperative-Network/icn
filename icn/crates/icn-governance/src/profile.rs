//! Governance profiles - rules for evaluating votes

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
            description: "Democratic 1-member-1-vote with quorum and majority approval"
                .to_string(),
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
        // Check quorum: did enough people vote?
        let total_votes = tally.total_votes();
        let quorum_required = (eligible_voter_count * params.quorum_percentage as usize) / 100;

        if total_votes < quorum_required {
            return Ok(DecisionOutcome::NoQuorum);
        }

        // Check approval: did enough vote "for"?
        let approval_required = (total_votes * params.approval_threshold_percentage as usize) / 100;

        if tally.for_votes > approval_required {
            Ok(DecisionOutcome::Accepted)
        } else {
            Ok(DecisionOutcome::Rejected)
        }
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
}
