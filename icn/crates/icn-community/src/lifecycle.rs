use crate::error::{CommunityError, Result};
use crate::types::{Community, CommunityId, CommunityStatus, CommunityType};
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
            return Err(CommunityError::NotPermitted(
                format!("Need {} founders, have {}", self.min_founders, request.founding_members.len())
            ));
        }

        let id = format!("community:{}", request.name.replace(' ', "-").to_lowercase());
        let mut community = Community::new(id, request.name, request.community_type, governance_domain);
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
