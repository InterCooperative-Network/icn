//! Community manager adapter for gateway
//!
//! This module provides an adapter between the gateway's API
//! and the CommunityActor from icn-community.

use crate::error::{GatewayError, Result};
use icn_community::{Community, CommunityHandle, CommunityType, MemberType, ResourcePool};

/// Community manager that uses CommunityActor via handle
pub struct CommunityManager {
    handle: CommunityHandle,
}

impl CommunityManager {
    /// Create a new manager from CommunityHandle
    pub fn new(handle: CommunityHandle) -> Self {
        Self { handle }
    }

    /// Create a new community
    pub async fn create_community(
        &self,
        id: Option<String>,
        name: String,
        community_type: CommunityType,
        founder_id: String,
        founder_type: MemberType,
        charter: String,
    ) -> Result<Community> {
        self.handle
            .create(id, name, community_type, founder_id, founder_type, charter)
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Get a community by ID
    pub async fn get_community(&self, id: &str) -> Result<Community> {
        self.handle
            .get(id.to_string())
            .await
            .map_err(|e| GatewayError::NotFound(format!("Community not found: {e}")))
    }

    /// List all communities
    pub async fn list_communities(&self) -> Result<Vec<Community>> {
        self.handle
            .list()
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Activate a forming community
    pub async fn activate_community(&self, id: &str) -> Result<Community> {
        self.handle
            .activate(id.to_string())
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Dissolve a community
    pub async fn dissolve_community(&self, id: &str) -> Result<Community> {
        self.handle
            .dissolve(id.to_string())
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Join a community
    pub async fn join_community(
        &self,
        community_id: &str,
        member_id: String,
        member_type: MemberType,
    ) -> Result<Community> {
        self.handle
            .join(community_id.to_string(), member_id, member_type)
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Leave a community
    pub async fn leave_community(&self, community_id: &str, member_id: &str) -> Result<Community> {
        self.handle
            .leave(community_id.to_string(), member_id.to_string())
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// List members of a community
    pub async fn list_members(
        &self,
        community_id: &str,
    ) -> Result<Vec<icn_community::types::Member>> {
        self.handle
            .list_members(community_id.to_string())
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Allocate resources from a pool
    pub async fn allocate_resource(
        &self,
        community_id: &str,
        pool_name: String,
        recipient: String,
        amount: u64,
    ) -> Result<Community> {
        self.handle
            .allocate_resource(community_id.to_string(), pool_name, recipient, amount)
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Get resource pools for a community
    pub async fn get_resource_pools(&self, community_id: &str) -> Result<Vec<ResourcePool>> {
        self.handle
            .get_resource_pools(community_id.to_string())
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Update a community
    pub async fn update_community(
        &self,
        community_id: &str,
        name: Option<String>,
        metadata: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Community> {
        self.handle
            .update(community_id.to_string(), name, metadata)
            .await
            .map_err(|e| GatewayError::InternalError(format!("CommunityActor error: {e}")))
    }

    /// Count communities
    pub async fn count(&self) -> Result<usize> {
        let communities = self.list_communities().await?;
        Ok(communities.len())
    }
}
