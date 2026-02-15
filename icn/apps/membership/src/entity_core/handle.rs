//! EntityHandle provides an async API for interacting with the EntityActor
//!
//! This handle wraps the message passing to provide a clean async interface
//! for entity registry operations.

use super::actor::EntityMessage;
use super::entity::{CooperativeEntity, EntityId, EntityType};
use super::error::{EntityError, Result};
use super::membership::Membership;
use tokio::sync::{mpsc, oneshot};

/// Error returned when the actor is disconnected
fn actor_disconnected() -> EntityError {
    EntityError::RegistryError("Entity actor disconnected".into())
}

/// Error returned when reply channel fails
fn reply_failed() -> EntityError {
    EntityError::RegistryError("Reply channel failed".into())
}

/// Handle for interacting with the EntityActor
///
/// This handle is cheap to clone and can be shared across tasks.
#[derive(Clone)]
pub struct EntityHandle {
    tx: mpsc::Sender<EntityMessage>,
}

impl EntityHandle {
    /// Create a new handle from a message sender
    pub fn new(tx: mpsc::Sender<EntityMessage>) -> Self {
        Self { tx }
    }

    /// Register a new entity
    pub async fn register(&self, entity: CooperativeEntity) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::Register { entity, reply })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Get an entity by ID
    pub async fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::Get {
                id: id.clone(),
                reply,
            })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Update an existing entity
    pub async fn update(&self, entity: CooperativeEntity) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::Update { entity, reply })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Delete an entity
    pub async fn delete(&self, id: &EntityId) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::Delete {
                id: id.clone(),
                reply,
            })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Check if an entity exists
    pub async fn exists(&self, id: &EntityId) -> Result<bool> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::Exists {
                id: id.clone(),
                reply,
            })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Add a membership relationship
    pub async fn add_membership(&self, membership: Membership) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::AddMembership { membership, reply })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Remove a membership relationship
    pub async fn remove_membership(
        &self,
        member_id: &EntityId,
        parent_id: &EntityId,
    ) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::RemoveMembership {
                member_id: member_id.clone(),
                parent_id: parent_id.clone(),
                reply,
            })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Update a membership
    pub async fn update_membership(&self, membership: Membership) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::UpdateMembership { membership, reply })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Get members of an entity
    pub async fn get_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::GetMembers {
                parent_id: parent_id.clone(),
                reply,
            })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Get memberships of an entity (what organizations they belong to)
    pub async fn get_memberships_of(&self, member_id: &EntityId) -> Result<Vec<Membership>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::GetMembershipsOf {
                member_id: member_id.clone(),
                reply,
            })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Get a specific membership relationship
    pub async fn get_membership(
        &self,
        member_id: &EntityId,
        parent_id: &EntityId,
    ) -> Result<Option<Membership>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::GetMembership {
                member_id: member_id.clone(),
                parent_id: parent_id.clone(),
                reply,
            })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// List entities by type
    pub async fn list_by_type(&self, entity_type: EntityType) -> Result<Vec<EntityId>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::ListByType { entity_type, reply })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Count entities in the registry
    pub async fn count(&self) -> Result<usize> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::Count { reply })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Get member count for an entity
    pub async fn member_count(&self, parent_id: &EntityId) -> Result<usize> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::MemberCount {
                parent_id: parent_id.clone(),
                reply,
            })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }

    /// Apply an update from gossip (no re-broadcast)
    ///
    /// This is used by the gossip notification handler to apply updates
    /// received from other nodes without triggering another broadcast.
    pub async fn apply_gossip_update(&self, entity: CooperativeEntity) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(EntityMessage::ApplyGossipUpdate { entity, reply })
            .await
            .map_err(|_| actor_disconnected())?;
        rx.await.map_err(|_| reply_failed())?
    }
}
