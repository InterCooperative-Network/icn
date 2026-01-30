//! ScopedReplicationAdjuster - Reacts to membership changes and rebalances
//! scope-aware replicas.
//!
//! When a cell or org membership changes, this module scans all `Scoped`
//! objects at the affected scope and generates repair actions for any that
//! are under- or over-replicated.

use anyhow::Result;
use std::sync::Arc;

use icn_kernel_api::scope::ScopeLevel;
use icn_kernel_api::services::CellService;
use icn_store::{ContentHash, Store};

/// Configuration for the scoped replication adjuster.
///
/// The `rebalance_interval_secs` and `max_concurrent_repairs` fields are used
/// when the adjuster is spawned as a background task (see Phase 19/20 roadmap).
/// Currently, `on_membership_change()` is called reactively.
// TODO(#924): Wire periodic rebalance loop using rebalance_interval_secs.
// TODO(#924): Enforce max_concurrent_repairs when executing repair actions.
#[derive(Clone, Debug)]
pub struct AdjusterConfig {
    /// How often to run periodic rebalance checks (seconds).
    /// Default: 300 (5 minutes).
    pub rebalance_interval_secs: u64,

    /// Fractional deviation from the target factor that triggers rebalance.
    /// Default: 0.2 (20% deviation).
    pub rebalance_threshold: f64,

    /// Maximum number of concurrent repair operations.
    /// Default: 10.
    pub max_concurrent_repairs: usize,
}

impl Default for AdjusterConfig {
    fn default() -> Self {
        Self {
            rebalance_interval_secs: 300,
            rebalance_threshold: 0.2,
            max_concurrent_repairs: 10,
        }
    }
}

/// A concrete repair action to restore replication invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairAction {
    /// Add replicas on the given target peers.
    AddReplica {
        content_hash: ContentHash,
        target_peers: Vec<String>,
    },
    /// Remove an excess replica from a peer.
    RemoveReplica {
        content_hash: ContentHash,
        excess_peer: String,
    },
}

/// Summary of replication health for a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeHealth {
    pub scope: ScopeLevel,
    pub total_objects: usize,
    pub under_replicated: usize,
    pub over_replicated: usize,
    pub healthy: usize,
}

/// Adjusts replication in response to membership changes and periodic scans.
///
/// Currently operates reactively via `on_membership_change()`. A periodic
/// background loop using `config.rebalance_interval_secs` is planned for
/// Phase 19/20 (see TODO in `AdjusterConfig`).
pub struct ScopedReplicationAdjuster {
    config: AdjusterConfig,
    store: Arc<dyn Store>,
    cell_service: Arc<dyn CellService>,
}

impl ScopedReplicationAdjuster {
    /// Create a new adjuster.
    pub fn new(
        config: AdjusterConfig,
        store: Arc<dyn Store>,
        cell_service: Arc<dyn CellService>,
    ) -> Self {
        Self {
            config,
            store,
            cell_service,
        }
    }

    /// React to a membership change at the given scope.
    ///
    /// Scans all `Scoped` objects matching `scope` and returns repair actions
    /// for any that are under- or over-replicated relative to their target factor.
    /// Actions are capped at `config.max_concurrent_repairs`.
    pub fn on_membership_change(&self, scope: ScopeLevel) -> Result<Vec<RepairAction>> {
        let mut actions = self.repair_actions(scope)?;
        actions.truncate(self.config.max_concurrent_repairs);
        Ok(actions)
    }

    /// Evaluate the replication health of all `Scoped` objects at the given scope.
    pub fn evaluate_scope(&self, scope: ScopeLevel) -> Result<ScopeHealth> {
        let hashes = self.store.list_scoped_replica_hashes(scope)?;
        let mut under = 0;
        let mut over = 0;
        let mut healthy = 0;

        for hash in &hashes {
            if let Some(metadata) = self.store.get_replica_metadata(hash)? {
                let target = metadata.effective_target_replicas(3);
                let max_scope = metadata.max_scope();
                let current = self.in_scope_healthy_count(&metadata, max_scope);

                if current < target {
                    under += 1;
                } else if current > target {
                    over += 1;
                } else {
                    healthy += 1;
                }
            }
        }

        Ok(ScopeHealth {
            scope,
            total_objects: hashes.len(),
            under_replicated: under,
            over_replicated: over,
            healthy,
        })
    }

