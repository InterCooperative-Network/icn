//! ICN Membership App
//!
//! Unified membership management for all entity types in ICN.
//! This app consolidates entity, cooperative, and community models
//! into a single crate.
//!
//! # Architecture
//!
//! ```text
//! icn-membership-app
//!   ├── entity_core  (from icn-entity: EntityId, CooperativeEntity, Membership, Registry)
//!   ├── coop_core    (from icn-coop: Cooperative, CoopActor, CoopStore, treasury)
//!   ├── community_core (from icn-community: Community, CommunityActor, CommunityStore)
//!   ├── entity       (unified EntityConfig, MembershipClass, CCL criteria)
//!   ├── membership   (unified MembershipTrait, UnifiedMembership, CCL evaluation)
//!   ├── coop         (cooperative-specific membership wrappers)
//!   └── community    (community-specific membership wrappers)
//! ```
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

// === Core modules (absorbed from original crates) ===
pub mod community_core;
pub mod coop_core;
pub mod entity_core;

// === Unified wrapper modules ===
pub mod community;
pub mod coop;
pub mod entity;
pub mod membership;

// === Re-export entity_core types (backward compat with icn-entity) ===
pub use entity_core::actor::{EntityActor, GossipHandle as EntityGossipHandle, ENTITY_TOPIC};
pub use entity_core::entity::{
    AccountId, AccountReference, CommunityProfile, CooperativeEntity, CooperativeProfile, EntityId,
    EntityKind, EntityRelationship, EntityStatus, EntityType, FederationProfile, RelationType,
};
pub use entity_core::error::{EntityError, Result as EntityResult};
pub use entity_core::handle::EntityHandle;
pub use entity_core::labor_exchange::{
    AssignmentApprovals, AssignmentId, AssignmentStatus, AssignmentType, Availability,
    CreditRouting, LaborAssignment, LaborNeed, LaborPool, PoolRegistration, RoutingMode,
};
pub use entity_core::lifecycle::{EntityLifecycle, LifecycleEvent as EntityLifecycleEvent};
pub use entity_core::membership::{
    Membership, MembershipCapability, MembershipRole, MembershipStatus, UnifiedMembershipStatus,
};
pub use entity_core::registry::{EntityRegistry, EntityRegistryHandle, InMemoryRegistry};
pub use entity_core::sled_registry::SledEntityRegistry;

// === Re-export coop_core types (backward compat with icn-coop) ===
pub use coop_core::actor::{CoopActor, COOP_TOPIC};
pub use coop_core::actor::{GossipHandle as CoopGossipHandle, TreasuryManagerHandle};
pub use coop_core::error::{CoopError, Result as CoopResult};
pub use coop_core::handle::CoopHandle;
pub use coop_core::lifecycle::{
    derive_treasury_anchor, derive_treasury_did, LifecycleEvent as CoopLifecycleEvent,
    LifecycleManager as CoopLifecycleManager,
};
pub use coop_core::membership::{MembershipChange, MembershipManager as CoopMembershipMgr};
pub use coop_core::store::CoopStore;
pub use coop_core::types::{
    AssetDistributionPlan, BalanceAction, CapitalReturnMethod, CoopStatus, CoopType, Cooperative,
    CooperativeId, DebtAction, DissolutionRequest, FormationRequest as CoopFormationRequest,
    Member as CoopMember, MemberRole as CoopMemberRole, MemberStatus as CoopMemberStatus,
    MembershipApplication, MembershipTier, StoredFounderSignature,
};

// === Re-export community_core types (backward compat with icn-community) ===
pub use community_core::actor::{CommunityActor, CommunityMessage, COMMUNITY_TOPIC};
pub use community_core::error::{CommunityError, Result as CommunityResult};
pub use community_core::handle::CommunityHandle;
pub use community_core::lifecycle::{
    CommunityLifecycle, FormationRequest as CommunityFormationRequest,
};
pub use community_core::membership::{
    MemberApplication, MembershipManager as CommunityMembershipMgr,
};
pub use community_core::resources::{ResourceAllocation, ResourceManager};
pub use community_core::store::{CommunityQuery, CommunityStore};
pub use community_core::types::{
    Community, CommunityStatus, CommunityType, MemberType as CommunityMemberType, ResourcePool,
};

// === Unified wrapper re-exports ===
pub use self::community::{CommunityMembershipConfig, CommunityMembershipManager, MemberType};
pub use self::coop::{CoopMembershipConfig, CoopMembershipManager};
pub use self::entity::{
    Condition, EntityConfig, MembershipClass, MembershipConfig, MembershipCriteria,
};
pub use self::membership::{
    MembershipError, MembershipManager, MembershipTrait, UnifiedMembership,
};

/// App metadata constant
const MEMBERSHIP_MANIFEST: &str = include_str!("../manifest.yaml");

/// Get the manifest for this app
pub fn manifest() -> &'static str {
    MEMBERSHIP_MANIFEST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_loads() {
        let manifest = manifest();
        assert!(manifest.contains("name: membership"));
        assert!(manifest.contains("ccl_schemas"));
    }

    #[test]
    fn test_entity_config_creation() {
        let config = EntityConfig::cooperative("test-coop", "Test Coop".to_string()).unwrap();
        assert_eq!(config.name, "Test Coop");
    }

    #[tokio::test]
    async fn test_membership_manager() {
        let manager = MembershipManager::new();
        let member_id = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let parent_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");

        let membership = manager
            .add_member(member_id, parent_id, MembershipRole::Worker, 0.3)
            .await
            .unwrap();

        assert!(matches!(membership.status, MembershipStatus::Pending));
    }

    #[test]
    fn test_all_entity_types_supported() {
        let _individual = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let _coop = EntityId::cooperative("test-coop").expect("Invalid coop ID");
        let _community = EntityId::community("test-comm").expect("Invalid community ID");
        let _federation = EntityId::federation("test-fed").expect("Invalid federation ID");
    }
}
