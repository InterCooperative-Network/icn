use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a cooperative
pub type CooperativeId = String;

/// Types of cooperatives supported
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CooperativeType {
    /// Worker-owned cooperative
    Worker,
    /// Consumer-owned cooperative
    Consumer,
    /// Producer cooperative
    Producer,
    /// Multi-stakeholder cooperative
    MultiStakeholder,
    /// Platform cooperative
    Platform,
    /// Housing cooperative
    Housing,
    /// Credit union
    CreditUnion,
}

/// Lifecycle status of a cooperative
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CooperativeStatus {
    /// Formation in progress (gathering founders)
    Forming,
    /// Active and operational
    Active,
    /// Temporarily suspended
    Suspended,
    /// Dissolution in progress
    Dissolving,
    /// Fully dissolved
    Dissolved,
}

/// Membership tier with associated rights and responsibilities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipTier {
    pub name: String,
    pub voting_weight: u32,
    pub profit_share_weight: u32,
    pub governance_rights: Vec<String>,
}

/// Member information within a cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub did: String,
    pub joined_at: DateTime<Utc>,
    pub tier: MembershipTier,
    pub capital_contribution: u64,
    pub active: bool,
}

/// Core cooperative structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cooperative {
    pub id: CooperativeId,
    pub name: String,
    pub coop_type: CooperativeType,
    pub status: CooperativeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Governance domain ID (links to icn-governance)
    pub governance_domain: String,

    /// Members and their details
    pub members: HashMap<String, Member>,

    /// Bylaws and governing documents (CCL contract IDs)
    pub bylaws: Vec<String>,

    /// Minimum members required for operations
    pub min_members: usize,

    /// Membership tiers available
    pub tiers: Vec<MembershipTier>,

    /// Total capital pool
    pub capital_pool: u64,

    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl Cooperative {
    pub fn new(
        id: CooperativeId,
        name: String,
        coop_type: CooperativeType,
        governance_domain: String,
        min_members: usize,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            coop_type,
            status: CooperativeStatus::Forming,
            created_at: now,
            updated_at: now,
            governance_domain,
            members: HashMap::new(),
            bylaws: Vec::new(),
            min_members,
            tiers: Vec::new(),
            capital_pool: 0,
            metadata: HashMap::new(),
        }
    }

    pub fn member_count(&self) -> usize {
        self.members.values().filter(|m| m.active).count()
    }

    pub fn can_activate(&self) -> bool {
        self.status == CooperativeStatus::Forming && self.member_count() >= self.min_members
    }

    pub fn total_voting_weight(&self) -> u32 {
        self.members
            .values()
            .filter(|m| m.active)
            .map(|m| m.tier.voting_weight)
            .sum()
    }
}
