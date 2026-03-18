use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type CommunityId = String;
pub type MemberId = String; // Can be DID or CooperativeId

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommunityType {
    Geographic, // Location-based community
    Interest,   // Shared interest/profession
    Solidarity, // Mutual aid network
    Ecosystem,  // Full cooperative ecosystem
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommunityStatus {
    Forming,
    Active,
    Suspended,
    Dissolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemberType {
    Individual(String),  // DID
    Cooperative(String), // CooperativeId
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub id: MemberId,
    pub member_type: MemberType,
    pub joined_at: DateTime<Utc>,
    pub voting_weight: u32,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePool {
    pub name: String,
    pub resource_type: String, // "compute", "storage", "credit", etc.
    pub total_capacity: u64,
    pub allocated: u64,
    pub unit: String, // "MB", "GB", "credits", etc.
}

impl ResourcePool {
    pub fn available(&self) -> u64 {
        self.total_capacity.saturating_sub(self.allocated)
    }

    pub fn can_allocate(&self, amount: u64) -> bool {
        self.available() >= amount
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub id: CommunityId,
    pub name: String,
    pub community_type: CommunityType,
    pub status: CommunityStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    pub governance_domain: String,
    pub members: HashMap<MemberId, Member>,
    pub resource_pools: HashMap<String, ResourcePool>,

    pub charter: String, // CCL contract defining community rules
    pub metadata: HashMap<String, String>,
}

impl Community {
    pub fn new(
        id: CommunityId,
        name: String,
        community_type: CommunityType,
        governance_domain: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            community_type,
            status: CommunityStatus::Forming,
            created_at: now,
            updated_at: now,
            governance_domain,
            members: HashMap::new(),
            resource_pools: HashMap::new(),
            charter: String::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn active_member_count(&self) -> usize {
        self.members.values().filter(|m| m.active).count()
    }

    pub fn total_voting_weight(&self) -> u32 {
        self.members
            .values()
            .filter(|m| m.active)
            .map(|m| m.voting_weight)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === ResourcePool tests ===

    #[test]
    fn test_resource_pool_available_full_capacity() {
        let pool = ResourcePool {
            name: "compute".to_string(),
            resource_type: "cpu".to_string(),
            total_capacity: 100,
            allocated: 0,
            unit: "cores".to_string(),
        };
        assert_eq!(pool.available(), 100);
    }

    #[test]
    fn test_resource_pool_available_partial() {
        let pool = ResourcePool {
            name: "storage".to_string(),
            resource_type: "disk".to_string(),
            total_capacity: 1000,
            allocated: 350,
            unit: "GB".to_string(),
        };
        assert_eq!(pool.available(), 650);
    }

    #[test]
    fn test_resource_pool_available_fully_allocated() {
        let pool = ResourcePool {
            name: "credits".to_string(),
            resource_type: "mutual_credit".to_string(),
            total_capacity: 500,
            allocated: 500,
            unit: "credits".to_string(),
        };
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn test_resource_pool_available_over_allocated_saturates() {
        // Edge case: allocated > total_capacity (shouldn't happen, but saturating_sub handles it)
        let pool = ResourcePool {
            name: "test".to_string(),
            resource_type: "test".to_string(),
            total_capacity: 100,
            allocated: 150,
            unit: "units".to_string(),
        };
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn test_resource_pool_can_allocate_sufficient() {
        let pool = ResourcePool {
            name: "compute".to_string(),
            resource_type: "cpu".to_string(),
            total_capacity: 100,
            allocated: 30,
            unit: "cores".to_string(),
        };
        assert!(pool.can_allocate(50));
        assert!(pool.can_allocate(70));
    }

    #[test]
    fn test_resource_pool_can_allocate_exact() {
        let pool = ResourcePool {
            name: "compute".to_string(),
            resource_type: "cpu".to_string(),
            total_capacity: 100,
            allocated: 30,
            unit: "cores".to_string(),
        };
        assert!(pool.can_allocate(70)); // Exactly available amount
    }

    #[test]
    fn test_resource_pool_cannot_allocate_insufficient() {
        let pool = ResourcePool {
            name: "storage".to_string(),
            resource_type: "disk".to_string(),
            total_capacity: 100,
            allocated: 80,
            unit: "GB".to_string(),
        };
        assert!(!pool.can_allocate(25)); // Need 25, only 20 available
        assert!(!pool.can_allocate(100));
    }

    #[test]
    fn test_resource_pool_can_allocate_zero() {
        let pool = ResourcePool {
            name: "test".to_string(),
            resource_type: "test".to_string(),
            total_capacity: 0,
            allocated: 0,
            unit: "units".to_string(),
        };
        assert!(pool.can_allocate(0));
        assert!(!pool.can_allocate(1));
    }

    // === Community tests ===

    #[test]
    fn test_community_new() {
        let community = Community::new(
            "comm-1".to_string(),
            "Test Community".to_string(),
            CommunityType::Geographic,
            "domain-1".to_string(),
        );

        assert_eq!(community.id, "comm-1");
        assert_eq!(community.name, "Test Community");
        assert_eq!(community.community_type, CommunityType::Geographic);
        assert_eq!(community.status, CommunityStatus::Forming);
        assert_eq!(community.governance_domain, "domain-1");
        assert!(community.members.is_empty());
        assert!(community.resource_pools.is_empty());
        assert!(community.charter.is_empty());
        assert!(community.metadata.is_empty());
    }

    #[test]
    fn test_community_new_all_types() {
        for community_type in [
            CommunityType::Geographic,
            CommunityType::Interest,
            CommunityType::Solidarity,
            CommunityType::Ecosystem,
        ] {
            let community = Community::new(
                format!("comm-{community_type:?}"),
                "Test".to_string(),
                community_type,
                "domain".to_string(),
            );
            assert_eq!(community.community_type, community_type);
            assert_eq!(community.status, CommunityStatus::Forming);
        }
    }

    #[test]
    fn test_community_active_member_count_empty() {
        let community = Community::new(
            "comm".to_string(),
            "Test".to_string(),
            CommunityType::Geographic,
            "domain".to_string(),
        );
        assert_eq!(community.active_member_count(), 0);
    }

    #[test]
    fn test_community_active_member_count_all_active() {
        let mut community = Community::new(
            "comm".to_string(),
            "Test".to_string(),
            CommunityType::Geographic,
            "domain".to_string(),
        );

        for i in 1..=5 {
            community.members.insert(
                format!("member-{i}"),
                Member {
                    id: format!("member-{i}"),
                    member_type: MemberType::Individual(format!("did:icn:{i}")),
                    joined_at: Utc::now(),
                    voting_weight: 1,
                    active: true,
                },
            );
        }

        assert_eq!(community.active_member_count(), 5);
    }

    #[test]
    fn test_community_active_member_count_mixed() {
        let mut community = Community::new(
            "comm".to_string(),
            "Test".to_string(),
            CommunityType::Interest,
            "domain".to_string(),
        );

        // Add 3 active members
        for i in 1..=3 {
            community.members.insert(
                format!("active-{i}"),
                Member {
                    id: format!("active-{i}"),
                    member_type: MemberType::Individual(format!("did:icn:active{i}")),
                    joined_at: Utc::now(),
                    voting_weight: 1,
                    active: true,
                },
            );
        }

        // Add 2 inactive members
        for i in 1..=2 {
            community.members.insert(
                format!("inactive-{i}"),
                Member {
                    id: format!("inactive-{i}"),
                    member_type: MemberType::Individual(format!("did:icn:inactive{i}")),
                    joined_at: Utc::now(),
                    voting_weight: 1,
                    active: false,
                },
            );
        }

        assert_eq!(community.active_member_count(), 3);
    }

    #[test]
    fn test_community_total_voting_weight_empty() {
        let community = Community::new(
            "comm".to_string(),
            "Test".to_string(),
            CommunityType::Solidarity,
            "domain".to_string(),
        );
        assert_eq!(community.total_voting_weight(), 0);
    }

    #[test]
    fn test_community_total_voting_weight_equal_weights() {
        let mut community = Community::new(
            "comm".to_string(),
            "Test".to_string(),
            CommunityType::Ecosystem,
            "domain".to_string(),
        );

        for i in 1..=4 {
            community.members.insert(
                format!("member-{i}"),
                Member {
                    id: format!("member-{i}"),
                    member_type: MemberType::Cooperative(format!("coop-{i}")),
                    joined_at: Utc::now(),
                    voting_weight: 1,
                    active: true,
                },
            );
        }

        assert_eq!(community.total_voting_weight(), 4);
    }

    #[test]
    fn test_community_total_voting_weight_varied_weights() {
        let mut community = Community::new(
            "comm".to_string(),
            "Test".to_string(),
            CommunityType::Ecosystem,
            "domain".to_string(),
        );

        // Add members with different voting weights
        community.members.insert(
            "founder".to_string(),
            Member {
                id: "founder".to_string(),
                member_type: MemberType::Individual("did:icn:founder".to_string()),
                joined_at: Utc::now(),
                voting_weight: 3,
                active: true,
            },
        );
        community.members.insert(
            "large-coop".to_string(),
            Member {
                id: "large-coop".to_string(),
                member_type: MemberType::Cooperative("coop-large".to_string()),
                joined_at: Utc::now(),
                voting_weight: 5,
                active: true,
            },
        );
        community.members.insert(
            "small-coop".to_string(),
            Member {
                id: "small-coop".to_string(),
                member_type: MemberType::Cooperative("coop-small".to_string()),
                joined_at: Utc::now(),
                voting_weight: 2,
                active: true,
            },
        );

        assert_eq!(community.total_voting_weight(), 10); // 3 + 5 + 2
    }

    #[test]
    fn test_community_total_voting_weight_excludes_inactive() {
        let mut community = Community::new(
            "comm".to_string(),
            "Test".to_string(),
            CommunityType::Interest,
            "domain".to_string(),
        );

        // Active member with weight 5
        community.members.insert(
            "active".to_string(),
            Member {
                id: "active".to_string(),
                member_type: MemberType::Individual("did:icn:active".to_string()),
                joined_at: Utc::now(),
                voting_weight: 5,
                active: true,
            },
        );

        // Inactive member with weight 10 (should NOT count)
        community.members.insert(
            "inactive".to_string(),
            Member {
                id: "inactive".to_string(),
                member_type: MemberType::Individual("did:icn:inactive".to_string()),
                joined_at: Utc::now(),
                voting_weight: 10,
                active: false,
            },
        );

        assert_eq!(community.total_voting_weight(), 5); // Only active member counts
    }

    // === Member and MemberType tests ===

    #[test]
    fn test_member_type_individual() {
        let member_type = MemberType::Individual("did:icn:abc123".to_string());
        if let MemberType::Individual(did) = member_type {
            assert_eq!(did, "did:icn:abc123");
        } else {
            panic!("Expected Individual variant");
        }
    }

    #[test]
    fn test_member_type_cooperative() {
        let member_type = MemberType::Cooperative("coop:worker-collective".to_string());
        if let MemberType::Cooperative(coop_id) = member_type {
            assert_eq!(coop_id, "coop:worker-collective");
        } else {
            panic!("Expected Cooperative variant");
        }
    }

    #[test]
    fn test_member_creation() {
        let now = Utc::now();
        let member = Member {
            id: "member-1".to_string(),
            member_type: MemberType::Individual("did:icn:test".to_string()),
            joined_at: now,
            voting_weight: 2,
            active: true,
        };

        assert_eq!(member.id, "member-1");
        assert_eq!(member.voting_weight, 2);
        assert!(member.active);
    }

    // === CommunityStatus tests ===

    #[test]
    fn test_community_status_equality() {
        assert_eq!(CommunityStatus::Forming, CommunityStatus::Forming);
        assert_eq!(CommunityStatus::Active, CommunityStatus::Active);
        assert_eq!(CommunityStatus::Suspended, CommunityStatus::Suspended);
        assert_eq!(CommunityStatus::Dissolved, CommunityStatus::Dissolved);

        assert_ne!(CommunityStatus::Forming, CommunityStatus::Active);
        assert_ne!(CommunityStatus::Active, CommunityStatus::Dissolved);
    }

    // === CommunityType tests ===

    #[test]
    fn test_community_type_equality() {
        assert_eq!(CommunityType::Geographic, CommunityType::Geographic);
        assert_eq!(CommunityType::Interest, CommunityType::Interest);
        assert_eq!(CommunityType::Solidarity, CommunityType::Solidarity);
        assert_eq!(CommunityType::Ecosystem, CommunityType::Ecosystem);

        assert_ne!(CommunityType::Geographic, CommunityType::Interest);
    }
}
