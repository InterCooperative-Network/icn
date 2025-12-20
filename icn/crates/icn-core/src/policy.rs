//! Centralized trust policy engine
//!
//! This module provides a unified interface for all trust-based access decisions
//! across ICN subsystems. It combines trust class computation with capability-based
//! access control to determine what operations a peer is allowed to perform.
//!
//! # Architecture
//!
//! - **PolicySource trait**: Abstraction for policy lookup
//! - **TrustPolicy struct**: Per-peer policy combining limits + capabilities
//! - **Capability enum**: Fine-grained permissions for operations
//! - **DefaultPolicySource**: Default implementation using TrustGraph
//!
//! # Usage
//!
//! ```rust,no_run
//! use icn_core::policy::{DefaultPolicySource, PolicySource, Capability};
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create policy source from trust graph
//! let keypair = icn_identity::KeyPair::generate()?;
//! let own_did = keypair.did().clone();
//! let store = Arc::new(icn_store::SledStore::temporary()?);
//! let trust_graph = Arc::new(RwLock::new(icn_trust::TrustGraph::new(store, own_did)));
//! let policy_source = DefaultPolicySource::new(trust_graph);
//!
//! // Look up policy for a peer
//! let peer_did = icn_identity::KeyPair::generate()?.did().clone();
//! let policy = policy_source.policy_for(&peer_did).await;
//!
//! // Check capabilities
//! if policy.has_capability(&Capability::DeployContract) {
//!     // Allow contract deployment
//! }
//! # Ok(())
//! # }
//! ```

use icn_identity::Did;
use icn_trust::{TrustClass, TrustGraph};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for looking up trust policies
///
/// This abstraction allows for different policy sources:
/// - DefaultPolicySource: Uses TrustGraph
/// - StaticPolicySource: Fixed policies for testing
/// - RemotePolicySource: Fetch policies from remote server
#[async_trait::async_trait]
pub trait PolicySource: Send + Sync {
    /// Get the trust policy for a DID
    async fn policy_for(&self, did: &Did) -> TrustPolicy;
}

/// Trust-based policy for a peer
///
/// Combines resource limits with capability-based permissions.
#[derive(Debug, Clone)]
pub struct TrustPolicy {
    /// Trust class of the peer
    pub class: TrustClass,

    /// Maximum messages per second from this peer
    pub max_messages_per_second: u32,

    /// Maximum concurrent QUIC streams
    pub max_streams: u32,

    /// Allowed gossip topics (empty = all public topics allowed)
    pub allowed_topics: Vec<String>,

    /// Allowed capabilities
    pub allowed_capabilities: Vec<Capability>,

    /// Per-capability rate limiting quotas
    ///
    /// Provides granular control over expensive operations.
    /// If a capability is not in this map, the default is no quota (unlimited).
    pub capability_quotas: HashMap<Capability, CapabilityQuota>,
}

impl TrustPolicy {
    /// Check if peer can access a specific topic
    pub fn can_access_topic(&self, topic: &str) -> bool {
        // Empty list means all public topics are allowed
        self.allowed_topics.is_empty() || self.allowed_topics.contains(&topic.to_string())
    }

