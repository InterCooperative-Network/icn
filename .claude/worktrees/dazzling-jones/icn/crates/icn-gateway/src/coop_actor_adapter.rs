//! Cooperative manager adapter for gateway
//!
//! This module provides an adapter between the gateway's CoopManager API
//! and the CoopActor from icn-coop. It converts between gateway types
//! and actor types, providing a smooth migration path.

use crate::coop::{convert_gateway_role_to_actor, Coop, CoopId, CoopMember, CoopSettings, MemberRole};
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
        id: CoopId,
        name: String,
        owner: Did,
        _timestamp: u64,
    ) -> Result<()> {
        // Map gateway CoopType to icn-coop CoopType
        // For now, default to Worker type
        let coop_type = icn_coop::CoopType::Worker;

        // Create cooperative via actor with the provided ID
        let _coop = self
            .handle
            .create_cooperative(Some(id), name, coop_type, owner)
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        Ok(())
    }

    /// Get a cooperative with members
    pub async fn get_coop(&self, id: &CoopId) -> Result<Coop> {
        let actor_coop = self
            .handle
            .get_cooperative(id.clone())
            .await
            .map_err(|e| GatewayError::NotFound(format!("Coop not found: {e}")))?;

        // Query members for this coop
        let members = self
            .handle
            .list_members(id.clone())
            .await
            .unwrap_or_default();

        // Convert with actual members
        Ok(convert_to_gateway_coop_with_members(actor_coop, members))
    }

    /// List all cooperatives with members
    pub async fn list_coops(&self) -> Result<Vec<Coop>> {
        let actor_coops = self
            .handle
            .list_cooperatives()
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        // For each coop, query its members
        let mut coops = Vec::with_capacity(actor_coops.len());
        for actor_coop in actor_coops {
            let members = self
                .handle
                .list_members(actor_coop.id.clone())
                .await
                .unwrap_or_default();
            coops.push(convert_to_gateway_coop_with_members(actor_coop, members));
        }

        Ok(coops)
    }

    /// Delete a cooperative
    pub async fn delete_coop(&self, id: &CoopId) -> Result<()> {
        self.handle
            .delete_cooperative(id.clone())
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))
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
        timestamp: u64,
    ) -> Result<Coop> {
        let actor_role = convert_gateway_role_to_actor(&role);

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
    pub async fn remove_member_atomic(&self, coop_id: &CoopId, did: &Did) -> Result<Coop> {
        self.handle
            .remove_member(coop_id.clone(), did.clone())
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        // Return updated coop
        self.get_coop(coop_id).await
    }

    /// Update member role
    pub async fn update_role_atomic(
        &self,
        coop_id: &CoopId,
        did: &Did,
        new_role: MemberRole,
    ) -> Result<Coop> {
        let actor_role = convert_gateway_role_to_actor(&new_role);

        self.handle
            .update_member_role(coop_id.clone(), did.clone(), actor_role)
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        // Return updated coop
        self.get_coop(coop_id).await
    }

    /// Update cooperative settings
    pub async fn update_settings_atomic<F>(&self, coop_id: &CoopId, updater: F) -> Result<Coop>
    where
        F: FnOnce(&mut CoopSettings) -> Result<()>,
    {
        // Get current coop to get settings
        let current_coop = self.get_coop(coop_id).await?;
        let mut settings = current_coop.settings;

        // Apply updater
        updater(&mut settings)?;

        // Convert settings to metadata map
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("governance_model".to_string(), settings.governance_model);
        metadata.insert("credit_policy".to_string(), settings.credit_policy);
        metadata.insert("currency".to_string(), settings.currency);

        // Update via actor
        self.handle
            .update_cooperative(coop_id.clone(), None, Some(metadata))
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        // Return updated coop
        self.get_coop(coop_id).await
    }

    /// Update cooperative
    pub async fn update_coop(&self, id: &CoopId, coop: Coop) -> Result<()> {
        // Convert settings to metadata
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("governance_model".to_string(), coop.settings.governance_model);
        metadata.insert("credit_policy".to_string(), coop.settings.credit_policy);
        metadata.insert("currency".to_string(), coop.settings.currency);

        // Update name and metadata
        self.handle
            .update_cooperative(id.clone(), Some(coop.name), Some(metadata))
            .await
            .map_err(|e| GatewayError::InternalError(format!("CoopActor error: {e}")))?;

        Ok(())
    }

    /// List all coop IDs
    pub async fn list_all_coop_ids(&self) -> Result<Vec<CoopId>> {
        let coops = self.list_coops().await?;
        Ok(coops.into_iter().map(|c| c.id).collect())
    }
}

/// Convert icn-coop::Cooperative to gateway::Coop with member list
fn convert_to_gateway_coop_with_members(
    actor_coop: icn_coop::Cooperative,
    actor_members: Vec<icn_coop::Member>,
) -> Coop {
    // Convert members
    let members: Vec<CoopMember> = actor_members
        .into_iter()
        .map(convert_to_gateway_member)
        .collect();

    // Extract settings from metadata if present
    let settings = CoopSettings {
        governance_model: actor_coop
            .metadata
            .get("governance_model")
            .cloned()
            .unwrap_or_else(|| "consensus".to_string()),
        credit_policy: actor_coop
            .metadata
            .get("credit_policy")
            .cloned()
            .unwrap_or_else(|| "conservative".to_string()),
        currency: actor_coop
            .metadata
            .get("currency")
            .cloned()
            .unwrap_or_else(|| "hours".to_string()),
    };

    Coop {
        id: actor_coop.id,
        name: actor_coop.name,
        members,
        settings,
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
