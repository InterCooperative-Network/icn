//! Use-based resource access model
//!
//! This module implements a use-based access model for resources that prevents
//! rent-seeking and speculation by requiring active use rather than passive ownership.
//!
//! # Core Concepts
//!
//! - **UseAccess**: Time-limited, renewable access rights with accumulation limits
//! - **Stewardship**: Access tied to duties and responsibilities
//! - **Anti-Speculation**: Rules preventing rent-seeking and profit from access transfers
//! - **Witness Signatures**: Optional cryptographic attestation for handoff procedures
//!
//! # Witness Signatures for Handoff Procedures
//!
//! For high-value resources, `HandoffProcedure` supports optional witness requirements:
//!
//! ```rust,ignore
//! use icn_ledger::use_access::{AccessModel, ResourceAccess, StewardshipDuty, UsageEvent};
//! use icn_entity::EntityId;
//!
//! // Create stewardship with witness-verified handoff
//! let witness1 = "did:icn:witness1".to_string();
//! let witness2 = "did:icn:witness2".to_string();
//!
//! let access = ResourceAccess::new(
//!     "high-value-equipment".to_string(),
//!     EntityId::individual_from_did(steward_did.clone()),
//!     AccessModel::Stewardship {
//!         duties: vec![StewardshipDuty::HandoffProcedure {
//!             steps: vec![
//!                 "Document current state".to_string(),
//!                 "Transfer credentials".to_string(),
//!             ],
//!             witnesses: Some(vec![witness1.clone(), witness2.clone()]),
//!             min_witness_signatures: Some(2),  // Require both witnesses
//!         }],
//!         review_period_seconds: 90 * 24 * 3600,
//!     },
//! );
//!
//! // Complete handoff step with witness attestation
//! let step_event = UsageEvent::handoff_step(timestamp, "Documented state", 0)
//!     .with_witness(witness1.clone())
//!     .with_witness(witness2.clone());
//! access.record_usage_event(step_event)?;
//!
//! // Validate handoff completion before transfer
//! access.validate_handoff_completion()?;
//! ```
//!
//! **Backward Compatibility**: When `witnesses` is `None`, no witness validation is performed,
//! maintaining existing behavior for community-context resources.
//!
//! # Example
//!
//! ```rust,ignore
//! use icn_ledger::use_access::{AccessModel, ResourceAccess};
//! use icn_entity::EntityId;
//!
//! // Create use-based access
//! let access = ResourceAccess::new(
//!     "tool-shed-001".to_string(),
//!     EntityId::individual_from_did(alice_did.clone()),
//!     AccessModel::UseAccess {
//!         duration_seconds: 7 * 24 * 3600, // 1 week
//!         renewable: true,
//!         max_accumulated: 4, // Max 4 weeks total
//!     },
//! );
//!
//! // Record usage
//! access.record_usage(current_time, "Used for repairs")?;
//!
//! // Check if idle
//! if access.is_idle(current_time, 48 * 3600) {
//!     // Revoke due to non-use
//! }
//! ```

use icn_entity::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use thiserror::Error;

/// Errors related to resource access
#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessError {
    /// Access has expired
    #[error("Access expired at {0}")]
    Expired(u64),

    /// Access is not renewable
    #[error("Access is not renewable")]
    NotRenewable,

    /// Maximum accumulated duration reached
    #[error("Maximum accumulated access reached: {0} renewals")]
    MaxAccumulatedReached(u32),

    /// Stewardship duty not fulfilled
    #[error("Stewardship duty not fulfilled: {0}")]
    DutyUnfulfilled(String),

    /// Idle period exceeded
    #[error("Resource idle for {idle_seconds}s, max allowed: {max_idle_seconds}s")]
    IdleTooLong {
        /// Seconds since last usage
        idle_seconds: u64,
        /// Maximum allowed idle time
        max_idle_seconds: u64,
    },

    /// Transfer for profit not allowed
    #[error("Cannot transfer access for profit")]
    ProfitTransferNotAllowed,

    /// Invalid timestamp provided (time travel attempt)
    #[error("Invalid timestamp {provided}: must be >= {minimum} (granted_at)")]
    InvalidTimestamp {
        /// The timestamp provided
        provided: u64,
        /// The minimum valid timestamp
        minimum: u64,
    },

    /// Access has been revoked
    #[error("Access has been revoked: {0}")]
    Revoked(String),

    /// Stewardship review overdue
    #[error("Stewardship review overdue: last review at {last_review}, current time {current_time}, period {review_period}s")]
    ReviewOverdue {
        /// Timestamp of last review
        last_review: u64,
        /// Current timestamp
        current_time: u64,
        /// Required review period
        review_period: u64,
    },
}

/// Result type for access operations
pub type Result<T> = std::result::Result<T, AccessError>;

/// Model for resource access
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessModel {
    /// Use-based access with time limits
    UseAccess {
        /// Duration of access in seconds
        duration_seconds: u64,
        /// Whether access can be renewed
        renewable: bool,
        /// Maximum number of times access can be accumulated/renewed
        max_accumulated: u32,
    },

    /// Stewardship-based access with responsibilities
    Stewardship {
        /// Required duties for maintaining access
        duties: Vec<StewardshipDuty>,
        /// Period in seconds between duty reviews
        review_period_seconds: u64,
    },
}

/// Stewardship duty that must be fulfilled
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StewardshipDuty {
    /// Maintenance requirement
    Maintenance {
        /// Description of maintenance
        description: String,
        /// Frequency in seconds
        frequency_seconds: u64,
    },

    /// Usage reporting requirement
    UsageReporting {
        /// Minimum reports per period
        min_reports: u32,
        /// Period in seconds
        period_seconds: u64,
    },

    /// Community benefit obligation
    CommunityBenefit {
        /// Description of benefit
        description: String,
        /// Required completion by timestamp
        due_by: u64,
    },

    /// Handoff procedure when relinquishing access
    HandoffProcedure {
        /// Steps required for proper handoff
        steps: Vec<String>,
        /// Optional witnesses who must attest to step completion
        /// When None, no witness validation is performed (backward compatible)
        witnesses: Option<Vec<String>>,
        /// Minimum number of witness signatures required per step
        /// Defaults to all witnesses if not specified
        min_witness_signatures: Option<u32>,
    },
}

/// Type of duty-related event for structured validation
///
/// Using structured event types instead of keyword matching provides:
/// - Type-safe duty verification
/// - O(1) per-event matching vs substring search/allocation on descriptions
///   (overall duty checking still performs a linear scan over the usage log)
/// - Clear documentation of expected event categories
/// - Explicit metadata prevents gaming (e.g., adding "maintenance" to unrelated descriptions)
///
/// # Note on `duty_id` fields
///
/// The `duty_id` fields in `Maintenance` and `CommunityBenefit` are reserved for
/// future use to correlate events with specific duty definitions. Currently,
/// `check_duties()` only matches on the variant type, not the `duty_id` value.
///
/// Planned use cases for `duty_id`:
/// - Tracking which specific maintenance requirement was fulfilled
/// - Correlating events with governance-defined duty catalogs
/// - Generating detailed compliance reports per duty
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DutyEventType {
    /// Maintenance task completion
    Maintenance {
        /// Optional identifier correlating this event with a specific duty definition.
        /// Reserved for future use (see enum-level docs).
        duty_id: Option<String>,
    },
    /// Usage or status report
    Report,
    /// Community benefit provided
    CommunityBenefit {
        /// Optional identifier correlating this event with a specific duty definition.
        /// Reserved for future use (see enum-level docs).
        duty_id: Option<String>,
    },
    /// Handoff procedure step completion
    HandoffStep {
        /// Index of the step in the handoff procedure (0-based)
        step_index: usize,
    },
    /// General usage event (not duty-specific)
    GeneralUsage,
}

/// Event recording resource usage
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEvent {
    /// Timestamp of usage
    pub timestamp: u64,
    /// Description of usage
    pub description: String,
    /// Optional witness DIDs (for verification)
    pub witnesses: Vec<String>,
    /// Structured duty type for explicit validation
    /// When None, falls back to keyword-based matching for backward compatibility
    #[serde(alias = "event_type")]
    pub duty_type: Option<DutyEventType>,
}

impl UsageEvent {
    /// Create a new usage event
    pub fn new(timestamp: u64, description: String) -> Self {
        Self {
            timestamp,
            description,
            witnesses: Vec::new(),
            duty_type: None,
        }
    }

    /// Create a new usage event with structured duty type
    pub fn with_duty_type(timestamp: u64, description: String, duty_type: DutyEventType) -> Self {
        Self {
            timestamp,
            description,
            witnesses: Vec::new(),
            duty_type: Some(duty_type),
        }
    }

    /// Alias for `with_duty_type` for convenience
    pub fn with_type(timestamp: u64, description: String, duty_type: DutyEventType) -> Self {
        Self::with_duty_type(timestamp, description, duty_type)
    }

    // === Convenience constructors for common duty types ===

    /// Create a maintenance event
    ///
    /// # Example
    /// ```rust,ignore
    /// let event = UsageEvent::maintenance(timestamp, "Weekly watering completed")
    ///     .with_witness("did:icn:witness1".to_string());
    /// access.record_usage_event(event)?;
    /// ```
    pub fn maintenance(timestamp: u64, description: impl Into<String>) -> Self {
        Self::with_duty_type(
            timestamp,
            description.into(),
            DutyEventType::Maintenance { duty_id: None },
        )
    }

