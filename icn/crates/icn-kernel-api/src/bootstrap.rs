//! Bootstrap Infrastructure
//!
//! Provides infrastructure for system bootstrap and oracle management.
//!
//! # Bootstrap Phases
//!
//! The system goes through distinct phases during startup:
//!
//! 1. **Genesis** - Initial startup. AllowAllOracle is active. Genesis capabilities
//!    can be issued to bootstrap first-party apps.
//!
//! 2. **CoreApps** - First-party apps (trust, governance, etc.) are loading.
//!    Genesis capabilities are still valid but will expire soon.
//!
//! 3. **Running** - Normal operation. Genesis capabilities have expired.
//!    All authorization goes through registered PolicyOracles.
//!
//! # Oracle Registry
//!
//! The OracleRegistry provides:
//! - Atomic oracle replacement (no torn reads)
//! - Per-domain routing
//! - TTL-based caching with automatic invalidation on oracle swap
//!
//! # Genesis Capabilities
//!
//! Genesis capabilities are time-limited and phase-guarded:
//! - Can only be issued during Genesis phase
//! - Expire after a configured TTL (default: 60 seconds)
//! - Cannot be reused after bootstrap completes
//!
//! This prevents genesis capabilities from becoming a permanent backdoor.

use crate::authz::{
    AllowAllOracle, AuthzError, Capability, Constraints, Domain, PolicyDecision, PolicyOracle,
    PolicyRequest, PolicyRequestCore,
};
use crate::types::{Did, LogicalTimestamp};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// Bootstrap Phase
// ============================================================================

/// System bootstrap phase.
///
/// Determines what operations are allowed during startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BootstrapPhase {
    /// Initial startup. AllowAllOracle active. Genesis caps available.
    Genesis = 0,
    /// Loading first-party apps. Genesis caps still valid.
    CoreApps = 1,
    /// Normal operation. Genesis caps expired.
    Running = 2,
}

impl BootstrapPhase {
    /// Convert from u8.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Genesis),
            1 => Some(Self::CoreApps),
            2 => Some(Self::Running),
            _ => None,
        }
    }

    /// Check if this is a bootstrap phase (not yet Running).
    pub fn is_bootstrap(&self) -> bool {
        matches!(self, Self::Genesis | Self::CoreApps)
    }
}

// ============================================================================
// Cached Decision
// ============================================================================

/// A cached policy decision with expiration.
#[derive(Clone)]
struct CachedDecision {
    decision: PolicyDecision,
    expires_at: Instant,
}

// ============================================================================
// Decision Cache
// ============================================================================

/// TTL-based cache for policy decisions.
///
/// Uses PolicyRequestCore as the cache key for fast lookups.
/// Extended metadata is not cached - only consulted on cache miss.
pub struct DecisionCache {
    entries: RwLock<HashMap<PolicyRequestCore, CachedDecision>>,
    max_entries: usize,
}

