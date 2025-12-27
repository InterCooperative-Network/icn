//! Vote delegation for liquid democracy
//!
//! This module enables members to delegate their voting power to trusted representatives.
//! Delegations can be scoped to specific proposals, domains, or be blanket delegations.
//!
//! # Delegation Rules
//!
//! - **No self-delegation**: A member cannot delegate to themselves
//! - **Cycle prevention**: Delegations cannot form cycles (A→B→C→A)
//! - **Max depth**: Transitive delegations are limited (default: 3 hops)
//! - **Conflict resolution**: If both delegator and delegate vote, delegator's vote wins
//! - **Expiry**: Delegations can have an optional expiration time
//!
//! # Eventual Consistency and Cycle Detection
//!
//! In ICN's gossip-based P2P system, proposal information may not be immediately
//! available on all nodes. When comparing domain and proposal scopes during cycle
//! detection, if the proposal's domain is unknown, we assume **no overlap** to
//! avoid blocking valid delegations.
//!
//! This means cycles may temporarily form if:
//! 1. Alice delegates Domain A → Bob
//! 2. Proposal X is created in Domain A on another node (not yet propagated)
//! 3. Bob delegates Proposal X → Alice (accepted because proposal unknown)
//! 4. Proposal info propagates, revealing the cycle
//!
//! **Reconciliation**: When a proposal is registered via [`DelegationManager::register_proposal`],
//! any cycles that are now detectable are logged and metrics are emitted. Operators
//! can monitor `icn_governance_delegation_cycles_detected_total` for alerts.
//!
//! # Example
//!
//! ```rust
//! use icn_governance::delegation::{Delegation, DelegationScope, DelegationManager};
//! use icn_identity::KeyPair;
//!
//! let alice = KeyPair::generate().unwrap().did().clone();
//! let bob = KeyPair::generate().unwrap().did().clone();
//!
//! let mut manager = DelegationManager::new();
//!
//! // Alice delegates all votes to Bob
//! let delegation = Delegation::new(
//!     alice.clone(),
//!     bob.clone(),
//!     DelegationScope::Blanket,
//! );
//!
//! manager.add_delegation(delegation).unwrap();
//!
//! // Check delegation
//! assert!(manager.get_delegate(&alice, &DelegationScope::Blanket).is_some());
//! ```

use crate::domain::GovernanceDomainId;
use crate::proposal::ProposalId;
use crate::Timestamp;
use anyhow::{bail, Result};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Maximum depth for transitive delegations
pub const DEFAULT_MAX_DELEGATION_DEPTH: usize = 3;

/// Unique identifier for a delegation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DelegationId(pub String);

impl DelegationId {
    /// Generate a new random delegation ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing ID string
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for DelegationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Scope of a vote delegation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DelegationScope {
    /// Delegate all votes across all domains
    Blanket,

    /// Delegate votes for a specific governance domain
    Domain(GovernanceDomainId),

    /// Delegate vote for a specific proposal only
    Proposal(ProposalId),
}

impl DelegationScope {
    /// Check if this scope matches a given proposal
    pub fn matches_proposal(
        &self,
        domain_id: &GovernanceDomainId,
        proposal_id: &ProposalId,
    ) -> bool {
        match self {
            DelegationScope::Blanket => true,
            DelegationScope::Domain(d) => d == domain_id,
            DelegationScope::Proposal(p) => p == proposal_id,
        }
    }
}

/// A vote delegation from one member to another
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delegation {
    /// Unique delegation ID
    pub id: DelegationId,

    /// DID of the member delegating their vote
    pub delegator: Did,

    /// DID of the member receiving the delegation
    pub delegate: Did,

    /// Scope of this delegation
    pub scope: DelegationScope,

    /// When the delegation was created
    pub created_at: Timestamp,

    /// Optional expiration timestamp (None = never expires)
    pub expires_at: Option<Timestamp>,

    /// When the delegation was revoked (None = still active)
    pub revoked_at: Option<Timestamp>,
}

impl Delegation {
    /// Create a new delegation
    pub fn new(delegator: Did, delegate: Did, scope: DelegationScope) -> Self {
        Self {
            id: DelegationId::generate(),
            delegator,
            delegate,
            scope,
            created_at: icn_time::current_timestamp_secs(),
            expires_at: None,
            revoked_at: None,
        }
    }

