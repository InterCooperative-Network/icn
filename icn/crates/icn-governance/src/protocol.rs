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
//! ```ignore
//! use icn_governance::protocol::{ProtocolParameter, ParameterValue, ParameterScope};
//!
//! let param = ProtocolParameter::new(
//!     "governance.min_quorum",
//!     "Minimum Quorum",
//!     "Minimum percentage of eligible voters required for a valid vote",
//!     ParameterValue::Percentage(50.0),  // 50% quorum
//! ).with_scope(ParameterScope::Global);
//! ```

use icn_entity::EntityId;
use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================================
// ProtocolParameter
// ============================================================================

/// A governable protocol parameter
///
/// Protocol parameters are configuration values that can be changed through
/// the governance process. Each parameter has:
/// - A unique identifier (e.g., "gossip.max_message_size")
/// - Constraints on valid values
/// - A scope determining where it applies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolParameter {
    /// Unique parameter identifier (e.g., "gossip.max_message_size")
    ///
    /// Format: `<category>.<name>` or `<category>.<subcategory>.<name>`
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Detailed description of what this parameter controls
    pub description: String,

    /// Current value
    pub value: ParameterValue,

    /// Constraints on valid values
    pub constraints: ParameterConstraints,

    /// Scope where this parameter applies
    pub scope: ParameterScope,

    /// When the parameter was last updated (Unix timestamp)
    pub updated_at: u64,

    /// Proposal ID that last changed this parameter (if any)
    pub updated_by: Option<String>,
}

impl ProtocolParameter {
    /// Create a new protocol parameter
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        value: ParameterValue,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            value,
            constraints: ParameterConstraints::default(),
            scope: ParameterScope::Global,
            updated_at: icn_time::current_timestamp_secs(),
            updated_by: None,
        }
    }

    /// Set the scope for this parameter
    #[must_use]
    pub fn with_scope(mut self, scope: ParameterScope) -> Self {
        self.scope = scope;
        self
    }

    /// Set constraints for this parameter
    #[must_use]
    pub fn with_constraints(mut self, constraints: ParameterConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Set minimum value constraint
    #[must_use]
    pub fn with_min(mut self, min: ParameterValue) -> Self {
        self.constraints.min = Some(min);
        self
    }

    /// Set maximum value constraint
    #[must_use]
    pub fn with_max(mut self, max: ParameterValue) -> Self {
        self.constraints.max = Some(max);
        self
    }

    /// Mark parameter as requiring restart to take effect
    #[must_use]
    pub fn requires_restart(mut self) -> Self {
        self.constraints.requires_restart = true;
        self
    }

    /// Validate a new value against constraints
    pub fn validate(&self, new_value: &ParameterValue) -> Result<(), ParameterValidationError> {
        // Check type compatibility
        if std::mem::discriminant(&self.value) != std::mem::discriminant(new_value) {
            return Err(ParameterValidationError::TypeMismatch {
                expected: self.value.type_name(),
                got: new_value.type_name(),
            });
        }

        // Check for NaN/Infinity in floating-point values
        match new_value {
            ParameterValue::Float(f) if !f.is_finite() => {
                return Err(ParameterValidationError::InvalidFloatValue {
                    value: *f,
                    reason: "value must be finite (not NaN or Infinity)",
                });
            }
            ParameterValue::Percentage(f) if !f.is_finite() => {
                return Err(ParameterValidationError::InvalidFloatValue {
                    value: *f,
                    reason: "percentage must be finite (not NaN or Infinity)",
                });
            }
            _ => {}
        }

        // Check minimum constraint
        if let Some(ref min) = self.constraints.min {
            if new_value < min {
                return Err(ParameterValidationError::BelowMinimum {
                    value: new_value.clone(),
                    min: min.clone(),
                });
            }
        }

        // Check maximum constraint
        if let Some(ref max) = self.constraints.max {
            if new_value > max {
                return Err(ParameterValidationError::AboveMaximum {
                    value: new_value.clone(),
                    max: max.clone(),
                });
            }
        }

        // Check allowed values
        if let Some(ref allowed) = self.constraints.allowed_values {
            if !allowed.contains(new_value) {
                return Err(ParameterValidationError::NotAllowed {
                    value: new_value.clone(),
                    allowed: allowed.clone(),
                });
            }
        }

        Ok(())
    }

    /// Get the parameter category (first part of ID before the dot)
    ///
    /// Parameter IDs follow a dot-separated naming convention:
    /// `<category>.<subcategory>.<name>` (e.g., "gossip.max_message_size")
    ///
    /// The category is the first component before the first dot.
    /// If there is no dot, the entire ID is returned as the category.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let param = ProtocolParameter::new("gossip.max_message_size", ...);
    /// assert_eq!(param.category(), "gossip");
    ///
    /// let param = ProtocolParameter::new("simple_param", ...);
    /// assert_eq!(param.category(), "simple_param");
    /// ```
    pub fn category(&self) -> &str {
        self.id.split('.').next().unwrap_or(&self.id)
    }
}