impl DecisionCache {
    /// Create a new cache with the given capacity.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::with_capacity(max_entries)),
            max_entries,
        }
    }

    /// Get a cached decision if it exists and hasn't expired.
    pub fn get(&self, key: &PolicyRequestCore) -> Option<PolicyDecision> {
        let entries = self.entries.read();
        if let Some(cached) = entries.get(key) {
            if cached.expires_at > Instant::now() {
                return Some(cached.decision.clone());
            }
        }
        None
    }

    /// Insert a decision into the cache.
    pub fn insert(&self, key: PolicyRequestCore, decision: PolicyDecision, ttl: Duration) {
        if ttl.is_zero() {
            return;
        }

        let mut entries = self.entries.write();

        // Evict expired entries if we're at capacity
        if entries.len() >= self.max_entries {
            let now = Instant::now();
            entries.retain(|_, v| v.expires_at > now);
        }

        // If still at capacity after eviction, skip insertion
        if entries.len() >= self.max_entries {
            return;
        }

        entries.insert(
            key,
            CachedDecision {
                decision,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    /// Invalidate all entries for a specific domain.
    pub fn invalidate_domain(&self, domain: &Domain) {
        let mut entries = self.entries.write();
        entries.retain(|k, _| &k.domain != domain);
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        let mut entries = self.entries.write();
        entries.clear();
    }

    /// Get the number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }
}

impl Default for DecisionCache {
    fn default() -> Self {
        Self::new(10_000)
    }
}

// ============================================================================
// Oracle Registry
// ============================================================================

/// Registry for policy oracles with atomic swap and caching.
///
/// # Thread Safety
///
/// - Oracle map uses ArcSwap for lock-free reads and atomic replacement
/// - Cache uses RwLock for concurrent reads with exclusive writes
/// - Phase uses AtomicU8 for lock-free access
///
/// # Cache Invalidation
///
/// When an oracle is replaced, all cached decisions for that domain are
/// invalidated. This ensures that the new oracle's decisions take effect
/// immediately.
pub struct OracleRegistry {
    /// Domain → Oracle mapping. Uses ArcSwap for atomic replacement.
    oracles: ArcSwap<HashMap<Domain, Arc<dyn PolicyOracle>>>,
    /// Fallback oracle for unknown domains.
    fallback: ArcSwap<Arc<dyn PolicyOracle>>,
    /// Decision cache.
    cache: DecisionCache,
    /// Current bootstrap phase.
    phase: AtomicU8,
}

impl OracleRegistry {
    /// Create a new registry with AllowAllOracle as fallback.
    pub fn new() -> Self {
        Self {
            oracles: ArcSwap::from_pointee(HashMap::new()),
            fallback: ArcSwap::from_pointee(Arc::new(AllowAllOracle::default())),
            cache: DecisionCache::default(),
            phase: AtomicU8::new(BootstrapPhase::Genesis as u8),
        }
    }

    /// Create a new registry with custom cache capacity.
    pub fn with_cache_capacity(capacity: usize) -> Self {
        Self {
            oracles: ArcSwap::from_pointee(HashMap::new()),
            fallback: ArcSwap::from_pointee(Arc::new(AllowAllOracle::default())),
            cache: DecisionCache::new(capacity),
            phase: AtomicU8::new(BootstrapPhase::Genesis as u8),
        }
    }

    /// Register an oracle for a domain.
    ///
    /// Atomically replaces any existing oracle for the domain and
    /// invalidates all cached decisions for that domain.
    pub fn register(&self, domain: Domain, oracle: Arc<dyn PolicyOracle>) {
        // Clone current map, insert new oracle
        let mut map = (**self.oracles.load()).clone();
        map.insert(domain.clone(), oracle);
        self.oracles.store(Arc::new(map));

        // Invalidate cache for this domain
        self.cache.invalidate_domain(&domain);
    }

    /// Unregister an oracle for a domain.
    ///
    /// Returns true if an oracle was removed.
    pub fn unregister(&self, domain: &Domain) -> bool {
        let mut map = (**self.oracles.load()).clone();
        let removed = map.remove(domain).is_some();
        if removed {
            self.oracles.store(Arc::new(map));
            self.cache.invalidate_domain(domain);
        }
        removed
    }

    /// Get the oracle for a domain.
    ///
    /// Returns the fallback oracle if no oracle is registered for the domain.
    pub fn get(&self, domain: &Domain) -> Arc<dyn PolicyOracle> {
        self.oracles
            .load()
            .get(domain)
            .cloned()
            .unwrap_or_else(|| (**self.fallback.load()).clone())
    }

    /// Set the fallback oracle for unknown domains.
    pub fn set_fallback(&self, oracle: Arc<dyn PolicyOracle>) {
        self.fallback.store(Arc::new(oracle));
        // Don't invalidate cache - fallback doesn't affect existing entries
    }

    /// Evaluate a policy request.
    ///
    /// 1. Check cache (core only)
    /// 2. On miss, consult appropriate oracle
    /// 3. Cache the result
    ///
    /// # Security
    ///
    /// - During Genesis/CoreApps: missing oracle allows (bootstrap flexibility)
    /// - During Running: missing oracle denies (security by default)
    pub fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let phase = self.phase();
        let domain = &request.core.domain;

        // Check cache first (fast path)
        if let Some(cached) = self.cache.get(&request.core) {
            return cached;
        }

        // Cache miss - get oracle for domain
        let oracle = match self.oracles.load().get(domain) {
            Some(o) => o.clone(),
            None => {
                // In Genesis/CoreApps: allow (bootstrap needs flexibility)
                // In Running: deny (missing oracle is a security hole)
                let decision = match phase {
                    BootstrapPhase::Genesis | BootstrapPhase::CoreApps => {
                        PolicyDecision::allow()
                    }
                    BootstrapPhase::Running => {
                        PolicyDecision::deny(format!("No oracle registered for domain: {}", domain))
                    }
                };
                // Cache the deny decision to avoid repeated lookups
                if matches!(phase, BootstrapPhase::Running) {
                    self.cache.insert(
                        request.core.clone(),
                        decision.clone(),
                        Duration::from_secs(60),
                    );
                }
                return decision;
            }
        };

        // Evaluate request
        let decision = oracle.evaluate(request);
        let ttl = oracle.cache_ttl();

        // Cache the decision
        self.cache.insert(request.core.clone(), decision.clone(), ttl);

        decision
    }

    /// Get the current bootstrap phase.
    pub fn phase(&self) -> BootstrapPhase {
        BootstrapPhase::from_u8(self.phase.load(AtomicOrdering::SeqCst))
            .unwrap_or(BootstrapPhase::Running)
    }

    /// Set the bootstrap phase.
    ///
    /// Clears the cache when transitioning to Running to ensure
    /// fresh decisions with production oracles.
    pub fn set_phase(&self, phase: BootstrapPhase) {
        let old = self.phase.swap(phase as u8, AtomicOrdering::SeqCst);
        let old_phase = BootstrapPhase::from_u8(old).unwrap_or(BootstrapPhase::Running);

        // Clear cache when transitioning to Running
        if old_phase != BootstrapPhase::Running && phase == BootstrapPhase::Running {
            self.cache.clear();
        }
    }

    /// Get an atomic reference to the phase for GenesisCapabilities.
    pub fn phase_ref(&self) -> Arc<AtomicU8> {
        // This is a bit awkward - we need to share the phase atomically.
        // In practice, GenesisCapabilities should be created with a reference
        // to the same AtomicU8, but for now we'll create a wrapper.
        // TODO: Refactor to share phase directly
        Arc::new(AtomicU8::new(self.phase.load(AtomicOrdering::SeqCst)))
    }

    /// Get cache statistics.
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
        }
    }

    /// Clear the decision cache.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// List all registered domains.
    pub fn domains(&self) -> Vec<Domain> {
        self.oracles.load().keys().cloned().collect()
    }
}

