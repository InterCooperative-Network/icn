//! Governance actor initialization
//!
//! Initializes the governance actor, gossip topic, and store.
//!
//! The caller is responsible for:
//! - Providing a gossip handle and event bus
//! - Wiring kernel executors (protocol params, governance executor) AFTER calling this
//!
//! This module takes only primitive/domain types -- no `icn-core` config types.

use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_kernel_api::events::EventEmitter;
use icn_store::SledStore;

use crate::actor::{GovernanceActor, GovernanceHandle};
use icn_governance::{MembershipResolver, StaticMembershipResolver};

/// Dependencies required for governance actor initialization.
///
/// Uses only kernel-api / domain types -- no icn-core config.
pub struct GovernanceActorDeps {
    /// Handle to the gossip actor
    pub gossip_handle: Arc<RwLock<GossipActor>>,
    /// Event bus for inter-actor communication
    pub event_bus: Arc<dyn EventEmitter>,
    /// Ed25519 signing key for GovernanceProof generation (None if keystore unavailable)
    pub signing_key: Option<Arc<ed25519_dalek::SigningKey>>,
}

/// Services returned from governance actor initialization.
pub struct GovernanceActorServices {
    /// Handle to the governance actor
    pub governance_handle: GovernanceHandle,
    /// Governance store for audit trail
    pub governance_store: Arc<dyn icn_store::Store>,
}

/// Initialize the governance actor.
///
/// Creates:
/// - Governance gossip topic (`governance:proposal`)
/// - Governance store (Sled)
/// - Static membership resolver
/// - GovernanceActor
///
/// The returned `GovernanceHandle` does NOT yet have protocol params or
/// a kernel executor attached. The caller (typically icn-core supervisor)
/// should wire those via `GovernanceHandle::with_protocol_params()` and
/// `GovernanceHandle::with_executor()`.
pub async fn init_governance_actor(
    store_path: &Path,
    did: Did,
    deps: GovernanceActorDeps,
) -> anyhow::Result<GovernanceActorServices> {
    // Create governance topic before spawning GovernanceActor
    {
        let mut gossip = deps.gossip_handle.write().await;
        gossip.create_topic(icn_gossip::Topic::new(
            "governance:proposal".to_string(),
            icn_gossip::AccessControl::Public,
        ));
    }

    // Open governance store
    let gov_store_path = store_path.join("governance");
    let gov_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&gov_store_path)?);

    // Create membership resolver
    let gov_resolver: Arc<dyn MembershipResolver + Send + Sync> =
        Arc::new(StaticMembershipResolver::new());

    // Spawn GovernanceActor
    let governance_handle = GovernanceActor::spawn(
        did,
        gov_store.clone(),
        deps.gossip_handle.clone(),
        gov_resolver,
        Some(deps.event_bus),
        deps.signing_key,
    )
    .await?;

    info!("Governance actor spawned at {}", gov_store_path.display());

    Ok(GovernanceActorServices {
        governance_handle,
        governance_store: gov_store,
    })
}
