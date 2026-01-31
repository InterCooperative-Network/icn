//! Governance actor initialization
//!
//! This module extracts the governance actor setup from the supervisor,
//! providing a cleaner separation of concerns for governance services.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::config::Config;
use icn_governance::{default_parameters, ProtocolParameterStore, SledParameterStore};
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
    /// Pre-created parameter store from daemon (avoids double sled open)
    pub protocol_parameter_store: Option<Arc<dyn ProtocolParameterStore>>,
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
    /// Protocol parameter store for governable parameters (Phase 20)
    pub protocol_parameter_store: Arc<dyn ProtocolParameterStore>,
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

    info!("✓ Governance actor spawned at {}", gov_store_path.display());

    // Spawn UpgradeActor for network-wide upgrade coordination
    let current_version = icn_kernel_api::Version::new(
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

    // Initialize protocol parameter store (Phase 20)
    let protocol_parameter_store: Arc<dyn ProtocolParameterStore> =
        if let Some(store) = deps.protocol_parameter_store {
            info!("Using daemon-provided protocol parameter store");
            store
        } else {
            let param_store_path = config.protocol_params_path();
            let param_db = sled::open(&param_store_path)?;
            Arc::new(SledParameterStore::new(Arc::new(param_db))?)
        };

    // Load default parameters on first run (if store is empty)
    {
        let existing = protocol_parameter_store.list()?;
        if existing.is_empty() {
            info!("Loading default protocol parameters...");
            let defaults = default_parameters();
            let count = defaults.len();
            for param in defaults {
                protocol_parameter_store.set(param, None, None)?;
            }
            info!("✓ {} default protocol parameters initialized", count);

            // Emit event for observability
            let event = crate::events::SystemEvent::ProtocolParametersInitialized {
                count,
                initialized_at: icn_time::current_timestamp_secs(),
            };
            deps.event_bus.emit(event).await;
        } else {
            let count = existing.len();
            info!("✓ Protocol parameter store loaded ({} parameters)", count);

            // Emit event for observability
            let event = crate::events::SystemEvent::ProtocolParametersLoaded {
                count,
                loaded_at: icn_time::current_timestamp_secs(),
            };
            deps.event_bus.emit(event).await;
        }
    }

    // Attach protocol parameter store to governance handle
    let governance_handle =
        governance_handle.with_protocol_params(protocol_parameter_store.clone());

    Ok(GovernanceServices {
        governance_handle,
        upgrade_handle,
        version_tracker,
        dead_letter_queue,
        governance_store: gov_store,
        protocol_parameter_store,
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
