//! Authorization Primitive
//!
//! Provides object-capability tokens and policy oracle interface.
//!
//! # Design
//!
//! - Capabilities are bearer tokens, not RBAC roles
//! - Policy decisions come from apps via PolicyOracle
//! - Kernel enforces decisions without understanding them
//!
//! # The Policy Oracle Pattern
//!
//! The kernel calls `PolicyOracle::evaluate()` to get authorization
//! decisions. Apps (like the trust app) implement this trait to
//! provide domain-specific logic. The kernel then enforces the
//! decision without understanding why it was made.
//!
//! # Non-Goals
//!
//! - Role hierarchies (use capabilities instead)
//! - Trust scores (trust app implements these via PolicyOracle)
//! - Predefined capability taxonomies (apps define their own)
//! - Any policy logic (just enforcement)

use crate::types::{CapabilityId, Did, LogicalTimestamp};
use std::collections::HashMap;
use std::time::Duration;

/// Capability token granting specific access.
///
/// Capabilities are bearer tokens that prove authorization.
/// They can be delegated with additional restrictions.
#[derive(Clone, Debug)]
pub struct Capability {
    /// Unique identifier for this capability
    pub id: CapabilityId,
    /// Resource this capability grants access to
    pub resource: String,
    /// Action permitted on the resource
    pub action: String,
    /// Constraints limiting the capability
    pub constraints: Constraints,
    /// If set, holder must co-sign requests (not bearer-only)
    pub holder: Option<Did>,
    /// Who issued this capability
    pub issuer: Did,
    /// When this capability expires
    pub expiration: LogicalTimestamp,
    /// Cryptographic proof of issuance
    pub signature: Vec<u8>,
}

/// Constraints on a capability.
#[derive(Clone, Debug, Default)]
pub struct Constraints {
    /// Maximum amount (for numeric resources)
    pub max_amount: Option<u64>,
    /// Maximum number of uses
    pub max_uses: Option<u32>,
    /// Allowed target resources/identities
    pub allowed_targets: Option<Vec<String>>,
    /// Custom constraints (app-specific)
    pub custom: HashMap<String, String>,
}

impl Constraints {
    /// Create empty constraints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum amount.
    pub fn with_max_amount(mut self, amount: u64) -> Self {
        self.max_amount = Some(amount);
        self
    }

    /// Set maximum uses.
    pub fn with_max_uses(mut self, uses: u32) -> Self {
        self.max_uses = Some(uses);
        self
    }

    /// Set allowed targets.
    pub fn with_allowed_targets(mut self, targets: Vec<String>) -> Self {
        self.allowed_targets = Some(targets);
        self
    }

    /// Add a custom constraint.
    pub fn with_custom(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom.insert(key.into(), value.into());
        self
    }
}

/// Result of a policy evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Request is allowed
    Allow,
    /// Request is denied
    Deny {
        /// Reason for denial (for logging/debugging)
        reason: String,
    },
    /// Request is allowed but rate-limited
    RateLimit {
        /// Rate limit class to apply
        class: RateLimitClass,
    },
}

impl PolicyDecision {
    /// Create an Allow decision.
    pub fn allow() -> Self {
        Self::Allow
    }

    /// Create a Deny decision.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }

    /// Create a RateLimit decision.
    pub fn rate_limit(class: RateLimitClass) -> Self {
        Self::RateLimit { class }
    }

    /// Check if this decision allows the request.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow | Self::RateLimit { .. })
    }
}

/// Rate limit classification.
///
/// These classes are kernel-defined but the mapping of
/// identities to classes is app-defined (via PolicyOracle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateLimitClass {
    /// No rate limiting (fully trusted)
    Unlimited,
    /// Standard rate limits
    Standard,
    /// Reduced limits (lower trust)
    Throttled,
    /// Severely restricted (minimal trust)
    Restricted,
    /// Custom rate (messages per second)
    Custom(u32),
}

impl RateLimitClass {
    /// Get the rate limit in messages per second.
    pub fn messages_per_second(&self) -> u32 {
        match self {
            Self::Unlimited => u32::MAX,
            Self::Standard => 100,
            Self::Throttled => 20,
            Self::Restricted => 5,
            Self::Custom(rate) => *rate,
        }
    }
}

/// Context for policy evaluation.
#[derive(Clone, Debug)]
pub struct PolicyContext {
    /// The actor making the request
    pub actor: Did,
    /// Resource being accessed
    pub resource: String,
    /// Action being performed
    pub action: String,
    /// Namespace context (if applicable)
    pub namespace: Option<String>,
    /// Additional metadata for decision-making
    pub metadata: HashMap<String, String>,
}

