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

    /// Emergency thresholds (super-majority requirements for emergency actions)
    pub emergency: EmergencyThresholds,
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
            emergency: EmergencyThresholds::default(),
        }
    }

    /// Create a new governance configuration with custom emergency thresholds
    pub fn with_emergency(
        profile: GovernanceProfileId,
        membership: MembershipConfig,
        params: GovernanceParams,
        emergency: EmergencyThresholds,
    ) -> Self {
        Self {
            profile,
            membership,
            params,
            emergency,
        }
    }

    /// Create the default cooperative governance config
    ///
    /// - Profile: "cooperative_default" (1-member-1-vote)
    /// - Membership: Trust threshold 0.3 (known peers)
    /// - Quorum: 50% of eligible voters
    /// - Approval: Simple majority (>50%)
    /// - Emergency: 67% quorum, 75% approval for freeze/veto, 80% for rollback
    pub fn cooperative_default() -> Self {
        Self {
            profile: GovernanceProfileId::builtin("cooperative_default"),
            membership: MembershipConfig::trust_threshold(0.3),
            params: GovernanceParams {
                quorum_percentage: 50,
                approval_threshold_percentage: 50,
                voting_period_seconds: 7 * 24 * 60 * 60, // 7 days
            },
            emergency: EmergencyThresholds::default(),
        }
    }
}

/// Emergency action thresholds
///
/// These require higher quorum and approval percentages than normal proposals
/// to prevent abuse while still allowing communities to respond to crises.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyThresholds {
    /// Quorum for freeze/unfreeze member (default: 67%)
    pub freeze_quorum_percentage: u8,
    /// Approval threshold for freeze/unfreeze member (default: 75%)
    pub freeze_approval_percentage: u8,

    /// Quorum for veto proposal (default: 67%)
    pub veto_quorum_percentage: u8,
    /// Approval threshold for veto proposal (default: 75%)
    pub veto_approval_percentage: u8,

    /// Quorum for force close proposal (default: 67%)
    pub force_close_quorum_percentage: u8,
    /// Approval threshold for force close proposal (default: 75%)
    pub force_close_approval_percentage: u8,

    /// Quorum for ledger rollback (default: 75%)
    /// This is the most severe action and requires highest threshold
    pub rollback_quorum_percentage: u8,
    /// Approval threshold for ledger rollback (default: 80%)
    pub rollback_approval_percentage: u8,

    /// Emergency voting period in seconds (default: 24 hours)
    /// Shorter than normal to allow rapid response
    pub emergency_voting_period_seconds: u64,
}

impl Default for EmergencyThresholds {
    fn default() -> Self {
        Self {
            // Freeze/unfreeze: 67% quorum, 75% approval
            freeze_quorum_percentage: 67,
            freeze_approval_percentage: 75,

            // Veto: 67% quorum, 75% approval
            veto_quorum_percentage: 67,
            veto_approval_percentage: 75,

            // Force close: 67% quorum, 75% approval
            force_close_quorum_percentage: 67,
            force_close_approval_percentage: 75,

            // Rollback: highest thresholds (75% quorum, 80% approval)
            rollback_quorum_percentage: 75,
            rollback_approval_percentage: 80,

            // 24 hour emergency voting period
            emergency_voting_period_seconds: 24 * 60 * 60,
        }
    }
}

impl EmergencyThresholds {
    /// Create new emergency thresholds with custom values
    pub fn new(
        freeze_quorum: u8,
        freeze_approval: u8,
        veto_quorum: u8,
        veto_approval: u8,
        force_close_quorum: u8,
        force_close_approval: u8,
        rollback_quorum: u8,
        rollback_approval: u8,
        voting_period_seconds: u64,
    ) -> Self {
        Self {
            freeze_quorum_percentage: freeze_quorum,
            freeze_approval_percentage: freeze_approval,
            veto_quorum_percentage: veto_quorum,
            veto_approval_percentage: veto_approval,
            force_close_quorum_percentage: force_close_quorum,
            force_close_approval_percentage: force_close_approval,
            rollback_quorum_percentage: rollback_quorum,
            rollback_approval_percentage: rollback_approval,
            emergency_voting_period_seconds: voting_period_seconds,
        }
    }

