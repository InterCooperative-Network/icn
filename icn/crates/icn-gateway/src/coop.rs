//! Cooperative namespace management
//!
//! Each co-op has:
//! - Unique namespace ID
//! - Member list with roles
//! - Settings (governance params, credit policies, etc.)
//! - Isolated ledger and contract storage

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use icn_identity::Did;
use serde::{Deserialize, Serialize};

use crate::error::{GatewayError, Result};

/// Cooperative namespace ID
pub type CoopId = String;

/// Member role within a cooperative
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
}

/// Cooperative member with role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoopMember {
    pub did: Did,
    pub role: MemberRole,
    pub joined_at: u64,
}

/// Cooperative settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoopSettings {
    pub governance_model: String, // e.g., "consensus", "majority"
    pub credit_policy: String,     // e.g., "conservative", "permissive"
    pub currency: String,          // e.g., "hours", "USD"
}

impl Default for CoopSettings {
    fn default() -> Self {
        Self {
            governance_model: "consensus".to_string(),
            credit_policy: "conservative".to_string(),
            currency: "hours".to_string(),
        }
    }
}

/// Cooperative namespace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coop {
    pub id: CoopId,
    pub name: String,
    pub members: Vec<CoopMember>,
    pub settings: CoopSettings,
    pub created_at: u64,
}

impl Coop {
    /// Create a new cooperative
    pub fn new(id: CoopId, name: String, owner: Did, created_at: u64) -> Self {
        Self {
            id,
            name,
            members: vec![CoopMember {
                did: owner,
                role: MemberRole::Owner,
                joined_at: created_at,
            }],
            settings: CoopSettings::default(),
            created_at,
        }
    }

    /// Check if a DID is a member
    pub fn is_member(&self, did: &Did) -> bool {
        self.members.iter().any(|m| &m.did == did)
    }

    /// Get a member's role
    pub fn get_role(&self, did: &Did) -> Option<MemberRole> {
        self.members
            .iter()
            .find(|m| &m.did == did)
            .map(|m| m.role.clone())
    }

    /// Check if a DID has a specific role or higher
    pub fn has_role(&self, did: &Did, required_role: MemberRole) -> bool {
        match self.get_role(did) {
            Some(MemberRole::Owner) => true, // Owner can do everything
            Some(MemberRole::Admin) => required_role != MemberRole::Owner,
            Some(MemberRole::Member) => required_role == MemberRole::Member,
            None => false,
        }
    }

    /// Add a member
    pub fn add_member(&mut self, did: Did, role: MemberRole, timestamp: u64) -> Result<()> {
        if self.is_member(&did) {
            return Err(GatewayError::BadRequest("Member already exists".to_string()));
        }

        self.members.push(CoopMember {
            did,
            role,
            joined_at: timestamp,
        });

        Ok(())
    }

    /// Remove a member
    pub fn remove_member(&mut self, did: &Did) -> Result<()> {
        let initial_len = self.members.len();
        self.members.retain(|m| &m.did != did);

        if self.members.len() == initial_len {
            return Err(GatewayError::NotFound("Member not found".to_string()));
        }

        // Ensure at least one owner remains
        if !self.members.iter().any(|m| m.role == MemberRole::Owner) {
            return Err(GatewayError::BadRequest(
                "Cannot remove last owner".to_string(),
            ));
        }

        Ok(())
    }

    /// Update member role
    pub fn update_role(&mut self, did: &Did, new_role: MemberRole) -> Result<()> {
        // Find member and get current role
        let current_role = self
            .members
            .iter()
            .find(|m| &m.did == did)
            .map(|m| m.role.clone())
            .ok_or_else(|| GatewayError::NotFound("Member not found".to_string()))?;

        // If demoting an owner, ensure at least one owner remains
        if current_role == MemberRole::Owner && new_role != MemberRole::Owner {
            let owner_count = self.members.iter().filter(|m| m.role == MemberRole::Owner).count();
            if owner_count <= 1 {
                return Err(GatewayError::BadRequest(
                    "Cannot demote last owner".to_string(),
                ));
            }
        }

        // Update role
        let member = self
            .members
            .iter_mut()
            .find(|m| &m.did == did)
            .unwrap(); // Safe: we already checked it exists

        member.role = new_role;
        Ok(())
    }
}

/// Cooperative namespace manager
pub struct CoopManager {
    coops: Arc<RwLock<HashMap<CoopId, Coop>>>,
}

