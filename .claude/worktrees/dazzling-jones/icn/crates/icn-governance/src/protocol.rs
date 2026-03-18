//! Protocol Governance Types
//!
//! This module defines types for governable protocol parameters.
//! Protocol parameters allow ICN to democratically manage its own evolution
//! through the governance system.
//!
//! # Overview
//!
//! Protocol parameters are configuration values that affect network behavior:
//! - Network timeouts and limits
//! - Gossip protocol settings
//! - Ledger defaults (credit limits, transaction sizes)
//! - Governance thresholds (quorum, approval percentages)
//! - Trust graph parameters
//!
//! # Parameter ID Naming Convention
//!
//! Parameter IDs use dot-separated hierarchical naming:
//! `<category>.<subcategory>.<name>` or `<category>.<name>`
//!
//! Examples:
//! - `gossip.max_message_size` - Gossip category, max message size
//! - `governance.min_quorum` - Governance category, minimum quorum
//! - `ledger.default_credit_limit` - Ledger category, default credit limit
//!
//! The category can be extracted using `ProtocolParameter::category()`.
//!
//! # Scope Resolution
//!
//! Parameters can be defined at different scopes with cascading override:
//! - **Global**: Default for all nodes in the network
//! - **Federation**: Override for a specific federation
//! - **Cooperative**: Override for a specific cooperative
//!
//! When resolving a parameter value, the most specific scope wins:
//! `Cooperative > Federation > Global`
//!
//! # Example
//!
//! ```no_run
//! use icn_governance::protocol::{ProtocolParameter, ParameterValue, ParameterScope};
//!
//! let param = ProtocolParameter::new(
//!     "governance.min_quorum",
//!     "Minimum Quorum",
//!     "Minimum percentage of eligible voters required for a valid vote",
//!     ParameterValue::Percentage(50.0),  // 50% quorum
//! ).with_scope(ParameterScope::Global);
//! ```

use serde::{Deserialize, Serialize};

// Re-export all protocol parameter types from kernel-api.
// These types are defined in kernel-api to allow icn-core to depend on them
// without pulling in icn-governance.
pub use icn_kernel_api::protocol_params::{
    ParameterChange, ParameterConstraints, ParameterScope, ParameterValidationError,
    ParameterValue, PendingChangeId, PendingChangeStatus, PendingParameterChange,
    ProtocolParameter, ProtocolParameterStore, KNOWN_PARAMETER_CATEGORIES,
};

// ============================================================================
// ProtocolChangeProposal
// ============================================================================

/// Proposal payload for changing a protocol parameter
///
/// This is used as part of ProposalPayload::ProtocolChange to
/// request changes to protocol parameters through governance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolChangeProposal {
    /// ID of the parameter to change
    pub parameter_id: String,

    /// Proposed new value
    pub new_value: ParameterValue,

    /// Rationale for the change
    pub rationale: String,

    /// Optional: When the change should take effect (Unix timestamp)
    /// If None, change is effective immediately upon proposal approval.
    ///
    /// **NOTE**: Delayed execution is not yet implemented. This field must be `None`.
    /// Setting this field will cause proposal validation to fail.
    /// See: https://github.com/InterCooperative-Network/icn/issues/282
    pub effective_at: Option<u64>,

    /// Optional: Scope override for this change
    /// If None, uses the parameter's current scope
    pub scope: Option<ParameterScope>,
}