// ============================================================================
// ParameterValue
// ============================================================================

/// Value types for protocol parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ParameterValue {
    /// Integer value (signed 64-bit)
    Integer(i64),

    /// Floating-point value
    Float(f64),

    /// String value
    String(String),

    /// Boolean value
    Boolean(bool),

    /// Duration in seconds
    Duration(u64),

    /// Size in bytes
    Bytes(u64),

    /// Percentage (0.0 to 100.0)
    Percentage(f64),
}

impl ParameterValue {
    /// Get the type name for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            ParameterValue::Integer(_) => "integer",
            ParameterValue::Float(_) => "float",
            ParameterValue::String(_) => "string",
            ParameterValue::Boolean(_) => "boolean",
            ParameterValue::Duration(_) => "duration",
            ParameterValue::Bytes(_) => "bytes",
            ParameterValue::Percentage(_) => "percentage",
        }
    }

    /// Get as integer, if applicable
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            ParameterValue::Integer(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as float, if applicable
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ParameterValue::Float(v) => Some(*v),
            ParameterValue::Percentage(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as string, if applicable
    pub fn as_string(&self) -> Option<&str> {
        match self {
            ParameterValue::String(v) => Some(v),
            _ => None,
        }
    }

    /// Get as boolean, if applicable
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ParameterValue::Boolean(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as duration in seconds, if applicable
    pub fn as_duration_secs(&self) -> Option<u64> {
        match self {
            ParameterValue::Duration(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as bytes, if applicable
    pub fn as_bytes(&self) -> Option<u64> {
        match self {
            ParameterValue::Bytes(v) => Some(*v),
            _ => None,
        }
    }
}

impl PartialOrd for ParameterValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (ParameterValue::Integer(a), ParameterValue::Integer(b)) => a.partial_cmp(b),
            (ParameterValue::Float(a), ParameterValue::Float(b)) => a.partial_cmp(b),
            (ParameterValue::Duration(a), ParameterValue::Duration(b)) => a.partial_cmp(b),
            (ParameterValue::Bytes(a), ParameterValue::Bytes(b)) => a.partial_cmp(b),
            (ParameterValue::Percentage(a), ParameterValue::Percentage(b)) => a.partial_cmp(b),
            (ParameterValue::String(a), ParameterValue::String(b)) => a.partial_cmp(b),
            // Boolean and cross-type comparisons are not ordered
            _ => None,
        }
    }
}

impl fmt::Display for ParameterValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParameterValue::Integer(v) => write!(f, "{v}"),
            ParameterValue::Float(v) => write!(f, "{v}"),
            ParameterValue::String(v) => write!(f, "\"{v}\""),
            ParameterValue::Boolean(v) => write!(f, "{v}"),
            ParameterValue::Duration(v) => {
                if *v >= 86400 {
                    write!(f, "{}d", v / 86400)
                } else if *v >= 3600 {
                    write!(f, "{}h", v / 3600)
                } else if *v >= 60 {
                    write!(f, "{}m", v / 60)
                } else {
                    write!(f, "{v}s")
                }
            }
            ParameterValue::Bytes(v) => {
                if *v >= 1_073_741_824 {
                    write!(f, "{}GB", v / 1_073_741_824)
                } else if *v >= 1_048_576 {
                    write!(f, "{}MB", v / 1_048_576)
                } else if *v >= 1024 {
                    write!(f, "{}KB", v / 1024)
                } else {
                    write!(f, "{v}B")
                }
            }
            ParameterValue::Percentage(v) => write!(f, "{v:.1}%"),
        }
    }
}

// ============================================================================
// ParameterConstraints
// ============================================================================

/// Constraints on parameter values
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParameterConstraints {
    /// Minimum allowed value (inclusive)
    pub min: Option<ParameterValue>,

    /// Maximum allowed value (inclusive)
    pub max: Option<ParameterValue>,

    /// If set, value must be one of these options
    pub allowed_values: Option<Vec<ParameterValue>>,

    /// Whether changing this parameter requires a daemon restart
    pub requires_restart: bool,

    /// Whether this parameter can be overridden at lower scopes
    pub allow_override: bool,
}

impl ParameterConstraints {
    /// Create constraints with min and max bounds
    pub fn bounded(min: ParameterValue, max: ParameterValue) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            ..Default::default()
        }
    }

    /// Create constraints with specific allowed values
    pub fn enumerated(values: Vec<ParameterValue>) -> Self {
        Self {
            allowed_values: Some(values),
            ..Default::default()
        }
    }
}

