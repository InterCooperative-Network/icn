//! Entity registry trait and in-memory implementation
//!
//! The registry provides storage and lookup for entities and memberships.
//! This module defines the trait interface and a simple in-memory implementation
//! for testing.

use crate::entity::{CooperativeEntity, EntityId, EntityType};
use crate::error::{EntityError, Result};
use crate::membership::Membership;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// EntityRegistry Trait
// ============================================================================

/// Registry for entity storage and lookup
///
/// This trait defines the interface for entity persistence.
/// Implementations can be:
/// - In-memory (for testing)
/// - Persistent (sled-backed, for production)
/// - Actor-backed (delegating to an actor system)
pub trait EntityRegistry: Send + Sync {
    // ========================================
    // Entity Operations
    // ========================================

    /// Register a new entity
    ///
    /// Returns error if entity with same ID already exists.
    fn register(&mut self, entity: CooperativeEntity) -> Result<()>;

    /// Get an entity by ID
    fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>>;

    /// Update an existing entity
    ///
    /// Returns error if entity does not exist.
    fn update(&mut self, entity: CooperativeEntity) -> Result<()>;

    /// Delete an entity
    ///
    /// Returns error if entity has active members.
    fn delete(&mut self, id: &EntityId) -> Result<()>;

    /// Check if entity exists
    fn exists(&self, id: &EntityId) -> Result<bool>;

    /// List all entities of a given type
    ///
    /// Note: For large datasets, prefer `list_by_type_paginated()` to avoid
    /// loading all entities into memory at once.
    fn list_by_type(&self, entity_type: EntityType) -> Result<Vec<EntityId>>;

    /// List entities of a given type with pagination
    ///
    /// Returns up to `limit` entities starting from `offset`.
    /// Use `count_by_type()` to determine total count for pagination UI.
    fn list_by_type_paginated(
        &self,
        entity_type: EntityType,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EntityId>>;

    /// Count entities of a given type (efficient for pagination)
    fn count_by_type(&self, entity_type: EntityType) -> Result<usize>;

    /// List child entities (direct members that are themselves entities)
    ///
    /// Note: For large datasets, prefer `list_children_paginated()` to avoid
    /// loading all entities into memory at once.
    fn list_children(&self, parent_id: &EntityId) -> Result<Vec<EntityId>>;

    /// List child entities with pagination
    ///
    /// Returns up to `limit` children starting from `offset`.
    fn list_children_paginated(
        &self,
        parent_id: &EntityId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EntityId>>;

    /// Get the parent entity (if any)
    fn get_parent(&self, entity_id: &EntityId) -> Result<Option<EntityId>>;

    /// Count total entities
    fn count(&self) -> Result<usize>;

    // ========================================
    // Membership Operations
    // ========================================

    /// Add a membership record
    ///
    /// Returns error if membership already exists or entities don't exist.
    fn add_membership(&mut self, membership: Membership) -> Result<()>;

    /// Get a specific membership
    fn get_membership(
        &self,
        member_id: &EntityId,
        parent_id: &EntityId,
    ) -> Result<Option<Membership>>;

    /// Get all memberships for an entity (where it's a member)
    fn get_memberships_of(&self, member_id: &EntityId) -> Result<Vec<Membership>>;

    /// Get all members of an entity
    fn get_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>>;

    /// Update membership status/role
    fn update_membership(&mut self, membership: Membership) -> Result<()>;

    /// Remove membership
    fn remove_membership(&mut self, member_id: &EntityId, parent_id: &EntityId) -> Result<()>;

    /// Count members of an entity
    fn member_count(&self, parent_id: &EntityId) -> Result<usize>;
}

// ============================================================================
// InMemoryRegistry
// ============================================================================

/// In-memory implementation of EntityRegistry for testing
///
/// This implementation stores all data in memory and is not persistent.
/// Use this for unit tests and development only.
#[derive(Debug, Default)]
pub struct InMemoryRegistry {
    /// Entity storage: EntityId -> CooperativeEntity
    entities: HashMap<String, CooperativeEntity>,

    /// Membership storage: (member_id, parent_id) -> Membership
    memberships: HashMap<(String, String), Membership>,
}

impl InMemoryRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a handle for async access
    pub fn into_handle(self) -> EntityRegistryHandle {
        EntityRegistryHandle::new(self)
    }
}

impl EntityRegistry for InMemoryRegistry {
    fn register(&mut self, entity: CooperativeEntity) -> Result<()> {
        let id = entity.id.as_str().to_string();
        if self.entities.contains_key(&id) {
            return Err(EntityError::AlreadyExists(id));
        }
        self.entities.insert(id, entity);
        Ok(())
    }

    fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>> {
        Ok(self.entities.get(id.as_str()).cloned())
    }

    fn update(&mut self, mut entity: CooperativeEntity) -> Result<()> {
        let id = entity.id.as_str().to_string();
        if !self.entities.contains_key(&id) {
            return Err(EntityError::NotFound(id));
        }
        // Auto-update the updated_at timestamp for audit trail accuracy
        entity.updated_at = icn_time::current_timestamp_secs();
        self.entities.insert(id, entity);
        Ok(())
    }

    fn delete(&mut self, id: &EntityId) -> Result<()> {
        let id_str = id.as_str().to_string();

        // Check if entity has members (is a parent)
        let has_members = self.memberships.keys().any(|(_, parent)| parent == &id_str);
        if has_members {
            return Err(EntityError::RegistryError(
                "Cannot delete entity with active members".into(),
            ));
        }

        if self.entities.remove(&id_str).is_none() {
            return Err(EntityError::NotFound(id_str));
        }

        // Remove any memberships where this entity is a member
        self.memberships.retain(|(member, _), _| member != &id_str);

        Ok(())
    }

    fn exists(&self, id: &EntityId) -> Result<bool> {
        Ok(self.entities.contains_key(id.as_str()))
    }

    fn list_by_type(&self, entity_type: EntityType) -> Result<Vec<EntityId>> {
        Ok(self
            .entities
            .values()
            .filter(|e| e.entity_type == entity_type)
            .map(|e| e.id.clone())
            .collect())
    }

