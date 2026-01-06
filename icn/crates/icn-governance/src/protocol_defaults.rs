//! Default protocol parameters for ICN
//!
//! These defaults are applied on first run before governance can modify them.
//! Each parameter includes constraints that limit what values governance can set.

use crate::protocol::{ParameterConstraints, ParameterScope, ParameterValue, ProtocolParameter};
use icn_time::current_timestamp_secs;

/// Create a global protocol parameter with constraints
fn param(
    id: &str,
    name: &str,
    description: &str,
    value: ParameterValue,
    constraints: ParameterConstraints,
) -> ProtocolParameter {
    ProtocolParameter {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        value,
        constraints,
        scope: ParameterScope::Global,
        updated_at: current_timestamp_secs(),
        updated_by: None,
        version: 0,
    }
}

/// Get all default protocol parameters
///
/// These parameters define the initial configuration of the ICN network.
/// Cooperatives and federations can override these through governance.
pub fn default_parameters() -> Vec<ProtocolParameter> {
    vec![
        // === Gossip Protocol Parameters ===
        param(
            "gossip.max_message_size",
            "Maximum Gossip Message Size",
            "Maximum size in bytes for gossip messages. Larger messages are rejected.",
            ParameterValue::Bytes(1_048_576), // 1MB
            ParameterConstraints {
                min: Some(ParameterValue::Bytes(1024)),       // 1KB minimum
                max: Some(ParameterValue::Bytes(10_485_760)), // 10MB maximum
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "gossip.fanout",
            "Gossip Fanout",
            "Number of peers to forward each gossip message to.",
            ParameterValue::Integer(8),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(2)),
                max: Some(ParameterValue::Integer(20)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "gossip.heartbeat_interval",
            "Gossip Heartbeat Interval",
            "Seconds between gossip protocol heartbeats.",
            ParameterValue::Duration(5),
            ParameterConstraints {
                min: Some(ParameterValue::Duration(1)),
                max: Some(ParameterValue::Duration(60)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "gossip.anti_entropy_interval",
            "Anti-Entropy Interval",
            "Seconds between anti-entropy synchronization rounds.",
            ParameterValue::Duration(30),
            ParameterConstraints {
                min: Some(ParameterValue::Duration(10)),
                max: Some(ParameterValue::Duration(300)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        // === Network Parameters ===
        param(
            "network.connection_timeout",
            "Connection Timeout",
            "Seconds before a connection attempt times out.",
            ParameterValue::Duration(30),
            ParameterConstraints {
                min: Some(ParameterValue::Duration(5)),
                max: Some(ParameterValue::Duration(120)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "network.max_connections",
            "Maximum Connections",
            "Maximum number of peer connections per node.",
            ParameterValue::Integer(100),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(10)),
                max: Some(ParameterValue::Integer(1000)),
                allowed_values: None,
                requires_restart: true, // Requires restart to apply
                allow_override: true,
            },
        ),
        param(
            "network.idle_timeout",
            "Idle Connection Timeout",
            "Seconds of inactivity before closing a connection.",
            ParameterValue::Duration(300),
            ParameterConstraints {
                min: Some(ParameterValue::Duration(60)),
                max: Some(ParameterValue::Duration(3600)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        // === Ledger Parameters ===
        param(
            "ledger.default_credit_limit",
            "Default Credit Limit",
            "Default credit limit for new member accounts.",
            ParameterValue::Integer(1000),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(0)),
                max: Some(ParameterValue::Integer(1_000_000)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "ledger.max_transaction_amount",
            "Maximum Transaction Amount",
            "Maximum amount for a single transaction.",
            ParameterValue::Integer(1_000_000),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(1000)),
                max: Some(ParameterValue::Integer(100_000_000)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "ledger.demurrage_rate",
            "Demurrage Rate",
            "Annual demurrage rate as a percentage (0-100).",
            ParameterValue::Percentage(0.0), // No demurrage by default
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(0.0)),
                max: Some(ParameterValue::Percentage(10.0)), // Max 10% annual demurrage
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        // === Governance Parameters ===
        param(
            "governance.min_quorum",
            "Minimum Quorum",
            "Minimum voter participation required as a percentage (0-100).",
            ParameterValue::Percentage(50.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(10.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.default_vote_duration",
            "Default Vote Duration",
            "Default voting period in seconds.",
            ParameterValue::Duration(604_800), // 7 days
            ParameterConstraints {
                min: Some(ParameterValue::Duration(3600)), // 1 hour minimum
                max: Some(ParameterValue::Duration(2_592_000)), // 30 days maximum
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.min_approval",
            "Minimum Approval",
            "Minimum approval percentage for proposals to pass (0-100).",
            ParameterValue::Percentage(50.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)), // At least simple majority
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.supermajority_threshold",
            "Supermajority Threshold",
            "Approval percentage required for constitutional changes (0-100).",
            ParameterValue::Percentage(66.67),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(60.0)),
                max: Some(ParameterValue::Percentage(90.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.max_delegation_depth",
            "Maximum Delegation Depth",
            "Maximum chain length for vote delegation.",
            ParameterValue::Integer(5),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(1)),
                max: Some(ParameterValue::Integer(10)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        // === Proposal-Type-Specific Thresholds (Issue #477) ===
        // These enable runtime configurability of quorum/approval thresholds via governance.
        // Cooperatives can override these to customize their decision-making rules.
        //
        // Emergency proposals (freeze, veto, rollback) have higher default thresholds
        // to prevent low-turnout manipulation attacks.
        //
        // Quorum: Minimum voter participation (% of eligible voters)
        param(
            "governance.quorum.freeze",
            "Freeze Member Quorum",
            "Quorum percentage required for freeze/unfreeze member proposals.",
            ParameterValue::Percentage(67.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)), // At least majority
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.quorum.veto",
            "Veto Proposal Quorum",
            "Quorum percentage required for veto proposals.",
            ParameterValue::Percentage(67.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.quorum.force_close",
            "Force Close Quorum",
            "Quorum percentage required for force close proposals.",
            ParameterValue::Percentage(67.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.quorum.rollback",
            "Ledger Rollback Quorum",
            "Quorum percentage required for ledger rollback proposals (highest threshold).",
            ParameterValue::Percentage(75.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(60.0)), // Higher minimum for rollback
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.quorum.treasury_budget",
            "Treasury Budget Quorum",
            "Quorum percentage required for treasury budget proposals.",
            ParameterValue::Percentage(50.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(25.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.quorum.treasury_withdrawal",
            "Treasury Withdrawal Quorum",
            "Quorum percentage required for treasury withdrawal proposals.",
            ParameterValue::Percentage(60.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(40.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.quorum.treasury_rule",
            "Treasury Rule Quorum",
            "Quorum percentage required for treasury spending rule modifications.",
            ParameterValue::Percentage(60.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(40.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        // Approval: Minimum yes votes as % of votes cast
        param(
            "governance.approval.freeze",
            "Freeze Member Approval",
            "Approval percentage required for freeze/unfreeze member proposals.",
            ParameterValue::Percentage(75.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.approval.veto",
            "Veto Proposal Approval",
            "Approval percentage required for veto proposals.",
            ParameterValue::Percentage(75.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.approval.force_close",
            "Force Close Approval",
            "Approval percentage required for force close proposals.",
            ParameterValue::Percentage(75.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.approval.rollback",
            "Ledger Rollback Approval",
            "Approval percentage required for ledger rollback proposals (highest threshold).",
            ParameterValue::Percentage(80.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(66.0)), // Higher minimum for rollback
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.approval.treasury_budget",
            "Treasury Budget Approval",
            "Approval percentage required for treasury budget proposals.",
            ParameterValue::Percentage(50.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.approval.treasury_withdrawal",
            "Treasury Withdrawal Approval",
            "Approval percentage required for treasury withdrawal proposals.",
            ParameterValue::Percentage(67.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "governance.approval.treasury_rule",
            "Treasury Rule Approval",
            "Approval percentage required for treasury spending rule modifications.",
            ParameterValue::Percentage(67.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(50.0)),
                max: Some(ParameterValue::Percentage(100.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        // === Trust Parameters ===
        param(
            "trust.min_endorsements",
            "Minimum Endorsements",
            "Minimum endorsements required for new members.",
            ParameterValue::Integer(3),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(1)),
                max: Some(ParameterValue::Integer(10)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "trust.decay_rate",
            "Trust Decay Rate",
            "Daily trust score decay rate as a percentage (0-100).",
            ParameterValue::Percentage(1.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(0.0)),
                max: Some(ParameterValue::Percentage(10.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "trust.anomaly_threshold",
            "Anomaly Detection Threshold",
            "Trust score threshold below which behavior is flagged for review.",
            ParameterValue::Percentage(20.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(5.0)),
                max: Some(ParameterValue::Percentage(50.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        // === Rate Limiting Parameters ===
        param(
            "ratelimit.isolated_limit",
            "Isolated Member Rate Limit",
            "Maximum messages per second for isolated (low trust) members.",
            ParameterValue::Integer(10),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(1)),
                max: Some(ParameterValue::Integer(50)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "ratelimit.known_limit",
            "Known Member Rate Limit",
            "Maximum messages per second for known members.",
            ParameterValue::Integer(50),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(10)),
                max: Some(ParameterValue::Integer(200)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "ratelimit.partner_limit",
            "Partner Rate Limit",
            "Maximum messages per second for partner members.",
            ParameterValue::Integer(100),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(20)),
                max: Some(ParameterValue::Integer(500)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "ratelimit.federated_limit",
            "Federated Member Rate Limit",
            "Maximum messages per second for federated members.",
            ParameterValue::Integer(200),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(50)),
                max: Some(ParameterValue::Integer(1000)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        // === Compute Layer Parameters ===
        param(
            "compute.task_timeout",
            "Task Timeout",
            "Maximum execution time for compute tasks in seconds.",
            ParameterValue::Duration(300),
            ParameterConstraints {
                min: Some(ParameterValue::Duration(10)),
                max: Some(ParameterValue::Duration(3600)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
        param(
            "compute.max_concurrent_tasks",
            "Max Concurrent Tasks",
            "Maximum number of concurrent compute tasks per node.",
            ParameterValue::Integer(10),
            ParameterConstraints {
                min: Some(ParameterValue::Integer(1)),
                max: Some(ParameterValue::Integer(100)),
                allowed_values: None,
                requires_restart: true,
                allow_override: true,
            },
        ),
        param(
            "compute.min_trust_score",
            "Minimum Trust Score for Compute",
            "Minimum trust score required to execute compute tasks (0-100).",
            ParameterValue::Percentage(40.0),
            ParameterConstraints {
                min: Some(ParameterValue::Percentage(10.0)),
                max: Some(ParameterValue::Percentage(90.0)),
                allowed_values: None,
                requires_restart: false,
                allow_override: true,
            },
        ),
    ]
}

/// Get a specific default parameter by ID
pub fn get_default_parameter(id: &str) -> Option<ProtocolParameter> {
    default_parameters().into_iter().find(|p| p.id == id)
}

/// Parameter categories for display purposes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterCategory {
    Gossip,
    Network,
    Ledger,
    Governance,
    Trust,
    RateLimit,
    Compute,
}

impl ParameterCategory {
    /// Get the prefix for this category
    pub fn prefix(&self) -> &'static str {
        match self {
            ParameterCategory::Gossip => "gossip.",
            ParameterCategory::Network => "network.",
            ParameterCategory::Ledger => "ledger.",
            ParameterCategory::Governance => "governance.",
            ParameterCategory::Trust => "trust.",
            ParameterCategory::RateLimit => "ratelimit.",
            ParameterCategory::Compute => "compute.",
        }
    }

    /// Determine category from parameter ID
    pub fn from_id(id: &str) -> Option<Self> {
        if id.starts_with("gossip.") {
            Some(ParameterCategory::Gossip)
        } else if id.starts_with("network.") {
            Some(ParameterCategory::Network)
        } else if id.starts_with("ledger.") {
            Some(ParameterCategory::Ledger)
        } else if id.starts_with("governance.") {
            Some(ParameterCategory::Governance)
        } else if id.starts_with("trust.") {
            Some(ParameterCategory::Trust)
        } else if id.starts_with("ratelimit.") {
            Some(ParameterCategory::RateLimit)
        } else if id.starts_with("compute.") {
            Some(ParameterCategory::Compute)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_parameters_not_empty() {
        let params = default_parameters();
        assert!(!params.is_empty());
        assert!(params.len() >= 20); // We have 24 parameters
    }

    #[test]
    fn test_all_parameters_have_valid_ids() {
        let params = default_parameters();
        for param in params {
            assert!(!param.id.is_empty());
            assert!(
                param.id.contains('.'),
                "Parameter ID should be namespaced: {}",
                param.id
            );
        }
    }

    #[test]
    fn test_all_parameters_have_category() {
        let params = default_parameters();
        for param in params {
            let category = ParameterCategory::from_id(&param.id);
            assert!(
                category.is_some(),
                "Parameter {} should have a category",
                param.id
            );
        }
    }

    #[test]
    fn test_get_default_parameter() {
        let param = get_default_parameter("governance.min_quorum");
        assert!(param.is_some());
        let param = param.unwrap();
        assert_eq!(param.id, "governance.min_quorum");
        assert!(matches!(param.value, ParameterValue::Percentage(50.0)));
    }

    #[test]
    fn test_get_default_parameter_not_found() {
        let param = get_default_parameter("nonexistent.parameter");
        assert!(param.is_none());
    }

    #[test]
    fn test_parameter_categories() {
        assert_eq!(
            ParameterCategory::from_id("gossip.fanout"),
            Some(ParameterCategory::Gossip)
        );
        assert_eq!(
            ParameterCategory::from_id("network.max_connections"),
            Some(ParameterCategory::Network)
        );
        assert_eq!(
            ParameterCategory::from_id("ledger.default_credit_limit"),
            Some(ParameterCategory::Ledger)
        );
        assert_eq!(
            ParameterCategory::from_id("governance.min_quorum"),
            Some(ParameterCategory::Governance)
        );
        assert_eq!(
            ParameterCategory::from_id("trust.min_endorsements"),
            Some(ParameterCategory::Trust)
        );
        assert_eq!(
            ParameterCategory::from_id("ratelimit.isolated_limit"),
            Some(ParameterCategory::RateLimit)
        );
        assert_eq!(
            ParameterCategory::from_id("compute.task_timeout"),
            Some(ParameterCategory::Compute)
        );
        assert_eq!(ParameterCategory::from_id("unknown.param"), None);
    }

    #[test]
    fn test_constraints_are_valid() {
        let params = default_parameters();
        for param in params {
            // All parameters should have at least min or max constraints
            let has_constraints = param.constraints.min.is_some()
                || param.constraints.max.is_some()
                || param.constraints.allowed_values.is_some();
            assert!(
                has_constraints,
                "Parameter {} should have constraints",
                param.id
            );
        }
    }

    #[test]
    fn test_percentage_params_in_range() {
        let params = default_parameters();
        for param in params {
            if let ParameterValue::Percentage(pct) = param.value {
                assert!(
                    (0.0..=100.0).contains(&pct),
                    "Parameter {} has invalid percentage: {}",
                    param.id,
                    pct
                );
            }
        }
    }

    #[test]
    fn test_no_duplicate_parameter_ids() {
        use std::collections::HashSet;
        let params = default_parameters();
        let mut seen = HashSet::new();
        for param in params {
            assert!(
                seen.insert(param.id.clone()),
                "Duplicate parameter ID found: {}",
                param.id
            );
        }
    }

    #[test]
    fn test_governance_threshold_parameters_exist() {
        // Emergency operation thresholds (Issue #477)
        let threshold_params = [
            "governance.quorum.freeze",
            "governance.quorum.veto",
            "governance.quorum.force_close",
            "governance.quorum.rollback",
            "governance.quorum.treasury_budget",
            "governance.quorum.treasury_withdrawal",
            "governance.quorum.treasury_rule",
            "governance.approval.freeze",
            "governance.approval.veto",
            "governance.approval.force_close",
            "governance.approval.rollback",
            "governance.approval.treasury_budget",
            "governance.approval.treasury_withdrawal",
            "governance.approval.treasury_rule",
        ];

        for param_id in threshold_params {
            let param = get_default_parameter(param_id);
            assert!(
                param.is_some(),
                "Missing governance threshold parameter: {param_id}"
            );
        }
    }

    #[test]
    fn test_emergency_thresholds_higher_than_standard() {
        // Emergency operations should require higher quorum than standard (50%)
        let standard_quorum = match get_default_parameter("governance.min_quorum") {
            Some(p) => match p.value {
                ParameterValue::Percentage(v) => v,
                _ => panic!("min_quorum should be Percentage"),
            },
            None => panic!("min_quorum not found"),
        };

        let emergency_quorum_params = [
            "governance.quorum.freeze",
            "governance.quorum.veto",
            "governance.quorum.force_close",
            "governance.quorum.rollback",
        ];

        for param_id in emergency_quorum_params {
            let param = get_default_parameter(param_id).expect("param exists");
            if let ParameterValue::Percentage(quorum) = param.value {
                assert!(
                    quorum > standard_quorum,
                    "{param_id} ({quorum}) should be higher than standard quorum ({standard_quorum})"
                );
            } else {
                panic!("{param_id} should be Percentage type");
            }
        }
    }

    #[test]
    fn test_threshold_constraints_valid() {
        let threshold_params = [
            "governance.quorum.freeze",
            "governance.approval.freeze",
            "governance.quorum.rollback",
            "governance.approval.rollback",
        ];

        for param_id in threshold_params {
            let param = get_default_parameter(param_id).expect("param exists");

            // All threshold params should have min >= 50% for security
            if let Some(ParameterValue::Percentage(min)) = param.constraints.min {
                assert!(
                    min >= 50.0,
                    "{param_id} min constraint ({min}) should be >= 50%"
                );
            } else {
                panic!("{param_id} should have min percentage constraint");
            }

            // All should have max <= 100%
            if let Some(ParameterValue::Percentage(max)) = param.constraints.max {
                assert!(
                    max <= 100.0,
                    "{param_id} max constraint ({max}) should be <= 100%"
                );
            }
        }
    }

    #[test]
    fn test_treasury_threshold_values() {
        // Budget creation requires different thresholds than withdrawal
        let budget_quorum =
            get_default_parameter("governance.quorum.treasury_budget").expect("param exists");
        let withdrawal_quorum =
            get_default_parameter("governance.quorum.treasury_withdrawal").expect("param exists");

        if let (ParameterValue::Percentage(budget), ParameterValue::Percentage(withdrawal)) =
            (&budget_quorum.value, &withdrawal_quorum.value)
        {
            // Withdrawal should require at least as much quorum as budget creation
            assert!(
                withdrawal >= budget,
                "treasury_withdrawal quorum ({withdrawal}) should be >= treasury_budget quorum ({budget})"
            );
        }
    }
}
