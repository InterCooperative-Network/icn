//! Membership resolution - determining who can vote

use crate::{GovernanceDomain, MembershipSource};
use anyhow::Result;
use icn_identity::Did;
use icn_trust::TrustGraph;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for resolving membership from different sources
///
/// # Sync vs Async
///
/// This trait provides synchronous methods for compatibility with sync code paths.
/// Implementations that access async resources (like `TrustMembershipResolver`) use
/// `block_in_place` internally to safely block from async contexts.
///
/// For better performance in async code, use the async methods directly on concrete
/// types (e.g., `TrustMembershipResolver::resolve_members_async`) when possible.
pub trait MembershipResolver: Send + Sync {
    /// Resolve the list of eligible voters for a domain
    ///
    /// # Note
    ///
    /// Some implementations (e.g., `TrustMembershipResolver`) may block the current
    /// thread briefly. In async contexts, consider using type-specific async methods.
    fn resolve_members(&self, domain: &GovernanceDomain) -> Result<Vec<Did>>;

    /// Check if a specific DID is an eligible voter
    fn is_member(&self, domain: &GovernanceDomain, did: &Did) -> Result<bool> {
        let members = self.resolve_members(domain)?;
        Ok(members.contains(did))
    }

    /// Get the count of eligible voters
    fn member_count(&self, domain: &GovernanceDomain) -> Result<usize> {
        Ok(self.resolve_members(domain)?.len())
    }
}

/// Static membership resolver (no trust graph integration)
///
/// This resolver only handles StaticList membership sources.
/// For TrustThreshold sources, it returns an error.
pub struct StaticMembershipResolver;

impl StaticMembershipResolver {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StaticMembershipResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl MembershipResolver for StaticMembershipResolver {
    fn resolve_members(&self, domain: &GovernanceDomain) -> Result<Vec<Did>> {
        match &domain.config.membership.source {
            MembershipSource::StaticList(members) => Ok(members.clone()),
            MembershipSource::TrustThreshold(_) => {
                anyhow::bail!("StaticMembershipResolver cannot handle TrustThreshold sources")
            }
        }
    }
}

/// Trust graph-based membership resolver
///
/// This resolver integrates with the trust graph to determine membership
/// based on trust scores. Members are those whose trust score (from the
/// node's perspective) meets or exceeds the configured threshold.
pub struct TrustMembershipResolver {
    trust_graph: Arc<RwLock<TrustGraph>>,
}

impl TrustMembershipResolver {
    /// Create a new trust-based membership resolver
    pub fn new(trust_graph: Arc<RwLock<TrustGraph>>) -> Self {
        Self { trust_graph }
    }

    /// Get the trust graph reference
    pub fn trust_graph(&self) -> &Arc<RwLock<TrustGraph>> {
        &self.trust_graph
    }
}

impl MembershipResolver for TrustMembershipResolver {
    fn resolve_members(&self, domain: &GovernanceDomain) -> Result<Vec<Did>> {
        match &domain.config.membership.source {
            MembershipSource::StaticList(members) => Ok(members.clone()),
            MembershipSource::TrustThreshold(threshold) => {
                // SAFETY: Use block_in_place to safely run blocking code from async context.
                // This allows other Tokio tasks to be moved off the current thread before
                // blocking. The MembershipResolver trait is sync, so we must block here.
                // For better async performance, use resolve_members_async instead when you
                // have a concrete TrustMembershipResolver reference.
                let threshold = *threshold;
                tokio::task::block_in_place(|| {
                    let graph = self.trust_graph.blocking_read();
                    graph.get_dids_above_threshold(threshold)
                })
            }
        }
    }

    fn is_member(&self, domain: &GovernanceDomain, did: &Did) -> Result<bool> {
        match &domain.config.membership.source {
            MembershipSource::StaticList(members) => Ok(members.contains(did)),
            MembershipSource::TrustThreshold(threshold) => {
                // SAFETY: Use block_in_place to safely run blocking code from async context.
                // See resolve_members for details. For async contexts, use is_member_async.
                let threshold = *threshold;
                tokio::task::block_in_place(|| {
                    let graph = self.trust_graph.blocking_read();
                    let score = graph.compute_trust_score(did)?;
                    Ok(score >= threshold)
                })
            }
        }
    }
}

impl TrustMembershipResolver {
    /// Async version of resolve_members for use in async contexts
    pub async fn resolve_members_async(&self, domain: &GovernanceDomain) -> Result<Vec<Did>> {
        match &domain.config.membership.source {
            MembershipSource::StaticList(members) => Ok(members.clone()),
            MembershipSource::TrustThreshold(threshold) => {
                let graph = self.trust_graph.read().await;
                graph.get_dids_above_threshold(*threshold)
            }
        }
    }

