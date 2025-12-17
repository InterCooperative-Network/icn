use crate::{Member, MemberRole, MemberStatus, CoopError, Result};
use icn_identity::Did;
use tracing::{info, warn};

pub struct MembershipManager {
    trust_threshold: f64,
}

#[derive(Debug, Clone)]
pub enum MembershipChange {
    Added { coop_id: String, member: Did, role: MemberRole },
    Approved { coop_id: String, member: Did },
    RoleChanged { coop_id: String, member: Did, old_role: MemberRole, new_role: MemberRole },
    Suspended { coop_id: String, member: Did, reason: String },
    Removed { coop_id: String, member: Did, reason: String },
    SharesUpdated { coop_id: String, member: Did, old: u64, new: u64 },
}

impl Default for MembershipManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MembershipManager {
    pub fn new() -> Self {
        Self {
            trust_threshold: 0.3, // Default minimum trust score
        }
    }

    pub async fn add_member(
        &self,
        mut member: Member,
        min_trust: f64,
    ) -> Result<Member> {
        info!("Adding member {} to coop {}", member.did, member.coop_id);
        
        // Founders bypass trust checks (they're bootstrapping the coop)
        if member.role != MemberRole::Founder {
            // Use provided min_trust or fall back to default threshold
            let threshold = if min_trust > 0.0 { min_trust } else { self.trust_threshold };
            
            // Note: In production, this would query the trust graph
            // For now, we accept all non-founder members as pending
            // The trust check will be enforced during approval
            warn!(
                "Trust check not yet wired for member {}. Required threshold: {}. Status: Pending",
                member.did, threshold
            );
        }
        
        member.status = MemberStatus::Pending;
        Ok(member)
    }

    pub async fn approve_member(&self, mut member: Member) -> Result<Member> {
        if member.status != MemberStatus::Pending {
            return Err(CoopError::InvalidStateTransition(
                format!("Cannot approve member with status {:?}", member.status)
            ));
        }

        info!("Approving member {} in coop {}", member.did, member.coop_id);
        
        member.status = MemberStatus::Active;
        Ok(member)
    }

    pub async fn change_role(
        &self,
        mut member: Member,
        new_role: MemberRole,
    ) -> Result<Member> {
        if member.status != MemberStatus::Active {
            return Err(CoopError::PermissionDenied(
                format!("Cannot change role for member with status {:?}", member.status)
            ));
        }

        info!("Changing role for {} in coop {} to {:?}", 
              member.did, member.coop_id, new_role);
        
        member.role = new_role;
        Ok(member)
    }

    pub async fn suspend_member(
        &self,
        mut member: Member,
        reason: String,
    ) -> Result<Member> {
        if member.status != MemberStatus::Active {
            return Err(CoopError::InvalidStateTransition(
                format!("Cannot suspend member with status {:?}", member.status)
            ));
        }

        warn!("Suspending member {} in coop {}: {}", 
              member.did, member.coop_id, reason);
        
        member.status = MemberStatus::Suspended;
        member.metadata.insert("suspension_reason".to_string(), reason);
        Ok(member)
    }

    pub async fn remove_member(
        &self,
        mut member: Member,
        reason: String,
    ) -> Result<Member> {
        info!("Removing member {} from coop {}: {}", 
              member.did, member.coop_id, reason);
        
        member.status = MemberStatus::Removed;
        member.metadata.insert("removal_reason".to_string(), reason);
        Ok(member)
    }

    pub async fn update_shares(
        &self,
        mut member: Member,
        new_shares: u64,
    ) -> Result<Member> {
        if member.status != MemberStatus::Active {
            return Err(CoopError::PermissionDenied(
                format!("Cannot update shares for member with status {:?}", member.status)
            ));
        }

        info!("Updating shares for {} in coop {} from {} to {}", 
              member.did, member.coop_id, member.shares, new_shares);
        
        member.shares = new_shares;
        Ok(member)
    }
}
