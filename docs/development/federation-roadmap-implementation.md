# ICN Federation Roadmap - Implementation Guide

**Status**: In Progress (Phase 1A complete)
**Started**: 2025-01-12
**Context**: Systematic implementation of all 3 major federation features

---

## Overview

This document tracks the implementation of three major feature sets to evolve ICN from proof-of-concept toward a production-capable federation system:

1. **Topology + NeighborSets** - Regional/cluster-based networking
2. **TrustPolicy + Resource Limits** - Centralized security enforcement
3. **Paginated RPC + Receipts** - Production API hygiene

Each feature builds on the previous, so they must be implemented sequentially.

---

## Phase 1: Topology + NeighborSets

### Phase 1A: Config Foundation ✅ COMPLETE

**Commit**: `fb8f5d4` - "feat: Add topology config foundation for scale-aware networking"

**Added**:
- `NodeRole` enum (Edge, Rendezvous, Archive)
- `TopologyConfig` struct with region/cluster/role
- `NeighborLimitsConfig` for per-set connection caps
- `FanoutConfig` for scope-aware gossip

**Files Modified**:
- `crates/icn-core/src/config.rs` (+150 lines)

**Tests**: Config serialization tests pass

---

### Phase 1B: NeighborSets Data Structure (NEXT)

**Goal**: Maintain categorized neighbor sets in NetworkActor

**Implementation**:

#### 1. Create `icn-net/src/topology.rs`

