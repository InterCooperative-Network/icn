//! Actor handle types and coordination
//!
//! This module provides centralized definitions for actor handle types
//! used by the supervisor during initialization and gateway integration.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use icn_kernel_api::effects::KernelEffect;
use icn_kernel_api::events::EventCallback;

/// Handle types collected during actor initialization for gateway integration
#[derive(Default)]
pub struct GatewayActorHandles {
    pub event_broadcaster: Option<Arc<icn_gateway::EventBroadcaster>>,
    pub compute: Option<icn_compute::ComputeHandle>,
    pub coop: Option<icn_coop::CoopHandle>,
    /// Opaque community-domain handle, provided by the daemon's
    /// `community_factory` and transported without interpretation. `icn-core`
    /// never imports the community crate; the gateway shell (which is allowed
    /// to know the concrete type) downcasts this to the community crate's
    /// handle type when mounting the community surface. A downcast failure is
    /// a wiring bug, logged and left unmounted — the same degraded posture as
    /// `None`.
    pub community: Option<Box<dyn Any + Send + Sync>>,
    pub trust_service: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    pub ledger_service: Option<Arc<dyn icn_kernel_api::services::LedgerService>>,
    pub governance: Option<icn_gateway::governance_mgr::GovernanceHandle>,
    /// Concrete actor-backed governance handle. Parallel to `governance`
    /// (which is the trait-object alias). Needed for installing the
    /// gateway's receipt_store on the actor's inner state.
    pub governance_actor_handle: Option<icn_governance_actor::GovernanceHandle>,
    pub treasury: Option<icn_gateway::TreasuryHandle>,
    pub ledger: Option<icn_gateway::LedgerHandle>,
    pub entity: Option<icn_entity::EntityHandle>,
    /// Canonical, provenance-aware coop_id↔EntityId name-binding store (#2082/#2190).
    /// Threaded to the gateway so observe-mode treasury classification can consult a
    /// trusted, fail-closed `StoreBackedCoopEntityResolver` (A2c). Observe-only.
    pub coop_entity_map: Option<icn_coop::CoopEntityMapHandle>,
    pub steward: Option<icn_steward::StewardHandle>,
    pub agreement_manager: Option<icn_federation::agreement::AgreementManagerHandle>,
    pub service_discovery_manager:
        Option<Arc<icn_gateway::service_discovery_mgr::ServiceDiscoveryManager>>,
    pub naming_service: Option<Arc<dyn icn_kernel_api::naming::NamingService>>,
    /// Commons handle for substrate commons state (anchors, holders, charters, stewards, etc.)
    /// When provided, the gateway's CommonsManager delegates to this shared handle instead of
    /// opening its own sled store.
    pub commons: Option<icn_commons::CommonsHandle>,
    /// Hook invoked when a Charter proposal is accepted.
    /// Type-erased so `icn-core` stays domain-agnostic.
    pub charter_accepted_hook: Option<Arc<dyn Fn(String, String) + Send + Sync>>,
    /// Federation service for clearing position queries and settlement.
    /// When provided, the gateway uses this service-owned clearing state rather
    /// than its own divergent ClearingManager instance.
    pub federation_service: Option<Arc<dyn icn_kernel_api::services::FederationService>>,
    /// Settlement engine for compute audit queries (task_id → settled status).
    pub settlement_engine: Option<Arc<dyn icn_kernel_api::services::SettlementQueryService>>,
    /// Deferred dispatch-evidence sink installer. The daemon constructs one
    /// `Arc<DeferredDispatchEvidenceSink>` at bootstrap; a clone is handed
    /// to the kernel via `BootstrapHandles.dispatch_evidence_sink`, and
    /// this field carries the *same* Arc through to the gateway so it can
    /// install the receipt store as the concrete backend once it is open.
    pub dispatch_evidence_sink_installer:
        Option<Arc<icn_governance_actor::DeferredDispatchEvidenceSink>>,
    /// Runtime-owned execution-record store — the same sled store the decision
    /// executor writes execution records to. Shared into the gateway
    /// receipt-chain read API so `/v1/receipts/chain/{decision_hash}` reads real
    /// execution records instead of re-opening the (exclusively locked) sled
    /// path and falling back to an empty temporary store.
    pub execution_query_store: Option<Arc<dyn icn_store::Store>>,
    /// Runtime-owned session-revocation store, shared with the RPC auth manager
    /// so a revoked credential is rejected on both authenticated surfaces
    /// (issue #2437).
    pub revocation_store: Option<Arc<dyn icn_store::Store>>,
    /// Best-effort execution-record retention cleanup, deferred to run *after*
    /// the gateway's dispatch-evidence backfill (Issue #1987 follow-up). Pruning
    /// terminal execution records before the backfill could delete a record
    /// whose `EffectDispatchEvidence` was lost in the crash window before it can
    /// be healed. Set only when a gateway will actually start; otherwise the
    /// supervisor runs cleanup inline and leaves this `None`.
    pub post_backfill_cleanup: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// Core actor handles returned from initialization
#[derive(Default)]
pub struct CoreActorHandles {
    pub network: Option<icn_net::NetworkHandle>,
    pub gossip: Option<Arc<RwLock<icn_gossip::GossipActor>>>,
    pub ledger: Option<icn_gateway::LedgerHandle>,
}

/// Shutdown-related handles that need to persist state
#[derive(Default)]
pub struct ShutdownHandles {
    pub misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
    pub security_store: Option<Arc<dyn icn_store::Store>>,
}

/// Event subscription handles that must persist for daemon lifetime
#[derive(Default)]
pub struct EventSubscriptionHandles {
    pub governance_event_subscription: Option<crate::events::SubscriptionHandle>,
}

/// Factory that creates an event subscription routing accepted proposals
/// through the kernel effect system.
///
/// The daemon provides this factory (backed by `icn_governance_actor::create_effect_subscription`),
/// keeping governance-actor references out of the supervisor's lifecycle/effect wiring
/// and centralizing effect subscription creation in the daemon.
///
/// Arguments: `effect_callback` receives translated `Vec<KernelEffect>` + decision receipt ID.
/// Returns: an `EventCallback` suitable for `EventBus::subscribe()`.
pub type EffectSubscriptionFactory =
    Box<dyn FnOnce(Arc<dyn Fn(Vec<KernelEffect>, String) + Send + Sync>) -> EventCallback + Send>;

/// Generic runtime resources the supervisor can supply at the point a
/// domain-specific subsystem factory is invoked.
///
/// `icn-core` constructs this from resources it already owns (the live
/// gossip actor, node identity, and configured store root) — none of these
/// types carry domain meaning. The factory that consumes them is built by
/// the daemon composition root and knows what to do with them; `icn-core`
/// does not.
pub struct CommunityFactoryInputs {
    pub gossip: Arc<RwLock<icn_gossip::GossipActor>>,
    pub node_did: icn_identity::Did,
    pub store_path: PathBuf,
}

/// Factory for constructing and wiring the community-domain subsystem.
///
/// Mirrors `EffectSubscriptionFactory`: the daemon builds this closure
/// (capturing nothing live) and hands it to the supervisor via
/// `BootstrapHandles`; the supervisor invokes it once, at the point in the
/// actor-initialization sequence where its inputs first become available
/// (after the gossip actor is spawned). The factory owns construction,
/// configuration, gossip-topic subscription, and notification-callback
/// registration entirely within the daemon/community crate — `icn-core`
/// receives back only an opaque, type-erased handle to transport onward to
/// the gateway. This keeps `icn-community` (a domain crate) out of
/// `icn-core` (kernel) while preserving the exact construction sequence.
pub type CommunityFactory = Box<
    dyn FnOnce(
            CommunityFactoryInputs,
        )
            -> futures::future::BoxFuture<'static, anyhow::Result<Box<dyn Any + Send + Sync>>>
        + Send,
>;

/// Shared type alias for the concrete ledger handle.
///
/// This confines the concrete Ledger type to one declaration.
/// Other supervisor modules import this alias instead of referencing
/// the ledger crate directly.
pub type LedgerHandle = Arc<RwLock<icn_ledger::Ledger>>;

/// Type alias for the dispute manager handle.
pub type DisputeManagerHandle = Arc<RwLock<icn_ledger::DisputeManager>>;

/// Type alias for the treasury manager handle.
pub type TreasuryManagerHandle = Arc<RwLock<icn_ledger::TreasuryManager>>;

/// Typed handles for domain objects passed from daemon to supervisor.
///
/// Each field is a concrete, typed handle. The daemon constructs these
/// objects (opening sled stores, initializing ledger services, etc.) and
/// passes them to the supervisor via `Runtime::with_bootstrap_handles()`.
/// The supervisor wires them into actors during initialization.
///
/// Trust is provided separately via `ServiceRegistry::with_trust()` using
/// the `TrustService` trait — not through these handles.
///
/// See `icn-kernel-api::services::ServiceRegistry` for trait-based abstractions.
pub struct BootstrapHandles {
    /// Pre-initialized ledger handle.
    pub ledger: LedgerHandle,
    /// Shared sled store for ledger (prevents double-open due to exclusive flock).
    pub ledger_store: Arc<icn_store::SledStore>,
    /// Dispute manager for payment dispute resolution.
    pub dispute_manager: DisputeManagerHandle,
    /// Treasury manager for cooperative treasury operations.
    pub treasury_manager: TreasuryManagerHandle,
    /// Contract runtime for CCL execution.
    pub contract_runtime: Arc<RwLock<icn_ccl::ContractRuntime>>,
    /// Contract actor for contract lifecycle management.
    pub contract_actor: Arc<RwLock<icn_ccl::ContractActor>>,
    /// Protocol parameter store for governable parameters.
    pub protocol_parameter_store: Arc<dyn icn_kernel_api::protocol_params::ProtocolParameterStore>,
    /// Factory for creating the governance effect subscription.
    /// When provided, the supervisor uses this instead of calling `icn_governance_actor` directly.
    pub effect_subscription_factory: Option<EffectSubscriptionFactory>,
    /// Factory for constructing the community-domain subsystem (store, actor,
    /// gossip subscription, notification callback). Built by the daemon
    /// composition root to keep `icn-community` out of `icn-core`.
    pub community_factory: Option<CommunityFactory>,
    /// Hook invoked when a Charter proposal is accepted.
    /// Type-erased so `icn-core` stays domain-agnostic (no `icn-charter-app` import).
    /// The daemon builds this closure from `Arc<CharterPolicyOracle>`.
    pub charter_accepted_hook: Option<Arc<dyn Fn(String, String) + Send + Sync>>,
    /// Pre-built balance callback for commons credit gate checks.
    /// Built by the daemon (composition root) to keep `icn-ledger` out of `icn-core`.
    pub balance_callback: Option<icn_compute::BalanceCallback>,
    /// Pre-built payment callback for compute task payment settlement.
    /// Built by the daemon (composition root) to keep `icn-ledger` out of `icn-core`.
    pub payment_callback: Option<icn_compute::PaymentCallback>,
    /// Pre-built commons settlement callback for commons-scope task completion.
    /// Built by the daemon (composition root) to keep `icn-ledger` out of `icn-core`.
    pub commons_settlement_callback: Option<icn_compute::CommonsSettlementCallback>,
    /// Pre-built settlement query engine for task audit queries.
    /// The concrete `SettlementEngine` (from `icn-ledger`) is built in the daemon and
    /// injected here as a trait object so `icn-core` never imports `icn-ledger`.
    pub settlement_query_engine: Option<Arc<dyn icn_kernel_api::services::SettlementQueryService>>,
    /// Sink for durable per-effect dispatch evidence after decision execution.
    ///
    /// When provided, the supervisor wires this into the decision-executor
    /// callback so that every accepted proposal (gateway- or actor-originated)
    /// produces observable dispatch evidence via the same code path.
    /// Kernel-neutral trait — the app-side implementer persists evidence.
    pub dispatch_evidence_sink: Option<Arc<dyn icn_kernel_api::effects::DispatchEvidenceSink>>,
    /// Concrete installer companion to `dispatch_evidence_sink`. Carries the
    /// same underlying `Arc<DeferredDispatchEvidenceSink>` so the supervisor
    /// can forward it to the gateway; the gateway calls `install_backend`
    /// once the receipt store is open, flipping the deferred sink from
    /// forwarder-to-nowhere into a live writer.
    ///
    /// Optional — leaving this `None` keeps the deferred sink in its
    /// pre-install "log and drop" state, which is the correct behavior for
    /// configurations where no gateway / no receipt store exists (e.g.
    /// standalone supervisor-only tests).
    pub dispatch_evidence_sink_installer:
        Option<Arc<icn_governance_actor::DeferredDispatchEvidenceSink>>,
}
