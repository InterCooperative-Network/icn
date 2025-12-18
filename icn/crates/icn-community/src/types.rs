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