impl PolicyContext {
    /// Create a new policy context.
    pub fn new(actor: Did, resource: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            actor,
            resource: resource.into(),
            action: action.into(),
            namespace: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the namespace.
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Request being evaluated by the policy oracle.
#[derive(Clone, Debug)]
pub struct PolicyRequest {
    /// Context of the request
    pub context: PolicyContext,
    /// Capability presented (if any)
    pub capability: Option<Capability>,
}

impl PolicyRequest {
    /// Create a request with just context (no capability).
    pub fn new(context: PolicyContext) -> Self {
        Self {
            context,
            capability: None,
        }
    }

    /// Create a request with a capability.
    pub fn with_capability(context: PolicyContext, capability: Capability) -> Self {
        Self {
            context,
            capability: Some(capability),
        }
    }
}

/// Policy oracle interface - implemented by apps (e.g., trust app).
///
/// The kernel calls this trait to get authorization decisions.
/// Apps implement domain-specific logic (trust scores, membership,
/// reputation, etc.) and return a decision the kernel can enforce.
///
/// # Implementation Notes
///
/// - Implementations should be fast (< 100μs for cached decisions)
/// - Results may be cached by the kernel for `cache_ttl()`
/// - Cross-org requests may require federation lookups
pub trait PolicyOracle: Send + Sync {
    /// Evaluate whether a request should be allowed.
    ///
    /// This is the core authorization decision point.
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision;

    /// Get cache TTL for decisions.
    ///
    /// The kernel may cache decisions for this duration.
    /// Return `Duration::ZERO` to disable caching.
    fn cache_ttl(&self) -> Duration {
        Duration::from_secs(60)
    }

    /// Whether this oracle handles cross-org requests.
    ///
    /// If true, the oracle should be consulted for requests
    /// involving external organizations.
    fn handles_cross_org(&self) -> bool {
        false
    }
}

/// Default oracle that allows everything.
///
/// Use this for simple single-coop deployments or during
/// bootstrap before the trust app is loaded.
pub struct AllowAllOracle;

impl PolicyOracle for AllowAllOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        PolicyDecision::Allow
    }
}

/// Oracle that denies everything.
///
/// Useful for testing or lockdown scenarios.
pub struct DenyAllOracle {
    reason: String,
}

impl DenyAllOracle {
    /// Create a deny-all oracle with a reason.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl PolicyOracle for DenyAllOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        PolicyDecision::Deny {
            reason: self.reason.clone(),
        }
    }
}

/// Capability management engine.
///
/// Handles issuance, verification, delegation, and revocation
/// of capability tokens.
pub trait CapabilityEngine: Send + Sync {
    /// Issue a new capability.
    fn issue(
        &self,
        resource: &str,
        action: &str,
        holder: &Did,
        constraints: Constraints,
        expiration: LogicalTimestamp,
    ) -> Result<Capability, AuthzError>;

    /// Check if a capability authorizes a request.
    fn check(&self, capability: &Capability, resource: &str, action: &str)
        -> Result<(), AuthzError>;

    /// Delegate a capability with additional restrictions.
    ///
    /// The delegated capability cannot exceed the original's scope.
    fn delegate(
        &self,
        capability: &Capability,
        to: &Did,
        restrictions: Constraints,
    ) -> Result<Capability, AuthzError>;

    /// Revoke a capability.
    fn revoke(&self, capability_id: &CapabilityId) -> Result<(), AuthzError>;

    /// Check if a capability has been revoked.
    fn is_revoked(&self, capability_id: &CapabilityId) -> Result<bool, AuthzError>;

    /// Verify a capability's signature.
    fn verify_signature(&self, capability: &Capability) -> Result<bool, AuthzError>;
}

/// Errors from authorization operations.
#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    /// Capability not found
    #[error("Capability not found: {0}")]
    NotFound(String),

    /// Capability has expired
    #[error("Capability expired")]
    Expired,

    /// Capability has been revoked
    #[error("Capability revoked")]
    Revoked,

    /// Capability does not authorize this action
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Invalid delegation (exceeds original scope)
    #[error("Invalid delegation: {0}")]
    InvalidDelegation(String),

    /// Invalid signature on capability
    #[error("Invalid capability signature")]
    InvalidSignature,

    /// Internal error
    #[error("Authorization error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_decision() {
        assert!(PolicyDecision::Allow.is_allowed());
        assert!(PolicyDecision::rate_limit(RateLimitClass::Standard).is_allowed());
        assert!(!PolicyDecision::deny("test").is_allowed());
    }

    #[test]
    fn test_rate_limit_class() {
        assert_eq!(RateLimitClass::Unlimited.messages_per_second(), u32::MAX);
        assert_eq!(RateLimitClass::Standard.messages_per_second(), 100);
        assert_eq!(RateLimitClass::Throttled.messages_per_second(), 20);
        assert_eq!(RateLimitClass::Restricted.messages_per_second(), 5);
        assert_eq!(RateLimitClass::Custom(50).messages_per_second(), 50);
    }

    #[test]
    fn test_allow_all_oracle() {
        let oracle = AllowAllOracle;
        let context = PolicyContext::new("did:icn:test".to_string(), "resource", "read");
        let request = PolicyRequest::new(context);
        assert_eq!(oracle.evaluate(&request), PolicyDecision::Allow);
    }

    #[test]
    fn test_deny_all_oracle() {
        let oracle = DenyAllOracle::new("system locked");
        let context = PolicyContext::new("did:icn:test".to_string(), "resource", "read");
        let request = PolicyRequest::new(context);
        match oracle.evaluate(&request) {
            PolicyDecision::Deny { reason } => assert_eq!(reason, "system locked"),
            _ => panic!("Expected Deny"),
        }
    }

    #[test]
    fn test_constraints_builder() {
        let constraints = Constraints::new()
            .with_max_amount(1000)
            .with_max_uses(5)
            .with_allowed_targets(vec!["target1".to_string()])
            .with_custom("key", "value");

        assert_eq!(constraints.max_amount, Some(1000));
        assert_eq!(constraints.max_uses, Some(5));
        assert_eq!(
            constraints.allowed_targets,
            Some(vec!["target1".to_string()])
        );
        assert_eq!(constraints.custom.get("key"), Some(&"value".to_string()));
    }
}
