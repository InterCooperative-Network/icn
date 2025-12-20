use crate::error::{CooperativeError, Result};
use crate::types::{
    Cooperative, CooperativeId, CooperativeStatus, CooperativeType, MembershipTier,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request to form a new cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationRequest {
    pub name: String,
    pub coop_type: CooperativeType,
    pub founding_members: Vec<String>, // DIDs
    pub min_members: usize,
    pub tiers: Vec<MembershipTier>,
    pub bylaws_ccl: Vec<String>,               // CCL contract source
    pub initial_capital: HashMap<String, u64>, // DID -> capital contribution
}

/// Request to dissolve a cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissolutionRequest {
    pub coop_id: CooperativeId,
    pub reason: String,
    pub asset_distribution: HashMap<String, u64>, // DID -> share
}

/// Manages cooperative lifecycle transitions
pub struct CooperativeLifecycle {
    min_founders: usize,
}

impl CooperativeLifecycle {
    pub fn new(min_founders: usize) -> Self {
        Self { min_founders }
    }

    /// Create a new cooperative in forming status
    pub fn form(
        &self,
        request: FormationRequest,
        governance_domain: String,
    ) -> Result<Cooperative> {
        if request.founding_members.len() < self.min_founders {
            return Err(CooperativeError::InsufficientFounders {
                required: self.min_founders,
                actual: request.founding_members.len(),
            });
        }

        let id = Self::generate_id(&request.name, &request.founding_members);
        let mut coop = Cooperative::new(
            id,
            request.name,
            request.coop_type,
            governance_domain,
            request.min_members,
        );

        coop.tiers = request.tiers;
        coop.bylaws = request.bylaws_ccl;

        // Add founding members
        for did in request.founding_members {
            let capital = request.initial_capital.get(&did).copied().unwrap_or(0);
            let member = crate::types::Member {
                did: did.clone(),
                joined_at: chrono::Utc::now(),
                tier: coop
                    .tiers
                    .first()
                    .cloned()
                    .unwrap_or_else(|| MembershipTier {
                        name: "Founding".to_string(),
                        voting_weight: 1,
                        profit_share_weight: 1,
                        governance_rights: vec![],
                    }),
                capital_contribution: capital,
                active: true,
            };
            coop.capital_pool += capital;
            coop.members.insert(did, member);
        }

        Ok(coop)
    }

    /// Activate a cooperative after formation
    pub fn activate(&self, coop: &mut Cooperative) -> Result<()> {
        if coop.status != CooperativeStatus::Forming {
            return Err(CooperativeError::InvalidStatusTransition {
                from: coop.status,
                to: CooperativeStatus::Active,
            });
        }

        if !coop.can_activate() {
            return Err(CooperativeError::InsufficientFounders {
                required: coop.min_members,
                actual: coop.member_count(),
            });
        }

        coop.status = CooperativeStatus::Active;
        coop.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Suspend a cooperative
    pub fn suspend(&self, coop: &mut Cooperative, reason: String) -> Result<()> {
        if coop.status != CooperativeStatus::Active {
            return Err(CooperativeError::InvalidStatusTransition {
                from: coop.status,
                to: CooperativeStatus::Suspended,
            });
        }

        coop.status = CooperativeStatus::Suspended;
        coop.updated_at = chrono::Utc::now();
        coop.metadata
            .insert("suspension_reason".to_string(), reason);
        Ok(())
    }

    /// Resume a suspended cooperative
    pub fn resume(&self, coop: &mut Cooperative) -> Result<()> {
        if coop.status != CooperativeStatus::Suspended {
            return Err(CooperativeError::InvalidStatusTransition {
                from: coop.status,
                to: CooperativeStatus::Active,
            });
        }

        coop.status = CooperativeStatus::Active;
        coop.updated_at = chrono::Utc::now();
        coop.metadata.remove("suspension_reason");
        Ok(())
    }

    /// Begin dissolution process
    pub fn begin_dissolution(
        &self,
        coop: &mut Cooperative,
        request: DissolutionRequest,
    ) -> Result<()> {
        if coop.status == CooperativeStatus::Dissolved
            || coop.status == CooperativeStatus::Dissolving
        {
            return Err(CooperativeError::InvalidStatusTransition {
                from: coop.status,
                to: CooperativeStatus::Dissolving,
            });
        }

        coop.status = CooperativeStatus::Dissolving;
        coop.updated_at = chrono::Utc::now();
        coop.metadata
            .insert("dissolution_reason".to_string(), request.reason);

        // Store asset distribution plan
        let distribution_json = serde_json::to_string(&request.asset_distribution)
            .map_err(|e| CooperativeError::Serialization(e.to_string()))?;
        coop.metadata
            .insert("asset_distribution".to_string(), distribution_json);

        Ok(())
    }

    /// Complete dissolution after all assets distributed
    pub fn complete_dissolution(&self, coop: &mut Cooperative) -> Result<()> {
        if coop.status != CooperativeStatus::Dissolving {
            return Err(CooperativeError::InvalidStatusTransition {
                from: coop.status,
                to: CooperativeStatus::Dissolved,
            });
        }

        coop.status = CooperativeStatus::Dissolved;
        coop.updated_at = chrono::Utc::now();

        // Deactivate all members
        for member in coop.members.values_mut() {
            member.active = false;
        }

        Ok(())
    }

    fn generate_id(name: &str, founding_members: &[String]) -> CooperativeId {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        for member in founding_members {
            member.hash(&mut hasher);
        }
        format!("coop:{:x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formation_lifecycle() {
        let lifecycle = CooperativeLifecycle::new(3);

        let request = FormationRequest {
            name: "Test Coop".to_string(),
            coop_type: CooperativeType::Worker,
            founding_members: vec![
                "did:icn:alice".to_string(),
                "did:icn:bob".to_string(),
                "did:icn:carol".to_string(),
            ],
            min_members: 3,
            tiers: vec![],
            bylaws_ccl: vec![],
            initial_capital: HashMap::new(),
        };

        let mut coop = lifecycle.form(request, "test-domain".to_string()).unwrap();
        assert_eq!(coop.status, CooperativeStatus::Forming);
        assert_eq!(coop.member_count(), 3);

        lifecycle.activate(&mut coop).unwrap();
        assert_eq!(coop.status, CooperativeStatus::Active);
    }

    #[test]
    fn test_insufficient_founders() {
        let lifecycle = CooperativeLifecycle::new(5);

        let request = FormationRequest {
            name: "Test Coop".to_string(),
            coop_type: CooperativeType::Worker,
            founding_members: vec!["did:icn:alice".to_string()],
            min_members: 3,
            tiers: vec![],
            bylaws_ccl: vec![],
            initial_capital: HashMap::new(),
        };

        let result = lifecycle.form(request, "test-domain".to_string());
        assert!(result.is_err());
    }
}
