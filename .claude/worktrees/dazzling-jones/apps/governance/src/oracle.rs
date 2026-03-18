//! Governance Policy Oracle
//!
//! Implements `PolicyOracle` for governance-based authorization decisions.
//!
//! # The Meaning Firewall
//!
//! This oracle converts governance decisions into kernel-enforceable constraints:
//! - Protocol parameters → rate limits, quotas, timeouts
//! - Governance rules → allow/deny decisions
//!
//! The kernel NEVER sees the governance semantics behind these constraints.

use icn_governance::ProtocolParameterStore;
use icn_kernel_api::authz::{
    ConstraintSet, ConstraintValue, Domain, PolicyContext, PolicyDecision, PolicyOracle,
    PolicyRequest, RateLimit,
};
use std::sync::Arc;
use tracing::debug;

/// Governance policy oracle
///
/// Converts governance parameters and rules into kernel-enforceable constraints.
pub struct GovernancePolicyOracle {
    parameter_store: Arc<dyn ProtocolParameterStore>,
}

impl GovernancePolicyOracle {
    /// Create a new governance policy oracle
    pub fn new(parameter_store: Arc<dyn ProtocolParameterStore>) -> Self {
        Self { parameter_store }
    }

    /// Convert governance parameters to constraints
    ///
    /// This is the MEANING FIREWALL BOUNDARY.
    /// Governance semantics (parameters, rules) are converted to generic constraints.
    fn params_to_constraints(&self, context: &PolicyContext) -> ConstraintSet {
        let mut constraints = ConstraintSet::default();

        // Look up rate limit from governance parameters
        if let Some(rate_limit_param) = self.get_param("rate_limit.default") {
            if let Ok(limit) = rate_limit_param.parse::<u32>() {
                constraints = constraints.with_rate_limit(RateLimit::new(limit, 1));
            }
        }

        // Check for action-specific rate limits
        if let Some(action) = context.metadata.get("action") {
            let param_key = format!("rate_limit.{}", action);
            if let Some(rate_limit_param) = self.get_param(&param_key) {
                if let Ok(limit) = rate_limit_param.parse::<u32>() {
                    constraints = constraints.with_rate_limit(RateLimit::new(limit, 1));
                }
            }
        }

        // Look up resource quotas
        if let Some(quota_param) = self.get_param("resource_quota.default") {
            if let Ok(quota) = quota_param.parse::<u64>() {
                constraints =
                    constraints.with_custom("resource_quota", ConstraintValue::Int(quota as i64));
            }
        }

        constraints
    }

    /// Get a parameter value from the store (as string for parsing)
    fn get_param(&self, key: &str) -> Option<String> {
        match self.parameter_store.get(key) {
            Ok(Some(param)) => Some(param.value.to_string()),
            _ => None,
        }
    }
}

impl PolicyOracle for GovernancePolicyOracle {
    fn domain(&self) -> Domain {
        Domain::new("governance")
    }

    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Only handle governance domain requests
        if *request.domain() != self.domain() {
            debug!(
                domain = %request.domain(),
                "GovernancePolicyOracle abstaining from non-governance domain"
            );
            return PolicyDecision::allow();
        }

        debug!(
            actor = %request.actor(),
            resource = ?request.context.resource,
            action = ?request.action(),
            "GovernancePolicyOracle evaluating request"
        );

        // Convert governance parameters to constraints
        // This crosses the meaning firewall - governance rules become generic limits
        let constraints = self.params_to_constraints(&request.context);

        PolicyDecision::Allow {
            constraints,
            policy_domain: None,
            evaluated_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_governance::{
        ParameterConstraints, ParameterScope, ParameterValue, ProtocolParameter, SledParameterStore,
    };
    use icn_kernel_api::authz::ActionKind;
    use icn_kernel_api::types::Did;

    fn create_test_store() -> Arc<dyn ProtocolParameterStore> {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = sled::open(temp_dir.path()).unwrap();
        Arc::new(SledParameterStore::new(Arc::new(db)).unwrap())
    }

    fn create_test_param(id: &str, value: ParameterValue) -> ProtocolParameter {
        ProtocolParameter {
            id: id.to_string(),
            name: id.to_string(),
            description: "Test parameter".to_string(),
            value,
            constraints: ParameterConstraints::default(),
            scope: ParameterScope::Global,
            updated_at: 0,
            updated_by: None,
            version: 0,
        }
    }

    #[test]
    fn test_oracle_domain() {
        let store = create_test_store();
        let oracle = GovernancePolicyOracle::new(store);
        assert_eq!(oracle.domain().as_str(), "governance");
    }

    #[test]
    fn test_oracle_allows_by_default() {
        let store = create_test_store();
        let oracle = GovernancePolicyOracle::new(store);

        let request = PolicyRequest::new(
            Did::from("did:icn:test123".to_string()),
            ActionKind::Write,
            Domain::new("governance"),
        );

        let decision = oracle.evaluate(&request);
        assert!(matches!(decision, PolicyDecision::Allow { .. }));
    }

    #[test]
    fn test_oracle_applies_rate_limit_from_params() {
        let store = create_test_store();

        // Set a default rate limit parameter
        let param = create_test_param("rate_limit.default", ParameterValue::Integer(100));
        store.set(param, None, None).unwrap();

        let oracle = GovernancePolicyOracle::new(store);

        let request = PolicyRequest::new(
            Did::from("did:icn:test123".to_string()),
            ActionKind::Write,
            Domain::new("governance"),
        );

        let decision = oracle.evaluate(&request);
        if let PolicyDecision::Allow { constraints, .. } = decision {
            assert!(constraints.rate_limit.is_some());
            assert_eq!(constraints.rate_limit.unwrap().messages_per_second, 100);
        } else {
            panic!("Expected Allow decision");
        }
    }

    #[test]
    fn test_oracle_abstains_from_other_domains() {
        let store = create_test_store();
        let oracle = GovernancePolicyOracle::new(store);

        let request = PolicyRequest::new(
            Did::from("did:icn:test123".to_string()),
            ActionKind::Read,
            Domain::new("trust"),
        );

        let decision = oracle.evaluate(&request);
        // Should allow with empty constraints (abstain)
        if let PolicyDecision::Allow { constraints, .. } = decision {
            assert!(constraints.rate_limit.is_none());
        } else {
            panic!("Expected Allow decision for non-governance domain");
        }
    }
}
