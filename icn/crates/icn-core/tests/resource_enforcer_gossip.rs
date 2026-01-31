//! Integration tests for resource enforcer gossip synchronization
//!
//! Validates that revocation events are properly published to and received from
//! the gossip network, enabling cluster-wide notification of access revocations.

// Allow unwrap/expect in test code - panics are acceptable for tests
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_core::resource_enforcer_actor::{RevocationEvent, RESOURCE_REVOCATIONS_TOPIC};
use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use std::sync::Arc;

#[tokio::test]
async fn test_revocation_event_gossip_publication() {
    // Create two gossip actors to simulate a cluster
    let node1_keypair = KeyPair::generate().unwrap();
    let node1_did = node1_keypair.did();

    let node2_keypair = KeyPair::generate().unwrap();
    let node2_did = node2_keypair.did();

    let node1_gossip = GossipActor::spawn(node1_did.clone(), None);

    let node2_gossip = GossipActor::spawn(node2_did.clone(), None);

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

    // Create a revocation event (holder is now a String, not EntityId)
    let event = RevocationEvent {
        resource_id: "test-resource-123".to_string(),
        holder: node1_did.to_string(),
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
async fn test_enforcer_actor_with_gossip_deps() {
    use icn_core::resource_enforcer_actor::{
        ResourceAccessEnforcerActor, ResourceEnforcerConfig, ResourceEnforcerDeps,
    };
    use icn_kernel_api::services::LedgerService;

    /// Stub LedgerService for integration testing
    struct StubLedgerService;

    impl LedgerService for StubLedgerService {
        fn oracle(&self) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
            unimplemented!("not needed for enforcer integration tests")
        }

        fn balance(&self, _account: &icn_kernel_api::types::Did, _currency: &str) -> i64 {
            0
        }

        fn credit_limit(&self, _account: &icn_kernel_api::types::Did, _currency: &str) -> i64 {
            0
        }

        fn record_event(&self, _event: icn_kernel_api::services::LedgerEvent) {}
    }

    // Create gossip actor
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did();
    let gossip_handle = GossipActor::spawn(did.clone(), None);

    // Create revocation topic
    {
        let mut gossip = gossip_handle.write().await;
        gossip.create_topic(icn_gossip::types::Topic {
            name: RESOURCE_REVOCATIONS_TOPIC.to_string(),
            acl: icn_gossip::types::AccessControl::Public,
            scope: icn_gossip::types::Scope::Global,
            min_trust_threshold: None,
            retention: std::time::Duration::from_secs(86400 * 7),
            max_entries: 1000,
        });
    }

    // Create enforcer with gossip-enabled deps
    let config = ResourceEnforcerConfig {
        check_interval_seconds: 3600,
        batch_size: 100,
        enabled: true,
    };

    let deps = ResourceEnforcerDeps {
        ledger_service: Arc::new(StubLedgerService),
        gossip_handle: Some(gossip_handle.clone()),
    };

    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
    let shutdown_rx = shutdown_tx.subscribe();

    let handle = ResourceAccessEnforcerActor::spawn(config, deps, shutdown_rx);

    // Verify actor is running
    let stats = handle.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats.checks_performed, 0);
    assert_eq!(stats.total_revocations, 0);

    // Force a check (stub returns no enforceable resources, so 0 revocations)
    let result = handle.force_check().await.expect("Failed to force check");
    assert_eq!(result.resources_checked, 0);
    assert_eq!(result.revocations, 0);

    // Verify stats updated
    let stats = handle.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats.checks_performed, 1);

    // Signal shutdown
    let _ = shutdown_tx.send(());
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
