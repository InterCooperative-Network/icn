//! Protocol parameter types and storage trait
//!
//! These types define the kernel-level interface for governable protocol
//! parameters. Domain apps (like governance) implement the storage trait
//! and provide parameter defaults, while the kernel uses these types
//! without understanding governance semantics.
//!
//! # Meaning Firewall
//!
//! These types are deliberately "dumb" — they carry values and constraints
//! without interpreting them. The governance app decides what parameters
//! exist and what their values mean. The kernel just stores, retrieves,
//! and schedules changes.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Known parameter categories for validation
pub const KNOWN_PARAMETER_CATEGORIES: &[&str] = &[
    "gossip",
    "network",
    "ledger",
    "governance",
    "trust",
    "ratelimit",
    "compute",
];

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

    /// Version for optimistic locking
    ///
    /// Incremented on each update to detect concurrent modifications.
    #[serde(default)]
    pub version: u64,
}

impl ProtocolParameter {
    /// Validate that a parameter ID follows the required format.
    ///
    /// Valid formats:
    /// - `category.name` (e.g., "gossip.fanout")
    /// - `category.subcategory.name` (e.g., "network.peer.timeout")
    pub fn validate_id(id: &str) -> Result<(), ParameterValidationError> {
        if id.is_empty() {
            return Err(ParameterValidationError::InvalidParameterId {
                id: id.to_string(),
                reason: "parameter ID cannot be empty",
            });
        }

        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() < 2 {
            return Err(ParameterValidationError::InvalidParameterId {
                id: id.to_string(),
                reason: "parameter ID must follow 'category.name' format (missing dot separator)",
            });
        }

        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                return Err(ParameterValidationError::InvalidParameterId {
                    id: id.to_string(),
                    reason: if i == 0 {
                        "category component cannot be empty"
                    } else if i == parts.len() - 1 {
                        "name component cannot be empty"
                    } else {
                        "subcategory component cannot be empty"
                    },
                });
            }
        }

        Ok(())
    }

    /// Validate that a parameter ID uses a known category.
    pub fn validate_known_category(id: &str) -> Result<(), ParameterValidationError> {
        Self::validate_id(id)?;

        let category = id.split('.').next().unwrap_or("");
        if !KNOWN_PARAMETER_CATEGORIES.contains(&category) {
            return Err(ParameterValidationError::InvalidParameterId {
                id: id.to_string(),
                reason: "unknown category (must be one of: gossip, network, ledger, governance, trust, ratelimit, compute)",
            });
        }

        Ok(())
    }

    /// Create a new protocol parameter with ID validation.
    pub fn try_new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        value: ParameterValue,
    ) -> Result<Self, ParameterValidationError> {
        let id_string = id.into();
        Self::validate_id(&id_string)?;

        Ok(Self {
            id: id_string,
            name: name.into(),
            description: description.into(),
            value,
            constraints: ParameterConstraints::default(),
            scope: ParameterScope::Global,
            updated_at: 0,
            updated_by: None,
            version: 0,
        })
    }

    /// Create a new protocol parameter (unchecked — for internal/test use).
    ///
    /// `updated_at` defaults to 0 because kernel-api does not depend on `icn_time`.
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
            updated_at: 0,
            updated_by: None,
            version: 0,
        }
    }

    /// Builder: set constraints
    #[must_use]
    pub fn with_constraints(mut self, constraints: ParameterConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Builder: set scope
    #[must_use]
    pub fn with_scope(mut self, scope: ParameterScope) -> Self {
        self.scope = scope;
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

    /// Builder: set the `updated_at` timestamp.
    ///
    /// Since kernel-api does not depend on `icn_time`, callers that need
    /// real timestamps should chain this after `new()`.
    #[must_use]
    pub fn with_updated_at(mut self, updated_at: u64) -> Self {
        self.updated_at = updated_at;
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
            ParameterValue::Percentage(f) if !(*f >= 0.0 && *f <= 100.0) => {
                return Err(ParameterValidationError::PercentageOutOfRange { value: *f });
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

        // Check allowed values (using approximate equality for floats)
        if let Some(ref allowed) = self.constraints.allowed_values {
            let is_allowed = allowed.iter().any(|a| new_value.approximately_eq(a));
            if !is_allowed {
                return Err(ParameterValidationError::NotAllowed {
                    value: new_value.clone(),
                    allowed: allowed.clone(),
                });
            }
        }

        Ok(())
    }

    /// Extract the category from the parameter ID (e.g., "gossip" from "gossip.fanout")
    pub fn category(&self) -> &str {
        self.id.split('.').next().unwrap_or(&self.id)
    }
}

// ============================================================================
// ParameterValue
// ============================================================================

/// Typed parameter value
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

/// Epsilon for floating-point comparisons
const FLOAT_COMPARISON_EPSILON: f64 = 1e-6;

impl ParameterValue {
    fn floats_approximately_equal(a: f64, b: f64) -> bool {
        if a == b {
            return true;
        }
        let diff = (a - b).abs();
        let max_abs = a.abs().max(b.abs());
        diff <= FLOAT_COMPARISON_EPSILON.max(max_abs * FLOAT_COMPARISON_EPSILON)
    }

    fn float_partial_cmp(a: f64, b: f64) -> Option<std::cmp::Ordering> {
        if Self::floats_approximately_equal(a, b) {
            Some(std::cmp::Ordering::Equal)
        } else {
            a.partial_cmp(&b)
        }
    }

    /// Check approximate equality (for float types).
    pub fn approximately_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ParameterValue::Float(a), ParameterValue::Float(b)) => {
                Self::floats_approximately_equal(*a, *b)
            }
            (ParameterValue::Percentage(a), ParameterValue::Percentage(b)) => {
                Self::floats_approximately_equal(*a, *b)
            }
            _ => self == other,
        }
    }
}

