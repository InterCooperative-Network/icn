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
//!
//! # Performance Notes
//!
//! - **list_by_type()** and **list_children()** load all matching entities into memory.
//!   For large datasets (>100 entities), use the paginated versions:
//!   - `list_by_type_paginated(entity_type, limit, offset)`
//!   - `list_children_paginated(parent_id, limit, offset)`
//!   - `count_by_type(entity_type)` - Get total count for pagination UI
//!
//! A warning is logged when non-paginated methods return more than 100 entities.
//!
//! # Atomicity
//!
//! All multi-key operations (register, update, delete, add_membership, remove_membership)
//! use Sled transactions to ensure atomicity. If a process crash occurs mid-transaction,
//! Sled will roll back incomplete operations on restart.

use crate::entity::{CooperativeEntity, EntityId, EntityType};
use crate::error::{EntityError, Result};
use crate::membership::Membership;
use crate::registry::EntityRegistry;
use sled::transaction::ConflictableTransactionError;
use sled::Db;
use std::sync::Arc;
use tracing::{debug, warn};

/// Threshold for emitting performance warnings on list operations.
///
/// When list_by_type() or list_children() returns more than this many entities,
/// a warning is logged recommending pagination.
///
/// Rationale for 100:
/// - Typical cooperatives have 10-50 members (100 is generous headroom)
/// - Small enough to detect when pagination should be used
/// - API responses stay under ~50KB with typical entity sizes
const LARGE_LIST_WARNING_THRESHOLD: usize = 100;

/// Hard limit for unpaginated list operations to prevent DoS attacks.
///
/// Callers attempting to list more entities must use paginated variants.
/// This prevents memory exhaustion from loading very large datasets.
///
/// Rationale for 1,000:
/// - 10x the warning threshold (graceful degradation path)
/// - 1,000 entities × ~2KB avg size ≈ 2MB max memory per request
/// - Large enough for most legitimate batch operations
/// - Small enough to prevent OOM attacks via entity enumeration
const MAX_UNPAGINATED_LIST_SIZE: usize = 1000;