```rust
//! Topology-aware neighbor management

use icn_identity::Did;
use std::collections::BTreeSet;
use std::time::Instant;

/// Categorized neighbor sets for topology-aware routing
#[derive(Debug, Default)]
pub struct NeighborSets {
    /// Local cluster neighbors (same region + cluster_id)
    pub local_cluster: BTreeSet<PeerId>,

    /// Regional neighbors (same region, different cluster)
    pub regional: BTreeSet<PeerId>,

    /// Backbone neighbors (different region, high trust)
    pub backbone: BTreeSet<PeerId>,

    /// Trusted neighbors (cross-region high-trust)
    pub trusted: BTreeSet<PeerId>,

    /// Metadata per peer
    metadata: HashMap<PeerId, PeerMetadata>,
}

#[derive(Debug, Clone)]
pub struct PeerId(pub Did);

#[derive(Debug)]
struct PeerMetadata {
    topology_info: TopologyInfo,
    rtt_ms: Option<u64>,
    trust_score: f32,
    connected_at: Instant,
}

#[derive(Debug, Clone)]
pub struct TopologyInfo {
    pub region: String,
    pub cluster_id: String,
    pub role: NodeRole,
}

impl NeighborSets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a neighbor in the appropriate set
    pub fn add_neighbor(&mut self, peer: PeerId, info: PeerMetadata, limits: &NeighborLimitsConfig) {
        // 1. Remove from old sets if already present
        self.remove_neighbor(&peer);

        // 2. Determine placement based on topology_info + trust
        let target_set = self.determine_set(&info, &peer);

        // 3. Enforce limit with LRU eviction (score-based)
        self.enforce_limit(target_set, limits);

        // 4. Insert into appropriate set
        match target_set {
            NeighborSet::LocalCluster => { self.local_cluster.insert(peer.clone()); }
            NeighborSet::Regional => { self.regional.insert(peer.clone()); }
            NeighborSet::Backbone => { self.backbone.insert(peer.clone()); }
            NeighborSet::Trusted => { self.trusted.insert(peer.clone()); }
        }

        // 5. Store metadata
        self.metadata.insert(peer, info);
    }

    /// Remove neighbor from all sets
    pub fn remove_neighbor(&mut self, peer: &PeerId) {
        self.local_cluster.remove(peer);
        self.regional.remove(peer);
        self.backbone.remove(peer);
        self.trusted.remove(peer);
        self.metadata.remove(peer);
    }

    /// Sample peers from a specific scope
    pub fn sample(&self, scope: Scope, count: usize) -> Vec<PeerId> {
        let set = match scope {
            Scope::LocalCluster => &self.local_cluster,
            Scope::Regional => &self.regional,
            Scope::Global => &self.backbone, // Global uses backbone + trusted
        };

        // Random sampling without replacement
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        let peers: Vec<_> = set.iter().cloned().collect();
        peers.choose_multiple(&mut rng, count).cloned().collect()
    }

    /// Get metrics for observability
    pub fn metrics(&self) -> NeighborMetrics {
        NeighborMetrics {
            local_cluster_count: self.local_cluster.len(),
            regional_count: self.regional.len(),
            backbone_count: self.backbone.len(),
            trusted_count: self.trusted.len(),
        }
    }

    fn determine_set(&self, info: &PeerMetadata, peer: &PeerId) -> NeighborSet {
        // Priority: Trust > Region > Cluster
        if info.trust_score >= 0.7 {
            return NeighborSet::Trusted;
        }

        // Compare with own topology (would need to pass this in)
        // For now, simplified logic:
        if info.topology_info.cluster_id == "local" {
            NeighborSet::LocalCluster
        } else if info.topology_info.region == "same" {
            NeighborSet::Regional
        } else {
            NeighborSet::Backbone
        }
    }

    fn enforce_limit(&mut self, target: NeighborSet, limits: &NeighborLimitsConfig) {
        let (set, max) = match target {
            NeighborSet::LocalCluster => (&mut self.local_cluster, limits.max_local_cluster),
            NeighborSet::Regional => (&mut self.regional, limits.max_regional),
            NeighborSet::Backbone => (&mut self.backbone, limits.max_backbone),
            NeighborSet::Trusted => (&mut self.trusted, limits.max_trusted),
        };

        while set.len() >= max {
            // LRU eviction: remove lowest-score peer
            let to_remove = self.find_lowest_score_peer(set);
            if let Some(peer) = to_remove {
                set.remove(&peer);
                self.metadata.remove(&peer);
            } else {
                break;
            }
        }
    }

    fn find_lowest_score_peer(&self, set: &BTreeSet<PeerId>) -> Option<PeerId> {
        set.iter()
            .min_by(|a, b| {
                let score_a = self.metadata.get(a).map(|m| m.trust_score).unwrap_or(0.0);
                let score_b = self.metadata.get(b).map(|m| m.trust_score).unwrap_or(0.0);
                score_a.partial_cmp(&score_b).unwrap()
            })
            .cloned()
    }
}

#[derive(Debug, Clone, Copy)]
enum NeighborSet {
    LocalCluster,
    Regional,
    Backbone,
    Trusted,
}

pub struct NeighborMetrics {
    pub local_cluster_count: usize,
    pub regional_count: usize,
    pub backbone_count: usize,
    pub trusted_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum Scope {
    LocalCluster,
    Regional,
    Global,
}
```

**Dependencies to add to `icn-net/Cargo.toml`**:
```toml
rand = "0.8"
```

#### 2. Wire into NetworkActor

**File**: `crates/icn-net/src/actor.rs`

Add field:
```rust
pub struct NetworkActor {
    // ... existing fields
    neighbor_sets: Arc<RwLock<NeighborSets>>,
    own_topology: TopologyInfo,
}
```

On connection establish:
```rust
async fn handle_new_connection(&self, peer_did: Did, remote_topology: TopologyInfo) {
    let peer_id = PeerId(peer_did);
    let trust_score = self.get_trust_score(&peer_id).await;
    let rtt = self.measure_rtt(&peer_id).await;

    let metadata = PeerMetadata {
        topology_info: remote_topology,
        rtt_ms: Some(rtt),
        trust_score,
        connected_at: Instant::now(),
    };

    let mut sets = self.neighbor_sets.write().await;
    sets.add_neighbor(peer_id, metadata, &self.config.topology.neighbor_limits);
}
```

**Handshake Changes**: Exchange `TopologyInfo` during TLS handshake via custom extension or application-level message.

