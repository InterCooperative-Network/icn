//! Abstract service traits for domain subsystems
//!
//! These traits define kernel-level interfaces that domain apps implement.
//! The kernel uses these traits without knowing about concrete implementations.
//!
//! # The Meaning Firewall
//!
//! These traits are carefully designed to maintain the meaning firewall:
//! - The kernel sees opaque handles and numeric values
//! - Domain semantics (trust classes, governance rules) stay in apps
//! - The kernel enforces constraints without understanding their origin

use crate::authz::PolicyOracle;
use crate::types::Did;
use std::sync::Arc;

/// Abstract trust service interface
///
/// This trait provides trust-related functionality to the kernel without
/// exposing TrustGraph, TrustClass, or other domain-specific types.
///
/// # Implementation
///
/// Apps that provide trust functionality implement this trait. The kernel
/// uses it to:
/// - Get a PolicyOracle for authorization decisions
/// - Query trust scores (as opaque f64 values)
/// - Record trust-affecting events
///
/// The kernel NEVER interprets trust scores - it just passes them through.
pub trait TrustService: Send + Sync {
    /// Get the PolicyOracle for this trust service
    ///
    /// The oracle converts trust decisions into ConstraintSets that the
    /// kernel can enforce blindly.
    fn oracle(&self) -> Arc<dyn PolicyOracle>;

    /// Get trust score for an actor (opaque value 0.0-1.0)
    ///
    /// The kernel may use this for routing decisions, but NEVER interprets
    /// the semantic meaning of the score.
    fn trust_score(&self, actor: &Did) -> f64;

    /// Record a trust-affecting event
    ///
    /// Called by the kernel when violations are detected. The trust service
    /// decides how to update trust scores based on the event type.
    fn record_event(&self, actor: &Did, event: TrustEvent);
}

/// Trust-affecting events that the kernel can report
///
/// These are generic event types that don't expose domain semantics.
/// The trust service interprets them according to its own rules.
#[derive(Debug, Clone)]
pub enum TrustEvent {
    /// Protocol violation detected
    ProtocolViolation {
        /// Severity (0.0-1.0, higher = more severe)
        severity: f64,
        /// Event category for logging
        category: String,
    },
    /// Positive interaction (attestation, successful cooperation)
    PositiveInteraction {
        /// Weight of the interaction (0.0-1.0)
        weight: f64,
    },
    /// Quarantine requested by security subsystem
    QuarantineRequested {
        /// Duration in seconds
        duration_secs: u64,
    },
}

/// Abstract security service interface
///
/// This trait provides misbehavior detection and response functionality.
/// The kernel uses it to record violations and query ban/quarantine status.
pub trait SecurityService: Send + Sync {
    /// Check if an actor is banned
    fn is_banned(&self, actor: &Did) -> bool;

    /// Check if an actor is quarantined
    fn is_quarantined(&self, actor: &Did) -> bool;

    /// Record a violation
    ///
    /// The security service decides how to respond based on violation type
    /// and the actor's history.
    fn record_violation(&self, actor: &Did, violation: SecurityViolation);
}

/// Security violations that the kernel can report
#[derive(Debug, Clone)]
pub enum SecurityViolation {
    /// Invalid cryptographic signature
    InvalidSignature,
    /// Message replay detected
    ReplayAttack,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Protocol message malformed
    MalformedMessage,
    /// Unauthorized action attempted
    UnauthorizedAction,
    /// Generic violation with severity
    Generic {
        severity: f64,
        description: String,
    },
}

/// Abstract governance service interface
///
/// This trait provides governance-related functionality to the kernel without
/// exposing proposals, voting mechanisms, or governance rules directly.
///
/// # Implementation
///
/// Apps that provide governance functionality implement this trait. The kernel
/// uses it to:
/// - Get a PolicyOracle for governance authorization decisions
/// - Query governance parameters (as opaque key-value pairs)
/// - Record governance events
///
/// The kernel NEVER interprets governance rules - it just enforces constraints.
pub trait GovernanceService: Send + Sync {
    /// Get the PolicyOracle for governance authorization
    ///
    /// The oracle converts governance rules into ConstraintSets that the
    /// kernel can enforce blindly.
    fn oracle(&self) -> Arc<dyn PolicyOracle>;

    /// Get a governance parameter value
    ///
    /// Parameters are returned as opaque strings. The kernel may use these
    /// for configuration but NEVER interprets their semantic meaning.
    fn get_parameter(&self, key: &str) -> Option<String>;

    /// List all governance parameter keys
    fn list_parameters(&self) -> Vec<String>;

    /// Record a governance event
    ///
    /// Called by the kernel when governance-relevant actions occur.
    fn record_event(&self, event: GovernanceEvent);
}

/// Governance events that the kernel can report
///
/// These are generic event types that don't expose governance semantics.
/// The governance service interprets them according to its own rules.
#[derive(Debug, Clone)]
pub enum GovernanceEvent {
    /// Parameter was accessed
    ParameterAccessed {
        key: String,
        accessor: Did,
    },
    /// Parameter change was requested
    ParameterChangeRequested {
        key: String,
        new_value: String,
        requestor: Did,
    },
    /// Generic governance action
    Action {
        action_type: String,
        actor: Did,
        metadata: std::collections::HashMap<String, String>,
    },
}

/// Builder for creating domain services
///
/// This allows the daemon to construct services and inject them into the kernel.
/// The kernel never needs to know about concrete implementations.
pub struct ServiceRegistry {
    trust: Option<Arc<dyn TrustService>>,
    security: Option<Arc<dyn SecurityService>>,
    governance: Option<Arc<dyn GovernanceService>>,
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceRegistry {
    /// Create a new empty service registry
    pub fn new() -> Self {
        Self {
            trust: None,
            security: None,
            governance: None,
        }
    }

    /// Register a trust service
    pub fn with_trust(mut self, service: Arc<dyn TrustService>) -> Self {
        self.trust = Some(service);
        self
    }

    /// Register a security service
    pub fn with_security(mut self, service: Arc<dyn SecurityService>) -> Self {
        self.security = Some(service);
        self
    }

    /// Register a governance service
    pub fn with_governance(mut self, service: Arc<dyn GovernanceService>) -> Self {
        self.governance = Some(service);
        self
    }

    /// Get the trust service (if registered)
    pub fn trust(&self) -> Option<&Arc<dyn TrustService>> {
        self.trust.as_ref()
    }

    /// Get the security service (if registered)
    pub fn security(&self) -> Option<&Arc<dyn SecurityService>> {
        self.security.as_ref()
    }

    /// Get the governance service (if registered)
    pub fn governance(&self) -> Option<&Arc<dyn GovernanceService>> {
        self.governance.as_ref()
    }
}
