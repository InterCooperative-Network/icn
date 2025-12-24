//! Sled-backed persistent entity registry
//!
//! This module provides a persistent implementation of the EntityRegistry trait
//! using Sled for storage. Unlike InMemoryRegistry, data persists across daemon
//! restarts.
//!
//! # Key Schema
//!
//! - `entity:{id}` -> CooperativeEntity (bincode)
//! - `membership:{parent_id}:{member_id}` -> Membership (bincode)
//! - `type:{entity_type}:{id}` -> () (secondary index for list_by_type)
//! - `member_of:{member_id}:{parent_id}` -> () (secondary index for get_memberships_of)

use crate::entity::{CooperativeEntity, EntityId, EntityType};
use crate::error::{EntityError, Result};
use crate::membership::Membership;
use crate::registry::EntityRegistry;
use sled::Db;
use std::sync::Arc;
use tracing::debug;

/// Sled-backed persistent entity registry
///
/// Provides persistent storage for entities and memberships using Sled.
/// This is the recommended implementation for production use.
///
/// # Example
///
/// ```ignore
/// use icn_entity::{SledEntityRegistry, CooperativeEntity};
///
/// let db = sled::open("/path/to/db")?;
/// let mut registry = SledEntityRegistry::new(Arc::new(db))?;
///
/// let entity = CooperativeEntity::cooperative("my-coop", "My Cooperative")?;
/// registry.register(entity)?;
/// ```
pub struct SledEntityRegistry {
    db: Arc<Db>,
}

impl SledEntityRegistry {
    /// Create a new SledEntityRegistry backed by the given database
    pub fn new(db: Arc<Db>) -> Result<Self> {
        debug!("SledEntityRegistry initialized");
        Ok(Self { db })
    }

    /// Create a temporary in-memory registry for testing
    #[cfg(test)]
    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| EntityError::RegistryError(format!("Failed to open temp db: {e}")))?;
        Self::new(Arc::new(db))
    }

    // ========================================
    // Key generation helpers
    // ========================================

    fn entity_key(id: &EntityId) -> Vec<u8> {
        format!("entity:{}", id.as_str()).into_bytes()
    }

    fn membership_key(parent_id: &EntityId, member_id: &EntityId) -> Vec<u8> {
        format!("membership:{}:{}", parent_id.as_str(), member_id.as_str()).into_bytes()
    }

    fn type_index_key(entity_type: EntityType, id: &EntityId) -> Vec<u8> {
        format!("type:{}:{}", entity_type, id.as_str()).into_bytes()
    }

    fn member_of_index_key(member_id: &EntityId, parent_id: &EntityId) -> Vec<u8> {
        format!("member_of:{}:{}", member_id.as_str(), parent_id.as_str()).into_bytes()
    }

    fn type_index_prefix(entity_type: EntityType) -> Vec<u8> {
        format!("type:{entity_type}:").into_bytes()
    }

    fn membership_prefix(parent_id: &EntityId) -> Vec<u8> {
        format!("membership:{}:", parent_id.as_str()).into_bytes()
    }

    fn member_of_prefix(member_id: &EntityId) -> Vec<u8> {
        format!("member_of:{}:", member_id.as_str()).into_bytes()
    }

    // ========================================
    // Serialization helpers
    // ========================================

    fn serialize_entity(entity: &CooperativeEntity) -> Result<Vec<u8>> {
        bincode::serde::encode_to_vec(entity, bincode::config::legacy())
            .map_err(|e| EntityError::RegistryError(format!("Failed to serialize entity: {e}")))
    }

    fn deserialize_entity(bytes: &[u8]) -> Result<CooperativeEntity> {
        bincode::serde::decode_from_slice(bytes, bincode::config::legacy())
            .map(|(entity, _)| entity)
            .map_err(|e| EntityError::RegistryError(format!("Failed to deserialize entity: {e}")))
    }

    fn serialize_membership(membership: &Membership) -> Result<Vec<u8>> {
        bincode::serde::encode_to_vec(membership, bincode::config::legacy())
            .map_err(|e| EntityError::RegistryError(format!("Failed to serialize membership: {e}")))
    }

    fn deserialize_membership(bytes: &[u8]) -> Result<Membership> {
        bincode::serde::decode_from_slice(bytes, bincode::config::legacy())
            .map(|(membership, _)| membership)
            .map_err(|e| {
                EntityError::RegistryError(format!("Failed to deserialize membership: {e}"))
            })
    }

    // ========================================
    // Helper for membership validation
    // ========================================

    fn validate_membership_relationship(
        &self,
        parent_entity: &CooperativeEntity,
        member_entity: &CooperativeEntity,
    ) -> Result<()> {
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

        Ok(())
    }
}