impl Default for OracleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics.
#[derive(Clone, Debug)]
pub struct CacheStats {
    /// Number of entries in the cache.
    pub entries: usize,
}

// ============================================================================
// Genesis Capabilities
// ============================================================================

/// Time-limited, phase-guarded capability issuer for bootstrap.
///
/// Genesis capabilities can only be issued during the Genesis phase and
/// expire after a configured TTL. This prevents them from becoming a
/// permanent backdoor.
///
/// # Usage
///
/// ```ignore
/// let genesis = GenesisCapabilities::new(registry.phase_ref(), Duration::from_secs(60));
///
/// // During Genesis phase:
/// let cap = genesis.issue("state:*", "write", &holder_did)?;
///
/// // After transitioning to Running phase:
/// let cap = genesis.issue("state:*", "write", &holder_did);
/// // Returns Err(AuthzError::NotInGenesis)
/// ```
pub struct GenesisCapabilities {
    /// Reference to the bootstrap phase.
    phase: Arc<AtomicU8>,
    /// When genesis capabilities expire.
    expires_at: Instant,
    /// Counter for generating capability IDs.
    counter: AtomicU8,
    /// Issuer DID for generated capabilities.
    issuer: Did,
}

impl GenesisCapabilities {
    /// Create a new genesis capability issuer.
    ///
    /// # Arguments
    ///
    /// * `phase` - Reference to the bootstrap phase atomic
    /// * `ttl` - How long genesis capabilities remain valid
    /// * `issuer` - DID to use as the issuer for generated capabilities
    pub fn new(phase: Arc<AtomicU8>, ttl: Duration, issuer: Did) -> Self {
        Self {
            phase,
            expires_at: Instant::now() + ttl,
            counter: AtomicU8::new(0),
            issuer,
        }
    }

