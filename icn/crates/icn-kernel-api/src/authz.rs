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
//! # The Meaning Firewall
//!
//! The kernel does NOT understand:
//! - Trust classes or scores
//! - Governance weights
//! - Credit multipliers
//! - Membership status
//!
//! It only understands:
//! - Allow with constraints
//! - Deny with reason
//!
//! Apps set constraint values. Kernel enforces them blindly.
//!
//! # Non-Goals
//!
//! - Role hierarchies (use capabilities instead)
//! - Trust scores (trust app implements these via PolicyOracle)
//! - Predefined capability taxonomies (apps define their own)
//! - Any policy logic (just enforcement)

use crate::types::{CapabilityId, Did, LogicalTimestamp};
use async_trait::async_trait;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

// ============================================================================
// Policy Decision Types
// ============================================================================

/// Result of a policy evaluation.
///
/// The kernel enforces these decisions WITHOUT understanding their semantic origin.
/// Apps (trust, governance, ledger) populate ConstraintSet via PolicyOracle.
#[derive(Clone, Debug)]
pub enum PolicyDecision {
    /// Request is allowed, optionally with constraints
    Allow {
        /// Constraints the kernel will enforce
        constraints: ConstraintSet,
    },
    /// Request is denied
    Deny {
        /// Error describing why the request was denied
        reason: PolicyError,
    },
}

impl PolicyDecision {
    /// Create an Allow decision with no constraints.
    pub fn allow() -> Self {
        Self::Allow {
            constraints: ConstraintSet::default(),
        }
    }

    /// Create an Allow decision with constraints.
    pub fn allow_with(constraints: ConstraintSet) -> Self {
        Self::Allow { constraints }
    }

    /// Create a Deny decision.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: PolicyError::Denied(reason.into()),
        }
    }

    /// Create a Deny decision with a PolicyError.
    pub fn deny_error(reason: PolicyError) -> Self {
        Self::Deny { reason }
    }

    /// Check if this decision allows the request.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    /// Get constraints if allowed, None if denied.
    pub fn constraints(&self) -> Option<&ConstraintSet> {
        match self {
            Self::Allow { constraints } => Some(constraints),
            Self::Deny { .. } => None,
        }
    }
}

/// Error returned when a policy denies a request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PolicyError {
    /// Generic denial with reason
    Denied(String),
    /// Actor not recognized
    UnknownActor,
    /// Resource not found
    ResourceNotFound,
    /// Insufficient privilege
    InsufficientPrivilege,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Capability expired
    CapabilityExpired,
    /// Capability revoked
    CapabilityRevoked,
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(reason) => write!(f, "Denied: {}", reason),
            Self::UnknownActor => write!(f, "Unknown actor"),
            Self::ResourceNotFound => write!(f, "Resource not found"),
            Self::InsufficientPrivilege => write!(f, "Insufficient privilege"),
            Self::RateLimitExceeded => write!(f, "Rate limit exceeded"),
            Self::CapabilityExpired => write!(f, "Capability expired"),
            Self::CapabilityRevoked => write!(f, "Capability revoked"),
        }
    }
}

impl std::error::Error for PolicyError {}

// ============================================================================
// Constraint Types
// ============================================================================

/// Kernel-enforced constraints.
///
/// The kernel does NOT know WHY these values exist.
/// Apps (trust, governance, ledger) set them via PolicyOracle.
/// Kernel enforces them blindly.
///
/// **Domain-blind rule**: kernel code must not interpret `custom` values.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConstraintSet {
    /// Rate limiting parameters
    pub rate_limit: Option<RateLimit>,
    /// Maximum topics this actor can subscribe to
    pub max_topics: Option<u32>,
    /// Maximum message size in bytes
    pub max_message_size: Option<u64>,
    /// Maximum concurrent connections
    pub max_connections: Option<u32>,
    /// Maximum subscriptions this actor can have
    pub max_subscriptions: Option<u32>,
    /// Maximum outstanding requests
    pub max_outstanding_requests: Option<u32>,
    /// App-specific constraints (opaque to the kernel)
    pub custom: HashMap<String, ConstraintValue>,
}

