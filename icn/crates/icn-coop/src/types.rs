use serde::{Deserialize, Serialize};
use icn_identity::Did;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cooperative {
    pub id: String,
    pub name: String,
    pub coop_type: CoopType,
    pub status: CoopStatus,
    pub domain_id: Option<String>,
    pub charter_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CoopType {
    Worker,
    Consumer,
    Producer,
    MultiStakeholder,
    Platform,
    Housing,
    Credit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CoopStatus {
    Forming,
    Active,
    Suspended,
    Dissolving,
    Dissolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub did: Did,
    pub coop_id: String,
    pub role: MemberRole,
    pub status: MemberStatus,
    pub joined_at: DateTime<Utc>,
    pub shares: u64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemberRole {
    Founder,
    Member,
    Worker,
    Consumer,
    Producer,
    BoardMember,
    Officer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemberStatus {
    Pending,
    Active,
    Suspended,
    Inactive,
    Removed,
}

impl Cooperative {
    pub fn new(name: String, coop_type: CoopType) -> Self {
        let now = Utc::now();
        Self {
            id: format!("coop:{}", uuid::Uuid::new_v4()),
            name,
            coop_type,
            status: CoopStatus::Forming,
            domain_id: None,
            charter_hash: None,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        }
    }

    pub fn can_transition_to(&self, new_status: &CoopStatus) -> bool {
        use CoopStatus::*;
        match (&self.status, new_status) {
            (Forming, Active) => true,
            (Active, Suspended) | (Active, Dissolving) => true,
            (Suspended, Active) | (Suspended, Dissolving) => true,
            (Dissolving, Dissolved) => true,
            _ => false,
        }
    }
}

impl Member {
    pub fn new(did: Did, coop_id: String, role: MemberRole) -> Self {
        Self {
            did,
            coop_id,
            role,
            status: MemberStatus::Pending,
            joined_at: Utc::now(),
            shares: 0,
            metadata: HashMap::new(),
        }
    }
}