    /// Validate thresholds are in valid ranges
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.freeze_quorum_percentage > 100 {
            anyhow::bail!("Freeze quorum percentage must be 0-100");
        }
        if self.freeze_approval_percentage > 100 {
            anyhow::bail!("Freeze approval percentage must be 0-100");
        }
        if self.veto_quorum_percentage > 100 {
            anyhow::bail!("Veto quorum percentage must be 0-100");
        }
        if self.veto_approval_percentage > 100 {
            anyhow::bail!("Veto approval percentage must be 0-100");
        }
        if self.force_close_quorum_percentage > 100 {
            anyhow::bail!("Force close quorum percentage must be 0-100");
        }
        if self.force_close_approval_percentage > 100 {
            anyhow::bail!("Force close approval percentage must be 0-100");
        }
        if self.rollback_quorum_percentage > 100 {
            anyhow::bail!("Rollback quorum percentage must be 0-100");
        }
        if self.rollback_approval_percentage > 100 {
            anyhow::bail!("Rollback approval percentage must be 0-100");
        }
        if self.emergency_voting_period_seconds == 0 {
            anyhow::bail!("Emergency voting period must be greater than 0");
        }
        Ok(())
    }

    /// Check if this is an emergency threshold (all > 50%)
    pub fn is_super_majority(&self) -> bool {
        self.freeze_approval_percentage > 50
            && self.veto_approval_percentage > 50
            && self.force_close_approval_percentage > 50
            && self.rollback_approval_percentage > 50
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

        // Emergency thresholds should be default
        assert_eq!(config.emergency.freeze_quorum_percentage, 67);
        assert_eq!(config.emergency.freeze_approval_percentage, 75);
        assert_eq!(config.emergency.rollback_quorum_percentage, 75);
        assert_eq!(config.emergency.rollback_approval_percentage, 80);
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

    #[test]
    fn test_emergency_thresholds_default() {
        let thresholds = EmergencyThresholds::default();

        // Freeze/unfreeze: 67% quorum, 75% approval
        assert_eq!(thresholds.freeze_quorum_percentage, 67);
        assert_eq!(thresholds.freeze_approval_percentage, 75);

        // Veto: 67% quorum, 75% approval
        assert_eq!(thresholds.veto_quorum_percentage, 67);
        assert_eq!(thresholds.veto_approval_percentage, 75);

        // Force close: 67% quorum, 75% approval
        assert_eq!(thresholds.force_close_quorum_percentage, 67);
        assert_eq!(thresholds.force_close_approval_percentage, 75);

        // Rollback: 75% quorum, 80% approval (highest)
        assert_eq!(thresholds.rollback_quorum_percentage, 75);
        assert_eq!(thresholds.rollback_approval_percentage, 80);

        // Emergency voting period: 24 hours
        assert_eq!(thresholds.emergency_voting_period_seconds, 24 * 60 * 60);
    }

    #[test]
    fn test_emergency_thresholds_validation() {
        let valid = EmergencyThresholds::default();
        assert!(valid.validate().is_ok());

        // Test invalid freeze quorum
        let invalid = EmergencyThresholds {
            freeze_quorum_percentage: 101,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        // Test invalid rollback approval
        let invalid = EmergencyThresholds {
            rollback_approval_percentage: 150,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        // Test invalid voting period
        let invalid = EmergencyThresholds {
            emergency_voting_period_seconds: 0,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_emergency_thresholds_is_super_majority() {
        let thresholds = EmergencyThresholds::default();
        assert!(thresholds.is_super_majority());

        // Test with low threshold (not super-majority)
        let low_threshold = EmergencyThresholds::new(
            50, 40, // freeze: below super-majority
            67, 75, 67, 75, 75, 80, 86400,
        );
        assert!(!low_threshold.is_super_majority());
    }

    #[test]
    fn test_governance_config_with_emergency() {
        let emergency = EmergencyThresholds::new(
            80,
            90, // freeze
            80,
            90, // veto
            80,
            90, // force close
            90,
            95,           // rollback
            12 * 60 * 60, // 12 hour voting
        );

        let config = GovernanceConfig::with_emergency(
            GovernanceProfileId::builtin("test"),
            MembershipConfig::trust_threshold(0.5),
            GovernanceParams::new(60, 60, 3600),
            emergency,
        );

        assert_eq!(config.emergency.freeze_quorum_percentage, 80);
        assert_eq!(config.emergency.freeze_approval_percentage, 90);
        assert_eq!(config.emergency.rollback_approval_percentage, 95);
    }
}