// ============================================================================
// ParameterScope
// ============================================================================

/// Scope at which a parameter applies
///
/// Parameters cascade from Global -> Federation -> Cooperative,
/// with more specific scopes overriding less specific ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParameterScope {
    /// Applies to all nodes in the network
    Global,

    /// Applies to a specific federation
    Federation {
        /// Federation entity ID
        id: EntityId,
    },

    /// Applies to a specific cooperative
    Cooperative {
        /// Cooperative entity ID
        id: EntityId,
    },
}

impl ParameterScope {
    /// Check if this scope is more specific than another
    pub fn is_more_specific_than(&self, other: &ParameterScope) -> bool {
        match (self, other) {
            // Cooperative is most specific
            (ParameterScope::Cooperative { .. }, ParameterScope::Federation { .. }) => true,
            (ParameterScope::Cooperative { .. }, ParameterScope::Global) => true,
            // Federation is more specific than Global
            (ParameterScope::Federation { .. }, ParameterScope::Global) => true,
            // Same scope or less specific
            _ => false,
        }
    }

    /// Get the entity ID if this is an entity-scoped parameter
    pub fn entity_id(&self) -> Option<&EntityId> {
        match self {
            ParameterScope::Global => None,
            ParameterScope::Federation { id } => Some(id),
            ParameterScope::Cooperative { id } => Some(id),
        }
    }
}

impl fmt::Display for ParameterScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParameterScope::Global => write!(f, "global"),
            ParameterScope::Federation { id } => write!(f, "federation:{}", id.identifier()),
            ParameterScope::Cooperative { id } => write!(f, "cooperative:{}", id.identifier()),
        }
    }
}

// ============================================================================
// ParameterChange (for history tracking)
// ============================================================================

/// Record of a parameter change for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterChange {
    /// Parameter ID that was changed
    pub parameter_id: String,

    /// Previous value
    pub old_value: ParameterValue,

    /// New value
    pub new_value: ParameterValue,

    /// When the change occurred (Unix timestamp)
    pub changed_at: u64,

    /// Proposal ID that authorized this change (if any)
    pub proposal_id: Option<String>,

    /// DID of the actor who made the change
    pub changed_by: Option<String>,
}

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
    /// If None, change is effective immediately upon proposal approval
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
    #[must_use]
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
// ParameterValidationError
// ============================================================================

/// Errors that can occur when validating parameter values
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParameterValidationError {
    /// Value type doesn't match parameter type
    #[error("Type mismatch: expected {expected}, got {got}")]
    TypeMismatch {
        /// Expected type
        expected: &'static str,
        /// Actual type
        got: &'static str,
    },

    /// Value is below minimum
    #[error("Value {value} is below minimum {min}")]
    BelowMinimum {
        /// The value that was provided
        value: ParameterValue,
        /// The minimum allowed value
        min: ParameterValue,
    },

    /// Value is above maximum
    #[error("Value {value} is above maximum {max}")]
    AboveMaximum {
        /// The value that was provided
        value: ParameterValue,
        /// The maximum allowed value
        max: ParameterValue,
    },

    /// Value is not in allowed set
    #[error("Value {value} is not in allowed set: {allowed:?}")]
    NotAllowed {
        /// The value that was provided
        value: ParameterValue,
        /// The allowed values
        allowed: Vec<ParameterValue>,
    },

    /// Invalid floating-point value (NaN or Infinity)
    #[error("Invalid float value {value}: {reason}")]
    InvalidFloatValue {
        /// The invalid value
        value: f64,
        /// Reason for rejection
        reason: &'static str,
    },

    /// Parameter not found
    #[error("Parameter not found: {0}")]
    NotFound(String),

    /// Parameter cannot be modified at this scope
    #[error("Parameter {parameter_id} cannot be overridden at scope {scope}")]
    ScopeNotAllowed {
        /// The parameter ID
        parameter_id: String,
        /// The scope that was attempted
        scope: ParameterScope,
    },
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
    fn test_scope_specificity() {
        let global = ParameterScope::Global;
        let fed = ParameterScope::Federation {
            id: EntityId::federation("test-fed").unwrap(),
        };
        let coop = ParameterScope::Cooperative {
            id: EntityId::cooperative("test-coop").unwrap(),
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
            id: EntityId::federation("midwest-fed").unwrap(),
        };
        assert_eq!(fed.to_string(), "federation:midwest-fed");

        let coop = ParameterScope::Cooperative {
            id: EntityId::cooperative("food-coop").unwrap(),
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
}