    /// Set expiration time
    ///
    /// Note: The expiry must be in the future (greater than created_at).
    /// If an invalid expiry is provided, the delegation is returned unchanged.
    pub fn with_expiry(mut self, expires_at: Timestamp) -> Self {
        // Validate: expiry must be after creation time
        if expires_at > self.created_at {
            self.expires_at = Some(expires_at);
        }
        self
    }

    /// Check if the delegation is currently active
    pub fn is_active(&self, now: Timestamp) -> bool {
        // Not revoked
        if self.revoked_at.is_some() {
            return false;
        }

        // Not expired
        if let Some(expires) = self.expires_at {
            if now >= expires {
                return false;
            }
        }

        true
    }

    /// Revoke this delegation
    pub fn revoke(&mut self) {
        self.revoked_at = Some(icn_time::current_timestamp_secs());
    }
}

/// Error types for delegation operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationError {
    /// Cannot delegate to yourself
    SelfDelegation,

    /// Delegation would create a cycle
    CycleDetected(Vec<Did>),

    /// Maximum delegation depth exceeded
    MaxDepthExceeded(usize),

    /// Delegation not found
    NotFound(DelegationId),

    /// Delegation already exists for this scope
    DuplicateDelegation,

    /// Delegation has expired
    Expired,

    /// Delegation was revoked
    Revoked,
}

impl std::fmt::Display for DelegationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DelegationError::SelfDelegation => write!(f, "Cannot delegate to yourself"),
            DelegationError::CycleDetected(cycle) => {
                write!(f, "Delegation would create a cycle: {cycle:?}")
            }
            DelegationError::MaxDepthExceeded(depth) => {
                write!(f, "Maximum delegation depth ({depth}) exceeded")
            }
            DelegationError::NotFound(id) => write!(f, "Delegation not found: {id}"),
            DelegationError::DuplicateDelegation => {
                write!(f, "Delegation already exists for this scope")
            }
            DelegationError::Expired => write!(f, "Delegation has expired"),
            DelegationError::Revoked => write!(f, "Delegation was revoked"),
        }
    }
}

impl std::error::Error for DelegationError {}

/// Manager for vote delegations
///
/// Handles delegation storage, validation, and resolution.
#[derive(Debug, Clone, Default)]
pub struct DelegationManager {
    /// All delegations by ID
    delegations: HashMap<DelegationId, Delegation>,

    /// Index: delegator -> delegations they've given
    delegations_from: HashMap<Did, Vec<DelegationId>>,

    /// Index: delegate -> delegations they've received
    delegations_to: HashMap<Did, Vec<DelegationId>>,

    /// Maximum allowed delegation depth
    max_depth: usize,

    /// Proposal to domain mapping for precise scope overlap checking
    proposal_domains: HashMap<ProposalId, GovernanceDomainId>,
}

impl DelegationManager {
    /// Create a new delegation manager
    pub fn new() -> Self {
        Self {
            delegations: HashMap::new(),
            delegations_from: HashMap::new(),
            delegations_to: HashMap::new(),
            max_depth: DEFAULT_MAX_DELEGATION_DEPTH,
            proposal_domains: HashMap::new(),
        }
    }

