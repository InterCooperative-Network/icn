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

    /// Get the policy for an isolated (untrusted) peer
    pub fn isolated() -> Self {
        TrustPolicy {
            class: TrustClass::Isolated,
            max_messages_per_second: 10,
            max_streams: 2,
            allowed_topics: vec![], // Only public topics
            allowed_capabilities: vec![],
        }
    }

    /// Get the policy for a known peer
    pub fn known() -> Self {
        TrustPolicy {
            class: TrustClass::Known,
            max_messages_per_second: 50,
            max_streams: 5,
            allowed_topics: vec![],
            allowed_capabilities: vec![Capability::ReadLedger],
        }
    }

    /// Get the policy for a partner peer
    pub fn partner() -> Self {
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
        }
    }

    /// Get the policy for a federated peer
    pub fn federated() -> Self {
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
}
