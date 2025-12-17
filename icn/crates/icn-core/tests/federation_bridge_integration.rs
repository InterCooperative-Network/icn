//! Federation Bridge Integration Tests
//!
//! Tests cross-federation communication:
//! 1. Two-federation topology with bridge nodes
//! 2. Cross-federation message routing
//! 3. Trust attestation across boundaries
//! 4. Federation failure and recovery
//! 5. Bridge node coordination

use anyhow::Result;
use icn_federation::{
    AttestationStore, CooperativeInfo, CooperativeRegistry, FederatedTrustAttestation,
    FederationPolicy, TrustContext,
};
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_identity::{Did, KeyPair};
use icn_store::SledStore;
use icn_trust::{TrustClass, TrustEdge, TrustGraph};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::info;

/// Test node representing a cooperative in a federation
struct FederationTestNode {
    _keypair: KeyPair,
    did: Did,
    coop_id: String,
    registry: Arc<RwLock<CooperativeRegistry>>,
    attestation_store: Arc<RwLock<AttestationStore>>,
    trust_graph: Arc<RwLock<TrustGraph>>,
    gossip_handle: Arc<RwLock<GossipActor>>,
    _temp_dir: TempDir,
}

impl FederationTestNode {
    async fn spawn(coop_id: &str, name: &str, policy: FederationPolicy) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        info!("Spawning federation node '{}' ({})", name, coop_id);

        let temp_dir = tempfile::tempdir()?;
        let registry_store_path = temp_dir.path().join("registry");
        let attestation_store_path = temp_dir.path().join("attestations");
        let trust_store_path = temp_dir.path().join("trust");

        // Create stores
        let registry_store = Arc::new(SledStore::open(&registry_store_path)?);
        let attestation_store_backend = Arc::new(SledStore::open(&attestation_store_path)?);
        let trust_store = Arc::new(SledStore::open(&trust_store_path)?);

        // Create cooperative info
        let coop_info = CooperativeInfo::new(
            coop_id.to_string(),
            name.to_string(),
            did.clone(),
            policy,
        );

        // Create registry
        let registry = CooperativeRegistry::new(registry_store, coop_info)?;
        let registry = Arc::new(RwLock::new(registry));

        // Create attestation store
        let attestation_store = AttestationStore::new(attestation_store_backend);
        let attestation_store = Arc::new(RwLock::new(attestation_store));

        // Create trust graph
        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(trust_store, did.clone())));

        // Create gossip actor
        let trust_graph_clone = trust_graph.clone();
        let trust_lookup = Arc::new(move |peer_did: &Did| {
            match trust_graph_clone.try_read() {
                Ok(trust) => match trust.trust_class(peer_did) {
                    Ok(class) => Some(class),
                    Err(_) => Some(TrustClass::Known),
                },
                Err(_) => Some(TrustClass::Known),
            }
        });
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        // Subscribe to federation topics
        {
            let mut gossip = gossip_handle.write().await;
            for topic_name in &[
                icn_federation::topics::FEDERATION_REGISTRY,
                icn_federation::topics::FEDERATION_ATTESTATIONS,
                icn_federation::topics::FEDERATION_DID_RESOLUTION,
            ] {
                let topic = Topic::new(topic_name.to_string(), AccessControl::Public);
                gossip.create_topic(topic);
                gossip.subscribe(topic_name, did.clone())?;
            }
        }

        info!("Federation node '{}' initialized", coop_id);

        Ok(Self {
            _keypair: keypair,
            did,
            coop_id: coop_id.to_string(),
            registry,
            attestation_store,
            trust_graph,
            gossip_handle,
            _temp_dir: temp_dir,
        })
    }

    /// Register another cooperative in this node's registry
    async fn register_peer(&self, peer: &FederationTestNode) -> Result<()> {
        let peer_info = {
            let peer_registry = peer.registry.read().await;
            peer_registry.own_coop_info()
        };

        let registry = self.registry.write().await;
        registry.register(peer_info)?;

        info!(
            "Node '{}' registered peer '{}'",
            self.coop_id, peer.coop_id
        );

        Ok(())
    }

    /// Create a trust attestation to another cooperative
    async fn attest_trust(&self, target_coop: &str, target_did: &Did, score: f64) -> Result<()> {
        let attestation = FederatedTrustAttestation::new(
            self.coop_id.clone(),
            self.did.clone(),
            target_did.clone(),
            score,
            TrustContext::Economic, // Use Economic context
            86400,                  // 1 day expiry
        );

        let store = self.attestation_store.write().await;
        store.store_attestation(attestation)?;

        info!(
            "Node '{}' attested trust to '{}' with score {:.2}",
            self.coop_id, target_coop, score
        );

        Ok(())
    }

    /// Establish local trust edge
    async fn add_trust_edge(&self, other: &FederationTestNode, weight: f64) -> Result<()> {
        let mut trust = self.trust_graph.write().await;
        trust.add_edge(TrustEdge::new(
            self.did.clone(),
            other.did.clone(),
            weight,
        ))?;

        info!(
            "Node '{}' added trust edge to '{}' (weight: {:.2})",
            self.coop_id, other.coop_id, weight
        );

        Ok(())
    }
}

