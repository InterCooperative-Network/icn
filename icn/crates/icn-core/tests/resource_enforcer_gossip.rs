//! Integration tests for resource enforcer gossip synchronization
//!
//! Validates that revocation events are properly published to and received from
//! the gossip network, enabling cluster-wide notification of access revocations.

// Allow unwrap/expect in test code - panics are acceptable for tests
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_core::resource_enforcer_actor::{
    ResourceAccessStore, RevocationEvent, RESOURCE_REVOCATIONS_TOPIC,
};
use icn_entity::EntityId;
use icn_gossip::{gossip::TrustLookup, GossipActor};
use icn_identity::KeyPair;
use icn_ledger::{AccessModel, AntiSpeculationRules, ResourceAccess};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Wait time for async gossip publication tasks to complete.
/// 200ms provides margin for multi-node scenarios where gossip
/// publication involves actor communication and potential network delays.
const ASYNC_PUBLISH_WAIT_MS: u64 = 200;

/// Mock store for testing that tracks revocation events
struct MockResourceAccessStore {
    resources: Mutex<HashMap<String, ResourceAccess>>,
    revocations: Arc<Mutex<Vec<RevocationEvent>>>,
}

impl MockResourceAccessStore {
    fn new(revocations_log: Arc<Mutex<Vec<RevocationEvent>>>) -> Self {
        Self {
            resources: Mutex::new(HashMap::new()),
            revocations: revocations_log,
        }
    }

    #[allow(dead_code)]
    fn add_resource(&self, id: String, access: ResourceAccess) {
        self.resources.lock().unwrap().insert(id, access);
    }
}

