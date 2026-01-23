//! Entity registry initialization with gossip synchronization
//!
//! Provides entity registry services for cooperative entities. The registry
//! stores cooperative/federation organizational structures and memberships,
//! with gossip-based synchronization across nodes.

use anyhow::Result;
use icn_entity::{EntityActor, EntityHandle, SledEntityRegistry, ENTITY_TOPIC};
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_identity::Did;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::config::Config;

/// Gossip topic for entity updates
pub const ENTITY_UPDATES_TOPIC: &str = ENTITY_TOPIC;

/// Handle type for gossip actor
pub type GossipHandle = Arc<RwLock<GossipActor>>;

/// Services provided by entity layer
pub struct EntityServices {
    /// Handle for interacting with the entity actor
    pub entity_handle: EntityHandle,
    /// Direct access to the underlying registry (for edge cases)
    pub entity_registry: Arc<SledEntityRegistry>,
}

/// Initialize entity services with persistent storage and gossip synchronization
///
/// This creates an entity registry for managing cooperative entities
/// and their memberships. The registry is backed by Sled for persistence
/// and is managed by an EntityActor that handles gossip synchronization.
///
/// # Arguments
///
/// * `config` - Configuration containing the store path
/// * `gossip_handle` - Handle to the gossip actor for subscribing and publishing
/// * `node_did` - The DID of this node for gossip subscriptions
pub async fn init_entity_services(
    config: &Config,
    gossip_handle: GossipHandle,
    node_did: Did,
) -> Result<EntityServices> {
    info!("Initializing entity services with gossip synchronization");

    // Create the persistent registry with shared database
    //
    // Note: We create two SledEntityRegistry instances sharing the same Sled database:
    // 1. `entity_registry` - Exposed for potential direct access (e.g., gateway reads)
    // 2. `actor_registry` - Owned by the EntityActor for all operations
    //
    // Both registries share the same underlying Sled Db via Arc, ensuring
    // consistent reads across the codebase while the actor manages all writes.
    let entity_store_path = config.store_path().join("entities");
    let db = Arc::new(sled::open(&entity_store_path)?);

    let registry = SledEntityRegistry::new(db.clone())?;
    let entity_registry = Arc::new(registry);

    // Create topic and subscribe to entity updates
    {
        let mut gossip = gossip_handle.write().await;

        // Create the entity topic if it doesn't exist
        let topic = Topic::new(ENTITY_UPDATES_TOPIC.to_string(), AccessControl::Public);
        gossip.create_topic(topic);

        // Subscribe this node to entity updates for receiving sync from peers
        if let Err(e) = gossip.subscribe(ENTITY_UPDATES_TOPIC, node_did).await {
            tracing::warn!("Failed to subscribe to entity:updates topic: {}", e);
        } else {
            info!("Subscribed to entity:updates topic");
        }
    }

    // Spawn the entity actor with gossip support using shared database
    let actor_registry = SledEntityRegistry::new(db)?;

    let tx = EntityActor::spawn(actor_registry, Some(gossip_handle));
    let entity_handle = EntityHandle::new(tx);

    info!(
        "✓ Entity registry initialized at {} with gossip sync on topic '{}'",
        entity_store_path.display(),
        ENTITY_UPDATES_TOPIC
    );

    Ok(EntityServices {
        entity_handle,
        entity_registry,
    })
}

/// Initialize entity services without gossip (for standalone testing)
///
/// This creates an entity registry without gossip synchronization.
/// Useful for unit tests and standalone gateway operation.
pub fn init_entity_services_standalone(config: &Config) -> Result<EntityServices> {
    info!("Initializing entity services (standalone mode, no gossip)");

    // Create shared database
    let entity_store_path = config.store_path().join("entities");
    let db = Arc::new(sled::open(&entity_store_path)?);

    let registry = SledEntityRegistry::new(db.clone())?;
    let entity_registry = Arc::new(registry);

    // Spawn actor without gossip using shared database
    let actor_registry = SledEntityRegistry::new(db)?;

    let tx = EntityActor::spawn(actor_registry, None);
    let entity_handle = EntityHandle::new(tx);

    info!(
        "✓ Entity registry initialized at {} (standalone mode)",
        entity_store_path.display()
    );

    Ok(EntityServices {
        entity_handle,
        entity_registry,
    })
}

/// Initialize entity services with a custom path (for testing)
#[cfg(test)]
pub fn init_entity_services_with_path(path: &std::path::Path) -> Result<EntityServices> {
    // Open the database once and share it between the registry and actor
    let db = Arc::new(sled::open(path)?);

    // Create the reference registry (for tests that need direct access)
    let registry = SledEntityRegistry::new(db.clone())?;
    let entity_registry = Arc::new(registry);

    // Create actor registry using the same shared db
    let actor_registry = SledEntityRegistry::new(db)?;
    let tx = EntityActor::spawn(actor_registry, None);
    let entity_handle = EntityHandle::new(tx);

    Ok(EntityServices {
        entity_handle,
        entity_registry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_entity::{CooperativeEntity, Membership, MembershipRole};
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_init_entity_services() {
        let temp_dir = tempfile::tempdir().unwrap();
        let services = init_entity_services_with_path(temp_dir.path()).unwrap();
        let handle = services.entity_handle;

        // Register an entity
        let entity = CooperativeEntity::cooperative("test-coop", "Test Coop").unwrap();
        let entity_id = entity.id.clone();

        handle.register(entity).await.unwrap();

        // Get it back
        let retrieved = handle.get(&entity_id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Test Coop");
    }

    #[tokio::test]
    async fn test_entity_services_membership() {
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

        handle.register(coop).await.unwrap();
        handle.register(individual).await.unwrap();

        // Add membership
        let membership =
            Membership::new(member_id.clone(), coop_id.clone(), MembershipRole::Member);
        handle.add_membership(membership).await.unwrap();

        // Check membership
        let members = handle.get_members(&coop_id).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].member_id, member_id);
    }
}