impl ConstraintSet {
    /// Create an empty constraint set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set rate limit.
    pub fn with_rate_limit(mut self, rate_limit: RateLimit) -> Self {
        self.rate_limit = Some(rate_limit);
        self
    }

    /// Set max topics.
    pub fn with_max_topics(mut self, max: u32) -> Self {
        self.max_topics = Some(max);
        self
    }

    /// Set max message size.
    pub fn with_max_message_size(mut self, size: u64) -> Self {
        self.max_message_size = Some(size);
        self
    }

    /// Set max connections.
    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = Some(max);
        self
    }

    /// Set max subscriptions.
    pub fn with_max_subscriptions(mut self, max: u32) -> Self {
        self.max_subscriptions = Some(max);
        self
    }

    /// Set max outstanding requests.
    pub fn with_max_outstanding_requests(mut self, max: u32) -> Self {
        self.max_outstanding_requests = Some(max);
        self
    }

    /// Add a custom constraint.
    pub fn with_custom(mut self, key: impl Into<String>, value: ConstraintValue) -> Self {
        self.custom.insert(key.into(), value);
        self
    }

    /// Merge another constraint set, preferring other's values when both present.
    pub fn merge(&mut self, other: &ConstraintSet) {
        if other.rate_limit.is_some() {
            self.rate_limit = other.rate_limit.clone();
        }
        if other.max_topics.is_some() {
            self.max_topics = other.max_topics;
        }
        if other.max_message_size.is_some() {
            self.max_message_size = other.max_message_size;
        }
        if other.max_connections.is_some() {
            self.max_connections = other.max_connections;
        }
        if other.max_subscriptions.is_some() {
            self.max_subscriptions = other.max_subscriptions;
        }
        if other.max_outstanding_requests.is_some() {
            self.max_outstanding_requests = other.max_outstanding_requests;
        }
        for (k, v) in &other.custom {
            self.custom.insert(k.clone(), v.clone());
        }
    }
}

/// Rate limit parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateLimit {
    /// Maximum messages per second
    pub messages_per_second: u32,
    /// Burst size (token bucket)
    pub burst_size: u32,
}

impl RateLimit {
    /// Create a new rate limit.
    pub fn new(messages_per_second: u32, burst_size: u32) -> Self {
        Self {
            messages_per_second,
            burst_size,
        }
    }

    /// Unlimited rate (effectively no limit).
    pub fn unlimited() -> Self {
        Self {
            messages_per_second: u32::MAX,
            burst_size: u32::MAX,
        }
    }

    /// Standard rate limit (100 msg/s, 25 burst).
    pub fn standard() -> Self {
        Self {
            messages_per_second: 100,
            burst_size: 25,
        }
    }

    /// Throttled rate limit (20 msg/s, 10 burst).
    pub fn throttled() -> Self {
        Self {
            messages_per_second: 20,
            burst_size: 10,
        }
    }

    /// Restricted rate limit (5 msg/s, 5 burst).
    pub fn restricted() -> Self {
        Self {
            messages_per_second: 5,
            burst_size: 5,
        }
    }
}

/// Bounded constraint value - NOT Box<dyn Any>.
///
/// This is serializable and has known size, unlike dynamic types.
///
/// # Float Handling
///
/// Uses `OrderedFloat<f64>` for the Float variant to ensure correct
/// equality semantics (NaN == NaN) for cache keys and ConstraintSet
/// comparisons. This prevents subtle cache misses when constraints
/// contain NaN values.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ConstraintValue {
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// Floating point value (uses OrderedFloat for correct equality semantics)
    Float(OrderedFloat<f64>),
    /// String value
    String(String),
    /// List of values
    List(Vec<ConstraintValue>),
}

