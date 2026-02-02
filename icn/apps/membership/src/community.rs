//! Community-specific membership functionality
//!
//! This module provides community-specific membership logic,
//! using CCL for membership rules.

use crate::entity::{EntityConfig, EntityId};
use crate::membership::{MembershipError, MembershipManager, MembershipTrait, UnifiedMembership};
use icn_entity::MembershipRole;
use serde::{Deserialize, Serialize};

/// Community membership configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityMembershipConfig {
    /// Entity configuration
    pub entity: EntityConfig,

    /// Community-specific member types
    pub member_types: Vec<MemberType>,

    /// Voting weight distribution
    pub voting_config: VotingConfig,
}

/// Member type in a community
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemberType {
    /// Individual member
    Individual,

    /// Cooperative member
    Cooperative,

    /// Organization member
    Organization,
}

/// Voting configuration for a community
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingConfig {
    /// Base weight for individuals
    pub individual_weight: u32,

    /// Base weight for cooperatives
    pub cooperative_weight: u32,

    /// Base weight for organizations
    pub organization_weight: u32,
}

impl Default for VotingConfig {
    fn default() -> Self {
        Self {
            individual_weight: 1,
            cooperative_weight: 5,
            organization_weight: 3,
        }
    }
}

impl CommunityMembershipConfig {
    /// Create a new community membership configuration
    pub fn new(entity: EntityConfig) -> Self {
        Self {
            entity,
            member_types: vec![
                MemberType::Individual,
                MemberType::Cooperative,
                MemberType::Organization,
            ],
            voting_config: VotingConfig::default(),
        }
    }

    /// Set voting configuration
    pub fn with_voting_config(mut self, config: VotingConfig) -> Self {
        self.voting_config = config;
        self
    }
}

/// Community membership manager
pub struct CommunityMembershipManager {
    base_manager: MembershipManager,
}

impl CommunityMembershipManager {
    /// Create a new community membership manager
    pub fn new() -> Self {
        Self {
            base_manager: MembershipManager::new(),
        }
    }

    /// Add a member to a community
    pub async fn add_community_member(
        &self,
        member_id: EntityId,
        community_id: EntityId,
        role: MembershipRole,
        member_type: MemberType,
        config: &CommunityMembershipConfig,
    ) -> Result<UnifiedMembership, MembershipError> {
        let min_trust = config.entity.membership_config.min_trust_threshold;

        let mut membership = self
            .base_manager
            .add_member(member_id, community_id, role, min_trust)
            .await?;

        // Set voting weight based on member type
        membership.shares = match member_type {
            MemberType::Individual => config.voting_config.individual_weight as u64,
            MemberType::Cooperative => config.voting_config.cooperative_weight as u64,
            MemberType::Organization => config.voting_config.organization_weight as u64,
        };

        // Store member type in metadata
        membership.metadata.insert(
            "member_type".to_string(),
            format!("{:?}", member_type),
        );

        Ok(membership)
    }

    /// Deactivate a member (soft delete)
    pub fn deactivate_member(
        &self,
        membership: &mut UnifiedMembership,
    ) -> Result<(), MembershipError> {
        if !membership.is_active() {
            return Err(MembershipError::InvalidStateTransition {
                from: format!("{:?}", membership.status),
                to: "Inactive".to_string(),
            });
        }

        membership.status = icn_entity::MembershipStatus::Inactive;
        membership.updated_at = icn_time::current_timestamp_secs();
        Ok(())
    }

    /// Update voting weight
    pub fn update_shares(
        &self,
        membership: &mut UnifiedMembership,
        new_weight: u32,
    ) -> Result<(), MembershipError> {
        if !membership.is_active() {
            return Err(MembershipError::PermissionDenied(
                "Cannot update voting weight for non-active member".to_string(),
            ));
        }

        membership.shares = new_weight as u64;
        membership.updated_at = icn_time::current_timestamp_secs();
        Ok(())
    }
}

impl Default for CommunityMembershipManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Forward compatibility wrapper for icn-community types
pub mod compat {
    use super::*;

    /// Convert icn-community Member to UnifiedMembership
    pub fn from_community_member(
        member: &icn_community::types::Member,
        community_id: &str,
    ) -> UnifiedMembership {
        // Parse member type from metadata
        let member_type = match &member.member_type {
            icn_community::MemberType::Individual(did_str) => {
                let did: icn_identity::Did = did_str
                    .as_str()
                    .parse()
                    .expect("Invalid DID in community member record");
                EntityId::from_did(&did)
            }
            icn_community::MemberType::Cooperative(id) => {
                EntityId::cooperative(id).expect("Invalid coop ID")
            }
        };

        let parent_id = EntityId::community(community_id).expect("Invalid community ID");

        UnifiedMembership {
            member_id: member_type,
            parent_id,
            role: MembershipRole::Member,
            status: if member.active {
                icn_entity::MembershipStatus::Active
            } else {
                icn_entity::MembershipStatus::Inactive
            },
            // Clamp pre-1970 timestamps to 0 (icn-entity expects u64)
            joined_at: member.joined_at.timestamp().max(0) as u64,
            updated_at: icn_time::current_timestamp_secs(),
            shares: member.voting_weight as u64,
            capabilities: MembershipRole::Member.default_capabilities(),
            metadata: std::collections::HashMap::new(),
            assignments: Vec::new(),
            labor_shares: Vec::new(),
            is_primary: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::MembershipConfig;
    use icn_entity::EntityType;

    fn create_test_community_config() -> CommunityMembershipConfig {
        let entity = EntityConfig {
            id: EntityId::community("test-comm").expect("Invalid community ID"),
            name: "Test Community".to_string(),
            entity_type: EntityType::Community,
            status: icn_entity::EntityStatus::Active,
            description: None,
            membership_config: MembershipConfig::default(),
            metadata: std::collections::HashMap::new(),
        };

        CommunityMembershipConfig::new(entity)
    }

    #[tokio::test]
    async fn test_add_community_member() {
        let manager = CommunityMembershipManager::new();
        let config = create_test_community_config();

        let member_id = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let community_id = EntityId::community("test-comm").expect("Invalid community ID");

        let membership = manager
            .add_community_member(
                member_id,
                community_id,
                MembershipRole::Member,
                MemberType::Individual,
                &config,
            )
            .await
            .unwrap();

        assert_eq!(membership.shares, 1);
    }

    #[tokio::test]
    async fn test_cooperative_shares() {
        let manager = CommunityMembershipManager::new();
        let config = create_test_community_config();

        let member_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");
        let community_id = EntityId::community("test-comm").expect("Invalid community ID");

        let membership = manager
            .add_community_member(
                member_id,
                community_id,
                MembershipRole::Member,
                MemberType::Cooperative,
                &config,
            )
            .await
            .unwrap();

        assert_eq!(membership.shares, 5);
    }

    #[test]
    fn test_deactivate_member() {
        let manager = CommunityMembershipManager::new();
        let member_id = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let community_id = EntityId::community("test-comm").expect("Invalid community ID");

        let mut membership =
            UnifiedMembership::active(member_id, community_id, MembershipRole::Member);

        assert!(manager.deactivate_member(&mut membership).is_ok());
        assert!(matches!(
            membership.status,
            icn_entity::MembershipStatus::Inactive
        ));
    }
}
