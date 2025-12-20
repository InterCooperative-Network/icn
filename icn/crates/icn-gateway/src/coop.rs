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
///
/// Role names reflect cooperative values:
/// - **Steward**: Primary caretaker with full administrative authority
/// - **Facilitator**: Helps coordinate activities, can manage members
/// - **Participant**: Active member of the cooperative
///
/// For backwards compatibility, the legacy names (Owner/Admin/Member) are
/// accepted when parsing from JSON or API requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemberRole {
    /// Primary caretaker with full administrative authority (formerly "Owner")
    #[serde(alias = "Owner", alias = "owner")]
    Steward,
    /// Coordinator who can manage members (formerly "Admin")
    #[serde(alias = "Admin", alias = "admin")]
    Facilitator,
    /// Active cooperative member (formerly "Member")
    #[serde(alias = "Member", alias = "member")]
    Participant,
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
    pub credit_policy: String,    // e.g., "conservative", "permissive"
    pub currency: String,         // e.g., "hours", "USD"
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
    pub fn new(id: CoopId, name: String, steward: Did, created_at: u64) -> Self {
        Self {
            id,
            name,
            members: vec![CoopMember {
                did: steward,
                role: MemberRole::Steward,
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
            Some(MemberRole::Steward) => true, // Steward can do everything
            Some(MemberRole::Facilitator) => required_role != MemberRole::Steward,
            Some(MemberRole::Participant) => required_role == MemberRole::Participant,
            None => false,
        }
    }

    /// Add a member
    pub fn add_member(&mut self, did: Did, role: MemberRole, timestamp: u64) -> Result<()> {
        // Check member limit BEFORE checking if member exists
        // This prevents TOCTOU race in concurrent add_member calls
        crate::validation::validate_member_count(self.members.len())?;

        if self.is_member(&did) {
            return Err(GatewayError::BadRequest(
                "Member already exists".to_string(),
            ));
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

        // Ensure at least one steward remains
        if !self.members.iter().any(|m| m.role == MemberRole::Steward) {
            return Err(GatewayError::BadRequest(
                "Cannot remove last steward".to_string(),
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

        // If demoting a steward, ensure at least one steward remains
        if current_role == MemberRole::Steward && new_role != MemberRole::Steward {
            let steward_count = self
                .members
                .iter()
                .filter(|m| m.role == MemberRole::Steward)
                .count();
            if steward_count <= 1 {
                return Err(GatewayError::BadRequest(
                    "Cannot demote last steward".to_string(),
                ));
            }
        }

        // Update role
        let member = self.members.iter_mut().find(|m| &m.did == did).unwrap(); // Safe: we already checked it exists

        member.role = new_role;
        Ok(())
    }
}

/// Cooperative namespace manager
pub struct CoopManager {
    coops: Arc<RwLock<HashMap<CoopId, Coop>>>,
    /// Cooperative handle from daemon (if connected)
    coop_handle: Option<icn_coop::CoopHandle>,
}

impl Default for CoopManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CoopManager {
    /// Create a new coop manager (in-memory only)
    pub fn new() -> Self {
        Self {
            coops: Arc::new(RwLock::new(HashMap::new())),
            coop_handle: None,
        }
    }

    /// Create a coop manager connected to daemon
    pub fn with_handle(handle: icn_coop::CoopHandle) -> Self {
        Self {
            coops: Arc::new(RwLock::new(HashMap::new())),
            coop_handle: Some(handle),
        }
    }

    /// Create a new cooperative
    pub async fn create_coop(
        &self,
        id: CoopId,
        name: String,
        owner: Did,
        timestamp: u64,
    ) -> Result<()> {
        // If we have a daemon handle, use it for persistent storage
        if let Some(ref handle) = self.coop_handle {
            // Use actor for persistence with explicit ID from gateway
            let coop_type = icn_coop::CoopType::Worker; // Default type
            let _cooperative = handle
                .create_cooperative(Some(id.clone()), name.clone(), coop_type, owner.clone())
                .await
                .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

            // Also store in local cache for fast lookup
            // This allows get_coop to succeed without hitting the actor every time
        }

        // Store in local cache (used when no daemon, or for fast lookup)
        let mut coops = self
            .coops
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        // Check global cooperative limit atomically (while holding write lock)
        // This prevents TOCTOU race where multiple threads check limit concurrently
        crate::validation::validate_coop_count(coops.len())?;

        // Use entry API for atomic check-and-insert (prevents any potential race)
        use std::collections::hash_map::Entry;
        match coops.entry(id.clone()) {
            Entry::Vacant(e) => {
                let coop = Coop::new(id, name, owner, timestamp);
                e.insert(coop);
                Ok(())
            }
            Entry::Occupied(_) => Err(GatewayError::BadRequest(
                "Coop ID already exists".to_string(),
            )),
        }
    }

    /// Get a cooperative
    pub async fn get_coop(&self, id: &CoopId) -> Result<Coop> {
        // Try daemon first if available
        if let Some(ref handle) = self.coop_handle {
            match handle.get_cooperative(id.clone()).await {
                Ok(actor_coop) => {
                    // Convert and return with member list
                    return self
                        .convert_actor_coop_with_members(handle, actor_coop)
                        .await;
                }
                Err(_) => {
                    // Fall through to local cache
                }
            }
        }

        // Fall back to local cache
        let coops = self
            .coops
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        coops
            .get(id)
            .cloned()
            .ok_or_else(|| GatewayError::NotFound("Coop not found".to_string()))
    }

    /// Update a cooperative
    pub fn update_coop(&self, id: &CoopId, coop: Coop) -> Result<()> {
        let mut coops = self
            .coops
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        if !coops.contains_key(id) {
            return Err(GatewayError::NotFound("Coop not found".to_string()));
        }

        coops.insert(id.clone(), coop);
        Ok(())
    }

    /// Delete a cooperative
    pub fn delete_coop(&self, id: &CoopId) -> Result<()> {
        let mut coops = self
            .coops
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        coops
            .remove(id)
            .ok_or_else(|| GatewayError::NotFound("Coop not found".to_string()))?;

        Ok(())
    }

    /// List all cooperatives (for testing/admin)
    pub async fn list_coops(&self) -> Result<Vec<Coop>> {
        // Try daemon first if available
        if let Some(ref handle) = self.coop_handle {
            match handle.list_cooperatives().await {
                Ok(actor_coops) => {
                    return Ok(actor_coops
                        .into_iter()
                        .map(convert_actor_coop_to_gateway)
                        .collect());
                }
                Err(_) => {
                    // Fall through to local cache
                }
            }
        }

        // Fall back to local cache
        let coops = self
            .coops
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        Ok(coops.values().cloned().collect())
    }

    /// List all cooperative IDs (for cleanup tasks)
    pub fn list_all_coop_ids(&self) -> Result<Vec<CoopId>> {
        let coops = self
            .coops
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        Ok(coops.keys().cloned().collect())
    }

    /// Get total number of cooperatives
    pub fn count(&self) -> Result<usize> {
        let coops = self
            .coops
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        Ok(coops.len())
    }

    /// Atomically add a member to a cooperative
    /// This prevents race conditions by holding the write lock during the entire operation
    pub fn add_member_atomic(
        &self,
        coop_id: &CoopId,
        did: Did,
        role: MemberRole,
        timestamp: u64,
    ) -> Result<Coop> {
        let mut coops = self
            .coops
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let coop = coops
            .get_mut(coop_id)
            .ok_or_else(|| GatewayError::NotFound("Coop not found".to_string()))?;

        coop.add_member(did, role, timestamp)?;

        Ok(coop.clone())
    }

    /// Atomically remove a member from a cooperative
    pub fn remove_member_atomic(&self, coop_id: &CoopId, did: &Did) -> Result<Coop> {
        let mut coops = self
            .coops
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let coop = coops
            .get_mut(coop_id)
            .ok_or_else(|| GatewayError::NotFound("Coop not found".to_string()))?;

        coop.remove_member(did)?;

        Ok(coop.clone())
    }

    /// Atomically update a member's role
    pub fn update_role_atomic(
        &self,
        coop_id: &CoopId,
        did: &Did,
        new_role: MemberRole,
    ) -> Result<Coop> {
        let mut coops = self
            .coops
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let coop = coops
            .get_mut(coop_id)
            .ok_or_else(|| GatewayError::NotFound("Coop not found".to_string()))?;

        coop.update_role(did, new_role)?;

        Ok(coop.clone())
    }

    /// Atomically update cooperative settings
    pub fn update_settings_atomic<F>(&self, coop_id: &CoopId, updater: F) -> Result<Coop>
    where
        F: FnOnce(&mut Coop) -> Result<()>,
    {
        let mut coops = self
            .coops
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let coop = coops
            .get_mut(coop_id)
            .ok_or_else(|| GatewayError::NotFound("Coop not found".to_string()))?;

        updater(coop)?;

        Ok(coop.clone())
    }

    /// Convert actor Cooperative to gateway Coop with member list populated
    async fn convert_actor_coop_with_members(
        &self,
        handle: &icn_coop::CoopHandle,
        actor_coop: icn_coop::Cooperative,
    ) -> Result<Coop> {
        // Query members from actor
        let actor_members = handle
            .list_members(actor_coop.id.clone())
            .await
            .unwrap_or_else(|_| Vec::new());

        // Convert actor members to gateway members
        let members = actor_members
            .into_iter()
            .map(|m| CoopMember {
                did: m.did,
                role: convert_actor_role_to_gateway(&m.role),
                joined_at: m.joined_at.timestamp() as u64,
            })
            .collect();

        Ok(Coop {
            id: actor_coop.id,
            name: actor_coop.name,
            members,
            settings: CoopSettings::default(),
            created_at: actor_coop.created_at.timestamp() as u64,
        })
    }
}

/// Convert icn_coop::Cooperative to gateway::Coop
fn convert_actor_coop_to_gateway(actor_coop: icn_coop::Cooperative) -> Coop {
    // TODO: Query members separately for accurate member list
    // For now, create placeholder with founder
    let placeholder_did: Did = serde_json::from_str("\"did:icn:placeholder\"").unwrap();

    Coop {
        id: actor_coop.id,
        name: actor_coop.name,
        members: vec![CoopMember {
            did: placeholder_did,
            role: MemberRole::Steward,
            joined_at: actor_coop.created_at.timestamp() as u64,
        }],
        settings: CoopSettings::default(),
        created_at: actor_coop.created_at.timestamp() as u64,
    }
}

/// Convert actor role to gateway role
fn convert_actor_role_to_gateway(role: &icn_coop::MemberRole) -> MemberRole {
    match role {
        icn_coop::MemberRole::Founder => MemberRole::Steward,
        icn_coop::MemberRole::Officer => MemberRole::Facilitator,
        icn_coop::MemberRole::BoardMember => MemberRole::Facilitator,
        icn_coop::MemberRole::Member => MemberRole::Participant,
        icn_coop::MemberRole::Worker => MemberRole::Participant,
        icn_coop::MemberRole::Consumer => MemberRole::Participant,
        icn_coop::MemberRole::Producer => MemberRole::Participant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;

    fn timestamp() -> u64 {
        icn_time::current_timestamp_secs()
    }

    #[tokio::test]
    async fn test_create_coop() {
        let manager = CoopManager::new();
        let steward = IdentityBundle::generate().unwrap();

        manager
            .create_coop(
                "test-coop".to_string(),
                "Test Coop".to_string(),
                steward.did().clone(),
                timestamp(),
            )
            .await
            .unwrap();

        let coop = manager.get_coop(&"test-coop".to_string()).await.unwrap();
        assert_eq!(coop.id, "test-coop");
        assert_eq!(coop.name, "Test Coop");
        assert_eq!(coop.members.len(), 1);
        assert_eq!(coop.members[0].role, MemberRole::Steward);
    }

    #[tokio::test]
    async fn test_add_member() {
        let manager = CoopManager::new();
        let steward = IdentityBundle::generate().unwrap();
        let participant = IdentityBundle::generate().unwrap();

        manager
            .create_coop(
                "test-coop".to_string(),
                "Test Coop".to_string(),
                steward.did().clone(),
                timestamp(),
            )
            .await
            .unwrap();

        let mut coop = manager.get_coop(&"test-coop".to_string()).await.unwrap();
        coop.add_member(
            participant.did().clone(),
            MemberRole::Participant,
            timestamp(),
        )
        .unwrap();

        assert_eq!(coop.members.len(), 2);
        assert!(coop.is_member(participant.did()));
    }

    #[tokio::test]
    async fn test_remove_member() {
        let manager = CoopManager::new();
        let steward = IdentityBundle::generate().unwrap();
        let participant = IdentityBundle::generate().unwrap();

        manager
            .create_coop(
                "test-coop".to_string(),
                "Test Coop".to_string(),
                steward.did().clone(),
                timestamp(),
            )
            .await
            .unwrap();

        let mut coop = manager.get_coop(&"test-coop".to_string()).await.unwrap();
        coop.add_member(
            participant.did().clone(),
            MemberRole::Participant,
            timestamp(),
        )
        .unwrap();
        coop.remove_member(participant.did()).unwrap();

        assert_eq!(coop.members.len(), 1);
        assert!(!coop.is_member(participant.did()));
    }

    #[tokio::test]
    async fn test_cannot_remove_last_steward() {
        let manager = CoopManager::new();
        let steward = IdentityBundle::generate().unwrap();

        manager
            .create_coop(
                "test-coop".to_string(),
                "Test Coop".to_string(),
                steward.did().clone(),
                timestamp(),
            )
            .await
            .unwrap();

        let mut coop = manager.get_coop(&"test-coop".to_string()).await.unwrap();
        let result = coop.remove_member(steward.did());

        assert!(matches!(result, Err(GatewayError::BadRequest(_))));
    }

    #[test]
    fn test_role_check() {
        let steward = IdentityBundle::generate().unwrap();
        let facilitator = IdentityBundle::generate().unwrap();
        let participant = IdentityBundle::generate().unwrap();

        let mut coop = Coop::new(
            "test".to_string(),
            "Test".to_string(),
            steward.did().clone(),
            timestamp(),
        );

        coop.add_member(
            facilitator.did().clone(),
            MemberRole::Facilitator,
            timestamp(),
        )
        .unwrap();
        coop.add_member(
            participant.did().clone(),
            MemberRole::Participant,
            timestamp(),
        )
        .unwrap();

        // Steward can do everything
        assert!(coop.has_role(steward.did(), MemberRole::Steward));
        assert!(coop.has_role(steward.did(), MemberRole::Facilitator));
        assert!(coop.has_role(steward.did(), MemberRole::Participant));

        // Facilitator cannot do steward actions
        assert!(!coop.has_role(facilitator.did(), MemberRole::Steward));
        assert!(coop.has_role(facilitator.did(), MemberRole::Facilitator));
        assert!(coop.has_role(facilitator.did(), MemberRole::Participant));

        // Participant can only do participant actions
        assert!(!coop.has_role(participant.did(), MemberRole::Steward));
        assert!(!coop.has_role(participant.did(), MemberRole::Facilitator));
        assert!(coop.has_role(participant.did(), MemberRole::Participant));
    }
}