impl From<bool> for ConstraintValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i64> for ConstraintValue {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<f64> for ConstraintValue {
    fn from(v: f64) -> Self {
        Self::Float(OrderedFloat(v))
    }
}

impl From<String> for ConstraintValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&str> for ConstraintValue {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

// ============================================================================
// Policy Request Types (Split for Caching)
// ============================================================================

/// Domain identifier for oracle routing.
///
/// Each domain has its own oracle. Examples: "trust", "governance", "ledger".
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Domain(pub String);

impl Domain {
    /// Create a new domain.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The trust domain.
    pub fn trust() -> Self {
        Self("trust".to_string())
    }

    /// The governance domain.
    pub fn governance() -> Self {
        Self("governance".to_string())
    }

    /// The ledger domain.
    pub fn ledger() -> Self {
        Self("ledger".to_string())
    }

    /// Get the domain name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Action categories.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum ActionKind {
    /// Read operation
    Read,
    /// Write operation
    Write,
    /// Subscribe to events
    Subscribe,
    /// Publish events
    Publish,
    /// Execute compute
    Execute,
    /// Custom action (app-defined)
    Custom(String),
}

impl ActionKind {
    /// Create a custom action.
    pub fn custom(name: impl Into<String>) -> Self {
        Self::Custom(name.into())
    }
}

/// Cacheable core of a policy request.
///
/// Small, hashable, no allocations on hot path.
/// Used as cache key for oracle decisions.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct PolicyRequestCore {
    /// The actor making the request
    pub actor: Did,
    /// Action being performed
    pub action: ActionKind,
    /// Policy scope for routing to the correct oracle
    pub domain: Domain,
}

impl PolicyRequestCore {
    /// Create a new policy request core.
    pub fn new(actor: Did, action: ActionKind, domain: Domain) -> Self {
        Self {
            actor,
            action,
            domain,
        }
    }
}

/// Context for policy request evaluation.
///
/// Inspected only on cache miss. Not part of cache key.
/// Values are opaque to the kernel and interpreted only by the oracle.
#[derive(Clone, Debug, Default)]
pub struct PolicyContext {
    /// Resource being accessed (if specific)
    pub resource: Option<String>,
    /// Namespace context
    pub namespace: Option<String>,
    /// Additional metadata for decision-making
    pub metadata: HashMap<String, String>,
    /// Capability presented (if any)
    pub capability: Option<Capability>,
}

impl PolicyContext {
    /// Create empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set resource.
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// Set namespace.
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set capability.
    pub fn with_capability(mut self, capability: Capability) -> Self {
        self.capability = Some(capability);
        self
    }
}

/// Full policy request combining core and context data.
#[derive(Clone, Debug)]
pub struct PolicyRequest {
    /// Cacheable core (used as cache key)
    pub core: PolicyRequestCore,
    /// Context metadata (inspected on cache miss)
    pub context: PolicyContext,
}

impl PolicyRequest {
    /// Create a request with just core (no extended data).
    pub fn new(actor: Did, action: ActionKind, domain: Domain) -> Self {
        Self {
            core: PolicyRequestCore::new(actor, action, domain),
            context: PolicyContext::default(),
        }
    }

    /// Create a request with core and context data.
    pub fn with_context(core: PolicyRequestCore, context: PolicyContext) -> Self {
        Self { core, context }
    }

    /// Get the actor.
    pub fn actor(&self) -> &Did {
        &self.core.actor
    }

    /// Get the action.
    pub fn action(&self) -> &ActionKind {
        &self.core.action
    }