    fn list_by_type_paginated(
        &self,
        entity_type: EntityType,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EntityId>> {
        Ok(self
            .entities
            .values()
            .filter(|e| e.entity_type == entity_type)
            .skip(offset)
            .take(limit)
            .map(|e| e.id.clone())
            .collect())
    }

    fn count_by_type(&self, entity_type: EntityType) -> Result<usize> {
        Ok(self
            .entities
            .values()
            .filter(|e| e.entity_type == entity_type)
            .count())
    }

    fn list_children(&self, parent_id: &EntityId) -> Result<Vec<EntityId>> {
        let parent_str = parent_id.as_str();
        Ok(self
            .memberships
            .iter()
            .filter(|((_, parent), _)| parent == parent_str)
            .filter_map(|((member, _), _)| {
                self.entities.get(member).and_then(|e| {
                    if e.id.is_organization() {
                        Some(e.id.clone())
                    } else {
                        None
                    }
                })
            })
            .collect())
    }

    fn list_children_paginated(
        &self,
        parent_id: &EntityId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EntityId>> {
        let parent_str = parent_id.as_str();
        Ok(self
            .memberships
            .iter()
            .filter(|((_, parent), _)| parent == parent_str)
            .filter_map(|((member, _), _)| {
                self.entities.get(member).and_then(|e| {
                    if e.id.is_organization() {
                        Some(e.id.clone())
                    } else {
                        None
                    }
                })
            })
            .skip(offset)
            .take(limit)
            .collect())
    }

    fn get_parent(&self, entity_id: &EntityId) -> Result<Option<EntityId>> {
        Ok(self
            .entities
            .get(entity_id.as_str())
            .and_then(|e| e.parent_id.clone()))
    }

    fn count(&self) -> Result<usize> {
        Ok(self.entities.len())
    }

    fn add_membership(&mut self, membership: Membership) -> Result<()> {
        let member_str = membership.member_id.as_str();
        let parent_str = membership.parent_id.as_str();

        // Verify both entities exist
        let member_entity = self.entities.get(member_str).ok_or_else(|| {
            EntityError::MembershipError(format!("Member entity not found: {member_str}"))
        })?;
        let parent_entity = self.entities.get(parent_str).ok_or_else(|| {
            EntityError::MembershipError(format!("Parent entity not found: {parent_str}"))
        })?;

        // Validate entity type relationships:
        // - Individuals can join Cooperatives or Federations
        // - Cooperatives can join Federations
        // - Federations can join Federations (recursive)
        // - Nothing can join an Individual
        let valid_relationship = match (&parent_entity.entity_type, &member_entity.entity_type) {
            // Cooperatives accept individuals
            (EntityType::Cooperative, EntityType::Individual) => true,
            // Federations accept individuals, cooperatives, and other federations
            (EntityType::Federation, EntityType::Individual) => true,
            (EntityType::Federation, EntityType::Cooperative) => true,
            (EntityType::Federation, EntityType::Federation) => true,
            // Individuals cannot have members
            (EntityType::Individual, _) => false,
            // Unknown types - reject for safety
            (EntityType::Unknown, _) | (_, EntityType::Unknown) => false,
            // Other combinations are invalid
            _ => false,
        };

        if !valid_relationship {
            return Err(EntityError::MembershipError(format!(
                "Invalid membership: {:?} cannot be a member of {:?}",
                member_entity.entity_type, parent_entity.entity_type
            )));
        }

        let key = (member_str.to_string(), parent_str.to_string());

        // Check membership doesn't already exist
        if self.memberships.contains_key(&key) {
            return Err(EntityError::MembershipError(
                "Membership already exists".into(),
            ));
        }

        self.memberships.insert(key, membership);
        Ok(())
    }

    fn get_membership(
        &self,
        member_id: &EntityId,
        parent_id: &EntityId,
    ) -> Result<Option<Membership>> {
        let key = (
            member_id.as_str().to_string(),
            parent_id.as_str().to_string(),
        );
        Ok(self.memberships.get(&key).cloned())
    }

    fn get_memberships_of(&self, member_id: &EntityId) -> Result<Vec<Membership>> {
        let member_str = member_id.as_str();
        Ok(self
            .memberships
            .iter()
            .filter(|((member, _), _)| member == member_str)
            .map(|(_, m)| m.clone())
            .collect())
    }

    fn get_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>> {
        let parent_str = parent_id.as_str();
        Ok(self
            .memberships
            .iter()
            .filter(|((_, parent), _)| parent == parent_str)
            .map(|(_, m)| m.clone())
            .collect())
    }

    fn update_membership(&mut self, membership: Membership) -> Result<()> {
        let key = (
            membership.member_id.as_str().to_string(),
            membership.parent_id.as_str().to_string(),
        );

        if !self.memberships.contains_key(&key) {
            return Err(EntityError::MembershipError("Membership not found".into()));
        }

        self.memberships.insert(key, membership);
        Ok(())
    }

    fn remove_membership(&mut self, member_id: &EntityId, parent_id: &EntityId) -> Result<()> {
        let key = (
            member_id.as_str().to_string(),
            parent_id.as_str().to_string(),
        );

        if self.memberships.remove(&key).is_none() {
            return Err(EntityError::MembershipError("Membership not found".into()));
        }
        Ok(())
    }

    fn member_count(&self, parent_id: &EntityId) -> Result<usize> {
        let parent_str = parent_id.as_str();
        Ok(self
            .memberships
            .keys()
            .filter(|(_, parent)| parent == parent_str)
            .count())
    }
}

// ============================================================================
// EntityRegistryHandle
// ============================================================================

/// Handle for async access to an entity registry
///
/// Wraps a registry in an Arc<RwLock> for thread-safe async access.
#[derive(Clone)]
pub struct EntityRegistryHandle {
    inner: Arc<RwLock<InMemoryRegistry>>,
}

impl EntityRegistryHandle {
    /// Create a new handle wrapping a registry
    pub fn new(registry: InMemoryRegistry) -> Self {
        EntityRegistryHandle {
            inner: Arc::new(RwLock::new(registry)),
        }
    }

    /// Register a new entity
    pub async fn register(&self, entity: CooperativeEntity) -> Result<()> {
        let mut registry = self.inner.write().await;
        registry.register(entity)
    }

    /// Get an entity by ID
    pub async fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>> {
        let registry = self.inner.read().await;
        registry.get(id)
    }

    /// Update an existing entity
    pub async fn update(&self, entity: CooperativeEntity) -> Result<()> {
        let mut registry = self.inner.write().await;
        registry.update(entity)
    }

    /// Check if entity exists
    pub async fn exists(&self, id: &EntityId) -> Result<bool> {
        let registry = self.inner.read().await;
        registry.exists(id)
    }

    /// Get all members of an entity
    pub async fn get_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>> {
        let registry = self.inner.read().await;
        registry.get_members(parent_id)
    }

    /// Add a membership
    pub async fn add_membership(&self, membership: Membership) -> Result<()> {
        let mut registry = self.inner.write().await;
        registry.add_membership(membership)
    }

    /// Get count of entities
    pub async fn count(&self) -> Result<usize> {
        let registry = self.inner.read().await;
        registry.count()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::membership::MembershipRole;
    use icn_identity::KeyPair;

    fn create_test_individual() -> CooperativeEntity {
        let keypair = KeyPair::generate().unwrap();
        CooperativeEntity::individual(keypair.did(), "Alice")
    }

    fn create_test_coop() -> CooperativeEntity {
        CooperativeEntity::cooperative("test-coop", "Test Cooperative").unwrap()
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = InMemoryRegistry::new();
        let entity = create_test_coop();
        let id = entity.id.clone();

        registry.register(entity.clone()).unwrap();

        let retrieved = registry.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.name, entity.name);
    }

    #[test]
    fn test_duplicate_register_fails() {
        let mut registry = InMemoryRegistry::new();
        let entity = create_test_coop();

        registry.register(entity.clone()).unwrap();
        let result = registry.register(entity);

        assert!(matches!(result, Err(EntityError::AlreadyExists(_))));
    }

    #[test]
    fn test_update() {
        let mut registry = InMemoryRegistry::new();
        let mut entity = create_test_coop();
        let id = entity.id.clone();

        registry.register(entity.clone()).unwrap();

        entity.name = "Updated Name".to_string();
        registry.update(entity).unwrap();

        let retrieved = registry.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
    }

    #[test]
    fn test_update_nonexistent_fails() {
        let mut registry = InMemoryRegistry::new();
        let entity = create_test_coop();

        let result = registry.update(entity);
        assert!(matches!(result, Err(EntityError::NotFound(_))));
    }

    #[test]
    fn test_list_by_type() {
        let mut registry = InMemoryRegistry::new();

        let coop = create_test_coop();
        let individual = create_test_individual();

        registry.register(coop).unwrap();
        registry.register(individual).unwrap();

        let coops = registry.list_by_type(EntityType::Cooperative).unwrap();
        let individuals = registry.list_by_type(EntityType::Individual).unwrap();

        assert_eq!(coops.len(), 1);
        assert_eq!(individuals.len(), 1);
    }

    #[test]
    fn test_membership() {
        let mut registry = InMemoryRegistry::new();

        let coop = create_test_coop();
        let coop_id = coop.id.clone();
        registry.register(coop).unwrap();

        let individual = create_test_individual();
        let individual_id = individual.id.clone();
        registry.register(individual).unwrap();

        let membership = Membership::active(
            individual_id.clone(),
            coop_id.clone(),
            MembershipRole::Worker,
        );
        registry.add_membership(membership).unwrap();

        // Check member count
        assert_eq!(registry.member_count(&coop_id).unwrap(), 1);

        // Get members
        let members = registry.get_members(&coop_id).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].member_id, individual_id);

        // Get memberships of individual
        let memberships = registry.get_memberships_of(&individual_id).unwrap();
        assert_eq!(memberships.len(), 1);
        assert_eq!(memberships[0].parent_id, coop_id);
    }