    /// Check if peer has a specific capability
    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.allowed_capabilities.contains(cap)
    }

    /// Get the rate limiting quota for a specific capability
    ///
    /// Returns `None` if the capability is not allowed, or if no quota is set
    /// (meaning unlimited for that capability).
    pub fn quota_for(&self, cap: &Capability) -> Option<&CapabilityQuota> {
        if !self.has_capability(cap) {
            return None;
        }
        self.capability_quotas.get(cap)
    }

    /// Get the policy for an isolated (untrusted) peer
    ///
    /// Isolated peers have no capabilities and are heavily rate-limited.
    pub fn isolated() -> Self {
        TrustPolicy {
            class: TrustClass::Isolated,
            max_messages_per_second: 10,
            max_streams: 2,
            allowed_topics: vec![], // Only public topics
            allowed_capabilities: vec![],
            capability_quotas: HashMap::new(), // No capabilities = no quotas needed
        }
    }

    /// Get the policy for a known peer
    ///
    /// Known peers can read the ledger with moderate quotas.
    pub fn known() -> Self {
        let mut quotas = HashMap::new();
        // Known: 10 ledger reads/min, 100/hour, burst 5
        quotas.insert(Capability::ReadLedger, CapabilityQuota::new(10, 100, 5));

        TrustPolicy {
            class: TrustClass::Known,
            max_messages_per_second: 50,
            max_streams: 5,
            allowed_topics: vec![],
            allowed_capabilities: vec![Capability::ReadLedger],
            capability_quotas: quotas,
        }
    }

    /// Get the policy for a partner peer
    ///
    /// Partner peers can read/write ledger and execute contracts with higher quotas.
    pub fn partner() -> Self {
        let mut quotas = HashMap::new();
        // Partner: generous read quotas
        quotas.insert(Capability::ReadLedger, CapabilityQuota::new(100, 1000, 20));
        // Partner: 100 ledger writes/min, 500/hour, burst 10
        quotas.insert(Capability::WriteLedger, CapabilityQuota::new(100, 500, 10));
        // Partner: 50 contract executions/min, 200/hour, burst 10
        quotas.insert(
            Capability::ExecuteContract,
            CapabilityQuota::new(50, 200, 10),
        );

        TrustPolicy {
            class: TrustClass::Partner,
            max_messages_per_second: 100,
            max_streams: 10,
            allowed_topics: vec![],
            allowed_capabilities: vec![
                Capability::ReadLedger,
                Capability::WriteLedger,
                Capability::ExecuteContract,
            ],
            capability_quotas: quotas,
        }
    }

    /// Get the policy for a federated peer
    ///
    /// Federated peers have the highest trust and most generous quotas.
    pub fn federated() -> Self {
        let mut quotas = HashMap::new();
        // Federated: very generous read quotas
        quotas.insert(
            Capability::ReadLedger,
            CapabilityQuota::new(500, 5000, 50),
        );
        // Federated: 500 ledger writes/min, 2000/hour, burst 50
        quotas.insert(
            Capability::WriteLedger,
            CapabilityQuota::new(500, 2000, 50),
        );
        // Federated: 50 contract deploys/hour (expensive operation)
        quotas.insert(Capability::DeployContract, CapabilityQuota::new(10, 50, 5));
        // Federated: 200 contract executions/min
        quotas.insert(
            Capability::ExecuteContract,
            CapabilityQuota::new(200, 1000, 20),
        );

        TrustPolicy {
            class: TrustClass::Federated,
            max_messages_per_second: 200,
            max_streams: 16,
            allowed_topics: vec![],
            allowed_capabilities: vec![
                Capability::ReadLedger,
                Capability::WriteLedger,
                Capability::DeployContract,
                Capability::ExecuteContract,
            ],
            capability_quotas: quotas,
        }
    }

    /// Create a policy for a specific trust class
    pub fn for_trust_class(class: TrustClass) -> Self {
        match class {
            TrustClass::Isolated => Self::isolated(),
            TrustClass::Known => Self::known(),
            TrustClass::Partner => Self::partner(),
            TrustClass::Federated => Self::federated(),
        }
    }
}

/// Fine-grained capability permissions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Read ledger entries
    ReadLedger,

    /// Write ledger entries (create transactions)
    WriteLedger,

    /// Deploy new contracts
    DeployContract,

    /// Execute contract rules
    ExecuteContract,

    /// Modify trust graph edges
    ModifyTrust,
}

/// Rate limiting quota for a specific capability
///
/// Provides defense-in-depth by limiting expensive operations
/// beyond the general network-level rate limiting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityQuota {
    /// Maximum operations allowed per minute
    pub max_per_minute: u32,

    /// Maximum operations allowed per hour
    pub max_per_hour: u32,

    /// Burst allowance (operations allowed in a short burst)
    pub burst: u32,
}

impl CapabilityQuota {
    /// Create a new capability quota
    pub fn new(max_per_minute: u32, max_per_hour: u32, burst: u32) -> Self {
        CapabilityQuota {
            max_per_minute,
            max_per_hour,
            burst,
        }
    }

    /// Create a quota that blocks all operations
    pub fn blocked() -> Self {
        CapabilityQuota {
            max_per_minute: 0,
            max_per_hour: 0,
            burst: 0,
        }
    }

    /// Create an unlimited quota (for fully trusted peers)
    pub fn unlimited() -> Self {
        CapabilityQuota {
            max_per_minute: u32::MAX,
            max_per_hour: u32::MAX,
            burst: u32::MAX,
        }
    }
}

/// Default policy source using TrustGraph
pub struct DefaultPolicySource {
    trust_graph: Arc<RwLock<TrustGraph>>,
}

