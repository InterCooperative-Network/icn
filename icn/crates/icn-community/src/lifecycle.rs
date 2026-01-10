use crate::error::{CommunityError, Result};
use crate::types::{Community, CommunityStatus, CommunityType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationRequest {
    pub name: String,
    pub community_type: CommunityType,
    pub founding_members: Vec<String>,
    pub charter_ccl: String,
}

pub struct CommunityLifecycle {
    min_founders: usize,
}

impl CommunityLifecycle {
    pub fn new(min_founders: usize) -> Self {
        Self { min_founders }
    }

    pub fn form(&self, request: FormationRequest, governance_domain: String) -> Result<Community> {
        if request.founding_members.len() < self.min_founders {
            return Err(CommunityError::NotPermitted(format!(
                "Need {} founders, have {}",
                self.min_founders,
                request.founding_members.len()
            )));
        }

        let id = format!(
            "community:{}",
            request.name.replace(' ', "-").to_lowercase()
        );
        let mut community =
            Community::new(id, request.name, request.community_type, governance_domain);
        community.charter = request.charter_ccl;
        Ok(community)
    }

    pub fn activate(&self, community: &mut Community) -> Result<()> {
        if community.status != CommunityStatus::Forming {
            return Err(CommunityError::InvalidStatusTransition {
                from: community.status,
                to: CommunityStatus::Active,
            });
        }
        community.status = CommunityStatus::Active;
        community.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub fn dissolve(&self, community: &mut Community) -> Result<()> {
        community.status = CommunityStatus::Dissolved;
        community.updated_at = chrono::Utc::now();
        for member in community.members.values_mut() {
            member.active = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Member, MemberType};
    use chrono::Utc;

    fn create_formation_request(name: &str, founders: Vec<&str>) -> FormationRequest {
        FormationRequest {
            name: name.to_string(),
            community_type: CommunityType::Geographic,
            founding_members: founders.iter().map(|s| s.to_string()).collect(),
            charter_ccl: "charter://v1/test".to_string(),
        }
    }

    // === CommunityLifecycle creation tests ===

    #[test]
    fn test_lifecycle_new() {
        let lifecycle = CommunityLifecycle::new(3);
        assert_eq!(lifecycle.min_founders, 3);
    }

    #[test]
    fn test_lifecycle_new_zero_founders() {
        let lifecycle = CommunityLifecycle::new(0);
        assert_eq!(lifecycle.min_founders, 0);
    }

    // === FormationRequest tests ===

    #[test]
    fn test_formation_request_creation() {
        let request = FormationRequest {
            name: "Worker Collective".to_string(),
            community_type: CommunityType::Solidarity,
            founding_members: vec!["alice".to_string(), "bob".to_string()],
            charter_ccl: "charter://v1".to_string(),
        };

        assert_eq!(request.name, "Worker Collective");
        assert_eq!(request.community_type, CommunityType::Solidarity);
        assert_eq!(request.founding_members.len(), 2);
        assert_eq!(request.charter_ccl, "charter://v1");
    }

    #[test]
    fn test_formation_request_all_community_types() {
        for ctype in [
            CommunityType::Geographic,
            CommunityType::Interest,
            CommunityType::Solidarity,
            CommunityType::Ecosystem,
        ] {
            let request = FormationRequest {
                name: "Test".to_string(),
                community_type: ctype,
                founding_members: vec!["founder".to_string()],
                charter_ccl: "charter".to_string(),
            };

            assert_eq!(request.community_type, ctype);
        }
    }

    #[test]
    fn test_formation_request_serialization() {
        let request = FormationRequest {
            name: "Test Community".to_string(),
            community_type: CommunityType::Interest,
            founding_members: vec!["did:icn:alice".to_string(), "did:icn:bob".to_string()],
            charter_ccl: "charter://test".to_string(),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: FormationRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.name, request.name);
        assert_eq!(deserialized.community_type, request.community_type);
        assert_eq!(deserialized.founding_members, request.founding_members);
        assert_eq!(deserialized.charter_ccl, request.charter_ccl);
    }

    // === form() tests ===

    #[test]
    fn test_form_basic() {
        let lifecycle = CommunityLifecycle::new(2);
        let request = create_formation_request("Worker Coop", vec!["alice", "bob"]);

        let community = lifecycle.form(request, "domain-1".to_string()).unwrap();

        assert_eq!(community.id, "community:worker-coop");
        assert_eq!(community.name, "Worker Coop");
        assert_eq!(community.community_type, CommunityType::Geographic);
        assert_eq!(community.status, CommunityStatus::Forming);
        assert_eq!(community.charter, "charter://v1/test");
        assert_eq!(community.governance_domain, "domain-1");
    }

    #[test]
    fn test_form_generates_id_from_name() {
        let lifecycle = CommunityLifecycle::new(1);

        // Test various name formats
        let test_cases = vec![
            ("Simple", "community:simple"),
            ("Two Words", "community:two-words"),
            ("Multiple Word Name", "community:multiple-word-name"),
            ("UPPERCASE", "community:uppercase"),
            ("MixedCase Name", "community:mixedcase-name"),
        ];

        for (name, expected_id) in test_cases {
            let request = create_formation_request(name, vec!["founder"]);
            let community = lifecycle.form(request, "domain".to_string()).unwrap();
            assert_eq!(community.id, expected_id, "Failed for name: {name}");
        }
    }

    #[test]
    fn test_form_insufficient_founders() {
        let lifecycle = CommunityLifecycle::new(3);
        let request = create_formation_request("Test", vec!["alice", "bob"]); // Only 2

        let result = lifecycle.form(request, "domain".to_string());

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::NotPermitted(msg) => {
                assert!(msg.contains("3"));
                assert!(msg.contains("2"));
            }
            _ => panic!("Expected NotPermitted error"),
        }
    }

    #[test]
    fn test_form_exact_minimum_founders() {
        let lifecycle = CommunityLifecycle::new(3);
        let request = create_formation_request("Test", vec!["a", "b", "c"]); // Exactly 3

        let community = lifecycle.form(request, "domain".to_string()).unwrap();
        assert_eq!(community.status, CommunityStatus::Forming);
    }

    #[test]
    fn test_form_more_than_minimum_founders() {
        let lifecycle = CommunityLifecycle::new(2);
        let request = create_formation_request("Test", vec!["a", "b", "c", "d", "e"]); // 5

        let community = lifecycle.form(request, "domain".to_string()).unwrap();
        assert_eq!(community.status, CommunityStatus::Forming);
    }

    #[test]
    fn test_form_zero_founder_requirement() {
        let lifecycle = CommunityLifecycle::new(0);
        let request = create_formation_request("Test", vec![]); // No founders

        let community = lifecycle.form(request, "domain".to_string()).unwrap();
        assert_eq!(community.status, CommunityStatus::Forming);
    }

    #[test]
    fn test_form_sets_charter() {
        let lifecycle = CommunityLifecycle::new(1);
        let mut request = create_formation_request("Test", vec!["founder"]);
        request.charter_ccl = "custom://charter/agreement".to_string();

        let community = lifecycle.form(request, "domain".to_string()).unwrap();
        assert_eq!(community.charter, "custom://charter/agreement");
    }

    // === activate() tests ===

    #[test]
    fn test_activate_from_forming() {
        let lifecycle = CommunityLifecycle::new(1);
        let request = create_formation_request("Test", vec!["founder"]);
        let mut community = lifecycle.form(request, "domain".to_string()).unwrap();

        assert_eq!(community.status, CommunityStatus::Forming);

        let original_updated_at = community.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));

        lifecycle.activate(&mut community).unwrap();

        assert_eq!(community.status, CommunityStatus::Active);
        assert!(community.updated_at > original_updated_at);
    }

    #[test]
    fn test_activate_from_active_fails() {
        let lifecycle = CommunityLifecycle::new(1);
        let request = create_formation_request("Test", vec!["founder"]);
        let mut community = lifecycle.form(request, "domain".to_string()).unwrap();

        lifecycle.activate(&mut community).unwrap();
        assert_eq!(community.status, CommunityStatus::Active);

        // Try to activate again
        let result = lifecycle.activate(&mut community);

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::InvalidStatusTransition { from, to } => {
                assert_eq!(from, CommunityStatus::Active);
                assert_eq!(to, CommunityStatus::Active);
            }
            _ => panic!("Expected InvalidStatusTransition error"),
        }
    }

    #[test]
    fn test_activate_from_suspended_fails() {
        let lifecycle = CommunityLifecycle::new(1);
        let mut community = Community::new(
            "test".to_string(),
            "Test".to_string(),
            CommunityType::Geographic,
            "domain".to_string(),
        );
        community.status = CommunityStatus::Suspended;

        let result = lifecycle.activate(&mut community);

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::InvalidStatusTransition { from, to } => {
                assert_eq!(from, CommunityStatus::Suspended);
                assert_eq!(to, CommunityStatus::Active);
            }
            _ => panic!("Expected InvalidStatusTransition error"),
        }
    }

    #[test]
    fn test_activate_from_dissolved_fails() {
        let lifecycle = CommunityLifecycle::new(1);
        let mut community = Community::new(
            "test".to_string(),
            "Test".to_string(),
            CommunityType::Geographic,
            "domain".to_string(),
        );
        community.status = CommunityStatus::Dissolved;

        let result = lifecycle.activate(&mut community);

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::InvalidStatusTransition { from, to } => {
                assert_eq!(from, CommunityStatus::Dissolved);
                assert_eq!(to, CommunityStatus::Active);
            }
            _ => panic!("Expected InvalidStatusTransition error"),
        }
    }

    // === dissolve() tests ===

    #[test]
    fn test_dissolve_from_forming() {
        let lifecycle = CommunityLifecycle::new(1);
        let request = create_formation_request("Test", vec!["founder"]);
        let mut community = lifecycle.form(request, "domain".to_string()).unwrap();

        let original_updated_at = community.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));

        lifecycle.dissolve(&mut community).unwrap();

        assert_eq!(community.status, CommunityStatus::Dissolved);
        assert!(community.updated_at > original_updated_at);
    }

    #[test]
    fn test_dissolve_from_active() {
        let lifecycle = CommunityLifecycle::new(1);
        let request = create_formation_request("Test", vec!["founder"]);
        let mut community = lifecycle.form(request, "domain".to_string()).unwrap();
        lifecycle.activate(&mut community).unwrap();

        lifecycle.dissolve(&mut community).unwrap();

        assert_eq!(community.status, CommunityStatus::Dissolved);
    }

    #[test]
    fn test_dissolve_deactivates_all_members() {
        let lifecycle = CommunityLifecycle::new(1);
        let request = create_formation_request("Test", vec!["founder"]);
        let mut community = lifecycle.form(request, "domain".to_string()).unwrap();

        // Add some members
        for i in 1..=5 {
            community.members.insert(
                format!("member-{i}"),
                Member {
                    id: format!("member-{i}"),
                    member_type: MemberType::Individual(format!("did:icn:user{i}")),
                    joined_at: Utc::now(),
                    voting_weight: 1,
                    active: true,
                },
            );
        }

        assert_eq!(community.active_member_count(), 5);

        lifecycle.dissolve(&mut community).unwrap();

        // All members should be inactive
        assert_eq!(community.active_member_count(), 0);
        assert_eq!(community.total_voting_weight(), 0);

        for member in community.members.values() {
            assert!(!member.active, "Member {} should be inactive", member.id);
        }
    }

    #[test]
    fn test_dissolve_already_dissolved() {
        let lifecycle = CommunityLifecycle::new(1);
        let mut community = Community::new(
            "test".to_string(),
            "Test".to_string(),
            CommunityType::Geographic,
            "domain".to_string(),
        );
        community.status = CommunityStatus::Dissolved;

        // Dissolving again should still work (idempotent)
        lifecycle.dissolve(&mut community).unwrap();
        assert_eq!(community.status, CommunityStatus::Dissolved);
    }

    #[test]
    fn test_dissolve_from_suspended() {
        let lifecycle = CommunityLifecycle::new(1);
        let mut community = Community::new(
            "test".to_string(),
            "Test".to_string(),
            CommunityType::Geographic,
            "domain".to_string(),
        );
        community.status = CommunityStatus::Suspended;

        lifecycle.dissolve(&mut community).unwrap();
        assert_eq!(community.status, CommunityStatus::Dissolved);
    }

    // === Full lifecycle workflow tests ===

    #[test]
    fn test_full_lifecycle_form_activate_dissolve() {
        let lifecycle = CommunityLifecycle::new(2);
        let request = FormationRequest {
            name: "Worker Collective".to_string(),
            community_type: CommunityType::Solidarity,
            founding_members: vec!["alice".to_string(), "bob".to_string()],
            charter_ccl: "cooperative://charter/v1".to_string(),
        };

        // Form
        let mut community = lifecycle.form(request, "domain".to_string()).unwrap();
        assert_eq!(community.status, CommunityStatus::Forming);

        // Add members
        community.members.insert(
            "alice".to_string(),
            Member {
                id: "alice".to_string(),
                member_type: MemberType::Individual("did:icn:alice".to_string()),
                joined_at: Utc::now(),
                voting_weight: 1,
                active: true,
            },
        );
        community.members.insert(
            "bob".to_string(),
            Member {
                id: "bob".to_string(),
                member_type: MemberType::Individual("did:icn:bob".to_string()),
                joined_at: Utc::now(),
                voting_weight: 1,
                active: true,
            },
        );

        assert_eq!(community.active_member_count(), 2);

        // Activate
        lifecycle.activate(&mut community).unwrap();
        assert_eq!(community.status, CommunityStatus::Active);
        assert_eq!(community.active_member_count(), 2);

        // Dissolve
        lifecycle.dissolve(&mut community).unwrap();
        assert_eq!(community.status, CommunityStatus::Dissolved);
        assert_eq!(community.active_member_count(), 0);
    }

    #[test]
    fn test_lifecycle_preserves_community_data() {
        let lifecycle = CommunityLifecycle::new(1);
        let request = FormationRequest {
            name: "Data Test".to_string(),
            community_type: CommunityType::Ecosystem,
            founding_members: vec!["founder".to_string()],
            charter_ccl: "test-charter".to_string(),
        };

        let mut community = lifecycle.form(request, "domain-1".to_string()).unwrap();

        // Add some data
        community
            .metadata
            .insert("key1".to_string(), "value1".to_string());
        community.resource_pools.insert(
            "compute".to_string(),
            crate::types::ResourcePool {
                name: "compute".to_string(),
                resource_type: "cpu".to_string(),
                total_capacity: 100,
                allocated: 0,
                unit: "cores".to_string(),
            },
        );

        // Activate
        lifecycle.activate(&mut community).unwrap();

        // Data should be preserved
        assert_eq!(community.metadata.get("key1").unwrap(), "value1");
        assert!(community.resource_pools.contains_key("compute"));
        assert_eq!(community.charter, "test-charter");
        assert_eq!(community.governance_domain, "domain-1");

        // Dissolve
        lifecycle.dissolve(&mut community).unwrap();

        // Data should still be preserved
        assert_eq!(community.metadata.get("key1").unwrap(), "value1");
        assert!(community.resource_pools.contains_key("compute"));
    }
}