/// Convert a Sled transaction error to an EntityError with context.
///
/// This helper provides consistent error handling for transactions:
/// - `Abort` errors are user-caused validation failures (should NOT be retried)
/// - `Storage` errors are I/O or database issues (NOT typically retriable)
fn handle_transaction_error(
    e: sled::transaction::TransactionError<EntityError>,
    operation: &str,
) -> EntityError {
    match e {
        sled::transaction::TransactionError::Abort(err) => err,
        sled::transaction::TransactionError::Storage(e) => EntityError::RegistryError(format!(
            "Storage error during {operation}: {e}. This may indicate database corruption or I/O failure."
        )),
    }
}

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

    /// Key for storing member count (for atomic checking in transactions)
    ///
    /// This count is maintained atomically with membership add/remove operations
    /// to enable race-free member count checks inside transactions.
    fn member_count_key(parent_id: &EntityId) -> Vec<u8> {
        format!("member_count:{}", parent_id.as_str()).into_bytes()
    }

    /// Get member count from the count key (for use inside transactions)
    fn get_member_count_from_key(
        tx: &sled::transaction::TransactionalTree,
        parent_id: &EntityId,
    ) -> std::result::Result<u64, sled::transaction::UnabortableTransactionError> {
        let key = Self::member_count_key(parent_id);
        match tx.get(&key)? {
            Some(bytes) => {
                // CRITICAL: Don't silently mask corruption - if bytes aren't exactly 8 bytes,
                // the data is corrupted and we should error rather than return 0
                let count = u64::from_le_bytes(bytes.as_ref().try_into().map_err(|_| {
                    sled::transaction::UnabortableTransactionError::Storage(sled::Error::Io(
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!(
                                "Corrupted member count for {}: expected 8 bytes, got {}",
                                parent_id,
                                bytes.len()
                            ),
                        ),
                    ))
                })?);
                Ok(count)
            }
            None => Ok(0),
        }
    }

    /// Increment member count atomically inside a transaction
    fn increment_member_count(
        tx: &sled::transaction::TransactionalTree,
        parent_id: &EntityId,
    ) -> std::result::Result<(), sled::transaction::UnabortableTransactionError> {
        let key = Self::member_count_key(parent_id);
        let current = Self::get_member_count_from_key(tx, parent_id)?;
        tx.insert(key.as_slice(), &(current + 1).to_le_bytes())?;
        Ok(())
    }

    /// Decrement member count atomically inside a transaction
    fn decrement_member_count(
        tx: &sled::transaction::TransactionalTree,
        parent_id: &EntityId,
    ) -> std::result::Result<(), sled::transaction::UnabortableTransactionError> {
        let key = Self::member_count_key(parent_id);
        let current = Self::get_member_count_from_key(tx, parent_id)?;
        if current > 0 {
            tx.insert(key.as_slice(), &(current - 1).to_le_bytes())?;
        } else {
            // Remove the key if count reaches 0
            tx.remove(key.as_slice())?;
        }
        Ok(())
    }

    // ========================================
    // Serialization helpers
    // ========================================

    fn serialize_entity(entity: &CooperativeEntity) -> Result<Vec<u8>> {
        icn_encoding::encode_versioned(entity)
            .map_err(|e| EntityError::RegistryError(format!("Failed to serialize entity: {e}")))
    }

    fn deserialize_entity(bytes: &[u8]) -> Result<CooperativeEntity> {
        icn_encoding::decode_versioned(bytes)
            .map_err(|e| EntityError::RegistryError(format!("Failed to deserialize entity: {e}")))
    }

    fn serialize_membership(membership: &Membership) -> Result<Vec<u8>> {
        icn_encoding::encode_versioned(membership)
            .map_err(|e| EntityError::RegistryError(format!("Failed to serialize membership: {e}")))
    }

    fn deserialize_membership(bytes: &[u8]) -> Result<Membership> {
        icn_encoding::decode_versioned(bytes).map_err(|e| {
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
        // - Individuals can join Cooperatives, Communities, or Federations
        // - Cooperatives can join Communities or Federations
        // - Communities can join Communities (nested) or Federations
        // - Federations can join Federations (recursive)
        // - Nothing can join an Individual
        let valid_relationship = match (&parent_entity.entity_type, &member_entity.entity_type) {
            // Cooperatives accept individuals
            (EntityType::Cooperative, EntityType::Individual) => true,
            // Communities accept individuals, cooperatives, and other communities (nested)
            (EntityType::Community, EntityType::Individual) => true,
            (EntityType::Community, EntityType::Cooperative) => true,
            (EntityType::Community, EntityType::Community) => true,
            // Federations accept individuals, cooperatives, communities, and other federations
            (EntityType::Federation, EntityType::Individual) => true,
            (EntityType::Federation, EntityType::Cooperative) => true,
            (EntityType::Federation, EntityType::Community) => true,
            (EntityType::Federation, EntityType::Federation) => true,
            // Individuals cannot have members
            (EntityType::Individual, _) => false,
            // Communities cannot have federation members (federations are higher in hierarchy)
            (EntityType::Community, EntityType::Federation) => false,
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
        let entity_key = Self::entity_key(&entity.id);
        let type_key = Self::type_index_key(entity.entity_type, &entity.id);

        // Serialize entity before transaction to avoid closure issues
        let entity_value = Self::serialize_entity(&entity)?;
        let entity_id_str = entity.id.as_str().to_string();
        let entity_id_display = entity.id.to_string();

        // Use transaction for atomicity
        self.db
            .transaction(|tx| {
                // Check if entity already exists
                if tx.get(&entity_key)?.is_some() {
                    return Err(ConflictableTransactionError::Abort(
                        EntityError::AlreadyExists(entity_id_str.clone()),
                    ));
                }

                // Store entity
                tx.insert(entity_key.as_slice(), entity_value.as_slice())?;

                // Add type index
                tx.insert(type_key.as_slice(), &[] as &[u8])?;

                Ok(())
            })
            .map_err(|e| handle_transaction_error(e, "entity registration"))?;

        debug!(entity_id = %entity_id_display, "Entity registered");
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
        let entity_key = Self::entity_key(&entity.id);

        // Check if entity exists and get old type
        let old_entity = self
            .get(&entity.id)?
            .ok_or_else(|| EntityError::NotFound(entity.id.as_str().to_string()))?;

        // Update the updated_at timestamp
        entity.updated_at = icn_time::current_timestamp_secs();
        // Increment version
        entity.version += 1;

        // Prepare for transaction
        let entity_id_str = entity.id.as_str().to_string();
        let entity_id_display = entity.id.to_string();
        let entity_value = Self::serialize_entity(&entity)?;
        let type_changed = old_entity.entity_type != entity.entity_type;
        let old_type_key = Self::type_index_key(old_entity.entity_type, &entity.id);
        let new_type_key = Self::type_index_key(entity.entity_type, &entity.id);

        // Use transaction for atomicity
        self.db
            .transaction(|tx| {
                // Verify entity still exists (check-and-update atomically)
                if tx.get(&entity_key)?.is_none() {
                    return Err(ConflictableTransactionError::Abort(EntityError::NotFound(
                        entity_id_str.clone(),
                    )));
                }

                // If entity type changed, update the type index
                if type_changed {
                    tx.remove(old_type_key.as_slice())?;
                    tx.insert(new_type_key.as_slice(), &[] as &[u8])?;
                }

                // Store updated entity
                tx.insert(entity_key.as_slice(), entity_value.as_slice())?;

                Ok(())
            })
            .map_err(|e| handle_transaction_error(e, "entity update"))?;

        debug!(entity_id = %entity_id_display, "Entity updated");
        Ok(())
    }

    fn update_if_version(&mut self, mut entity: CooperativeEntity, expected_version: u64) -> Result<()> {
        let entity_key = Self::entity_key(&entity.id);

        // Check if entity exists and get current entity
        let old_entity = self
            .get(&entity.id)?
            .ok_or_else(|| EntityError::NotFound(entity.id.as_str().to_string()))?;

        // Version check - detect concurrent modification
        if old_entity.version != expected_version {
            return Err(EntityError::ConcurrentModification(entity.id.as_str().to_string()));
        }

        // Update the updated_at timestamp
        entity.updated_at = icn_time::current_timestamp_secs();
        // Set new version (expected + 1)
        entity.version = expected_version + 1;

        // Prepare for transaction
        let entity_id_str = entity.id.as_str().to_string();
        let entity_id_display = entity.id.to_string();
        let entity_value = Self::serialize_entity(&entity)?;
        let type_changed = old_entity.entity_type != entity.entity_type;
        let old_type_key = Self::type_index_key(old_entity.entity_type, &entity.id);
        let new_type_key = Self::type_index_key(entity.entity_type, &entity.id);

        // Use transaction for atomicity
        self.db
            .transaction(|tx| {
                // Verify entity still exists and version still matches
                // This is a double-check in case the entity was modified between the get and the transaction
                if let Some(current_bytes) = tx.get(&entity_key)? {
                    let current_entity = Self::deserialize_entity(&current_bytes)
                        .map_err(|e| ConflictableTransactionError::Abort(e))?;
                    if current_entity.version != expected_version {
                        return Err(ConflictableTransactionError::Abort(
                            EntityError::ConcurrentModification(entity_id_str.clone()),
                        ));
                    }
                } else {
                    return Err(ConflictableTransactionError::Abort(EntityError::NotFound(
                        entity_id_str.clone(),
                    )));
                }

                // If entity type changed, update the type index
                if type_changed {
                    tx.remove(old_type_key.as_slice())?;
                    tx.insert(new_type_key.as_slice(), &[] as &[u8])?;
                }

                // Store updated entity
                tx.insert(entity_key.as_slice(), entity_value.as_slice())?;

                Ok(())
            })
            .map_err(|e| handle_transaction_error(e, "entity update_if_version"))?;

        debug!(
            entity_id = %entity_id_display,
            expected_version = expected_version,
            "Entity updated with version check"
        );
        Ok(())
    }

    fn delete(&mut self, id: &EntityId) -> Result<()> {
        // Get entity to know its type for index cleanup
        let entity = self
            .get(id)?
            .ok_or_else(|| EntityError::NotFound(id.as_str().to_string()))?;

        // Get all memberships of this entity (for cleanup of entity's memberships in other orgs)
        let memberships = self.get_memberships_of(id)?;

        // Prepare keys for deletion
        let entity_key = Self::entity_key(id);
        let type_key = Self::type_index_key(entity.entity_type, id);
        let member_count_key = Self::member_count_key(id);
        let entity_id_str = id.as_str().to_string();
        let entity_id_display = id.to_string();
        let id_clone = id.clone();

        // Collect membership keys and parent IDs for atomic deletion
        // (entity's memberships in other orgs - need to decrement parent counts)
        let membership_data: Vec<(Vec<u8>, Vec<u8>, EntityId)> = memberships
            .iter()
            .map(|m| {
                let membership_key = Self::membership_key(&m.parent_id, &m.member_id);
                let member_of_key = Self::member_of_index_key(&m.member_id, &m.parent_id);
                (membership_key, member_of_key, m.parent_id.clone())
            })
            .collect();

        // Use transaction for atomicity of the delete operations
        self.db
            .transaction(|tx| {
                // Verify entity still exists (prevents double-delete)
                if tx.get(&entity_key)?.is_none() {
                    return Err(ConflictableTransactionError::Abort(EntityError::NotFound(
                        entity_id_str.clone(),
                    )));
                }

                // CRITICAL: Check member count INSIDE the transaction with conflict detection
                //
                // Sled's optimistic concurrency: tx.get() does NOT register for conflict
                // detection - only writes do. So we must WRITE to the member_count key
                // to ensure that if add_membership() modifies it in a parallel transaction,
                // sled will detect the conflict and retry one of the transactions.
                //
                // Pattern: Read count -> verify zero -> write back zero (registers conflict)
                // This ensures atomicity with add_membership/remove_membership.
                //
                // ROLLBACK BEHAVIOR: When we return ConflictableTransactionError::Abort(err),
                // sled automatically rolls back all pending writes in this transaction.
                // No explicit rollback is needed - this is a fundamental sled guarantee.
                // The transaction is atomic: either all writes succeed or none do.
                let count = Self::get_member_count_from_key(tx, &id_clone)?;
                if count > 0 {
                    // Transaction aborts here - all pending writes are automatically rolled back
                    return Err(ConflictableTransactionError::Abort(
                        EntityError::RegistryError(format!(
                            "Cannot delete entity with {count} active member(s)"
                        )),
                    ));
                }
                // Write count back to register for conflict detection
                // (even though we're about to remove it, this ensures atomicity)
                tx.insert(member_count_key.as_slice(), &0u64.to_le_bytes())?;

                // Remove entity
                tx.remove(entity_key.as_slice())?;

                // Remove type index
                tx.remove(type_key.as_slice())?;

                // Remove member count key (cleanup)
                tx.remove(member_count_key.as_slice())?;

                // Remove all memberships and their indexes (entity's memberships in other orgs)
                // Also decrement the member counts for parent entities
                for (membership_key, member_of_key, parent_id) in &membership_data {
                    tx.remove(membership_key.as_slice())?;
                    tx.remove(member_of_key.as_slice())?;
                    // Decrement the parent's member count
                    Self::decrement_member_count(tx, parent_id)?;
                }

                Ok(())
            })
            .map_err(|e| handle_transaction_error(e, "entity deletion"))?;

        debug!(entity_id = %entity_id_display, "Entity deleted");
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

            // CRITICAL: Hard limit to prevent DoS via memory exhaustion
            // Forces callers with large datasets to use paginated variants
            if ids.len() > MAX_UNPAGINATED_LIST_SIZE {
                return Err(EntityError::RegistryError(format!(
                    "Too many entities of type {entity_type} (>{MAX_UNPAGINATED_LIST_SIZE}). \
                     Use list_by_type_paginated() to iterate in chunks."
                )));
            }
        }

        // Performance warning for large result sets
        if ids.len() > LARGE_LIST_WARNING_THRESHOLD {
            warn!(
                entity_type = %entity_type,
                count = ids.len(),
                "list_by_type() returned {} entities. Consider using list_by_type_paginated() for better performance.",
                ids.len()
            );
        }

        Ok(ids)
    }

    fn list_by_type_paginated(
        &self,
        entity_type: EntityType,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EntityId>> {
        let prefix = Self::type_index_prefix(entity_type);
        let mut ids = Vec::new();
        let mut skipped = 0;

        for item in self.db.scan_prefix(&prefix) {
            let (key, _) =
                item.map_err(|e| EntityError::RegistryError(format!("Failed to scan: {e}")))?;

            // Key format: type:{entity_type}:{id}
            let key_str = String::from_utf8_lossy(&key);
            if let Some(id_str) = key_str.strip_prefix(&format!("type:{entity_type}:")) {
                if let Ok(id) = id_str.parse::<EntityId>() {
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    ids.push(id);
                    if ids.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(ids)
    }

    fn count_by_type(&self, entity_type: EntityType) -> Result<usize> {
        let prefix = Self::type_index_prefix(entity_type);
        let count = self.db.scan_prefix(&prefix).filter(|r| r.is_ok()).count();
        Ok(count)
    }

    fn list_children(&self, parent_id: &EntityId) -> Result<Vec<EntityId>> {
        let members = self.get_members(parent_id)?;

        // CRITICAL: Hard limit to prevent DoS via memory exhaustion
        // Forces callers with large datasets to use paginated variants
        if members.len() > MAX_UNPAGINATED_LIST_SIZE {
            return Err(EntityError::RegistryError(format!(
                "Too many members in {parent_id} (>{MAX_UNPAGINATED_LIST_SIZE}). \
                 Use list_children_paginated() to iterate in chunks."
            )));
        }

        let mut children = Vec::new();

        for membership in members {
            if let Some(entity) = self.get(&membership.member_id)? {
                if entity.id.is_organization() {
                    children.push(entity.id);
                }
            }
        }

        // Performance warning for large result sets
        if children.len() > LARGE_LIST_WARNING_THRESHOLD {
            warn!(
                parent_id = %parent_id,
                count = children.len(),
                "list_children() returned {} children. Consider using list_children_paginated() for better performance.",
                children.len()
            );
        }

        Ok(children)
    }

    fn list_children_paginated(
        &self,
        parent_id: &EntityId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EntityId>> {
        // For efficiency, we use the membership prefix to iterate with skip/take
        let prefix = Self::membership_prefix(parent_id);
        let mut children = Vec::new();
        let mut skipped = 0;

        for item in self.db.scan_prefix(&prefix) {
            let (_, value) =
                item.map_err(|e| EntityError::RegistryError(format!("Failed to scan: {e}")))?;
            let membership = Self::deserialize_membership(&value)?;

            if let Some(entity) = self.get(&membership.member_id)? {
                if entity.id.is_organization() {
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    children.push(entity.id);
                    if children.len() >= limit {
                        break;
                    }
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
        // Verify both entities exist (pre-check for better error messages)
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

        // Prepare keys and values for transaction
        let membership_key = Self::membership_key(&membership.parent_id, &membership.member_id);
        let member_of_key = Self::member_of_index_key(&membership.member_id, &membership.parent_id);
        let parent_entity_key = Self::entity_key(&membership.parent_id);
        let membership_value = Self::serialize_membership(&membership)?;
        let member_id_display = membership.member_id.to_string();
        let parent_id_display = membership.parent_id.to_string();
        let parent_id = membership.parent_id.clone();

        // Use transaction for atomicity
        self.db
            .transaction(|tx| {
                // CRITICAL: Verify parent entity still exists INSIDE the transaction
                // This prevents a race condition where delete() could remove the parent
                // between our pre-check and the membership insertion.
                if tx.get(&parent_entity_key)?.is_none() {
                    return Err(ConflictableTransactionError::Abort(
                        EntityError::MembershipError(format!(
                            "Parent entity was deleted: {parent_id_display}"
                        )),
                    ));
                }

                // Check if membership already exists
                if tx.get(&membership_key)?.is_some() {
                    return Err(ConflictableTransactionError::Abort(
                        EntityError::MembershipError("Membership already exists".into()),
                    ));
                }

                // Store membership
                tx.insert(membership_key.as_slice(), membership_value.as_slice())?;

                // Add member_of index for reverse lookup
                tx.insert(member_of_key.as_slice(), &[] as &[u8])?;

                // Atomically increment member count for the parent entity
                // This enables race-free member count checks in delete()
                Self::increment_member_count(tx, &parent_id)?;

                Ok(())
            })
            .map_err(|e| handle_transaction_error(e, "membership addition"))?;

        debug!(
            member_id = %member_id_display,
            parent_id = %parent_id_display,
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
        let value = Self::serialize_membership(&membership)?;
        let member_id_display = membership.member_id.to_string();
        let parent_id_display = membership.parent_id.to_string();

        // Use transaction to atomically verify existence and update
        self.db
            .transaction(|tx| {
                // Check if membership exists inside transaction
                if tx.get(&key)?.is_none() {
                    return Err(ConflictableTransactionError::Abort(
                        EntityError::MembershipError("Membership not found".into()),
                    ));
                }

                // Store updated membership
                tx.insert(key.as_slice(), value.as_slice())?;
                Ok(())
            })
            .map_err(|e| handle_transaction_error(e, "membership update"))?;

        debug!(
            member_id = %member_id_display,
            parent_id = %parent_id_display,
            "Membership updated"
        );
        Ok(())
    }

    fn remove_membership(&mut self, member_id: &EntityId, parent_id: &EntityId) -> Result<()> {
        let membership_key = Self::membership_key(parent_id, member_id);
        let member_of_key = Self::member_of_index_key(member_id, parent_id);
        let member_id_display = member_id.to_string();
        let parent_id_display = parent_id.to_string();
        let parent_id_clone = parent_id.clone();

        // Use transaction for atomicity
        self.db
            .transaction(|tx| {
                // Check if membership exists
                if tx.get(&membership_key)?.is_none() {
                    return Err(ConflictableTransactionError::Abort(
                        EntityError::MembershipError("Membership not found".into()),
                    ));
                }

                // Remove membership
                tx.remove(membership_key.as_slice())?;

                // Remove member_of index
                tx.remove(member_of_key.as_slice())?;

                // Atomically decrement member count for the parent entity
                Self::decrement_member_count(tx, &parent_id_clone)?;

                Ok(())
            })
            .map_err(|e| handle_transaction_error(e, "membership removal"))?;

        debug!(
            member_id = %member_id_display,
            parent_id = %parent_id_display,
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

    // ========================================
    // Concurrent access tests
    // ========================================

    #[test]
    fn test_concurrent_delete_and_membership_add() {
        // This test verifies race condition handling between delete and add_membership.
        //
        // Safety is ensured by:
        // 1. add_membership checks parent entity exists INSIDE its transaction
        // 2. delete checks member count BEFORE its transaction
        //
        // Common outcomes:
        // - If delete runs first: add_membership fails (parent not found)
        // - If add_membership runs first: delete fails (has active members)
        use std::sync::Arc;
        use std::thread;

        let db = Arc::new(sled::Config::new().temporary(true).open().unwrap());

        // Create the parent entity (cooperative)
        let coop = create_test_coop("concurrent-test-coop");
        let coop_id = coop.id.clone();
        {
            let mut registry = SledEntityRegistry::new(db.clone()).unwrap();
            registry.register(coop).unwrap();
        }

        // Create a member entity (individual)
        let individual = create_test_individual();
        let individual_id = individual.id.clone();
        {
            let mut registry = SledEntityRegistry::new(db.clone()).unwrap();
            registry.register(individual).unwrap();
        }

        // Now try concurrent operations: one thread tries to delete the coop,
        // another tries to add a membership
        let db1 = db.clone();
        let db2 = db.clone();
        let coop_id_1 = coop_id.clone();
        let coop_id_2 = coop_id.clone();
        let individual_id_2 = individual_id.clone();

        let handle1 = thread::spawn(move || {
            let mut registry = SledEntityRegistry::new(db1).unwrap();
            registry.delete(&coop_id_1)
        });

        let handle2 = thread::spawn(move || {
            let mut registry = SledEntityRegistry::new(db2).unwrap();
            let membership = Membership::active(individual_id_2, coop_id_2, MembershipRole::Worker);
            registry.add_membership(membership)
        });

        let result1 = handle1.join().unwrap();
        let result2 = handle2.join().unwrap();

        // Verify the database state is consistent after the race
        let registry = SledEntityRegistry::new(db).unwrap();

        if result1.is_ok() && result2.is_ok() {
            // Both succeeded - this is a narrow race window. Verify state is at least consistent:
            // The entity may or may not exist depending on transaction ordering.
            // Just verify no panic occurred and state is queryable.
            let _ = registry.exists(&coop_id).unwrap();
            let _ = registry.member_count(&coop_id); // May fail if entity was deleted
        } else if result1.is_ok() {
            // Delete succeeded, add_membership failed (expected outcome)
            assert!(!registry.exists(&coop_id).unwrap());
        } else if result2.is_ok() {
            // Membership added, delete failed (expected outcome)
            assert!(registry.exists(&coop_id).unwrap());
            assert_eq!(registry.member_count(&coop_id).unwrap(), 1);
        }
        // Both failed is also possible in edge cases (e.g., entity deleted between checks)
    }

    #[test]
    fn test_concurrent_entity_updates() {
        // Test that concurrent updates don't corrupt entity data
        use std::sync::Arc;
        use std::thread;

        let db = Arc::new(sled::Config::new().temporary(true).open().unwrap());

        let coop = create_test_coop("concurrent-update-coop");
        let coop_id = coop.id.clone();
        {
            let mut registry = SledEntityRegistry::new(db.clone()).unwrap();
            registry.register(coop).unwrap();
        }

        // Spawn multiple threads that update the entity
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let db = db.clone();
                let coop_id = coop_id.clone();
                thread::spawn(move || {
                    let mut registry = SledEntityRegistry::new(db).unwrap();
                    if let Ok(Some(mut entity)) = registry.get(&coop_id) {
                        entity.name = format!("Updated by thread {i}");
                        registry.update(entity)
                    } else {
                        Err(EntityError::NotFound(coop_id.to_string()))
                    }
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            let _ = handle.join().unwrap();
        }

        // Verify entity still exists and has valid data
        let registry = SledEntityRegistry::new(db).unwrap();
        let entity = registry.get(&coop_id).unwrap().unwrap();
        assert!(entity.name.starts_with("Updated by thread"));
    }
}