#### 3. Tests

**File**: `crates/icn-net/src/topology.rs` (add `#[cfg(test)]` module)

- `test_neighbor_placement()` - Verify set assignment logic
- `test_lru_eviction()` - Verify limit enforcement
- `test_sampling()` - Verify random sampling
- `test_metrics()` - Verify counter accuracy

---

### Phase 1C: Scope-Aware Gossip

**Goal**: Replace broadcast-to-all with scope-aware fanout

#### 1. Add Scope to GossipMessage

**File**: `crates/icn-gossip/src/protocol.rs`

Add scope header:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    pub scope: Scope, // NEW
    pub payload: GossipPayload,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Scope {
    LocalCluster,
    Regional,
    Global,
}
```

#### 2. Scope-Aware Fanout

**File**: `crates/icn-gossip/src/gossip.rs`

Replace broadcast loops:
```rust
pub async fn announce(&mut self, topic: &str, hash: ContentHash) -> Result<()> {
    let scope = self.determine_scope(topic);
    let fanout = self.config.fanout_for_scope(scope);

    // Sample appropriate neighbor set via NetworkActor
    let targets = self.network_handle.sample_neighbors(scope, fanout).await?;

    for target in targets {
        let msg = GossipMessage {
            scope,
            payload: GossipPayload::Announce { topic, hash, ... },
        };
        self.send_to_peer(target, msg).await?;
    }

    Ok(())
}

fn determine_scope(&self, topic: &str) -> Scope {
    // Check topic metadata
    self.topics.get(topic)
        .map(|meta| meta.scope)
        .unwrap_or(Scope::Global)
}
```

#### 3. Topic Scope Configuration

**File**: `crates/icn-gossip/src/topic.rs`

Add scope to TopicMetadata:
```rust
pub struct TopicMeta {
    pub id: String,
    pub acl: AccessControl,
    pub scope: Scope, // NEW
    pub max_entries: usize,
}

impl Topic {
    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.meta.scope = scope;
        self
    }
}
```

**Default scopes**:
- `contracts:deploy` → `Scope::Regional`
- `ledger:sync` → `Scope::LocalCluster`
- System topics → `Scope::Global`

---

### Phase 1D: Metrics

**File**: `crates/icn-obs/src/metrics/topology.rs` (NEW)

```rust
use prometheus::{IntGaugeVec, HistogramVec};

lazy_static! {
    pub static ref NEIGHBORS_BY_SET: IntGaugeVec = register_int_gauge_vec!(
        "icn_neighbors_total",
        "Number of neighbors per set",
        &["set"]
    ).unwrap();

    pub static ref FANOUT_BY_SCOPE: HistogramVec = register_histogram_vec!(
        "icn_gossip_fanout",
        "Gossip fanout count per scope",
        &["scope"]
    ).unwrap();
}

pub fn update_neighbor_metrics(metrics: &NeighborMetrics) {
    NEIGHBORS_BY_SET.with_label_values(&["local_cluster"]).set(metrics.local_cluster_count as i64);
    NEIGHBORS_BY_SET.with_label_values(&["regional"]).set(metrics.regional_count as i64);
    NEIGHBORS_BY_SET.with_label_values(&["backbone"]).set(metrics.backbone_count as i64);
    NEIGHBORS_BY_SET.with_label_values(&["trusted"]).set(metrics.trusted_count as i64);
}
```

Call from supervisor periodic task (every 10s).

---

### Phase 1E: Integration Tests

**File**: `crates/icn-core/tests/topology_integration.rs` (NEW)

```rust
#[tokio::test]
async fn test_neighbor_set_placement() {
    // Create 3 nodes in different regions
    let node_na = create_node_with_topology("na-east", "coop-1", NodeRole::Edge).await;
    let node_eu = create_node_with_topology("eu-west", "coop-1", NodeRole::Edge).await;
    let node_na2 = create_node_with_topology("na-east", "coop-2", NodeRole::Edge).await;

    // Connect all
    connect_bidirectional(&node_na, &node_eu).await;
    connect_bidirectional(&node_na, &node_na2).await;

    // Verify node_na neighbor sets:
    let sets = node_na.get_neighbor_sets().await;
    assert!(sets.local_cluster.contains(&node_na2.peer_id())); // Same region+cluster
    assert!(sets.regional.is_empty()); // No regional peers yet
    assert!(sets.backbone.contains(&node_eu.peer_id())); // Different region
}

