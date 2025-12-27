use chrono::{DateTime, Utc};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a cooperative
pub type CooperativeId = String;

/// Membership tier with associated rights and responsibilities
///
/// Tiers allow cooperatives to define different levels of membership
/// with varying voting weights, profit shares, and governance rights.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipTier {
    /// Name of the tier (e.g., "Worker", "Consumer", "Patron")
    pub name: String,
    /// Voting weight for governance decisions
    pub voting_weight: u32,
    /// Weight for profit/surplus distribution
    pub profit_share_weight: u32,
    /// List of governance rights (e.g., "vote", "propose", "elect_board")
    pub governance_rights: Vec<String>,
}

impl MembershipTier {
    /// Create a standard member tier with equal voting and profit share
    pub fn standard(name: &str) -> Self {
        Self {
            name: name.to_string(),
            voting_weight: 1,
            profit_share_weight: 1,
            governance_rights: vec!["vote".to_string()],
        }
    }

    /// Create a founder tier with enhanced rights
    pub fn founder() -> Self {
        Self {
            name: "Founder".to_string(),
            voting_weight: 1,
            profit_share_weight: 1,
            governance_rights: vec![
                "vote".to_string(),
                "propose".to_string(),
                "elect_board".to_string(),
                "amend_bylaws".to_string(),
            ],
        }
    }
}

impl Default for MembershipTier {
    fn default() -> Self {
        Self::standard("Member")
    }
}

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

    // === Fields from icn-cooperative for richer governance ===
    /// Minimum members required for the cooperative to operate
    #[serde(default = "default_min_members")]
    pub min_members: usize,

    /// Available membership tiers
    #[serde(default)]
    pub tiers: Vec<MembershipTier>,

    /// Total capital pool contributed by members
    #[serde(default)]
    pub capital_pool: u64,

    /// Bylaws and governing documents (CCL contract IDs or hashes)
    #[serde(default)]
    pub bylaws: Vec<String>,
}

