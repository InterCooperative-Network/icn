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
use icn_governance::{
    MembershipResolver, StaticMembershipResolver, TrustServiceMembershipResolver,
};

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
    /// Optional TrustService for TrustThreshold membership resolution.
    ///
    /// When provided, the governance actor is initialized with a
    /// `TrustServiceMembershipResolver` that can handle both `StaticList` and
    /// `TrustThreshold` membership sources.  Without this, only `StaticList`
    /// domains can be closed (TrustThreshold close will error at quorum calculation).
    pub trust_service: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
}

/// Services returned from governance actor initialization.
///
/// # No raw governance-store handle (#2864 A2)
///
/// This used to carry the daemon's raw governance `dyn Store` ("for audit
/// trail"). Nothing used it for that; it only invited a second, unrestricted
/// writer on the store that holds `gov:domain:*` records. It is gone: the only
/// holders of that handle are now inside the actor. The control compiles; the
/// `compile_fail` block differs from it only by the removed field.
///
/// ```
/// use icn_governance_actor::init::GovernanceActorServices;
/// fn _only_the_handle(s: GovernanceActorServices) {
///     let _ = s.governance_handle;
/// }
/// ```
///
/// ```compile_fail
/// use icn_governance_actor::init::GovernanceActorServices;
/// fn _only_the_handle(s: GovernanceActorServices) {
///     let _ = s.governance_handle;
///     let _raw = s.governance_store;
/// }
/// ```
pub struct GovernanceActorServices {
    /// Handle to the governance actor
    pub governance_handle: GovernanceHandle,
}

/// Initialize the governance actor.
///
/// Creates:
/// - Governance gossip topic (`governance:proposal`)
/// - Governance store (Sled, opened at `<store_path>/governance` and owned by
///   the actor from here on: no handle to it is returned)
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
    let gov_store_path = store_path.join("governance");
    let gov_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&gov_store_path)?);
    init_governance_actor_with_store(gov_store, store_path, did, deps).await
}

/// [`init_governance_actor`] with the governance store supplied by the caller.
///
/// The store is moved into the actor. A caller that wants to inspect persisted
/// state while the actor runs (sled's directory lock forbids reopening the
/// directory) opens the store itself and keeps its own handle, so it is that
/// caller's handle, not one this crate hands out. `store_path` still locates the
/// sibling `governance_action_items` database.
pub async fn init_governance_actor_with_store(
    gov_store: Arc<dyn icn_store::Store>,
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

    // Create membership resolver.
    // When a TrustService is available, use TrustServiceMembershipResolver so that
    // TrustThreshold domains can be closed (quorum denominator + delegation scope).
    // Without a trust service, fall back to StaticMembershipResolver which only
    // handles StaticList domains and errors on TrustThreshold.
    let gov_resolver: Arc<dyn MembershipResolver + Send + Sync> =
        if let Some(trust_svc) = deps.trust_service {
            Arc::new(TrustServiceMembershipResolver::new(trust_svc))
        } else {
            Arc::new(StaticMembershipResolver::new())
        };

    // Spawn GovernanceActor
    let governance_handle = GovernanceActor::spawn(
        did,
        gov_store,
        deps.gossip_handle.clone(),
        gov_resolver,
        Some(deps.event_bus),
        deps.signing_key,
    )
    .await?;

    // Wire action item store for the decision-to-action bridge.
    // Opens a dedicated sled DB alongside the governance state store.
    let action_item_path = store_path.join("governance_action_items");
    let action_item_db = sled::open(&action_item_path)
        .map_err(|e| anyhow::anyhow!("Failed to open action item store: {e}"))?;
    let action_store: Arc<dyn icn_governance::ActionItemStoreBackend> = Arc::new(
        crate::manager::SledActionItemStore::new(Arc::new(action_item_db)),
    );
    let governance_handle = governance_handle.with_action_item_store(action_store);

    info!(
        "Governance actor spawned at {}",
        store_path.join("governance").display()
    );

    Ok(GovernanceActorServices { governance_handle })
}