impl PartialOrd for ParameterValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (ParameterValue::Integer(a), ParameterValue::Integer(b)) => a.partial_cmp(b),
            (ParameterValue::Float(a), ParameterValue::Float(b)) => Self::float_partial_cmp(*a, *b),
            (ParameterValue::Duration(a), ParameterValue::Duration(b)) => a.partial_cmp(b),
            (ParameterValue::Bytes(a), ParameterValue::Bytes(b)) => a.partial_cmp(b),
            (ParameterValue::Percentage(a), ParameterValue::Percentage(b)) => {
                Self::float_partial_cmp(*a, *b)
            }
            (ParameterValue::String(a), ParameterValue::String(b)) => a.partial_cmp(b),
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
///
/// Entity IDs are stored as plain strings. The governance app is
/// responsible for interpreting these as `EntityId` values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParameterScope {
    /// Applies to all nodes in the network
    Global,
    /// Applies to a specific federation
    Federation {
        /// Federation entity ID (as string)
        id: String,
    },
    /// Applies to a specific cooperative
    Cooperative {
        /// Cooperative entity ID (as string)
        id: String,
    },
}

impl ParameterScope {
    /// Check if this scope is more specific than another
    pub fn is_more_specific_than(&self, other: &ParameterScope) -> bool {
        matches!(
            (self, other),
            (
                ParameterScope::Cooperative { .. },
                ParameterScope::Federation { .. }
            ) | (ParameterScope::Cooperative { .. }, ParameterScope::Global)
                | (ParameterScope::Federation { .. }, ParameterScope::Global)
        )
    }

    /// Get the entity ID string if this is an entity-scoped parameter
    pub fn entity_id_str(&self) -> Option<&str> {
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
            ParameterScope::Federation { id } => write!(f, "federation:{id}"),
            ParameterScope::Cooperative { id } => write!(f, "cooperative:{id}"),
        }
    }
}

// ============================================================================
// ParameterChange
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
// ParameterValidationError
// ============================================================================

