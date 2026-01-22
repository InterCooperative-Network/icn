//! Actor handle types and coordination
//!
//! This module provides centralized definitions for actor handle types
//! used by the supervisor during initialization and gateway integration.

use std::sync::Arc;
use tokio::sync::RwLock;

/// Handle types collected during actor initialization for gateway integration
pub struct GatewayActorHandles {
    pub event_broadcaster: Option<Arc<icn_gateway::EventBroadcaster>>,
    pub compute: Option<icn_compute::ComputeHandle>,
    pub coop: Option<icn_coop::CoopHandle>,
    pub community: Option<icn_community::CommunityHandle>,
    pub trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,
    pub governance: Option<Arc<dyn icn_governance::GovernanceOps + Send + Sync>>,
    pub treasury: Option<icn_gateway::TreasuryHandle>,
    pub ledger: Option<icn_gateway::LedgerHandle>,
    pub entity: Option<super::init_entity::EntityHandle>,
    pub steward: Option<icn_steward::StewardHandle>,
    pub agreement_manager: Option<icn_federation::agreement::AgreementManagerHandle>,
}

impl Default for GatewayActorHandles {
    fn default() -> Self {
        Self {
            event_broadcaster: None,
            compute: None,
            coop: None,
            community: None,
            trust_graph: None,
            governance: None,
            treasury: None,
            ledger: None,
            entity: None,
            steward: None,
            agreement_manager: None,
        }
    }
}

/// Core actor handles returned from initialization
pub struct CoreActorHandles {
    pub network: Option<icn_net::NetworkHandle>,
    pub gossip: Option<Arc<RwLock<icn_gossip::GossipActor>>>,
    pub ledger: Option<icn_gateway::LedgerHandle>,
}

impl Default for CoreActorHandles {
    fn default() -> Self {
        Self {
            network: None,
            gossip: None,
            ledger: None,
        }
    }
}

/// Shutdown-related handles that need to persist state
pub struct ShutdownHandles {
    pub misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,
    pub security_store: Option<Arc<dyn icn_store::Store>>,
}

impl Default for ShutdownHandles {
    fn default() -> Self {
        Self {
            misbehavior_detector: None,
            security_store: None,
        }
    }
}

/// Event subscription handles that must persist for daemon lifetime
pub struct EventSubscriptionHandles {
    pub governance_event_subscription: Option<crate::events::SubscriptionHandle>,
    pub policy_governance_subscription: Option<crate::events::SubscriptionHandle>,
}

impl Default for EventSubscriptionHandles {
    fn default() -> Self {
        Self {
            governance_event_subscription: None,
            policy_governance_subscription: None,
        }
    }
}