impl DefaultPolicySource {
    /// Create a new default policy source
    pub fn new(trust_graph: Arc<RwLock<TrustGraph>>) -> Self {
        DefaultPolicySource { trust_graph }
    }
}

#[async_trait::async_trait]
impl PolicySource for DefaultPolicySource {
    async fn policy_for(&self, did: &Did) -> TrustPolicy {
        let trust_graph = self.trust_graph.read().await;
        let class = trust_graph.trust_class(did).unwrap_or(TrustClass::Isolated);

        TrustPolicy::for_trust_class(class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_trust::TrustEdge;

    #[test]
    fn test_policy_isolated() {
        let policy = TrustPolicy::isolated();
        assert_eq!(policy.class, TrustClass::Isolated);
        assert_eq!(policy.max_messages_per_second, 10);
        assert_eq!(policy.max_streams, 2);
        assert!(policy.allowed_capabilities.is_empty());
        assert!(!policy.has_capability(&Capability::ReadLedger));
    }

    #[test]
    fn test_policy_known() {
        let policy = TrustPolicy::known();
        assert_eq!(policy.class, TrustClass::Known);
        assert_eq!(policy.max_messages_per_second, 50);
        assert!(policy.has_capability(&Capability::ReadLedger));
        assert!(!policy.has_capability(&Capability::WriteLedger));
    }

    #[test]
    fn test_policy_partner() {
        let policy = TrustPolicy::partner();
        assert_eq!(policy.class, TrustClass::Partner);
        assert_eq!(policy.max_messages_per_second, 100);
        assert!(policy.has_capability(&Capability::ReadLedger));
        assert!(policy.has_capability(&Capability::WriteLedger));
        assert!(policy.has_capability(&Capability::ExecuteContract));
        assert!(!policy.has_capability(&Capability::DeployContract));
    }

    #[test]
    fn test_policy_federated() {
        let policy = TrustPolicy::federated();
        assert_eq!(policy.class, TrustClass::Federated);
        assert_eq!(policy.max_messages_per_second, 200);
        assert_eq!(policy.max_streams, 16);
        assert!(policy.has_capability(&Capability::ReadLedger));
        assert!(policy.has_capability(&Capability::WriteLedger));
        assert!(policy.has_capability(&Capability::DeployContract));
        assert!(policy.has_capability(&Capability::ExecuteContract));
        assert!(!policy.has_capability(&Capability::ModifyTrust));
    }

    #[test]
    fn test_topic_access_empty_list() {
        let policy = TrustPolicy::partner();
        // Empty list means all public topics allowed
        assert!(policy.can_access_topic("any:topic"));
        assert!(policy.can_access_topic("global:identity"));
    }

    #[test]
    fn test_topic_access_restricted() {
        let mut policy = TrustPolicy::partner();
        policy.allowed_topics = vec!["allowed:topic".to_string()];

        assert!(policy.can_access_topic("allowed:topic"));
        assert!(!policy.can_access_topic("forbidden:topic"));
    }

    #[tokio::test]
    async fn test_default_policy_source_isolated() {
        use icn_store::SledStore;

        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn icn_store::Store>;
        let own_did = KeyPair::generate().unwrap().did().clone();
        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(store, own_did)));
        let policy_source = DefaultPolicySource::new(trust_graph.clone());

        // Unknown peer should get isolated policy
        let unknown_did = KeyPair::generate().unwrap().did().clone();
        let policy = policy_source.policy_for(&unknown_did).await;

        assert_eq!(policy.class, TrustClass::Isolated);
        assert_eq!(policy.max_messages_per_second, 10);
    }

