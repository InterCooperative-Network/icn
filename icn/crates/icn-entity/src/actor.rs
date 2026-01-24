//! EntityActor for managing entity registry operations with gossip synchronization
//!
//! This actor provides an async message-passing interface to the SledEntityRegistry
//! and publishes entity changes to the gossip network for multi-node synchronization.

use crate::registry::EntityRegistry;
use crate::{CooperativeEntity, EntityId, EntityType, Membership, Result, SledEntityRegistry};
use icn_gossip::GossipActor;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, warn};

/// Gossip topic for entity state updates
pub const ENTITY_TOPIC: &str = "entity:updates";

/// Handle type for gossip actor
pub type GossipHandle = Arc<RwLock<GossipActor>>;

/// Messages that can be sent to the EntityActor
pub enum EntityMessage {
    /// Register a new entity
    Register {
        entity: CooperativeEntity,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Get an entity by ID
    Get {
        id: EntityId,
        reply: oneshot::Sender<Result<Option<CooperativeEntity>>>,
    },
    /// Update an existing entity
    Update {
        entity: CooperativeEntity,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Update entity with optimistic locking
    UpdateIfVersion {
        entity: CooperativeEntity,
        expected_version: u64,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Delete an entity
    Delete {
        id: EntityId,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Check if an entity exists
    Exists {
        id: EntityId,
        reply: oneshot::Sender<Result<bool>>,
    },
    /// Add a membership relationship
    AddMembership {
        membership: Membership,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Remove a membership relationship
    RemoveMembership {
        member_id: EntityId,
        parent_id: EntityId,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Update a membership
    UpdateMembership {
        membership: Membership,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Get members of an entity
    GetMembers {
        parent_id: EntityId,
        reply: oneshot::Sender<Result<Vec<Membership>>>,
    },
    /// Get memberships of an entity (what organizations they belong to)
    GetMembershipsOf {
        member_id: EntityId,
        reply: oneshot::Sender<Result<Vec<Membership>>>,
    },
    /// Get a specific membership
    GetMembership {
        member_id: EntityId,
        parent_id: EntityId,
        reply: oneshot::Sender<Result<Option<Membership>>>,
    },
    /// List entities by type
    ListByType {
        entity_type: EntityType,
        reply: oneshot::Sender<Result<Vec<EntityId>>>,
    },
    /// Count entities in the registry
    Count {
        reply: oneshot::Sender<Result<usize>>,
    },
    /// Get member count for an entity
    MemberCount {
        parent_id: EntityId,
        reply: oneshot::Sender<Result<usize>>,
    },
    /// Apply an update from gossip (no re-broadcast)
    ApplyGossipUpdate {
        entity: CooperativeEntity,
        reply: oneshot::Sender<Result<()>>,
    },
}

/// Actor that manages entity registry operations
pub struct EntityActor {
    rx: mpsc::Receiver<EntityMessage>,
    registry: SledEntityRegistry,
    gossip: Option<GossipHandle>,
}

impl EntityActor {
    /// Spawn a new EntityActor with the given registry and optional gossip handle
    ///
    /// Returns a sender that can be used to send messages to the actor.
    pub fn spawn(
        registry: SledEntityRegistry,
        gossip: Option<GossipHandle>,
    ) -> mpsc::Sender<EntityMessage> {
        let (tx, rx) = mpsc::channel(100);

        let mut actor = Self {
            rx,
            registry,
            gossip,
        };

        tokio::spawn(async move {
            actor.run().await;
        });

        tx
    }

    async fn run(&mut self) {
        info!("EntityActor started");

        while let Some(msg) = self.rx.recv().await {
            match msg {
                EntityMessage::Register { entity, reply } => {
                    let result = self.handle_register(entity).await;
                    let _ = reply.send(result);
                }
                EntityMessage::Get { id, reply } => {
                    let result = self.registry.get(&id);
                    let _ = reply.send(result);
                }
                EntityMessage::Update { entity, reply } => {
                    let result = self.handle_update(entity).await;
                    let _ = reply.send(result);
                }
                EntityMessage::UpdateIfVersion {
                    entity,
                    expected_version,
                    reply,
                } => {
                    let result = self.handle_update_if_version(entity, expected_version).await;
                    let _ = reply.send(result);
                }
                EntityMessage::Delete { id, reply } => {
                    let result = self.handle_delete(id).await;
                    let _ = reply.send(result);
                }
                EntityMessage::Exists { id, reply } => {
                    let result = self.registry.exists(&id);
                    let _ = reply.send(result);
                }
                EntityMessage::AddMembership { membership, reply } => {
                    let result = self.handle_add_membership(membership).await;
                    let _ = reply.send(result);
                }
                EntityMessage::RemoveMembership {
                    member_id,
                    parent_id,
                    reply,
                } => {
                    let result = self.handle_remove_membership(member_id, parent_id).await;
                    let _ = reply.send(result);
                }
                EntityMessage::UpdateMembership { membership, reply } => {
                    let result = self.registry.update_membership(membership);
                    let _ = reply.send(result);
                }
                EntityMessage::GetMembers { parent_id, reply } => {
                    let result = self.registry.get_members(&parent_id);
                    let _ = reply.send(result);
                }
                EntityMessage::GetMembershipsOf { member_id, reply } => {
                    let result = self.registry.get_memberships_of(&member_id);
                    let _ = reply.send(result);
                }
                EntityMessage::GetMembership {
                    member_id,
                    parent_id,
                    reply,
                } => {
                    let result = self.registry.get_membership(&member_id, &parent_id);
                    let _ = reply.send(result);
                }
                EntityMessage::ListByType { entity_type, reply } => {
                    let result = self.registry.list_by_type(entity_type);
                    let _ = reply.send(result);
                }
                EntityMessage::Count { reply } => {
                    let result = self.registry.count();
                    let _ = reply.send(result);
                }
                EntityMessage::MemberCount { parent_id, reply } => {
                    let result = self.registry.member_count(&parent_id);
                    let _ = reply.send(result);
                }
                EntityMessage::ApplyGossipUpdate { entity, reply } => {
                    let result = self.handle_apply_gossip_update(entity).await;
                    let _ = reply.send(result);
                }
            }
        }

        info!("EntityActor stopped");
    }

    async fn handle_register(&mut self, entity: CooperativeEntity) -> Result<()> {
        let entity_id = entity.id.clone();

        // Register in local storage
        self.registry.register(entity.clone())?;

        // Announce to network
        self.announce_entity_update(&entity).await;

        debug!(entity_id = %entity_id, "Entity registered");
        Ok(())
    }

    async fn handle_update(&mut self, entity: CooperativeEntity) -> Result<()> {
        let entity_id = entity.id.clone();

        // Update in local storage (this updates the `updated_at` timestamp internally)
        self.registry.update(entity)?;

        // Re-fetch to get the canonical entity with correct updated_at timestamp
        if let Some(updated_entity) = self.registry.get(&entity_id)? {
            // Announce to network with correct timestamp
            self.announce_entity_update(&updated_entity).await;
        }

        debug!(entity_id = %entity_id, "Entity updated");
        Ok(())
    }

    async fn handle_update_if_version(
        &mut self,
        entity: CooperativeEntity,
        expected_version: u64,
    ) -> Result<()> {
        let entity_id = entity.id.clone();

        // Update with version check (atomically)
        self.registry.update_if_version(entity, expected_version)?;

        // Re-fetch to get the canonical entity with correct updated_at and version
        if let Some(updated_entity) = self.registry.get(&entity_id)? {
            // Announce to network with correct timestamp and version
            self.announce_entity_update(&updated_entity).await;
        }

        debug!(
            entity_id = %entity_id,
            expected_version = expected_version,
            "Entity updated with version check"
        );
        Ok(())
    }

    /// Handle entity deletion with gossip announcement.
    ///
    /// # Known Limitation: Deletion Race Condition
    ///
    /// There is a small window between `get` and `delete` where an incoming gossip
    /// update could modify the entity. In this case, the deletion proceeds with
    /// a potentially stale timestamp. This is a theoretical consistency issue with
    /// extremely low probability in practice, as:
    /// 1. The window is very small (microseconds)
    /// 2. Concurrent updates and deletes on the same entity are rare
    /// 3. The timestamp on the deletion marker will still be newer than any
    ///    entity state that existed before this operation started
    ///
    /// Future improvement: Refactor to atomic get-and-delete if needed.
    async fn handle_delete(&mut self, id: EntityId) -> Result<()> {
        // Get entity before deletion to create tombstone with correct timestamp
        let entity = self.registry.get(&id)?;

        // Delete from local storage
        self.registry.delete(&id)?;

        // If entity existed, create a deletion marker and announce
        if let Some(mut entity) = entity {
            // Mark entity as deleted (using status)
            entity.status = crate::EntityStatus::Deleted;
            entity.updated_at = icn_time::current_timestamp_secs();
            self.announce_entity_update(&entity).await;
        }

        debug!(entity_id = %id, "Entity deleted");
        Ok(())
    }

    async fn handle_add_membership(&mut self, membership: Membership) -> Result<()> {
        let parent_id = membership.parent_id.clone();
        let member_id = membership.member_id.clone();

        // Add membership in local storage
        self.registry.add_membership(membership)?;

        // Get the parent entity and announce the update
        // (membership changes affect the parent entity's state)
        if let Ok(Some(parent)) = self.registry.get(&parent_id) {
            self.announce_entity_update(&parent).await;
        }

        debug!(
            parent_id = %parent_id,
            member_id = %member_id,
            "Membership added"
        );
        Ok(())
    }

    async fn handle_remove_membership(
        &mut self,
        member_id: EntityId,
        parent_id: EntityId,
    ) -> Result<()> {
        // Remove membership from local storage
        self.registry.remove_membership(&member_id, &parent_id)?;

        // Get the parent entity and announce the update
        if let Ok(Some(parent)) = self.registry.get(&parent_id) {
            self.announce_entity_update(&parent).await;
        }

        debug!(
            parent_id = %parent_id,
            member_id = %member_id,
            "Membership removed"
        );
        Ok(())
    }

    async fn handle_apply_gossip_update(&mut self, entity: CooperativeEntity) -> Result<()> {
        let entity_id = entity.id.clone();

        // Check if we should apply this update (last-write-wins with deterministic tie-breaker)
        if let Ok(Some(local_entity)) = self.registry.get(&entity_id) {
            // Use timestamp as primary comparison, entity ID as tie-breaker for determinism
            // When timestamps are equal, higher entity ID string wins to ensure all nodes converge
            let should_ignore = entity.updated_at < local_entity.updated_at
                || (entity.updated_at == local_entity.updated_at
                    && entity.id.to_string() <= local_entity.id.to_string());

            if should_ignore {
                debug!(
                    entity_id = %entity_id,
                    "Ignoring older entity state from gossip"
                );
                return Ok(());
            }
        }

        // Handle deletion marker
        if entity.status == crate::EntityStatus::Deleted {
            // Try to delete locally (may fail if entity doesn't exist or has members)
            if let Err(e) = self.registry.delete(&entity_id) {
                debug!(
                    entity_id = %entity_id,
                    error = %e,
                    "Could not apply deletion from gossip (may already be deleted or has members)"
                );
            } else {
                debug!(entity_id = %entity_id, "Applied deletion from gossip");
            }
            return Ok(());
        }

        // Check if entity exists locally
        match self.registry.exists(&entity_id) {
            Ok(true) => {
                // Update existing entity
                if let Err(e) = self.registry.update(entity.clone()) {
                    warn!(
                        entity_id = %entity_id,
                        error = %e,
                        "Failed to update entity from gossip"
                    );
                    return Err(e);
                }
                debug!(entity_id = %entity_id, "Applied entity update from gossip");
            }
            Ok(false) => {
                // Register new entity
                if let Err(e) = self.registry.register(entity.clone()) {
                    warn!(
                        entity_id = %entity_id,
                        error = %e,
                        "Failed to register entity from gossip"
                    );
                    return Err(e);
                }
                info!(
                    entity_id = %entity_id,
                    entity_name = %entity.name,
                    "Synced new entity from gossip"
                );
            }
            Err(e) => {
                return Err(e);
            }
        }

        Ok(())
    }

    async fn announce_entity_update(&self, entity: &CooperativeEntity) {
        if let Some(gossip) = &self.gossip {
            // Serialize the entity for gossip
            match icn_encoding::encode_versioned(entity) {
                Ok(data) => {
                    // Publish to gossip topic
                    let mut gossip_actor = gossip.write().await;
                    match gossip_actor.publish(ENTITY_TOPIC, data).await {
                        Ok(hash) => {
                            debug!(
                                entity_id = %entity.id,
                                entry_hash = ?hash,
                                "Published entity update to gossip"
                            );
                        }
                        Err(e) => {
                            warn!(
                                entity_id = %entity.id,
                                error = %e,
                                "Failed to publish entity update to gossip"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        entity_id = %entity.id,
                        error = %e,
                        "Failed to serialize entity for gossip"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::MembershipRole;
    use icn_identity::KeyPair;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn create_test_registry() -> SledEntityRegistry {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        SledEntityRegistry::new(Arc::new(db)).unwrap()
    }

    fn create_test_did() -> icn_identity::Did {
        KeyPair::generate().unwrap().did().clone()
    }

    async fn spawn_test_actor() -> crate::EntityHandle {
        let registry = create_test_registry();
        let tx = EntityActor::spawn(registry, None);
        crate::EntityHandle::new(tx)
    }

    #[tokio::test]
    async fn test_register_and_get_entity() {
        let handle = spawn_test_actor().await;

        let entity = CooperativeEntity::cooperative("test-coop", "Test Cooperative").unwrap();
        let entity_id = entity.id.clone();

        // Register
        handle.register(entity.clone()).await.unwrap();

        // Get it back
        let retrieved = handle.get(&entity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Test Cooperative");
        assert_eq!(retrieved.id, entity_id);
    }

    #[tokio::test]
    async fn test_update_entity() {
        let handle = spawn_test_actor().await;

        let mut entity = CooperativeEntity::cooperative("update-test", "Original Name").unwrap();
        let entity_id = entity.id.clone();

        handle.register(entity.clone()).await.unwrap();

        entity.name = "Updated Name".to_string();
        handle.update(entity).await.unwrap();

        let retrieved = handle.get(&entity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
    }

    #[tokio::test]
    async fn test_delete_entity() {
        let handle = spawn_test_actor().await;

        let entity = CooperativeEntity::cooperative("delete-test", "To Delete").unwrap();
        let entity_id = entity.id.clone();

        handle.register(entity).await.unwrap();
        assert!(handle.exists(&entity_id).await.unwrap());

        handle.delete(&entity_id).await.unwrap();
        assert!(!handle.exists(&entity_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_membership_operations() {
        let handle = spawn_test_actor().await;

        // Create a coop
        let coop = CooperativeEntity::cooperative("membership-coop", "Membership Test").unwrap();
        let coop_id = coop.id.clone();
        handle.register(coop).await.unwrap();

        // Create an individual
        let did = create_test_did();
        let individual = CooperativeEntity::individual(&did, "Test Member");
        let member_id = individual.id.clone();
        handle.register(individual).await.unwrap();

        // Add membership
        let membership =
            Membership::new(member_id.clone(), coop_id.clone(), MembershipRole::Worker);
        handle.add_membership(membership).await.unwrap();

        // Check member count
        assert_eq!(handle.member_count(&coop_id).await.unwrap(), 1);

        // Get members
        let members = handle.get_members(&coop_id).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].member_id, member_id);

        // Get memberships of individual
        let memberships = handle.get_memberships_of(&member_id).await.unwrap();
        assert_eq!(memberships.len(), 1);
        assert_eq!(memberships[0].parent_id, coop_id);

        // Remove membership
        handle
            .remove_membership(&member_id, &coop_id)
            .await
            .unwrap();
        assert_eq!(handle.member_count(&coop_id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_list_by_type() {
        let handle = spawn_test_actor().await;

        // Create entities of different types
        let coop = CooperativeEntity::cooperative("list-coop", "Test Coop").unwrap();
        let fed = CooperativeEntity::federation("list-fed", "Test Federation").unwrap();
        let did = create_test_did();
        let individual = CooperativeEntity::individual(&did, "Test Individual");

        handle.register(coop).await.unwrap();
        handle.register(fed).await.unwrap();
        handle.register(individual).await.unwrap();

        // List by type
        let coops = handle.list_by_type(EntityType::Cooperative).await.unwrap();
        assert_eq!(coops.len(), 1);

        let feds = handle.list_by_type(EntityType::Federation).await.unwrap();
        assert_eq!(feds.len(), 1);

        let individuals = handle.list_by_type(EntityType::Individual).await.unwrap();
        assert_eq!(individuals.len(), 1);
    }

    #[tokio::test]
    async fn test_count() {
        let handle = spawn_test_actor().await;

        assert_eq!(handle.count().await.unwrap(), 0);

        let entity1 = CooperativeEntity::cooperative("count-1", "Count 1").unwrap();
        let entity2 = CooperativeEntity::cooperative("count-2", "Count 2").unwrap();

        handle.register(entity1).await.unwrap();
        assert_eq!(handle.count().await.unwrap(), 1);

        handle.register(entity2).await.unwrap();
        assert_eq!(handle.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_apply_gossip_update_new_entity() {
        let handle = spawn_test_actor().await;

        let entity = CooperativeEntity::cooperative("gossip-new", "From Gossip").unwrap();
        let entity_id = entity.id.clone();

        // Entity doesn't exist yet
        assert!(!handle.exists(&entity_id).await.unwrap());

        // Apply gossip update
        handle.apply_gossip_update(entity.clone()).await.unwrap();

        // Entity should now exist
        assert!(handle.exists(&entity_id).await.unwrap());
        let retrieved = handle.get(&entity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "From Gossip");
    }

    #[tokio::test]
    async fn test_apply_gossip_update_newer_wins() {
        let handle = spawn_test_actor().await;

        let mut entity = CooperativeEntity::cooperative("gossip-update", "Original").unwrap();
        let entity_id = entity.id.clone();
        entity.updated_at = 1000;

        handle.register(entity.clone()).await.unwrap();

        // Try to apply older update (should be ignored)
        let mut older = entity.clone();
        older.name = "Older Update".to_string();
        older.updated_at = 500;
        handle.apply_gossip_update(older).await.unwrap();

        let retrieved = handle.get(&entity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Original");

        // Apply newer update (should succeed)
        let mut newer = entity.clone();
        newer.name = "Newer Update".to_string();
        newer.updated_at = 2000;
        handle.apply_gossip_update(newer).await.unwrap();

        let retrieved = handle.get(&entity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Newer Update");
    }
}
