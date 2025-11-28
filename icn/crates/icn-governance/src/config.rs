//! Governance configuration and parameters

use crate::membership::MembershipConfig;
use crate::profile::GovernanceProfileId;
use serde::{Deserialize, Serialize};

/// Complete governance configuration for a domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Governance profile ID (e.g., "cooperative_default", "contract:\<did\>")
    pub profile: GovernanceProfileId,

    /// Membership configuration (who can vote)
    pub membership: MembershipConfig,

    /// Governance parameters (quorum, thresholds, etc.)
    pub params: GovernanceParams,
}

impl GovernanceConfig {
    /// Create a new governance configuration
    pub fn new(
        profile: GovernanceProfileId,
        membership: MembershipConfig,
        params: GovernanceParams,
    ) -> Self {
        Self {
            profile,
            membership,
            params,
        }
    }

    /// Create the default cooperative governance config
    ///
    /// - Profile: "cooperative_default" (1-member-1-vote)
    /// - Membership: Trust threshold 0.3 (known peers)
    /// - Quorum: 50% of eligible voters
    /// - Approval: Simple majority (>50%)
    pub fn cooperative_default() -> Self {
        Self {
            profile: GovernanceProfileId::builtin("cooperative_default"),
            membership: MembershipConfig::trust_threshold(0.3),
            params: GovernanceParams {
                quorum_percentage: 50,
                approval_threshold_percentage: 50,
                voting_period_seconds: 7 * 24 * 60 * 60, // 7 days
            },
        }
    }
}

/// Governance parameters (quorum, thresholds, voting period)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceParams {
    /// Minimum percentage of eligible voters that must vote (0-100)
    pub quorum_percentage: u8,

    /// Percentage of votes needed to pass (0-100)
    pub approval_threshold_percentage: u8,

    /// How long voting is open (in seconds)
    pub voting_period_seconds: u64,
}

impl GovernanceParams {
    /// Create new governance parameters
    pub fn new(
        quorum_percentage: u8,
        approval_threshold_percentage: u8,
        voting_period_seconds: u64,
    ) -> Self {
        Self {
            quorum_percentage,
            approval_threshold_percentage,
            voting_period_seconds,
        }
    }

    /// Validate parameters are in valid ranges
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.quorum_percentage > 100 {
            anyhow::bail!("Quorum percentage must be 0-100");
        }
        if self.approval_threshold_percentage > 100 {
            anyhow::bail!("Approval threshold must be 0-100");
        }
        if self.voting_period_seconds == 0 {
            anyhow::bail!("Voting period must be greater than 0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cooperative_default() {
        let config = GovernanceConfig::cooperative_default();

        assert_eq!(config.profile.0, "cooperative_default");
        assert_eq!(config.params.quorum_percentage, 50);
        assert_eq!(config.params.approval_threshold_percentage, 50);
        assert_eq!(config.params.voting_period_seconds, 7 * 24 * 60 * 60);
    }

    #[test]
    fn test_params_validation() {
        let valid = GovernanceParams::new(50, 50, 3600);
        assert!(valid.validate().is_ok());

        let invalid_quorum = GovernanceParams::new(101, 50, 3600);
        assert!(invalid_quorum.validate().is_err());

        let invalid_threshold = GovernanceParams::new(50, 101, 3600);
        assert!(invalid_threshold.validate().is_err());

        let invalid_period = GovernanceParams::new(50, 50, 0);
        assert!(invalid_period.validate().is_err());
    }
}
