use crate::error::{CooperativeError, Result};
use crate::types::{Cooperative, Member};
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Application to join a cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipApplication {
    pub applicant_did: String,
    pub coop_id: String,
    pub tier: String, // Desired tier name
    pub capital_contribution: u64,
    pub statement: String, // Why they want to join
}

/// Membership-related actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MembershipAction {
    Apply(MembershipApplication),
    Approve {
        coop_id: String,
        did: String,
    },
    Reject {
        coop_id: String,
        did: String,
        reason: String,
    },
    Remove {
        coop_id: String,
        did: String,
        reason: String,
    },
    ChangeTier {
        coop_id: String,
        did: String,
        new_tier: String,
    },
    Suspend {
        coop_id: String,
        did: String,
        reason: String,
    },
    Reinstate {
        coop_id: String,
        did: String,
    },
}

/// Manages cooperative membership
pub struct MembershipManager {
    /// Minimum trust score required for membership
    _min_trust_score: f32,
}

impl MembershipManager {
    pub fn new(min_trust_score: f32) -> Self {
        Self {
            _min_trust_score: min_trust_score,
        }
    }

    /// Add a new member to the cooperative
    pub fn add_member(
        &self,
        coop: &mut Cooperative,
        did: String,
        tier_name: &str,
        capital_contribution: u64,
    ) -> Result<()> {
        if coop.members.contains_key(&did) {
            return Err(CooperativeError::MemberAlreadyExists(did));
        }

        let tier = coop
            .tiers
            .iter()
            .find(|t| t.name == tier_name)
            .cloned()
            .ok_or_else(|| CooperativeError::InvalidType(format!("Tier not found: {tier_name}")))?;

        let member = Member {
            did: did.clone(),
            joined_at: Utc::now(),
            tier,
            capital_contribution,
            active: true,
        };

        coop.capital_pool += capital_contribution;
        coop.members.insert(did, member);
        coop.updated_at = Utc::now();

        Ok(())
    }

    /// Remove a member from the cooperative
    pub fn remove_member(
        &self,
        coop: &mut Cooperative,
        did: &str,
        refund_capital: bool,
    ) -> Result<()> {
        let member = coop
            .members
            .get_mut(did)
            .ok_or_else(|| CooperativeError::MemberNotFound(did.to_string()))?;

        member.active = false;

        if refund_capital {
            coop.capital_pool = coop
                .capital_pool
                .saturating_sub(member.capital_contribution);
        }

        coop.updated_at = Utc::now();
        Ok(())
    }

    /// Change a member's tier
    pub fn change_tier(
        &self,
        coop: &mut Cooperative,
        did: &str,
        new_tier_name: &str,
    ) -> Result<()> {
        let member = coop
            .members
            .get_mut(did)
            .ok_or_else(|| CooperativeError::MemberNotFound(did.to_string()))?;

        let new_tier = coop
            .tiers
            .iter()
            .find(|t| t.name == new_tier_name)
            .cloned()
            .ok_or_else(|| {
                CooperativeError::InvalidType(format!("Tier not found: {new_tier_name}"))
            })?;

        member.tier = new_tier;
        coop.updated_at = Utc::now();
        Ok(())
    }

    /// Suspend a member
    pub fn suspend_member(&self, coop: &mut Cooperative, did: &str) -> Result<()> {
        let member = coop
            .members
            .get_mut(did)
            .ok_or_else(|| CooperativeError::MemberNotFound(did.to_string()))?;

        member.active = false;
        coop.updated_at = Utc::now();
        Ok(())
    }

    /// Reinstate a suspended member
    pub fn reinstate_member(&self, coop: &mut Cooperative, did: &str) -> Result<()> {
        let member = coop
            .members
            .get_mut(did)
            .ok_or_else(|| CooperativeError::MemberNotFound(did.to_string()))?;

        member.active = true;
        coop.updated_at = Utc::now();
        Ok(())
    }

    /// Get voting weight for a member
    pub fn get_voting_weight(&self, coop: &Cooperative, did: &str) -> Result<u32> {
        let member = coop
            .members
            .get(did)
            .ok_or_else(|| CooperativeError::MemberNotFound(did.to_string()))?;

        if !member.active {
            return Ok(0);
        }

        Ok(member.tier.voting_weight)
    }

    /// Calculate profit share for a member
    pub fn calculate_profit_share(
        &self,
        coop: &Cooperative,
        did: &str,
        total_profit: u64,
    ) -> Result<u64> {
        let member = coop
            .members
            .get(did)
            .ok_or_else(|| CooperativeError::MemberNotFound(did.to_string()))?;

        if !member.active {
            return Ok(0);
        }

        let total_weight: u32 = coop
            .members
            .values()
            .filter(|m| m.active)
            .map(|m| m.tier.profit_share_weight)
            .sum();

        if total_weight == 0 {
            return Ok(0);
        }

        let share =
            (total_profit as u128 * member.tier.profit_share_weight as u128) / total_weight as u128;
        Ok(share as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CooperativeType, MembershipTier};

    fn create_test_coop() -> Cooperative {
        let mut coop = Cooperative::new(
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            CooperativeType::Worker,
            "test-domain".to_string(),
            3,
        );

        coop.tiers.push(MembershipTier {
            name: "Worker".to_string(),
            voting_weight: 1,
            profit_share_weight: 1,
            governance_rights: vec![],
        });

        coop
    }

    #[test]
    fn test_add_member() {
        let mut coop = create_test_coop();
        let manager = MembershipManager::new(0.3);

        manager
            .add_member(&mut coop, "did:icn:alice".to_string(), "Worker", 1000)
            .unwrap();

        assert_eq!(coop.members.len(), 1);
        assert_eq!(coop.capital_pool, 1000);
    }

    #[test]
    fn test_profit_share_calculation() {
        let mut coop = create_test_coop();
        let manager = MembershipManager::new(0.3);

        manager
            .add_member(&mut coop, "did:icn:alice".to_string(), "Worker", 1000)
            .unwrap();
        manager
            .add_member(&mut coop, "did:icn:bob".to_string(), "Worker", 1000)
            .unwrap();

        let share = manager
            .calculate_profit_share(&coop, "did:icn:alice", 10000)
            .unwrap();
        assert_eq!(share, 5000); // 50% of profits
    }

    #[test]
    fn test_remove_member() {
        let mut coop = create_test_coop();
        let manager = MembershipManager::new(0.3);

        manager
            .add_member(&mut coop, "did:icn:alice".to_string(), "Worker", 1000)
            .unwrap();

        manager
            .remove_member(&mut coop, "did:icn:alice", true)
            .unwrap();

        assert_eq!(coop.capital_pool, 0);
        assert!(!coop.members.get("did:icn:alice").unwrap().active);
    }
}