    /// Async version of is_member for use in async contexts
    pub async fn is_member_async(&self, domain: &GovernanceDomain, did: &Did) -> Result<bool> {
        match &domain.config.membership.source {
            MembershipSource::StaticList(members) => Ok(members.contains(did)),
            MembershipSource::TrustThreshold(threshold) => {
                let graph = self.trust_graph.read().await;
                let score = graph.compute_trust_score(did)?;
                Ok(score >= *threshold)
            }
        }
    }
}

/// Composite membership resolver
///
/// This resolver tries multiple strategies in order:
/// 1. Static list lookup
/// 2. Trust graph lookup (if available)
/// 3. Returns union of all resolved members
pub struct CompositeMembershipResolver {
    resolvers: Vec<Box<dyn MembershipResolver>>,
}

impl CompositeMembershipResolver {
    pub fn new() -> Self {
        Self {
            resolvers: Vec::new(),
        }
    }

    /// Add a resolver to the chain
    pub fn add_resolver(mut self, resolver: Box<dyn MembershipResolver>) -> Self {
        self.resolvers.push(resolver);
        self
    }
}

impl Default for CompositeMembershipResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl MembershipResolver for CompositeMembershipResolver {
    fn resolve_members(&self, domain: &GovernanceDomain) -> Result<Vec<Did>> {
        let mut all_members = HashSet::new();
        let mut last_error = None;

        for resolver in &self.resolvers {
            match resolver.resolve_members(domain) {
                Ok(members) => {
                    all_members.extend(members);
                }
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        if all_members.is_empty() {
            if let Some(err) = last_error {
                return Err(err);
            }
            anyhow::bail!("No resolvers could determine membership");
        }

        Ok(all_members.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GovernanceConfig, MembershipConfig};
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use icn_trust::TrustEdge;

    #[test]
    fn test_static_resolver_with_static_list() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();
        let did2 = kp2.did().clone();

        let membership = MembershipConfig::static_list(vec![did1.clone(), did2.clone()]);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = StaticMembershipResolver::new();
        let members = resolver.resolve_members(&domain).unwrap();

        assert_eq!(members.len(), 2);
        assert!(members.contains(&did1));
        assert!(members.contains(&did2));
    }

    #[test]
    fn test_static_resolver_with_trust_threshold_fails() {
        let membership = MembershipConfig::trust_threshold(0.5);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = StaticMembershipResolver::new();
        let result = resolver.resolve_members(&domain);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot handle TrustThreshold"));
    }

    #[test]
    fn test_is_member() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let kp3 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();
        let did2 = kp2.did().clone();
        let did3 = kp3.did().clone();

        let membership = MembershipConfig::static_list(vec![did1.clone(), did2.clone()]);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = StaticMembershipResolver::new();

        assert!(resolver.is_member(&domain, &did1).unwrap());
        assert!(resolver.is_member(&domain, &did2).unwrap());
        assert!(!resolver.is_member(&domain, &did3).unwrap());
    }

    #[test]
    fn test_member_count() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();
        let did2 = kp2.did().clone();

        let membership = MembershipConfig::static_list(vec![did1, did2]);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = StaticMembershipResolver::new();
        let count = resolver.member_count(&domain).unwrap();

        assert_eq!(count, 2);
    }

    #[test]
    fn test_composite_resolver() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();
        let did2 = kp2.did().clone();

        let membership = MembershipConfig::static_list(vec![did1.clone(), did2.clone()]);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = CompositeMembershipResolver::new()
            .add_resolver(Box::new(StaticMembershipResolver::new()));

        let members = resolver.resolve_members(&domain).unwrap();

