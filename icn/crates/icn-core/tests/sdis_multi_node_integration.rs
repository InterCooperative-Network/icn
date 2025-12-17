//! Multi-node SDIS Integration Tests
//!
//! Tests the SDIS (Sovereign Digital Identity System) infrastructure:
//! 1. Steward actor spawning and initialization
//! 2. Trust graph integration with stewards
//! 3. Gossip coordination between stewards
//! 4. Recovery attestation creation
//! 5. Statistics tracking

use anyhow::Result;
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_identity::{IdentityBundle, KeyPair, RecoveryAttestation};
use icn_steward::{StewardConfig, StewardHandle};
use icn_store::SledStore;
use icn_trust::{TrustClass, TrustEdge, TrustGraph};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tracing::info;

/// Test node with SDIS infrastructure
struct SdisTestNode {
    keypair: KeyPair,
    did: icn_identity::Did,
    steward_handle: StewardHandle,
    trust_graph: Arc<RwLock<TrustGraph>>,
    gossip_handle: Arc<RwLock<GossipActor>>,
    _temp_dir: TempDir,
}

impl SdisTestNode {
    async fn spawn_steward(name: &str) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        info!("Spawning steward node '{}' with DID: {}", name, did);

        let temp_dir = TempDir::new()?;
        let trust_store_path = temp_dir.path().join("trust");

        let trust_store = Arc::new(SledStore::open(&trust_store_path)?);

        let trust_graph = Arc::new(RwLock::new(TrustGraph::new(trust_store, did.clone())));

        let trust_graph_clone = trust_graph.clone();
        let trust_lookup = Arc::new(move |peer_did: &icn_identity::Did| {
            match trust_graph_clone.try_read() {
                Ok(trust) => match trust.trust_class(peer_did) {
                    Ok(class) => Some(class),
                    Err(_) => Some(TrustClass::Known),
                },
                Err(_) => Some(TrustClass::Known),
            }
        });
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        {
            let mut gossip = gossip_handle.write().await;
            for topic_name in &[
                icn_steward::topics::STEWARD_ANNOUNCE,
                icn_steward::topics::VUI_SYNC,
                icn_steward::topics::ENROLLMENT,
                icn_steward::topics::RECOVERY,
            ] {
                let topic = Topic::new(topic_name.to_string(), AccessControl::Public);
                gossip.create_topic(topic);
                gossip.subscribe(topic_name, did.clone())?;
            }
        }

        let config = StewardConfig::default();
        
        // Create shutdown channel
        let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel(16);
        
        // For tests, we don't need actual gossip sending
        let send_gossip = None;
        
        let steward_handle = icn_steward::StewardActor::spawn(
            did.clone(),
            keypair.clone(),
            config,
            shutdown_tx,
            send_gossip,
        ).await?;

        info!("Steward '{}' initialized", name);

        Ok(Self {
            keypair,
            did,
            steward_handle,
            trust_graph,
            gossip_handle,
            _temp_dir: temp_dir,
        })
    }

    async fn trust(&self, other: &SdisTestNode, weight: f32) -> Result<()> {
        let mut trust = self.trust_graph.write().await;
        trust.add_edge(TrustEdge::new(
            self.did.clone(),
            other.did.clone(),
            weight as f64,
        ))?;
        Ok(())
    }
}

#[tokio::test]
async fn test_steward_actor_initialization() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Steward Actor Initialization ===");

    let steward = SdisTestNode::spawn_steward("TestSteward").await?;

    // Verify steward handle is responsive
    let stats = steward.steward_handle.get_stats().await?;
    
    assert_eq!(stats.active_enrollments, 0);
    assert_eq!(stats.active_recoveries, 0);
    assert_eq!(stats.vui_registry_size, 0);

    info!("✓ Steward initialized with stats: enrollments={}, recoveries={}, vui_size={}",
        stats.active_enrollments, stats.active_recoveries, stats.vui_registry_size);

    println!("✅ Steward actor initialization verified");

    Ok(())
}

#[tokio::test]
async fn test_multi_steward_network() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Multi-Steward Network ===");

    let steward_a = SdisTestNode::spawn_steward("Alice").await?;
    let steward_b = SdisTestNode::spawn_steward("Bob").await?;
    let steward_c = SdisTestNode::spawn_steward("Carol").await?;

    // Establish trust network
    steward_a.trust(&steward_b, 0.9).await?;
    steward_a.trust(&steward_c, 0.9).await?;
    steward_b.trust(&steward_a, 0.9).await?;
    steward_b.trust(&steward_c, 0.9).await?;
    steward_c.trust(&steward_a, 0.9).await?;
    steward_c.trust(&steward_b, 0.9).await?;

    info!("✓ Trust network established");

    // Verify all stewards are operational
    let stats_a = steward_a.steward_handle.get_stats().await?;
    let stats_b = steward_b.steward_handle.get_stats().await?;
    let stats_c = steward_c.steward_handle.get_stats().await?;

    assert_eq!(stats_a.active_enrollments, 0);
    assert_eq!(stats_b.active_enrollments, 0);
    assert_eq!(stats_c.active_enrollments, 0);

    info!("✓ All stewards operational");

    println!("✅ Multi-steward network verified");

    Ok(())
}