impl EntityRegistry for SledEntityRegistry {
    fn register(&mut self, entity: CooperativeEntity) -> Result<()> {
        let key = Self::entity_key(&entity.id);

        // Check if entity already exists
        if self
            .db
            .contains_key(&key)
            .map_err(|e| EntityError::RegistryError(format!("DB error: {e}")))?
        {
            return Err(EntityError::AlreadyExists(entity.id.as_str().to_string()));
        }

        // Serialize and store entity
        let value = Self::serialize_entity(&entity)?;
        self.db
            .insert(&key, value)
            .map_err(|e| EntityError::RegistryError(format!("Failed to insert entity: {e}")))?;

        // Add type index
        let type_key = Self::type_index_key(entity.entity_type, &entity.id);
        self.db
            .insert(&type_key, &[])
            .map_err(|e| EntityError::RegistryError(format!("Failed to insert type index: {e}")))?;

        debug!(entity_id = %entity.id, "Entity registered");
        Ok(())
    }

    fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>> {
        let key = Self::entity_key(id);

        match self
            .db
            .get(&key)
            .map_err(|e| EntityError::RegistryError(format!("DB error: {e}")))?
        {
            Some(value) => Ok(Some(Self::deserialize_entity(&value)?)),
            None => Ok(None),
        }
    }

    fn update(&mut self, mut entity: CooperativeEntity) -> Result<()> {
        let key = Self::entity_key(&entity.id);

        // Check if entity exists
        let old_entity = self.get(&entity.id)?;
        if old_entity.is_none() {
            return Err(EntityError::NotFound(entity.id.as_str().to_string()));
        }
        let old_entity = old_entity
            .ok_or_else(|| EntityError::RegistryError("Entity disappeared during update".into()))?;

        // Update the updated_at timestamp
        entity.updated_at = icn_time::current_timestamp_secs();

        // If entity type changed, update the type index
        if old_entity.entity_type != entity.entity_type {
            // Remove old type index
            let old_type_key = Self::type_index_key(old_entity.entity_type, &entity.id);
            self.db.remove(&old_type_key).map_err(|e| {
                EntityError::RegistryError(format!("Failed to remove old type index: {e}"))
            })?;

            // Add new type index
            let new_type_key = Self::type_index_key(entity.entity_type, &entity.id);
            self.db.insert(&new_type_key, &[]).map_err(|e| {
                EntityError::RegistryError(format!("Failed to insert new type index: {e}"))
            })?;
        }

        // Store updated entity
        let value = Self::serialize_entity(&entity)?;
        self.db
            .insert(&key, value)
            .map_err(|e| EntityError::RegistryError(format!("Failed to update entity: {e}")))?;

        debug!(entity_id = %entity.id, "Entity updated");
        Ok(())
    }

