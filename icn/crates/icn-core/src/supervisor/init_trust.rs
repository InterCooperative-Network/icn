//! Trust graph and security initialization
//!
//! Initializes the trust graph, recovery store, and Byzantine fault detection.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use icn_identity::Did;
use icn_kernel_api::services::TrustService;
use icn_security::{MisbehaviorDetector, MisbehaviorThresholds};
use icn_store::SledStore;
use icn_trust::TrustGraph;

use crate::config::Config;

/// Services initialized during trust setup
///
/// # Kernel/App Separation
///
/// This struct exposes `trust_graph` for app-layer operations (mutations like
/// adding/removing trust edges). Kernel components should NOT use `trust_graph`
/// directly - they should use `TrustService` from the `ServiceRegistry` instead.
///
/// The `trust_graph` field is exposed here because:
/// - `IdentityActor` needs it for trust mutations (add_edge, remove_edge)
/// - RPC handlers need it for trust API endpoints
/// - `MisbehaviorDetector` callbacks need it for trust penalties
///
/// For read-only trust queries in kernel components, use:
/// ```ignore
/// let trust_service = service_registry.trust();
/// let score = trust_service.trust_score(&did);
/// ```
pub struct TrustServices {
    /// The trust graph for app-layer mutations (add/remove edges)
    ///
    /// **Note**: Kernel data-path components should use `TrustService` from
    /// `ServiceRegistry` instead of accessing this directly.
    pub trust_graph: Arc<RwLock<TrustGraph>>,
    /// Byzantine fault detector for misbehavior tracking
    pub misbehavior_detector: Arc<RwLock<MisbehaviorDetector>>,
    /// Store for social recovery events
    pub recovery_store: Arc<dyn icn_store::Store>,
    /// Store for security/reputation state (misbehavior detector persistence)
    pub security_store: Arc<dyn icn_store::Store>,
}

/// Initialize trust graph and security services
///
/// Creates:
/// - Trust graph with persistent storage
/// - Recovery store for social recovery
/// - Misbehavior detector with TrustService integration
pub async fn init_trust_services(
    config: &Config,
    did: Did,
    trust_service: Option<Arc<dyn TrustService>>,
) -> anyhow::Result<TrustServices> {
    // Create trust graph
    // Note: Phase 21 adds TrustGraphFacade for multi-graph support (Social, Economic, Technical).
    // Currently using TrustGraph directly. Migration to TrustGraphFacade requires updating
    // consumer type signatures. See docs/trust-multi-graph-migration.md for migration guide.
    let trust_store_path = config.store_path().join("trust");
    let trust_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&trust_store_path)?);
    let trust_graph = TrustGraph::new(trust_store, did.clone());
    let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

    info!("Trust graph initialized at {}", trust_store_path.display());

    init_trust_services_impl(config, trust_graph_handle, trust_service).await
}

/// Initialize trust services with an externally-provided TrustGraph
///
/// This variant is used when the daemon provides a TrustGraph via ServiceRegistry,
/// enabling proper kernel/app separation where the daemon owns the TrustGraph.
///
/// Creates:
/// - Recovery store for social recovery
/// - Misbehavior detector with TrustService integration
/// - Wires the provided TrustGraph for penalty callbacks
pub async fn init_trust_services_with_graph(
    config: &Config,
    trust_graph_handle: Arc<RwLock<TrustGraph>>,
    trust_service: Option<Arc<dyn TrustService>>,
) -> anyhow::Result<TrustServices> {
    info!("Using daemon-provided TrustGraph for trust services");
    init_trust_services_impl(config, trust_graph_handle, trust_service).await
}

