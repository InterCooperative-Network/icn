//! Cooperative-specific membership functionality
//!
//! This module provides cooperative-specific membership logic,
//! using CCL for membership rules.

use crate::entity::{EntityConfig, EntityId, MembershipClass};
use crate::entity_core::membership::MembershipRole;
use crate::membership::{MembershipError, MembershipManager, MembershipTrait, UnifiedMembership};
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

        membership.shares = new_shares;
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
    pub fn from_coop_member(
        member: &crate::coop_core::types::Member,
    ) -> Result<UnifiedMembership, MembershipError> {
        let member_id = EntityId::from_did(&member.did);
        let parent_id = EntityId::cooperative(&member.coop_id)
            .map_err(|e| MembershipError::InvalidCriteria(e.to_string()))?;

        let role = match member.role {
            crate::coop_core::types::MemberRole::Founder => MembershipRole::Founder,
            crate::coop_core::types::MemberRole::Member => MembershipRole::Member,
            crate::coop_core::types::MemberRole::Worker => MembershipRole::Worker,
            crate::coop_core::types::MemberRole::Consumer => MembershipRole::Consumer,
            crate::coop_core::types::MemberRole::Producer => MembershipRole::Producer,
            crate::coop_core::types::MemberRole::BoardMember => MembershipRole::BoardMember,
            // TODO: crate::coop_core::types::MemberRole::Officer doesn't carry a title;
            // use a generic placeholder until the source crate is extended.
            crate::coop_core::types::MemberRole::Officer => MembershipRole::Officer {
                title: "Officer".to_string(),
            },
        };

        let status = match member.status {
            crate::coop_core::types::MemberStatus::Pending => {
                crate::entity_core::membership::MembershipStatus::Pending
            }
            crate::coop_core::types::MemberStatus::Active => {
                crate::entity_core::membership::MembershipStatus::Active
            }
            crate::coop_core::types::MemberStatus::Suspended => {
                crate::entity_core::membership::MembershipStatus::Suspended
            }
            crate::coop_core::types::MemberStatus::Inactive => {
                crate::entity_core::membership::MembershipStatus::Inactive
            }
            crate::coop_core::types::MemberStatus::Removed => {
                crate::entity_core::membership::MembershipStatus::Removed
            }
        };

        Ok(UnifiedMembership {
            member_id,
            parent_id,
            role: role.clone(),
            status,
            // Clamp pre-1970 timestamps to 0 (icn-entity expects u64)
            joined_at: member.joined_at.timestamp().max(0) as u64,
            updated_at: icn_time::current_timestamp_secs(),
            shares: member.shares,
            capabilities: role.default_capabilities(),
            metadata: member.metadata.clone(),
            assignments: Vec::new(),
            labor_shares: Vec::new(),
            is_primary: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::MembershipConfig;
    use crate::entity_core::entity::EntityType;

    fn create_test_coop_config() -> CoopMembershipConfig {
        let entity = EntityConfig {
            id: EntityId::cooperative("test-coop").expect("Invalid coop ID"),
            name: "Test Coop".to_string(),
            entity_type: EntityType::Cooperative,
            status: crate::entity_core::entity::EntityStatus::Active,
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

        let member_id = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let coop_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");

        let membership = manager
            .add_coop_member(member_id, coop_id, MembershipRole::Worker, &config)
            .await
            .unwrap();

        assert!(matches!(
            membership.status,
            crate::entity_core::membership::MembershipStatus::Pending
        ));
    }

    #[test]
    fn test_update_shares() {
        let manager = CoopMembershipManager::new();
        let member_id = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let coop_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");

        let mut membership = UnifiedMembership::active(member_id, coop_id, MembershipRole::Worker);

        assert!(manager.update_shares(&mut membership, 100).is_ok());
        assert_eq!(membership.shares, 100);
    }

    #[test]
    fn test_labor_assignments() {
        let manager = CoopMembershipManager::new();
        let member_id = EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did());
        let coop_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");

        let mut membership = UnifiedMembership::active(member_id, coop_id, MembershipRole::Worker);

        manager.add_assignment(&mut membership, "assignment-1".to_string());
        assert_eq!(membership.assignments.len(), 1);

        manager.remove_assignment(&mut membership, "assignment-1");
        assert_eq!(membership.assignments.len(), 0);
    }
}