/// Errors that can occur when validating parameter values
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParameterValidationError {
    /// Value type doesn't match parameter type
    #[error("Type mismatch: expected {expected}, got {got}. Ensure the value type matches the parameter definition.")]
    TypeMismatch {
        expected: &'static str,
        got: &'static str,
    },

    /// Value is below minimum
    #[error("Value {value} is below minimum {min}. Provide a value >= {min}.")]
    BelowMinimum {
        value: ParameterValue,
        min: ParameterValue,
    },

    /// Value is above maximum
    #[error("Value {value} is above maximum {max}. Provide a value <= {max}.")]
    AboveMaximum {
        value: ParameterValue,
        max: ParameterValue,
    },

    /// Parameter ID format is invalid
    #[error(
        "Invalid parameter ID '{id}': {reason}. Use format: category.name (e.g., 'gossip.fanout')."
    )]
    InvalidParameterId { id: String, reason: &'static str },

    /// Percentage value is out of valid range (0.0-100.0)
    #[error("Percentage {value} is out of range (must be 0.0-100.0). Use 50.0 for 50%, not 0.5.")]
    PercentageOutOfRange { value: f64 },

    /// Value is not in allowed set
    #[error("Value {value} is not in allowed set. Valid values: {allowed:?}")]
    NotAllowed {
        value: ParameterValue,
        allowed: Vec<ParameterValue>,
    },

    /// Invalid floating-point value (NaN or Infinity)
    #[error("Invalid float value {value}: {reason}. Use a finite number.")]
    InvalidFloatValue { value: f64, reason: &'static str },

    /// Parameter not found
    #[error("Parameter not found: '{0}'. Use list_parameters() to see available parameters.")]
    NotFound(String),

    /// Parameter cannot be modified at this scope
    #[error("Parameter '{parameter_id}' cannot be overridden at {scope} scope. Check constraints.allow_override.")]
    ScopeNotAllowed {
        parameter_id: String,
        scope: ParameterScope,
    },
}

// ============================================================================
// PendingParameterChange
// ============================================================================

/// Unique identifier for a pending parameter change
pub type PendingChangeId = String;

/// Status of a pending parameter change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingChangeStatus {
    /// Change is scheduled but not yet applied
    Pending,
    /// Change was successfully applied at the effective time
    Applied,
    /// Change was cancelled before it took effect
    Cancelled,
    /// Change was superseded by a newer change to the same parameter
    Superseded,
}

impl fmt::Display for PendingChangeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PendingChangeStatus::Pending => write!(f, "pending"),
            PendingChangeStatus::Applied => write!(f, "applied"),
            PendingChangeStatus::Cancelled => write!(f, "cancelled"),
            PendingChangeStatus::Superseded => write!(f, "superseded"),
        }
    }
}

/// A scheduled parameter change waiting to take effect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingParameterChange {
    /// Unique identifier for this pending change
    pub id: PendingChangeId,
    /// ID of the parameter being changed
    pub parameter_id: String,
    /// The new value to apply
    pub new_value: ParameterValue,
    /// Unix timestamp when this change should take effect
    pub effective_at: u64,
    /// Scope where this change applies
    pub scope: ParameterScope,
    /// ID of the proposal that authorized this change
    pub proposal_id: String,
    /// Unix timestamp when this pending change was created
    pub created_at: u64,
    /// Current status of this pending change
    pub status: PendingChangeStatus,
    /// Rationale for the change (copied from proposal)
    pub rationale: String,
    /// If superseded, the ID of the change that superseded this one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<PendingChangeId>,
    /// If applied, the timestamp when it was applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<u64>,
    /// If cancelled, the reason for cancellation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<String>,
}

