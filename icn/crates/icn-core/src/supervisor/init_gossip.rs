//! Gossip actor initialization and configuration
//!
//! Initializes the gossip protocol layer including:
//! - GossipActor with trust-gated subscriptions
//! - Snapshot restoration
//! - Partition detection and healing
//! - Storage quota management
//! - Replication manager

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use icn_gossip::{GossipActor, PartitionConfig, PartitionDetector, PartitionHealer, VectorClock};
use icn_identity::{Did, IdentityBundle};
use icn_security::MisbehaviorDetector;
use icn_snapshot::StateSnapshot;
use icn_store::SledStore;
use icn_trust::TrustGraph;

use super::init_trust::TrustLookup;
use crate::config::Config;
use crate::replication::{ReplicationConfig, ReplicationManager};

/// Services initialized during gossip setup
pub struct GossipServices {
    /// The gossip actor handle
    pub gossip_handle: Arc<RwLock<GossipActor>>,
    /// Gossip persistent store
    pub gossip_store: Arc<dyn icn_store::Store>,
    /// Partition detector for split-brain prevention
    pub partition_detector: Arc<RwLock<PartitionDetector>>,
    /// Partition healer for recovery
    pub partition_healer: Arc<RwLock<PartitionHealer>>,
    /// Storage quota manager
    pub storage_quota: Arc<RwLock<icn_store::StorageQuotaManager>>,
    /// Loaded snapshot (if any)
    pub loaded_snapshot: Option<StateSnapshot>,
}

/// Dependencies for gossip initialization
pub struct GossipDeps {
    pub trust_graph: Arc<RwLock<TrustGraph>>,
    pub trust_lookup: TrustLookup,
    pub misbehavior_detector: Arc<RwLock<MisbehaviorDetector>>,
    pub identity_bundle: IdentityBundle,
}

/// Initialize gossip services
///
/// Creates:
/// - GossipActor with trust-gated subscriptions
/// - Restores state from snapshot if available
/// - Sets up partition detection and healing
/// - Initializes storage quota management
/// - Spawns replication manager
pub async fn init_gossip_services(
    config: &Config,
    did: Did,
    deps: GossipDeps,
) -> anyhow::Result<GossipServices> {
    // Spawn Gossip actor with trust graph for fine-grained trust-based subscription control
    let gossip_handle = GossipActor::spawn_with_trust_graph(
        did.clone(),
        deps.trust_lookup,
        Some(deps.trust_graph.clone()),
    );

    info!("Gossip actor spawned with trust-gated subscription support");

    // Set keypair for signing outgoing gossip messages
    {
        let keypair = deps.identity_bundle.keypair()?;
        let mut gossip = gossip_handle.write().await;
        gossip.set_keypair(keypair);
    }

    info!("Gossip actor configured with signing keypair");

    // Restore gossip state from snapshot if available
    let loaded_snapshot = restore_gossip_snapshot(config, &gossip_handle).await;

    // Set up gossip store for replica metadata tracking (Phase 17)
    let gossip_store_path = config.store_path().join("gossip");
    let gossip_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&gossip_store_path)?);
    {
        let mut gossip = gossip_handle.write().await;
        gossip.set_store(gossip_store.clone());
    }
    info!(
        "Gossip store initialized at {} for replica tracking",
        gossip_store_path.display()
    );

    // Spawn ReplicationManager for monitoring and maintaining data durability (Phase 17 Week 3)
    let replication_config = ReplicationConfig::default();
    let _replication_handle = ReplicationManager::spawn(
        did.clone(),
        replication_config.clone(),
        gossip_store.clone(),
        deps.trust_graph.clone(),
        gossip_handle.clone(),
    );

    info!(
        "ReplicationManager spawned (target: {} replicas, health check: {}s)",
        replication_config.target_replicas, replication_config.health_check_interval_secs
    );

    // Phase 18 Week 3: Initialize PartitionDetector and PartitionHealer
    let partition_config = PartitionConfig::default(); // 5 min threshold, 30s check interval
    let partition_detector = PartitionDetector::new(partition_config.clone());
    let partition_detector_handle = Arc::new(RwLock::new(partition_detector));

    // Create partition healer with new vector clock (will be merged during healing)
    let partition_healer = PartitionHealer::new(VectorClock::new());
    let partition_healer_handle = Arc::new(RwLock::new(partition_healer));

    // Set partition detector and healer on gossip actor
    {
        let mut gossip = gossip_handle.write().await;
        gossip.set_partition_detector(partition_detector_handle.clone());
        gossip.set_partition_healer(partition_healer_handle.clone());
    }

    info!(
        "Partition detector and healer initialized (threshold: {:?})",
        partition_config.partition_threshold
    );

    // Set shared misbehavior detector on gossip actor for unified Byzantine fault tracking
    {
        let mut gossip = gossip_handle.write().await;
        gossip.set_misbehavior_detector(deps.misbehavior_detector.clone());
    }
    info!("Gossip actor configured with shared Byzantine fault detector");

    // Phase 18 Week 6: Initialize StorageQuotaManager for gossip entries
    // Default: 1GB global limit, 90% eviction threshold, 10MB per-DID quota
    let storage_quota_manager = icn_store::StorageQuotaManager::new(
        1024 * 1024 * 1024, // 1GB global limit
        0.9,                // Start evicting at 90% capacity
    );
    let storage_quota_handle = Arc::new(RwLock::new(storage_quota_manager));

    // Set storage quota manager on gossip actor
    {
        let mut gossip = gossip_handle.write().await;
        gossip.set_storage_quota_manager(storage_quota_handle.clone());
    }

    info!("Storage quota manager initialized (global limit: 1GB, eviction threshold: 90%)");

    Ok(GossipServices {
        gossip_handle,
        gossip_store,
        partition_detector: partition_detector_handle,
        partition_healer: partition_healer_handle,
        storage_quota: storage_quota_handle,
        loaded_snapshot,
    })
}