#[tokio::test]
async fn test_scope_aware_fanout() {
    // Create 10 local + 5 regional nodes
    let local_nodes = create_local_cluster(10).await;
    let regional_nodes = create_regional_cluster(5).await;

    let source = &local_nodes[0];

    // Publish to local-scoped topic
    let topic = Topic::new("test:local").with_scope(Scope::LocalCluster);
    source.create_topic(topic).await;
    source.publish("test:local", b"data").await;

    // Wait for propagation
    sleep(Duration::from_secs(2)).await;

    // Verify: local nodes received, regional did not
    for node in &local_nodes[1..] {
        assert!(node.has_entry("test:local", hash).await);
    }
    for node in &regional_nodes {
        assert!(!node.has_entry("test:local", hash).await);
    }
}
```

---

## Phase 2: TrustPolicy + Resource Limits

### Phase 2A: TrustPolicy Infrastructure

**Goal**: Centralize all trust-based access decisions

#### 1. Create `icn-core/src/policy.rs`

```rust
//! Centralized trust policy engine

use icn_identity::Did;
use icn_trust::TrustClass;

pub trait PolicySource: Send + Sync {
    fn policy_for(&self, did: &Did) -> TrustPolicy;
}

#[derive(Debug, Clone)]
pub struct TrustPolicy {
    pub class: TrustClass,
    pub max_messages_per_second: u32,
    pub max_streams: u32,
    pub allowed_topics: Vec<String>, // Empty = all allowed
    pub allowed_capabilities: Vec<Capability>,
}