impl ProtocolChangeProposal {
    /// Create a new protocol change proposal
    pub fn new(
        parameter_id: impl Into<String>,
        new_value: ParameterValue,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            parameter_id: parameter_id.into(),
            new_value,
            rationale: rationale.into(),
            effective_at: None,
            scope: None,
        }
    }

    /// Set delayed effective date
    ///
    /// **NOTE**: Delayed execution is not yet implemented. Using this method
    /// will cause proposal validation to fail at creation time.
    /// See: https://github.com/InterCooperative-Network/icn/issues/282
    #[must_use]
    #[deprecated(
        since = "0.1.0",
        note = "Delayed execution not implemented. See issue #282."
    )]
    pub fn effective_at(mut self, timestamp: u64) -> Self {
        self.effective_at = Some(timestamp);
        self
    }

    /// Set scope for this change
    #[must_use]
    pub fn with_scope(mut self, scope: ParameterScope) -> Self {
        self.scope = Some(scope);
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_creation() {
        let param = ProtocolParameter::new(
            "gossip.max_message_size",
            "Max Message Size",
            "Maximum size of gossip messages in bytes",
            ParameterValue::Bytes(1_048_576),
        );

        assert_eq!(param.id, "gossip.max_message_size");
        assert_eq!(param.category(), "gossip");
        assert!(matches!(param.scope, ParameterScope::Global));
    }

    #[test]
    fn test_parameter_validation_type_mismatch() {
        let param = ProtocolParameter::new(
            "test.int",
            "Test",
            "Test parameter",
            ParameterValue::Integer(100),
        );

        let result = param.validate(&ParameterValue::String("invalid".into()));
        assert!(matches!(
            result,
            Err(ParameterValidationError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn test_parameter_validation_min_max() {
        let param = ProtocolParameter::new(
            "test.bounded",
            "Bounded",
            "Bounded parameter",
            ParameterValue::Integer(50),
        )
        .with_min(ParameterValue::Integer(10))
        .with_max(ParameterValue::Integer(100));

        // Valid value
        assert!(param.validate(&ParameterValue::Integer(50)).is_ok());

        // Below minimum
        let result = param.validate(&ParameterValue::Integer(5));
        assert!(matches!(
            result,
            Err(ParameterValidationError::BelowMinimum { .. })
        ));

        // Above maximum
        let result = param.validate(&ParameterValue::Integer(150));
        assert!(matches!(
            result,
            Err(ParameterValidationError::AboveMaximum { .. })
        ));
    }

    #[test]
    fn test_parameter_validation_nan_infinity() {
        let param = ProtocolParameter::new(
            "test.float",
            "Float Test",
            "Test parameter",
            ParameterValue::Float(1.0),
        );

        // Valid finite values should pass
        assert!(param.validate(&ParameterValue::Float(0.0)).is_ok());
        assert!(param.validate(&ParameterValue::Float(-100.5)).is_ok());

        // NaN should fail
        let result = param.validate(&ParameterValue::Float(f64::NAN));
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidFloatValue { .. })
        ));

        // Infinity should fail
        let result = param.validate(&ParameterValue::Float(f64::INFINITY));
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidFloatValue { .. })
        ));

        // Negative infinity should fail
        let result = param.validate(&ParameterValue::Float(f64::NEG_INFINITY));
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidFloatValue { .. })
        ));

        // Test percentage type as well
        let pct_param = ProtocolParameter::new(
            "test.pct",
            "Percentage Test",
            "Test parameter",
            ParameterValue::Percentage(50.0),
        );

        let result = pct_param.validate(&ParameterValue::Percentage(f64::NAN));
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidFloatValue { .. })
        ));
    }

    #[test]
    fn test_parameter_id_validation() {
        // Valid IDs
        assert!(ProtocolParameter::validate_id("gossip.fanout").is_ok());
        assert!(ProtocolParameter::validate_id("network.peer.timeout").is_ok());
        assert!(ProtocolParameter::validate_id("a.b").is_ok());
        assert!(ProtocolParameter::validate_id("a.b.c.d.e").is_ok());

        // Invalid: no dot separator
        let result = ProtocolParameter::validate_id("nodot");
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidParameterId { .. })
        ));

        // Invalid: empty string
        let result = ProtocolParameter::validate_id("");
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidParameterId { .. })
        ));

        // Invalid: empty category
        let result = ProtocolParameter::validate_id(".name");
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidParameterId { .. })
        ));

        // Invalid: empty name
        let result = ProtocolParameter::validate_id("category.");
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidParameterId { .. })
        ));

        // Invalid: empty subcategory
        let result = ProtocolParameter::validate_id("a..c");
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidParameterId { .. })
        ));
    }

    #[test]
    fn test_try_new_validates_id() {
        // Valid ID should work
        let result = ProtocolParameter::try_new(
            "gossip.fanout",
            "Fanout",
            "Gossip fanout",
            ParameterValue::Integer(8),
        );
        assert!(result.is_ok());

        // Invalid ID should fail
        let result = ProtocolParameter::try_new(
            "invalid",
            "Invalid",
            "No dot separator",
            ParameterValue::Integer(1),
        );
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidParameterId { .. })
        ));
    }

    #[test]
    fn test_percentage_range_validation() {
        let param = ProtocolParameter::new(
            "test.pct",
            "Percentage Test",
            "Test parameter",
            ParameterValue::Percentage(50.0),
        );

        // Valid percentages
        assert!(param.validate(&ParameterValue::Percentage(0.0)).is_ok());
        assert!(param.validate(&ParameterValue::Percentage(50.0)).is_ok());
        assert!(param.validate(&ParameterValue::Percentage(100.0)).is_ok());

        // Out of range: negative
        let result = param.validate(&ParameterValue::Percentage(-1.0));
        assert!(matches!(
            result,
            Err(ParameterValidationError::PercentageOutOfRange { .. })
        ));

        // Out of range: above 100
        let result = param.validate(&ParameterValue::Percentage(101.0));
        assert!(matches!(
            result,
            Err(ParameterValidationError::PercentageOutOfRange { .. })
        ));
    }

    #[test]
    fn test_parameter_value_display() {
        assert_eq!(ParameterValue::Integer(42).to_string(), "42");
        assert_eq!(ParameterValue::Boolean(true).to_string(), "true");
        assert_eq!(ParameterValue::Duration(3600).to_string(), "1h");
        assert_eq!(ParameterValue::Duration(90).to_string(), "1m");
        assert_eq!(ParameterValue::Duration(45).to_string(), "45s");
        assert_eq!(ParameterValue::Bytes(1_048_576).to_string(), "1MB");
        assert_eq!(ParameterValue::Bytes(1024).to_string(), "1KB");
        assert_eq!(ParameterValue::Percentage(50.0).to_string(), "50.0%");
    }

    #[test]
    fn test_parameter_value_comparison() {
        assert!(ParameterValue::Integer(10) < ParameterValue::Integer(20));
        assert!(ParameterValue::Float(1.5) > ParameterValue::Float(1.0));
        assert!(ParameterValue::Duration(60) == ParameterValue::Duration(60));
    }

    #[test]
    fn test_parameter_value_epsilon_comparison() {
        // Test that nearly-equal floats are treated as equal
        let a = ParameterValue::Float(50.0);
        let b = ParameterValue::Float(50.0 + 1e-12); // Very small difference
        assert!(a.approximately_eq(&b));
        assert!(a <= b); // Should be equal, so <= is true
        assert!(a >= b); // Should be equal, so >= is true

        // Test that clearly different floats are not equal
        let c = ParameterValue::Float(50.0);
        let d = ParameterValue::Float(51.0);
        assert!(!c.approximately_eq(&d));
        assert!(c < d);

        // Test percentages with epsilon
        let p1 = ParameterValue::Percentage(66.666666666);
        let p2 = ParameterValue::Percentage(66.666666667);
        assert!(p1.approximately_eq(&p2));

        // Test that integer comparison is exact (no epsilon)
        let i1 = ParameterValue::Integer(100);
        let i2 = ParameterValue::Integer(100);
        let i3 = ParameterValue::Integer(101);
        assert!(i1.approximately_eq(&i2));
        assert!(!i1.approximately_eq(&i3));
    }

    #[test]
    fn test_parameter_validation_with_float_epsilon() {
        // Create a parameter with a float max of 50.0
        let param = ProtocolParameter::new(
            "test.float_bounded",
            "Float Test",
            "Test float validation",
            ParameterValue::Float(25.0),
        )
        .with_max(ParameterValue::Float(50.0));

        // Value exactly at max should pass
        assert!(param.validate(&ParameterValue::Float(50.0)).is_ok());

        // Value very slightly above max (within epsilon) should pass
        assert!(param.validate(&ParameterValue::Float(50.0 + 1e-12)).is_ok());

        // Value clearly above max should fail
        assert!(param.validate(&ParameterValue::Float(50.1)).is_err());
    }

    #[test]
    fn test_parameter_value_json_roundtrip_precision() {
        // Test that float values remain comparable after JSON serialization
        // This is critical because JSON can introduce floating point precision issues

        // Test typical percentage values
        let original = ParameterValue::Percentage(66.67);
        let json = serde_json::to_string(&original).unwrap();
        let roundtripped: ParameterValue = serde_json::from_str(&json).unwrap();
        assert!(
            original.approximately_eq(&roundtripped),
            "Percentage 66.67 should be equal after JSON roundtrip"
        );

        // Test edge case: supermajority threshold (2/3 as percentage)
        let supermajority = ParameterValue::Percentage(2.0 / 3.0 * 100.0);
        let json = serde_json::to_string(&supermajority).unwrap();
        let roundtripped: ParameterValue = serde_json::from_str(&json).unwrap();
        assert!(
            supermajority.approximately_eq(&roundtripped),
            "Supermajority threshold should be equal after JSON roundtrip"
        );

        // Test Float type with a precise value
        let float_val = ParameterValue::Float(std::f64::consts::PI);
        let json = serde_json::to_string(&float_val).unwrap();
        let roundtripped: ParameterValue = serde_json::from_str(&json).unwrap();
        assert!(
            float_val.approximately_eq(&roundtripped),
            "Float should be equal after JSON roundtrip"
        );

        // Test that validation still works after roundtrip
        let param = ProtocolParameter::new(
            "test.pct",
            "Percentage Test",
            "Test parameter",
            ParameterValue::Percentage(50.0),
        )
        .with_min(ParameterValue::Percentage(0.0))
        .with_max(ParameterValue::Percentage(100.0));

        // Serialize parameter, deserialize, and validate a value
        let param_json = serde_json::to_string(&param).unwrap();
        let param_roundtripped: ProtocolParameter = serde_json::from_str(&param_json).unwrap();

        // Value at 66.67 should validate against both original and roundtripped constraints
        let test_val = ParameterValue::Percentage(66.67);
        assert!(param.validate(&test_val).is_ok());
        assert!(param_roundtripped.validate(&test_val).is_ok());

        // Constraint comparison after roundtrip should work
        if let Some(max) = &param.constraints.max {
            if let Some(max_rt) = &param_roundtripped.constraints.max {
                assert!(
                    max.approximately_eq(max_rt),
                    "Max constraint should be equal after JSON roundtrip"
                );
            }
        }
    }

    #[test]
    fn test_scope_specificity() {
        let global = ParameterScope::Global;
        let fed = ParameterScope::Federation {
            id: "test-fed".to_string(),
        };
        let coop = ParameterScope::Cooperative {
            id: "test-coop".to_string(),
        };

        assert!(fed.is_more_specific_than(&global));
        assert!(coop.is_more_specific_than(&global));
        assert!(coop.is_more_specific_than(&fed));
        assert!(!global.is_more_specific_than(&fed));
        assert!(!fed.is_more_specific_than(&coop));
    }

    #[test]
    fn test_scope_display() {
        assert_eq!(ParameterScope::Global.to_string(), "global");

        let fed = ParameterScope::Federation {
            id: "midwest-fed".to_string(),
        };
        assert_eq!(fed.to_string(), "federation:midwest-fed");

        let coop = ParameterScope::Cooperative {
            id: "food-coop".to_string(),
        };
        assert_eq!(coop.to_string(), "cooperative:food-coop");
    }

    #[test]
    fn test_protocol_change_proposal() {
        let proposal = ProtocolChangeProposal::new(
            "governance.min_quorum",
            ParameterValue::Percentage(0.6),
            "Increase quorum to ensure broader participation",
        );

        assert_eq!(proposal.parameter_id, "governance.min_quorum");
        assert!(proposal.effective_at.is_none());
        assert!(proposal.scope.is_none());
    }

    #[test]
    fn test_parameter_serialization() {
        let param = ProtocolParameter::new(
            "test.param",
            "Test Parameter",
            "A test parameter",
            ParameterValue::Integer(42),
        );

        let json = serde_json::to_string(&param).unwrap();
        let parsed: ProtocolParameter = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, param.id);
        assert_eq!(parsed.value, param.value);
    }

    #[test]
    fn test_parameter_value_accessors() {
        assert_eq!(ParameterValue::Integer(42).as_integer(), Some(42));
        assert_eq!(ParameterValue::Float(1.5).as_float(), Some(1.5));
        assert_eq!(
            ParameterValue::String("hello".into()).as_string(),
            Some("hello")
        );
        assert_eq!(ParameterValue::Boolean(true).as_bool(), Some(true));
        assert_eq!(ParameterValue::Duration(60).as_duration_secs(), Some(60));
        assert_eq!(ParameterValue::Bytes(1024).as_bytes(), Some(1024));

        // Cross-type accessors should return None
        assert_eq!(ParameterValue::Integer(42).as_string(), None);
        assert_eq!(ParameterValue::String("hello".into()).as_integer(), None);
    }

    #[test]
    fn test_known_category_validation() {
        // Known categories should pass
        assert!(ProtocolParameter::validate_known_category("gossip.fanout").is_ok());
        assert!(ProtocolParameter::validate_known_category("network.timeout").is_ok());
        assert!(ProtocolParameter::validate_known_category("ledger.credit_limit").is_ok());
        assert!(ProtocolParameter::validate_known_category("governance.quorum").is_ok());
        assert!(ProtocolParameter::validate_known_category("trust.decay").is_ok());
        assert!(ProtocolParameter::validate_known_category("ratelimit.max").is_ok());
        assert!(ProtocolParameter::validate_known_category("compute.timeout").is_ok());

        // Subcategories should work
        assert!(ProtocolParameter::validate_known_category("gossip.anti_entropy.interval").is_ok());

        // Unknown categories should fail
        let result = ProtocolParameter::validate_known_category("custom.parameter");
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidParameterId { .. })
        ));

        // Invalid format should still fail
        let result = ProtocolParameter::validate_known_category("nodot");
        assert!(matches!(
            result,
            Err(ParameterValidationError::InvalidParameterId { .. })
        ));
    }

    // ============================================================================
    // PendingParameterChange tests
    // ============================================================================

    #[test]
    fn test_pending_change_creation() {
        let change = PendingParameterChange::new(
            "pending:governance.quorum:12345:abc",
            "governance.quorum",
            ParameterValue::Percentage(60.0),
            1700000000,
            ParameterScope::Global,
            "prop-123",
            "Increase quorum for better participation",
        );

        assert_eq!(change.parameter_id, "governance.quorum");
        assert_eq!(change.effective_at, 1700000000);
        assert_eq!(change.status, PendingChangeStatus::Pending);
        assert!(change.applied_at.is_none());
        assert!(change.superseded_by.is_none());
        assert!(change.cancellation_reason.is_none());
    }

    #[test]
    fn test_pending_change_generate_id() {
        let id1 = PendingParameterChange::generate_id("governance.quorum");
        let id2 = PendingParameterChange::generate_id("governance.quorum");

        // IDs should start with expected prefix
        assert!(id1.starts_with("pending:governance.quorum:"));
        assert!(id2.starts_with("pending:governance.quorum:"));

        // IDs should be unique due to atomic counter
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_pending_change_is_due() {
        let change = PendingParameterChange::new(
            "test-id",
            "governance.quorum",
            ParameterValue::Percentage(60.0),
            1700000000,
            ParameterScope::Global,
            "prop-123",
            "Test",
        );

        // Not yet due
        assert!(!change.is_due(1699999999));

        // Exactly due
        assert!(change.is_due(1700000000));

        // Past due
        assert!(change.is_due(1700000001));
    }

    #[test]
    fn test_pending_change_mark_applied() {
        let mut change = PendingParameterChange::new(
            "test-id",
            "governance.quorum",
            ParameterValue::Percentage(60.0),
            1700000000,
            ParameterScope::Global,
            "prop-123",
            "Test",
        );

        change.mark_applied();

        assert_eq!(change.status, PendingChangeStatus::Applied);
        assert!(change.applied_at.is_some());
    }

    #[test]
    fn test_pending_change_mark_cancelled() {
        let mut change = PendingParameterChange::new(
            "test-id",
            "governance.quorum",
            ParameterValue::Percentage(60.0),
            1700000000,
            ParameterScope::Global,
            "prop-123",
            "Test",
        );

        change.mark_cancelled("Governance override");

        assert_eq!(change.status, PendingChangeStatus::Cancelled);
        assert_eq!(
            change.cancellation_reason,
            Some("Governance override".to_string())
        );
    }

    #[test]
    fn test_pending_change_mark_superseded() {
        let mut change = PendingParameterChange::new(
            "test-id",
            "governance.quorum",
            ParameterValue::Percentage(60.0),
            1700000000,
            ParameterScope::Global,
            "prop-123",
            "Test",
        );

        change.mark_superseded("newer-change-id");

        assert_eq!(change.status, PendingChangeStatus::Superseded);
        assert_eq!(change.superseded_by, Some("newer-change-id".to_string()));
    }

    #[test]
    fn test_pending_change_time_until_effective() {
        let change = PendingParameterChange::new(
            "test-id",
            "governance.quorum",
            ParameterValue::Percentage(60.0),
            1700000000,
            ParameterScope::Global,
            "prop-123",
            "Test",
        );

        // Before effective time
        assert_eq!(change.time_until_effective(1699999000), Some(1000));

        // Exactly at effective time
        assert_eq!(change.time_until_effective(1700000000), None);

        // After effective time
        assert_eq!(change.time_until_effective(1700000100), None);
    }

    #[test]
    fn test_pending_change_status_display() {
        assert_eq!(PendingChangeStatus::Pending.to_string(), "pending");
        assert_eq!(PendingChangeStatus::Applied.to_string(), "applied");
        assert_eq!(PendingChangeStatus::Cancelled.to_string(), "cancelled");
        assert_eq!(PendingChangeStatus::Superseded.to_string(), "superseded");
    }

    #[test]
    fn test_pending_change_serialization() {
        let change = PendingParameterChange::new(
            "test-id",
            "governance.quorum",
            ParameterValue::Percentage(60.0),
            1700000000,
            ParameterScope::Global,
            "prop-123",
            "Test rationale",
        );

        let json = serde_json::to_string(&change).unwrap();
        let parsed: PendingParameterChange = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, change.id);
        assert_eq!(parsed.parameter_id, change.parameter_id);
        assert_eq!(parsed.effective_at, change.effective_at);
        assert_eq!(parsed.status, change.status);
    }

    #[test]
    fn test_pending_change_cancelled_not_due() {
        let mut change = PendingParameterChange::new(
            "test-id",
            "governance.quorum",
            ParameterValue::Percentage(60.0),
            1700000000,
            ParameterScope::Global,
            "prop-123",
            "Test",
        );

        change.mark_cancelled("Cancelled by governance");

        // Even though time has passed, cancelled changes are not due
        assert!(!change.is_due(1700000001));
    }
}