    /// Create a report event
    pub fn report(timestamp: u64, description: impl Into<String>) -> Self {
        Self::with_duty_type(timestamp, description.into(), DutyEventType::Report)
    }

    /// Create a community benefit event
    pub fn community_benefit(timestamp: u64, description: impl Into<String>) -> Self {
        Self::with_duty_type(
            timestamp,
            description.into(),
            DutyEventType::CommunityBenefit { duty_id: None },
        )
    }

    /// Create a handoff step event
    pub fn handoff_step(timestamp: u64, description: impl Into<String>, step_index: usize) -> Self {
        Self::with_duty_type(
            timestamp,
            description.into(),
            DutyEventType::HandoffStep { step_index },
        )
    }

    // === Witness management ===

    /// Add a witness to this event
    ///
    /// Witness DIDs should be in the format `did:icn:<identifier>`.
    /// Invalid DIDs are accepted for backward compatibility but may be
    /// rejected in future versions.
    pub fn with_witness(mut self, witness_did: String) -> Self {
        self.witnesses.push(witness_did);
        self
    }

    /// Add a validated witness to this event
    ///
    /// Returns `None` if the DID format is invalid.
    /// Valid DID format: `did:icn:<identifier>` where identifier is non-empty.
    pub fn with_validated_witness(mut self, witness_did: String) -> Option<Self> {
        if Self::is_valid_did_format(&witness_did) {
            self.witnesses.push(witness_did);
            Some(self)
        } else {
            None
        }
    }

    /// Check if a string is a valid DID format
    ///
    /// Valid format: `did:icn:<identifier>` where identifier is non-empty alphanumeric.
    fn is_valid_did_format(did: &str) -> bool {
        did.starts_with("did:icn:")
            && did.len() > 8
            && did[8..].chars().all(|c| c.is_alphanumeric() || c == ':')
    }

    /// Validate that this event has sufficient unique witnesses
    ///
    /// Counts only unique witness DIDs to prevent gaming by adding duplicates.
    pub fn has_sufficient_witnesses(&self, min_witnesses: usize) -> bool {
        let unique_witnesses: HashSet<&String> = self.witnesses.iter().collect();
        unique_witnesses.len() >= min_witnesses
    }
}

/// Anti-speculation rules to prevent rent-seeking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntiSpeculationRules {
    /// Maximum idle period in seconds before revocation ("use it or lose it")
    pub max_idle_period_seconds: u64,

    /// Whether transfers for profit are prohibited
    pub no_profit_transfer: bool,
}

impl AntiSpeculationRules {
    /// Create standard anti-speculation rules
    pub fn standard() -> Self {
        Self {
            max_idle_period_seconds: 30 * 24 * 3600, // 30 days
            no_profit_transfer: true,
        }
    }

    /// Create strict anti-speculation rules
    pub fn strict() -> Self {
        Self {
            max_idle_period_seconds: 7 * 24 * 3600, // 7 days
            no_profit_transfer: true,
        }
    }

    /// Create lenient anti-speculation rules
    pub fn lenient() -> Self {
        Self {
            max_idle_period_seconds: 90 * 24 * 3600, // 90 days
            no_profit_transfer: false,
        }
    }
}

/// Resource access tracking
///
/// Fields are public for inspection but modifications should go through
/// the provided methods to ensure invariants are maintained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ResourceAccess {
    /// Unique identifier for the resource
    pub resource_id: String,

    /// Entity holding the access
    pub holder: EntityId,

    /// Access model (use-based or stewardship)
    pub model: AccessModel,

    /// When access was granted (timestamp in seconds)
    pub granted_at: u64,

    /// When access expires (None for indefinite stewardship)
    pub expires_at: Option<u64>,

    /// Log of usage events (bounded to prevent unbounded growth)
    pub usage_log: VecDeque<UsageEvent>,

    /// Anti-speculation rules
    pub rules: AntiSpeculationRules,

    /// Number of times renewed (for accumulation tracking)
    pub renewal_count: u32,

    /// Whether access has been revoked (e.g., due to failed duty checks)
    pub revoked: bool,

    /// Reason for revocation (if revoked)
    pub revocation_reason: Option<String>,

    /// Timestamp of last stewardship review (for review_period enforcement)
    pub last_review: Option<u64>,
}

impl ResourceAccess {
    /// Maximum usage log entries to retain
    const MAX_USAGE_LOG_SIZE: usize = 1000;

    /// Create new resource access
    pub fn new(resource_id: String, holder: EntityId, model: AccessModel) -> Self {
        let granted_at = icn_time::current_timestamp_secs();
        let expires_at = Self::calculate_expiration(&model, granted_at);

        Self {
            resource_id,
            holder,
            model,
            granted_at,
            expires_at,
            usage_log: VecDeque::new(),
            rules: AntiSpeculationRules::standard(),
            renewal_count: 0,
            revoked: false,
            revocation_reason: None,
            last_review: None,
        }
    }

    /// Create with custom anti-speculation rules
    pub fn with_rules(mut self, rules: AntiSpeculationRules) -> Self {
        self.rules = rules;
        self
    }

    /// Calculate expiration timestamp based on access model
    ///
    /// Uses saturating_add to prevent overflow on very long durations.
    fn calculate_expiration(model: &AccessModel, granted_at: u64) -> Option<u64> {
        match model {
            AccessModel::UseAccess {
                duration_seconds, ..
            } => Some(granted_at.saturating_add(*duration_seconds)),
            AccessModel::Stewardship { .. } => None, // Stewardship is ongoing
        }
    }

    /// Check if access is currently valid
    pub fn is_valid(&self, current_time: u64) -> bool {
        if let Some(expires_at) = self.expires_at {
            current_time < expires_at
        } else {
            true // Stewardship doesn't expire by time alone
        }
    }

    /// Check if access has expired
    pub fn is_expired(&self, current_time: u64) -> bool {
        !self.is_valid(current_time)
    }

    /// Attempt to renew access
    ///
    /// # Arguments
    /// * `current_time` - The current timestamp (must be >= granted_at)
    ///
    /// # Renewal Behavior
    ///
    /// Renewal extends from the later of (current expiration, current time).
    /// This means:
    /// - If renewed before expiration: time is "banked" (extends from expiration)
    /// - If renewed after expiration: extends from current time
    ///
    /// This is intentional to reward proactive renewal while preventing
    /// accumulation beyond `max_accumulated` renewals.
    ///
    /// # Errors
    /// Returns `AccessError::InvalidTimestamp` if current_time is before granted_at
    pub fn renew(&mut self, current_time: u64) -> Result<()> {
        // Validate time is not in the past (before access was granted)
        if current_time < self.granted_at {
            return Err(AccessError::InvalidTimestamp {
                provided: current_time,
                minimum: self.granted_at,
            });
        }

        match &self.model {
            AccessModel::UseAccess {
                duration_seconds,
                renewable,
                max_accumulated,
            } => {
                if !renewable {
                    return Err(AccessError::NotRenewable);
                }

                if self.renewal_count >= *max_accumulated {
                    return Err(AccessError::MaxAccumulatedReached(*max_accumulated));
                }

                // Renew from current time or expiration, whichever is later
                let renewal_base = self
                    .expires_at
                    .map(|exp| exp.max(current_time))
                    .unwrap_or(current_time);
                self.expires_at = Some(renewal_base.saturating_add(*duration_seconds));
                self.renewal_count += 1;

                Ok(())
            }
            AccessModel::Stewardship { .. } => {
                // Stewardship doesn't need renewal, but duties must be fulfilled
                Ok(())
            }
        }
    }

    /// Record a usage event
    pub fn record_usage(&mut self, timestamp: u64, description: String) -> Result<()> {
        // Delegate to record_usage_event to avoid duplicate validation logic
        let event = UsageEvent::new(timestamp, description);
        self.record_usage_event(event)
    }

    /// Record a structured usage event with duty type and optional witnesses
    ///
    /// This method allows recording events with explicit duty types for type-safe
    /// validation, rather than relying on keyword matching in descriptions.
    ///
    /// # Arguments
    /// * `event` - The pre-constructed UsageEvent (use `UsageEvent::with_duty_type()`)
    ///
    /// # Example
    /// ```rust,ignore
    /// let event = UsageEvent::with_duty_type(
    ///     timestamp,
    ///     "Weekly maintenance completed".to_string(),
    ///     DutyEventType::Maintenance { duty_id: Some("maint-001".to_string()) },
    /// )
    /// .with_witness("did:icn:witness1".to_string());
    ///
    /// access.record_usage_event(event)?;
    /// ```
    pub fn record_usage_event(&mut self, event: UsageEvent) -> Result<()> {
        // Check if access has been revoked
        if self.revoked {
            return Err(AccessError::Revoked(
                self.revocation_reason
                    .clone()
                    .unwrap_or_else(|| "Access revoked".to_string()),
            ));
        }

        // Validate access is still valid (for UseAccess expiration)
        if self.is_expired(event.timestamp) {
            return Err(AccessError::Expired(self.expires_at.unwrap_or(0)));
        }

        self.usage_log.push_back(event);

        // Bound the log size
        while self.usage_log.len() > Self::MAX_USAGE_LOG_SIZE {
            self.usage_log.pop_front();
        }

        Ok(())
    }

    /// Revoke access with a reason
    ///
    /// Once revoked, usage cannot be recorded and the access is effectively terminated.
    pub fn revoke(&mut self, reason: String) {
        self.revoked = true;
        self.revocation_reason = Some(reason);
    }

    /// Check if access is revoked
    pub fn is_revoked(&self) -> bool {
        self.revoked
    }

