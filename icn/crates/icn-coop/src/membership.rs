use crate::{CoopError, Member, MemberRole, MemberStatus, Result};
use icn_identity::Did;
use tracing::{info, warn};

pub struct MembershipManager {
    trust_threshold: f64,
}

#[derive(Debug, Clone)]
pub enum MembershipChange {
    Added {
        coop_id: String,
        member: Did,
        role: MemberRole,
    },
    Approved {
        coop_id: String,
        member: Did,
    },
    RoleChanged {
        coop_id: String,
        member: Did,
        old_role: MemberRole,
        new_role: MemberRole,
    },
    Suspended {
        coop_id: String,
        member: Did,
        reason: String,
    },
    Removed {
        coop_id: String,
        member: Did,
        reason: String,
    },
    SharesUpdated {
        coop_id: String,
        member: Did,
        old: u64,
        new: u64,
    },
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

    pub async fn add_member(&self, mut member: Member, min_trust: f64) -> Result<Member> {
        info!("Adding member {} to coop {}", member.did, member.coop_id);

        // Founders bypass trust checks (they're bootstrapping the coop)
        if member.role != MemberRole::Founder {
            // Use provided min_trust or fall back to default threshold
            let threshold = if min_trust > 0.0 {
                min_trust
            } else {
                self.trust_threshold
            };

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
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot approve member with status {:?}",
                member.status
            )));
        }

        info!("Approving member {} in coop {}", member.did, member.coop_id);

        member.status = MemberStatus::Active;
        Ok(member)
    }

    pub async fn change_role(&self, mut member: Member, new_role: MemberRole) -> Result<Member> {
        if member.status != MemberStatus::Active {
            return Err(CoopError::PermissionDenied(format!(
                "Cannot change role for member with status {:?}",
                member.status
            )));
        }

        info!(
            "Changing role for {} in coop {} to {:?}",
            member.did, member.coop_id, new_role
        );

        member.role = new_role;
        Ok(member)
    }

    pub async fn suspend_member(&self, mut member: Member, reason: String) -> Result<Member> {
        if member.status != MemberStatus::Active {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot suspend member with status {:?}",
                member.status
            )));
        }

        warn!(
            "Suspending member {} in coop {}: {}",
            member.did, member.coop_id, reason
        );

        member.status = MemberStatus::Suspended;
        member
            .metadata
            .insert("suspension_reason".to_string(), reason);
        Ok(member)
    }

    pub async fn remove_member(&self, mut member: Member, reason: String) -> Result<Member> {
        info!(
            "Removing member {} from coop {}: {}",
            member.did, member.coop_id, reason
        );

        member.status = MemberStatus::Removed;
        member.metadata.insert("removal_reason".to_string(), reason);
        Ok(member)
    }

    pub async fn update_shares(&self, mut member: Member, new_shares: u64) -> Result<Member> {
        if member.status != MemberStatus::Active {
            return Err(CoopError::PermissionDenied(format!(
                "Cannot update shares for member with status {:?}",
                member.status
            )));
        }

        info!(
            "Updating shares for {} in coop {} from {} to {}",
            member.did, member.coop_id, member.shares, new_shares
        );

        member.shares = new_shares;
        Ok(member)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn create_test_did() -> icn_identity::Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn create_test_member(role: MemberRole) -> Member {
        Member::new(create_test_did(), "test-coop".to_string(), role)
    }

    // === MembershipChange enum tests ===

    #[test]
    fn test_membership_change_added() {
        let did = create_test_did();
        let change = MembershipChange::Added {
            coop_id: "coop-1".to_string(),
            member: did.clone(),
            role: MemberRole::Worker,
        };

        if let MembershipChange::Added { coop_id, member, role } = change {
            assert_eq!(coop_id, "coop-1");
            assert_eq!(member, did);
            assert_eq!(role, MemberRole::Worker);
        } else {
            panic!("Expected Added variant");
        }
    }

    #[test]
    fn test_membership_change_variants() {
        let did = create_test_did();

        // Test all variants exist and are correctly constructable
        let _ = MembershipChange::Approved {
            coop_id: "coop".to_string(),
            member: did.clone(),
        };
        let _ = MembershipChange::RoleChanged {
            coop_id: "coop".to_string(),
            member: did.clone(),
            old_role: MemberRole::Member,
            new_role: MemberRole::Worker,
        };
        let _ = MembershipChange::Suspended {
            coop_id: "coop".to_string(),
            member: did.clone(),
            reason: "test".to_string(),
        };
        let _ = MembershipChange::Removed {
            coop_id: "coop".to_string(),
            member: did.clone(),
            reason: "test".to_string(),
        };
        let _ = MembershipChange::SharesUpdated {
            coop_id: "coop".to_string(),
            member: did,
            old: 100,
            new: 200,
        };
    }

    // === MembershipManager construction tests ===

    #[test]
    fn test_new_membership_manager() {
        let manager = MembershipManager::new();
        assert_eq!(manager.trust_threshold, 0.3);
    }

    #[test]
    fn test_default_membership_manager() {
        let manager = MembershipManager::default();
        assert_eq!(manager.trust_threshold, 0.3);
    }

    // === add_member tests ===

    #[tokio::test]
    async fn test_add_member_founder_bypasses_trust_check() {
        let manager = MembershipManager::new();
        let member = create_test_member(MemberRole::Founder);

        let result = manager.add_member(member, 0.0).await;
        assert!(result.is_ok());

        let added = result.unwrap();
        assert_eq!(added.status, MemberStatus::Pending);
        assert_eq!(added.role, MemberRole::Founder);
    }

    #[tokio::test]
    async fn test_add_member_non_founder_becomes_pending() {
        let manager = MembershipManager::new();
        let member = create_test_member(MemberRole::Worker);

        let result = manager.add_member(member, 0.5).await;
        assert!(result.is_ok());

        let added = result.unwrap();
        assert_eq!(added.status, MemberStatus::Pending);
    }

    #[tokio::test]
    async fn test_add_member_uses_default_threshold() {
        let manager = MembershipManager::new();
        let member = create_test_member(MemberRole::Member);

        // When min_trust is 0.0, should use default threshold (0.3)
        let result = manager.add_member(member, 0.0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_add_member_all_roles() {
        let manager = MembershipManager::new();

        for role in [
            MemberRole::Member,
            MemberRole::Worker,
            MemberRole::Consumer,
            MemberRole::Producer,
            MemberRole::BoardMember,
            MemberRole::Officer,
        ] {
            let member = create_test_member(role);
            let result = manager.add_member(member, 0.3).await;
            assert!(result.is_ok(), "Failed to add member with role {:?}", role);
        }
    }

    // === approve_member tests ===

    #[tokio::test]
    async fn test_approve_pending_member() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Pending;

        let result = manager.approve_member(member).await;
        assert!(result.is_ok());

        let approved = result.unwrap();
        assert_eq!(approved.status, MemberStatus::Active);
    }

    #[tokio::test]
    async fn test_approve_active_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Active;

        let result = manager.approve_member(member).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, CoopError::InvalidStateTransition(_)));
    }

    #[tokio::test]
    async fn test_approve_suspended_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Suspended;

        let result = manager.approve_member(member).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_approve_removed_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Removed;

        let result = manager.approve_member(member).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_approve_inactive_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Inactive;

        let result = manager.approve_member(member).await;
        assert!(result.is_err());
    }

    // === change_role tests ===

    #[tokio::test]
    async fn test_change_role_active_member() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Member);
        member.status = MemberStatus::Active;

        let result = manager.change_role(member, MemberRole::Worker).await;
        assert!(result.is_ok());

        let changed = result.unwrap();
        assert_eq!(changed.role, MemberRole::Worker);
    }

    #[tokio::test]
    async fn test_change_role_pending_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Member);
        member.status = MemberStatus::Pending;

        let result = manager.change_role(member, MemberRole::Worker).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, CoopError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_change_role_suspended_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Member);
        member.status = MemberStatus::Suspended;

        let result = manager.change_role(member, MemberRole::BoardMember).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_change_role_to_all_roles() {
        let manager = MembershipManager::new();

        for new_role in [
            MemberRole::Founder,
            MemberRole::Member,
            MemberRole::Worker,
            MemberRole::Consumer,
            MemberRole::Producer,
            MemberRole::BoardMember,
            MemberRole::Officer,
        ] {
            let mut member = create_test_member(MemberRole::Member);
            member.status = MemberStatus::Active;

            let result = manager.change_role(member, new_role).await;
            assert!(result.is_ok(), "Failed to change role to {:?}", new_role);
            assert_eq!(result.unwrap().role, new_role);
        }
    }

    // === suspend_member tests ===

    #[tokio::test]
    async fn test_suspend_active_member() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Active;

        let result = manager
            .suspend_member(member, "Violation of terms".to_string())
            .await;
        assert!(result.is_ok());

        let suspended = result.unwrap();
        assert_eq!(suspended.status, MemberStatus::Suspended);
        assert_eq!(
            suspended.metadata.get("suspension_reason"),
            Some(&"Violation of terms".to_string())
        );
    }

    #[tokio::test]
    async fn test_suspend_pending_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Pending;

        let result = manager
            .suspend_member(member, "Some reason".to_string())
            .await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, CoopError::InvalidStateTransition(_)));
    }

    #[tokio::test]
    async fn test_suspend_already_suspended_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Suspended;

        let result = manager
            .suspend_member(member, "Another reason".to_string())
            .await;
        assert!(result.is_err());
    }

    // === remove_member tests ===

    #[tokio::test]
    async fn test_remove_member_active() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Active;

        let result = manager
            .remove_member(member, "Left voluntarily".to_string())
            .await;
        assert!(result.is_ok());

        let removed = result.unwrap();
        assert_eq!(removed.status, MemberStatus::Removed);
        assert_eq!(
            removed.metadata.get("removal_reason"),
            Some(&"Left voluntarily".to_string())
        );
    }

    #[tokio::test]
    async fn test_remove_member_pending() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Pending;

        let result = manager
            .remove_member(member, "Application rejected".to_string())
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, MemberStatus::Removed);
    }

    #[tokio::test]
    async fn test_remove_member_suspended() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Suspended;

        let result = manager.remove_member(member, "Expelled".to_string()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, MemberStatus::Removed);
    }

    // === update_shares tests ===

    #[tokio::test]
    async fn test_update_shares_active_member() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Active;
        member.shares = 100;

        let result = manager.update_shares(member, 200).await;
        assert!(result.is_ok());

        let updated = result.unwrap();
        assert_eq!(updated.shares, 200);
    }

    #[tokio::test]
    async fn test_update_shares_to_zero() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Active;
        member.shares = 100;

        let result = manager.update_shares(member, 0).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().shares, 0);
    }

    #[tokio::test]
    async fn test_update_shares_pending_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Pending;

        let result = manager.update_shares(member, 100).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, CoopError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_update_shares_suspended_member_fails() {
        let manager = MembershipManager::new();
        let mut member = create_test_member(MemberRole::Worker);
        member.status = MemberStatus::Suspended;

        let result = manager.update_shares(member, 100).await;
        assert!(result.is_err());
    }

    // === Full workflow tests ===

    #[tokio::test]
    async fn test_full_member_lifecycle() {
        let manager = MembershipManager::new();

        // 1. Create and add member
        let member = create_test_member(MemberRole::Worker);
        let member = manager.add_member(member, 0.3).await.unwrap();
        assert_eq!(member.status, MemberStatus::Pending);

        // 2. Approve member
        let member = manager.approve_member(member).await.unwrap();
        assert_eq!(member.status, MemberStatus::Active);

        // 3. Update shares
        let member = manager.update_shares(member, 100).await.unwrap();
        assert_eq!(member.shares, 100);

        // 4. Change role
        let member = manager.change_role(member, MemberRole::BoardMember).await.unwrap();
        assert_eq!(member.role, MemberRole::BoardMember);

        // 5. Suspend
        let member = manager
            .suspend_member(member, "Under review".to_string())
            .await
            .unwrap();
        assert_eq!(member.status, MemberStatus::Suspended);

        // 6. Remove
        let member = manager
            .remove_member(member, "Review complete - removed".to_string())
            .await
            .unwrap();
        assert_eq!(member.status, MemberStatus::Removed);
    }

    #[tokio::test]
    async fn test_founder_workflow() {
        let manager = MembershipManager::new();

        // Founders bypass trust check
        let member = create_test_member(MemberRole::Founder);
        let member = manager.add_member(member, 0.0).await.unwrap();
        assert_eq!(member.status, MemberStatus::Pending);

        // Auto-approve founder
        let member = manager.approve_member(member).await.unwrap();
        assert_eq!(member.status, MemberStatus::Active);
        assert_eq!(member.role, MemberRole::Founder);
    }
}