    fn delete(&mut self, id: &EntityId) -> Result<()> {
        // Check if entity has members
        let member_count = self.member_count(id)?;
        if member_count > 0 {
            return Err(EntityError::RegistryError(
                "Cannot delete entity with active members".into(),
            ));
        }

        // Get entity to know its type for index cleanup
        let entity = self.get(id)?;
        if entity.is_none() {
            return Err(EntityError::NotFound(id.as_str().to_string()));
        }
        let entity =
            entity.ok_or_else(|| EntityError::RegistryError("Entity disappeared".into()))?;

        // Remove entity
        let key = Self::entity_key(id);
        self.db
            .remove(&key)
            .map_err(|e| EntityError::RegistryError(format!("Failed to delete entity: {e}")))?;

        // Remove type index
        let type_key = Self::type_index_key(entity.entity_type, id);
        self.db
            .remove(&type_key)
            .map_err(|e| EntityError::RegistryError(format!("Failed to remove type index: {e}")))?;

        // Remove any memberships where this entity is a member
        let memberships = self.get_memberships_of(id)?;
        for membership in memberships {
            self.remove_membership(&membership.member_id, &membership.parent_id)?;
        }

        debug!(entity_id = %id, "Entity deleted");
        Ok(())
    }

    fn exists(&self, id: &EntityId) -> Result<bool> {
        let key = Self::entity_key(id);
        self.db
            .contains_key(&key)
            .map_err(|e| EntityError::RegistryError(format!("DB error: {e}")))
    }

    fn list_by_type(&self, entity_type: EntityType) -> Result<Vec<EntityId>> {
        let prefix = Self::type_index_prefix(entity_type);
        let mut ids = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            let (key, _) =
                item.map_err(|e| EntityError::RegistryError(format!("Failed to scan: {e}")))?;

            // Key format: type:{entity_type}:{id}
            let key_str = String::from_utf8_lossy(&key);
            if let Some(id_str) = key_str.strip_prefix(&format!("type:{entity_type}:")) {
                if let Ok(id) = id_str.parse::<EntityId>() {
                    ids.push(id);
                }
            }
        }