    /// Check if resource is idle (no recent usage)
    pub fn is_idle(&self, current_time: u64, max_idle_seconds: u64) -> bool {
        if let Some(last_usage) = self.usage_log.back() {
            current_time.saturating_sub(last_usage.timestamp) > max_idle_seconds
        } else {
            // No usage recorded - idle since granted
            current_time.saturating_sub(self.granted_at) > max_idle_seconds
        }
    }

    /// Validate against anti-speculation rules
    pub fn validate_rules(&self, current_time: u64) -> Result<()> {
        // Check idle period
        if self.is_idle(current_time, self.rules.max_idle_period_seconds) {
            let idle_seconds = if let Some(last_usage) = self.usage_log.back() {
                current_time.saturating_sub(last_usage.timestamp)
            } else {
                current_time.saturating_sub(self.granted_at)
            };

            return Err(AccessError::IdleTooLong {
                idle_seconds,
                max_idle_seconds: self.rules.max_idle_period_seconds,
            });
        }

        Ok(())
    }

    /// Check if stewardship duties are fulfilled
    ///
    /// This method validates duty completion by checking the usage log.
    /// It first tries structured event types (O(1) matching), then falls
    /// back to keyword matching for backward compatibility.
    ///
    /// Performance: Uses `.filter()` for time-based filtering to correctly
    /// handle potential out-of-order events (from clock skew, etc.). Iterates
    /// in reverse to find the most recent matching event first. Complexity is
    /// O(k) best case where k is recent events (with chronologically ordered
    /// events), O(n) worst case if events are scattered through the log.
    pub fn check_duties(&self, current_time: u64) -> Result<()> {
        match &self.model {
            AccessModel::Stewardship { duties, .. } => {
                for duty in duties {
                    match duty {
                        StewardshipDuty::Maintenance {
                            description,
                            frequency_seconds,
                        } => {
                            // Find last maintenance event within the time window
                            // Priority: structured duty type > keyword matching
                            // Note: Using filter instead of take_while for correctness with
                            // potentially out-of-order events (clock skew, etc.)
                            let cutoff_time = current_time.saturating_sub(*frequency_seconds);
                            let last_maintenance = self
                                .usage_log
                                .iter()
                                .rev()
                                .filter(|e| e.timestamp >= cutoff_time)
                                .find(|e| {
                                    // First check structured duty type (O(1))
                                    if let Some(duty_type) = &e.duty_type {
                                        matches!(duty_type, DutyEventType::Maintenance { .. })
                                    } else {
                                        // Fallback to keyword matching for backward compatibility
                                        e.description.to_lowercase().contains("maintenance")
                                    }
                                })
                                .map(|e| e.timestamp);

                            let overdue = if let Some(last) = last_maintenance {
                                current_time.saturating_sub(last) > *frequency_seconds
                            } else {
                                current_time.saturating_sub(self.granted_at) > *frequency_seconds
                            };

                            if overdue {
                                return Err(AccessError::DutyUnfulfilled(format!(
                                    "Maintenance overdue: {}",
                                    description
                                )));
                            }
                        }
                        StewardshipDuty::UsageReporting {
                            min_reports,
                            period_seconds,
                        } => {
                            // Count reports in the period
                            // Reports are any usage event with Report type or any event (for backward compat)
                            // Note: Using filter instead of take_while for correctness with
                            // potentially out-of-order events (clock skew, etc.)
                            let period_start = current_time.saturating_sub(*period_seconds);
                            let report_count = self
                                .usage_log
                                .iter()
                                .rev()
                                .filter(|e| e.timestamp >= period_start)
                                .filter(|e| {
                                    // Count structured Report events or any event for backward compat
                                    if let Some(duty_type) = &e.duty_type {
                                        matches!(duty_type, DutyEventType::Report)
                                    } else {
                                        true // Backward compat: count all events as reports
                                    }
                                })
                                .count() as u32;

                            if report_count < *min_reports {
                                return Err(AccessError::DutyUnfulfilled(format!(
                                    "Insufficient usage reports: {} < {}",
                                    report_count, min_reports
                                )));
                            }
                        }
                        StewardshipDuty::CommunityBenefit {
                            description,
                            due_by,
                        } => {
                            // Check if benefit was provided before deadline
                            // Priority: structured duty type > keyword matching
                            let description_lower = description.to_lowercase();
                            let benefit_provided = self.usage_log.iter().any(|e| {
                                // First check structured duty type
                                if let Some(duty_type) = &e.duty_type {
                                    matches!(duty_type, DutyEventType::CommunityBenefit { .. })
                                } else {
                                    // Fallback to keyword matching
                                    e.description.to_lowercase().contains(&description_lower)
                                }
                            });

                            if !benefit_provided && current_time >= *due_by {
                                return Err(AccessError::DutyUnfulfilled(format!(
                                    "Community benefit not provided: {}",
                                    description
                                )));
                            }
                        }
                        StewardshipDuty::HandoffProcedure { .. } => {
                            // Handoff is validated at transfer time
                        }
                    }
                }
                Ok(())
            }
            AccessModel::UseAccess { .. } => Ok(()), // No duties for use access
        }
    }

    /// Check if a stewardship review is overdue
    ///
    /// Returns an error if the time since last review exceeds the review period.
    /// For non-Stewardship access models, always returns Ok.
    pub fn check_review_period(&self, current_time: u64) -> Result<()> {
        match &self.model {
            AccessModel::Stewardship {
                review_period_seconds,
                ..
            } => {
                let last_review = self.last_review.unwrap_or(self.granted_at);
                let time_since_review = current_time.saturating_sub(last_review);

                if time_since_review > *review_period_seconds {
                    return Err(AccessError::ReviewOverdue {
                        last_review,
                        current_time,
                        review_period: *review_period_seconds,
                    });
                }
                Ok(())
            }
            AccessModel::UseAccess { .. } => Ok(()),
        }
    }

    /// Validate duty completion with witness verification
    ///
    /// This method validates that the usage event has sufficient unique witnesses
    /// to qualify for duty completion, and that this access is a stewardship model
    /// (since only stewardship has duties).
    ///
    /// # Arguments
    /// * `event` - The usage event to validate
    /// * `min_witnesses` - Minimum number of unique witnesses required
    ///
    /// # Returns
    /// `true` if the event has at least `min_witnesses` unique witnesses and
    /// this is a stewardship access model, `false` otherwise
    pub fn validate_duty_completion(&self, event: &UsageEvent, min_witnesses: usize) -> bool {
        matches!(self.model, AccessModel::Stewardship { .. })
            && event.has_sufficient_witnesses(min_witnesses)
    }

    /// Record that a stewardship review has been completed
    pub fn record_review(&mut self, timestamp: u64) {
        self.last_review = Some(timestamp);
    }

    /// Check if a review is needed (non-error version for queries)
    pub fn needs_review(&self, current_time: u64) -> bool {
        match &self.model {
            AccessModel::Stewardship {
                review_period_seconds,
                ..
            } => {
                let last_review = self.last_review.unwrap_or(self.granted_at);
                current_time.saturating_sub(last_review) > *review_period_seconds
            }
            AccessModel::UseAccess { .. } => false,
        }
    }

    /// Validate transfer (enforces no-profit rule)
    pub fn validate_transfer(&self, price: Option<i64>) -> Result<()> {
        if self.rules.no_profit_transfer {
            // Any paid transfer is considered profit-seeking
            if let Some(price) = price {
                if price > 0 {
                    return Err(AccessError::ProfitTransferNotAllowed);
                }
            }
        }
        Ok(())
    }

    /// Validate handoff procedure completion with witness verification
    ///
    /// Checks that all required handoff steps have been completed with sufficient
    /// witness attestations when witness requirements are configured.
    ///
    /// # Arguments
    /// * `current_time` - Current timestamp for validation
    ///
    /// # Returns
    /// * `Ok(())` if:
    ///   - No handoff procedure is defined
    ///   - Handoff procedure has no witness requirements
    ///   - All handoff steps completed with sufficient witnesses
    /// * `Err(AccessError::DutyUnfulfilled)` if handoff steps are incomplete or lack witnesses
    pub fn validate_handoff_completion(&self) -> Result<()> {
        // Only validate if this is a Stewardship model with HandoffProcedure
        let AccessModel::Stewardship { duties, .. } = &self.model else {
            return Ok(()); // No handoff required for non-stewardship
        };

        // Find HandoffProcedure duty
        let handoff_duty = duties
            .iter()
            .find(|d| matches!(d, StewardshipDuty::HandoffProcedure { .. }));

        let Some(StewardshipDuty::HandoffProcedure {
            steps,
            witnesses: witness_requirement,
            min_witness_signatures,
        }) = handoff_duty
        else {
            return Ok(()); // No handoff procedure defined
        };

        // If no witness requirement, fall back to existing behavior (OK)
        let Some(required_witnesses) = witness_requirement else {
            return Ok(());
        };

        // Determine minimum witness count
        // Use saturating conversion to handle systems where u32 might exceed usize::MAX
        let min_witnesses = min_witness_signatures
            .map(|n| {
                // On systems where u32 > usize::MAX (e.g., 16-bit), use usize::MAX
                usize::try_from(n).unwrap_or(usize::MAX)
            })
            .unwrap_or(required_witnesses.len());

        // Check each step has been completed with sufficient witnesses
        for (step_index, step_desc) in steps.iter().enumerate() {
            let step_completed = self.usage_log.iter().any(|event| {
                // Check if this is a handoff step event for this step index
                let is_correct_step = matches!(
                    event.duty_type,
                    Some(DutyEventType::HandoffStep { step_index: idx }) if idx == step_index
                );

                // Check if it has sufficient witnesses
                let has_witnesses = event.has_sufficient_witnesses(min_witnesses);

                is_correct_step && has_witnesses
            });

            if !step_completed {
                return Err(AccessError::DutyUnfulfilled(format!(
                    "Handoff step {} not completed with {} witnesses: {}",
                    step_index, min_witnesses, step_desc
                )));
            }
        }

        Ok(())
    }