impl Default for CoopManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CoopManager {
    /// Create a new coop manager
    pub fn new() -> Self {
        Self {
            coops: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new cooperative
    pub fn create_coop(&self, id: CoopId, name: String, owner: Did, timestamp: u64) -> Result<()> {
        let mut coops = self.coops.write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        if coops.contains_key(&id) {
            return Err(GatewayError::BadRequest("Coop ID already exists".to_string()));
        }

        let coop = Coop::new(id.clone(), name, owner, timestamp);
        coops.insert(id, coop);

        Ok(())
    }

    /// Get a cooperative
    pub fn get_coop(&self, id: &CoopId) -> Result<Coop> {
        let coops = self.coops.read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        coops
            .get(id)
            .cloned()
            .ok_or_else(|| GatewayError::NotFound("Coop not found".to_string()))
    }

    /// Update a cooperative
    pub fn update_coop(&self, id: &CoopId, coop: Coop) -> Result<()> {
        let mut coops = self.coops.write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        if !coops.contains_key(id) {
            return Err(GatewayError::NotFound("Coop not found".to_string()));
        }

        coops.insert(id.clone(), coop);
        Ok(())
    }

    /// Delete a cooperative
    pub fn delete_coop(&self, id: &CoopId) -> Result<()> {
        let mut coops = self.coops.write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        coops
            .remove(id)
            .ok_or_else(|| GatewayError::NotFound("Coop not found".to_string()))?;

        Ok(())
    }

    /// List all cooperatives (for testing/admin)
    pub fn list_coops(&self) -> Result<Vec<Coop>> {
        let coops = self.coops.read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        Ok(coops.values().cloned().collect())
    }

    /// List all cooperative IDs (for cleanup tasks)
    pub fn list_all_coop_ids(&self) -> Result<Vec<CoopId>> {
        let coops = self.coops.read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        Ok(coops.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;

    fn timestamp() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn test_create_coop() {
        let manager = CoopManager::new();
        let owner = IdentityBundle::generate().unwrap();

        manager
            .create_coop(
                "test-coop".to_string(),
                "Test Coop".to_string(),
                owner.did().clone(),
                timestamp(),
            )
            .unwrap();

        let coop = manager.get_coop(&"test-coop".to_string()).unwrap();
        assert_eq!(coop.id, "test-coop");
        assert_eq!(coop.name, "Test Coop");
        assert_eq!(coop.members.len(), 1);
        assert_eq!(coop.members[0].role, MemberRole::Owner);
    }

    #[test]
    fn test_add_member() {
        let manager = CoopManager::new();
        let owner = IdentityBundle::generate().unwrap();
        let member = IdentityBundle::generate().unwrap();

        manager
            .create_coop(
                "test-coop".to_string(),
                "Test Coop".to_string(),
                owner.did().clone(),
                timestamp(),
            )
            .unwrap();

        let mut coop = manager.get_coop(&"test-coop".to_string()).unwrap();
        coop.add_member(member.did().clone(), MemberRole::Member, timestamp())
            .unwrap();

        assert_eq!(coop.members.len(), 2);
        assert!(coop.is_member(member.did()));
    }

    #[test]
    fn test_remove_member() {
        let manager = CoopManager::new();
        let owner = IdentityBundle::generate().unwrap();
        let member = IdentityBundle::generate().unwrap();

        manager
            .create_coop(
                "test-coop".to_string(),
                "Test Coop".to_string(),
                owner.did().clone(),
                timestamp(),
            )
            .unwrap();

        let mut coop = manager.get_coop(&"test-coop".to_string()).unwrap();
        coop.add_member(member.did().clone(), MemberRole::Member, timestamp())
            .unwrap();
        coop.remove_member(member.did()).unwrap();

        assert_eq!(coop.members.len(), 1);
        assert!(!coop.is_member(member.did()));
    }

    #[test]
    fn test_cannot_remove_last_owner() {
        let manager = CoopManager::new();
        let owner = IdentityBundle::generate().unwrap();

        manager
            .create_coop(
                "test-coop".to_string(),
                "Test Coop".to_string(),
                owner.did().clone(),
                timestamp(),
            )
            .unwrap();

        let mut coop = manager.get_coop(&"test-coop".to_string()).unwrap();
        let result = coop.remove_member(owner.did());

        assert!(matches!(result, Err(GatewayError::BadRequest(_))));
    }

    #[test]
    fn test_role_check() {
        let owner = IdentityBundle::generate().unwrap();
        let admin = IdentityBundle::generate().unwrap();
        let member = IdentityBundle::generate().unwrap();

        let mut coop = Coop::new(
            "test".to_string(),
            "Test".to_string(),
            owner.did().clone(),
            timestamp(),
        );

        coop.add_member(admin.did().clone(), MemberRole::Admin, timestamp())
            .unwrap();
        coop.add_member(member.did().clone(), MemberRole::Member, timestamp())
            .unwrap();

        // Owner can do everything
        assert!(coop.has_role(owner.did(), MemberRole::Owner));
        assert!(coop.has_role(owner.did(), MemberRole::Admin));
        assert!(coop.has_role(owner.did(), MemberRole::Member));

        // Admin cannot do owner actions
        assert!(!coop.has_role(admin.did(), MemberRole::Owner));
        assert!(coop.has_role(admin.did(), MemberRole::Admin));
        assert!(coop.has_role(admin.did(), MemberRole::Member));

        // Member can only do member actions
        assert!(!coop.has_role(member.did(), MemberRole::Owner));
        assert!(!coop.has_role(member.did(), MemberRole::Admin));
        assert!(coop.has_role(member.did(), MemberRole::Member));
    }
}