fn default_min_members() -> usize {
    3
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CoopType {
    Worker,
    Consumer,
    Producer,
    MultiStakeholder,
    Platform,
    Housing,
    Credit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

    // === Fields from icn-cooperative for richer membership ===
    /// Optional membership tier (for tier-based governance)
    #[serde(default)]
    pub tier: Option<MembershipTier>,

    /// Capital contribution made by this member
    #[serde(default)]
    pub capital_contribution: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemberRole {
    Founder,
    Member,
    Worker,
    Consumer,
    Producer,
    BoardMember,
    Officer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemberStatus {
    Pending,
    Active,
    Suspended,
    Inactive,
    Removed,
}

/// Application to join a cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipApplication {
    /// DID of the applicant
    pub applicant_did: Did,
    /// Target cooperative ID
    pub coop_id: String,
    /// Desired tier name (if tier-based)
    pub tier: Option<String>,
    /// Desired role
    pub role: MemberRole,
    /// Capital contribution offered
    pub capital_contribution: u64,
    /// Statement explaining why they want to join
    pub statement: String,
    /// When the application was submitted
    pub submitted_at: DateTime<Utc>,
}

impl MembershipApplication {
    pub fn new(applicant_did: Did, coop_id: String, role: MemberRole, statement: String) -> Self {
        Self {
            applicant_did,
            coop_id,
            tier: None,
            role,
            capital_contribution: 0,
            statement,
            submitted_at: Utc::now(),
        }
    }

    pub fn with_capital(mut self, amount: u64) -> Self {
        self.capital_contribution = amount;
        self
    }

    pub fn with_tier(mut self, tier: String) -> Self {
        self.tier = Some(tier);
        self
    }
}

impl Cooperative {
    /// Create a new cooperative with explicit ID
    pub fn new_with_id(id: String, name: String, coop_type: CoopType) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            coop_type,
            status: CoopStatus::Forming,
            domain_id: None,
            charter_hash: None,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
            min_members: 3,
            tiers: Vec::new(),
            capital_pool: 0,
            bylaws: Vec::new(),
        }
    }

    /// Create a new cooperative with auto-generated ID
    pub fn new(name: String, coop_type: CoopType) -> Self {
        let id = format!("coop:{}", uuid::Uuid::new_v4());
        Self::new_with_id(id, name, coop_type)
    }

    /// Create a new cooperative with governance domain
    pub fn new_with_domain(
        id: String,
        name: String,
        coop_type: CoopType,
        domain_id: String,
        min_members: usize,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            coop_type,
            status: CoopStatus::Forming,
            domain_id: Some(domain_id),
            charter_hash: None,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
            min_members,
            tiers: Vec::new(),
            capital_pool: 0,
            bylaws: Vec::new(),
        }
    }

    pub fn can_transition_to(&self, new_status: &CoopStatus) -> bool {
        use CoopStatus::*;
        matches!(
            (&self.status, new_status),
            (Forming, Active)
                | (Active, Suspended)
                | (Active, Dissolving)
                | (Suspended, Active)
                | (Suspended, Dissolving)
                | (Dissolving, Dissolved)
        )
    }

    /// Add a membership tier
    pub fn add_tier(&mut self, tier: MembershipTier) {
        self.tiers.push(tier);
        self.updated_at = Utc::now();
    }

    /// Find a tier by name
    pub fn find_tier(&self, name: &str) -> Option<&MembershipTier> {
        self.tiers.iter().find(|t| t.name == name)
    }

    /// Add capital to the pool
    pub fn add_capital(&mut self, amount: u64) {
        self.capital_pool = self.capital_pool.saturating_add(amount);
        self.updated_at = Utc::now();
    }

    /// Remove capital from the pool (e.g., for member exit)
    pub fn remove_capital(&mut self, amount: u64) {
        self.capital_pool = self.capital_pool.saturating_sub(amount);
        self.updated_at = Utc::now();
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
            tier: None,
            capital_contribution: 0,
        }
    }

    /// Create a member with a specific tier
    pub fn with_tier(mut self, tier: MembershipTier) -> Self {
        self.tier = Some(tier);
        self
    }

    /// Set capital contribution
    pub fn with_capital(mut self, amount: u64) -> Self {
        self.capital_contribution = amount;
        self
    }

    /// Get voting weight (from tier or default 1)
    pub fn voting_weight(&self) -> u32 {
        self.tier.as_ref().map(|t| t.voting_weight).unwrap_or(1)
    }

    /// Get profit share weight (from tier or default 1)
    pub fn profit_share_weight(&self) -> u32 {
        self.tier
            .as_ref()
            .map(|t| t.profit_share_weight)
            .unwrap_or(1)
    }

    /// Check if member has a specific governance right
    pub fn has_governance_right(&self, right: &str) -> bool {
        self.tier
            .as_ref()
            .map(|t| t.governance_rights.iter().any(|r| r == right))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_tier_standard() {
        let tier = MembershipTier::standard("Worker");
        assert_eq!(tier.name, "Worker");
        assert_eq!(tier.voting_weight, 1);
        assert_eq!(tier.profit_share_weight, 1);
    }

    #[test]
    fn test_membership_tier_founder() {
        let tier = MembershipTier::founder();
        assert_eq!(tier.name, "Founder");
        assert!(tier.governance_rights.contains(&"amend_bylaws".to_string()));
    }

    #[test]
    fn test_cooperative_with_tiers() {
        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        coop.add_tier(MembershipTier::founder());
        coop.add_tier(MembershipTier::standard("Worker"));

        assert_eq!(coop.tiers.len(), 2);
        assert!(coop.find_tier("Founder").is_some());
        assert!(coop.find_tier("Worker").is_some());
        assert!(coop.find_tier("Consumer").is_none());
    }

    #[test]
    fn test_cooperative_capital() {
        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        assert_eq!(coop.capital_pool, 0);

        coop.add_capital(1000);
        assert_eq!(coop.capital_pool, 1000);

        coop.add_capital(500);
        assert_eq!(coop.capital_pool, 1500);

        coop.remove_capital(300);
        assert_eq!(coop.capital_pool, 1200);
    }

    #[test]
    fn test_member_with_tier() {
        let did = icn_identity::KeyPair::generate()
            .expect("keypair")
            .did()
            .clone();

        let tier = MembershipTier {
            name: "Senior Worker".to_string(),
            voting_weight: 2,
            profit_share_weight: 3,
            governance_rights: vec!["vote".to_string(), "propose".to_string()],
        };

        let member = Member::new(did, "coop:test".to_string(), MemberRole::Worker)
            .with_tier(tier)
            .with_capital(500);

        assert_eq!(member.voting_weight(), 2);
        assert_eq!(member.profit_share_weight(), 3);
        assert_eq!(member.capital_contribution, 500);
        assert!(member.has_governance_right("vote"));
        assert!(member.has_governance_right("propose"));
        assert!(!member.has_governance_right("elect_board"));
    }
}