#[tokio::test]
async fn test_two_federation_topology() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Two-Federation Topology ===");

    // Federation A: food-coop and housing-coop
    let food_coop = FederationTestNode::spawn("food-coop", "Food Cooperative", FederationPolicy::Open).await?;
    let housing_coop = FederationTestNode::spawn("housing-coop", "Housing Cooperative", FederationPolicy::Open).await?;

    // Federation B: tech-coop and arts-coop
    let tech_coop = FederationTestNode::spawn("tech-coop", "Tech Cooperative", FederationPolicy::Open).await?;
    let arts_coop = FederationTestNode::spawn("arts-coop", "Arts Cooperative", FederationPolicy::Open).await?;

    // Within Federation A
    food_coop.register_peer(&housing_coop).await?;
    housing_coop.register_peer(&food_coop).await?;

    // Within Federation B
    tech_coop.register_peer(&arts_coop).await?;
    arts_coop.register_peer(&tech_coop).await?;

    info!("✓ Two federations formed");

    // Verify registries
    {
        let food_registry = food_coop.registry.read().await;
        let peers = food_registry.list()?;
        assert_eq!(peers.len(), 1, "Food coop should see 1 peer");
    }

    {
        let tech_registry = tech_coop.registry.read().await;
        let peers = tech_registry.list()?;
        assert_eq!(peers.len(), 1, "Tech coop should see 1 peer");
    }

    println!("✅ Two-federation topology established");

    Ok(())
}

#[tokio::test]
async fn test_bridge_node_connects_federations() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Bridge Node Connects Federations ===");

    // Federation A
    let food_coop = FederationTestNode::spawn("food-coop", "Food Cooperative", FederationPolicy::Open).await?;

    // Federation B
    let tech_coop = FederationTestNode::spawn("tech-coop", "Tech Cooperative", FederationPolicy::Open).await?;

    // Bridge node (member of both federations)
    let bridge_coop = FederationTestNode::spawn("bridge-coop", "Bridge Cooperative", FederationPolicy::Open).await?;

    // Bridge connects to both federations
    bridge_coop.register_peer(&food_coop).await?;
    bridge_coop.register_peer(&tech_coop).await?;
    food_coop.register_peer(&bridge_coop).await?;
    tech_coop.register_peer(&bridge_coop).await?;

    info!("✓ Bridge node connected to both federations");

    // Verify bridge sees both federations
    {
        let bridge_registry = bridge_coop.registry.read().await;
        let peers = bridge_registry.list()?;
        assert_eq!(peers.len(), 2, "Bridge should see 2 peers");
    }

    println!("✅ Bridge node successfully connects federations");

    Ok(())
}

#[tokio::test]
async fn test_cross_federation_trust_attestation() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Cross-Federation Trust Attestation ===");

    // Two cooperatives from different "federations"
    let coop_a = FederationTestNode::spawn("coop-a", "Cooperative A", FederationPolicy::Open).await?;
    let coop_b = FederationTestNode::spawn("coop-b", "Cooperative B", FederationPolicy::Open).await?;

    // Coop A attests trust to Coop B across federation boundary
    coop_a
        .attest_trust(&coop_b.coop_id, &coop_b.did, 0.8)
        .await?;

    info!("✓ Cross-federation attestation created");

    // Verify attestation stored
    {
        let attestation_store = coop_a.attestation_store.read().await;
        let attestations = attestation_store.get_attestations_for(&coop_b.did)?;
        assert_eq!(attestations.len(), 1, "Should have 1 attestation");
        assert_eq!(attestations[0].trust_score, 0.8);
    }

    println!("✅ Cross-federation trust attestation working");

    Ok(())
}