    /// Get the domain.
    pub fn domain(&self) -> &Domain {
        &self.core.domain
    }
}

// ============================================================================
// Policy Oracle Interface
// ============================================================================

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
///
/// # The Meaning Firewall
///
/// Oracles convert domain semantics (trust scores, membership status)
/// into kernel-enforceable constraints. The kernel never knows the
/// semantic origin of these constraints.
///
/// # Sync and Async Paths
///
/// This trait provides both synchronous and asynchronous evaluation paths:
///
/// - `evaluate()`: Synchronous method for hot-path performance
/// - `evaluate_async()`: Asynchronous method for native async implementations
///
/// Oracle implementations that need to access async resources have two options:
///
/// **Option 1: Sync-optimized (default)**
/// 1. Use `parking_lot::RwLock` instead of `tokio::sync::RwLock`
/// 2. Pre-compute values during async initialization
/// 3. Cache results internally
/// 4. Rely on default `evaluate_async()` calling `evaluate()`
///
/// **Option 2: Native async (opt-in)**
/// 1. Override `evaluate_async()` with native async implementation
/// 2. Use `tokio::sync::RwLock` or other async primitives
/// 3. Default `evaluate()` can use `tokio::task::block_in_place()` for sync callers
///
/// Most implementations should use Option 1 for better hot-path performance.
/// Option 2 is for cases where async I/O or contended tokio locks are unavoidable.
#[async_trait]
pub trait PolicyOracle: Send + Sync {
    /// Evaluate whether a request should be allowed (synchronous).
    ///
    /// Returns Allow with constraints or Deny with reason.
    /// The kernel enforces the decision without understanding it.
    ///
    /// # Performance
    ///
    /// This method is called on the hot path. Implementations should:
    /// - Complete in <1ms for cached results
    /// - Avoid blocking I/O or async runtime operations
    /// - Use internal caching for expensive computations
    ///
    /// # Default Implementation
    ///
    /// If your implementation overrides `evaluate_async()` for native async support,
    /// you should provide a `evaluate()` implementation that uses
    /// `tokio::task::block_in_place()` to bridge to the async path when called
    /// from a tokio runtime context.
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision;

    /// Evaluate whether a request should be allowed (asynchronous).
    ///
    /// Returns Allow with constraints or Deny with reason.
    /// The kernel enforces the decision without understanding it.
    ///
    /// # Default Implementation
    ///
    /// By default, this delegates to the synchronous `evaluate()` method.
    /// Override this method if your oracle can benefit from native async
    /// operations (e.g., async I/O, tokio locks).
    ///
    /// # When to Override
    ///
    /// Override this method when:
    /// - Your oracle uses `tokio::sync::RwLock` or other async primitives
    /// - Evaluation requires async I/O (database, network)
    /// - Lock contention is high and async locks provide better fairness
    ///
    /// Most implementations should NOT override this - the default sync path
    /// is more efficient for cached in-memory decisions.
    async fn evaluate_async(&self, request: &PolicyRequest) -> PolicyDecision {
        self.evaluate(request)
    }

    /// Get cache TTL for decisions from this oracle.
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

    /// Domain this oracle is responsible for.
    fn domain(&self) -> Domain;
}

/// Default oracle that allows everything.
///
/// Use this for simple single-coop deployments or during
/// bootstrap before the trust app is loaded.
pub struct AllowAllOracle {
    domain: Domain,
}

impl AllowAllOracle {
    /// Create an AllowAllOracle for a specific domain.
    pub fn new(domain: Domain) -> Self {
        Self { domain }
    }

    /// Create an AllowAllOracle for all domains (wildcard).
    pub fn wildcard() -> Self {
        Self {
            domain: Domain::new("*"),
        }
    }
}

impl Default for AllowAllOracle {
    fn default() -> Self {
        Self::wildcard()
    }
}

impl PolicyOracle for AllowAllOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        PolicyDecision::allow()
    }

    fn domain(&self) -> Domain {
        self.domain.clone()
    }
}

/// Oracle that denies everything.
///
/// Useful for testing or lockdown scenarios.
pub struct DenyAllOracle {
    domain: Domain,
    reason: String,
}

impl DenyAllOracle {
    /// Create a deny-all oracle with a reason.
    pub fn new(domain: Domain, reason: impl Into<String>) -> Self {
        Self {
            domain,
            reason: reason.into(),
        }
    }
}

impl PolicyOracle for DenyAllOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        PolicyDecision::deny(&self.reason)
    }

    fn domain(&self) -> Domain {
        self.domain.clone()
    }
}

