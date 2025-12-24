//! Entity Manager for Gateway API
//!
//! Provides entity registry operations for the gateway API.
//! This is a simplified interface that can be backed by:
//! 1. In-memory storage (for standalone gateway)
//! 2. EntityActor handle (when integrated with daemon)

use anyhow::Result;
use icn_entity::{
    CooperativeEntity, EntityId, EntityRegistry, EntityType, InMemoryRegistry, Membership,
};
use std::sync::{Arc, RwLock};
use tracing::debug;

/// Handle type for actor-backed entity registry
///
/// This uses the `EntityRegistry` trait to avoid direct dependency on `icn-core`.
pub type EntityHandle = Arc<RwLock<dyn EntityRegistry + Send + Sync>>;

/// Entity manager for gateway API
///
/// Supports two modes:
/// - **Standalone mode** (`new()`): In-memory storage, for testing only
/// - **Actor-backed mode** (`with_handle()`): Delegates to daemon's EntityActor
pub struct EntityManager {
    /// In-memory registry for standalone mode
    standalone: Option<Arc<RwLock<InMemoryRegistry>>>,
    /// Optional handle to daemon's EntityActor
    entity_handle: Option<EntityHandle>,
}

impl EntityManager {
    /// Create a new entity manager with in-memory storage
    ///
    /// **Warning**: This mode is for testing only. State is lost on restart.
    pub fn new() -> Self {
        debug!("EntityManager created in standalone mode (in-memory only)");
        EntityManager {
            standalone: Some(Arc::new(RwLock::new(InMemoryRegistry::new()))),
            entity_handle: None,
        }
    }

    /// Create an entity manager backed by the daemon's EntityActor
    ///
    /// This is the recommended mode for production.
    pub fn with_handle(handle: EntityHandle) -> Self {
        debug!("EntityManager created with daemon EntityActor handle");
        EntityManager {
            standalone: None,
            entity_handle: Some(handle),
        }
    }

    /// Check if running in actor-backed mode
    pub fn is_actor_backed(&self) -> bool {
        self.entity_handle.is_some()
    }

    /// Register a new entity
    pub fn register(&self, entity: CooperativeEntity) -> Result<()> {
        if let Some(ref handle) = self.entity_handle {
            let mut registry = handle
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.register(entity)?);
        }

        if let Some(ref standalone) = self.standalone {
            let mut registry = standalone
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.register(entity)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// Get an entity by ID
    pub fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>> {
        if let Some(ref handle) = self.entity_handle {
            let registry = handle
                .read()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.get(id)?);
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
    pub fn update(&self, entity: CooperativeEntity) -> Result<()> {
        if let Some(ref handle) = self.entity_handle {
            let mut registry = handle
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.update(entity)?);
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
    pub fn remove(&self, id: &EntityId) -> Result<()> {
        // First check if entity has members
        let members = self.get_members(id)?;
        if !members.is_empty() {
            anyhow::bail!("Cannot remove entity with active members. Remove all members first.");
        }

        if let Some(ref handle) = self.entity_handle {
            let mut registry = handle
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.delete(id)?);
        }

        if let Some(ref standalone) = self.standalone {
            let mut registry = standalone
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.delete(id)?);
        }

        anyhow::bail!("Entity manager not initialized")
    }

    /// List all entities
    ///
    /// Collects entities of all types (Individual, Cooperative, Federation)
    pub fn list(&self) -> Result<Vec<CooperativeEntity>> {
        let entity_types = [
            EntityType::Individual,
            EntityType::Cooperative,
            EntityType::Federation,
        ];

        let mut all_entities = Vec::new();

        for entity_type in entity_types {
            let ids = self.list_by_type(entity_type)?;
            for id in ids {
                if let Some(entity) = self.get(&id)? {
                    all_entities.push(entity);
                }
            }
        }

        Ok(all_entities)
    }

    /// List entity IDs by type
    fn list_by_type(&self, entity_type: EntityType) -> Result<Vec<EntityId>> {
        if let Some(ref handle) = self.entity_handle {
            let registry = handle
                .read()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.list_by_type(entity_type)?);
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
    pub fn add_membership(&self, membership: Membership) -> Result<()> {
        if let Some(ref handle) = self.entity_handle {
            let mut registry = handle
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.add_membership(membership)?);
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
    pub fn get_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>> {
        if let Some(ref handle) = self.entity_handle {
            let registry = handle
                .read()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            return Ok(registry.get_members(parent_id)?);
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
    pub fn remove_membership(&self, parent_id: &EntityId, member_id: &EntityId) -> Result<()> {
        if let Some(ref handle) = self.entity_handle {
            let mut registry = handle
                .write()
                .map_err(|e| anyhow::anyhow!("Entity registry lock poisoned: {e}"))?;
            // EntityRegistry trait takes (member_id, parent_id)
            return Ok(registry.remove_membership(member_id, parent_id)?);
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

    #[test]
    fn test_standalone_mode() {
        let mgr = EntityManager::new();
        assert!(!mgr.is_actor_backed());

        // Register an entity using proper constructor
        let entity = CooperativeEntity::cooperative("test-coop", "Test Coop").unwrap();
        mgr.register(entity.clone()).unwrap();

        // Get it back
        let retrieved = mgr.get(&entity.id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Test Coop");

        // List entities
        let entities = mgr.list().unwrap();
        assert_eq!(entities.len(), 1);
    }

    #[test]
    fn test_cannot_remove_with_members() {
        let mgr = EntityManager::new();

        let entity = CooperativeEntity::cooperative("parent-coop", "Parent Coop").unwrap();
        let coop_id = entity.id.clone();

        // Create a member entity from a generated keypair
        let keypair = KeyPair::generate().unwrap();
        let member_entity = CooperativeEntity::individual(keypair.did(), "Test Member");
        let member_id = member_entity.id.clone();

        mgr.register(entity).unwrap();
        mgr.register(member_entity).unwrap();

        // Add a member
        let membership =
            Membership::new(member_id.clone(), coop_id.clone(), MembershipRole::Member);
        mgr.add_membership(membership).unwrap();

        // Try to remove - should fail
        let result = mgr.remove(&coop_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("active members"));

        // Remove the member first
        mgr.remove_membership(&coop_id, &member_id).unwrap();

        // Now removal should succeed
        mgr.remove(&coop_id).unwrap();
    }
}