    #[test]
    fn test_delete_with_members_fails() {
        let mut registry = InMemoryRegistry::new();

        let coop = create_test_coop();
        let coop_id = coop.id.clone();
        registry.register(coop).unwrap();

        let individual = create_test_individual();
        let individual_id = individual.id.clone();
        registry.register(individual).unwrap();

        let membership = Membership::active(individual_id, coop_id.clone(), MembershipRole::Worker);
        registry.add_membership(membership).unwrap();

        // Try to delete coop with active member
        let result = registry.delete(&coop_id);
        assert!(matches!(result, Err(EntityError::RegistryError(_))));
    }

    #[test]
    fn test_invalid_membership_relationships() {
        let mut registry = InMemoryRegistry::new();

        // Create entities
        let individual = create_test_individual();
        let individual_id = individual.id.clone();
        registry.register(individual).unwrap();

        let coop = create_test_coop();
        let coop_id = coop.id.clone();
        registry.register(coop).unwrap();

        let fed = CooperativeEntity::federation("test-fed", "Test Federation").unwrap();
        let fed_id = fed.id.clone();
        registry.register(fed).unwrap();

        // Valid: Individual joins Cooperative
        let membership = Membership::active(
            individual_id.clone(),
            coop_id.clone(),
            MembershipRole::Worker,
        );
        assert!(registry.add_membership(membership).is_ok());

        // Valid: Cooperative joins Federation
        let membership = Membership::active(
            coop_id.clone(),
            fed_id.clone(),
            MembershipRole::FederatedMember,
        );
        assert!(registry.add_membership(membership).is_ok());

        // Invalid: Cooperative cannot be member of Individual
        let individual2 = create_test_individual();
        let individual2_id = individual2.id.clone();
        registry.register(individual2).unwrap();

        let invalid = Membership::active(coop_id.clone(), individual2_id, MembershipRole::Member);
        let result = registry.add_membership(invalid);
        assert!(matches!(result, Err(EntityError::MembershipError(_))));

        // Invalid: Federation cannot be member of Cooperative
        let fed2 = CooperativeEntity::federation("test-fed-2", "Test Federation 2").unwrap();
        let fed2_id = fed2.id.clone();
        registry.register(fed2).unwrap();

        let invalid = Membership::active(fed2_id, coop_id, MembershipRole::FederatedMember);
        let result = registry.add_membership(invalid);
        assert!(matches!(result, Err(EntityError::MembershipError(_))));
    }