impl PendingParameterChange {
    /// Create a new pending parameter change.
    ///
    /// The `created_at` timestamp defaults to 0 because kernel-api does not
    /// depend on `icn_time`. Chain `.with_created_at(ts)` to set a real value.
    pub fn new(
        id: impl Into<String>,
        parameter_id: impl Into<String>,
        new_value: ParameterValue,
        effective_at: u64,
        scope: ParameterScope,
        proposal_id: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            parameter_id: parameter_id.into(),
            new_value,
            effective_at,
            scope,
            proposal_id: proposal_id.into(),
            created_at: 0, // Use with_created_at() to set a real timestamp
            status: PendingChangeStatus::Pending,
            rationale: rationale.into(),
            superseded_by: None,
            applied_at: None,
            cancellation_reason: None,
        }
    }

    /// Set the creation timestamp (builder pattern).
    ///
    /// Since kernel-api does not depend on `icn_time`, callers that need
    /// real timestamps should call this after `new()`.
    pub fn with_created_at(mut self, created_at: u64) -> Self {
        self.created_at = created_at;
        self
    }

    /// Generate a unique ID for a pending change.
    ///
    /// The timestamp component is set to 0 in this kernel-api version because
    /// kernel-api does not depend on `icn_time`. Callers needing timestamp-based
    /// IDs should use `generate_id_with_timestamp` instead.
    pub fn generate_id(parameter_id: &str) -> String {
        Self::generate_id_with_timestamp(parameter_id, 0)
    }

    /// Generate a unique ID for a pending change with a caller-provided timestamp.
    pub fn generate_id_with_timestamp(parameter_id: &str, timestamp: u64) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("pending:{parameter_id}:{timestamp}:{counter:08x}")
    }

    /// Check if this change is due (effective_at <= current time)
    pub fn is_due(&self, current_time: u64) -> bool {
        self.status == PendingChangeStatus::Pending && self.effective_at <= current_time
    }

    /// Mark this change as applied.
    ///
    /// The `applied_at` timestamp defaults to 0. Use `mark_applied_at()` to
    /// provide a real timestamp from `icn_time::current_timestamp_secs()`.
    pub fn mark_applied(&mut self) {
        self.mark_applied_at(0);
    }

    /// Mark this change as applied with a specific timestamp.
    pub fn mark_applied_at(&mut self, timestamp: u64) {
        self.status = PendingChangeStatus::Applied;
        self.applied_at = Some(timestamp);
    }

    /// Mark this change as cancelled
    pub fn mark_cancelled(&mut self, reason: impl Into<String>) {
        self.status = PendingChangeStatus::Cancelled;
        self.cancellation_reason = Some(reason.into());
    }

    /// Mark this change as superseded by another change
    pub fn mark_superseded(&mut self, superseded_by: impl Into<String>) {
        self.status = PendingChangeStatus::Superseded;
        self.superseded_by = Some(superseded_by.into());
    }

    /// Time until this change takes effect (in seconds), or None if already due
    pub fn time_until_effective(&self, current_time: u64) -> Option<u64> {
        if self.effective_at > current_time {
            Some(self.effective_at - current_time)
        } else {
            None
        }
    }
}

// ============================================================================
// ProtocolParameterStore trait
// ============================================================================

/// Trait for protocol parameter storage operations
///
/// This is the kernel-level abstraction for parameter storage. Domain apps
/// (governance) provide concrete implementations (e.g., SledParameterStore).
pub trait ProtocolParameterStore: Send + Sync {
    /// Get a parameter by ID (global scope)
    fn get(&self, id: &str) -> anyhow::Result<Option<ProtocolParameter>>;

    /// Get a parameter with scope resolution
    ///
    /// Resolves the effective value by checking scopes in order:
    /// Cooperative > Federation > Global.
    /// Entity IDs are passed as strings.
    fn get_effective(
        &self,
        id: &str,
        coop_id: Option<&str>,
        fed_id: Option<&str>,
    ) -> anyhow::Result<Option<ProtocolParameter>>;

    /// Set a parameter value
    fn set(
        &self,
        param: ProtocolParameter,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) -> anyhow::Result<()>;

    /// List all parameters (global scope only)
    fn list(&self) -> anyhow::Result<Vec<ProtocolParameter>>;

    /// List all parameters in a category
    fn list_by_category(&self, category: &str) -> anyhow::Result<Vec<ProtocolParameter>>;

    /// Get change history for a parameter
    fn get_history(&self, id: &str) -> anyhow::Result<Vec<ParameterChange>>;

