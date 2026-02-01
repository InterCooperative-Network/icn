//! ICN Membership App
//!
//! Unified membership management for all entity types in ICN.
//! This app consolidates membership models from icn-entity, icn-coop,
//! and icn-community into a single, CCL-driven implementation.
//!
//! # Overview
//!
//! The membership app handles:
//! - Unified entity model (individuals, cooperatives, communities, federations)
//! - Generic membership trait for all entity types
//! - CCL-based membership criteria evaluation
//! - Cooperative-specific features (shares, labor assignments)
//! - Community-specific features (multi-type members, voting weights)
//!
//! # CCL Integration
//!
//! This is the second CCL consumer in ICN (after governance).
//! Membership criteria can be defined in CCL:
//!
//! ```yaml
//! entity:
//!   name: "Rochester Civic Assembly"
//!   type: community
//!   membership:
//!     classes:
//!       - name: resident
//!         criteria:
//!           all:
//!             - field: verified_address
//!               op: "=="
//!               value: true
//! ```
//!
//! # Architecture
//!
//! ```text
//! MembershipApp
//!   ├── entity (unified EntityId model)
//!   ├── membership (generic trait + UnifiedMembership)
//!   ├── coop (cooperative-specific logic)
//!   └── community (community-specific logic)
//! ```

pub mod community;
pub mod coop;
pub mod entity;
pub mod membership;

// Re-export main types
pub use community::{CommunityMembershipConfig, CommunityMembershipManager, MemberType};
pub use coop::{CoopMembershipConfig, CoopMembershipManager};
pub use entity::{Condition, EntityConfig, MembershipClass, MembershipConfig, MembershipCriteria};
pub use membership::{
    MembershipCapability, MembershipError, MembershipManager, MembershipRole, MembershipStatus,
    MembershipTrait, UnifiedMembership,
};

// Re-export EntityId for convenience
pub use icn_entity::EntityId;

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
        let config = EntityConfig::cooperative("test-coop", "Test Coop".to_string());
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

        assert!(matches!(
            membership.status,
            icn_entity::MembershipStatus::Pending
        ));
    }

    #[tokio::test]
    async fn test_coop_membership() {
        let coop_manager = CoopMembershipManager::new();
        let config = CoopMembershipConfig::new(EntityConfig::cooperative(
            "test-coop",
            "Test Coop".to_string(),
        ));

        let member_id = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let coop_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");

        let membership = coop_manager
            .add_coop_member(member_id, coop_id, MembershipRole::Worker, &config)
            .await
            .unwrap();

        assert!(matches!(
            membership.status,
            icn_entity::MembershipStatus::Pending
        ));
    }

    #[tokio::test]
    async fn test_community_membership() {
        let community_manager = CommunityMembershipManager::new();
        let config = CommunityMembershipConfig::new(EntityConfig::community(
            "test-comm",
            "Test Community".to_string(),
        ));

        let member_id = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let community_id = EntityId::community("test-comm").expect("Invalid community ID");

        let membership = community_manager
            .add_community_member(
                member_id,
                community_id,
                MembershipRole::Member,
                MemberType::Individual,
                &config,
            )
            .await
            .unwrap();

        assert_eq!(membership.voting_weight, 1);
    }

    #[test]
    fn test_all_entity_types_supported() {
        // Test that all entity types can be created
        let _individual =
            EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let _coop = EntityId::cooperative("test-coop").expect("Invalid coop ID");
        let _community = EntityId::community("test-comm").expect("Invalid community ID");
        let _federation = EntityId::federation("test-fed").expect("Invalid federation ID");
    }
}