    /// Get time until expiration (if applicable)
    pub fn time_until_expiration(&self, current_time: u64) -> Option<u64> {
        self.expires_at.map(|exp| exp.saturating_sub(current_time))
    }

    /// Get time since last usage
    pub fn time_since_last_usage(&self, current_time: u64) -> u64 {
        if let Some(last_usage) = self.usage_log.back() {
            current_time.saturating_sub(last_usage.timestamp)
        } else {
            current_time.saturating_sub(self.granted_at)
        }
    }
}

use tracing::error;

/// Trait for persistent storage of resource access records
///
/// This trait abstracts over the storage backend, allowing different implementations
/// (e.g., sled, in-memory for testing, or distributed storage for federation).
pub trait ResourceAccessStore: Send + Sync {
    /// Grant access to a resource
    fn grant(&self, access: ResourceAccess) -> anyhow::Result<()>;

    /// Revoke access to a resource
    fn revoke(&self, resource_id: &str, holder: &EntityId, reason: String) -> anyhow::Result<()>;

    /// Get access record for a specific resource and holder
    fn get(&self, resource_id: &str, holder: &EntityId) -> anyhow::Result<Option<ResourceAccess>>;

    /// List all access records for a specific holder
    fn list_by_holder(&self, holder: &EntityId) -> anyhow::Result<Vec<ResourceAccess>>;

    /// List all access records for a specific resource
    fn list_by_resource(&self, resource_id: &str) -> anyhow::Result<Vec<ResourceAccess>>;

    /// Find expired access records at the given timestamp
    fn find_expired(&self, current_time: u64) -> anyhow::Result<Vec<ResourceAccess>>;

    /// Find idle access records (not used recently)
    fn find_idle(&self, current_time: u64, max_idle: u64) -> anyhow::Result<Vec<ResourceAccess>>;
}

/// Sled-backed resource access store
///
/// Keys are structured as:
/// - Primary: `ledger:resource_access:<resource_id>:<holder>` -> ResourceAccess (JSON)
/// - Index: `ledger:resource_access:idx:holder:<holder>:<resource_id>` -> primary key bytes
/// - Index: `ledger:resource_access:idx:resource:<resource_id>:<holder>` -> primary key bytes
///
/// Index values contain the primary key, enabling single-lookup queries without
/// the N+1 I/O pattern of key-only indexes.
pub struct SledResourceAccessStore {
    store: std::sync::Arc<dyn icn_store::Store>,
}

impl SledResourceAccessStore {
    /// Create a new resource access store using the given storage backend
    pub fn new(store: std::sync::Arc<dyn icn_store::Store>) -> Self {
        Self { store }
    }

    /// Key prefix for all resource access records
    const ACCESS_PREFIX: &'static [u8] = b"ledger:resource_access:";

    /// Primary key for access record: ledger:resource_access:<resource_id>:<holder>
    fn access_key(resource_id: &str, holder: &EntityId) -> Vec<u8> {
        format!("ledger:resource_access:{}:{}", resource_id, holder).into_bytes()
    }

    /// Index key for holder lookup: ledger:resource_access:idx:holder:<holder>:<resource_id>
    fn holder_index_key(holder: &EntityId, resource_id: &str) -> Vec<u8> {
        format!(
            "ledger:resource_access:idx:holder:{}:{}",
            holder, resource_id
        )
        .into_bytes()
    }

    /// Index key for resource lookup: ledger:resource_access:idx:resource:<resource_id>:<holder>
    fn resource_index_key(resource_id: &str, holder: &EntityId) -> Vec<u8> {
        format!(
            "ledger:resource_access:idx:resource:{}:{}",
            resource_id, holder
        )
        .into_bytes()
    }

    /// Prefix for holder index scan
    fn holder_prefix(holder: &EntityId) -> Vec<u8> {
        format!("ledger:resource_access:idx:holder:{}:", holder).into_bytes()
    }

    /// Prefix for resource index scan
    fn resource_prefix(resource_id: &str) -> Vec<u8> {
        format!("ledger:resource_access:idx:resource:{}:", resource_id).into_bytes()
    }

    /// Extract resource_id from a holder index key by stripping the holder prefix.
    ///
    /// Key format: `ledger:resource_access:idx:holder:<holder>:<resource_id>`
    /// Given the prefix `ledger:resource_access:idx:holder:<holder>:`, we extract the resource_id
    /// which is everything after that prefix.
    fn extract_resource_id_from_holder_key(key: &[u8], prefix: &[u8]) -> anyhow::Result<String> {
        if key.len() <= prefix.len() {
            anyhow::bail!("Key too short to contain resource_id after prefix");
        }
        let resource_id_bytes = &key[prefix.len()..];
        let resource_id = std::str::from_utf8(resource_id_bytes)?;
        Ok(resource_id.to_string())
    }

    /// Extract holder EntityId from a resource index key by stripping the resource prefix.
    ///
    /// Key format: `ledger:resource_access:idx:resource:<resource_id>:<holder>`
    /// Given the prefix `ledger:resource_access:idx:resource:<resource_id>:`, we extract the holder
    /// which is everything after that prefix.
    fn extract_holder_from_resource_key(key: &[u8], prefix: &[u8]) -> anyhow::Result<EntityId> {
        use std::str::FromStr;

        if key.len() <= prefix.len() {
            anyhow::bail!("Key too short to contain holder after prefix");
        }
        let holder_bytes = &key[prefix.len()..];
        let holder_str = std::str::from_utf8(holder_bytes)?;
        EntityId::from_str(holder_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse EntityId: {}", e))
    }
}

impl ResourceAccessStore for SledResourceAccessStore {
    fn grant(&self, access: ResourceAccess) -> anyhow::Result<()> {
        let key = Self::access_key(&access.resource_id, &access.holder);
        let holder_idx = Self::holder_index_key(&access.holder, &access.resource_id);
        let resource_idx = Self::resource_index_key(&access.resource_id, &access.holder);

        // Serialize access record
        let value = serde_json::to_vec(&access)?;

        // Store primary record and indexes with best-effort rollback on failure.
        // Note: These writes are not atomic. If a rollback fails, the store may be left
        // in an inconsistent state (primary record without indexes). Recovery requires
        // calling grant() again to overwrite the orphaned record.
        //
        // Index values store the primary key to enable single-lookup queries
        // (avoids N+1 I/O pattern in list_by_* methods).
        //
        // 1. Write primary record
        self.store.put(&key, &value)?;
        // 2. Write holder index with primary key as value; on failure, delete primary record
        if let Err(err) = self.store.put(&holder_idx, &key) {
            // Best-effort rollback of primary record
            if let Err(rollback_err) = self.store.delete(&key) {
                error!(
                    resource_id = access.resource_id,
                    holder = %access.holder,
                    error = %rollback_err,
                    "Failed to rollback primary record after holder index write failed"
                );
            }
            return Err(err);
        }
        // 3. Write resource index with primary key as value; on failure, delete holder index and primary record
        if let Err(err) = self.store.put(&resource_idx, &key) {
            // Best-effort rollback of previously written entries
            if let Err(rollback_err) = self.store.delete(&holder_idx) {
                error!(
                    resource_id = access.resource_id,
                    holder = %access.holder,
                    error = %rollback_err,
                    "Failed to rollback holder index after resource index write failed"
                );
            }
            if let Err(rollback_err) = self.store.delete(&key) {
                error!(
                    resource_id = access.resource_id,
                    holder = %access.holder,
                    error = %rollback_err,
                    "Failed to rollback primary record after resource index write failed"
                );
            }
            return Err(err);
        }

        Ok(())
    }

    /// Revoke access to a resource
    ///
    /// This marks the access record as revoked but intentionally retains the
    /// index entries for audit trail purposes. The revoked record remains
    /// discoverable via `list_by_holder` and `list_by_resource` queries,
    /// allowing cooperatives to track historical access patterns.
    ///
    /// Callers should filter results by `access.is_revoked()` if they only
    /// want active access records. The `find_expired` and `find_idle` methods
    /// already filter out revoked records.
    fn revoke(&self, resource_id: &str, holder: &EntityId, reason: String) -> anyhow::Result<()> {
        let key = Self::access_key(resource_id, holder);

        // Load existing access
        if let Some(bytes) = self.store.get(&key)? {
            let mut access: ResourceAccess = serde_json::from_slice(&bytes)?;

            // Revoke it (index entries are intentionally retained for audit trail)
            access.revoke(reason);

            // Save updated record
            let value = serde_json::to_vec(&access)?;
            self.store.put(&key, &value)?;

            Ok(())
        } else {
            anyhow::bail!(
                "Access not found for resource {} and holder {}",
                resource_id,
                holder
            );
        }
    }

    fn get(&self, resource_id: &str, holder: &EntityId) -> anyhow::Result<Option<ResourceAccess>> {
        let key = Self::access_key(resource_id, holder);

        if let Some(bytes) = self.store.get(&key)? {
            let access: ResourceAccess = serde_json::from_slice(&bytes)?;
            Ok(Some(access))
        } else {
            Ok(None)
        }
    }