    /// Set maximum delegation depth
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Register a proposal's domain for precise scope overlap checking
    ///
    /// Call this when a proposal is created to enable accurate domain/proposal
    /// scope overlap detection during delegation cycle checking.
    ///
    /// # Cycle Reconciliation
    ///
    /// When a proposal is registered, this method checks for any delegation cycles
    /// that may have formed while the proposal's domain was unknown. In an eventually
    /// consistent, gossip-based system, delegations may be created before proposal
    /// information propagates to all nodes. This reconciliation detects those cycles
    /// and logs them for operator awareness.
    ///
    /// Cycles are NOT automatically revoked - they are logged and metrics are emitted
    /// so that governance can decide how to handle them.
    pub fn register_proposal(&mut self, proposal_id: ProposalId, domain_id: GovernanceDomainId) {
        self.proposal_domains
            .insert(proposal_id.clone(), domain_id.clone());

        // Reconcile: check for cycles that are now detectable with this proposal's domain
        let cycles = self.detect_cycles_for_proposal(&proposal_id, &domain_id);
        if !cycles.is_empty() {
            tracing::warn!(
                proposal_id = %proposal_id.0,
                domain_id = %domain_id.0,
                cycle_count = cycles.len(),
                "Delegation cycles detected after proposal registration - \
                 these may have formed during gossip propagation"
            );
            for (delegator, delegate, path) in &cycles {
                tracing::warn!(
                    delegator = %delegator,
                    delegate = %delegate,
                    path = ?path,
                    "Cycle detected: delegation from {} to {} creates cycle via proposal {}",
                    delegator, delegate, proposal_id.0
                );
            }
            // Emit metrics
            icn_obs::metrics::governance::delegation_cycle_detected_inc();
            icn_obs::metrics::governance::delegation_cycles_found_add(cycles.len() as u64);
        }
    }

    /// Detect delegation cycles that involve a specific proposal
    ///
    /// This is called during proposal registration to find cycles that may have
    /// formed while the proposal's domain was unknown.
    fn detect_cycles_for_proposal(
        &self,
        proposal_id: &ProposalId,
        domain_id: &GovernanceDomainId,
    ) -> Vec<(Did, Did, Vec<Did>)> {
        let now = icn_time::current_timestamp_secs();
        let mut cycles = Vec::new();

        // Find all delegations that reference this proposal
        let _proposal_scope = DelegationScope::Proposal(proposal_id.clone());
        let domain_scope = DelegationScope::Domain(domain_id.clone());

        for delegation in self.delegations.values() {
            if !delegation.is_active(now) {
                continue;
            }

            // Check if this delegation involves the proposal
            let involves_proposal = match &delegation.scope {
                DelegationScope::Proposal(p) => p == proposal_id,
                _ => false,
            };

            if !involves_proposal {
                continue;
            }

            // Now check if this delegation creates a cycle with domain delegations
            // by walking the chain from the delegate
            if self.would_create_cycle_with_scope(
                &delegation.delegator,
                &delegation.delegate,
                &domain_scope,
            ) {
                let path =
                    self.find_cycle(&delegation.delegator, &delegation.delegate, &domain_scope);
                cycles.push((
                    delegation.delegator.clone(),
                    delegation.delegate.clone(),
                    path,
                ));
            }
        }

        cycles
    }

    /// Check if a delegation would create a cycle when checked against a specific scope
    ///
    /// This is similar to would_create_cycle but uses an explicit scope for checking.
    fn would_create_cycle_with_scope(
        &self,
        delegator: &Did,
        delegate: &Did,
        scope: &DelegationScope,
    ) -> bool {
        let now = icn_time::current_timestamp_secs();
        let mut current = delegate.clone();
        let mut visited = HashSet::new();
        visited.insert(delegator.clone());

        for _ in 0..self.max_depth {
            if visited.contains(&current) {
                return true;
            }
            visited.insert(current.clone());

            // Find any active delegation from current that matches scope
            let next = self
                .delegations_from
                .get(&current)
                .and_then(|ids| {
                    ids.iter()
                        .filter_map(|id| self.delegations.get(id))
                        .find(|d| d.is_active(now) && self.scopes_overlap(&d.scope, scope))
                })
                .map(|d| d.delegate.clone());

            match next {
                Some(d) => current = d,
                None => return false,
            }
        }

        false
    }

    /// Unregister a proposal (e.g., when proposal is finalized)
    pub fn unregister_proposal(&mut self, proposal_id: &ProposalId) {
        self.proposal_domains.remove(proposal_id);
    }

    /// Get the domain for a proposal, if registered
    pub fn get_proposal_domain(&self, proposal_id: &ProposalId) -> Option<&GovernanceDomainId> {
        self.proposal_domains.get(proposal_id)
    }