    /// Generate concrete repair actions for `Scoped` objects at the given scope.
    pub fn repair_actions(&self, scope: ScopeLevel) -> Result<Vec<RepairAction>> {
        let hashes = self.store.list_scoped_replica_hashes(scope)?;
        let mut actions = Vec::new();

        for hash in hashes {
            if let Some(metadata) = self.store.get_replica_metadata(&hash)? {
                let target = metadata.effective_target_replicas(3);
                let max_scope = metadata.max_scope();
                let current_healthy = self.in_scope_healthy_count(&metadata, max_scope);

                if current_healthy < target {
                    // Under-replicated: find peers in scope that don't already hold replicas
                    let needed = target - current_healthy;
                    let existing: std::collections::HashSet<String> = metadata
                        .replicas
                        .iter()
                        .map(|r| r.peer_did.clone())
                        .collect();

                    let candidates = self.peers_in_scope(scope);
                    let target_peers: Vec<String> = candidates
                        .into_iter()
                        .filter(|p| !existing.contains(p))
                        .take(needed)
                        .collect();

                    if !target_peers.is_empty() {
                        actions.push(RepairAction::AddReplica {
                            content_hash: hash,
                            target_peers,
                        });
                    }
                } else if current_healthy > target {
                    // Over-replicated: pick excess replicas to remove.
                    // Removal preference: widest-scope replicas first (e.g., Org before Cell).
                    // This preserves locality — data stays closer to the cell that owns it.
                    let excess = current_healthy - target;
                    let mut in_scope_healthy: Vec<(String, ScopeLevel)> = metadata
                        .replicas
                        .iter()
                        .filter(|r| {
                            r.health == icn_store::ReplicaHealth::Healthy
                                && self.is_replica_in_scope(&r.peer_did, max_scope)
                        })
                        .map(|r| {
                            let did = icn_kernel_api::types::Did::from(r.peer_did.clone());
                            let s = self.cell_service.peer_scope(&did);
                            (r.peer_did.clone(), s)
                        })
                        .collect();
                    // Sort widest scope first (highest ordinal), so we prefer to remove
                    // the least-local replicas.
                    in_scope_healthy.sort_by(|a, b| b.1.cmp(&a.1));
                    for (peer, _) in in_scope_healthy.iter().take(excess) {
                        actions.push(RepairAction::RemoveReplica {
                            content_hash: hash,
                            excess_peer: peer.clone(),
                        });
                    }
                }
            }
        }

        Ok(actions)
    }

    /// Count healthy replicas that are within the object's max_scope.
    ///
    /// If no max_scope is set, all healthy replicas count.
    fn in_scope_healthy_count(
        &self,
        metadata: &icn_store::ReplicaMetadata,
        max_scope: Option<ScopeLevel>,
    ) -> usize {
        metadata
            .replicas
            .iter()
            .filter(|r| {
                r.health == icn_store::ReplicaHealth::Healthy
                    && self.is_replica_in_scope(&r.peer_did, max_scope)
            })
            .count()
    }

    /// Check if a replica peer is within the given max_scope.
    fn is_replica_in_scope(&self, peer_did: &str, max_scope: Option<ScopeLevel>) -> bool {
        match max_scope {
            Some(max) => {
                let did = icn_kernel_api::types::Did::from(peer_did.to_string());
                let peer_scope = self.cell_service.peer_scope(&did);
                max.includes(peer_scope)
            }
            None => true,
        }
    }

    /// Return peer DIDs that are within the given scope relative to the local node.
    ///
    /// For `Cell` scope, returns cell members. For `Org` scope and wider, also
    /// includes org peers known to the `CellService`.
    ///
    /// Note: For scopes wider than `Org`, a complete peer set would require
    /// gossip-based discovery (handled by `ReplicationManager`). This method
    /// provides the best-effort set from the `CellService` for adjuster use.
    // TODO(#924): For Federation+ scopes, integrate with gossip peer discovery.
    fn peers_in_scope(&self, scope: ScopeLevel) -> Vec<String> {
        if let Some(cell_id) = self.cell_service.local_cell() {
            let mut peers: Vec<String> = self
                .cell_service
                .cell_members(&cell_id)
                .into_iter()
                .map(|d| d.to_string())
                .collect();

            if scope >= ScopeLevel::Org {
                // Include org peers (same org, possibly different cell).
                // CellService.is_org_peer() can check individual peers, but
                // we don't have a list_org_peers() method yet. We rely on
                // cell_members as the known subset. The ReplicationManager
                // handles the full gossip-based peer set for production use.
            }

            peers.sort();
            peers.dedup();
            peers
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::scope::{CellId, MockCellService};
    use icn_kernel_api::state::{ObjectReplication, ReplicationPolicy};
    use icn_store::{ReplicaHealth, ReplicaMetadata, SledStore};

    fn cell_id() -> CellId {
        CellId::derive(b"org", "test-cell", &[0u8; 32])
    }

    fn make_store_and_service(members: Vec<&str>) -> (Arc<dyn Store>, Arc<dyn CellService>) {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let mut svc = MockCellService::new(Some(cell_id()));
        for m in members {
            svc = svc.with_member(m.into());
        }
        (store, Arc::new(svc) as Arc<dyn CellService>)
    }

    fn scoped_config(scope: ScopeLevel, factor: u8) -> ObjectReplication {
        ObjectReplication::new(
            ReplicationPolicy::Scoped { scope, factor },
            scope,
            scope.widen().unwrap_or(scope),
        )
        .unwrap()
    }

    #[test]
    fn test_on_membership_change_detects_under_replication() {
        let (store, svc) = make_store_and_service(vec!["did:icn:alice", "did:icn:bob"]);

        // Store a Cell-scoped object with factor=2 but only 1 healthy replica
        let hash = [0xAAu8; 32];
        let mut meta =
            ReplicaMetadata::new(hash).with_replication_config(scoped_config(ScopeLevel::Cell, 2));
        meta.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
        store.put_replica_metadata(&meta).unwrap();

        let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);
        let actions = adjuster.on_membership_change(ScopeLevel::Cell).unwrap();

        // Should generate an AddReplica action for bob
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            RepairAction::AddReplica {
                content_hash,
                target_peers,
            } => {
                assert_eq!(content_hash, &hash);
                assert_eq!(target_peers, &["did:icn:bob".to_string()]);
            }
            _ => panic!("Expected AddReplica"),
        }
    }