    fn list_by_holder(&self, holder: &EntityId) -> anyhow::Result<Vec<ResourceAccess>> {
        let prefix = Self::holder_prefix(holder);
        let index_entries = self.store.scan(&prefix)?;

        let mut results = Vec::new();
        for (index_key, primary_key) in index_entries {
            // Index value contains the primary key; use it directly to fetch data.
            // Fall back to legacy behavior (extract from index key) for empty values
            // to maintain backward compatibility during migration.
            if primary_key.is_empty() {
                // Legacy empty index value: extract resource_id from key and do second lookup
                let resource_id = Self::extract_resource_id_from_holder_key(&index_key, &prefix)?;
                if let Some(access) = self.get(&resource_id, holder)? {
                    results.push(access);
                }
            } else {
                // Optimized path: fetch data using the primary key stored in the index value
                if let Some(bytes) = self.store.get(&primary_key)? {
                    let access: ResourceAccess = serde_json::from_slice(&bytes)?;
                    results.push(access);
                }
            }
        }

        Ok(results)
    }

    fn list_by_resource(&self, resource_id: &str) -> anyhow::Result<Vec<ResourceAccess>> {
        let prefix = Self::resource_prefix(resource_id);
        let index_entries = self.store.scan(&prefix)?;

        let mut results = Vec::new();
        for (index_key, primary_key) in index_entries {
            // Index value contains the primary key; use it directly to fetch data.
            // Fall back to legacy behavior (extract from index key) for empty values
            // to maintain backward compatibility during migration.
            if primary_key.is_empty() {
                // Legacy empty index value: extract holder from key and do second lookup
                let holder = Self::extract_holder_from_resource_key(&index_key, &prefix)?;
                if let Some(access) = self.get(resource_id, &holder)? {
                    results.push(access);
                }
            } else {
                // Optimized path: fetch data using the primary key stored in the index value
                if let Some(bytes) = self.store.get(&primary_key)? {
                    let access: ResourceAccess = serde_json::from_slice(&bytes)?;
                    results.push(access);
                }
            }
        }

        Ok(results)
    }

    fn find_expired(&self, current_time: u64) -> anyhow::Result<Vec<ResourceAccess>> {
        let all_entries = self.store.scan(Self::ACCESS_PREFIX)?;

        let mut expired = Vec::new();
        for (key, value) in all_entries {
            // Skip index entries (they contain "idx:" in the key path)
            // Index entries store primary keys as values, not JSON data
            let key_str = String::from_utf8_lossy(&key);
            if key_str.contains(":idx:") {
                continue;
            }

            let access: ResourceAccess = serde_json::from_slice(&value)?;

            // Skip revoked records (already handled)
            if access.is_revoked() {
                continue;
            }

            if access.is_expired(current_time) {
                expired.push(access);
            }
        }

        Ok(expired)
    }

    fn find_idle(&self, current_time: u64, max_idle: u64) -> anyhow::Result<Vec<ResourceAccess>> {
        let all_entries = self.store.scan(Self::ACCESS_PREFIX)?;

        let mut idle = Vec::new();
        for (key, value) in all_entries {
            // Skip index entries (they contain "idx:" in the key path)
            let key_str = String::from_utf8_lossy(&key);
            if key_str.contains(":idx:") {
                continue;
            }

            let access: ResourceAccess = serde_json::from_slice(&value)?;

            // Skip revoked records (already handled)
            if access.is_revoked() {
                continue;
            }

            // Skip expired records (handled by find_expired)
            if access.is_expired(current_time) {
                continue;
            }

            if access.is_idle(current_time, max_idle) {
                idle.push(access);
            }
        }

        Ok(idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn create_test_entity() -> EntityId {
        let keypair = KeyPair::generate().unwrap();
        EntityId::from_did(keypair.did())
    }

    #[test]
    fn test_use_access_creation() {
        let entity = create_test_entity();
        let access = ResourceAccess::new(
            "tool-001".to_string(),
            entity.clone(),
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        assert_eq!(access.resource_id, "tool-001");
        assert_eq!(access.holder, entity);
        assert!(access.is_valid(access.granted_at));
        assert_eq!(access.renewal_count, 0);
    }

    #[test]
    fn test_use_access_expiration() {
        let entity = create_test_entity();
        let access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 3600, // 1 hour
                renewable: true,
                max_accumulated: 4,
            },
        );

        let current_time = access.granted_at;
        assert!(access.is_valid(current_time));
        assert!(!access.is_expired(current_time));

        // After duration passes
        let future_time = current_time + 3601;
        assert!(!access.is_valid(future_time));
        assert!(access.is_expired(future_time));
    }

    #[test]
    fn test_use_access_renewal() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 3600,
                renewable: true,
                max_accumulated: 2,
            },
        );

        let current_time = access.granted_at + 1800; // Halfway through

        // First renewal
        assert!(access.renew(current_time).is_ok());
        assert_eq!(access.renewal_count, 1);

        // Second renewal (hits max)
        assert!(access.renew(current_time).is_ok());
        assert_eq!(access.renewal_count, 2);

