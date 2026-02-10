//! Unified Membership Model
//!
//! This module consolidates membership models from icn-entity, icn-coop,
//! and icn-community, providing a generic trait-based approach.

use crate::entity::{EntityId, MembershipCriteria};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Re-export membership types from icn-entity
pub use icn_entity::membership::{
    Membership as EntityMembership, MembershipCapability, MembershipRole, MembershipStatus,
    UnifiedMembershipStatus,
};

/// Generic membership trait that all membership types implement
pub trait MembershipTrait {
    /// Get the member's entity ID
    fn member_id(&self) -> &EntityId;

    /// Get the parent entity ID
    fn parent_id(&self) -> &EntityId;

    /// Get the membership role
    fn role(&self) -> &MembershipRole;

    /// Get the membership status
    fn status(&self) -> &MembershipStatus;

    /// Check if membership is active
    fn is_active(&self) -> bool {
        matches!(self.status(), MembershipStatus::Active)
    }

    /// Check if member can vote
    fn can_vote(&self) -> bool;

    /// Check if member can propose
    fn can_propose(&self) -> bool;

    /// Get voting shares/weight
    fn shares(&self) -> u64;
}

/// Unified membership record that works for all entity types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMembership {
    /// The entity being granted membership
    pub member_id: EntityId,

    /// The entity granting membership
    pub parent_id: EntityId,

    /// Role within the parent entity
    pub role: MembershipRole,

    /// Current membership status
    pub status: MembershipStatus,

    /// When membership was created (Unix timestamp)
    pub joined_at: u64,

    /// When membership was last updated (Unix timestamp)
    pub updated_at: u64,

    /// Voting shares/weight
    pub shares: u64,

    /// Capabilities granted by this membership
    pub capabilities: Vec<MembershipCapability>,

    /// Custom metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,

    /// Active labor assignments
    #[serde(default)]
    pub assignments: Vec<String>,

    /// Labor share IDs
    #[serde(default)]
    pub labor_shares: Vec<String>,

    /// Is this the primary membership for multi-coop workers?
    #[serde(default = "default_is_primary")]
    pub is_primary: bool,
}

fn default_is_primary() -> bool {
    true
}

impl UnifiedMembership {
    /// Create a new pending membership
    pub fn new(member_id: EntityId, parent_id: EntityId, role: MembershipRole) -> Self {
        let now = icn_time::current_timestamp_secs();
        let capabilities = role.default_capabilities();

        Self {
            member_id,
            parent_id,
            role,
            status: MembershipStatus::Pending,
            joined_at: now,
            updated_at: now,
            shares: 1,
            capabilities,
            metadata: HashMap::new(),
            assignments: Vec::new(),
            labor_shares: Vec::new(),
            is_primary: true,
        }
    }

    /// Create an active membership
    pub fn active(member_id: EntityId, parent_id: EntityId, role: MembershipRole) -> Self {
        let mut m = Self::new(member_id, parent_id, role);
        m.status = MembershipStatus::Active;
        m
    }

    /// Set voting weight
    pub fn with_shares(mut self, weight: u64) -> Self {
        self.shares = weight;
        self
    }

    /// Set capabilities
    pub fn with_capabilities(mut self, capabilities: Vec<MembershipCapability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Add a capability
    pub fn add_capability(&mut self, capability: MembershipCapability) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Check if has capability
    pub fn has_capability(&self, capability: &MembershipCapability) -> bool {
        self.capabilities.contains(capability)
    }

    /// Approve membership (pending -> active)
    pub fn approve(&mut self) -> Result<(), MembershipError> {
        if self.status != MembershipStatus::Pending {
            return Err(MembershipError::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: "Active".to_string(),
            });
        }
        self.status = MembershipStatus::Active;
        self.updated_at = icn_time::current_timestamp_secs();
        Ok(())
    }