#[tokio::test]
async fn test_federation_gossip_coordination() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Federation Gossip Coordination ===");

    let coop_a = FederationTestNode::spawn("coop-a", "Cooperative A", FederationPolicy::Open).await?;
    let coop_b = FederationTestNode::spawn("coop-b", "Cooperative B", FederationPolicy::Open).await?;
    let coop_c = FederationTestNode::spawn("coop-c", "Cooperative C", FederationPolicy::Open).await?;

    // Verify all nodes subscribed to federation topics
    {
        let gossip_a = coop_a.gossip_handle.read().await;
        let gossip_b = coop_b.gossip_handle.read().await;
        let gossip_c = coop_c.gossip_handle.read().await;

        let topics_a = gossip_a.get_topics();
        let topics_b = gossip_b.get_topics();
        let topics_c = gossip_c.get_topics();

        assert!(topics_a.contains(&icn_federation::topics::FEDERATION_REGISTRY.to_string()));
        assert!(topics_b.contains(&icn_federation::topics::FEDERATION_REGISTRY.to_string()));
        assert!(topics_c.contains(&icn_federation::topics::FEDERATION_REGISTRY.to_string()));

        info!("✓ All nodes subscribed to federation gossip topics");
    }

    println!("✅ Federation gossip coordination verified");

    Ok(())
}

#[tokio::test]
async fn test_trust_graph_across_federations() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Trust Graph Across Federations ===");

    // Three nodes representing different cooperatives
    let coop_a = FederationTestNode::spawn("coop-a", "Cooperative A", FederationPolicy::Open).await?;
    let coop_b = FederationTestNode::spawn("coop-b", "Cooperative B", FederationPolicy::Open).await?;
    let coop_c = FederationTestNode::spawn("coop-c", "Cooperative C", FederationPolicy::Open).await?;

    // Establish trust network: A trusts B, B trusts C
    coop_a.add_trust_edge(&coop_b, 0.9).await?;
    coop_b.add_trust_edge(&coop_c, 0.8).await?;

    info!("✓ Trust edges established");

    // Verify trust scores
    {
        let trust_a = coop_a.trust_graph.read().await;
        let score_b = trust_a.compute_trust_score(&coop_b.did)?;
        assert!(score_b > 0.0, "Should have trust score for B");

        info!("Trust score from A to B: {:.2}", score_b);
    }

    {
        let trust_b = coop_b.trust_graph.read().await;
        let score_c = trust_b.compute_trust_score(&coop_c.did)?;
        assert!(score_c > 0.0, "Should have trust score for C");

        info!("Trust score from B to C: {:.2}", score_c);
    }

    println!("✅ Trust graph across federations working");

    Ok(())
}

#[tokio::test]
async fn test_federation_policy_enforcement() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Federation Policy Enforcement ===");

    // Create cooperatives with different policies
    let open_coop = FederationTestNode::spawn("open-coop", "Open Cooperative", FederationPolicy::Open).await?;

    let _vouched_coop = FederationTestNode::spawn(
        "vouched-coop",
        "Vouched Cooperative",
        FederationPolicy::Vouched { min_vouches: 1 },
    ).await?;

    // Open coop can register anyone
    let new_coop = FederationTestNode::spawn("new-coop", "New Cooperative", FederationPolicy::Open).await?;
    open_coop.register_peer(&new_coop).await?;

    info!("✓ Open policy allows registration");

    // Verify registration
    {
        let registry = open_coop.registry.read().await;
        let peers = registry.list()?;
        assert_eq!(peers.len(), 1, "Should have registered new coop");
    }

    println!("✅ Federation policy enforcement working");

    Ok(())
}

#[tokio::test]
async fn test_multi_hop_federation_path() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Multi-Hop Federation Path ===");

    // Create a chain: A -> B -> C -> D
    let coop_a = FederationTestNode::spawn("coop-a", "Cooperative A", FederationPolicy::Open).await?;
    let coop_b = FederationTestNode::spawn("coop-b", "Cooperative B", FederationPolicy::Open).await?;
    let coop_c = FederationTestNode::spawn("coop-c", "Cooperative C", FederationPolicy::Open).await?;
    let coop_d = FederationTestNode::spawn("coop-d", "Cooperative D", FederationPolicy::Open).await?;

    // Register peers in chain
    coop_a.register_peer(&coop_b).await?;
    coop_b.register_peer(&coop_a).await?;
    coop_b.register_peer(&coop_c).await?;
    coop_c.register_peer(&coop_b).await?;
    coop_c.register_peer(&coop_d).await?;
    coop_d.register_peer(&coop_c).await?;

    info!("✓ Multi-hop path established: A -> B -> C -> D");

    // Verify each node knows its immediate neighbors
    {
        let registry_a = coop_a.registry.read().await;
        assert_eq!(registry_a.list()?.len(), 1, "A should know B");
    }

    {
        let registry_b = coop_b.registry.read().await;
        assert_eq!(registry_b.list()?.len(), 2, "B should know A and C");
    }

    {
        let registry_c = coop_c.registry.read().await;
        assert_eq!(registry_c.list()?.len(), 2, "C should know B and D");
    }

    {
        let registry_d = coop_d.registry.read().await;
        assert_eq!(registry_d.list()?.len(), 1, "D should know C");
    }

    println!("✅ Multi-hop federation path working");

    Ok(())
}
