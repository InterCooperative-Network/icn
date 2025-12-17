//! Cooperative manager adapter for gateway
//!
//! This module provides an adapter between the gateway's CoopManager API
//! and the CoopActor from icn-coop. It converts between gateway types
//! and actor types, providing a smooth migration path.

use crate::coop::{Coop, CoopId, CoopMember, CoopSettings, MemberRole};
use crate::error::{GatewayError, Result};
use icn_coop::CoopHandle;
use icn_identity::Did;

/// Cooperative manager that uses CoopActor via handle
pub struct ActorCoopManager {
    handle: CoopHandle,
}

impl ActorCoopManager {
    /// Create a new manager from CoopHandle
    pub fn new(handle: CoopHandle) -> Self {
        Self { handle }
    }

    /// Create a new cooperative
    pub async fn create_coop(
        &self,
        _id: CoopId,
        name: String,
        owner: Did,
        _timestamp: u64,
    ) -> Result<()> {
        // Map gateway CoopType to icn-coop CoopType
        // For now, default to Worker type
        let coop_type = icn_coop::CoopType::Worker;

        // Create cooperative via actor
        let _coop = self
            .handle
            .create_cooperative(name, coop_type, owner)
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        // Note: The actor generates its own ID, but gateway wants to use provided ID
        // TODO: Update CoopActor to accept optional ID parameter
        
        Ok(())
    }

    /// Get a cooperative
    pub async fn get_coop(&self, id: &CoopId) -> Result<Coop> {
        let actor_coop = self
            .handle
            .get_cooperative(id.clone())
            .await
            .map_err(|e| GatewayError::NotFound(format!("Coop not found: {e}")))?;

        // Convert icn-coop::Cooperative to gateway::Coop
        Ok(convert_to_gateway_coop(actor_coop))
    }

    /// List all cooperatives
    pub async fn list_coops(&self) -> Result<Vec<Coop>> {
        let actor_coops = self
            .handle
            .list_cooperatives()
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        Ok(actor_coops.into_iter().map(convert_to_gateway_coop).collect())
    }

    /// Delete a cooperative
    pub async fn delete_coop(&self, _id: &CoopId) -> Result<()> {
        // TODO: Add delete method to CoopActor
        Err(GatewayError::InternalError(
            "Delete not yet implemented in CoopActor".to_string(),
        ))
    }

    /// Count cooperatives
    pub async fn count(&self) -> Result<usize> {
        let coops = self.list_coops().await?;
        Ok(coops.len())
    }

    /// Add member to cooperative
    pub async fn add_member_atomic(
        &self,
        coop_id: &CoopId,
        did: Did,
        role: MemberRole,
        _timestamp: u64,
    ) -> Result<Coop> {
        // Map gateway MemberRole to icn-coop MemberRole
        let actor_role = match role {
            MemberRole::Steward => icn_coop::MemberRole::Founder,
            MemberRole::Facilitator => icn_coop::MemberRole::Officer,
            MemberRole::Participant => icn_coop::MemberRole::Member,
        };

        let _member = self
            .handle
            .add_member(coop_id.clone(), did, actor_role)
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        // Return updated coop
        self.get_coop(coop_id).await
    }

    /// List members of a cooperative
    pub async fn list_members(&self, coop_id: &CoopId) -> Result<Vec<CoopMember>> {
        let members = self
            .handle
            .list_members(coop_id.clone())
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        Ok(members.into_iter().map(convert_to_gateway_member).collect())
    }

    /// Remove member from cooperative
    pub async fn remove_member_atomic(&self, _coop_id: &CoopId, _did: &Did) -> Result<Coop> {
        // TODO: Add remove_member method to CoopActor
        Err(GatewayError::InternalError(
            "Remove member not yet implemented".to_string(),
        ))
    }

    /// Update member role
    pub async fn update_role_atomic(
        &self,
        _coop_id: &CoopId,
        _did: &Did,
        _new_role: MemberRole,
    ) -> Result<Coop> {
        // TODO: Add update_role method to CoopActor
        Err(GatewayError::InternalError(
            "Update role not yet implemented".to_string(),
        ))
    }

    /// Update cooperative settings
    pub async fn update_settings_atomic<F>(&self, _coop_id: &CoopId, _updater: F) -> Result<Coop>
    where
        F: FnOnce(&mut CoopSettings) -> Result<()>,
    {
        // TODO: Add update_settings method to CoopActor
        Err(GatewayError::InternalError(
            "Update settings not yet implemented".to_string(),
        ))
    }

    /// Update cooperative
    pub async fn update_coop(&self, _id: &CoopId, _coop: Coop) -> Result<()> {
        // TODO: Add update method to CoopActor
        Err(GatewayError::InternalError(
            "Update coop not yet implemented".to_string(),
        ))
    }

    /// List all coop IDs
    pub async fn list_all_coop_ids(&self) -> Result<Vec<CoopId>> {
        let coops = self.list_coops().await?;
        Ok(coops.into_iter().map(|c| c.id).collect())
    }
}

/// Convert icn-coop::Cooperative to gateway::Coop
fn convert_to_gateway_coop(actor_coop: icn_coop::Cooperative) -> Coop {
    // Create a placeholder DID - in reality we'd query members separately
    // This is a temporary limitation until we integrate member queries
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

/// Convert icn-coop::Member to gateway::CoopMember
fn convert_to_gateway_member(actor_member: icn_coop::Member) -> CoopMember {
    let role = match actor_member.role {
        icn_coop::MemberRole::Founder => MemberRole::Steward,
        icn_coop::MemberRole::Officer => MemberRole::Facilitator,
        icn_coop::MemberRole::BoardMember => MemberRole::Facilitator,
        _ => MemberRole::Participant,
    };

    CoopMember {
        did: actor_member.did,
        role,
        joined_at: actor_member.joined_at.timestamp() as u64,
    }
}
