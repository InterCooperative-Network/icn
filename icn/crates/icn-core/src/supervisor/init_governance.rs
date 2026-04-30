//! Governance services initialization (thin wrapper)
//!
//! Delegates governance actor spawning to `icn_governance_actor::init` and
//! then creates kernel-level infrastructure (UpgradeActor, DeadLetterQueue,
//! VersionTracker, KernelGovernanceExecutor) that belongs in icn-core.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::config::Config;
use icn_governance_actor::GovernanceHandle;
use icn_identity::Did;
use icn_kernel_api::protocol_params::ProtocolParameterStore;
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
    /// Pre-created parameter store from daemon (with defaults already loaded)
    pub protocol_parameter_store: Arc<dyn ProtocolParameterStore>,
    /// Ed25519 signing key for GovernanceProof generation (None if keystore unavailable)
    pub signing_key: Option<Arc<ed25519_dalek::SigningKey>>,
    /// Optional TrustService for TrustThreshold membership resolution.
    ///
    /// Passed through to `GovernanceActorDeps` so the governance actor is
    /// initialized with a `TrustServiceMembershipResolver` instead of the
    /// fallback `StaticMembershipResolver`.  Without this, TrustThreshold
    /// domains cannot be closed (actor's quorum calculation errors).
    pub trust_service: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    /// Charter policy oracle for charter-derived threshold evaluation.
    /// When present, wired into the governance actor so that `CloseProposal`
    /// can consult charter thresholds as a fallback when protocol params absent.
    pub charter_oracle: Option<Arc<dyn icn_kernel_api::authz::PolicyOracle>>,
}

/// Services returned from governance initialization
pub struct GovernanceServices {
    /// Handle to the governance actor
    pub governance_handle: GovernanceHandle,
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
/// Delegates governance actor spawning to `icn_governance_actor::init`, then
/// creates kernel-level infrastructure:
/// - UpgradeActor for network-wide upgrade coordination
/// - Dead-letter queue for failed operations recovery
/// - Version tracker for upgrade state
/// - KernelGovernanceExecutor for proposal execution
pub async fn init_governance_services(
    config: &Config,
    did: Did,
    deps: GovernanceDeps,
) -> anyhow::Result<GovernanceServices> {
    // Delegate governance actor spawning to the governance app crate
    let actor_services = icn_governance_actor::init::init_governance_actor(
        &config.store_path(),
        did.clone(),
        icn_governance_actor::init::GovernanceActorDeps {
            gossip_handle: deps.gossip_handle.clone(),
            event_bus: deps.event_bus.clone(),
            signing_key: deps.signing_key,
            trust_service: deps.trust_service,
        },
    )
    .await?;

    // --- Kernel-level infrastructure below ---

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

    // Protocol parameter store is provided by the daemon (with defaults already loaded)
    let protocol_parameter_store = deps.protocol_parameter_store;
    {
        let count = protocol_parameter_store.list()?.len();
        info!("✓ Protocol parameter store ready ({} parameters)", count);
    }

    // Create the kernel governance executor for proposal execution
    let governance_executor = Arc::new(
        crate::supervisor::governance_executor::KernelGovernanceExecutor::new(
            protocol_parameter_store.clone(),
        ),
    );
    info!("✓ Governance executor created");

    // Attach protocol parameter store, executor, and charter oracle to governance handle.
    // The charter oracle is optional (not available in test environments or early bootstrap);
    // the governance actor falls back to protocol params / domain config when absent.
    let governance_handle_base = actor_services
        .governance_handle
        .with_protocol_params(protocol_parameter_store.clone())
        .with_executor(governance_executor);
    let governance_handle = if let Some(oracle) = deps.charter_oracle {
        governance_handle_base.with_charter_oracle(oracle)
    } else {
        governance_handle_base
    };

    Ok(GovernanceServices {
        governance_handle,
        upgrade_handle,
        version_tracker,
        dead_letter_queue,
        governance_store: actor_services.governance_store,
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