        Ok(ids)
    }

    fn list_children(&self, parent_id: &EntityId) -> Result<Vec<EntityId>> {
        let members = self.get_members(parent_id)?;
        let mut children = Vec::new();

        for membership in members {
            if let Some(entity) = self.get(&membership.member_id)? {
                if entity.id.is_organization() {
                    children.push(entity.id);
                }
            }
        }

        Ok(children)
    }

    fn get_parent(&self, entity_id: &EntityId) -> Result<Option<EntityId>> {
        Ok(self.get(entity_id)?.and_then(|e| e.parent_id))
    }

    fn count(&self) -> Result<usize> {
        let prefix = b"entity:";
        let count = self.db.scan_prefix(prefix).count();
        Ok(count)
    }

    fn add_membership(&mut self, membership: Membership) -> Result<()> {
        // Verify both entities exist
        let member_entity = self.get(&membership.member_id)?.ok_or_else(|| {
            EntityError::MembershipError(format!(
                "Member entity not found: {}",
                membership.member_id
            ))
        })?;
        let parent_entity = self.get(&membership.parent_id)?.ok_or_else(|| {
            EntityError::MembershipError(format!(
                "Parent entity not found: {}",
                membership.parent_id
            ))
        })?;

        // Validate relationship
        self.validate_membership_relationship(&parent_entity, &member_entity)?;

        // Check if membership already exists
        let key = Self::membership_key(&membership.parent_id, &membership.member_id);
        if self
            .db
            .contains_key(&key)
            .map_err(|e| EntityError::RegistryError(format!("DB error: {e}")))?
        {
            return Err(EntityError::MembershipError(
                "Membership already exists".into(),
            ));
        }

        // Store membership
        let value = Self::serialize_membership(&membership)?;
        self.db
            .insert(&key, value)
            .map_err(|e| EntityError::RegistryError(format!("Failed to insert membership: {e}")))?;

        // Add member_of index for reverse lookup
        let member_of_key = Self::member_of_index_key(&membership.member_id, &membership.parent_id);
        self.db.insert(&member_of_key, &[]).map_err(|e| {
            EntityError::RegistryError(format!("Failed to insert member_of index: {e}"))
        })?;

        debug!(
            member_id = %membership.member_id,
            parent_id = %membership.parent_id,
            "Membership added"
        );
        Ok(())
    }

    fn get_membership(
        &self,
        member_id: &EntityId,
        parent_id: &EntityId,
    ) -> Result<Option<Membership>> {
        let key = Self::membership_key(parent_id, member_id);

        match self
            .db
            .get(&key)
            .map_err(|e| EntityError::RegistryError(format!("DB error: {e}")))?
        {
            Some(value) => Ok(Some(Self::deserialize_membership(&value)?)),
            None => Ok(None),
        }
    }

    fn get_memberships_of(&self, member_id: &EntityId) -> Result<Vec<Membership>> {
        let prefix = Self::member_of_prefix(member_id);
        let mut memberships = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            let (key, _) =
                item.map_err(|e| EntityError::RegistryError(format!("Failed to scan: {e}")))?;

            // Key format: member_of:{member_id}:{parent_id}
            let key_str = String::from_utf8_lossy(&key);
            let prefix_str = format!("member_of:{}:", member_id.as_str());
            if let Some(parent_id_str) = key_str.strip_prefix(&prefix_str) {
                if let Ok(parent_id) = parent_id_str.parse::<EntityId>() {
                    if let Some(membership) = self.get_membership(member_id, &parent_id)? {
                        memberships.push(membership);
                    }
                }
            }
        }

        Ok(memberships)
    }

    fn get_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>> {
        let prefix = Self::membership_prefix(parent_id);
        let mut memberships = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            let (_, value) =
                item.map_err(|e| EntityError::RegistryError(format!("Failed to scan: {e}")))?;
            memberships.push(Self::deserialize_membership(&value)?);
        }

        Ok(memberships)
    }

    fn update_membership(&mut self, membership: Membership) -> Result<()> {
        let key = Self::membership_key(&membership.parent_id, &membership.member_id);

        // Check if membership exists
        if !self
            .db
            .contains_key(&key)
            .map_err(|e| EntityError::RegistryError(format!("DB error: {e}")))?
        {
            return Err(EntityError::MembershipError("Membership not found".into()));
        }

        // Store updated membership
        let value = Self::serialize_membership(&membership)?;
        self.db
            .insert(&key, value)
            .map_err(|e| EntityError::RegistryError(format!("Failed to update membership: {e}")))?;

        debug!(
            member_id = %membership.member_id,
            parent_id = %membership.parent_id,
            "Membership updated"
        );
        Ok(())
    }

    fn remove_membership(&mut self, member_id: &EntityId, parent_id: &EntityId) -> Result<()> {
        let key = Self::membership_key(parent_id, member_id);

        // Check if membership exists
        if self
            .db
            .remove(&key)
            .map_err(|e| EntityError::RegistryError(format!("DB error: {e}")))?
            .is_none()
        {
            return Err(EntityError::MembershipError("Membership not found".into()));
        }

        // Remove member_of index
        let member_of_key = Self::member_of_index_key(member_id, parent_id);
        self.db
            .remove(&member_of_key)
            .map_err(|e| EntityError::RegistryError(format!("Failed to remove index: {e}")))?;

        debug!(
            member_id = %member_id,
            parent_id = %parent_id,
            "Membership removed"
        );
        Ok(())
    }

    fn member_count(&self, parent_id: &EntityId) -> Result<usize> {
        let prefix = Self::membership_prefix(parent_id);
        let count = self.db.scan_prefix(&prefix).count();
        Ok(count)
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

    fn create_test_coop(slug: &str) -> CooperativeEntity {
        CooperativeEntity::cooperative(slug, format!("Test Coop {slug}")).unwrap()
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = SledEntityRegistry::temporary().unwrap();
        let entity = create_test_coop("test-coop");
        let id = entity.id.clone();

        registry.register(entity.clone()).unwrap();

        let retrieved = registry.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.name, entity.name);
        assert_eq!(retrieved.id, entity.id);
    }

    #[test]
    fn test_duplicate_register_fails() {
        let mut registry = SledEntityRegistry::temporary().unwrap();
        let entity = create_test_coop("dupe-coop");

        registry.register(entity.clone()).unwrap();
        let result = registry.register(entity);

        assert!(matches!(result, Err(EntityError::AlreadyExists(_))));
    }

    #[test]
    fn test_update() {
        let mut registry = SledEntityRegistry::temporary().unwrap();
        let mut entity = create_test_coop("update-coop");
        let id = entity.id.clone();

        registry.register(entity.clone()).unwrap();

        entity.name = "Updated Name".to_string();
        registry.update(entity).unwrap();

        let retrieved = registry.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
    }

    #[test]
    fn test_update_nonexistent_fails() {
        let mut registry = SledEntityRegistry::temporary().unwrap();
        let entity = create_test_coop("noexist-coop");

        let result = registry.update(entity);
        assert!(matches!(result, Err(EntityError::NotFound(_))));
    }

    #[test]
    fn test_delete() {
        let mut registry = SledEntityRegistry::temporary().unwrap();
        let entity = create_test_coop("delete-coop");
        let id = entity.id.clone();

        registry.register(entity).unwrap();
        assert!(registry.exists(&id).unwrap());

        registry.delete(&id).unwrap();
        assert!(!registry.exists(&id).unwrap());
    }

    #[test]
    fn test_list_by_type() {
        let mut registry = SledEntityRegistry::temporary().unwrap();

        let coop = create_test_coop("list-coop");
        let individual = create_test_individual();

        registry.register(coop).unwrap();
        registry.register(individual).unwrap();

        let coops = registry.list_by_type(EntityType::Cooperative).unwrap();
        let individuals = registry.list_by_type(EntityType::Individual).unwrap();

        assert_eq!(coops.len(), 1);
        assert_eq!(individuals.len(), 1);
    }

    #[test]
    fn test_count() {
        let mut registry = SledEntityRegistry::temporary().unwrap();

        assert_eq!(registry.count().unwrap(), 0);

        registry.register(create_test_coop("count-coop1")).unwrap();
        assert_eq!(registry.count().unwrap(), 1);

        registry.register(create_test_coop("count-coop2")).unwrap();
        assert_eq!(registry.count().unwrap(), 2);
    }

    #[test]
    fn test_membership() {
        let mut registry = SledEntityRegistry::temporary().unwrap();

        let coop = create_test_coop("member-coop");
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
        let mut registry = SledEntityRegistry::temporary().unwrap();

        let coop = create_test_coop("delete-member-coop");
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
    fn test_remove_membership() {
        let mut registry = SledEntityRegistry::temporary().unwrap();

        let coop = create_test_coop("remove-member-coop");
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

        assert_eq!(registry.member_count(&coop_id).unwrap(), 1);

        registry
            .remove_membership(&individual_id, &coop_id)
            .unwrap();

        assert_eq!(registry.member_count(&coop_id).unwrap(), 0);
    }

    #[test]
    fn test_get_membership() {
        let mut registry = SledEntityRegistry::temporary().unwrap();

        let coop = create_test_coop("get-membership-coop");
        let coop_id = coop.id.clone();
        registry.register(coop).unwrap();

        let individual = create_test_individual();
        let individual_id = individual.id.clone();
        registry.register(individual).unwrap();

        // No membership yet
        assert!(registry
            .get_membership(&individual_id, &coop_id)
            .unwrap()
            .is_none());

        let membership = Membership::active(
            individual_id.clone(),
            coop_id.clone(),
            MembershipRole::Founder,
        );
        registry.add_membership(membership).unwrap();

        // Now it exists
        let retrieved = registry
            .get_membership(&individual_id, &coop_id)
            .unwrap()
            .unwrap();
        assert!(matches!(retrieved.role, MembershipRole::Founder));
    }

    #[test]
    fn test_update_membership() {
        let mut registry = SledEntityRegistry::temporary().unwrap();

        let coop = create_test_coop("update-membership-coop");
        let coop_id = coop.id.clone();
        registry.register(coop).unwrap();

        let individual = create_test_individual();
        let individual_id = individual.id.clone();
        registry.register(individual).unwrap();

        let membership = Membership::active(
            individual_id.clone(),
            coop_id.clone(),
            MembershipRole::Member,
        );
        registry.add_membership(membership).unwrap();

        // Update to BoardMember
        let mut updated = registry
            .get_membership(&individual_id, &coop_id)
            .unwrap()
            .unwrap();
        updated.role = MembershipRole::BoardMember;
        registry.update_membership(updated).unwrap();

        let retrieved = registry
            .get_membership(&individual_id, &coop_id)
            .unwrap()
            .unwrap();
        assert!(matches!(retrieved.role, MembershipRole::BoardMember));
    }

    #[test]
    fn test_invalid_membership_relationships() {
        let mut registry = SledEntityRegistry::temporary().unwrap();

        // Create entities
        let individual = create_test_individual();
        let individual_id = individual.id.clone();
        registry.register(individual).unwrap();

        let coop = create_test_coop("rel-test-coop");
        let coop_id = coop.id.clone();
        registry.register(coop).unwrap();

        let fed = CooperativeEntity::federation("rel-test-fed", "Test Federation").unwrap();
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
        let fed2 = CooperativeEntity::federation("rel-test-fed-2", "Test Federation 2").unwrap();
        let fed2_id = fed2.id.clone();
        registry.register(fed2).unwrap();

        let invalid = Membership::active(fed2_id, coop_id, MembershipRole::FederatedMember);
        let result = registry.add_membership(invalid);
        assert!(matches!(result, Err(EntityError::MembershipError(_))));
    }

    #[test]
    fn test_list_children() {
        let mut registry = SledEntityRegistry::temporary().unwrap();

        // Create a federation with cooperative members
        let fed = CooperativeEntity::federation("child-test-fed", "Test Federation").unwrap();
        let fed_id = fed.id.clone();
        registry.register(fed).unwrap();

        let coop1 = create_test_coop("child-test-coop1");
        let coop1_id = coop1.id.clone();
        registry.register(coop1).unwrap();

        let coop2 = create_test_coop("child-test-coop2");
        let coop2_id = coop2.id.clone();
        registry.register(coop2).unwrap();

        let individual = create_test_individual();
        let individual_id = individual.id.clone();
        registry.register(individual).unwrap();

        // Add coops and individual to federation
        registry
            .add_membership(Membership::active(
                coop1_id.clone(),
                fed_id.clone(),
                MembershipRole::FederatedMember,
            ))
            .unwrap();
        registry
            .add_membership(Membership::active(
                coop2_id.clone(),
                fed_id.clone(),
                MembershipRole::FederatedMember,
            ))
            .unwrap();
        registry
            .add_membership(Membership::active(
                individual_id,
                fed_id.clone(),
                MembershipRole::Member,
            ))
            .unwrap();

        // list_children should only return organizations (coops), not individuals
        let children = registry.list_children(&fed_id).unwrap();
        assert_eq!(children.len(), 2);
        assert!(children.contains(&coop1_id));
        assert!(children.contains(&coop2_id));
    }

    #[test]
    fn test_persistence_across_reopens() {
        // This test verifies data persists by creating a temp directory,
        // opening/closing the db, and checking data is still there
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("entity_test");

        let entity_id;

        // First session: create and register
        {
            let db = sled::open(&db_path).unwrap();
            let mut registry = SledEntityRegistry::new(Arc::new(db)).unwrap();

            let entity = create_test_coop("persist-coop");
            entity_id = entity.id.clone();
            registry.register(entity).unwrap();

            assert_eq!(registry.count().unwrap(), 1);
        }

        // Second session: reopen and verify
        {
            let db = sled::open(&db_path).unwrap();
            let registry = SledEntityRegistry::new(Arc::new(db)).unwrap();

            assert_eq!(registry.count().unwrap(), 1);
            let entity = registry.get(&entity_id).unwrap().unwrap();
            assert_eq!(entity.name, "Test Coop persist-coop");
        }
    }
}