    #[test]
    fn test_on_membership_change_no_unnecessary_action() {
        let (store, svc) = make_store_and_service(vec!["did:icn:alice", "did:icn:bob"]);

        // Store a Cell-scoped object with factor=2 and 2 healthy replicas
        let hash = [0xBBu8; 32];
        let mut meta =
            ReplicaMetadata::new(hash).with_replication_config(scoped_config(ScopeLevel::Cell, 2));
        meta.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
        meta.add_replica("did:icn:bob".to_string(), ReplicaHealth::Healthy);
        store.put_replica_metadata(&meta).unwrap();

        let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);
        let actions = adjuster.on_membership_change(ScopeLevel::Cell).unwrap();

        assert!(
            actions.is_empty(),
            "No actions needed when properly replicated"
        );
    }

    #[test]
    fn test_evaluate_scope_cell() {
        let (store, svc) = make_store_and_service(vec!["did:icn:alice", "did:icn:bob"]);

        // Object 1: under-replicated (1 of 2)
        let hash1 = [0x01u8; 32];
        let mut m1 =
            ReplicaMetadata::new(hash1).with_replication_config(scoped_config(ScopeLevel::Cell, 2));
        m1.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
        store.put_replica_metadata(&m1).unwrap();

        // Object 2: healthy (2 of 2)
        let hash2 = [0x02u8; 32];
        let mut m2 =
            ReplicaMetadata::new(hash2).with_replication_config(scoped_config(ScopeLevel::Cell, 2));
        m2.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
        m2.add_replica("did:icn:bob".to_string(), ReplicaHealth::Healthy);
        store.put_replica_metadata(&m2).unwrap();

        let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);
        let health = adjuster.evaluate_scope(ScopeLevel::Cell).unwrap();

        assert_eq!(health.total_objects, 2);
        assert_eq!(health.under_replicated, 1);
        assert_eq!(health.healthy, 1);
        assert_eq!(health.over_replicated, 0);
    }

    #[test]
    fn test_repair_actions_add_replica() {
        let (store, svc) =
            make_store_and_service(vec!["did:icn:alice", "did:icn:bob", "did:icn:carol"]);

        let hash = [0xCCu8; 32];
        let mut meta =
            ReplicaMetadata::new(hash).with_replication_config(scoped_config(ScopeLevel::Cell, 3));
        meta.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
        store.put_replica_metadata(&meta).unwrap();

        let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);
        let actions = adjuster.repair_actions(ScopeLevel::Cell).unwrap();

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            RepairAction::AddReplica { target_peers, .. } => {
                assert_eq!(target_peers.len(), 2); // bob and carol
            }
            _ => panic!("Expected AddReplica"),
        }
    }

    #[test]
    fn test_repair_actions_remove_replica() {
        let (store, svc) =
            make_store_and_service(vec!["did:icn:alice", "did:icn:bob", "did:icn:carol"]);

        // Over-replicated: factor=1 but 3 healthy replicas
        let hash = [0xDDu8; 32];
        let mut meta =
            ReplicaMetadata::new(hash).with_replication_config(scoped_config(ScopeLevel::Cell, 1));
        meta.add_replica("did:icn:alice".to_string(), ReplicaHealth::Healthy);
        meta.add_replica("did:icn:bob".to_string(), ReplicaHealth::Healthy);
        meta.add_replica("did:icn:carol".to_string(), ReplicaHealth::Healthy);
        store.put_replica_metadata(&meta).unwrap();

        let adjuster = ScopedReplicationAdjuster::new(AdjusterConfig::default(), store, svc);
        let actions = adjuster.repair_actions(ScopeLevel::Cell).unwrap();

        // Should remove 2 excess replicas
        assert_eq!(actions.len(), 2);
        for action in &actions {
            assert!(matches!(action, RepairAction::RemoveReplica { .. }));
        }
    }
}
