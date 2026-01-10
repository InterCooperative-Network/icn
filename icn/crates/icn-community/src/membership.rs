use crate::error::{CommunityError, Result};
use crate::types::{Community, Member, MemberId, MemberType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberApplication {
    pub community_id: String,
    pub applicant_id: MemberId,
    pub member_type: MemberType,
    pub statement: String,
}

pub struct MembershipManager;

impl MembershipManager {
    pub fn new() -> Self {
        Self
    }

    pub fn add_member(
        &self,
        community: &mut Community,
        member_id: MemberId,
        member_type: MemberType,
        voting_weight: u32,
    ) -> Result<()> {
        // Check for duplicate membership to prevent data overwrite
        if community.members.contains_key(&member_id) {
            return Err(CommunityError::MemberAlreadyExists(member_id));
        }

        let member = Member {
            id: member_id.clone(),
            member_type,
            joined_at: chrono::Utc::now(),
            voting_weight,
            active: true,
        };
        community.members.insert(member_id, member);
        community.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub fn remove_member(&self, community: &mut Community, member_id: &str) -> Result<()> {
        let member = community
            .members
            .get_mut(member_id)
            .ok_or_else(|| CommunityError::MemberNotFound(member_id.to_string()))?;
        member.active = false;
        community.updated_at = chrono::Utc::now();
        Ok(())
    }
}

impl Default for MembershipManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CommunityStatus, CommunityType};

    fn create_test_community() -> Community {
        Community::new(
            "test-comm".to_string(),
            "Test Community".to_string(),
            CommunityType::Solidarity,
            "test-domain".to_string(),
        )
    }

    // === MembershipManager creation tests ===

    #[test]
    fn test_membership_manager_new() {
        let _manager = MembershipManager::new();
    }

    #[test]
    fn test_membership_manager_default() {
        let _manager: MembershipManager = Default::default();
    }

    // === MemberApplication tests ===

    #[test]
    fn test_member_application_creation() {
        let application = MemberApplication {
            community_id: "comm-1".to_string(),
            applicant_id: "applicant-1".to_string(),
            member_type: MemberType::Individual("did:icn:abc123".to_string()),
            statement: "I want to join this community".to_string(),
        };

        assert_eq!(application.community_id, "comm-1");
        assert_eq!(application.applicant_id, "applicant-1");
        assert_eq!(application.statement, "I want to join this community");
    }

    #[test]
    fn test_member_application_with_cooperative() {
        let application = MemberApplication {
            community_id: "comm-1".to_string(),
            applicant_id: "coop-xyz".to_string(),
            member_type: MemberType::Cooperative("coop:worker-collective".to_string()),
            statement: "Our cooperative seeks to participate".to_string(),
        };

        if let MemberType::Cooperative(id) = &application.member_type {
            assert_eq!(id, "coop:worker-collective");
        } else {
            panic!("Expected Cooperative member type");
        }
    }

    #[test]
    fn test_member_application_serialization() {
        let application = MemberApplication {
            community_id: "comm-1".to_string(),
            applicant_id: "member-1".to_string(),
            member_type: MemberType::Individual("did:icn:test".to_string()),
            statement: "Test statement".to_string(),
        };

        let serialized = serde_json::to_string(&application).unwrap();
        let deserialized: MemberApplication = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.community_id, application.community_id);
        assert_eq!(deserialized.applicant_id, application.applicant_id);
        assert_eq!(deserialized.statement, application.statement);
    }

    // === add_member tests ===

    #[test]
    fn test_add_member_individual() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();
        let original_updated_at = community.updated_at;

        manager
            .add_member(
                &mut community,
                "member-1".to_string(),
                MemberType::Individual("did:icn:alice".to_string()),
                1,
            )
            .unwrap();

        assert_eq!(community.members.len(), 1);
        let member = community.members.get("member-1").unwrap();
        assert_eq!(member.id, "member-1");
        assert_eq!(member.voting_weight, 1);
        assert!(member.active);
        assert!(community.updated_at >= original_updated_at);
    }

    #[test]
    fn test_add_member_cooperative() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        manager
            .add_member(
                &mut community,
                "coop-member".to_string(),
                MemberType::Cooperative("coop:workers".to_string()),
                5,
            )
            .unwrap();

        let member = community.members.get("coop-member").unwrap();
        if let MemberType::Cooperative(id) = &member.member_type {
            assert_eq!(id, "coop:workers");
        } else {
            panic!("Expected Cooperative member type");
        }
        assert_eq!(member.voting_weight, 5);
    }

    #[test]
    fn test_add_multiple_members() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        for i in 1..=5 {
            manager
                .add_member(
                    &mut community,
                    format!("member-{i}"),
                    MemberType::Individual(format!("did:icn:user{i}")),
                    i,
                )
                .unwrap();
        }

        assert_eq!(community.members.len(), 5);
        assert_eq!(community.active_member_count(), 5);
        assert_eq!(community.total_voting_weight(), 15); // 1+2+3+4+5
    }

    #[test]
    fn test_add_member_duplicate_fails() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        manager
            .add_member(
                &mut community,
                "member-1".to_string(),
                MemberType::Individual("did:icn:alice".to_string()),
                1,
            )
            .unwrap();

        // Try to add same member again
        let result = manager.add_member(
            &mut community,
            "member-1".to_string(),
            MemberType::Individual("did:icn:bob".to_string()),
            2,
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::MemberAlreadyExists(id) => {
                assert_eq!(id, "member-1");
            }
            _ => panic!("Expected MemberAlreadyExists error"),
        }

        // Original member should be unchanged
        let member = community.members.get("member-1").unwrap();
        if let MemberType::Individual(did) = &member.member_type {
            assert_eq!(did, "did:icn:alice");
        }
        assert_eq!(member.voting_weight, 1);
    }

    #[test]
    fn test_add_member_with_zero_voting_weight() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        manager
            .add_member(
                &mut community,
                "observer".to_string(),
                MemberType::Individual("did:icn:observer".to_string()),
                0,
            )
            .unwrap();

        let member = community.members.get("observer").unwrap();
        assert_eq!(member.voting_weight, 0);
        assert_eq!(community.total_voting_weight(), 0);
    }

    #[test]
    fn test_add_member_with_high_voting_weight() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        manager
            .add_member(
                &mut community,
                "major-coop".to_string(),
                MemberType::Cooperative("coop:large".to_string()),
                1000,
            )
            .unwrap();

        let member = community.members.get("major-coop").unwrap();
        assert_eq!(member.voting_weight, 1000);
    }

    // === remove_member tests ===

    #[test]
    fn test_remove_member_deactivates() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        manager
            .add_member(
                &mut community,
                "member-1".to_string(),
                MemberType::Individual("did:icn:alice".to_string()),
                1,
            )
            .unwrap();

        assert!(community.members.get("member-1").unwrap().active);

        let original_updated_at = community.updated_at;

        manager.remove_member(&mut community, "member-1").unwrap();

        // Member should still exist but be inactive
        let member = community.members.get("member-1").unwrap();
        assert!(!member.active);
        assert!(community.updated_at >= original_updated_at);
    }

    #[test]
    fn test_remove_member_affects_active_count() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        manager
            .add_member(
                &mut community,
                "member-1".to_string(),
                MemberType::Individual("did:icn:alice".to_string()),
                1,
            )
            .unwrap();

        manager
            .add_member(
                &mut community,
                "member-2".to_string(),
                MemberType::Individual("did:icn:bob".to_string()),
                2,
            )
            .unwrap();

        assert_eq!(community.active_member_count(), 2);
        assert_eq!(community.total_voting_weight(), 3);

        manager.remove_member(&mut community, "member-1").unwrap();

        assert_eq!(community.active_member_count(), 1);
        assert_eq!(community.total_voting_weight(), 2); // Only member-2's weight
    }

    #[test]
    fn test_remove_nonexistent_member_fails() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        let result = manager.remove_member(&mut community, "nonexistent");

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::MemberNotFound(id) => {
                assert_eq!(id, "nonexistent");
            }
            _ => panic!("Expected MemberNotFound error"),
        }
    }

    #[test]
    fn test_remove_already_inactive_member() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        manager
            .add_member(
                &mut community,
                "member-1".to_string(),
                MemberType::Individual("did:icn:alice".to_string()),
                1,
            )
            .unwrap();

        // Remove twice
        manager.remove_member(&mut community, "member-1").unwrap();
        manager.remove_member(&mut community, "member-1").unwrap(); // Should still work

        assert!(!community.members.get("member-1").unwrap().active);
    }

    // === Workflow tests ===

    #[test]
    fn test_add_remove_add_workflow() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        // Add member
        manager
            .add_member(
                &mut community,
                "member-1".to_string(),
                MemberType::Individual("did:icn:alice".to_string()),
                1,
            )
            .unwrap();

        assert_eq!(community.active_member_count(), 1);

        // Remove (deactivate) member
        manager.remove_member(&mut community, "member-1").unwrap();
        assert_eq!(community.active_member_count(), 0);

        // Cannot add with same ID again (still exists, just inactive)
        let result = manager.add_member(
            &mut community,
            "member-1".to_string(),
            MemberType::Individual("did:icn:alice".to_string()),
            1,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_mixed_member_types() {
        let manager = MembershipManager::new();
        let mut community = create_test_community();

        // Add individuals
        manager
            .add_member(
                &mut community,
                "individual-1".to_string(),
                MemberType::Individual("did:icn:alice".to_string()),
                1,
            )
            .unwrap();

        manager
            .add_member(
                &mut community,
                "individual-2".to_string(),
                MemberType::Individual("did:icn:bob".to_string()),
                1,
            )
            .unwrap();

        // Add cooperatives
        manager
            .add_member(
                &mut community,
                "coop-1".to_string(),
                MemberType::Cooperative("coop:workers".to_string()),
                5,
            )
            .unwrap();

        manager
            .add_member(
                &mut community,
                "coop-2".to_string(),
                MemberType::Cooperative("coop:farmers".to_string()),
                3,
            )
            .unwrap();

        assert_eq!(community.members.len(), 4);
        assert_eq!(community.total_voting_weight(), 10); // 1+1+5+3
    }

    // === Community status tests ===

    #[test]
    fn test_operations_on_different_community_statuses() {
        let manager = MembershipManager::new();

        for status in [
            CommunityStatus::Forming,
            CommunityStatus::Active,
            CommunityStatus::Suspended,
            CommunityStatus::Dissolved,
        ] {
            let mut community = create_test_community();
            community.status = status;

            manager
                .add_member(
                    &mut community,
                    "member-1".to_string(),
                    MemberType::Individual("did:icn:test".to_string()),
                    1,
                )
                .unwrap();

            manager.remove_member(&mut community, "member-1").unwrap();

            assert!(!community.members.get("member-1").unwrap().active);
        }
    }
}