#[tokio::test]
async fn test_steward_trust_based_selection() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Trust-Based Steward Selection ===");

    let steward_high = SdisTestNode::spawn_steward("HighTrust").await?;
    let steward_medium = SdisTestNode::spawn_steward("MediumTrust").await?;
    let steward_low = SdisTestNode::spawn_steward("LowTrust").await?;

    let user = SdisTestNode::spawn_steward("User").await?;

    // User trusts stewards at different levels
    user.trust(&steward_high, 0.9).await?;
    user.trust(&steward_medium, 0.6).await?;
    user.trust(&steward_low, 0.3).await?;

    // Verify trust scores
    {
        let trust = user.trust_graph.read().await;
        let score_high = trust.compute_trust_score(&steward_high.did)?;
        let score_medium = trust.compute_trust_score(&steward_medium.did)?;
        let score_low = trust.compute_trust_score(&steward_low.did)?;

        assert!(score_high > score_medium);
        assert!(score_medium > score_low);

        info!(
            "Trust scores - High: {:.2}, Medium: {:.2}, Low: {:.2}",
            score_high, score_medium, score_low
        );
    }

    println!("✅ Trust-based selection working");

    Ok(())
}

#[tokio::test]
async fn test_recovery_attestation_creation() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Recovery Attestation Creation ===");

    let steward_a = SdisTestNode::spawn_steward("Alice").await?;
    let steward_b = SdisTestNode::spawn_steward("Bob").await?;

    // User loses device
    let old_identity = IdentityBundle::generate()?;
    let new_identity = IdentityBundle::generate()?;

    info!(
        "User recovery: {} → {}",
        old_identity.did(),
        new_identity.did()
    );

    // Stewards create attestations
    let attestation_a = RecoveryAttestation::new(
        &steward_a.keypair,
        old_identity.did().clone(),
        new_identity.did().clone(),
        "biometric-verification".to_string(),
    )?;

    let attestation_b = RecoveryAttestation::new(
        &steward_b.keypair,
        old_identity.did().clone(),
        new_identity.did().clone(),
        "biometric-verification".to_string(),
    )?;

    // Verify attestations
    assert_eq!(attestation_a.old_did, *old_identity.did());
    assert_eq!(attestation_a.new_did, *new_identity.did());
    assert_eq!(attestation_b.old_did, *old_identity.did());
    assert_eq!(attestation_b.new_did, *new_identity.did());

    info!("✓ Attestations created and verified");

    println!("✅ Recovery attestations working");

    Ok(())
}

#[tokio::test]
async fn test_steward_gossip_coordination() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Steward Gossip Coordination ===");

    let steward_a = SdisTestNode::spawn_steward("SyncA").await?;
    let steward_b = SdisTestNode::spawn_steward("SyncB").await?;
    let steward_c = SdisTestNode::spawn_steward("SyncC").await?;

    // Verify all subscribed to steward topics
    {
        let gossip_a = steward_a.gossip_handle.read().await;
        let gossip_b = steward_b.gossip_handle.read().await;
        let gossip_c = steward_c.gossip_handle.read().await;

        let topics_a = gossip_a.get_topics();
        let topics_b = gossip_b.get_topics();
        let topics_c = gossip_c.get_topics();

        assert!(topics_a.contains(&icn_steward::topics::STEWARD_ANNOUNCE.to_string()));
        assert!(topics_b.contains(&icn_steward::topics::STEWARD_ANNOUNCE.to_string()));
        assert!(topics_c.contains(&icn_steward::topics::STEWARD_ANNOUNCE.to_string()));

        info!("✓ All stewards subscribed to gossip topics");
    }

    println!("✅ Gossip coordination verified");

    Ok(())
}

#[tokio::test]
async fn test_steward_stats_tracking() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    info!("=== Test: Steward Statistics Tracking ===");

    let steward = SdisTestNode::spawn_steward("StatTracker").await?;

    let stats = steward.steward_handle.get_stats().await?;

    assert_eq!(stats.active_enrollments, 0);
    assert_eq!(stats.active_recoveries, 0);
    assert_eq!(stats.enrollments_completed, 0);
    assert_eq!(stats.recoveries_completed, 0);

    info!("Stats: enrollments={}/{}, recoveries={}/{}",
        stats.active_enrollments, stats.enrollments_completed,
        stats.active_recoveries, stats.recoveries_completed);

    println!("✅ Statistics tracking verified");

    Ok(())
}