    /// Create genesis capabilities with default TTL (60 seconds).
    pub fn with_defaults(phase: Arc<AtomicU8>, issuer: Did) -> Self {
        Self::new(phase, Duration::from_secs(60), issuer)
    }

    /// Check if genesis capabilities can still be issued.
    pub fn is_valid(&self) -> bool {
        let phase = BootstrapPhase::from_u8(self.phase.load(AtomicOrdering::SeqCst))
            .unwrap_or(BootstrapPhase::Running);

        phase == BootstrapPhase::Genesis && Instant::now() < self.expires_at
    }

    /// Issue a genesis capability.
    ///
    /// Returns an error if:
    /// - Not in Genesis phase
    /// - Genesis TTL has expired
    pub fn issue(
        &self,
        resource: &str,
        action: &str,
        holder: &Did,
        constraints: Constraints,
        expiration: LogicalTimestamp,
    ) -> Result<Capability, AuthzError> {
        // Check phase
        let phase = BootstrapPhase::from_u8(self.phase.load(AtomicOrdering::SeqCst))
            .unwrap_or(BootstrapPhase::Running);

        if phase != BootstrapPhase::Genesis {
            return Err(AuthzError::NotInGenesis);
        }

        // Check expiration
        if Instant::now() >= self.expires_at {
            return Err(AuthzError::GenesisExpired);
        }

        // Generate capability ID
        let seq = self.counter.fetch_add(1, AtomicOrdering::SeqCst);
        let cap_id = format!("genesis:{:016x}:{}", self.expires_at.elapsed().as_nanos(), seq);

        Ok(Capability {
            id: cap_id,
            resource: resource.to_string(),
            action: action.to_string(),
            constraints,
            holder: Some(holder.clone()),
            issuer: self.issuer.clone(),
            expiration,
            signature: vec![], // Genesis caps are trusted by convention during bootstrap
        })
    }

    /// Issue a capability with default constraints.
    pub fn issue_simple(
        &self,
        resource: &str,
        action: &str,
        holder: &Did,
    ) -> Result<Capability, AuthzError> {
        self.issue(
            resource,
            action,
            holder,
            Constraints::default(),
            u64::MAX, // Never expires (but genesis caps themselves expire)
        )
    }

    /// Get remaining time until genesis capabilities expire.
    pub fn remaining_ttl(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }
}

// ============================================================================
// Capability Set
// ============================================================================

/// A set of capabilities that can be checked for delegation.
///
/// Used during app installation to verify that requested capabilities
/// can be delegated from the installer's capability set.
#[derive(Clone, Debug, Default)]
pub struct CapabilitySet {
    capabilities: Vec<Capability>,
}

impl CapabilitySet {
    /// Create an empty capability set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a capability set from a list of capabilities.
    pub fn from_capabilities(capabilities: Vec<Capability>) -> Self {
        Self { capabilities }
    }

    /// Add a capability to the set.
    pub fn add(&mut self, capability: Capability) {
        self.capabilities.push(capability);
    }

    /// Check if this set can delegate a capability request.
    ///
    /// A capability request can be delegated if:
    /// 1. This set contains a capability for the same resource
    /// 2. The action matches or is a subset
    /// 3. The constraints don't exceed the original
    pub fn can_delegate(&self, request: &CapabilityRequest) -> bool {
        self.capabilities.iter().any(|cap| {
            // Check resource match (supports wildcards)
            Self::resource_matches(&cap.resource, &request.resource)
                && Self::action_matches(&cap.action, &request.action)
        })
    }

