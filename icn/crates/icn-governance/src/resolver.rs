//! Membership resolution - determining who can vote

use crate::{GovernanceDomain, MembershipSource};
use anyhow::Result;
use icn_identity::Did;
use std::collections::HashSet;

/// Trait for resolving membership from different sources
pub trait MembershipResolver: Send + Sync {
    /// Resolve the list of eligible voters for a domain
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
/// based on trust scores.
///
/// NOTE: This is a placeholder for future integration with icn-trust.
/// Full implementation will require TrustGraph access.
pub struct TrustMembershipResolver {
    // Future: Arc<RwLock<TrustGraph>>
    _trust_graph: (),
}

impl TrustMembershipResolver {
    pub fn new(/* trust_graph: Arc<RwLock<TrustGraph>> */) -> Self {
        Self {
            _trust_graph: (), // Placeholder
        }
    }
}

impl MembershipResolver for TrustMembershipResolver {
    fn resolve_members(&self, domain: &GovernanceDomain) -> Result<Vec<Did>> {
        match &domain.config.membership.source {
            MembershipSource::StaticList(members) => Ok(members.clone()),
            MembershipSource::TrustThreshold(_threshold) => {
                // Future implementation:
                // 1. Get all peers from trust graph
                // 2. Filter peers with trust score >= threshold
                // 3. Return their DIDs

                anyhow::bail!(
                    "TrustMembershipResolver trust graph integration not yet implemented"
                )
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
}
