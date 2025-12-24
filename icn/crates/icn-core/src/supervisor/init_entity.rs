//! Entity registry initialization
//!
//! Provides entity registry services for cooperative entities. The registry
//! stores cooperative/federation organizational structures and memberships.

use anyhow::Result;
use icn_entity::{EntityRegistry, SledEntityRegistry};
use std::sync::{Arc, RwLock};
use tracing::info;

use crate::config::Config;

/// Handle type for entity registry
///
/// This provides thread-safe access to the entity registry.
/// Uses SledEntityRegistry for persistent storage across daemon restarts.
pub type EntityHandle = Arc<RwLock<dyn EntityRegistry + Send + Sync>>;

/// Services provided by entity layer
pub struct EntityServices {
    /// Handle for interacting with entity registry
    pub entity_handle: EntityHandle,
}

/// Initialize entity services with persistent storage
///
/// This creates an entity registry for managing cooperative entities
/// and their memberships. The registry is backed by Sled for persistence
/// and wrapped in Arc<RwLock> for thread-safe concurrent access.
///
/// # Arguments
///
/// * `config` - Configuration containing the store path
pub fn init_entity_services(config: &Config) -> Result<EntityServices> {
    info!("Initializing entity services");

    let entity_store_path = config.store_path().join("entities");
    let db = sled::open(&entity_store_path)?;
    let registry = SledEntityRegistry::new(Arc::new(db))?;
    let entity_handle: EntityHandle = Arc::new(RwLock::new(registry));

    info!(
        "✓ Entity registry initialized at {}",
        entity_store_path.display()
    );

    Ok(EntityServices { entity_handle })
}

/// Initialize entity services with a custom path (for testing)
#[cfg(test)]
pub fn init_entity_services_with_path(path: &std::path::Path) -> Result<EntityServices> {
    let db = sled::open(path)?;
    let registry = SledEntityRegistry::new(Arc::new(db))?;
    let entity_handle: EntityHandle = Arc::new(RwLock::new(registry));
    Ok(EntityServices { entity_handle })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_entity::{CooperativeEntity, Membership, MembershipRole};
    use icn_identity::KeyPair;

    #[test]
    fn test_init_entity_services() {
        let temp_dir = tempfile::tempdir().unwrap();
        let services = init_entity_services_with_path(temp_dir.path()).unwrap();
        let handle = services.entity_handle;

        // Register an entity
        let entity = CooperativeEntity::cooperative("test-coop", "Test Coop").unwrap();
        let entity_id = entity.id.clone();

        {
            let mut registry = handle.write().unwrap();
            registry.register(entity).unwrap();
        }

        // Get it back
        {
            let registry = handle.read().unwrap();
            let retrieved = registry.get(&entity_id).unwrap().unwrap();
            assert_eq!(retrieved.name, "Test Coop");
        }
    }

    #[test]
    fn test_entity_services_membership() {
        let temp_dir = tempfile::tempdir().unwrap();
        let services = init_entity_services_with_path(temp_dir.path()).unwrap();
        let handle = services.entity_handle;

        // Create a cooperative
        let coop = CooperativeEntity::cooperative("membership-test", "Membership Test").unwrap();
        let coop_id = coop.id.clone();

        // Create an individual member
        let keypair = KeyPair::generate().unwrap();
        let individual = CooperativeEntity::individual(keypair.did(), "Test Member");
        let member_id = individual.id.clone();

        {
            let mut registry = handle.write().unwrap();
            registry.register(coop).unwrap();
            registry.register(individual).unwrap();

            // Add membership
            let membership =
                Membership::new(member_id.clone(), coop_id.clone(), MembershipRole::Member);
            registry.add_membership(membership).unwrap();
        }

        // Check membership
        {
            let registry = handle.read().unwrap();
            let members = registry.get_members(&coop_id).unwrap();
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].member_id, member_id);
        }
    }
}
