//! Entity registry initialization
//!
//! Provides entity registry services for cooperative entities. The registry
//! stores cooperative/federation organizational structures and memberships.

use anyhow::Result;
use icn_entity::{EntityRegistry, InMemoryRegistry};
use std::sync::{Arc, RwLock};
use tracing::info;

/// Handle type for entity registry
///
/// This provides thread-safe access to the entity registry.
/// For now, we use InMemoryRegistry which is not persistent.
/// A future enhancement would add SledEntityRegistry for persistence.
pub type EntityHandle = Arc<RwLock<dyn EntityRegistry + Send + Sync>>;

/// Services provided by entity layer
pub struct EntityServices {
    /// Handle for interacting with entity registry
    pub entity_handle: EntityHandle,
}

/// Initialize entity services
///
/// This creates an entity registry for managing cooperative entities
/// and their memberships. The registry is wrapped in Arc<RwLock> for
/// thread-safe concurrent access.
///
/// # Note
/// Currently uses InMemoryRegistry which is not persistent across restarts.
/// For production use, this should be replaced with a Sled-backed implementation.
pub fn init_entity_services() -> Result<EntityServices> {
    info!("Initializing entity services");

    // Create in-memory registry
    // TODO: Replace with SledEntityRegistry for persistence
    let registry = InMemoryRegistry::new();
    let entity_handle: EntityHandle = Arc::new(RwLock::new(registry));

    info!("Entity registry initialized (in-memory mode)");

    Ok(EntityServices { entity_handle })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_entity::{CooperativeEntity, Membership, MembershipRole};
    use icn_identity::KeyPair;

    #[test]
    fn test_init_entity_services() {
        let services = init_entity_services().unwrap();
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
        let services = init_entity_services().unwrap();
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