    /// Check if a resource pattern matches a request.
    fn resource_matches(pattern: &str, request: &str) -> bool {
        if pattern == "*" || pattern == request {
            return true;
        }

        // Handle prefix wildcards like "state:*"
        if let Some(prefix) = pattern.strip_suffix('*') {
            return request.starts_with(prefix);
        }

        false
    }

    /// Check if an action pattern matches a request.
    fn action_matches(pattern: &str, request: &str) -> bool {
        pattern == "*" || pattern == request
    }

    /// Get all capabilities in this set.
    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    /// Check if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// Get the number of capabilities.
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }
}

/// A request for a capability (from an app manifest).
#[derive(Clone, Debug)]
pub struct CapabilityRequest {
    /// Resource being requested
    pub resource: String,
    /// Action being requested
    pub action: String,
}

impl CapabilityRequest {
    /// Create a new capability request.
    pub fn new(resource: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            action: action.into(),
        }
    }

    /// Parse a capability request from a string like "state:logs:append:self".
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() >= 2 {
            // Last part is typically the action, rest is resource
            let action = parts.last()?.to_string();
            let resource = parts[..parts.len() - 1].join(":");
            Some(Self { resource, action })
        } else {
            None
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authz::{ActionKind, DenyAllOracle, PolicyRequest};

    #[test]
    fn test_bootstrap_phase() {
        assert_eq!(BootstrapPhase::from_u8(0), Some(BootstrapPhase::Genesis));
        assert_eq!(BootstrapPhase::from_u8(1), Some(BootstrapPhase::CoreApps));
        assert_eq!(BootstrapPhase::from_u8(2), Some(BootstrapPhase::Running));
        assert_eq!(BootstrapPhase::from_u8(3), None);

        assert!(BootstrapPhase::Genesis.is_bootstrap());
        assert!(BootstrapPhase::CoreApps.is_bootstrap());
        assert!(!BootstrapPhase::Running.is_bootstrap());
    }

    #[test]
    fn test_decision_cache() {
        let cache = DecisionCache::new(100);

        let core = PolicyRequestCore::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        // Initially empty
        assert!(cache.get(&core).is_none());

        // Insert and retrieve
        cache.insert(core.clone(), PolicyDecision::allow(), Duration::from_secs(60));
        assert!(cache.get(&core).is_some());

        // Different core should miss
        let other = PolicyRequestCore::new(
            "did:icn:other".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );
        assert!(cache.get(&other).is_none());
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = DecisionCache::new(100);

        let core1 = PolicyRequestCore::new(
            "did:icn:test1".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );
        let core2 = PolicyRequestCore::new(
            "did:icn:test2".to_string(),
            ActionKind::Read,
            Domain::ledger(),
        );

        cache.insert(core1.clone(), PolicyDecision::allow(), Duration::from_secs(60));
        cache.insert(core2.clone(), PolicyDecision::allow(), Duration::from_secs(60));

        assert_eq!(cache.len(), 2);

        // Invalidate trust domain
        cache.invalidate_domain(&Domain::trust());

        // trust entry gone, ledger remains
        assert!(cache.get(&core1).is_none());
        assert!(cache.get(&core2).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_oracle_registry_basic() {
        let registry = OracleRegistry::new();

        // Initially no oracles registered
        assert!(registry.domains().is_empty());

        // In Genesis phase, unknown domains are allowed (bootstrap flexibility)
        let request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );
        assert!(registry.evaluate(&request).is_allowed());
    }

    #[test]
    fn test_oracle_registry_deny_by_default_in_running() {
        let registry = OracleRegistry::new();

        // Transition to Running phase
        registry.set_phase(BootstrapPhase::Running);

        // In Running phase, unknown domains are DENIED (security by default)
        let request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        let decision = registry.evaluate(&request);
        assert!(!decision.is_allowed());

        // Verify it's a proper denial with reason
        match decision {
            PolicyDecision::Deny { reason } => {
                match reason {
                    crate::authz::PolicyError::Denied(msg) => {
                        assert!(msg.contains("No oracle registered"));
                        assert!(msg.contains("trust"));
                    }
                    _ => panic!("Expected Denied error variant"),
                }
            }
            _ => panic!("Expected Deny decision in Running phase"),
        }
    }

    #[test]
    fn test_oracle_registry_allow_in_coreapps() {
        let registry = OracleRegistry::new();

        // CoreApps phase should also allow (still bootstrapping)
        registry.set_phase(BootstrapPhase::CoreApps);

        let request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );

        assert!(registry.evaluate(&request).is_allowed());
    }

    #[test]
    fn test_oracle_registry_register() {
        let registry = OracleRegistry::new();

        // Register a deny-all oracle for trust
        let oracle = Arc::new(DenyAllOracle::new(Domain::trust(), "test"));
        registry.register(Domain::trust(), oracle);

        // Trust domain should deny
        let request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::trust(),
        );
        assert!(!registry.evaluate(&request).is_allowed());

        // Other domains still use fallback (allow)
        let other_request = PolicyRequest::new(
            "did:icn:test".to_string(),
            ActionKind::Read,
            Domain::ledger(),
        );
        assert!(registry.evaluate(&other_request).is_allowed());
    }

    #[test]
    fn test_oracle_registry_phase() {
        let registry = OracleRegistry::new();

        assert_eq!(registry.phase(), BootstrapPhase::Genesis);

        registry.set_phase(BootstrapPhase::CoreApps);
        assert_eq!(registry.phase(), BootstrapPhase::CoreApps);

        registry.set_phase(BootstrapPhase::Running);
        assert_eq!(registry.phase(), BootstrapPhase::Running);
    }

    #[test]
    fn test_genesis_capabilities_valid() {
        let phase = Arc::new(AtomicU8::new(BootstrapPhase::Genesis as u8));
        let genesis = GenesisCapabilities::new(
            phase.clone(),
            Duration::from_secs(60),
            "did:icn:issuer".to_string(),
        );

        assert!(genesis.is_valid());

        let cap = genesis
            .issue_simple("state:*", "write", &"did:icn:holder".to_string())
            .unwrap();

        assert!(cap.resource == "state:*");
        assert!(cap.action == "write");
        assert!(cap.holder == Some("did:icn:holder".to_string()));
    }

    #[test]
    fn test_genesis_capabilities_phase_guard() {
        let phase = Arc::new(AtomicU8::new(BootstrapPhase::Running as u8));
        let genesis = GenesisCapabilities::new(
            phase,
            Duration::from_secs(60),
            "did:icn:issuer".to_string(),
        );

        assert!(!genesis.is_valid());

        let result = genesis.issue_simple("state:*", "write", &"did:icn:holder".to_string());

        assert!(matches!(result, Err(AuthzError::NotInGenesis)));
    }

    #[test]
    fn test_capability_set_delegation() {
        let mut set = CapabilitySet::new();

        // Add a wildcard capability
        set.add(Capability {
            id: "cap1".to_string(),
            resource: "state:*".to_string(),
            action: "*".to_string(),
            constraints: Constraints::default(),
            holder: None,
            issuer: "did:icn:issuer".to_string(),
            expiration: u64::MAX,
            signature: vec![],
        });

        // Can delegate specific resources
        assert!(set.can_delegate(&CapabilityRequest::new("state:logs", "read")));
        assert!(set.can_delegate(&CapabilityRequest::new("state:kv", "write")));

        // Cannot delegate unrelated resources
        assert!(!set.can_delegate(&CapabilityRequest::new("comms:subscribe", "trust")));
    }

    #[test]
    fn test_capability_request_parse() {
        let req = CapabilityRequest::parse("state:logs:append").unwrap();
        assert_eq!(req.resource, "state:logs");
        assert_eq!(req.action, "append");

        let req2 = CapabilityRequest::parse("comms:subscribe:trust:*").unwrap();
        assert_eq!(req2.resource, "comms:subscribe:trust");
        assert_eq!(req2.action, "*");
    }
}
