//! Service registry for supervisor initialization
//!
//! Provides structured containers for passing services between initialization modules.

use std::sync::Arc;
use tokio::sync::RwLock;

use icn_gossip::GossipActor;
use icn_identity::{Did, IdentityBundle};
use icn_ledger::{DisputeManager, Ledger};
use icn_net::NetworkHandle;
use icn_security::MisbehaviorDetector;
use icn_trust::TrustGraph;

use crate::dead_letter::DeadLetterQueue;
use crate::events::EventBus;
use crate::governance::GovernanceHandle;
use crate::node::NodeProfile;

/// Services initialized during trust setup
pub struct TrustServices {
    pub trust_graph: Arc<RwLock<TrustGraph>>,
    pub misbehavior_detector: Arc<RwLock<MisbehaviorDetector>>,
    pub recovery_store: Arc<dyn icn_store::Store>,
}

/// Services initialized during gossip setup
pub struct GossipServices {
    pub gossip_handle: Arc<RwLock<GossipActor>>,
    pub gossip_store: Arc<dyn icn_store::Store>,
}

/// Services initialized during ledger setup
pub struct LedgerServices {
    pub ledger_handle: Arc<RwLock<Ledger>>,
    pub dispute_manager: Arc<RwLock<DisputeManager>>,
    pub contract_runtime: Arc<icn_ccl::ContractRuntime>,
    pub contract_actor: Arc<RwLock<icn_ccl::ContractActor>>,
}

/// Services initialized during network setup
pub struct NetworkServices {
    pub network_handle: NetworkHandle,
    pub identity_handle: crate::identity::IdentityHandle,
}

/// Services initialized during governance setup
pub struct GovernanceServices {
    pub governance_handle: GovernanceHandle,
    pub event_bus: Arc<EventBus>,
    pub dead_letter_queue: Arc<DeadLetterQueue<icn_store::SledStore>>,
}

/// Complete service registry holding all initialized services
///
/// This is built up incrementally during supervisor initialization
/// and provides a single place to access all service handles.
pub struct ServiceRegistry {
    // Core identity
    pub did: Did,
    pub identity_bundle: IdentityBundle,

    // Foundation services
    pub trust_services: TrustServices,
    pub gossip_services: GossipServices,
    pub ledger_services: LedgerServices,
    pub network_services: NetworkServices,

    // Governance
    pub governance_services: GovernanceServices,

    // Node profile
    pub node_profile: Arc<RwLock<NodeProfile>>,
}

impl ServiceRegistry {
    /// Get the trust graph handle
    pub fn trust_graph(&self) -> &Arc<RwLock<TrustGraph>> {
        &self.trust_services.trust_graph
    }

    /// Get the gossip actor handle
    pub fn gossip(&self) -> &Arc<RwLock<GossipActor>> {
        &self.gossip_services.gossip_handle
    }

    /// Get the ledger handle
    pub fn ledger(&self) -> &Arc<RwLock<Ledger>> {
        &self.ledger_services.ledger_handle
    }

    /// Get the network handle
    pub fn network(&self) -> &NetworkHandle {
        &self.network_services.network_handle
    }

    /// Get the governance handle
    pub fn governance(&self) -> &GovernanceHandle {
        &self.governance_services.governance_handle
    }
}