    /// Suspend membership
    pub fn suspend(&mut self, reason: String) -> Result<(), MembershipError> {
        if self.status != MembershipStatus::Active {
            return Err(MembershipError::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: "Suspended".to_string(),
            });
        }
        self.status = MembershipStatus::Suspended;
        self.metadata
            .insert("suspension_reason".to_string(), reason);
        self.updated_at = icn_time::current_timestamp_secs();
        Ok(())
    }

    /// Remove membership
    pub fn remove(&mut self, reason: String) {
        self.status = MembershipStatus::Removed;
        self.metadata.insert("removal_reason".to_string(), reason);
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Change role
    pub fn change_role(&mut self, new_role: MembershipRole) -> Result<(), MembershipError> {
        if self.status != MembershipStatus::Active {
            return Err(MembershipError::PermissionDenied(
                "Cannot change role for non-active member".to_string(),
            ));
        }
        self.role = new_role;
        self.updated_at = icn_time::current_timestamp_secs();
        Ok(())
    }
}

impl MembershipTrait for UnifiedMembership {
    fn member_id(&self) -> &EntityId {
        &self.member_id
    }

    fn parent_id(&self) -> &EntityId {
        &self.parent_id
    }

    fn role(&self) -> &MembershipRole {
        &self.role
    }

    fn status(&self) -> &MembershipStatus {
        &self.status
    }

    fn can_vote(&self) -> bool {
        self.is_active() && self.shares > 0 && self.has_capability(&MembershipCapability::Vote)
    }

    fn can_propose(&self) -> bool {
        self.is_active() && self.has_capability(&MembershipCapability::Propose)
    }

    fn shares(&self) -> u64 {
        self.shares
    }
}

/// Membership manager for handling membership operations
pub struct MembershipManager {
    /// Minimum trust threshold
    trust_threshold: f64,
}

impl MembershipManager {
    /// Create a new membership manager
    pub fn new() -> Self {
        Self {
            trust_threshold: 0.3,
        }
    }

    /// Add a new member
    pub async fn add_member(
        &self,
        member_id: EntityId,
        parent_id: EntityId,
        role: MembershipRole,
        min_trust: f64,
    ) -> Result<UnifiedMembership, MembershipError> {
        // Check trust threshold
        let threshold = if min_trust > 0.0 {
            min_trust
        } else {
            self.trust_threshold
        };

        // TODO: Query trust PolicyOracle for actual trust score and reject
        // members below threshold. Currently all members are accepted as Pending.
        tracing::debug!(
            "Adding member {:?} to {:?} with trust threshold {}",
            member_id,
            parent_id,
            threshold
        );

        Ok(UnifiedMembership::new(member_id, parent_id, role))
    }

    /// Maximum number of conditions allowed in a single criteria block.
    const MAX_CONDITIONS: usize = 64;

    /// Maximum number of elements allowed in an `in` operator array.
    const MAX_IN_ARRAY_SIZE: usize = 1000;