    /// Get paginated change history for a parameter
    fn get_history_paginated(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> anyhow::Result<(Vec<ParameterChange>, usize)>;

    /// Prune old history entries
    fn prune_history(&self, id: &str, max_entries: usize) -> anyhow::Result<usize>;

    /// Delete a parameter
    fn delete(&self, id: &str) -> anyhow::Result<()>;

    /// Check if a parameter exists
    fn exists(&self, id: &str) -> anyhow::Result<bool>;

    /// Count total parameters
    fn count(&self) -> anyhow::Result<usize>;

    /// Count total history entries across all parameters
    fn total_history_count(&self) -> anyhow::Result<usize>;

    /// Validate a new value against a parameter's constraints
    fn validate(
        &self,
        id: &str,
        new_value: &ParameterValue,
    ) -> Result<(), ParameterValidationError>;

    /// List all scoped (non-global) parameters
    fn list_scoped_parameters(&self) -> anyhow::Result<Vec<ProtocolParameter>>;

    /// Delete a specific scoped parameter by ID and scope
    fn delete_scoped_parameter(&self, id: &str, scope: &ParameterScope) -> anyhow::Result<bool>;

    /// Add a pending parameter change for delayed execution
    fn add_pending_change(&self, change: PendingParameterChange) -> anyhow::Result<()>;

    /// Get a pending change by ID
    fn get_pending_change(
        &self,
        id: &PendingChangeId,
    ) -> anyhow::Result<Option<PendingParameterChange>>;

    /// List all pending changes
    fn list_pending_changes(&self) -> anyhow::Result<Vec<PendingParameterChange>>;

    /// List pending changes for a specific parameter
    fn list_pending_changes_for_parameter(
        &self,
        parameter_id: &str,
    ) -> anyhow::Result<Vec<PendingParameterChange>>;

    /// Get all pending changes that are due (effective_at <= timestamp)
    fn get_changes_due_before(&self, timestamp: u64)
        -> anyhow::Result<Vec<PendingParameterChange>>;

    /// Update a pending change
    fn update_pending_change(&self, change: PendingParameterChange) -> anyhow::Result<()>;

    /// Cancel a pending change
    fn cancel_pending_change(&self, id: &PendingChangeId, reason: &str) -> anyhow::Result<()>;

    /// Get count of active pending changes (status = Pending)
    fn count_pending_changes(&self) -> anyhow::Result<usize>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ProtocolParameter builder tests ---

    #[test]
    fn test_new_defaults() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Integer(42));
        assert_eq!(p.id, "id");
        assert_eq!(p.updated_at, 0);
        assert_eq!(p.version, 0);
        assert_eq!(p.scope, ParameterScope::Global);
    }

    #[test]
    fn test_with_updated_at() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Integer(1))
            .with_updated_at(1234567890);
        assert_eq!(p.updated_at, 1234567890);
    }

    #[test]
    fn test_with_scope() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Boolean(true))
            .with_scope(ParameterScope::Cooperative {
                id: "coop-1".into(),
            });
        assert_eq!(
            p.scope,
            ParameterScope::Cooperative {
                id: "coop-1".into()
            }
        );
    }

    #[test]
    fn test_with_constraints() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Integer(10))
            .with_min(ParameterValue::Integer(0))
            .with_max(ParameterValue::Integer(100));
        assert_eq!(p.constraints.min, Some(ParameterValue::Integer(0)));
        assert_eq!(p.constraints.max, Some(ParameterValue::Integer(100)));
    }

    // --- Validation tests ---

    #[test]
    fn test_validate_type_mismatch() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Integer(10));
        let err = p.validate(&ParameterValue::Float(1.0)).unwrap_err();
        assert!(matches!(err, ParameterValidationError::TypeMismatch { .. }));
    }

    #[test]
    fn test_validate_nan_rejected() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Float(1.0));
        let err = p.validate(&ParameterValue::Float(f64::NAN)).unwrap_err();
        assert!(matches!(
            err,
            ParameterValidationError::InvalidFloatValue { .. }
        ));
    }

    #[test]
    fn test_validate_infinity_rejected() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Float(1.0));
        let err = p
            .validate(&ParameterValue::Float(f64::INFINITY))
            .unwrap_err();
        assert!(matches!(
            err,
            ParameterValidationError::InvalidFloatValue { .. }
        ));
    }

    #[test]
    fn test_validate_below_min() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Integer(10))
            .with_min(ParameterValue::Integer(5));
        let err = p.validate(&ParameterValue::Integer(3)).unwrap_err();
        assert!(matches!(err, ParameterValidationError::BelowMinimum { .. }));
    }

    #[test]
    fn test_validate_above_max() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Integer(10))
            .with_max(ParameterValue::Integer(100));
        let err = p.validate(&ParameterValue::Integer(200)).unwrap_err();
        assert!(matches!(err, ParameterValidationError::AboveMaximum { .. }));
    }

    #[test]
    fn test_validate_in_range() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Integer(10))
            .with_min(ParameterValue::Integer(0))
            .with_max(ParameterValue::Integer(100));
        assert!(p.validate(&ParameterValue::Integer(50)).is_ok());
    }

    #[test]
    fn test_validate_float_epsilon_boundary() {
        // Float comparison uses epsilon tolerance
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::Float(1.0))
            .with_min(ParameterValue::Float(1.0));
        // Value very slightly below min should still pass due to epsilon
        assert!(p.validate(&ParameterValue::Float(1.0 - 1e-10)).is_ok());
    }

    #[test]
    fn test_validate_string_same_type_ok() {
        let p = ProtocolParameter::new("id", "name", "desc", ParameterValue::String("x".into()));
        assert!(p.validate(&ParameterValue::String("y".into())).is_ok());
    }

    // --- ParameterScope tests ---

    #[test]
    fn test_scope_specificity() {
        let global = ParameterScope::Global;
        let fed = ParameterScope::Federation { id: "fed-1".into() };
        let coop = ParameterScope::Cooperative {
            id: "coop-1".into(),
        };

        assert!(coop.is_more_specific_than(&fed));
        assert!(coop.is_more_specific_than(&global));
        assert!(fed.is_more_specific_than(&global));
        assert!(!global.is_more_specific_than(&fed));
        assert!(!fed.is_more_specific_than(&coop));
    }

    #[test]
    fn test_scope_display() {
        assert_eq!(ParameterScope::Global.to_string(), "global");
        assert_eq!(
            ParameterScope::Cooperative {
                id: "my-coop".into()
            }
            .to_string(),
            "cooperative:my-coop"
        );
    }

    // --- PendingParameterChange tests ---

    #[test]
    fn test_pending_change_new_defaults() {
        let c = PendingParameterChange::new(
            "change-1",
            "param-1",
            ParameterValue::Integer(42),
            1000,
            ParameterScope::Global,
            "proposal-1",
            "testing",
        );
        assert_eq!(c.created_at, 0);
        assert_eq!(c.status, PendingChangeStatus::Pending);
        assert!(c.applied_at.is_none());
    }

    #[test]
    fn test_pending_change_with_created_at() {
        let c = PendingParameterChange::new(
            "change-1",
            "param-1",
            ParameterValue::Integer(1),
            1000,
            ParameterScope::Global,
            "p-1",
            "r",
        )
        .with_created_at(999);
        assert_eq!(c.created_at, 999);
    }

    #[test]
    fn test_mark_applied_at() {
        let mut c = PendingParameterChange::new(
            "change-1",
            "param-1",
            ParameterValue::Integer(1),
            1000,
            ParameterScope::Global,
            "p-1",
            "r",
        );
        c.mark_applied_at(5000);
        assert_eq!(c.status, PendingChangeStatus::Applied);
        assert_eq!(c.applied_at, Some(5000));
    }

    #[test]
    fn test_generate_id_uniqueness() {
        let id1 = PendingParameterChange::generate_id("param-a");
        let id2 = PendingParameterChange::generate_id("param-a");
        assert_ne!(id1, id2, "Generated IDs must be unique");
        assert!(id1.starts_with("pending:param-a:"));
    }

    #[test]
    fn test_generate_id_with_timestamp() {
        let id = PendingParameterChange::generate_id_with_timestamp("param-b", 12345);
        assert!(id.starts_with("pending:param-b:12345:"));
    }

    // --- ParameterValue display tests ---

    #[test]
    fn test_parameter_value_display() {
        assert_eq!(ParameterValue::Integer(42).to_string(), "42");
        assert_eq!(ParameterValue::Float(2.5).to_string(), "2.5");
        assert_eq!(ParameterValue::Boolean(true).to_string(), "true");
        assert_eq!(
            ParameterValue::String("hello".into()).to_string(),
            "\"hello\""
        );
        assert_eq!(ParameterValue::Duration(90).to_string(), "1m");
    }

    #[test]
    fn test_parameter_value_type_name() {
        assert_eq!(ParameterValue::Integer(0).type_name(), "integer");
        assert_eq!(ParameterValue::Float(0.0).type_name(), "float");
        assert_eq!(ParameterValue::Boolean(false).type_name(), "boolean");
        assert_eq!(ParameterValue::String(String::new()).type_name(), "string");
        assert_eq!(ParameterValue::Duration(0).type_name(), "duration");
    }

    // --- ParameterValue PartialOrd tests ---

    #[test]
    fn test_integer_ordering() {
        assert!(ParameterValue::Integer(5) < ParameterValue::Integer(10));
        assert!(ParameterValue::Integer(10) > ParameterValue::Integer(5));
        assert_eq!(
            ParameterValue::Integer(5).partial_cmp(&ParameterValue::Integer(5)),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn test_float_ordering() {
        assert!(ParameterValue::Float(1.0) < ParameterValue::Float(2.0));
    }

    #[test]
    fn test_cross_type_ordering_is_none() {
        assert_eq!(
            ParameterValue::Integer(5).partial_cmp(&ParameterValue::Float(5.0)),
            None
        );
    }
}
