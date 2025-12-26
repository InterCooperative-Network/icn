//! CommunityHandle - Async handle for sending messages to CommunityActor
//!
//! Provides a convenient async API for interacting with the community actor.

use crate::actor::CommunityMessage;
use crate::types::{Community, CommunityType, Member, MemberType, ResourcePool};
use crate::{CommunityError, Result};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

/// Handle for sending messages to the CommunityActor
#[derive(Clone)]
pub struct CommunityHandle {
    tx: mpsc::Sender<CommunityMessage>,
}

impl CommunityHandle {
    /// Create a new handle from a message sender
    pub fn new(tx: mpsc::Sender<CommunityMessage>) -> Self {
        Self { tx }
    }

    /// Create a new community
    pub async fn create(
        &self,
        id: Option<String>,
        name: String,
        community_type: CommunityType,
        founder_id: String,
        founder_type: MemberType,
        charter: String,
    ) -> Result<Community> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::Create {
                id,
                name,
                community_type,
                founder_id,
                founder_type,
                charter,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Get a community by ID
    pub async fn get(&self, community_id: String) -> Result<Community> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::Get {
                community_id,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// List all communities
    pub async fn list(&self) -> Result<Vec<Community>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::List { reply })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Activate a forming community
    pub async fn activate(&self, community_id: String) -> Result<Community> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::Activate {
                community_id,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Dissolve a community
    pub async fn dissolve(&self, community_id: String) -> Result<Community> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::Dissolve {
                community_id,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Join a community
    pub async fn join(
        &self,
        community_id: String,
        member_id: String,
        member_type: MemberType,
    ) -> Result<Community> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::Join {
                community_id,
                member_id,
                member_type,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Leave a community
    pub async fn leave(&self, community_id: String, member_id: String) -> Result<Community> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::Leave {
                community_id,
                member_id,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// List members of a community
    pub async fn list_members(&self, community_id: String) -> Result<Vec<Member>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::ListMembers {
                community_id,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Allocate resources from a pool
    pub async fn allocate_resource(
        &self,
        community_id: String,
        pool_name: String,
        recipient: String,
        amount: u64,
    ) -> Result<Community> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::AllocateResource {
                community_id,
                pool_name,
                recipient,
                amount,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Get resource pools for a community
    pub async fn get_resource_pools(&self, community_id: String) -> Result<Vec<ResourcePool>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::GetResourcePools {
                community_id,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Update a community
    pub async fn update(
        &self,
        community_id: String,
        name: Option<String>,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<Community> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::Update {
                community_id,
                name,
                metadata,
                reply,
            })
            .await
            .map_err(|_| CommunityError::Governance("Actor disconnected".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Reply failed".into()))?
    }

    /// Gracefully shutdown the actor
    ///
    /// Sends a shutdown message to the actor and waits for confirmation.
    /// This allows the actor to complete any in-flight operations before stopping.
    pub async fn shutdown(&self) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(CommunityMessage::Shutdown { reply })
            .await
            .map_err(|_| CommunityError::Governance("Actor already stopped".into()))?;
        rx.await
            .map_err(|_| CommunityError::Governance("Shutdown confirmation failed".into()))?;
        Ok(())
    }
}