    /// Evaluate membership criteria using CCL
    pub async fn evaluate_criteria(
        &self,
        member_data: &HashMap<String, serde_json::Value>,
        criteria: &MembershipCriteria,
    ) -> Result<bool, MembershipError> {
        if criteria.all.len() > Self::MAX_CONDITIONS || criteria.any.len() > Self::MAX_CONDITIONS {
            return Err(MembershipError::InvalidCriteria(format!(
                "Too many conditions (max {} per block)",
                Self::MAX_CONDITIONS
            )));
        }

        // Evaluate "all" conditions
        for condition in &criteria.all {
            if !Self::evaluate_condition(member_data, condition)? {
                return Ok(false);
            }
        }

        // Evaluate "any" conditions (at least one must be true)
        if !criteria.any.is_empty() {
            let mut any_true = false;
            for condition in &criteria.any {
                if Self::evaluate_condition(member_data, condition)? {
                    any_true = true;
                    break;
                }
            }
            if !any_true {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Evaluate a single condition
    fn evaluate_condition(
        data: &HashMap<String, serde_json::Value>,
        condition: &crate::entity::Condition,
    ) -> Result<bool, MembershipError> {
        let field_value = data.get(&condition.field);

        match condition.op.as_str() {
            "==" => Ok(field_value == Some(&condition.value)),
            "!=" => Ok(field_value != Some(&condition.value)),
            ">" | ">=" | "<" | "<=" => {
                let v2 = condition.value.as_f64().ok_or_else(|| {
                    MembershipError::InvalidCriteria(format!(
                        "Non-numeric comparison value for operator '{}'",
                        condition.op
                    ))
                })?;
                let v1 = match field_value {
                    Some(v) => v.as_f64().ok_or_else(|| {
                        MembershipError::InvalidCriteria(format!(
                            "Field '{}' is not numeric",
                            condition.field
                        ))
                    })?,
                    None => return Ok(false), // Missing field ⇒ condition not met
                };
                Ok(match condition.op.as_str() {
                    ">" => v1 > v2,
                    ">=" => v1 >= v2,
                    "<" => v1 < v2,
                    "<=" => v1 <= v2,
                    _ => unreachable!(),
                })
            }
            "in" => {
                if let (Some(v1), Some(arr)) = (field_value, condition.value.as_array()) {
                    if arr.len() > MembershipManager::MAX_IN_ARRAY_SIZE {
                        return Err(MembershipError::InvalidCriteria(format!(
                            "'in' operator array too large: {} elements (max: {})",
                            arr.len(),
                            MembershipManager::MAX_IN_ARRAY_SIZE
                        )));
                    }
                    Ok(arr.contains(v1))
                } else {
                    Ok(false)
                }
            }
            _ => Err(MembershipError::InvalidCriteria(format!(
                "Unknown operator: {}",
                condition.op
            ))),
        }
    }
}

impl Default for MembershipManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Membership errors
#[derive(Debug, thiserror::Error)]
pub enum MembershipError {
    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid criteria: {0}")]
    InvalidCriteria(String),

    #[error("Member not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_entity_id() -> EntityId {
        EntityId::from_did(icn_identity::KeyPair::generate().unwrap().did())
    }

    #[test]
    fn test_unified_membership_creation() {
        let member_id = create_test_entity_id();
        let parent_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");

        let membership =
            UnifiedMembership::new(member_id.clone(), parent_id, MembershipRole::Worker);

        assert_eq!(membership.member_id, member_id);
        assert!(matches!(membership.status, MembershipStatus::Pending));
        assert_eq!(membership.shares, 1);
    }

    #[test]
    fn test_membership_state_transitions() {
        let member_id = create_test_entity_id();
        let parent_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");

        let mut membership = UnifiedMembership::new(member_id, parent_id, MembershipRole::Worker);

        // Approve
        assert!(membership.approve().is_ok());
        assert!(matches!(membership.status, MembershipStatus::Active));

        // Suspend
        assert!(membership.suspend("Test reason".to_string()).is_ok());
        assert!(matches!(membership.status, MembershipStatus::Suspended));
    }

    #[test]
    fn test_membership_capabilities() {
        let member_id = create_test_entity_id();
        let parent_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");

        let mut membership =
            UnifiedMembership::active(member_id, parent_id, MembershipRole::Worker);

        assert!(membership.can_vote());
        assert!(membership.can_propose());

        membership.add_capability(MembershipCapability::Invite);
        assert!(membership.has_capability(&MembershipCapability::Invite));
    }

    #[tokio::test]
    async fn test_criteria_evaluation() {
        let manager = MembershipManager::new();

        let mut data = HashMap::new();
        data.insert("verified_address".to_string(), serde_json::json!(true));

        let criteria = MembershipCriteria {
            all: vec![crate::entity::Condition {
                field: "verified_address".to_string(),
                op: "==".to_string(),
                value: serde_json::json!(true),
            }],
            any: vec![],
        };

        let result = manager.evaluate_criteria(&data, &criteria).await.unwrap();
        assert!(result);
    }

    #[test]
    fn test_invalid_state_transition_approve_active() {
        let member_id = create_test_entity_id();
        let parent_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");
        let mut membership =
            UnifiedMembership::active(member_id, parent_id, MembershipRole::Worker);

        // Already active, can't approve again
        let err = membership.approve().unwrap_err();
        assert!(matches!(
            err,
            MembershipError::InvalidStateTransition { .. }
        ));
    }

    #[test]
    fn test_invalid_state_transition_suspend_pending() {
        let member_id = create_test_entity_id();
        let parent_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");
        let mut membership = UnifiedMembership::new(member_id, parent_id, MembershipRole::Worker);

        // Pending member can't be suspended
        let err = membership.suspend("reason".to_string()).unwrap_err();
        assert!(matches!(
            err,
            MembershipError::InvalidStateTransition { .. }
        ));
    }

    #[test]
    fn test_change_role_on_inactive_fails() {
        let member_id = create_test_entity_id();
        let parent_id = EntityId::cooperative("test-coop").expect("Invalid coop ID");
        let mut membership = UnifiedMembership::new(member_id, parent_id, MembershipRole::Worker);

        let err = membership.change_role(MembershipRole::Founder).unwrap_err();
        assert!(matches!(err, MembershipError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_unknown_operator_returns_error() {
        let manager = MembershipManager::new();
        let data = HashMap::new();

        let criteria = MembershipCriteria {
            all: vec![crate::entity::Condition {
                field: "x".to_string(),
                op: "~=".to_string(),
                value: serde_json::json!(1),
            }],
            any: vec![],
        };

        let err = manager
            .evaluate_criteria(&data, &criteria)
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::InvalidCriteria(_)));
    }

    #[tokio::test]
    async fn test_max_conditions_exceeded() {
        let manager = MembershipManager::new();
        let data = HashMap::new();

        let too_many: Vec<crate::entity::Condition> = (0..65)
            .map(|i| crate::entity::Condition {
                field: format!("f{i}"),
                op: "==".to_string(),
                value: serde_json::json!(true),
            })
            .collect();

        let criteria = MembershipCriteria {
            all: too_many,
            any: vec![],
        };

        let err = manager
            .evaluate_criteria(&data, &criteria)
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::InvalidCriteria(_)));
    }

    #[tokio::test]
    async fn test_empty_any_array_passes() {
        let manager = MembershipManager::new();
        let data = HashMap::new();

        // Empty `any` array should not block (no "any" requirement)
        let criteria = MembershipCriteria {
            all: vec![],
            any: vec![],
        };

        let result = manager.evaluate_criteria(&data, &criteria).await.unwrap();
        assert!(result, "Empty criteria should pass");
    }

    #[tokio::test]
    async fn test_in_operator_array_size_limit() {
        let manager = MembershipManager::new();
        let mut data = HashMap::new();
        data.insert("role".to_string(), serde_json::json!("admin"));

        let big_array: Vec<serde_json::Value> = (0..1001)
            .map(|i| serde_json::json!(format!("v{}", i)))
            .collect();

        let criteria = MembershipCriteria {
            all: vec![crate::entity::Condition {
                field: "role".to_string(),
                op: "in".to_string(),
                value: serde_json::Value::Array(big_array),
            }],
            any: vec![],
        };

        let err = manager
            .evaluate_criteria(&data, &criteria)
            .await
            .unwrap_err();
        assert!(matches!(err, MembershipError::InvalidCriteria(_)));
    }

    // ========================================
    // UnifiedMembershipStatus Integration Tests
    // ========================================

    #[test]
    fn test_unified_status_reexport() {
        // Verify UnifiedMembershipStatus is properly re-exported
        let status = UnifiedMembershipStatus::default();
        assert_eq!(status, UnifiedMembershipStatus::Applicant);
    }

    #[test]
    fn test_unified_status_lifecycle() {
        use UnifiedMembershipStatus::*;

        // Test the full lifecycle path
        let mut status = Applicant;
        assert!(!status.is_active());
        assert!(!status.can_participate());

        // Approve application
        assert!(status.can_transition_to(&Pending));
        status = Pending;

        // Complete onboarding
        assert!(status.can_transition_to(&Active));
        status = Active;
        assert!(status.is_active());
        assert!(status.can_participate());

        // Temporarily suspend
        assert!(status.can_transition_to(&Suspended));
        status = Suspended;
        assert!(!status.is_active());

        // Reactivate
        assert!(status.can_transition_to(&Active));
        status = Active;

        // Terminate
        assert!(status.can_transition_to(&Terminated));
        status = Terminated;
        assert!(status.is_terminal());

        // Cannot transition out of terminal
        assert!(!status.can_transition_to(&Active));
    }

    #[test]
    fn test_unified_status_conversion_from_membership_status() {
        // Test that legacy MembershipStatus converts correctly
        let active: UnifiedMembershipStatus = MembershipStatus::Active.into();
        assert_eq!(active, UnifiedMembershipStatus::Active);

        let removed: UnifiedMembershipStatus = MembershipStatus::Removed.into();
        assert_eq!(removed, UnifiedMembershipStatus::Terminated);
    }
}