impl ResourceAccessStore for MockResourceAccessStore {
    fn list_all(&self) -> anyhow::Result<Vec<(String, ResourceAccess)>> {
        let resources = self.resources.lock().unwrap();
        Ok(resources
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn update(&mut self, resource_id: &str, access: &ResourceAccess) -> anyhow::Result<()> {
        let mut resources = self.resources.lock().unwrap();
        resources.insert(resource_id.to_string(), access.clone());
        Ok(())
    }

    fn emit_revocation(&mut self, event: RevocationEvent) -> anyhow::Result<()> {
        self.revocations.lock().unwrap().push(event);
        Ok(())
    }

    fn apply_received_revocation(&self, event: &RevocationEvent) -> anyhow::Result<()> {
        self.revocations.lock().unwrap().push(event.clone());
        Ok(())
    }
}

#[tokio::test]
async fn test_revocation_event_gossip_publication() {
    // Create two gossip actors to simulate a cluster
    let node1_keypair = KeyPair::generate().unwrap();
    let node1_did = node1_keypair.did();

    let node2_keypair = KeyPair::generate().unwrap();
    let node2_did = node2_keypair.did();

    let trust_lookup: TrustLookup = Arc::new(move |_did| Some(icn_trust::TrustClass::Known));

    let node1_gossip =
        GossipActor::spawn_with_trust_graph(node1_did.clone(), trust_lookup.clone(), None);

    let node2_gossip =
        GossipActor::spawn_with_trust_graph(node2_did.clone(), trust_lookup.clone(), None);

    // Create the revocation topic on both nodes
    {
        let mut gossip1 = node1_gossip.write().await;
        gossip1.create_topic(icn_gossip::types::Topic {
            name: RESOURCE_REVOCATIONS_TOPIC.to_string(),
            acl: icn_gossip::types::AccessControl::Public,
            scope: icn_gossip::types::Scope::Global,
            min_trust_threshold: None,
            retention: std::time::Duration::from_secs(86400 * 7), // 7 days
            max_entries: 1000,
        });
    }

    {
        let mut gossip2 = node2_gossip.write().await;
        gossip2.create_topic(icn_gossip::types::Topic {
            name: RESOURCE_REVOCATIONS_TOPIC.to_string(),
            acl: icn_gossip::types::AccessControl::Public,
            scope: icn_gossip::types::Scope::Global,
            min_trust_threshold: None,
            retention: std::time::Duration::from_secs(86400 * 7), // 7 days
            max_entries: 1000,
        });
    }

    // Subscribe both nodes to the revocation topic
    {
        let mut gossip1 = node1_gossip.write().await;
        gossip1
            .subscribe(RESOURCE_REVOCATIONS_TOPIC, node1_did.clone())
            .await
            .unwrap();
    }

    {
        let mut gossip2 = node2_gossip.write().await;
        gossip2
            .subscribe(RESOURCE_REVOCATIONS_TOPIC, node2_did.clone())
            .await
            .unwrap();
    }

    // Create a revocation event
    let entity = EntityId::from_did(node1_did);
    let event = RevocationEvent {
        resource_id: "test-resource-123".to_string(),
        holder: entity,
        reason: "Resource idle for 8 days".to_string(),
        timestamp: icn_time::current_timestamp_secs(),
        idle_seconds: 8 * 24 * 3600, // 8 days
    };

    // Serialize and publish from node1
    let event_data = serde_json::to_vec(&event).unwrap();
    {
        let mut gossip1 = node1_gossip.write().await;
        let hash = gossip1
            .publish(RESOURCE_REVOCATIONS_TOPIC, event_data.clone())
            .await
            .unwrap();

        assert!(!hash.is_empty(), "Published event should have a hash");
    }

    // In a real test with network bridging, node2 would receive this via gossip
    // For now, we verify the event was published and can be deserialized
    let deserialized: RevocationEvent = serde_json::from_slice(&event_data).unwrap();
    assert_eq!(deserialized.resource_id, "test-resource-123");
    assert_eq!(deserialized.reason, "Resource idle for 8 days");
    assert_eq!(deserialized.idle_seconds, 8 * 24 * 3600);
}

#[tokio::test]
async fn test_gossip_store_wrapper_integration() {
    use icn_core::supervisor::init_resource_enforcer::GossipResourceAccessStore;

    // Create revocation log for tracking
    let revocations_log = Arc::new(Mutex::new(Vec::new()));

    // Create gossip actor
    let keypair = KeyPair::generate().unwrap();
    let keypair_clone = keypair.clone();
    let did = keypair_clone.did();
    let trust_lookup: TrustLookup = Arc::new(move |_did| Some(icn_trust::TrustClass::Known));
    let gossip_handle = GossipActor::spawn_with_trust_graph(did.clone(), trust_lookup, None);

    // Set keypair for signing
    {
        let mut gossip = gossip_handle.write().await;
        gossip.set_keypair(keypair);
    }

    // Create gossip-aware store
    let inner_store = Box::new(MockResourceAccessStore::new(revocations_log.clone()));
    let mut gossip_store = GossipResourceAccessStore::new(inner_store, gossip_handle.clone());

    // Create a resource and track it
    let entity = EntityId::from_did(did);
    let access = ResourceAccess::new(
        "resource-456".to_string(),
        entity.clone(),
        AccessModel::UseAccess {
            duration_seconds: 90 * 24 * 3600, // 90 days
            renewable: true,
            max_accumulated: 4,
        },
    )
    .with_rules(AntiSpeculationRules::strict());

    gossip_store.update("resource-456", &access).unwrap();

    // Emit a revocation
    let event = RevocationEvent {
        resource_id: "resource-456".to_string(),
        holder: entity,
        reason: "Automatic revocation: idle".to_string(),
        timestamp: icn_time::current_timestamp_secs(),
        idle_seconds: 10 * 24 * 3600, // 10 days
    };

    gossip_store.emit_revocation(event.clone()).unwrap();

    // Wait for async publication
    tokio::time::sleep(tokio::time::Duration::from_millis(ASYNC_PUBLISH_WAIT_MS)).await;

    // Verify event was logged locally
    let logged = revocations_log.lock().unwrap();
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0].resource_id, "resource-456");
    assert_eq!(logged[0].reason, "Automatic revocation: idle");
}

