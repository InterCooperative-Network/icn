//! Entity Manager for Gateway API
//!
//! Provides entity registry operations for the gateway API.
//! This is a simplified interface that can be backed by:
//! 1. In-memory storage (for standalone gateway)
//! 2. EntityActor handle (when integrated with daemon)

use anyhow::Result;
// Note: EntityRegistry trait must be in scope for InMemoryRegistry methods to be callable
use icn_entity::{
    CooperativeEntity, EntityError, EntityId, EntityRegistry, EntityType, InMemoryRegistry,
    Membership,
};
use std::sync::{Arc, RwLock};
use tracing::debug;

/// Handle type for actor-backed entity registry
///
/// Uses the new async EntityHandle from icn-entity for daemon integration.
pub type EntityHandle = icn_entity::EntityHandle;

/// Entity manager for gateway API
///
/// Supports two modes:
/// - **Standalone mode** (`new()`): In-memory storage, for testing only
/// - **Actor-backed mode** (`with_handle()`): Delegates to daemon's EntityActor
pub struct EntityManager {
    /// In-memory registry for standalone mode
    standalone: Option<Arc<RwLock<InMemoryRegistry>>>,
    /// Optional handle to daemon's EntityActor (async)
    actor_handle: Option<EntityHandle>,
}

impl EntityManager {
    /// Create a new entity manager with in-memory storage
    ///
    /// **Warning**: This mode is for testing only. State is lost on restart.
    pub fn new() -> Self {
        debug!("EntityManager created in standalone mode (in-memory only)");
        EntityManager {
            standalone: Some(Arc::new(RwLock::new(InMemoryRegistry::new()))),
            actor_handle: None,
        }
    }

    /// Create an entity manager backed by the daemon's EntityActor
    ///
    /// This is the recommended mode for production.
    pub fn with_handle(handle: EntityHandle) -> Self {
        debug!("EntityManager created with daemon EntityActor handle");
        EntityManager {
            standalone: None,
            actor_handle: Some(handle),
        }
    }

    /// Check if running in actor-backed mode
    pub fn is_actor_backed(&self) -> bool {
        self.actor_handle.is_some()
    }