/// Internal implementation for trust service initialization
async fn init_trust_services_impl(
    config: &Config,
    trust_graph_handle: Arc<RwLock<TrustGraph>>,
    trust_service: Option<Arc<dyn TrustService>>,
) -> anyhow::Result<TrustServices> {
    // Create recovery store for social recovery events
    let recovery_store_path = config.store_path().join("recovery");
    let recovery_store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&recovery_store_path)?);
    info!(
        "Recovery store initialized at {}",
        recovery_store_path.display()
    );

    // Create security store for misbehavior detector persistence
    let security_store_path = config.store_path().join("security");
    let security_store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&security_store_path)?);
    info!(
        "Security store initialized at {}",
        security_store_path.display()
    );

    // Create shared MisbehaviorDetector for Byzantine fault detection (Phase 18)
    // This is shared between NetworkActor and GossipActor to ensure unified tracking
    // Load persisted reputation/ban state from previous session
    let mut detector =
        MisbehaviorDetector::with_store(MisbehaviorThresholds::default(), security_store.as_ref())
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to load persisted misbehavior state, starting fresh: {}",
                    e
                );
                MisbehaviorDetector::new(MisbehaviorThresholds::default())
            });

    // Wire TrustService for kernel/app separation (replaces legacy callback)
    if let Some(ts) = trust_service {
        detector.set_trust_service(ts);
        info!("MisbehaviorDetector using TrustService (kernel/app separation)");
    } else {
        debug!("No TrustService available; MisbehaviorDetector will not report to trust layer");
    }

    let misbehavior_detector = Arc::new(RwLock::new(detector));
    info!("Shared Byzantine fault detector created");

    Ok(TrustServices {
        trust_graph: trust_graph_handle,
        misbehavior_detector,
        recovery_store,
        security_store,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::authz::PolicyOracle;
    use icn_kernel_api::services::TrustEvent;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    /// Mock TrustService that records events for verification
    struct MockTrustService {
        event_count: AtomicUsize,
    }

    impl MockTrustService {
        fn new() -> Self {
            Self {
                event_count: AtomicUsize::new(0),
            }
        }

        fn event_count(&self) -> usize {
            self.event_count.load(Ordering::Relaxed)
        }
    }

    impl TrustService for MockTrustService {
        fn oracle(&self) -> Arc<dyn PolicyOracle> {
            unimplemented!("not needed for these tests")
        }

        fn trust_score(&self, _actor: &String) -> f64 {
            0.5
        }

        fn record_event(&self, _actor: &String, _event: TrustEvent) {
            self.event_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[tokio::test]
    async fn test_trust_services_init() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let services = init_trust_services(&config, did, None).await.unwrap();

        // Verify services were initialized (just check we can acquire locks)
        let _graph = services.trust_graph.read().await;
        let _detector = services.misbehavior_detector.read().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_misbehavior_reports_to_trust_service() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let own_did = keypair.did().clone();

        let mock_ts = Arc::new(MockTrustService::new());
        let services = init_trust_services(&config, own_did, Some(mock_ts.clone()))
            .await
            .unwrap();

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

        // TrustService should have received one event
        assert_eq!(
            mock_ts.event_count(),
            1,
            "TrustService should have received exactly one event"
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

        let mock_ts = Arc::new(MockTrustService::new());
        let services = init_trust_services(&config, own_did, Some(mock_ts.clone()))
            .await
            .unwrap();

        let peer_keypair = icn_identity::KeyPair::generate().unwrap();
        let peer_did = peer_keypair.did().clone();

        // Record multiple violations to trigger quarantine/ban
        {
            let mut detector = services.misbehavior_detector.write().await;
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

        // All 10 violations should have been reported to TrustService
        assert_eq!(
            mock_ts.event_count(),
            10,
            "TrustService should have received all violation events"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_release_sends_positive_event() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let own_did = keypair.did().clone();

        let mock_ts = Arc::new(MockTrustService::new());
        let services = init_trust_services(&config, own_did, Some(mock_ts.clone()))
            .await
            .unwrap();

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

        // 3 violation events
        assert_eq!(mock_ts.event_count(), 3);

        // Verify quarantined
        {
            let detector = services.misbehavior_detector.read().await;
            assert!(
                detector.is_quarantined(&peer_did),
                "Peer should be quarantined after 3 InvalidSignature violations"
            );
        }

        // Force release from quarantine
        {
            let mut detector = services.misbehavior_detector.write().await;
            detector.force_release_from_quarantine(&peer_did);
        }

        // Should have one more event (PositiveInteraction from release)
        assert_eq!(
            mock_ts.event_count(),
            4,
            "Release should send a PositiveInteraction event"
        );

        // Verify no longer quarantined
        let detector = services.misbehavior_detector.read().await;
        assert!(
            !detector.is_quarantined(&peer_did),
            "Peer should no longer be quarantined after force release"
        );
    }
}