    #[tokio::test]
    async fn test_default_policy_source_known() {
        use icn_store::SledStore;

        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn icn_store::Store>;
        let alice = KeyPair::generate().unwrap().did().clone();
        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(store, alice.clone())));
        let policy_source = DefaultPolicySource::new(trust_graph.clone());

        // Add trust edge - single edge gives Known class
        let bob = KeyPair::generate().unwrap().did().clone();

        {
            let mut tg = trust_graph.write().await;
            let _ = tg.add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.5));
        }

        let policy = policy_source.policy_for(&bob).await;
        assert_eq!(policy.class, TrustClass::Known);
        assert_eq!(policy.max_messages_per_second, 50);
        assert!(policy.has_capability(&Capability::ReadLedger));
        assert!(!policy.has_capability(&Capability::WriteLedger));
    }

    #[test]
    fn test_for_trust_class() {
        let isolated = TrustPolicy::for_trust_class(TrustClass::Isolated);
        assert_eq!(isolated.max_messages_per_second, 10);

        let known = TrustPolicy::for_trust_class(TrustClass::Known);
        assert_eq!(known.max_messages_per_second, 50);

        let partner = TrustPolicy::for_trust_class(TrustClass::Partner);
        assert_eq!(partner.max_messages_per_second, 100);

        let federated = TrustPolicy::for_trust_class(TrustClass::Federated);
        assert_eq!(federated.max_messages_per_second, 200);
    }

    #[test]
    fn test_capability_quota_isolated() {
        let policy = TrustPolicy::isolated();
        // Isolated peers have no capabilities, so no quotas
        assert!(policy.quota_for(&Capability::ReadLedger).is_none());
        assert!(policy.quota_for(&Capability::WriteLedger).is_none());
        assert!(policy.capability_quotas.is_empty());
    }

    #[test]
    fn test_capability_quota_known() {
        let policy = TrustPolicy::known();
        // Known peers can read ledger with quotas
        let quota = policy.quota_for(&Capability::ReadLedger);
        assert!(quota.is_some());
        let quota = quota.unwrap();
        assert_eq!(quota.max_per_minute, 10);
        assert_eq!(quota.max_per_hour, 100);
        assert_eq!(quota.burst, 5);

        // No write capability, so no quota
        assert!(policy.quota_for(&Capability::WriteLedger).is_none());
    }

    #[test]
    fn test_capability_quota_partner() {
        let policy = TrustPolicy::partner();

        // Partner can read ledger
        let read_quota = policy.quota_for(&Capability::ReadLedger).unwrap();
        assert_eq!(read_quota.max_per_minute, 100);
        assert_eq!(read_quota.max_per_hour, 1000);

        // Partner can write ledger
        let write_quota = policy.quota_for(&Capability::WriteLedger).unwrap();
        assert_eq!(write_quota.max_per_minute, 100);
        assert_eq!(write_quota.max_per_hour, 500);

        // Partner can execute contracts
        let exec_quota = policy.quota_for(&Capability::ExecuteContract).unwrap();
        assert_eq!(exec_quota.max_per_minute, 50);
        assert_eq!(exec_quota.max_per_hour, 200);

        // Partner cannot deploy contracts
        assert!(policy.quota_for(&Capability::DeployContract).is_none());
    }

    #[test]
    fn test_capability_quota_federated() {
        let policy = TrustPolicy::federated();

        // Federated has all quotas
        let read_quota = policy.quota_for(&Capability::ReadLedger).unwrap();
        assert_eq!(read_quota.max_per_minute, 500);
        assert_eq!(read_quota.max_per_hour, 5000);
        assert_eq!(read_quota.burst, 50);

        let write_quota = policy.quota_for(&Capability::WriteLedger).unwrap();
        assert_eq!(write_quota.max_per_minute, 500);
        assert_eq!(write_quota.max_per_hour, 2000);

        let deploy_quota = policy.quota_for(&Capability::DeployContract).unwrap();
        assert_eq!(deploy_quota.max_per_minute, 10);
        assert_eq!(deploy_quota.max_per_hour, 50);

        let exec_quota = policy.quota_for(&Capability::ExecuteContract).unwrap();
        assert_eq!(exec_quota.max_per_minute, 200);
        assert_eq!(exec_quota.max_per_hour, 1000);

        // Federated cannot modify trust directly
        assert!(policy.quota_for(&Capability::ModifyTrust).is_none());
    }

    #[test]
    fn test_capability_quota_constructors() {
        let quota = CapabilityQuota::new(10, 100, 5);
        assert_eq!(quota.max_per_minute, 10);
        assert_eq!(quota.max_per_hour, 100);
        assert_eq!(quota.burst, 5);

        let blocked = CapabilityQuota::blocked();
        assert_eq!(blocked.max_per_minute, 0);
        assert_eq!(blocked.max_per_hour, 0);
        assert_eq!(blocked.burst, 0);

        let unlimited = CapabilityQuota::unlimited();
        assert_eq!(unlimited.max_per_minute, u32::MAX);
        assert_eq!(unlimited.max_per_hour, u32::MAX);
        assert_eq!(unlimited.burst, u32::MAX);
    }
}