// ============================================================================
// Capability Types
// ============================================================================

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
    fn check(
        &self,
        capability: &Capability,
        resource: &str,
        action: &str,
    ) -> Result<(), AuthzError>;

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

// ============================================================================
// Errors
// ============================================================================

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

    /// Not in genesis phase (cannot issue genesis capabilities)
    #[error("Not in genesis phase")]
    NotInGenesis,

    /// Genesis capabilities have expired
    #[error("Genesis capabilities expired")]
    GenesisExpired,

    /// Internal error
    #[error("Authorization error: {0}")]
    Internal(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_decision() {
        assert!(PolicyDecision::allow().is_allowed());
        assert!(PolicyDecision::allow_with(ConstraintSet::default()).is_allowed());
        assert!(!PolicyDecision::deny("test").is_allowed());
    }

    #[test]
    fn test_policy_decision_constraints() {
        let constraints = ConstraintSet::new()
            .with_rate_limit(RateLimit::standard())
            .with_max_topics(10);

        let decision = PolicyDecision::allow_with(constraints);

        let c = decision.constraints().unwrap();
        assert_eq!(c.rate_limit.as_ref().unwrap().messages_per_second, 100);
        assert_eq!(c.max_topics, Some(10));
    }

    #[test]
    fn test_rate_limit_presets() {
        assert_eq!(RateLimit::unlimited().messages_per_second, u32::MAX);
        assert_eq!(RateLimit::standard().messages_per_second, 100);
        assert_eq!(RateLimit::throttled().messages_per_second, 20);
        assert_eq!(RateLimit::restricted().messages_per_second, 5);
    }

    #[test]
    fn test_constraint_set_merge() {
        let mut base = ConstraintSet::new().with_max_topics(5);

        let overlay = ConstraintSet::new().with_max_topics(10);

        base.merge(&overlay);

        assert_eq!(base.max_topics, Some(10)); // Overwritten
    }

    #[test]
    fn test_constraint_value_conversions() {
        assert_eq!(ConstraintValue::from(true), ConstraintValue::Bool(true));
        assert_eq!(ConstraintValue::from(42i64), ConstraintValue::Int(42));
        assert_eq!(
            ConstraintValue::from(2.5f64),
            ConstraintValue::Float(OrderedFloat(2.5))
        );
        assert_eq!(
            ConstraintValue::from("test"),
            ConstraintValue::String("test".to_string())
        );
    }

    #[test]
    fn test_policy_request_core() {
        let core = PolicyRequestCore::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        assert_eq!(core.actor, "did:icn:test");
        assert_eq!(core.action, ActionKind::Read);
        assert_eq!(core.domain, Domain::trust());
    }

    #[test]
    fn test_policy_request_with_context() {
        let request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Write,
            Domain::ledger(),
        );

        assert_eq!(request.actor(), "did:icn:test");
        assert!(request.context.resource.is_none());
    }

    #[test]
    fn test_allow_all_oracle() {
        let oracle = AllowAllOracle::default();
        let request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        let decision = oracle.evaluate(&request);
        assert!(decision.is_allowed());
    }

    #[test]
    fn test_deny_all_oracle() {
        let oracle = DenyAllOracle::new(Domain::trust(), "system locked");
        let request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        let decision = oracle.evaluate(&request);
        assert!(!decision.is_allowed());

        match decision {
            PolicyDecision::Deny { reason } => {
                assert!(matches!(reason, PolicyError::Denied(r) if r == "system locked"));
            }
            _ => panic!("Expected Deny"),
        }
    }

    #[test]
    fn test_domain_presets() {
        assert_eq!(Domain::trust().as_str(), "trust");
        assert_eq!(Domain::governance().as_str(), "governance");
        assert_eq!(Domain::ledger().as_str(), "ledger");
    }

    #[test]
    fn test_action_kind_custom() {
        let action = ActionKind::custom("witness");
        assert_eq!(action, ActionKind::Custom("witness".to_string()));
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