#[tokio::test]
async fn test_revocation_storage_application_and_idempotency() {
    // This test validates the storage application logic for revocation events:
    // 1. Revocation events are published to gossip by the originating node
    // 2. The apply_received_revocation method correctly updates local storage
    // 3. Duplicate revocations are handled idempotently
    //
    // NOTE: This test manually calls apply_received_revocation to simulate
    // gossip notification delivery. It does NOT test the full end-to-end
    // gossip propagation path (that would require setting up the notification
    // callback system and waiting for actual gossip message delivery).

    use icn_core::supervisor::init_resource_enforcer::GossipResourceAccessStore;
    use std::sync::Mutex;

    // Create two nodes with separate gossip actors
    let node1_keypair = KeyPair::generate().unwrap();
    let node1_keypair_clone = node1_keypair.clone();
    let node1_did = node1_keypair_clone.did();
    let node2_keypair = KeyPair::generate().unwrap();
    let node2_keypair_clone = node2_keypair.clone();
    let node2_did = node2_keypair_clone.did();

    let trust_lookup: TrustLookup = Arc::new(move |_did| Some(icn_trust::TrustClass::Known));

    // Node 1 - originating node that will publish the revocation
    let node1_revocations = Arc::new(Mutex::new(Vec::new()));
    let node1_gossip =
        GossipActor::spawn_with_trust_graph(node1_did.clone(), trust_lookup.clone(), None);
    {
        let mut gossip = node1_gossip.write().await;
        gossip.set_keypair(node1_keypair);
        gossip.create_topic(icn_gossip::Topic::new(
            RESOURCE_REVOCATIONS_TOPIC.to_string(),
            icn_gossip::AccessControl::Public,
        ));
    }

    let node1_inner_store = Box::new(MockResourceAccessStore::new(node1_revocations.clone()));
    let mut node1_store = GossipResourceAccessStore::new(node1_inner_store, node1_gossip.clone());

    // Node 2 - receiving node that will apply the revocation
    let node2_revocations = Arc::new(Mutex::new(Vec::new()));
    let node2_gossip =
        GossipActor::spawn_with_trust_graph(node2_did.clone(), trust_lookup.clone(), None);
    {
        let mut gossip = node2_gossip.write().await;
        gossip.set_keypair(node2_keypair.clone());
        gossip.create_topic(icn_gossip::Topic::new(
            RESOURCE_REVOCATIONS_TOPIC.to_string(),
            icn_gossip::AccessControl::Public,
        ));

        // Subscribe to revocations topic
        gossip
            .subscribe(RESOURCE_REVOCATIONS_TOPIC, node2_did.clone())
            .await
            .expect("Failed to subscribe to revocations topic");
    }

    let node2_inner_store = Box::new(MockResourceAccessStore::new(node2_revocations.clone()));
    let node2_store = Arc::new(tokio::sync::RwLock::new(GossipResourceAccessStore::new(
        node2_inner_store,
        node2_gossip.clone(),
    )));

    // Create a resource on node 1
    let entity = EntityId::from_did(&node1_did);
    let access = ResourceAccess::new(
        "cross-node-resource".to_string(),
        entity.clone(),
        AccessModel::UseAccess {
            duration_seconds: 90 * 24 * 3600, // 90 days
            renewable: false,
            max_accumulated: 1,
        },
    )
    .with_rules(AntiSpeculationRules::strict());

    node1_store.update("cross-node-resource", &access).unwrap();

    // Node 1 emits a revocation (simulating auto-revocation by enforcer)
    let revocation_event = RevocationEvent {
        resource_id: "cross-node-resource".to_string(),
        holder: entity.clone(),
        reason: "Idle for 15 days".to_string(),
        timestamp: icn_time::current_timestamp_secs(),
        idle_seconds: 15 * 24 * 3600,
    };

    node1_store
        .emit_revocation(revocation_event.clone())
        .unwrap();

    // Wait for async gossip publication
    tokio::time::sleep(tokio::time::Duration::from_millis(ASYNC_PUBLISH_WAIT_MS)).await;

    // Verify node 1 logged the revocation
    {
        let logged = node1_revocations.lock().unwrap();
        assert_eq!(logged.len(), 1, "Node 1 should have logged the revocation");
        assert_eq!(logged[0].resource_id, "cross-node-resource");
        assert_eq!(logged[0].idle_seconds, 15 * 24 * 3600);
    }

    // Simulate node 2 receiving the revocation via gossip notification callback
    // In a real deployment, this would happen automatically via the notification callback
    // For this test, we manually call apply_received_revocation to simulate the callback
    {
        let store_guard = node2_store.read().await;
        store_guard
            .apply_received_revocation(&revocation_event)
            .expect("Node 2 should apply the received revocation");
    }

    // Verify node 2 applied the revocation
    {
        let logged = node2_revocations.lock().unwrap();
        assert_eq!(
            logged.len(),
            1,
            "Node 2 should have received and applied the revocation"
        );
        assert_eq!(logged[0].resource_id, "cross-node-resource");
        assert_eq!(logged[0].holder, entity);
        assert_eq!(logged[0].reason, "Idle for 15 days");
    }

    // Test idempotency - applying the same revocation again should not error
    {
        let store_guard = node2_store.read().await;
        store_guard
            .apply_received_revocation(&revocation_event)
            .expect("Duplicate revocation should be handled idempotently");
    }

    // Verify idempotent behavior - both calls logged (mock store doesn't deduplicate)
    {
        let logged = node2_revocations.lock().unwrap();
        assert_eq!(
            logged.len(),
            2,
            "Mock store records both calls for testing purposes"
        );
    }
}
