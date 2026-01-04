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
    /// - Deliberation: 7 days required
    /// - Voting: 7 days
    /// - Emergency: 67% quorum, 75% approval for freeze/veto, 80% for rollback
    pub fn cooperative_default() -> Self {
        Self {
            profile: GovernanceProfileId::builtin("cooperative_default"),
            membership: MembershipConfig::trust_threshold(0.3),
            params: GovernanceParams::default(),
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

    // === Treasury Thresholds (Issue #246) ===
    /// Quorum for treasury budget creation (default: 50%)
    pub treasury_budget_quorum_percentage: u8,
    /// Approval threshold for treasury budget creation (default: 50%)
    pub treasury_budget_approval_percentage: u8,

    /// Quorum for large treasury withdrawals (default: 60%)
    pub treasury_withdrawal_quorum_percentage: u8,
    /// Approval threshold for large treasury withdrawals (default: 67%)
    pub treasury_withdrawal_approval_percentage: u8,

    /// Quorum for treasury spending rule modifications (default: 60%)
    pub treasury_rule_quorum_percentage: u8,
    /// Approval threshold for treasury spending rule modifications (default: 67%)
    pub treasury_rule_approval_percentage: u8,
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

            // Treasury budget creation: 50% quorum, 50% approval (normal)
            treasury_budget_quorum_percentage: 50,
            treasury_budget_approval_percentage: 50,

            // Large treasury withdrawals: 60% quorum, 67% approval
            treasury_withdrawal_quorum_percentage: 60,
            treasury_withdrawal_approval_percentage: 67,

            // Treasury spending rule modifications: 60% quorum, 67% approval
            treasury_rule_quorum_percentage: 60,
            treasury_rule_approval_percentage: 67,
        }
    }
}

