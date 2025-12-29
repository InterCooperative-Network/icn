//! Trust graph and security initialization
//!
//! Initializes the trust graph, recovery store, and Byzantine fault detection.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use icn_identity::Did;
use icn_security::{MisbehaviorDetector, MisbehaviorThresholds, TrustPenaltyCallback};
use icn_store::SledStore;
use icn_trust::{TrustClass, TrustEdge, TrustGraph};

use crate::config::Config;

/// Services initialized during trust setup
pub struct TrustServices {
    /// The trust graph for tracking peer relationships
    pub trust_graph: Arc<RwLock<TrustGraph>>,
    /// Byzantine fault detector for misbehavior tracking
    pub misbehavior_detector: Arc<RwLock<MisbehaviorDetector>>,
    /// Store for social recovery events
    pub recovery_store: Arc<dyn icn_store::Store>,
}

/// Trust lookup closure type for gossip actor
pub type TrustLookup = Arc<dyn Fn(&Did) -> Option<TrustClass> + Send + Sync>;

/// Initialize trust graph and security services
///
/// Creates:
/// - Trust graph with persistent storage
/// - Recovery store for social recovery
/// - Misbehavior detector with trust penalty callback
pub async fn init_trust_services(config: &Config, did: Did) -> anyhow::Result<TrustServices> {
    // Create trust graph
    // Note: Phase 21 adds TrustGraphFacade for multi-graph support (Social, Economic, Technical).
    // Currently using TrustGraph directly. Migration to TrustGraphFacade requires updating
    // consumer type signatures. See docs/trust-multi-graph-migration.md for migration guide.
    let trust_store_path = config.store_path().join("trust");
    let trust_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&trust_store_path)?);
    let trust_graph = TrustGraph::new(trust_store, did.clone());
    let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

    info!("Trust graph initialized at {}", trust_store_path.display());

    // Create recovery store for social recovery events
    let recovery_store_path = config.store_path().join("recovery");
    let recovery_store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&recovery_store_path)?);
    info!(
        "Recovery store initialized at {}",
        recovery_store_path.display()
    );

    // Create shared MisbehaviorDetector for Byzantine fault detection (Phase 18)
    // This is shared between NetworkActor and GossipActor to ensure unified tracking
    let mut detector = MisbehaviorDetector::new(MisbehaviorThresholds::default());

    // Set up trust penalty callback to update trust graph (Phase 18)
    let trust_penalty_callback = create_trust_penalty_callback(trust_graph_handle.clone(), did);
    detector.set_trust_penalty_callback(trust_penalty_callback);

    let misbehavior_detector = Arc::new(RwLock::new(detector));
    info!("Shared Byzantine fault detector created with trust graph integration");

    Ok(TrustServices {
        trust_graph: trust_graph_handle,
        misbehavior_detector,
        recovery_store,
    })
}

/// Create trust lookup closure for gossip actor
///
/// Returns a closure that can synchronously look up a peer's trust class.
pub fn create_trust_lookup(trust_graph: Arc<RwLock<TrustGraph>>) -> TrustLookup {
    Arc::new(move |peer_did: &Did| {
        // Use a blocking task since we're in a sync context
        let graph = trust_graph.clone();
        let peer = peer_did.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let graph = graph.read().await;
                graph.trust_class(&peer).ok()
            })
        })
    })
}