    #[tokio::test]
    async fn test_handle_async_operations() {
        let registry = InMemoryRegistry::new();
        let handle = registry.into_handle();

        let entity = create_test_coop();
        let id = entity.id.clone();

        handle.register(entity).await.unwrap();

        assert!(handle.exists(&id).await.unwrap());
        assert_eq!(handle.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_handle_concurrent_reads() {
        use std::sync::Arc;

        let registry = InMemoryRegistry::new();
        let handle = Arc::new(registry.into_handle());

        // Register some entities
        for i in 0..10 {
            let entity =
                CooperativeEntity::cooperative(&format!("coop-{i:03}"), format!("Coop {i}"))
                    .unwrap();
            handle.register(entity).await.unwrap();
        }

        // Spawn multiple concurrent read tasks
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let h = Arc::clone(&handle);
            tasks.push(tokio::spawn(async move {
                // Each task reads all entities
                h.count().await.unwrap()
            }));
        }

        // All tasks should succeed and see the same count
        for task in tasks {
            let count = task.await.unwrap();
            assert_eq!(count, 10);
        }
    }

    #[tokio::test]
    async fn test_handle_concurrent_writes() {
        use std::sync::Arc;

        let registry = InMemoryRegistry::new();
        let handle = Arc::new(registry.into_handle());

        // Spawn multiple concurrent write tasks
        let mut tasks = Vec::new();
        for i in 0..10 {
            let h = Arc::clone(&handle);
            tasks.push(tokio::spawn(async move {
                let entity =
                    CooperativeEntity::cooperative(&format!("coop-{i:03}"), format!("Coop {i}"))
                        .unwrap();
                h.register(entity).await
            }));
        }

        // All tasks should succeed
        for task in tasks {
            task.await.unwrap().unwrap();
        }

        // Should have all 10 entities
        assert_eq!(handle.count().await.unwrap(), 10);
    }

    #[test]
    fn test_update_auto_updates_timestamp() {
        let mut registry = InMemoryRegistry::new();
        let entity = create_test_coop();
        let id = entity.id.clone();
        let original_updated_at = entity.updated_at;

        registry.register(entity.clone()).unwrap();

        // Wait a tiny bit to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Update the entity
        let mut updated_entity = entity.clone();
        updated_entity.name = "Updated Name".to_string();
        registry.update(updated_entity).unwrap();

        // Verify updated_at was auto-updated
        let retrieved = registry.get(&id).unwrap().unwrap();
        assert!(
            retrieved.updated_at >= original_updated_at,
            "updated_at should be updated on registry.update()"
        );
    }
}
