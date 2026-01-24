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
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use thiserror::Error;
use tracing::error;

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

    /// Storage backend error
    #[error("Storage error: {0}")]
    StorageError(String),
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
    },
}

/// Type of duty-related event for structured validation
///
/// Using structured event types instead of keyword matching provides:
/// - Type-safe duty verification
/// - O(1) matching instead of O(n) string search
/// - Clear documentation of expected event categories
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DutyEventType {
    /// Maintenance task completion
    Maintenance,
    /// Usage or status report
    Report,
    /// Community benefit provided
    CommunityBenefit,
    /// General usage (not duty-specific)
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
    /// Structured event type for duty validation
    /// When None, falls back to keyword-based matching for backward compatibility
    pub event_type: Option<DutyEventType>,
}

impl UsageEvent {
    /// Create a new usage event
    pub fn new(timestamp: u64, description: String) -> Self {
        Self {
            timestamp,
            description,
            witnesses: Vec::new(),
            event_type: None,
        }
    }

    /// Create a new usage event with structured event type
    pub fn with_type(timestamp: u64, description: String, event_type: DutyEventType) -> Self {
        Self {
            timestamp,
            description,
            witnesses: Vec::new(),
            event_type: Some(event_type),
        }
    }

    /// Add a witness to this event
    pub fn with_witness(mut self, witness_did: String) -> Self {
        self.witnesses.push(witness_did);
        self
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

    /// Timestamp of last transfer (for audit trail)
    /// Set when access is transferred to a new holder via ResourceAccessStore::transfer()
    #[serde(default)]
    pub last_transferred_at: Option<u64>,
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
            last_transferred_at: None,
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
        // Check if access has been revoked
        if self.revoked {
            return Err(AccessError::Revoked(
                self.revocation_reason
                    .clone()
                    .unwrap_or_else(|| "Access revoked".to_string()),
            ));
        }

        // Validate access is still valid (for UseAccess expiration)
        if self.is_expired(timestamp) {
            return Err(AccessError::Expired(self.expires_at.unwrap_or(0)));
        }

        let event = UsageEvent::new(timestamp, description);
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
    pub fn check_duties(&self, current_time: u64) -> Result<()> {
        match &self.model {
            AccessModel::Stewardship { duties, .. } => {
                for duty in duties {
                    match duty {
                        StewardshipDuty::Maintenance {
                            description,
                            frequency_seconds,
                        } => {
                            // Find last maintenance event
                            // Priority: structured event type > keyword matching
                            let last_maintenance = self
                                .usage_log
                                .iter()
                                .rev()
                                .find(|e| {
                                    // First check structured event type (O(1))
                                    if let Some(event_type) = &e.event_type {
                                        matches!(event_type, DutyEventType::Maintenance)
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
                            let period_start = current_time.saturating_sub(*period_seconds);
                            let report_count = self
                                .usage_log
                                .iter()
                                .filter(|e| {
                                    if e.timestamp < period_start {
                                        return false;
                                    }
                                    // Count structured Report events or any event for backward compat
                                    if let Some(event_type) = &e.event_type {
                                        matches!(event_type, DutyEventType::Report)
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
                            // Priority: structured event type > keyword matching
                            let description_lower = description.to_lowercase();
                            let benefit_provided = self.usage_log.iter().any(|e| {
                                // First check structured event type
                                if let Some(event_type) = &e.event_type {
                                    matches!(event_type, DutyEventType::CommunityBenefit)
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
    /// Validate transfer price against anti-speculation rules
    ///
    /// Returns error if price > 0 and no_profit_transfer is enabled.
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

    /// Validate all preconditions for transferring this access to a new holder
    ///
    /// This method performs comprehensive validation including:
    /// 1. Access is not revoked
    /// 2. Transfer price complies with no-profit rules
    /// 3. HandoffProcedure steps are completed (for Stewardship)
    ///
    /// Handoff validation uses exact segment matching to prevent gaming.
    /// For example, logging "Failed to transfer credentials" will NOT satisfy
    /// a step requiring "Transfer credentials".
    ///
    /// # Arguments
    /// * `price` - Optional transfer price for validation
    ///
    /// # Returns
    /// * `Ok(())` if all preconditions are met
    /// * `Err(AccessError)` describing the validation failure
    pub fn validate_transfer_preconditions(&self, price: Option<i64>) -> Result<()> {
        // Check access is not revoked
        if self.revoked {
            return Err(AccessError::Revoked(
                self.revocation_reason
                    .clone()
                    .unwrap_or_else(|| "Access revoked".to_string()),
            ));
        }

        // Validate transfer price (no-profit rule)
        self.validate_transfer(price)?;

        // For Stewardship, validate HandoffProcedure steps are completed
        if let AccessModel::Stewardship { duties, .. } = &self.model {
            self.validate_handoff_steps(duties)?;
        }

        Ok(())
    }

    /// Validate that all required handoff steps have been completed
    ///
    /// Uses exact segment matching: the step text must appear as a complete
    /// segment (separated by common delimiters) in a usage log entry.
    fn validate_handoff_steps(&self, duties: &[StewardshipDuty]) -> Result<()> {
        // Find HandoffProcedure duty if it exists
        let handoff_duty = duties.iter().find_map(|d| match d {
            StewardshipDuty::HandoffProcedure { steps } => Some(steps),
            _ => None,
        });

        if let Some(required_steps) = handoff_duty {
            for step in required_steps {
                if !self.is_handoff_step_completed(step) {
                    return Err(AccessError::DutyUnfulfilled(format!(
                        "Handoff step not completed: {}",
                        step
                    )));
                }
            }
        }

        Ok(())
    }

    /// Check if a specific handoff step has been completed
    ///
    /// Uses exact segment matching to prevent gaming. The step must appear
    /// as a complete segment in the usage log, not just as a substring.
    fn is_handoff_step_completed(&self, step: &str) -> bool {
        let step_lower = step.to_lowercase();
        self.usage_log.iter().any(|event| {
            let desc_lower = event.description.to_lowercase();
            // Exact match on full description
            if desc_lower == step_lower {
                return true;
            }
            // Segment match: step appears as complete segment between delimiters
            desc_lower
                .split(&[',', ';', '.', '-', ':', '|'][..])
                .any(|segment| segment.trim() == step_lower)
        })
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

// =============================================================================
// ResourceAccessStore - Persistent storage for resource access records
// =============================================================================

/// Key prefix for primary resource access records
const ACCESS_PREFIX: &str = "resource_access:data:";

/// Key prefix for holder index entries (maps holder -> list of resource_ids)
const HOLDER_INDEX_PREFIX: &str = "resource_access:holder_index:";

/// Deserialize holder index with error logging
///
/// Returns empty vec on deserialization failure but logs the error for
/// corruption detection. This allows graceful degradation while maintaining
/// visibility into data integrity issues.
fn deserialize_holder_index(bytes: &[u8], context: &str) -> Vec<String> {
    match serde_json::from_slice(bytes) {
        Ok(index) => index,
        Err(e) => {
            error!(
                context = %context,
                error = %e,
                bytes_len = bytes.len(),
                "Failed to deserialize holder index - possible data corruption"
            );
            Vec::new()
        }
    }
}

/// Trait for storing and managing resource access records
///
/// Provides CRUD operations for [`ResourceAccess`] records with support for
/// efficient querying by holder via secondary indexes.
///
/// # Implementations
///
/// - [`SledResourceAccessStore`]: Production-ready Sled-backed implementation
///
/// # Implementation Requirements
///
/// **Implementers MUST maintain the holder index** to ensure `list_by_holder()` works correctly:
///
/// - `put()`: Add resource_id to the new holder's index; if holder changed, remove from old holder's index
/// - `remove()`: Remove resource_id from the holder's index
/// - `transfer()`: Remove from old holder's index, add to new holder's index
///
/// The provided default `transfer()` relies on `put()` to maintain indexes. If your implementation
/// uses a different indexing strategy, you MUST override `transfer()` to maintain consistency.
///
/// # Index Retention Policy
///
/// Index entries (holder -> resource mappings) are intentionally retained even when
/// access is revoked. This design choice enables:
///
/// 1. **Audit trails**: Historical queries can reconstruct who held access to what resources
/// 2. **Dispute resolution**: Revocation reasons and timestamps remain queryable
/// 3. **Analytics**: Usage patterns and access history inform governance decisions
///
/// Callers should filter by `revoked` status when listing active access. The `list_by_holder`
/// method returns all records (including revoked) to support audit use cases.
///
/// To permanently remove a record and its indexes, use `remove()` instead of revocation.
///
/// # Example
///
/// ```rust,ignore
/// use icn_ledger::use_access::{ResourceAccessStore, SledResourceAccessStore, ResourceAccess};
///
/// let store = SledResourceAccessStore::new(sled_backend);
///
/// // Grant access
/// store.put(&access)?;
///
/// // Query by holder
/// let resources = store.list_by_holder(&holder_id)?;
///
/// // Transfer to new holder
/// let (old_holder, updated) = store.transfer("resource-1", new_holder, None, timestamp)?;
/// ```
pub trait ResourceAccessStore: Send + Sync {
    /// Get resource access by resource_id
    ///
    /// # Returns
    /// - `Ok(Some(access))` if the resource exists
    /// - `Ok(None)` if the resource does not exist
    /// - `Err` on storage errors
    fn get(&self, resource_id: &str) -> anyhow::Result<Option<ResourceAccess>>;

    /// Store or update resource access
    ///
    /// Creates or updates the primary record and maintains holder index consistency.
    /// If the holder index update fails, a best-effort rollback of the primary record
    /// is attempted. Rollback failures are logged but do not change the returned error.
    fn put(&self, access: &ResourceAccess) -> anyhow::Result<()>;

    /// Remove resource access and clean up all associated indexes
    ///
    /// This permanently deletes the record. For audit-preserving removal, use
    /// `ResourceAccess::revoke()` followed by `put()` instead.
    fn remove(&self, resource_id: &str) -> anyhow::Result<()>;

    /// List all resource access records for a given holder
    ///
    /// Returns all records including revoked ones to support audit queries.
    /// Filter by `!access.revoked` for active access only.
    ///
    /// # Performance Note
    ///
    /// The default implementation uses an N+1 query pattern: one query to get
    /// the holder's resource ID list, then one query per resource to fetch details.
    /// This is acceptable for typical workloads (< 50 resources per holder) but
    /// may need optimization for holders with many resources. Consider monitoring
    /// via metrics and implementing batch loading if needed.
    fn list_by_holder(&self, holder: &EntityId) -> anyhow::Result<Vec<ResourceAccess>>;

    /// Transfer resource access to a new holder
    ///
    /// This is a default implementation that validates preconditions and persists
    /// the transfer. Implementations with secondary indexes (like `SledResourceAccessStore`)
    /// should override this to maintain index consistency.
    ///
    /// # Validation
    /// Uses `ResourceAccess::validate_transfer_preconditions()` which enforces:
    /// 1. Access is not revoked
    /// 2. No-profit transfer rules
    /// 3. HandoffProcedure completion (for Stewardship access)
    ///
    /// # Arguments
    /// * `resource_id` - The resource to transfer
    /// * `new_holder` - Entity receiving access
    /// * `price` - Optional transfer price (for validation)
    /// * `current_time` - Transfer timestamp for audit trail
    ///
    /// # Returns
    /// A tuple of (old_holder, updated ResourceAccess with new holder)
    fn transfer(
        &self,
        resource_id: &str,
        new_holder: EntityId,
        price: Option<i64>,
        current_time: u64,
    ) -> Result<(EntityId, ResourceAccess)> {
        // Get existing access
        let mut access = self
            .get(resource_id)
            .map_err(|e| AccessError::StorageError(e.to_string()))?
            .ok_or_else(|| AccessError::Revoked(format!("Resource {} not found", resource_id)))?;

        // Validate all transfer preconditions
        access.validate_transfer_preconditions(price)?;

        // Store old holder for event emission
        let old_holder = access.holder.clone();

        // Update holder and record transfer timestamp for audit trail
        access.holder = new_holder;
        access.last_transferred_at = Some(current_time);

        // Persist the updated access
        self.put(&access)
            .map_err(|e| AccessError::StorageError(e.to_string()))?;

        Ok((old_holder, access))
    }
}

/// Sled-backed resource access store
pub struct SledResourceAccessStore {
    store: Arc<dyn Store>,
}

impl SledResourceAccessStore {
    /// Create a new resource access store
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    fn access_key(resource_id: &str) -> Vec<u8> {
        format!("{ACCESS_PREFIX}{}", resource_id).into_bytes()
    }

    fn holder_index_key(holder: &EntityId) -> Vec<u8> {
        format!("{HOLDER_INDEX_PREFIX}{}", holder).into_bytes()
    }
}

impl ResourceAccessStore for SledResourceAccessStore {
    fn get(&self, resource_id: &str) -> anyhow::Result<Option<ResourceAccess>> {
        let key = Self::access_key(resource_id);
        match self.store.get(&key)? {
            Some(bytes) => {
                let access: ResourceAccess = serde_json::from_slice(&bytes)?;
                Ok(Some(access))
            }
            None => Ok(None),
        }
    }

    fn put(&self, access: &ResourceAccess) -> anyhow::Result<()> {
        let key = Self::access_key(&access.resource_id);
        let bytes = serde_json::to_vec(access)?;
        self.store.put(&key, &bytes)?;

        // Update holder index for list_by_holder queries
        let holder_key = Self::holder_index_key(&access.holder);
        let mut holder_resources: Vec<String> = self
            .store
            .get(&holder_key)?
            .map(|b| deserialize_holder_index(&b, &format!("put:{}", access.holder)))
            .unwrap_or_default();

        if !holder_resources.contains(&access.resource_id) {
            holder_resources.push(access.resource_id.clone());
            let holder_bytes = serde_json::to_vec(&holder_resources)?;
            if let Err(index_err) = self.store.put(&holder_key, &holder_bytes) {
                // Best-effort rollback: try to delete the primary record
                // to maintain consistency between primary and index
                if let Err(rollback_err) = self.store.delete(&key) {
                    error!(
                        resource_id = %access.resource_id,
                        index_error = %index_err,
                        rollback_error = %rollback_err,
                        "Failed to rollback primary record after holder index update failed. \
                         Database may be in inconsistent state."
                    );
                }
                return Err(index_err);
            }
        }

        Ok(())
    }

    fn remove(&self, resource_id: &str) -> anyhow::Result<()> {
        // Get the access to remove it from holder index
        if let Some(access) = self.get(resource_id)? {
            let holder_key = Self::holder_index_key(&access.holder);
            let mut holder_resources: Vec<String> = self
                .store
                .get(&holder_key)?
                .map(|b| deserialize_holder_index(&b, &format!("remove:{}", access.holder)))
                .unwrap_or_default();

            holder_resources.retain(|r| r != resource_id);
            let holder_bytes = serde_json::to_vec(&holder_resources)?;
            self.store.put(&holder_key, &holder_bytes)?;
        }

        let key = Self::access_key(resource_id);
        self.store.delete(&key)?;
        Ok(())
    }

    fn list_by_holder(&self, holder: &EntityId) -> anyhow::Result<Vec<ResourceAccess>> {
        let holder_key = Self::holder_index_key(holder);
        let resource_ids: Vec<String> = self
            .store
            .get(&holder_key)?
            .map(|b| deserialize_holder_index(&b, &format!("list_by_holder:{}", holder)))
            .unwrap_or_default();

        let mut accesses = Vec::new();
        for resource_id in resource_ids {
            if let Some(access) = self.get(&resource_id)? {
                accesses.push(access);
            }
        }
        Ok(accesses)
    }

    /// Transfer resource access to a new holder with index maintenance.
    ///
    /// # Transaction Boundary Warning
    ///
    /// This operation performs three sequential writes that are NOT atomic:
    /// 1. Remove resource from old holder's index
    /// 2. Update the resource access record (holder + timestamp)
    /// 3. Add resource to new holder's index (via `put()`)
    ///
    /// If step 3 fails after step 1 succeeds, the resource will be temporarily
    /// orphaned (not in either holder's index). The resource data remains intact
    /// and can be recovered by calling `put()` again. For applications requiring
    /// stronger consistency, consider wrapping this in application-level retry logic.
    ///
    /// A future enhancement could use sled's transactional API for true atomicity.
    fn transfer(
        &self,
        resource_id: &str,
        new_holder: EntityId,
        price: Option<i64>,
        current_time: u64,
    ) -> Result<(EntityId, ResourceAccess)> {
        // Get existing access
        let mut access = self
            .get(resource_id)
            .map_err(|e| AccessError::StorageError(e.to_string()))?
            .ok_or_else(|| AccessError::Revoked(format!("Resource {} not found", resource_id)))?;

        // Validate all transfer preconditions using shared helper
        access.validate_transfer_preconditions(price)?;

        // Store old holder for event emission and index cleanup
        let old_holder = access.holder.clone();

        // Remove resource from old holder's index
        let old_holder_key = Self::holder_index_key(&old_holder);
        let mut old_holder_resources: Vec<String> = self
            .store
            .get(&old_holder_key)
            .map_err(|e| AccessError::StorageError(e.to_string()))?
            .map(|b| deserialize_holder_index(&b, &format!("transfer:{}", old_holder)))
            .unwrap_or_default();
        old_holder_resources.retain(|r| r != resource_id);
        let old_holder_bytes = serde_json::to_vec(&old_holder_resources)
            .map_err(|e| AccessError::StorageError(e.to_string()))?;
        self.store
            .put(&old_holder_key, &old_holder_bytes)
            .map_err(|e| AccessError::StorageError(e.to_string()))?;

        // Update holder and record transfer timestamp for audit trail
        access.holder = new_holder;
        access.last_transferred_at = Some(current_time);

        // Persist the updated access (this also adds to new holder's index)
        self.put(&access)
            .map_err(|e| AccessError::StorageError(e.to_string()))?;

        Ok((old_holder, access))
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

            if let Some(StewardshipDuty::HandoffProcedure { steps }) = handoff_duty {
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
        access.usage_log.push_back(UsageEvent::with_type(
            maintenance_time,
            "Completed weekly maintenance".to_string(),
            DutyEventType::Maintenance,
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
        access.usage_log.push_back(UsageEvent::with_type(
            benefit_time,
            "Held meeting for neighbors".to_string(), // Different description
            DutyEventType::CommunityBenefit,
        ));

        // Should pass because we used structured event type
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
        access.usage_log.push_back(UsageEvent::with_type(
            access.granted_at + 5 * 24 * 3600,
            "Weekly status update".to_string(),
            DutyEventType::Report,
        ));
        access.usage_log.push_back(UsageEvent::with_type(
            access.granted_at + 10 * 24 * 3600,
            "Progress report".to_string(),
            DutyEventType::Report,
        ));

        // Should pass with 2 report events
        assert!(access.check_duties(current_time).is_ok());
    }

    #[test]
    fn test_usage_event_with_type_constructor() {
        let event = UsageEvent::with_type(
            1234567890,
            "Maintenance completed".to_string(),
            DutyEventType::Maintenance,
        );

        assert_eq!(event.timestamp, 1234567890);
        assert_eq!(event.description, "Maintenance completed");
        assert!(event.witnesses.is_empty());
        assert_eq!(event.event_type, Some(DutyEventType::Maintenance));
    }

    // =============================================================================
    // ResourceAccessStore Tests
    // =============================================================================

    #[test]
    fn test_resource_access_store_basic_operations() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

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

        // Put and get
        access_store.put(&access).unwrap();
        let retrieved = access_store.get("tool-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().resource_id, "tool-001");

        // List by holder
        let holder_accesses = access_store.list_by_holder(&entity).unwrap();
        assert_eq!(holder_accesses.len(), 1);
        assert_eq!(holder_accesses[0].resource_id, "tool-001");

        // Remove
        access_store.remove("tool-001").unwrap();
        let retrieved = access_store.get("tool-001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_resource_access_transfer_success() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = create_test_entity();
        let bob = create_test_entity();

        let access = ResourceAccess::new(
            "tool-001".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        )
        .with_rules(AntiSpeculationRules::standard());

        access_store.put(&access).unwrap();

        // Verify initial holder index
        let alice_resources = access_store.list_by_holder(&alice).unwrap();
        assert_eq!(alice_resources.len(), 1);
        assert_eq!(alice_resources[0].resource_id, "tool-001");

        // Free transfer should succeed
        let current_time = access.granted_at + 3600;
        let (old_holder, updated_access) = access_store
            .transfer("tool-001", bob.clone(), None, current_time)
            .unwrap();

        assert_eq!(old_holder, alice);
        assert_eq!(updated_access.holder, bob);
        assert_eq!(updated_access.resource_id, "tool-001");

        // Verify it's persisted
        let retrieved = access_store.get("tool-001").unwrap().unwrap();
        assert_eq!(retrieved.holder, bob);

        // Verify holder index cleanup: resource removed from old holder's index
        let alice_resources_after = access_store.list_by_holder(&alice).unwrap();
        assert!(
            alice_resources_after.is_empty(),
            "Resource should be removed from old holder's index"
        );

        // Verify holder index: resource added to new holder's index
        let bob_resources = access_store.list_by_holder(&bob).unwrap();
        assert_eq!(
            bob_resources.len(),
            1,
            "Resource should be in new holder's index"
        );
        assert_eq!(bob_resources[0].resource_id, "tool-001");
    }

    #[test]
    fn test_resource_access_transfer_profit_blocked() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = create_test_entity();
        let bob = create_test_entity();

        let access = ResourceAccess::new(
            "tool-001".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        )
        .with_rules(AntiSpeculationRules::standard());

        access_store.put(&access).unwrap();

        // Paid transfer should fail (no_profit_transfer = true)
        let current_time = access.granted_at + 3600;
        let result = access_store.transfer("tool-001", bob, Some(100), current_time);

        assert!(matches!(result, Err(AccessError::ProfitTransferNotAllowed)));
    }

    #[test]
    fn test_stewardship_handoff_procedure_validation() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = create_test_entity();
        let bob = create_test_entity();

        let handoff_steps = vec![
            "Document all tasks".to_string(),
            "Meet with new steward".to_string(),
            "Transfer credentials".to_string(),
        ];

        let mut access = ResourceAccess::new(
            "community-garden".to_string(),
            alice.clone(),
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: handoff_steps.clone(),
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Without handoff steps completed, transfer should fail
        access_store.put(&access).unwrap();
        let result = access_store.transfer(
            "community-garden",
            bob.clone(),
            None,
            access.granted_at + 1000,
        );
        assert!(matches!(result, Err(AccessError::DutyUnfulfilled(_))));

        // Complete all handoff steps
        access
            .record_usage(
                access.granted_at + 100,
                "Document all tasks - completed".to_string(),
            )
            .unwrap();
        access
            .record_usage(
                access.granted_at + 200,
                "Meet with new steward - done".to_string(),
            )
            .unwrap();
        access
            .record_usage(
                access.granted_at + 300,
                "Transfer credentials - complete".to_string(),
            )
            .unwrap();

        access_store.put(&access).unwrap();

        // Now transfer should succeed
        let (old_holder, updated_access) = access_store
            .transfer(
                "community-garden",
                bob.clone(),
                None,
                access.granted_at + 1000,
            )
            .unwrap();

        assert_eq!(old_holder, alice);
        assert_eq!(updated_access.holder, bob);

        // Verify holder index cleanup after stewardship transfer
        let alice_resources = access_store.list_by_holder(&alice).unwrap();
        assert!(
            alice_resources.is_empty(),
            "Resource should be removed from old holder's index"
        );
        let bob_resources = access_store.list_by_holder(&bob).unwrap();
        assert_eq!(
            bob_resources.len(),
            1,
            "Resource should be in new holder's index"
        );
    }

    #[test]
    fn test_transfer_revoked_access_fails() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = create_test_entity();
        let bob = create_test_entity();

        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        // Revoke access
        access.revoke("Policy violation".to_string());
        access_store.put(&access).unwrap();

        // Transfer should fail
        let result = access_store.transfer("tool-001", bob, None, access.granted_at + 1000);
        assert!(matches!(result, Err(AccessError::Revoked(_))));
    }

    #[test]
    fn test_transfer_nonexistent_resource_fails() {
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let bob = create_test_entity();
        let result = access_store.transfer("nonexistent", bob, None, 1234567890);
        assert!(matches!(result, Err(AccessError::Revoked(_))));
    }

    // =============================================================================
    // Edge Case Tests (per code review recommendations)
    // =============================================================================

    #[test]
    fn test_handoff_with_empty_steps_allows_transfer() {
        // When HandoffProcedure has empty steps, transfer should succeed
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = create_test_entity();
        let bob = create_test_entity();

        let access = ResourceAccess::new(
            "tool-001".to_string(),
            alice.clone(),
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: vec![], // Empty steps
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        access_store.put(&access).unwrap();

        // Transfer should succeed since no steps are required
        let result = access_store.transfer("tool-001", bob.clone(), None, access.granted_at + 1000);
        assert!(result.is_ok());
        let (old_holder, updated) = result.unwrap();
        assert_eq!(old_holder, alice);
        assert_eq!(updated.holder, bob);
    }

    #[test]
    fn test_multiple_holders_index_isolation() {
        // Verify that holder indexes are properly isolated
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = create_test_entity();
        let bob = create_test_entity();
        let charlie = create_test_entity();

        // Alice holds tool-001 and tool-002
        let access1 = ResourceAccess::new(
            "tool-001".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );
        let access2 = ResourceAccess::new(
            "tool-002".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        // Bob holds tool-003
        let access3 = ResourceAccess::new(
            "tool-003".to_string(),
            bob.clone(),
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        access_store.put(&access1).unwrap();
        access_store.put(&access2).unwrap();
        access_store.put(&access3).unwrap();

        // Verify Alice has 2 resources
        let alice_resources = access_store.list_by_holder(&alice).unwrap();
        assert_eq!(alice_resources.len(), 2);

        // Verify Bob has 1 resource
        let bob_resources = access_store.list_by_holder(&bob).unwrap();
        assert_eq!(bob_resources.len(), 1);
        assert_eq!(bob_resources[0].resource_id, "tool-003");

        // Verify Charlie has 0 resources
        let charlie_resources = access_store.list_by_holder(&charlie).unwrap();
        assert!(charlie_resources.is_empty());

        // Transfer tool-001 from Alice to Charlie
        let _ = access_store
            .transfer("tool-001", charlie.clone(), None, 1000)
            .unwrap();

        // Verify Alice now has 1 resource
        let alice_resources = access_store.list_by_holder(&alice).unwrap();
        assert_eq!(alice_resources.len(), 1);
        assert_eq!(alice_resources[0].resource_id, "tool-002");

        // Verify Charlie now has 1 resource
        let charlie_resources = access_store.list_by_holder(&charlie).unwrap();
        assert_eq!(charlie_resources.len(), 1);
        assert_eq!(charlie_resources[0].resource_id, "tool-001");

        // Verify Bob's resources unchanged
        let bob_resources = access_store.list_by_holder(&bob).unwrap();
        assert_eq!(bob_resources.len(), 1);
    }

    #[test]
    fn test_handoff_step_gaming_prevention() {
        // Verify that "Failed to <step>" doesn't pass validation for "<step>"
        let alice = create_test_entity();

        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            alice,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: vec!["Transfer credentials".to_string()],
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Log a failure message that contains the step text
        access
            .record_usage(
                access.granted_at + 100,
                "Failed to transfer credentials due to system error".to_string(),
            )
            .unwrap();

        // Validation should FAIL because "transfer credentials" is not a complete segment
        let result = access.validate_transfer_preconditions(None);
        assert!(
            result.is_err(),
            "Gaming attempt should be rejected - 'Failed to transfer credentials' should not match 'Transfer credentials'"
        );
    }

    #[test]
    fn test_handoff_step_exact_segment_matching() {
        // Verify exact segment matching works correctly
        let alice = create_test_entity();

        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            alice,
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: vec!["Transfer credentials".to_string()],
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        // Log with proper segment delimiter
        access
            .record_usage(
                access.granted_at + 100,
                "Completed: Transfer credentials - verified by witness".to_string(),
            )
            .unwrap();

        // Validation should PASS because "Transfer credentials" is a complete segment
        let result = access.validate_transfer_preconditions(None);
        assert!(
            result.is_ok(),
            "Proper segment-delimited step should be accepted"
        );
    }

    #[test]
    fn test_handoff_duplicate_steps() {
        // Verify handling of duplicate step names
        let store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = create_test_entity();
        let bob = create_test_entity();

        let mut access = ResourceAccess::new(
            "tool-001".to_string(),
            alice.clone(),
            AccessModel::Stewardship {
                duties: vec![StewardshipDuty::HandoffProcedure {
                    steps: vec![
                        "Sign document".to_string(),
                        "Sign document".to_string(), // Duplicate
                    ],
                }],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        access_store.put(&access).unwrap();

        // Only log the step once
        access
            .record_usage(access.granted_at + 100, "Sign document".to_string())
            .unwrap();
        access_store.put(&access).unwrap();

        // Transfer should succeed (same step logged once satisfies both duplicates)
        let result = access_store.transfer("tool-001", bob, None, access.granted_at + 1000);
        assert!(
            result.is_ok(),
            "Single log entry should satisfy duplicate steps"
        );
    }
}
