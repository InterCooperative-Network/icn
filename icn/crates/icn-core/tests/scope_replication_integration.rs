#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for scope-aware replication (Epic 5).
//!
//! Tests the interaction between:
//! - `ObjectReplication` (per-object config in kernel-api)
//! - `ReplicaMetadata.replication_config` (stored in icn-store)
//! - `ScopedReplicationAdjuster` (repair logic in icn-core)
//! - `ReplicationManager` (per-object target override)

use anyhow::Result;
use icn_core::replication::{
    AdjusterConfig, ReplicationConfig, ReplicationManager, ScopedReplicationAdjuster,
};
use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_kernel_api::authz::PolicyOracle;
use icn_kernel_api::scope::{CellId, MockCellService, ScopeLevel};
use icn_kernel_api::services::{CellService, TrustEvent, TrustService};
use icn_kernel_api::state::{ObjectReplication, ReplicationPolicy};
use icn_store::{ContentHash, ReplicaHealth, ReplicaMetadata, SledStore, Store};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

struct MockTrustService;

impl TrustService for MockTrustService {
    fn oracle(&self) -> Arc<dyn PolicyOracle> {
        Arc::new(icn_kernel_api::AllowAllOracle::default())
    }

    fn trust_score(&self, _actor: &icn_kernel_api::types::Did) -> f64 {
        0.5 // All peers are trusted
    }

    fn record_event(&self, _actor: &icn_kernel_api::types::Did, _event: TrustEvent) {}
}

fn cell_id() -> CellId {
    CellId::derive(b"org", "integration-cell", &[0u8; 32])
}

fn make_store() -> Arc<dyn Store> {
    Arc::new(SledStore::temporary().unwrap())
}

fn make_cell_service(members: &[&str]) -> Arc<dyn CellService> {
    let mut svc = MockCellService::new(Some(cell_id()));
    for m in members {
        svc = svc.with_member((*m).into());
    }
    Arc::new(svc)
}

fn scoped_config(scope: ScopeLevel, factor: u8, max: ScopeLevel) -> ObjectReplication {
    ObjectReplication::new(ReplicationPolicy::Scoped { scope, factor }, scope, max).unwrap()
}

fn test_hash(byte: u8) -> ContentHash {
    [byte; 32]
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

/// Store a blob with `Scoped { Cell, factor: 2 }` and verify the adjuster
/// identifies it needs 2 cell-local replicas.
#[tokio::test]
async fn test_store_blob_cell_scope_replication() {
    let store = make_store();
    let svc = make_cell_service(&["did:icn:alice", "did:icn:bob"]);

    let hash = test_hash(0x01);
    let config = scoped_config(ScopeLevel::Cell, 2, ScopeLevel::Org);
    let meta = ReplicaMetadata::new(hash).with_replication_config(config);
    store.put_replica_metadata(&meta).unwrap();

    let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);

    let actions = adjuster.on_membership_change(ScopeLevel::Cell).unwrap();
    // Under-replicated by 2 (no replicas yet, target=2)
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        icn_core::replication::RepairAction::AddReplica { target_peers, .. } => {
            assert_eq!(target_peers.len(), 2, "Should target alice and bob");
        }
        _ => panic!("Expected AddReplica"),
    }
}

/// Verify that `max_scope` is enforced: a blob with `max_scope: Cell` should not
/// count org-peer replicas toward its target, and repair actions should only
/// target cell members.
#[tokio::test]
async fn test_max_scope_enforcement() {
    let store = make_store();
    // alice is a cell member; bob is an org peer (outside Cell scope)
    let mut svc = MockCellService::new(Some(cell_id()));
    svc = svc.with_member("did:icn:alice".into());
    svc = svc.with_org_peer("did:icn:bob".into());
    let svc = Arc::new(svc) as Arc<dyn CellService>;

    let hash = test_hash(0x02);
    let config = scoped_config(ScopeLevel::Cell, 2, ScopeLevel::Cell);

    // bob has a healthy replica, but it's outside max_scope: Cell
    let mut meta = ReplicaMetadata::new(hash).with_replication_config(config);
    meta.add_replica("did:icn:bob".to_string(), ReplicaHealth::Healthy);
    store.put_replica_metadata(&meta).unwrap();

    let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store.clone(), svc);

    // Health check: bob's replica is out of scope, so the object is still
    // under-replicated (0 in-scope healthy, target = 2)
    let health = adjuster.evaluate_scope(ScopeLevel::Cell).unwrap();
    assert_eq!(health.under_replicated, 1, "bob's replica is out of scope");

    // Repair actions should target alice (the only in-scope candidate)
    let actions = adjuster.on_membership_change(ScopeLevel::Cell).unwrap();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        icn_core::replication::RepairAction::AddReplica { target_peers, .. } => {
            assert_eq!(target_peers.len(), 1);
            assert_eq!(target_peers[0], "did:icn:alice");
        }
        _ => panic!("Expected AddReplica"),
    }
}

/// Simulate a node departure: one cell member leaves, adjuster generates AddReplica.
#[tokio::test]
async fn test_node_departure_triggers_repair() {
    let store = make_store();
    // Initially alice and bob are members and hold replicas
    let svc = make_cell_service(&["did:icn:alice", "did:icn:carol"]);

    let hash = test_hash(0x03);
    let config = scoped_config(ScopeLevel::Cell, 2, ScopeLevel::Org);
    let mut meta = ReplicaMetadata::new(hash).with_replication_config(config);
    // alice is healthy, bob was a member but left (simulated by not being in cell_service)
    meta.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
    // bob's replica is stale/unreachable since they left
    meta.add_replica("did:icn:bob".to_string(), ReplicaHealth::Unreachable);
    store.put_replica_metadata(&meta).unwrap();

    let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);
    let actions = adjuster.on_membership_change(ScopeLevel::Cell).unwrap();

    // Should generate AddReplica for carol (the remaining cell member without a replica)
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        icn_core::replication::RepairAction::AddReplica {
            content_hash,
            target_peers,
        } => {
            assert_eq!(content_hash, &hash);
            assert!(target_peers.contains(&"did:icn:carol".to_string()));
        }
        _ => panic!("Expected AddReplica"),
    }
}

