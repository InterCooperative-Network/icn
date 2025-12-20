//! Governance actor initialization
//!
//! This module extracts the governance actor setup from the supervisor,
//! providing a cleaner separation of concerns for governance services.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::config::Config;
use icn_identity::Did;
use icn_store::SledStore;

/// Type alias for the event bus
pub type EventBus = Arc<crate::events::EventBus>;

/// Type alias for the gossip handle
pub type GossipHandle = Arc<RwLock<icn_gossip::GossipActor>>;

/// Type alias for the version tracker
pub type VersionTracker = Arc<RwLock<crate::supervisor::version_tracker::VersionTracker>>;

/// Dependencies required for governance initialization
pub struct GovernanceDeps {
    /// Handle to the gossip actor
    pub gossip_handle: GossipHandle,
    /// Event bus for inter-actor communication
    pub event_bus: EventBus,
    /// Shutdown signal receiver
    pub shutdown_rx: tokio::sync::broadcast::Receiver<()>,
}

/// Services returned from governance initialization
pub struct GovernanceServices {
    /// Handle to the governance actor
    pub governance_handle: crate::governance::GovernanceHandle,
    /// Handle to the upgrade actor
    pub upgrade_handle: crate::upgrade_actor::UpgradeHandle,
    /// Version tracker for upgrade coordination
    pub version_tracker: VersionTracker,
    /// Dead-letter queue for failed operations recovery
    pub dead_letter_queue: Arc<crate::dead_letter::DeadLetterQueue<SledStore>>,
    /// Governance store for audit trail
    pub governance_store: Arc<dyn icn_store::Store>,
}

/// Initialize governance services
///
/// Creates:
/// - GovernanceActor for proposal management
/// - UpgradeActor for network-wide upgrade coordination
/// - Dead-letter queue for failed operations recovery
/// - Version tracker for upgrade state
pub async fn init_governance_services(
    config: &Config,
    did: Did,
    deps: GovernanceDeps,
) -> anyhow::Result<GovernanceServices> {
    // Create governance topic before spawning GovernanceActor
    {
        let mut gossip = deps.gossip_handle.write().await;
        gossip.create_topic(icn_gossip::Topic::new(
            "governance:proposal".to_string(),
            icn_gossip::AccessControl::Public,
        ));
    }

    // Spawn Governance actor
    let gov_store_path = config.store_path().join("governance");
    let gov_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&gov_store_path)?);
    let gov_resolver: Arc<dyn icn_governance::MembershipResolver + Send + Sync> =
        Arc::new(icn_governance::StaticMembershipResolver::new());

    let governance_handle = crate::governance::GovernanceActor::spawn(
        did.clone(),
        gov_store.clone(),
        deps.gossip_handle.clone(),
        gov_resolver,
        Some(deps.event_bus.clone()),
    )
    .await?;

    info!(
        "✓ Governance actor spawned at {}",
        gov_store_path.display()
    );

    // Spawn UpgradeActor for network-wide upgrade coordination
    let current_version = icn_governance::proposal::Version::new(
        crate::upgrade::CURRENT_VERSION.0,
        crate::upgrade::CURRENT_VERSION.1,
        crate::upgrade::CURRENT_VERSION.2,
    );
    let version_tracker = Arc::new(RwLock::new(
        crate::supervisor::version_tracker::VersionTracker::new(current_version.clone()),
    ));
    let version_string = format!(
        "{}.{}.{}",
        current_version.major, current_version.minor, current_version.patch
    );
    let upgrade_handle = crate::upgrade_actor::UpgradeActor::spawn(
        did.clone(),
        version_string.clone(),
        version_tracker.clone(),
        deps.gossip_handle.clone(),
        deps.shutdown_rx,
    );
    info!(
        "✓ Upgrade coordinator spawned (version: {})",
        version_string
    );
    icn_obs::metrics::supervisor::actor_spawned_inc("upgrade");

    // Create dead-letter queue for failed operations recovery
    let dlq_store_path = config.store_path().join("dead_letter");
    let dlq_store = Arc::new(SledStore::open(&dlq_store_path)?);
    let dead_letter_queue = Arc::new(crate::dead_letter::DeadLetterQueue::new(dlq_store));
    info!(
        "✓ Dead-letter queue initialized at {}",
        dlq_store_path.display()
    );

    Ok(GovernanceServices {
        governance_handle,
        upgrade_handle,
        version_tracker,
        dead_letter_queue,
        governance_store: gov_store,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governance_deps_struct() {
        // Verify GovernanceDeps fields are accessible
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<GossipHandle>();
        assert_send_sync::<EventBus>();
    }
}