        assert_eq!(members.len(), 2);
        assert!(members.contains(&did1));
        assert!(members.contains(&did2));
    }

    #[test]
    fn test_trust_resolver_with_static_list() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let owner = KeyPair::generate().unwrap();
        let owner_did = owner.did().clone();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();
        let did2 = kp2.did().clone();

        let graph = TrustGraph::new(store, owner_did);
        let trust_graph = Arc::new(RwLock::new(graph));

        let membership = MembershipConfig::static_list(vec![did1.clone(), did2.clone()]);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = TrustMembershipResolver::new(trust_graph);
        let members = resolver.resolve_members(&domain).unwrap();

        assert_eq!(members.len(), 2);
        assert!(members.contains(&did1));
        assert!(members.contains(&did2));
    }

    #[test]
    fn test_trust_resolver_with_trust_threshold() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let owner = KeyPair::generate().unwrap();
        let owner_did = owner.did().clone();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let kp3 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();
        let did2 = kp2.did().clone();
        let did3 = kp3.did().clone();

        let mut graph = TrustGraph::new(store, owner_did.clone());

        // Owner trusts did1 highly (0.8 direct = 0.56 score)
        graph
            .add_edge(TrustEdge::new(owner_did.clone(), did1.clone(), 0.8))
            .unwrap();

        // Owner trusts did2 moderately (0.5 direct = 0.35 score)
        graph
            .add_edge(TrustEdge::new(owner_did.clone(), did2.clone(), 0.5))
            .unwrap();

        // Owner has low trust in did3 (0.2 direct = 0.14 score)
        graph
            .add_edge(TrustEdge::new(owner_did.clone(), did3.clone(), 0.2))
            .unwrap();

        let trust_graph = Arc::new(RwLock::new(graph));

        // Threshold 0.3: should include did1 (0.56) and did2 (0.35)
        let membership = MembershipConfig::trust_threshold(0.3);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = TrustMembershipResolver::new(trust_graph);
        let members = resolver.resolve_members(&domain).unwrap();

        assert_eq!(members.len(), 2);
        assert!(members.contains(&did1));
        assert!(members.contains(&did2));
        assert!(!members.contains(&did3));
    }

    #[test]
    fn test_trust_resolver_is_member() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let owner = KeyPair::generate().unwrap();
        let owner_did = owner.did().clone();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();
        let did2 = kp2.did().clone();

        let mut graph = TrustGraph::new(store, owner_did.clone());

        // Owner trusts did1 highly (0.8 direct = 0.56 score)
        graph
            .add_edge(TrustEdge::new(owner_did.clone(), did1.clone(), 0.8))
            .unwrap();

        // Owner has low trust in did2 (0.2 direct = 0.14 score)
        graph
            .add_edge(TrustEdge::new(owner_did.clone(), did2.clone(), 0.2))
            .unwrap();

        let trust_graph = Arc::new(RwLock::new(graph));

        // Threshold 0.5: only did1 should be a member
        let membership = MembershipConfig::trust_threshold(0.5);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = TrustMembershipResolver::new(trust_graph);

        assert!(resolver.is_member(&domain, &did1).unwrap());
        assert!(!resolver.is_member(&domain, &did2).unwrap());
    }

    #[tokio::test]
    async fn test_trust_resolver_async_methods() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let owner = KeyPair::generate().unwrap();
        let owner_did = owner.did().clone();

        let kp1 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();

        let mut graph = TrustGraph::new(store, owner_did.clone());

        // Owner trusts did1 (0.8 direct = 0.56 score)
        graph
            .add_edge(TrustEdge::new(owner_did.clone(), did1.clone(), 0.8))
            .unwrap();

        let trust_graph = Arc::new(RwLock::new(graph));

        // Threshold 0.5: did1 should be a member
        let membership = MembershipConfig::trust_threshold(0.5);
        let mut config = GovernanceConfig::cooperative_default();
        config.membership = membership;

        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        let resolver = TrustMembershipResolver::new(trust_graph);

        // Test async resolve
        let members = resolver.resolve_members_async(&domain).await.unwrap();
        assert_eq!(members.len(), 1);
        assert!(members.contains(&did1));

        // Test async is_member
        assert!(resolver.is_member_async(&domain, &did1).await.unwrap());
    }
}