    /// Register a new entity
    ///
    /// For actor-backed mode, this is async. For standalone mode, operations are sync.
    pub async fn register(&self, entity: CooperativeEntity) -> Result<()> {
        if let Some(ref handle) = self.actor_handle {
            return handle
                .register(entity)
                .await
                .map_err(|e| anyhow::anyhow!("Entity registration failed: {e}"));
        }

        if let Some(ref standalone) = self.standalone {
            let mut registry = standalone
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.register(entity)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// Ensure an entity exists, registering it atomically if not
    ///
    /// This method uses a try-insert pattern to avoid race conditions:
    /// - If the entity doesn't exist, it's registered
    /// - If the entity already exists (including from a concurrent registration), returns Ok(())
    /// - Only propagates errors for actual failures (not "already exists")
    ///
    /// This is primarily used for auto-registering individual entities when users
    /// perform operations that require their entity to exist.
    pub async fn ensure_entity_exists(&self, entity: CooperativeEntity) -> Result<bool> {
        let entity_id = entity.id.clone();
        match self.register(entity).await {
            Ok(()) => {
                debug!(entity_id = %entity_id, "Auto-registered entity");
                Ok(true) // Entity was newly created
            }
            Err(e) => {
                // Check if this is an "already exists" error (from concurrent registration)
                let error_str = e.to_string();
                if error_str.contains("already exists")
                    || error_str.contains("AlreadyExists")
                    || e.downcast_ref::<EntityError>().is_some_and(|entity_err| {
                        matches!(entity_err, EntityError::AlreadyExists(_))
                    })
                {
                    debug!(entity_id = %entity_id, "Entity already exists (concurrent registration)");
                    Ok(false) // Entity already existed
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Get an entity by ID
    pub async fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>> {
        if let Some(ref handle) = self.actor_handle {
            return handle
                .get(id)
                .await
                .map_err(|e| anyhow::anyhow!("Entity get failed: {e}"));
        }

        if let Some(ref standalone) = self.standalone {
            let registry = standalone
                .read()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.get(id)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// Update an entity
    pub async fn update(&self, entity: CooperativeEntity) -> Result<()> {
        if let Some(ref handle) = self.actor_handle {
            return handle
                .update(entity)
                .await
                .map_err(|e| anyhow::anyhow!("Entity update failed: {e}"));
        }

        if let Some(ref standalone) = self.standalone {
            let mut registry = standalone
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.update(entity)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// Remove an entity (only if no members)
    ///
    /// This method checks for members before deletion to prevent orphaned memberships.
    pub async fn remove(&self, id: &EntityId) -> Result<()> {
        if let Some(ref handle) = self.actor_handle {
            // For actor mode, check members first
            let members = handle
                .get_members(id)
                .await
                .map_err(|e| anyhow::anyhow!("Entity get_members failed: {e}"))?;
            if !members.is_empty() {
                anyhow::bail!(
                    "Cannot remove entity with active members. Remove all members first."
                );
            }
            return handle
                .delete(id)
                .await
                .map_err(|e| anyhow::anyhow!("Entity delete failed: {e}"));
        }

        if let Some(ref standalone) = self.standalone {
            let mut registry = standalone
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            // Atomically check for members and delete while holding write lock
            let members = registry.get_members(id)?;
            if !members.is_empty() {
                anyhow::bail!(
                    "Cannot remove entity with active members. Remove all members first."
                );
            }
            return Ok(registry.delete(id)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// List all entities
    ///
    /// Collects entities of all types (Individual, Cooperative, Federation)
    pub async fn list(&self) -> Result<Vec<CooperativeEntity>> {
        let entity_types = [
            EntityType::Individual,
            EntityType::Cooperative,
            EntityType::Federation,
        ];

        let mut all_entities = Vec::new();

        for entity_type in entity_types {
            let ids = self.list_by_type(entity_type).await?;
            for id in ids {
                if let Some(entity) = self.get(&id).await? {
                    all_entities.push(entity);
                }
            }
        }

        Ok(all_entities)
    }

    /// List entity IDs by type
    async fn list_by_type(&self, entity_type: EntityType) -> Result<Vec<EntityId>> {
        if let Some(ref handle) = self.actor_handle {
            return handle
                .list_by_type(entity_type)
                .await
                .map_err(|e| anyhow::anyhow!("Entity list_by_type failed: {e}"));
        }

        if let Some(ref standalone) = self.standalone {
            let registry = standalone
                .read()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.list_by_type(entity_type)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// Add a membership
    pub async fn add_membership(&self, membership: Membership) -> Result<()> {
        if let Some(ref handle) = self.actor_handle {
            return handle
                .add_membership(membership)
                .await
                .map_err(|e| anyhow::anyhow!("Entity add_membership failed: {e}"));
        }

        if let Some(ref standalone) = self.standalone {
            let mut registry = standalone
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.add_membership(membership)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// Get members of an entity
    pub async fn get_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>> {
        if let Some(ref handle) = self.actor_handle {
            return handle
                .get_members(parent_id)
                .await
                .map_err(|e| anyhow::anyhow!("Entity get_members failed: {e}"));
        }

        if let Some(ref standalone) = self.standalone {
            let registry = standalone
                .read()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.get_members(parent_id)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// Remove a membership
    ///
    /// Note: Removes the membership of `member_id` from `parent_id`
    pub async fn remove_membership(
        &self,
        parent_id: &EntityId,
        member_id: &EntityId,
    ) -> Result<()> {
        if let Some(ref handle) = self.actor_handle {
            // EntityHandle takes (member_id, parent_id)
            return handle
                .remove_membership(member_id, parent_id)
                .await
                .map_err(|e| anyhow::anyhow!("Entity remove_membership failed: {e}"));
        }

        if let Some(ref standalone) = self.standalone {
            let mut registry = standalone
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            // EntityRegistry trait takes (member_id, parent_id)
            return Ok(registry.remove_membership(member_id, parent_id)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }
}

impl Default for EntityManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_entity::MembershipRole;
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_standalone_mode() {
        let mgr = EntityManager::new();
        assert!(!mgr.is_actor_backed());

        // Register an entity using proper constructor
        let entity = CooperativeEntity::cooperative("test-coop", "Test Coop").unwrap();
        mgr.register(entity.clone()).await.unwrap();

        // Get it back
        let retrieved = mgr.get(&entity.id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Test Coop");

        // List entities
        let entities = mgr.list().await.unwrap();
        assert_eq!(entities.len(), 1);
    }

    #[tokio::test]
    async fn test_cannot_remove_with_members() {
        let mgr = EntityManager::new();

        let entity = CooperativeEntity::cooperative("parent-coop", "Parent Coop").unwrap();
        let coop_id = entity.id.clone();

        // Create a member entity from a generated keypair
        let keypair = KeyPair::generate().unwrap();
        let member_entity = CooperativeEntity::individual(keypair.did(), "Test Member");
        let member_id = member_entity.id.clone();

        mgr.register(entity).await.unwrap();
        mgr.register(member_entity).await.unwrap();

        // Add a member
        let membership =
            Membership::new(member_id.clone(), coop_id.clone(), MembershipRole::Member);
        mgr.add_membership(membership).await.unwrap();

        // Try to remove - should fail
        let result = mgr.remove(&coop_id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("active members"));

        // Remove the member first
        mgr.remove_membership(&coop_id, &member_id).await.unwrap();

        // Now removal should succeed
        mgr.remove(&coop_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_ensure_entity_exists_creates_new() {
        let mgr = EntityManager::new();
        let keypair = KeyPair::generate().unwrap();
        let entity = CooperativeEntity::individual(keypair.did(), "Test User");
        let entity_id = entity.id.clone();

        // First call should create the entity
        let was_created = mgr.ensure_entity_exists(entity).await.unwrap();
        assert!(was_created, "Entity should be newly created");

        // Verify entity exists
        let retrieved = mgr.get(&entity_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test User");
    }

    #[tokio::test]
    async fn test_ensure_entity_exists_handles_existing() {
        let mgr = EntityManager::new();
        let keypair = KeyPair::generate().unwrap();
        let entity = CooperativeEntity::individual(keypair.did(), "Test User");
        let entity_id = entity.id.clone();

        // Register entity directly first
        mgr.register(entity.clone()).await.unwrap();

        // ensure_entity_exists should return Ok(false) for existing entity
        let entity_copy = CooperativeEntity::individual(keypair.did(), "Test User");
        let was_created = mgr.ensure_entity_exists(entity_copy).await.unwrap();
        assert!(!was_created, "Entity already existed, should return false");

        // Entity should still be there
        let retrieved = mgr.get(&entity_id).await.unwrap();
        assert!(retrieved.is_some());
    }
}
