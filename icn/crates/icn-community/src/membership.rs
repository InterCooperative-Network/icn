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