        // Third renewal should fail
        let result = access.renew(current_time);
        assert!(matches!(result, Err(AccessError::MaxAccumulatedReached(2))));
    }

    #[test]
    fn test_non_renewable_access() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 3600,
                renewable: false,
                max_accumulated: 1,
            },
        );

        let result = access.renew(access.granted_at);
        assert!(matches!(result, Err(AccessError::NotRenewable)));
    }

    #[test]
    fn test_usage_recording() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        let usage_time = access.granted_at + 100;
        let result = access.record_usage(usage_time, "Used for repairs".to_string());
        assert!(result.is_ok());
        assert_eq!(access.usage_log.len(), 1);
        assert_eq!(access.usage_log[0].description, "Used for repairs");
    }

    #[test]
    fn test_idle_detection() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        // No usage - should be idle after threshold
        let current_time = access.granted_at + 8 * 24 * 3600; // 8 days later
        assert!(access.is_idle(current_time, 7 * 24 * 3600)); // 7-day threshold

        // Record usage
        access
            .record_usage(current_time, "Used tool".to_string())
            .unwrap();
        assert!(!access.is_idle(current_time, 7 * 24 * 3600));

        // Idle again after another 8 days
        let future_time = current_time + 8 * 24 * 3600;
        assert!(access.is_idle(future_time, 7 * 24 * 3600));
    }

    #[test]
    fn test_anti_speculation_validation() {
        let entity = create_test_entity();
        let access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        )
        .with_rules(AntiSpeculationRules {
            max_idle_period_seconds: 7 * 24 * 3600,
            no_profit_transfer: true,
        });

        // Within idle period - valid
        let current_time = access.granted_at + 6 * 24 * 3600;
        assert!(access.validate_rules(current_time).is_ok());

        // Beyond idle period - invalid
        let future_time = access.granted_at + 8 * 24 * 3600;
        let result = access.validate_rules(future_time);
        assert!(matches!(result, Err(AccessError::IdleTooLong { .. })));
    }

    #[test]
    fn test_no_profit_transfer() {
        let entity = create_test_entity();
        let access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        )
        .with_rules(AntiSpeculationRules {
            max_idle_period_seconds: 7 * 24 * 3600,
            no_profit_transfer: true,
        });

        // Free transfer OK
        assert!(access.validate_transfer(None).is_ok());
        assert!(access.validate_transfer(Some(0)).is_ok());

        // Paid transfer not allowed
        let result = access.validate_transfer(Some(100));
        assert!(matches!(result, Err(AccessError::ProfitTransferNotAllowed)));
    }

    #[test]
    fn test_stewardship_model() {
        let entity = create_test_entity();
        let access = ResourceAccess::new(
            "community-garden".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![
                    StewardshipDuty::Maintenance {
                        description: "Water plants weekly".to_string(),
                        frequency_seconds: 7 * 24 * 3600,
                    },
                    StewardshipDuty::UsageReporting {
                        min_reports: 4,
                        period_seconds: 30 * 24 * 3600,
                    },
                ],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Stewardship doesn't expire by time
        assert!(access.expires_at.is_none());
        assert!(access.is_valid(access.granted_at + 365 * 24 * 3600));
    }

    #[test]
    fn test_stewardship_maintenance_duty() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "community-garden".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::Maintenance {
                    description: "Water plants".to_string(),
                    frequency_seconds: 7 * 24 * 3600, // Weekly
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // No maintenance yet - should fail after frequency passes
        let current_time = access.granted_at + 8 * 24 * 3600; // 8 days later
        let result = access.check_duties(current_time);
        assert!(matches!(result, Err(AccessError::DutyUnfulfilled(_))));

        // Record maintenance
        let maintenance_time = access.granted_at + 6 * 24 * 3600;
        access
            .record_usage(maintenance_time, "Performed maintenance".to_string())
            .unwrap();

        // Should pass now
        assert!(access.check_duties(maintenance_time + 3600).is_ok());
    }

    #[test]
    fn test_stewardship_reporting_duty() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "community-garden".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::UsageReporting {
                    min_reports: 4,
                    period_seconds: 30 * 24 * 3600, // Monthly
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        let current_time = access.granted_at + 15 * 24 * 3600; // Halfway through month

        // Record only 3 reports
        for i in 0..3 {
            access
                .record_usage(
                    access.granted_at + i * 24 * 3600,
                    format!("Usage report {}", i),
                )
                .unwrap();
        }

        // Should fail (need 4 reports)
        let result = access.check_duties(current_time);
        assert!(matches!(result, Err(AccessError::DutyUnfulfilled(_))));

        // Add 4th report
        access
            .record_usage(current_time, "Usage report 4".to_string())
            .unwrap();

        // Should pass now
        assert!(access.check_duties(current_time).is_ok());
    }

    #[test]
    fn test_usage_log_bounded() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 365 * 24 * 3600,
                renewable: true,
                max_accumulated: 10,
            },
        );

        // Record more than MAX_USAGE_LOG_SIZE events
        for i in 0..1500 {
            let time = access.granted_at + i * 3600;
            access.record_usage(time, format!("Usage {}", i)).unwrap();
        }

        // Log should be bounded
        assert_eq!(access.usage_log.len(), ResourceAccess::MAX_USAGE_LOG_SIZE);

        // Oldest entries should be removed (first entries should be gone)
        assert!(!access.usage_log[0].description.contains("Usage 0"));
    }

    #[test]
    fn test_stewardship_community_benefit_provided_before_deadline() {
        let entity = create_test_entity();
        let granted_at = 1000;
        let deadline = granted_at + 30 * 24 * 3600; // 30 days

        let mut access = ResourceAccess::new(
            "community-center".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::CommunityBenefit {
                    description: "Host workshop".to_string(),
                    due_by: deadline,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );
        access.granted_at = granted_at;

        // Provide benefit before deadline
        let benefit_time = granted_at + 20 * 24 * 3600;
        access
            .record_usage(benefit_time, "Host workshop for community".to_string())
            .unwrap();

        // Check after deadline - should pass because benefit was provided
        let check_time = deadline + 3600;
        assert!(access.check_duties(check_time).is_ok());
    }

    #[test]
    fn test_stewardship_community_benefit_not_provided_by_deadline() {
        let entity = create_test_entity();
        let granted_at = 1000;
        let deadline = granted_at + 30 * 24 * 3600; // 30 days

        let access = ResourceAccess::new(
            "community-center".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::CommunityBenefit {
                    description: "Host workshop".to_string(),
                    due_by: deadline,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Check at deadline without providing benefit - should fail
        let result = access.check_duties(deadline);
        assert!(
            matches!(result, Err(AccessError::DutyUnfulfilled(msg)) if msg.contains("Host workshop"))
        );
    }

    #[test]
    fn test_stewardship_community_benefit_before_deadline_no_check() {
        let entity = create_test_entity();
        let granted_at = 1000;
        let deadline = granted_at + 30 * 24 * 3600; // 30 days

        let access = ResourceAccess::new(
            "community-center".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::CommunityBenefit {
                    description: "Host workshop".to_string(),
                    due_by: deadline,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Check before deadline without providing benefit - should still pass
        // (duty only enforced at/after deadline)
        let check_time = deadline - 3600;
        assert!(access.check_duties(check_time).is_ok());
    }

    #[test]
    fn test_stewardship_community_benefit_case_insensitive() {
        let entity = create_test_entity();
        let granted_at = 1000;
        let deadline = granted_at + 30 * 24 * 3600;

        let mut access = ResourceAccess::new(
            "community-center".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::CommunityBenefit {
                    description: "Host Workshop".to_string(), // Mixed case
                    due_by: deadline,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );
        access.granted_at = granted_at;

        // Record with different case
        let benefit_time = granted_at + 20 * 24 * 3600;
        access
            .record_usage(benefit_time, "HOST WORKSHOP session completed".to_string())
            .unwrap();

        // Should pass due to case-insensitive matching
        let check_time = deadline + 3600;
        assert!(access.check_duties(check_time).is_ok());
    }

    #[test]
    fn test_renewal_time_travel_rejected() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 5,
            },
        );

        // Try to renew with time before grant - should fail
        let past_time = access.granted_at - 1000;
        let result = access.renew(past_time);
        assert!(matches!(
            result,
            Err(AccessError::InvalidTimestamp { provided, minimum })
            if provided == past_time && minimum == access.granted_at
        ));
    }

    #[test]
    fn test_revocation_prevents_usage() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        assert!(!access.is_revoked());

        // Revoke access
        access.revoke("Failed duty check".to_string());
        assert!(access.is_revoked());

        // Try to record usage - should fail
        let result = access.record_usage(access.granted_at + 100, "Usage attempt".to_string());
        assert!(
            matches!(result, Err(AccessError::Revoked(msg)) if msg.contains("Failed duty check"))
        );
    }

    #[test]
    fn test_stewardship_review_period() {
        let entity = create_test_entity();
        let granted_at = 1000;
        let review_period = 90 * 24 * 3600; // 90 days

        let mut access = ResourceAccess::new(
            "community-garden".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![],
                review_period_seconds: review_period,
            },
        );
        access.granted_at = granted_at;

        // Within review period - should be OK
        let check_time = granted_at + 80 * 24 * 3600; // 80 days
        assert!(access.check_review_period(check_time).is_ok());
        assert!(!access.needs_review(check_time));

        // After review period - should fail
        let overdue_time = granted_at + 91 * 24 * 3600; // 91 days
        let result = access.check_review_period(overdue_time);
        assert!(matches!(result, Err(AccessError::ReviewOverdue { .. })));
        assert!(access.needs_review(overdue_time));

        // Record a review
        access.record_review(overdue_time);
        assert!(!access.needs_review(overdue_time));
        assert!(access.check_review_period(overdue_time).is_ok());

        // Overdue again after another review period
        let later_time = overdue_time + 91 * 24 * 3600;
        assert!(access.needs_review(later_time));
    }

    #[test]
    fn test_use_access_no_review_period() {
        let entity = create_test_entity();
        let access = ResourceAccess::new(
            "tool-001".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        // UseAccess never needs review
        assert!(!access.needs_review(access.granted_at + 365 * 24 * 3600));
        assert!(access
            .check_review_period(access.granted_at + 365 * 24 * 3600)
            .is_ok());
    }

    #[test]
    fn test_stewardship_handoff_procedure() {
        // HandoffProcedure defines steps that must be completed when
        // transferring stewardship. This test documents the expected structure.
        let entity = create_test_entity();
        let handoff_steps = vec![
            "Document all ongoing maintenance tasks".to_string(),
            "Meet with incoming steward for orientation".to_string(),
            "Transfer access credentials securely".to_string(),
            "Notify community of transition".to_string(),
        ];

        let access = ResourceAccess::new(
            "community-space".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: handoff_steps.clone(),
                    witnesses: None,
                    min_witness_signatures: None,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Verify the handoff steps are stored in the access model
        if let AccessModel::Stewardship { duties, .. } = &access.model {
            let handoff_duty = duties
                .iter()
                .find(|d| matches!(d, StewardshipDuty::HandoffProcedure { .. }));
            assert!(handoff_duty.is_some());

            if let Some(StewardshipDuty::HandoffProcedure { steps, .. }) = handoff_duty {
                assert_eq!(steps.len(), 4);
                assert!(steps[0].contains("Document"));
                assert!(steps[3].contains("community"));
            }
        } else {
            panic!("Expected Stewardship model");
        }

        // HandoffProcedure is not checked in check_duties() - it's validated
        // at transfer time (when ResourceAccessStore.transfer() is called).
        // This allows the regular duty checks to pass while handoff is pending.
        assert!(access.check_duties(access.granted_at + 1000).is_ok());
    }

    #[test]
    fn test_structured_maintenance_event() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "community-garden".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::Maintenance {
                    description: "Water plants".to_string(),
                    frequency_seconds: 7 * 24 * 3600, // Weekly
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Without maintenance event, should fail after frequency passes
        let current_time = access.granted_at + 8 * 24 * 3600;
        assert!(access.check_duties(current_time).is_err());

        // Record structured maintenance event
        let maintenance_time = access.granted_at + 6 * 24 * 3600;
        access.usage_log.push_back(UsageEvent::with_duty_type(
            maintenance_time,
            "Completed weekly maintenance".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        ));

        // Should pass now
        assert!(access.check_duties(maintenance_time + 3600).is_ok());
    }

    #[test]
    fn test_structured_community_benefit_event() {
        let entity = create_test_entity();
        let granted_at = 1000;
        let deadline = granted_at + 30 * 24 * 3600;

        let mut access = ResourceAccess::new(
            "community-center".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::CommunityBenefit {
                    description: "Host workshop".to_string(),
                    due_by: deadline,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );
        access.granted_at = granted_at;

        // Record structured community benefit event (no keyword needed!)
        let benefit_time = granted_at + 20 * 24 * 3600;
        access.usage_log.push_back(UsageEvent::with_duty_type(
            benefit_time,
            "Held meeting for neighbors".to_string(), // Different description
            DutyEventType::CommunityBenefit { duty_id: None },
        ));

        // Should pass because we used structured duty type
        let check_time = deadline + 3600;
        assert!(access.check_duties(check_time).is_ok());
    }

    #[test]
    fn test_structured_report_event() {
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "community-garden".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::UsageReporting {
                    min_reports: 2,
                    period_seconds: 30 * 24 * 3600,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        let current_time = access.granted_at + 15 * 24 * 3600;

        // Add structured report events
        access.usage_log.push_back(UsageEvent::with_duty_type(
            access.granted_at + 5 * 24 * 3600,
            "Weekly status update".to_string(),
            DutyEventType::Report,
        ));
        access.usage_log.push_back(UsageEvent::with_duty_type(
            access.granted_at + 10 * 24 * 3600,
            "Progress report".to_string(),
            DutyEventType::Report,
        ));

        // Should pass with 2 report events
        assert!(access.check_duties(current_time).is_ok());
    }

    #[test]
    fn test_usage_event_with_duty_type_constructor() {
        let event = UsageEvent::with_duty_type(
            1234567890,
            "Maintenance completed".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        );

        assert_eq!(event.timestamp, 1234567890);
        assert_eq!(event.description, "Maintenance completed");
        assert!(event.witnesses.is_empty());
        assert_eq!(
            event.duty_type,
            Some(DutyEventType::Maintenance { duty_id: None })
        );
    }

    #[test]
    fn test_usage_event_with_witnesses() {
        let witness1 = "did:icn:abc123".to_string();
        let witness2 = "did:icn:def456".to_string();

        let event = UsageEvent::with_duty_type(
            1234567890,
            "Maintenance completed".to_string(),
            DutyEventType::Maintenance {
                duty_id: Some("maint-001".to_string()),
            },
        )
        .with_witness(witness1.clone())
        .with_witness(witness2.clone());

        assert_eq!(event.witnesses.len(), 2);
        assert_eq!(event.witnesses[0], witness1);
        assert_eq!(event.witnesses[1], witness2);
        assert!(event.has_sufficient_witnesses(2));
        assert!(!event.has_sufficient_witnesses(3));
    }

    #[test]
    fn test_witness_deduplication_prevents_gaming() {
        // Verify that duplicate witnesses are not counted multiple times
        // This prevents gaming by adding the same witness DID repeatedly
        let same_witness = "did:icn:abc123".to_string();

        let event = UsageEvent::with_duty_type(
            1234567890,
            "Maintenance completed".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        )
        .with_witness(same_witness.clone())
        .with_witness(same_witness.clone())
        .with_witness(same_witness.clone());

        // Raw witness count is 3, but unique count should be 1
        assert_eq!(event.witnesses.len(), 3);
        assert!(event.has_sufficient_witnesses(1));
        // Should fail because only 1 unique witness despite 3 entries
        assert!(!event.has_sufficient_witnesses(2));
        assert!(!event.has_sufficient_witnesses(3));
    }

    #[test]
    fn test_validate_duty_completion_with_witnesses() {
        let entity = create_test_entity();
        let access = ResourceAccess::new(
            "community-garden".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::Maintenance {
                    description: "Water plants".to_string(),
                    frequency_seconds: 7 * 24 * 3600,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Event with 2 witnesses
        let event_with_witnesses = UsageEvent::with_duty_type(
            1234567890,
            "Maintenance completed".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        )
        .with_witness("did:icn:witness1".to_string())
        .with_witness("did:icn:witness2".to_string());

        // Event without witnesses
        let event_no_witnesses = UsageEvent::with_duty_type(
            1234567890,
            "Maintenance completed".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        );

        // Validate with different witness requirements
        assert!(access.validate_duty_completion(&event_with_witnesses, 1));
        assert!(access.validate_duty_completion(&event_with_witnesses, 2));
        assert!(!access.validate_duty_completion(&event_with_witnesses, 3));
        assert!(!access.validate_duty_completion(&event_no_witnesses, 1));
        assert!(access.validate_duty_completion(&event_no_witnesses, 0));
    }

    #[test]
    fn test_duty_event_type_variants() {
        // Test Maintenance with duty_id
        let maintenance_event = UsageEvent::with_duty_type(
            1000,
            "Weekly watering".to_string(),
            DutyEventType::Maintenance {
                duty_id: Some("garden-maint-1".to_string()),
            },
        );
        assert!(matches!(
            maintenance_event.duty_type,
            Some(DutyEventType::Maintenance { .. })
        ));

        // Test CommunityBenefit with duty_id
        let benefit_event = UsageEvent::with_duty_type(
            2000,
            "Workshop hosted".to_string(),
            DutyEventType::CommunityBenefit {
                duty_id: Some("workshop-2025".to_string()),
            },
        );
        assert!(matches!(
            benefit_event.duty_type,
            Some(DutyEventType::CommunityBenefit { .. })
        ));

        // Test HandoffStep
        let handoff_event = UsageEvent::with_duty_type(
            3000,
            "Documented current state".to_string(),
            DutyEventType::HandoffStep { step_index: 0 },
        );
        assert!(matches!(
            handoff_event.duty_type,
            Some(DutyEventType::HandoffStep { step_index: 0 })
        ));

        // Test GeneralUsage
        let general_event = UsageEvent::with_duty_type(
            4000,
            "Regular use".to_string(),
            DutyEventType::GeneralUsage,
        );
        assert!(matches!(
            general_event.duty_type,
            Some(DutyEventType::GeneralUsage)
        ));
    }

    #[test]
    fn test_convenience_constructors() {
        // Test maintenance convenience constructor
        let event = UsageEvent::maintenance(1000, "Weekly watering");
        assert!(matches!(
            event.duty_type,
            Some(DutyEventType::Maintenance { duty_id: None })
        ));
        assert_eq!(event.description, "Weekly watering");

        // Test report convenience constructor
        let event = UsageEvent::report(2000, "Monthly status update");
        assert!(matches!(event.duty_type, Some(DutyEventType::Report)));

        // Test community_benefit convenience constructor
        let event = UsageEvent::community_benefit(3000, "Workshop hosted");
        assert!(matches!(
            event.duty_type,
            Some(DutyEventType::CommunityBenefit { duty_id: None })
        ));

        // Test handoff_step convenience constructor
        let event = UsageEvent::handoff_step(4000, "Document state", 2);
        assert!(matches!(
            event.duty_type,
            Some(DutyEventType::HandoffStep { step_index: 2 })
        ));
    }

    #[test]
    fn test_witness_did_format_validation() {
        // Valid DIDs
        assert!(UsageEvent::is_valid_did_format("did:icn:abc123"));
        assert!(UsageEvent::is_valid_did_format("did:icn:node:abc123"));

        // Invalid DIDs
        assert!(!UsageEvent::is_valid_did_format("did:icn:")); // Empty identifier
        assert!(!UsageEvent::is_valid_did_format("did:key:abc123")); // Wrong method
        assert!(!UsageEvent::is_valid_did_format("abc123")); // Not a DID
        assert!(!UsageEvent::is_valid_did_format("")); // Empty
    }

    #[test]
    fn test_with_validated_witness() {
        let event = UsageEvent::maintenance(1000, "Task completed");

        // Valid witness
        let event = event.with_validated_witness("did:icn:witness1".to_string());
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.witnesses.len(), 1);

        // Invalid witness returns None
        let event = event.with_validated_witness("invalid".to_string());
        assert!(event.is_none());
    }

    #[test]
    fn test_backward_compat_deserialization() {
        // Simulate old JSON format with event_type (aliased to duty_type)
        let old_json = r#"{
            "timestamp": 1234567890,
            "description": "Old event",
            "witnesses": [],
            "event_type": "Report"
        }"#;

        let event: UsageEvent = serde_json::from_str(old_json).unwrap();
        assert_eq!(event.timestamp, 1234567890);
        assert_eq!(event.description, "Old event");
        assert!(matches!(event.duty_type, Some(DutyEventType::Report)));

        // New JSON format with duty_type works too
        let new_json = r#"{
            "timestamp": 1234567890,
            "description": "New event",
            "witnesses": [],
            "duty_type": "Report"
        }"#;

        let event: UsageEvent = serde_json::from_str(new_json).unwrap();
        assert!(matches!(event.duty_type, Some(DutyEventType::Report)));

        // Complex variant with duty_id
        let complex_json = r#"{
            "timestamp": 1234567890,
            "description": "Maintenance",
            "witnesses": ["did:icn:witness1"],
            "duty_type": { "Maintenance": { "duty_id": "maint-001" } }
        }"#;

        let event: UsageEvent = serde_json::from_str(complex_json).unwrap();
        assert!(matches!(
            event.duty_type,
            Some(DutyEventType::Maintenance {
                duty_id: Some(ref id)
            }) if id == "maint-001"
        ));
    }

    #[test]
    fn test_duty_check_with_large_log_early_termination() {
        // Test that demonstrates early termination optimization
        // Creates a large usage log with old events, and verifies that
        // duty checking correctly finds recent events without scanning the entire log
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "community-garden".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![
                    StewardshipDuty::Maintenance {
                        description: "Water plants".to_string(),
                        frequency_seconds: 7 * 24 * 3600, // Weekly
                    },
                    StewardshipDuty::UsageReporting {
                        min_reports: 2,
                        period_seconds: 30 * 24 * 3600, // Monthly
                    },
                ],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        let base_time = 1000u64;
        access.granted_at = base_time;

        // Add 500 older usage events (outside the 7-day maintenance window)
        // 500 events × 1 hour = ~20.8 days from base_time
        for i in 0..500 {
            let old_time = base_time + (i * 3600); // 1 hour apart
            access
                .usage_log
                .push_back(UsageEvent::new(old_time, format!("Old event {}", i)));
        }

        // Current time is 100 days after grant
        let current_time = base_time + (100 * 24 * 3600);

        // Add recent events in CHRONOLOGICAL order (oldest first)
        // This matches real-world usage where record_usage() appends events sequentially
        // The filter() approach handles out-of-order events correctly for robustness

        // Report 1 (10 days ago) - oldest of the recent events
        let report_time_1 = current_time - (10 * 24 * 3600);
        access.usage_log.push_back(UsageEvent::with_duty_type(
            report_time_1,
            "Report 1".to_string(),
            DutyEventType::Report,
        ));

        // Maintenance event (5 days ago)
        let maintenance_time = current_time - (5 * 24 * 3600);
        access.usage_log.push_back(UsageEvent::with_duty_type(
            maintenance_time,
            "Recent maintenance".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        ));

        // Report 2 (3 days ago) - most recent
        let report_time_2 = current_time - (3 * 24 * 3600);
        access.usage_log.push_back(UsageEvent::with_duty_type(
            report_time_2,
            "Report 2".to_string(),
            DutyEventType::Report,
        ));

        // Check duties should pass - the optimization allows early termination
        // once we encounter events older than the relevant time windows
        let result = access.check_duties(current_time);
        assert!(result.is_ok(), "check_duties failed: {:?}", result);

        // Verify that maintenance is still considered current
        assert!(!access.is_idle(current_time, 7 * 24 * 3600));
    }

    #[test]
    fn test_duty_check_with_out_of_order_events() {
        // Verifies that .filter() correctly handles out-of-order events
        // (e.g., from clock skew or manual insertion)
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "workshop-tools".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::Maintenance {
                    description: "Clean equipment".to_string(),
                    frequency_seconds: 7 * 24 * 3600, // Weekly
                }],
                review_period_seconds: 30 * 24 * 3600,
            },
        );

        let current_time = 100 * 24 * 3600u64; // 100 days in seconds
        access.granted_at = 0;

        // Insert events OUT OF ORDER to simulate clock skew:
        // Event 1: Recent maintenance (within window) - inserted first
        access.usage_log.push_back(UsageEvent::with_duty_type(
            current_time - (2 * 24 * 3600), // 2 days ago
            "Recent cleaning".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        ));

        // Event 2: Old event with EARLIER timestamp - inserted second (out of order)
        access.usage_log.push_back(UsageEvent::with_duty_type(
            current_time - (30 * 24 * 3600), // 30 days ago
            "Old event".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        ));

        // Event 3: Another old event
        access.usage_log.push_back(UsageEvent::with_duty_type(
            current_time - (20 * 24 * 3600), // 20 days ago
            "Another old event".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        ));

        // With .filter(), all events are checked regardless of order,
        // so the recent maintenance event (2 days ago) should be found
        let result = access.check_duties(current_time);
        assert!(
            result.is_ok(),
            "check_duties should find recent event despite out-of-order insertion: {:?}",
            result
        );
    }

    #[test]
    fn test_duty_check_with_saturating_cutoff() {
        // Verifies saturating_sub prevents underflow when current_time < frequency_seconds
        let entity = create_test_entity();
        let mut access = ResourceAccess::new(
            "new-resource".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::Maintenance {
                    description: "Maintain".to_string(),
                    frequency_seconds: 1_000_000, // Large frequency
                }],
                review_period_seconds: 2_000_000,
            },
        );

        // Very early timestamp (smaller than frequency_seconds)
        let current_time = 100u64;
        access.granted_at = 0;

        // Add maintenance event at time 50
        access.usage_log.push_back(UsageEvent::with_duty_type(
            50,
            "Early maintenance".to_string(),
            DutyEventType::Maintenance { duty_id: None },
        ));

        // cutoff_time = 100.saturating_sub(1_000_000) = 0
        // Event at timestamp 50 >= 0, so it should be found
        let result = access.check_duties(current_time);
        assert!(
            result.is_ok(),
            "check_duties should work with saturating cutoff: {:?}",
            result
        );
    }

    #[test]
    fn test_handoff_with_witness_requirements() {
        // Test HandoffProcedure with witness requirements
        let entity = create_test_entity();
        let witness1 = "did:icn:witness1".to_string();
        let witness2 = "did:icn:witness2".to_string();
        let handoff_steps = vec![
            "Document all ongoing tasks".to_string(),
            "Transfer credentials".to_string(),
        ];

        let mut access = ResourceAccess::new(
            "high-value-resource".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: handoff_steps.clone(),
                    witnesses: Some(vec![witness1.clone(), witness2.clone()]),
                    min_witness_signatures: Some(2),
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Without any handoff steps completed, validation should fail
        let result = access.validate_handoff_completion();
        assert!(result.is_err());
        assert!(matches!(result, Err(AccessError::DutyUnfulfilled(_))));

        // Complete first step with one witness - should fail (need 2)
        access.usage_log.push_back(
            UsageEvent::handoff_step(access.granted_at + 100, "Documented tasks", 0)
                .with_witness(witness1.clone()),
        );
        let result = access.validate_handoff_completion();
        assert!(result.is_err());

        // Complete first step with two witnesses - should still fail (second step incomplete)
        access.usage_log.pop_back();
        access.usage_log.push_back(
            UsageEvent::handoff_step(access.granted_at + 100, "Documented tasks", 0)
                .with_witness(witness1.clone())
                .with_witness(witness2.clone()),
        );
        let result = access.validate_handoff_completion();
        assert!(result.is_err());

        // Complete second step with two witnesses - should pass
        access.usage_log.push_back(
            UsageEvent::handoff_step(access.granted_at + 200, "Transferred credentials", 1)
                .with_witness(witness1.clone())
                .with_witness(witness2.clone()),
        );
        let result = access.validate_handoff_completion();
        assert!(
            result.is_ok(),
            "Should pass with all steps completed: {:?}",
            result
        );
    }

    #[test]
    fn test_handoff_without_witness_requirements() {
        // Test backward compatibility - HandoffProcedure without witness requirements
        let entity = create_test_entity();
        let handoff_steps = vec![
            "Document tasks".to_string(),
            "Transfer credentials".to_string(),
        ];

        let access = ResourceAccess::new(
            "community-tool".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: handoff_steps.clone(),
                    witnesses: None, // No witness requirement
                    min_witness_signatures: None,
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Should pass without any steps recorded (backward compatible)
        let result = access.validate_handoff_completion();
        assert!(
            result.is_ok(),
            "Should pass without witness requirements: {:?}",
            result
        );
    }

    #[test]
    fn test_handoff_min_witnesses_default() {
        // Test that min_witness_signatures defaults to all witnesses if not specified
        let entity = create_test_entity();
        let witness1 = "did:icn:witness1".to_string();
        let witness2 = "did:icn:witness2".to_string();
        let witness3 = "did:icn:witness3".to_string();

        let mut access = ResourceAccess::new(
            "resource".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: vec!["Step 1".to_string()],
                    witnesses: Some(vec![witness1.clone(), witness2.clone(), witness3.clone()]),
                    min_witness_signatures: None, // Should default to 3 (all witnesses)
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // With only 2 witnesses - should fail
        access.usage_log.push_back(
            UsageEvent::handoff_step(access.granted_at + 100, "Step 1", 0)
                .with_witness(witness1.clone())
                .with_witness(witness2.clone()),
        );
        let result = access.validate_handoff_completion();
        assert!(
            result.is_err(),
            "Should fail with only 2 of 3 required witnesses"
        );

        // With all 3 witnesses - should pass
        access.usage_log.pop_back();
        access.usage_log.push_back(
            UsageEvent::handoff_step(access.granted_at + 100, "Step 1", 0)
                .with_witness(witness1.clone())
                .with_witness(witness2.clone())
                .with_witness(witness3.clone()),
        );
        let result = access.validate_handoff_completion();
        assert!(result.is_ok(), "Should pass with all 3 witnesses");
    }

    #[test]
    fn test_handoff_validation_with_min_subset() {
        // Test that min_witness_signatures can be less than total witnesses
        let entity = create_test_entity();
        let witness1 = "did:icn:witness1".to_string();
        let witness2 = "did:icn:witness2".to_string();
        let witness3 = "did:icn:witness3".to_string();

        let mut access = ResourceAccess::new(
            "resource".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: vec!["Step 1".to_string()],
                    witnesses: Some(vec![witness1.clone(), witness2.clone(), witness3.clone()]),
                    min_witness_signatures: Some(2), // Only need 2 of 3
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // With 2 witnesses (any 2) - should pass
        access.usage_log.push_back(
            UsageEvent::handoff_step(access.granted_at + 100, "Step 1", 0)
                .with_witness(witness1.clone())
                .with_witness(witness3.clone()), // witness2 not included
        );
        let result = access.validate_handoff_completion();
        assert!(result.is_ok(), "Should pass with 2 of 3 required witnesses");
    }

    #[test]
    fn test_handoff_non_stewardship_access() {
        // Test that non-stewardship access doesn't require handoff validation
        let entity = create_test_entity();
        let access = ResourceAccess::new(
            "tool".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        // Should always pass for UseAccess
        let result = access.validate_handoff_completion();
        assert!(
            result.is_ok(),
            "Non-stewardship access should not require handoff"
        );
    }

    #[test]
    fn test_handoff_duplicate_witnesses_not_counted() {
        // Test that duplicate witnesses are not counted (gaming prevention)
        let entity = create_test_entity();
        let witness1 = "did:icn:witness1".to_string();

        let mut access = ResourceAccess::new(
            "resource".to_string(),
            entity,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: vec!["Step 1".to_string()],
                    witnesses: Some(vec![witness1.clone()]),
                    min_witness_signatures: Some(2), // Need 2 unique witnesses
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Add same witness twice - should fail (only counts as 1)
        access.usage_log.push_back(
            UsageEvent::handoff_step(access.granted_at + 100, "Step 1", 0)
                .with_witness(witness1.clone())
                .with_witness(witness1.clone()), // Duplicate
        );
        let result = access.validate_handoff_completion();
        assert!(result.is_err(), "Duplicate witnesses should not be counted");
    }
}