impl EmergencyThresholds {
    /// Create new emergency thresholds with custom values
    /// Treasury thresholds use defaults; use builder methods to customize
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
            // Use defaults for treasury thresholds
            treasury_budget_quorum_percentage: 50,
            treasury_budget_approval_percentage: 50,
            treasury_withdrawal_quorum_percentage: 60,
            treasury_withdrawal_approval_percentage: 67,
            treasury_rule_quorum_percentage: 60,
            treasury_rule_approval_percentage: 67,
        }
    }

    /// Set treasury budget thresholds
    pub fn with_treasury_budget_thresholds(mut self, quorum: u8, approval: u8) -> Self {
        self.treasury_budget_quorum_percentage = quorum;
        self.treasury_budget_approval_percentage = approval;
        self
    }

    /// Set treasury withdrawal thresholds
    pub fn with_treasury_withdrawal_thresholds(mut self, quorum: u8, approval: u8) -> Self {
        self.treasury_withdrawal_quorum_percentage = quorum;
        self.treasury_withdrawal_approval_percentage = approval;
        self
    }

    /// Set treasury spending rule thresholds
    pub fn with_treasury_rule_thresholds(mut self, quorum: u8, approval: u8) -> Self {
        self.treasury_rule_quorum_percentage = quorum;
        self.treasury_rule_approval_percentage = approval;
        self
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
        // Treasury threshold validation
        if self.treasury_budget_quorum_percentage > 100 {
            anyhow::bail!("Treasury budget quorum percentage must be 0-100");
        }
        if self.treasury_budget_approval_percentage > 100 {
            anyhow::bail!("Treasury budget approval percentage must be 0-100");
        }
        if self.treasury_withdrawal_quorum_percentage > 100 {
            anyhow::bail!("Treasury withdrawal quorum percentage must be 0-100");
        }
        if self.treasury_withdrawal_approval_percentage > 100 {
            anyhow::bail!("Treasury withdrawal approval percentage must be 0-100");
        }
        if self.treasury_rule_quorum_percentage > 100 {
            anyhow::bail!("Treasury rule quorum percentage must be 0-100");
        }
        if self.treasury_rule_approval_percentage > 100 {
            anyhow::bail!("Treasury rule approval percentage must be 0-100");
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

/// Governance parameters (quorum, thresholds, voting period, deliberation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceParams {
    /// Minimum percentage of eligible voters that must vote (0-100)
    pub quorum_percentage: u8,

    /// Percentage of votes needed to pass (0-100)
    pub approval_threshold_percentage: u8,

    /// How long voting is open (in seconds)
    pub voting_period_seconds: u64,

    /// Default deliberation period before voting (in seconds)
    /// Default: 604800 (7 days)
    pub deliberation_period_seconds: u64,

    /// Whether deliberation is required before voting
    /// When true, proposals must go through Draft → Deliberation → Open
    /// When false, proposals can skip directly to Open (Draft → Open)
    /// Default: true
    pub require_deliberation: bool,

    /// Minimum deliberation period (in seconds)
    /// Cannot set deliberation shorter than this
    /// Default: 86400 (1 day)
    pub min_deliberation_seconds: u64,

    /// Maximum deliberation period (in seconds)
    /// Cannot set deliberation longer than this
    /// Default: 2592000 (30 days)
    pub max_deliberation_seconds: u64,

    /// Maximum execution delay for proposals (in seconds)
    /// Limits how far into the future a proposal's effective_at can be set
    /// Default: 31536000 (1 year = 365 days)
    #[serde(default = "default_max_execution_delay")]
    pub max_execution_delay_seconds: u64,
}

/// Default max execution delay: 1 year
fn default_max_execution_delay() -> u64 {
    365 * 24 * 60 * 60 // 31536000 seconds
}

impl Default for GovernanceParams {
    fn default() -> Self {
        Self {
            quorum_percentage: 50,
            approval_threshold_percentage: 50,
            voting_period_seconds: 7 * 24 * 60 * 60, // 7 days
            deliberation_period_seconds: 7 * 24 * 60 * 60, // 7 days
            require_deliberation: true,
            min_deliberation_seconds: 24 * 60 * 60, // 1 day
            max_deliberation_seconds: 30 * 24 * 60 * 60, // 30 days
            max_execution_delay_seconds: default_max_execution_delay(),
        }
    }
}

impl GovernanceParams {
    /// Create new governance parameters with basic settings
    /// Uses default deliberation settings
    pub fn new(
        quorum_percentage: u8,
        approval_threshold_percentage: u8,
        voting_period_seconds: u64,
    ) -> Self {
        Self {
            quorum_percentage,
            approval_threshold_percentage,
            voting_period_seconds,
            ..Default::default()
        }
    }

    /// Create governance parameters with custom deliberation settings
    pub fn with_deliberation(
        quorum_percentage: u8,
        approval_threshold_percentage: u8,
        voting_period_seconds: u64,
        deliberation_period_seconds: u64,
        require_deliberation: bool,
    ) -> Self {
        Self {
            quorum_percentage,
            approval_threshold_percentage,
            voting_period_seconds,
            deliberation_period_seconds,
            require_deliberation,
            ..Default::default()
        }
    }

    /// Set deliberation requirement
    pub fn set_require_deliberation(mut self, require: bool) -> Self {
        self.require_deliberation = require;
        self
    }

    /// Set deliberation period
    pub fn set_deliberation_period(mut self, seconds: u64) -> Self {
        self.deliberation_period_seconds = seconds;
        self
    }

    /// Set minimum and maximum deliberation bounds
    pub fn set_deliberation_bounds(mut self, min_seconds: u64, max_seconds: u64) -> Self {
        self.min_deliberation_seconds = min_seconds;
        self.max_deliberation_seconds = max_seconds;
        self
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

        // Deliberation validation
        if self.require_deliberation && self.deliberation_period_seconds == 0 {
            anyhow::bail!("Deliberation period must be greater than 0 when required");
        }
        if self.deliberation_period_seconds < self.min_deliberation_seconds {
            anyhow::bail!(
                "Deliberation period ({}) is less than minimum ({})",
                self.deliberation_period_seconds,
                self.min_deliberation_seconds
            );
        }
        if self.deliberation_period_seconds > self.max_deliberation_seconds {
            anyhow::bail!(
                "Deliberation period ({}) exceeds maximum ({})",
                self.deliberation_period_seconds,
                self.max_deliberation_seconds
            );
        }
        if self.min_deliberation_seconds > self.max_deliberation_seconds {
            anyhow::bail!(
                "Minimum deliberation ({}) exceeds maximum ({})",
                self.min_deliberation_seconds,
                self.max_deliberation_seconds
            );
        }

        Ok(())
    }

    /// Validate a specific deliberation duration against bounds
    pub fn validate_deliberation_duration(&self, duration_seconds: u64) -> anyhow::Result<()> {
        if duration_seconds < self.min_deliberation_seconds {
            anyhow::bail!(
                "Deliberation duration ({} seconds) is less than minimum ({} seconds)",
                duration_seconds,
                self.min_deliberation_seconds
            );
        }
        if duration_seconds > self.max_deliberation_seconds {
            anyhow::bail!(
                "Deliberation duration ({} seconds) exceeds maximum ({} seconds)",
                duration_seconds,
                self.max_deliberation_seconds
            );
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

        // Deliberation parameters should be default
        assert_eq!(config.params.deliberation_period_seconds, 7 * 24 * 60 * 60);
        assert!(config.params.require_deliberation);
        assert_eq!(config.params.min_deliberation_seconds, 24 * 60 * 60);
        assert_eq!(config.params.max_deliberation_seconds, 30 * 24 * 60 * 60);

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
    fn test_deliberation_validation() {
        // Valid default params
        let valid = GovernanceParams::default();
        assert!(valid.validate().is_ok());

        // Deliberation period below minimum
        let invalid = GovernanceParams {
            deliberation_period_seconds: 3600, // 1 hour, below 1 day minimum
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        // Deliberation period above maximum
        let invalid = GovernanceParams {
            deliberation_period_seconds: 60 * 24 * 60 * 60, // 60 days, above 30 day max
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        // Min > max is invalid
        let invalid = GovernanceParams {
            min_deliberation_seconds: 10 * 24 * 60 * 60,
            max_deliberation_seconds: 5 * 24 * 60 * 60,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        // Zero deliberation with require_deliberation = true
        let invalid = GovernanceParams {
            deliberation_period_seconds: 0,
            min_deliberation_seconds: 0,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        // Zero deliberation with require_deliberation = false is fine
        let valid = GovernanceParams {
            require_deliberation: false,
            deliberation_period_seconds: 0,
            min_deliberation_seconds: 0,
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_params_with_deliberation() {
        let params = GovernanceParams::with_deliberation(
            60,    // quorum
            51,    // approval
            3600,  // voting period
            86400, // deliberation period (1 day)
            true,  // require deliberation
        );

        assert_eq!(params.quorum_percentage, 60);
        assert_eq!(params.approval_threshold_percentage, 51);
        assert_eq!(params.voting_period_seconds, 3600);
        assert_eq!(params.deliberation_period_seconds, 86400);
        assert!(params.require_deliberation);
    }

    #[test]
    fn test_validate_deliberation_duration() {
        let params = GovernanceParams::default();

        // Valid duration within bounds
        assert!(params
            .validate_deliberation_duration(7 * 24 * 60 * 60)
            .is_ok());

        // Below minimum
        assert!(params.validate_deliberation_duration(3600).is_err());

        // Above maximum
        assert!(params
            .validate_deliberation_duration(60 * 24 * 60 * 60)
            .is_err());
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
