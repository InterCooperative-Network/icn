//! Actor handle types and coordination
//!
//! This module provides centralized definitions for actor handle types
//! used by the supervisor during initialization and gateway integration.

use std::sync::Arc;
use tokio::sync::RwLock;

/// Handle types collected during actor initialization for gateway integration
#[derive(Default)]
pub struct GatewayActorHandles {
    pub event_broadcaster: Option<Arc<icn_gateway::EventBroadcaster>>,
    pub compute: Option<icn_compute::ComputeHandle>,
    pub coop: Option<icn_coop::CoopHandle>,
    pub community: Option<icn_community::CommunityHandle>,
    pub trust_service: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    pub governance: Option<Arc<dyn icn_governance::GovernanceOps + Send + Sync>>,
    pub treasury: Option<icn_gateway::TreasuryHandle>,
    pub ledger: Option<icn_gateway::LedgerHandle>,
    pub entity: Option<icn_entity::EntityHandle>,
    pub steward: Option<icn_steward::StewardHandle>,
    pub agreement_manager: Option<icn_federation::agreement::AgreementManagerHandle>,
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
    pub policy_governance_subscription: Option<crate::events::SubscriptionHandle>,
}

/// Typed handles for domain objects passed from daemon to supervisor.
///
/// This replaces the type-erased `raw_handles` HashMap that was previously
/// on `ServiceRegistry`. Each field is a concrete, typed handle — no
/// `Any` downcasting required.
///
/// The daemon constructs these objects (opening sled stores, initializing
/// ledger services, etc.) and passes them to the supervisor. The supervisor
/// wires them into actors during initialization.
///
/// **Note**: `TrustGraph` is intentionally absent — all kernel components
/// (MisbehaviorDetector, ReplicationManager, StorageChallenge, RPC) have
/// migrated to the `TrustService` trait on `ServiceRegistry`. The daemon
/// passes trust via `ServiceRegistry::with_trust()` instead.
///
/// See `icn-kernel-api::services::ServiceRegistry` for trait-based abstractions.
pub struct BootstrapHandles {
    /// Pre-initialized ledger handle.
    pub ledger: Arc<RwLock<icn_ledger::Ledger>>,
    /// Shared sled store for ledger (prevents double-open due to exclusive flock).
    pub ledger_store: Arc<icn_store::SledStore>,
    /// Dispute manager for payment dispute resolution.
    pub dispute_manager: Arc<RwLock<icn_ledger::DisputeManager>>,
    /// Treasury manager for cooperative treasury operations.
    pub treasury_manager: Arc<RwLock<icn_ledger::TreasuryManager>>,
    /// Contract runtime for CCL execution.
    pub contract_runtime: Arc<RwLock<icn_ccl::ContractRuntime>>,
    /// Contract actor for contract lifecycle management.
    pub contract_actor: Arc<RwLock<icn_ccl::ContractActor>>,
    /// Protocol parameter store for governable parameters.
    pub protocol_parameter_store: Arc<dyn icn_kernel_api::protocol_params::ProtocolParameterStore>,
}