    /// Add a new delegation
    pub fn add_delegation(&mut self, delegation: Delegation) -> Result<()> {
        // Validate: no self-delegation
        if delegation.delegator == delegation.delegate {
            bail!(DelegationError::SelfDelegation);
        }

        // Validate: no cycles
        if self.would_create_cycle(
            &delegation.delegator,
            &delegation.delegate,
            &delegation.scope,
        ) {
            let cycle = self.find_cycle(
                &delegation.delegator,
                &delegation.delegate,
                &delegation.scope,
            );
            bail!(DelegationError::CycleDetected(cycle));
        }

        // Validate: max depth
        // Check how many hops lead TO the delegator (incoming chain length)
        // Adding this delegation would add one more hop to those chains
        let incoming_depth = self.compute_incoming_depth(&delegation.delegator, &delegation.scope);
        if incoming_depth >= self.max_depth {
            bail!(DelegationError::MaxDepthExceeded(self.max_depth));
        }

        // Check for duplicate (same delegator + scope)
        let existing = self.get_active_delegation(&delegation.delegator, &delegation.scope);
        if existing.is_some() {
            bail!(DelegationError::DuplicateDelegation);
        }

        // Store the delegation
        let id = delegation.id.clone();
        let delegator = delegation.delegator.clone();
        let delegate = delegation.delegate.clone();

        self.delegations.insert(id.clone(), delegation);
        self.delegations_from
            .entry(delegator)
            .or_default()
            .push(id.clone());
        self.delegations_to.entry(delegate).or_default().push(id);

        Ok(())
    }

    /// Revoke a delegation by ID
    pub fn revoke_delegation(&mut self, id: &DelegationId) -> Result<()> {
        let delegation = self
            .delegations
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!(DelegationError::NotFound(id.clone())))?;