/// Verify that `LocalOnly` objects are not affected by scope logic.
#[tokio::test]
async fn test_existing_local_only_unchanged() {
    let store = make_store();
    let svc = make_cell_service(&["did:icn:alice"]);

    // Store a non-scoped object (no replication_config)
    let hash = test_hash(0x04);
    let meta = ReplicaMetadata::new(hash);
    store.put_replica_metadata(&meta).unwrap();

    let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);

    // Cell-scoped scan should not include this object
    let actions = adjuster.on_membership_change(ScopeLevel::Cell).unwrap();
    assert!(
        actions.is_empty(),
        "Non-scoped objects should not produce repair actions"
    );
}

/// Verify that `ClusterStrong` objects (via ObjectReplication but not Scoped) are
/// not returned by the cell-scope adjuster scan.
#[tokio::test]
async fn test_existing_cluster_strong_unchanged() {
    let store = make_store();
    let svc = make_cell_service(&["did:icn:alice"]);

    let hash = test_hash(0x05);
    let config = ObjectReplication::new(
        ReplicationPolicy::ClusterStrong,
        ScopeLevel::Cell,
        ScopeLevel::Org,
    )
    .unwrap();
    let meta = ReplicaMetadata::new(hash).with_replication_config(config);
    store.put_replica_metadata(&meta).unwrap();

    let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);
    let actions = adjuster.on_membership_change(ScopeLevel::Cell).unwrap();
    assert!(
        actions.is_empty(),
        "ClusterStrong should not appear in Scoped scan"
    );
}

/// Verify `evaluate_scope` returns correct health stats.
///
/// Uses a cell service with alice, bob as cell members and carol as an org peer.
/// The objects have max_scope: Org, so carol's replicas count as in-scope.
#[tokio::test]
async fn test_scope_health_report() {
    let store = make_store();
    // alice, bob are cell members; carol is an org peer (within Org max_scope)
    let mut svc = MockCellService::new(Some(cell_id()));
    svc = svc.with_member("did:icn:alice".into());
    svc = svc.with_member("did:icn:bob".into());
    svc = svc.with_org_peer("did:icn:carol".into());
    let svc = Arc::new(svc) as Arc<dyn CellService>;

    // Object 1: healthy (2/2 in-scope)
    let hash1 = test_hash(0x10);
    let config1 = scoped_config(ScopeLevel::Cell, 2, ScopeLevel::Org);
    let mut m1 = ReplicaMetadata::new(hash1).with_replication_config(config1);
    m1.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
    m1.add_replica("did:icn:bob".to_string(), ReplicaHealth::Healthy);
    store.put_replica_metadata(&m1).unwrap();

    // Object 2: under-replicated (1/2 in-scope)
    let hash2 = test_hash(0x11);
    let config2 = scoped_config(ScopeLevel::Cell, 2, ScopeLevel::Org);
    let mut m2 = ReplicaMetadata::new(hash2).with_replication_config(config2);
    m2.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
    store.put_replica_metadata(&m2).unwrap();

    // Object 3: over-replicated (3/2 in-scope — carol is an org peer, within Org max_scope)
    let hash3 = test_hash(0x12);
    let config3 = scoped_config(ScopeLevel::Cell, 2, ScopeLevel::Org);
    let mut m3 = ReplicaMetadata::new(hash3).with_replication_config(config3);
    m3.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
    m3.add_replica("did:icn:bob".to_string(), ReplicaHealth::Healthy);
    m3.add_replica("did:icn:carol".to_string(), ReplicaHealth::Healthy);
    store.put_replica_metadata(&m3).unwrap();

    let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);
    let health = adjuster.evaluate_scope(ScopeLevel::Cell).unwrap();

    assert_eq!(health.scope, ScopeLevel::Cell);
    assert_eq!(health.total_objects, 3);
    assert_eq!(health.healthy, 1);
    assert_eq!(health.under_replicated, 1);
    assert_eq!(health.over_replicated, 1);
}

/// Verify the manager uses per-object Scoped factor to decide replication health.
#[tokio::test]
async fn test_manager_scope_aware_health_check() -> Result<()> {
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();
    let store = make_store();
    let trust_service = Arc::new(MockTrustService) as Arc<dyn TrustService>;
    let gossip = GossipActor::spawn(did.clone(), None);

    let config = ReplicationConfig {
        target_replicas: 5, // global target is high
        ..Default::default()
    };

    let mut manager = ReplicationManager::new(did, config, store.clone(), trust_service, gossip);

    // Object with Scoped factor=1 should be satisfied with 1 replica
    let hash = test_hash(0x20);
    let obj_config = scoped_config(ScopeLevel::Cell, 1, ScopeLevel::Org);
    let mut meta = ReplicaMetadata::new(hash).with_replication_config(obj_config);
    meta.add_replica("did:icn:peer1".to_string(), ReplicaHealth::Healthy);
    store.put_replica_metadata(&meta)?;

    // Should be healthy despite global target=5, because per-object target=1
    // We test the internal method indirectly through trigger_health_check
    let result = manager.trigger_health_check().await;
    assert!(result.is_ok());

    Ok(())
}