/// Create trust penalty callback for misbehavior detector
///
/// NOTE: This callback is synchronous (uses block_in_place) to prevent race conditions
/// with gossip-received trust updates. The caller waits for the trust update to complete.
fn create_trust_penalty_callback(
    trust_graph: Arc<RwLock<TrustGraph>>,
    own_did: Did,
) -> TrustPenaltyCallback {
    Arc::new(move |peer_did: &Did, reputation_score: f64| {
        let graph = trust_graph.clone();
        let peer = peer_did.clone();
        let own = own_did.clone();

        // Use block_in_place to synchronously update trust graph
        // This prevents races with gossip-received trust updates
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                // Map reputation (0.0-1.0) to trust score (0.0-1.0)
                // Reputation below 0.5 becomes untrusted (<0.1)
                let trust_score = if reputation_score < 0.5 {
                    reputation_score * 0.2 // 0.5 → 0.1, 0.0 → 0.0
                } else {
                    reputation_score // Keep 0.5-1.0 range
                };

                let mut graph = graph.write().await;
                let edge = TrustEdge::new(own.clone(), peer.clone(), trust_score);

                if let Err(e) = graph.add_edge(edge) {
                    warn!(
                        "Failed to update trust graph for {} (reputation: {:.2}): {}",
                        peer, reputation_score, e
                    );
                } else {
                    debug!(
                        "Updated trust for {} to {:.2} (reputation: {:.2})",
                        peer, trust_score, reputation_score
                    );
                }
            })
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_trust_services_init() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let services = init_trust_services(&config, did).await.unwrap();

        // Verify services were initialized (just check we can acquire locks)
        let _graph = services.trust_graph.read().await;
        let _detector = services.misbehavior_detector.read().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_lookup_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let services = init_trust_services(&config, did.clone()).await.unwrap();
        let lookup = create_trust_lookup(services.trust_graph.clone());

        // Unknown peer should return None or Isolated
        let unknown_did = icn_identity::KeyPair::generate().unwrap().did().clone();
        let result = lookup(&unknown_did);
        // Either None or Isolated is acceptable for unknown peers
        assert!(result.is_none() || result == Some(TrustClass::Isolated));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_misbehavior_updates_trust_graph() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let own_did = keypair.did().clone();

        let services = init_trust_services(&config, own_did.clone()).await.unwrap();

        // Create a peer DID
        let peer_keypair = icn_identity::KeyPair::generate().unwrap();
        let peer_did = peer_keypair.did().clone();

        // Record a violation against the peer
        {
            let mut detector = services.misbehavior_detector.write().await;
            let violation = icn_security::Violation::InvalidSignature {
                message_hash: [0u8; 32],
            };
            detector.record_violation(&peer_did, violation, vec![]);
        }

        // Give the callback time to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify the trust graph was updated
        let graph = services.trust_graph.read().await;
        let trust_score = graph.compute_trust_score(&peer_did).unwrap_or(0.0);

        // Score should be reduced from the violation (0.75 after one InvalidSignature)
        // The callback maps this to trust score, so it should be 0.75
        assert!(
            (0.5..=1.0).contains(&trust_score),
            "Trust score should be moderate after violation: got {trust_score}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_severe_misbehavior_causes_quarantine() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let own_did = keypair.did().clone();

        let services = init_trust_services(&config, own_did).await.unwrap();

        let peer_keypair = icn_identity::KeyPair::generate().unwrap();
        let peer_did = peer_keypair.did().clone();

        // Record multiple violations to trigger quarantine/ban
        {
            let mut detector = services.misbehavior_detector.write().await;

            // Record enough violations to drop reputation below quarantine threshold
            // InvalidSignature has severity 5, penalty = 0.25 per violation
            // After 3 violations: 1.0 - (3 × 0.25) = 0.25 (quarantined)
            // After 4+ violations: 1.0 - (4 × 0.25) = 0.0 (banned, as score <= ban_threshold of 0.0)
            for _ in 0..10 {
                let violation = icn_security::Violation::InvalidSignature {
                    message_hash: [0u8; 32],
                };
                detector.record_violation(&peer_did, violation, vec![]);
            }
        }

        // Verify peer is quarantined OR banned (severe misbehavior escalates to ban)
        let detector = services.misbehavior_detector.read().await;
        assert!(
            detector.is_quarantined(&peer_did) || detector.is_banned(&peer_did),
            "Peer should be quarantined or banned after many violations"
        );

        // Trust should be very low
        let graph = services.trust_graph.read().await;
        let trust_score = graph.compute_trust_score(&peer_did).unwrap_or(0.0);
        assert!(
            trust_score < 0.3,
            "Trust score should be very low when quarantined/banned: got {trust_score}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_release_restores_trust() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let own_did = keypair.did().clone();

        let services = init_trust_services(&config, own_did).await.unwrap();

        let peer_keypair = icn_identity::KeyPair::generate().unwrap();
        let peer_did = peer_keypair.did().clone();

        // Quarantine the peer using InvalidSignature (severity 5, penalty 0.25)
        // After 3 violations: 1.0 - (3 * 0.25) = 0.25 (quarantined but not banned)
        {
            let mut detector = services.misbehavior_detector.write().await;
            for _ in 0..3 {
                let violation = icn_security::Violation::InvalidSignature {
                    message_hash: [0u8; 32],
                };
                detector.record_violation(&peer_did, violation, vec![]);
            }
        }

        // Verify quarantined (not banned yet)
        {
            let detector = services.misbehavior_detector.read().await;
            assert!(
                detector.is_quarantined(&peer_did),
                "Peer should be quarantined after 3 InvalidSignature violations"
            );
            assert!(
                !detector.is_banned(&peer_did),
                "Peer should NOT be banned yet"
            );
        }

        // Get trust score before release
        let trust_before = {
            let graph = services.trust_graph.read().await;
            graph.compute_trust_score(&peer_did).unwrap_or(0.0)
        };

        // Force release from quarantine
        {
            let mut detector = services.misbehavior_detector.write().await;
            detector.force_release_from_quarantine(&peer_did);
        }

        // Give callback time to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify no longer quarantined
        {
            let detector = services.misbehavior_detector.read().await;
            assert!(
                !detector.is_quarantined(&peer_did),
                "Peer should no longer be quarantined after force release"
            );
        }

        // Trust should improve after release
        let graph = services.trust_graph.read().await;
        let trust_after = graph.compute_trust_score(&peer_did).unwrap_or(0.0);
        assert!(
            trust_after > trust_before,
            "Trust should improve after quarantine release: before={trust_before}, after={trust_after}"
        );
        // Trust should be at least moderate after reset to 0.6 reputation
        assert!(
            trust_after >= 0.3,
            "Trust should be at least moderate after release: got {trust_after}"
        );
    }
}