        delegation.revoke();
        Ok(())
    }

    /// Get a delegation by ID
    pub fn get_delegation(&self, id: &DelegationId) -> Option<&Delegation> {
        self.delegations.get(id)
    }

    /// Get active delegation for a delegator and scope
    pub fn get_active_delegation(
        &self,
        delegator: &Did,
        scope: &DelegationScope,
    ) -> Option<&Delegation> {
        let now = icn_time::current_timestamp_secs();
        self.delegations_from
            .get(delegator)?
            .iter()
            .filter_map(|id| self.delegations.get(id))
            .find(|d| d.is_active(now) && d.scope == *scope)
    }

    /// Get the delegate for a delegator and scope (if any active delegation exists)
    pub fn get_delegate(&self, delegator: &Did, scope: &DelegationScope) -> Option<&Did> {
        self.get_active_delegation(delegator, scope)
            .map(|d| &d.delegate)
    }

    /// Get all delegations from a delegator
    pub fn get_delegations_from(&self, delegator: &Did) -> Vec<&Delegation> {
        self.delegations_from
            .get(delegator)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.delegations.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all delegations to a delegate
    pub fn get_delegations_to(&self, delegate: &Did) -> Vec<&Delegation> {
        self.delegations_to
            .get(delegate)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.delegations.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get active delegations from a delegator
    pub fn get_active_delegations_from(&self, delegator: &Did) -> Vec<&Delegation> {
        let now = icn_time::current_timestamp_secs();
        self.get_delegations_from(delegator)
            .into_iter()
            .filter(|d| d.is_active(now))
            .collect()
    }

    /// Get active delegations to a delegate
    pub fn get_active_delegations_to(&self, delegate: &Did) -> Vec<&Delegation> {
        let now = icn_time::current_timestamp_secs();
        self.get_delegations_to(delegate)
            .into_iter()
            .filter(|d| d.is_active(now))
            .collect()
    }

    /// Resolve the final delegate for a voter on a specific proposal
    ///
    /// Follows the delegation chain up to max_depth, returning the final delegate.
    /// Returns the original voter if they have no delegation.
    pub fn resolve_delegate(
        &self,
        voter: &Did,
        domain_id: &GovernanceDomainId,
        proposal_id: &ProposalId,
    ) -> Did {
        let now = icn_time::current_timestamp_secs();
        let mut current = voter.clone();
        let mut visited = HashSet::new();

        for _ in 0..self.max_depth {
            visited.insert(current.clone());

            // Find matching delegation (proposal > domain > blanket priority)
            let delegation = self.find_matching_delegation(&current, domain_id, proposal_id, now);

            match delegation {
                Some(d) => {
                    // Prevent infinite loops
                    if visited.contains(&d.delegate) {
                        break;
                    }
                    current = d.delegate.clone();
                }
                None => break,
            }
        }

        current
    }

    /// Find all delegators who have delegated to a specific delegate for a proposal
    pub fn find_delegators(
        &self,
        delegate: &Did,
        domain_id: &GovernanceDomainId,
        proposal_id: &ProposalId,
    ) -> Vec<Did> {
        let now = icn_time::current_timestamp_secs();
        let mut delegators = Vec::new();

        for delegation in self.get_active_delegations_to(delegate) {
            if delegation.is_active(now)
                && delegation.scope.matches_proposal(domain_id, proposal_id)
            {
                delegators.push(delegation.delegator.clone());
            }
        }

        delegators
    }

    // === Private Methods ===

    /// Find a matching delegation for a voter on a proposal
    ///
    /// Priority: Proposal-specific > Domain-specific > Blanket
    fn find_matching_delegation(
        &self,
        delegator: &Did,
        domain_id: &GovernanceDomainId,
        proposal_id: &ProposalId,
        now: Timestamp,
    ) -> Option<&Delegation> {
        let delegations = self.get_delegations_from(delegator);

        // Check proposal-specific first
        if let Some(d) = delegations
            .iter()
            .find(|d| d.is_active(now) && d.scope == DelegationScope::Proposal(proposal_id.clone()))
        {
            return Some(d);
        }

        // Check domain-specific
        if let Some(d) = delegations
            .iter()
            .find(|d| d.is_active(now) && d.scope == DelegationScope::Domain(domain_id.clone()))
        {
            return Some(d);
        }

        // Check blanket
        delegations
            .iter()
            .find(|d| d.is_active(now) && d.scope == DelegationScope::Blanket)
            .copied()
    }

    /// Check if adding a delegation would create a cycle
    fn would_create_cycle(&self, delegator: &Did, delegate: &Did, scope: &DelegationScope) -> bool {
        let now = icn_time::current_timestamp_secs();
        let mut current = delegate.clone();
        let mut visited = HashSet::new();
        visited.insert(delegator.clone());

        for _ in 0..self.max_depth {
            if visited.contains(&current) {
                return true;
            }
            visited.insert(current.clone());

            // Find any active delegation from current that matches scope
            let next = self
                .delegations_from
                .get(&current)
                .and_then(|ids| {
                    ids.iter()
                        .filter_map(|id| self.delegations.get(id))
                        .find(|d| d.is_active(now) && self.scopes_overlap(&d.scope, scope))
                })
                .map(|d| d.delegate.clone());

            match next {
                Some(d) => current = d,
                None => return false,
            }
        }

        false
    }

    /// Find the cycle path (for error reporting)
    fn find_cycle(&self, delegator: &Did, delegate: &Did, scope: &DelegationScope) -> Vec<Did> {
        let now = icn_time::current_timestamp_secs();
        let mut path = vec![delegator.clone(), delegate.clone()];
        let mut current = delegate.clone();

        for _ in 0..self.max_depth {
            let next = self
                .delegations_from
                .get(&current)
                .and_then(|ids| {
                    ids.iter()
                        .filter_map(|id| self.delegations.get(id))
                        .find(|d| d.is_active(now) && self.scopes_overlap(&d.scope, scope))
                })
                .map(|d| d.delegate.clone());

            match next {
                Some(d) => {
                    if path.contains(&d) {
                        path.push(d);
                        return path;
                    }
                    path.push(d.clone());
                    current = d;
                }
                None => break,
            }
        }

        path
    }

    /// Check if two scopes overlap (for cycle detection)
    ///
    /// This function is used to determine if two delegations could conflict.
    /// Uses registered proposal domains for precise overlap checking when available.
    ///
    /// # Behavior
    ///
    /// - Blanket scope overlaps with all scopes
    /// - Domain scopes overlap only if they match
    /// - Proposal scopes overlap only if they match
    /// - Domain + Proposal: uses registered domain mapping for precise check.
    ///   If proposal is not registered, assumes NO overlap (proposal-specific
    ///   delegations are narrower than domain delegations, so actual conflicts
    ///   will be detected when the proposal is properly registered).
    fn scopes_overlap(&self, a: &DelegationScope, b: &DelegationScope) -> bool {
        match (a, b) {
            (DelegationScope::Blanket, _) | (_, DelegationScope::Blanket) => true,
            (DelegationScope::Domain(d1), DelegationScope::Domain(d2)) => d1 == d2,
            (DelegationScope::Domain(d), DelegationScope::Proposal(p))
            | (DelegationScope::Proposal(p), DelegationScope::Domain(d)) => {
                // Use registered domain mapping for precise check
                match self.proposal_domains.get(p) {
                    Some(proposal_domain) => proposal_domain == d,
                    // Proposal-specific delegations are narrower than domain delegations,
                    // so assume no overlap when proposal domain is unknown
                    None => false,
                }
            }
            (DelegationScope::Proposal(p1), DelegationScope::Proposal(p2)) => p1 == p2,
        }
    }

    /// Compute the incoming delegation chain depth to a person
    ///
    /// This counts how many hops lead TO this person as a delegate.
    /// For example, if Alice -> Bob -> Charlie, then Charlie has incoming depth 2.
    fn compute_incoming_depth(&self, delegate: &Did, scope: &DelegationScope) -> usize {
        let mut visited = HashSet::new();
        self.compute_incoming_depth_with_visited(delegate, scope, &mut visited)
    }

    /// Helper for compute_incoming_depth with visited set to prevent infinite recursion
    fn compute_incoming_depth_with_visited(
        &self,
        delegate: &Did,
        scope: &DelegationScope,
        visited: &mut HashSet<Did>,
    ) -> usize {
        // Prevent infinite recursion if we've already visited this delegate
        if visited.contains(delegate) {
            return 0;
        }
        visited.insert(delegate.clone());

        // Safety limit to prevent stack overflow even with corrupted data
        if visited.len() > self.max_depth + 10 {
            return 0;
        }

        let now = icn_time::current_timestamp_secs();

        // Find all delegators who have delegated to this delegate
        let delegators: Vec<Did> = self
            .delegations_to
            .get(delegate)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.delegations.get(id))
                    .filter(|d| d.is_active(now) && self.scopes_overlap(&d.scope, scope))
                    .map(|d| d.delegator.clone())
                    .collect()
            })
            .unwrap_or_default();

        if delegators.is_empty() {
            return 0;
        }

        // Recursively find the maximum depth
        let mut max_depth = 0;
        for delegator in delegators {
            let depth = 1 + self.compute_incoming_depth_with_visited(&delegator, scope, visited);
            if depth > max_depth {
                max_depth = depth;
            }
        }

        max_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did(seed: u8) -> Did {
        // Create deterministic DIDs for testing
        Did::from_anchor_id(&[seed; 32])
    }

    #[test]
    fn test_create_delegation() {
        let alice = test_did(1);
        let bob = test_did(2);

        let delegation = Delegation::new(alice.clone(), bob.clone(), DelegationScope::Blanket);

        assert_eq!(delegation.delegator, alice);
        assert_eq!(delegation.delegate, bob);
        assert!(delegation.is_active(icn_time::current_timestamp_secs()));
    }

    #[test]
    fn test_delegation_expiry() {
        let alice = test_did(1);
        let bob = test_did(2);

        let now = icn_time::current_timestamp_secs();
        let delegation =
            Delegation::new(alice, bob, DelegationScope::Blanket).with_expiry(now + 3600);

        // Active now
        assert!(delegation.is_active(now));

        // Expired in the future
        assert!(!delegation.is_active(now + 3601));
    }

    #[test]
    fn test_delegation_revoke() {
        let alice = test_did(1);
        let bob = test_did(2);

        let mut delegation = Delegation::new(alice, bob, DelegationScope::Blanket);

        assert!(delegation.is_active(icn_time::current_timestamp_secs()));

        delegation.revoke();

        assert!(!delegation.is_active(icn_time::current_timestamp_secs()));
    }

    #[test]
    fn test_manager_add_delegation() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let delegation = Delegation::new(alice.clone(), bob.clone(), DelegationScope::Blanket);

        manager.add_delegation(delegation).unwrap();

        assert!(manager
            .get_delegate(&alice, &DelegationScope::Blanket)
            .is_some());
        assert_eq!(
            manager
                .get_delegate(&alice, &DelegationScope::Blanket)
                .unwrap(),
            &bob
        );
    }

    #[test]
    fn test_self_delegation_fails() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);

        let delegation = Delegation::new(alice.clone(), alice.clone(), DelegationScope::Blanket);

        let result = manager.add_delegation(delegation);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("yourself"));
    }

    #[test]
    fn test_cycle_detection() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);

        // Alice -> Bob
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Bob -> Charlie
        manager
            .add_delegation(Delegation::new(
                bob.clone(),
                charlie.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Charlie -> Alice would create a cycle
        let result = manager.add_delegation(Delegation::new(
            charlie.clone(),
            alice.clone(),
            DelegationScope::Blanket,
        ));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn test_max_depth() {
        let mut manager = DelegationManager::new().with_max_depth(2);
        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);
        let dave = test_did(4);

        // Alice -> Bob (depth 0)
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Bob -> Charlie (depth 1)
        manager
            .add_delegation(Delegation::new(
                bob.clone(),
                charlie.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Charlie -> Dave would exceed max depth (2)
        let result = manager.add_delegation(Delegation::new(
            charlie.clone(),
            dave.clone(),
            DelegationScope::Blanket,
        ));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("depth"));
    }

    #[test]
    fn test_resolve_delegate_single() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        let resolved = manager.resolve_delegate(&alice, &domain_id, &proposal_id);
        assert_eq!(resolved, bob);
    }

    #[test]
    fn test_resolve_delegate_transitive() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        // Alice -> Bob -> Charlie
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        manager
            .add_delegation(Delegation::new(
                bob.clone(),
                charlie.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        let resolved = manager.resolve_delegate(&alice, &domain_id, &proposal_id);
        assert_eq!(resolved, charlie);
    }

    #[test]
    fn test_scope_priority() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);
        let domain_id = GovernanceDomainId::new("test-coop");
        let proposal_id = ProposalId::generate();

        // Blanket delegation to Bob
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Proposal-specific delegation to Charlie (higher priority)
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                charlie.clone(),
                DelegationScope::Proposal(proposal_id.clone()),
            ))
            .unwrap();

        let resolved = manager.resolve_delegate(&alice, &domain_id, &proposal_id);
        // Should resolve to Charlie (proposal-specific takes priority)
        assert_eq!(resolved, charlie);
    }

    #[test]
    fn test_duplicate_delegation_fails() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);

        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Blanket,
            ))
            .unwrap();

        // Try to add another blanket delegation
        let result = manager.add_delegation(Delegation::new(
            alice.clone(),
            charlie.clone(),
            DelegationScope::Blanket,
        ));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_revoke_delegation() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let delegation = Delegation::new(alice.clone(), bob.clone(), DelegationScope::Blanket);
        let id = delegation.id.clone();

        manager.add_delegation(delegation).unwrap();

        assert!(manager
            .get_delegate(&alice, &DelegationScope::Blanket)
            .is_some());

        manager.revoke_delegation(&id).unwrap();

        // After revocation, should not find active delegation
        assert!(manager
            .get_delegate(&alice, &DelegationScope::Blanket)
            .is_none());
    }

    #[test]
    fn test_precise_domain_proposal_no_overlap() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let domain_a = GovernanceDomainId::new("domain-a");
        let domain_b = GovernanceDomainId::new("domain-b");
        let proposal_in_b = ProposalId::new("proposal-in-domain-b");

        // Register that the proposal is in domain B
        manager.register_proposal(proposal_in_b.clone(), domain_b.clone());

        // Alice delegates domain A to Bob
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Domain(domain_a.clone()),
            ))
            .unwrap();

        // Bob delegates proposal (in domain B) to Alice
        // This should succeed because domain A != domain B (no cycle)
        let result = manager.add_delegation(Delegation::new(
            bob.clone(),
            alice.clone(),
            DelegationScope::Proposal(proposal_in_b.clone()),
        ));

        assert!(result.is_ok(), "Expected no cycle but got: {result:?}");
    }

    #[test]
    fn test_precise_domain_proposal_with_overlap() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let domain_a = GovernanceDomainId::new("domain-a");
        let proposal_in_a = ProposalId::new("proposal-in-domain-a");

        // Register that the proposal is in domain A
        manager.register_proposal(proposal_in_a.clone(), domain_a.clone());

        // Alice delegates domain A to Bob
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Domain(domain_a.clone()),
            ))
            .unwrap();

        // Bob delegates proposal (in domain A) to Alice
        // This should fail because domain A == proposal's domain (cycle detected)
        let result = manager.add_delegation(Delegation::new(
            bob.clone(),
            alice.clone(),
            DelegationScope::Proposal(proposal_in_a.clone()),
        ));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn test_unregistered_proposal_no_overlap_assumed() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let domain_a = GovernanceDomainId::new("domain-a");
        let unknown_proposal = ProposalId::new("unknown-proposal");

        // Don't register the proposal - should assume no overlap with domain

        // Alice delegates domain A to Bob
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Domain(domain_a.clone()),
            ))
            .unwrap();

        // Bob delegates unknown proposal to Alice
        // Should succeed - proposal-specific delegations are narrower than domain,
        // so we assume no overlap when the proposal's domain is unknown
        let result = manager.add_delegation(Delegation::new(
            bob.clone(),
            alice.clone(),
            DelegationScope::Proposal(unknown_proposal.clone()),
        ));

        assert!(
            result.is_ok(),
            "Expected delegation to succeed but got: {result:?}"
        );
    }

    #[test]
    fn test_cycle_detected_on_proposal_registration() {
        let mut manager = DelegationManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let domain_a = GovernanceDomainId::new("domain-a");
        let proposal_in_a = ProposalId::new("proposal-in-domain-a");

        // Scenario: Create a cycle that's not detected initially because
        // proposal's domain is unknown, then register the proposal and
        // verify the cycle is detected during reconciliation.

        // Step 1: Alice delegates domain A to Bob
        manager
            .add_delegation(Delegation::new(
                alice.clone(),
                bob.clone(),
                DelegationScope::Domain(domain_a.clone()),
            ))
            .unwrap();

        // Step 2: Bob delegates proposal to Alice (accepted - proposal domain unknown)
        manager
            .add_delegation(Delegation::new(
                bob.clone(),
                alice.clone(),
                DelegationScope::Proposal(proposal_in_a.clone()),
            ))
            .unwrap();

        // Step 3: Now register the proposal - cycle should be detected
        // The cycle exists: Alice --(domain A)--> Bob --(proposal in A)--> Alice
        manager.register_proposal(proposal_in_a.clone(), domain_a.clone());

        // Verify cycle is detected via detect_cycles_for_proposal
        let cycles = manager.detect_cycles_for_proposal(&proposal_in_a, &domain_a);
        assert!(
            !cycles.is_empty(),
            "Expected cycle to be detected after proposal registration"
        );
        assert_eq!(cycles.len(), 1, "Expected exactly one cycle");

        // Verify the cycle involves the correct participants
        let (delegator, delegate, path) = &cycles[0];
        assert_eq!(
            delegator, &bob,
            "Cycle should start from Bob (the proposal delegator)"
        );
        assert_eq!(
            delegate, &alice,
            "Cycle should go to Alice (the proposal delegate)"
        );
        assert!(
            path.contains(&alice) && path.contains(&bob),
            "Path should contain both Alice and Bob"
        );
    }
}