/// Gossip topic for NAT traversal connection candidate announcements
pub const NETWORK_CANDIDATES_TOPIC: &str = "network:candidates";

/// Gossip topic for distributed snapshot coordination
pub const SNAPSHOT_COORDINATE_TOPIC: &str = "snapshot:coordinate";

/// Standard topic subscription configuration
pub struct TopicSubscriptionConfig {
    /// Whether federation features are enabled
    pub federation_enabled: bool,
}

/// Subscribe to standard gossip topics
///
/// Subscribes to core topics required for ICN operation:
/// - trust:attestations - Trust graph updates
/// - contracts:deploy - Contract deployment messages
/// - identity:recovery - Social recovery requests
/// - network:candidates - NAT traversal peer discovery
/// - snapshot:coordinate - Distributed snapshot coordination
/// - Federation topics (if enabled):
///   - federation:registry
///   - federation:trust
///   - federation:clearing
pub async fn subscribe_standard_topics(
    gossip: &mut GossipActor,
    did: &Did,
    config: TopicSubscriptionConfig,
) {
    // Subscribe to trust attestations topic
    if let Err(e) = gossip
        .subscribe(
            crate::trust_propagation::TRUST_ATTESTATIONS_TOPIC,
            did.clone(),
        )
        .await
    {
        warn!("Failed to subscribe to trust attestations topic: {}", e);
    } else {
        info!("Subscribed to trust:attestations topic");
    }

    // Subscribe to contracts:deploy topic with trust-gated access (min trust 0.4)
    if let Err(e) = gossip.subscribe("contracts:deploy", did.clone()).await {
        warn!("Failed to subscribe to contracts:deploy topic: {}", e);
    } else {
        info!("Subscribed to contracts:deploy topic");
    }

    // Subscribe to identity:recovery topic for social recovery events
    if let Err(e) = gossip
        .subscribe(icn_identity::IDENTITY_RECOVERY_TOPIC, did.clone())
        .await
    {
        warn!("Failed to subscribe to identity:recovery topic: {}", e);
    } else {
        info!("Subscribed to identity:recovery topic");
    }

    // Subscribe to network:candidates topic for NAT traversal peer discovery
    if let Err(e) = gossip
        .subscribe(NETWORK_CANDIDATES_TOPIC, did.clone())
        .await
    {
        warn!("Failed to subscribe to network:candidates topic: {}", e);
    } else {
        info!("Subscribed to network:candidates topic");
    }

    // Subscribe to snapshot:coordinate topic for distributed snapshots
    if let Err(e) = gossip
        .subscribe(SNAPSHOT_COORDINATE_TOPIC, did.clone())
        .await
    {
        warn!("Failed to subscribe to snapshot:coordinate topic: {}", e);
    } else {
        info!("Subscribed to snapshot:coordinate topic");
    }

    // Subscribe to federation topics if federation is enabled
    if config.federation_enabled {
        if let Err(e) = gossip
            .subscribe(icn_federation::TOPIC_FEDERATION_REGISTRY, did.clone())
            .await
        {
            warn!("Failed to subscribe to federation:registry topic: {}", e);
        } else {
            info!("Subscribed to federation:registry topic");
        }

        if let Err(e) = gossip
            .subscribe(icn_federation::TOPIC_FEDERATION_TRUST, did.clone())
            .await
        {
            warn!("Failed to subscribe to federation:trust topic: {}", e);
        } else {
            info!("Subscribed to federation:trust topic");
        }

        if let Err(e) = gossip
            .subscribe(icn_federation::TOPIC_FEDERATION_CLEARING, did.clone())
            .await
        {
            warn!("Failed to subscribe to federation:clearing topic: {}", e);
        } else {
            info!("Subscribed to federation:clearing topic");
        }

        if let Err(e) = gossip
            .subscribe(icn_federation::TOPIC_FEDERATION_AGREEMENTS, did.clone())
            .await
        {
            warn!("Failed to subscribe to federation:agreements topic: {}", e);
        } else {
            info!("Subscribed to federation:agreements topic");
        }
    }
}