impl TrustPolicy {
    pub fn can_access_topic(&self, topic: &str) -> bool {
        self.allowed_topics.is_empty() || self.allowed_topics.contains(&topic.to_string())
    }

    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.allowed_capabilities.contains(cap)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Capability {
    ReadLedger,
    WriteLedger,
    DeployContract,
    ExecuteContract,
    ModifyTrust,
}

pub struct DefaultPolicySource {
    trust_graph: Arc<RwLock<TrustGraph>>,
}

impl PolicySource for DefaultPolicySource {
    fn policy_for(&self, did: &Did) -> TrustPolicy {
        let class = self.trust_graph.blocking_read()
            .trust_class(did)
            .unwrap_or(TrustClass::Isolated);

        match class {
            TrustClass::Isolated => TrustPolicy {
                class,
                max_messages_per_second: 10,
                max_streams: 2,
                allowed_topics: vec![], // All public topics
                allowed_capabilities: vec![],
            },
            TrustClass::Known => TrustPolicy {
                class,
                max_messages_per_second: 50,
                max_streams: 5,
                allowed_topics: vec![],
                allowed_capabilities: vec![Capability::ReadLedger],
            },
            TrustClass::Partner => TrustPolicy {
                class,
                max_messages_per_second: 100,
                max_streams: 10,
                allowed_topics: vec![],
                allowed_capabilities: vec![
                    Capability::ReadLedger,
                    Capability::WriteLedger,
                    Capability::ExecuteContract,
                ],
            },
            TrustClass::Federated => TrustPolicy {
                class,
                max_messages_per_second: 200,
                max_streams: 16,
                allowed_topics: vec![],
                allowed_capabilities: vec![
                    Capability::ReadLedger,
                    Capability::WriteLedger,
                    Capability::DeployContract,
                    Capability::ExecuteContract,
                ],
            },
        }
    }
}
```

#### 2. Wire Through Subsystems

**NetworkActor**: Check policy before accepting streams
```rust
async fn handle_incoming_stream(&self, peer: &Did, stream: QuicStream) -> Result<()> {
    let policy = self.policy_source.policy_for(peer);

    // Check stream limit
    let current_streams = self.active_streams_for_peer(peer);
    if current_streams >= policy.max_streams {
        stream.close();
        return Err(Error::TooManyStreams);
    }

    // Accept stream...
}
```

**GossipActor**: Check policy before delivering messages
```rust
async fn handle_announce(&mut self, from: &Did, topic: &str, hash: ContentHash) -> Result<()> {
    let policy = self.policy_source.policy_for(from);

    if !policy.can_access_topic(topic) {
        warn!("Peer {} denied access to topic {}", from, topic);
        return Err(Error::Forbidden);
    }

    // Process announce...
}
```

**ContractRuntime**: Check policy before executing
```rust
pub async fn invoke_rule(&self, contract: &Contract, rule: &str, caller: &Did) -> Result<ExecutionResult> {
    let policy = self.policy_source.policy_for(caller);

    // Check if caller can execute contracts
    if !policy.has_capability(&Capability::ExecuteContract) {
        return Err(Error::InsufficientCapability);
    }

    // Check fuel budget based on policy
    let fuel_budget = self.fuel_budget_for_policy(&policy);

    // Execute...
}
```

---

### Phase 2B: Global Rate Limiter

**Goal**: Enforce server-wide message rate cap

**File**: `crates/icn-net/src/global_rate_limit.rs` (NEW)

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct GlobalRateLimiter {
    max_global_mps: u32,
    window_start: Arc<RwLock<Instant>>,
    message_count: Arc<AtomicU64>,
}

impl GlobalRateLimiter {
    pub fn new(max_global_mps: u32) -> Self {
        Self {
            max_global_mps,
            window_start: Arc::new(RwLock::new(Instant::now())),
            message_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn check(&self) -> bool {
        let now = Instant::now();
        let mut start = self.window_start.write().unwrap();

        // Reset window if >1s elapsed
        if now.duration_since(*start) > Duration::from_secs(1) {
            *start = now;
            self.message_count.store(0, Ordering::Relaxed);
        }

        let count = self.message_count.fetch_add(1, Ordering::Relaxed);
        count < self.max_global_mps as u64
    }
}
```

Add to NetworkActor, check before processing any message.

---

### Phase 2C: Tests

- `test_policy_enforcement()` - Verify isolation
- `test_stream_limits()` - Verify per-peer caps
- `test_global_rate_limit()` - Verify server-wide cap
- `test_capability_gates()` - Verify CCL enforcement

---

## Phase 3: Paginated RPC + Receipts

### Phase 3A: Receipt Type

**File**: `crates/icn-rpc/src/receipt.rs` (NEW)

```rust
use serde::{Serialize, Deserialize};
use icn_identity::Did;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub id: ReceiptId,
    pub timestamp: u64,
    pub caller: Did,
    pub operation: Operation,
    pub outcome: Outcome,
    pub resources: Resources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptId(pub String); // UUID

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    ContractDeploy { code_hash: String },
    ContractExecute { code_hash: String, rule: String },
    LedgerTransfer { from: Did, to: Did, amount: i128 },
    TrustEdgeAdd { from: Did, to: Did, score: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Outcome {
    Success { commit_hash: Option<String> },
    Failure { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resources {
    pub fuel_used: u64,
    pub bytes_processed: usize,
    pub wall_time_ms: u64,
}
```

**Store receipts** in a TTL-bounded cache (e.g., last 10k receipts, 24h TTL).

Expose `/receipt/<id>` RPC endpoint.

---

### Phase 3B: Paginated List APIs

**Pattern** (apply to all list methods):

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct PageRequest {
    pub offset: usize,
    pub limit: usize, // Server caps at max_page_size
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PageResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub has_more: bool,
}

// Example: list_contracts
pub async fn list_contracts(&self, page: PageRequest) -> Result<PageResponse<ContractInfo>> {
    let limit = page.limit.min(MAX_PAGE_SIZE); // e.g., 100
    let contracts = self.contract_actor.list_contracts().await;

    let total = contracts.len();
    let items: Vec<_> = contracts.into_iter()
        .skip(page.offset)
        .take(limit)
        .collect();
    let has_more = page.offset + items.len() < total;

    Ok(PageResponse { items, total, has_more })
}
```

**Apply to**:
- `list_contracts`
- `list_neighbors` (new)
- `list_trust_edges`
- `list_ledger_entries`
- `list_gossip_topics`

---

### Phase 3C: icnctl Updates

**Add global flags**:
```
--limit N     Max results to return (default: 20)
--offset N    Skip first N results (default: 0)
```

**Update commands**:
```bash
icnctl contract list --limit 50 --offset 100
icnctl trust list-edges --limit 10
icnctl ledger entries --limit 100 --offset 500
```

**Pagination indicator**:
```
Showing 21-40 of 157 total
Use --offset 40 to see more
```

---

## Testing Strategy

### Unit Tests
- Each module's `#[cfg(test)]` section
- Property tests for invariants (ledger balance, vector clocks)

### Integration Tests
- `topology_integration.rs` - Multi-region scenarios
- `policy_enforcement_integration.rs` - Trust-gated operations
- `pagination_integration.rs` - Large result sets

### Scale Tests (icn-testkit)
- In-process sim: 100 nodes, 10 regions
- Measure: memory O(1) per node, convergence time
- Churn: 10%/min joins/leaves

---

## Deployment

### Migration Notes

**Topology** (backward compatible):
- Existing nodes default to region="default", cluster="default"
- Gradually configure regions in config files
- Old nodes will be placed in "backbone" set by new nodes

**TrustPolicy** (breaking for untrusted):
- `TrustClass::Isolated` gets restricted capabilities
- Existing mutual trust edges preserved
- May need to explicitly upgrade trust for some peers

**Pagination** (backward compatible):
- Old `list_*` calls get entire list (no pagination)
- New calls use `PageRequest` parameter

---

## Metrics Dashboard

**Grafana panels to add**:

```
Topology:
- icn_neighbors_total{set="local_cluster|regional|backbone|trusted"}
- icn_gossip_fanout{scope="local|regional|global"} (histogram)

Policy:
- icn_policy_checks_total{result="allowed|denied"}
- icn_stream_rejections_total{reason="limit|capability"}
- icn_messages_rate_limited_total{class="isolated|known|partner|federated"}

API:
- icn_rpc_page_size{method="list_contracts|list_neighbors|..."} (histogram)
- icn_receipt_lookups_total{result="hit|miss"}
```

---

## Status Snapshot

✅ Phase 1A: Topology config foundation (commit `fb8f5d4`)
🔄 Phase 1B: NeighborSets data structure (NEXT - see implementation above)
⏳ Phase 1C: Scope-aware gossip
⏳ Phase 1D: Metrics
⏳ Phase 1E: Integration tests
⏳ Phase 2: TrustPolicy (all)
⏳ Phase 3: Paginated RPC (all)

---

## Next Actions

1. **Implement Phase 1B** following the code template above
2. **Run**: `cargo test -p icn-net topology` to verify
3. **Commit**: "feat: Implement NeighborSets for topology-aware routing"
4. **Continue** to Phase 1C (gossip scope)
5. **Commit each phase** before moving to next

---

## Questions / Blockers

- **Handshake protocol**: How to exchange TopologyInfo? (Custom TLS extension vs. post-handshake app message)
- **Trust score updates**: How often to recompute neighbor set placement?
- **Scope defaults**: Should all topics be Global by default, or infer from ACL?

---

## References

- Original roadmap: (your detailed technical spec)
- Phase 10C commit: `f9e5493`
- Production hardening commit: `d0f6ed9`
- Config test commit: `fb8f5d4`

---

**Last Updated**: 2025-01-12
**Next Review**: After Phase 1 complete
