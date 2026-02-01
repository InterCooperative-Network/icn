//! Cooperative-specific membership functionality
//!
//! This module provides cooperative-specific membership logic,
//! using CCL for membership rules.

use crate::entity::{EntityConfig, EntityId, MembershipClass};
use crate::membership::{MembershipError, MembershipManager, UnifiedMembership};
use icn_entity::MembershipRole;
use serde::{Deserialize, Serialize};

/// Cooperative membership configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoopMembershipConfig {
    /// Entity configuration
    pub entity: EntityConfig,

    /// Share-based voting
    pub share_based_voting: bool,

    /// Member tiers (worker, consumer, producer, etc.)
    pub tiers: Vec<MembershipClass>,
}

impl CoopMembershipConfig {
    /// Create a new cooperative membership configuration
    pub fn new(entity: EntityConfig) -> Self {
        Self {
            entity,
            share_based_voting: true,
            tiers: Vec::new(),
        }
    }

    /// Add a membership tier
    pub fn with_tier(mut self, tier: MembershipClass) -> Self {
        self.tiers.push(tier);
        self
    }
}

/// Cooperative membership manager
pub struct CoopMembershipManager {
    base_manager: MembershipManager,
}

impl CoopMembershipManager {
    /// Create a new cooperative membership manager
    pub fn new() -> Self {
        Self {
            base_manager: MembershipManager::new(),
        }
    }

    /// Add a member to a cooperative
    pub async fn add_coop_member(
        &self,
        member_id: EntityId,
        coop_id: EntityId,
        role: MembershipRole,
        config: &CoopMembershipConfig,
    ) -> Result<UnifiedMembership, MembershipError> {
        let min_trust = config.entity.membership_config.min_trust_threshold;
        self.base_manager
            .add_member(member_id, coop_id, role, min_trust)
            .await
    }

    /// Update member shares (for share-based voting)
    pub fn update_shares(
        &self,
        membership: &mut UnifiedMembership,
        new_shares: u64,
    ) -> Result<(), MembershipError> {
        if !membership.is_active() {
            return Err(MembershipError::PermissionDenied(
                "Cannot update shares for non-active member".to_string(),
            ));
        }

        membership.voting_weight = new_shares;
        membership.updated_at = icn_time::current_timestamp_secs();
        Ok(())
    }

    /// Add labor assignment
    pub fn add_assignment(&self, membership: &mut UnifiedMembership, assignment_id: String) {
        if !membership.assignments.contains(&assignment_id) {
            membership.assignments.push(assignment_id);
            membership.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Remove labor assignment
    pub fn remove_assignment(&self, membership: &mut UnifiedMembership, assignment_id: &str) {
        membership.assignments.retain(|id| id != assignment_id);
        membership.updated_at = icn_time::current_timestamp_secs();
    }

    /// Set primary membership (for multi-coop workers)
    pub fn set_primary(&self, membership: &mut UnifiedMembership, is_primary: bool) {
        membership.is_primary = is_primary;
        membership.updated_at = icn_time::current_timestamp_secs();
    }
}

impl Default for CoopMembershipManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Forward compatibility wrapper for icn-coop types
pub mod compat {
    use super::*;

    /// Convert icn-coop Member to UnifiedMembership
    pub fn from_coop_member(member: &icn_coop::Member) -> UnifiedMembership {
        let member_id = EntityId::individual(&member.did);
        let parent_id = EntityId::cooperative(&member.coop_id);

        let role = match member.role {
            icn_coop::MemberRole::Founder => MembershipRole::Founder,
            icn_coop::MemberRole::Member => MembershipRole::Member,
            icn_coop::MemberRole::Worker => MembershipRole::Worker,
            icn_coop::MemberRole::Consumer => MembershipRole::Consumer,
            icn_coop::MemberRole::Producer => MembershipRole::Producer,
            icn_coop::MemberRole::BoardMember => MembershipRole::BoardMember,
            icn_coop::MemberRole::Officer => MembershipRole::Officer,
        };

        let status = match member.status {
            icn_coop::MemberStatus::Pending => icn_entity::MembershipStatus::Pending,
            icn_coop::MemberStatus::Active => icn_entity::MembershipStatus::Active,
            icn_coop::MemberStatus::Suspended => icn_entity::MembershipStatus::Suspended,
            icn_coop::MemberStatus::Inactive => icn_entity::MembershipStatus::Inactive,
            icn_coop::MemberStatus::Removed => icn_entity::MembershipStatus::Removed,
        };

        UnifiedMembership {
            member_id,
            parent_id,
            role,
            status,
            joined_at: member
                .joined_at
                .and_utc()
                .timestamp()
                .try_into()
                .unwrap_or(0),
            updated_at: icn_time::current_timestamp_secs(),
            voting_weight: member.shares,
            capabilities: role.default_capabilities(),
            metadata: member.metadata.clone(),
            assignments: Vec::new(),
            labor_shares: Vec::new(),
            is_primary: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::MembershipConfig;
    use icn_entity::EntityType;

    fn create_test_coop_config() -> CoopMembershipConfig {
        let entity = EntityConfig {
            id: EntityId::cooperative("test-coop"),
            name: "Test Coop".to_string(),
            entity_type: EntityType::Cooperative,
            status: icn_entity::EntityStatus::Active,
            description: None,
            membership_config: MembershipConfig::default(),
            metadata: std::collections::HashMap::new(),
        };

        CoopMembershipConfig::new(entity)
    }

    #[tokio::test]
    async fn test_add_coop_member() {
        let manager = CoopMembershipManager::new();
        let config = create_test_coop_config();

        let member_id = EntityId::individual(icn_identity::KeyPair::generate().unwrap().did());
        let coop_id = EntityId::cooperative("test-coop");

        let membership = manager
            .add_coop_member(member_id, coop_id, MembershipRole::Worker, &config)
            .await
            .unwrap();

        assert!(matches!(
            membership.status,
            icn_entity::MembershipStatus::Pending
        ));
    }

    #[test]
    fn test_update_shares() {
        let manager = CoopMembershipManager::new();
        let member_id = EntityId::individual(icn_identity::KeyPair::generate().unwrap().did());
        let coop_id = EntityId::cooperative("test-coop");

        let mut membership =
            UnifiedMembership::active(member_id, coop_id, MembershipRole::Worker);

        assert!(manager.update_shares(&mut membership, 100).is_ok());
        assert_eq!(membership.voting_weight, 100);
    }

    #[test]
    fn test_labor_assignments() {
        let manager = CoopMembershipManager::new();
        let member_id = EntityId::individual(icn_identity::KeyPair::generate().unwrap().did());
        let coop_id = EntityId::cooperative("test-coop");

        let mut membership =
            UnifiedMembership::active(member_id, coop_id, MembershipRole::Worker);

        manager.add_assignment(&mut membership, "assignment-1".to_string());
        assert_eq!(membership.assignments.len(), 1);

        manager.remove_assignment(&mut membership, "assignment-1");
        assert_eq!(membership.assignments.len(), 0);
    }
}