/// Restore gossip state from snapshot if available
async fn restore_gossip_snapshot(
    config: &Config,
    gossip_handle: &Arc<RwLock<GossipActor>>,
) -> Option<StateSnapshot> {
    let data_dir = config.store_path();
    let load_start = std::time::Instant::now();
    let load_result = icn_snapshot::load_snapshot(&data_dir);
    let load_duration = load_start.elapsed();

    match load_result {
        Ok(Some(snapshot)) => {
            icn_obs::metrics::snapshot::load_total_inc();
            icn_obs::metrics::snapshot::load_duration_record(load_duration.as_secs_f64());

            info!(
                "Found state snapshot (version {}, created at {}) - loaded in {:.3}s",
                snapshot.version,
                snapshot.created_at,
                load_duration.as_secs_f64()
            );

            // Warn if checksum file is missing (legacy snapshot)
            let checksum_path = data_dir.join("state.snapshot.sha256");
            if !checksum_path.exists() {
                warn!("⚠️  Snapshot loaded without checksum verification (legacy snapshot). Run 'icnctl snapshot create' to generate checksum.");
            } else {
                info!("✅ Snapshot checksum verified");
            }

            // Record snapshot contents metrics
            if let Some(ref gossip_state) = snapshot.gossip_state {
                icn_obs::metrics::snapshot::gossip_vector_clock_entries_set(
                    gossip_state.vector_clock.len(),
                );
                icn_obs::metrics::snapshot::gossip_subscriptions_set(
                    gossip_state.subscriptions.len(),
                );
                icn_obs::metrics::snapshot::gossip_topics_set(gossip_state.topics.len());
            }
            if let Some(ref network_state) = snapshot.network_state {
                icn_obs::metrics::snapshot::network_x25519_keys_set(
                    network_state.peer_x25519_keys.len(),
                );
            }

            if let Some(gossip_state) = snapshot.gossip_state.clone() {
                let mut gossip = gossip_handle.write().await;
                if let Err(e) = gossip.restore_state(gossip_state) {
                    warn!("Failed to restore gossip state: {}", e);
                } else {
                    info!("✅ Gossip state restored from snapshot");
                }
            }

            Some(snapshot)
        }
        Ok(None) => {
            debug!("No state snapshot found, starting with fresh state");
            None
        }
        Err(e) => {
            icn_obs::metrics::snapshot::load_errors_inc();
            warn!("Failed to load state snapshot: {}", e);
            None
        }
    }
}
