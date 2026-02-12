# ICN Scheduler Evolution Plan

**Status**: Foundation Complete (Phase 16A)
**Next**: Phase 16B (Placement Scoring)
**Last Updated**: 2025-11-23

## Executive Summary

This document describes the evolution of ICN's compute layer from **reactive task claiming** (Phase 15) to a **distributed, trust-governed scheduler** (Phases 16A-E) that supports:

- Resource-aware placement (CPU/RAM/GPU/storage/network)
- Intelligent scoring (trust + capacity + locality + economics)
- Stateful actors with migration
- Multi-tier cooperative fabric (edge/community/regional)
- Per-coop scheduling policies

The design is **incremental**—each phase builds on the previous without disrupting existing functionality. This respects ICN's pilot-driven development philosophy while laying groundwork for the long-term vision.

---

## Vision Recap

ICN's compute layer aims to be a **planetary cooperative scheduler**:

1. **Trust-Governed**: Trust scores (from web-of-participation) gate access, set limits, and influence placement
2. **Economics-Aware**: Mutual credit balances determine priority; nodes earn credits by providing resources
3. **Locality-Aware**: Prefer nearby executors (network latency, data locality, geography)
4. **Cooperative Control**: Per-coop policies define custom rules, quotas, and budgets
5. **Distributed**: No central scheduler—gossip-based negotiation across nodes

### Multi-Tier Fabric

```
┌─────────────────────────────────────────────────────┐
│                  Cooperative Fabric                 │
├─────────────────────────────────────────────────────┤
│  Tier C: Regional Compute                           │
│  - Co-op data centers, racks                        │
│  - High capacity, always-on                         │
│  - Batch jobs, distributed training                 │
├─────────────────────────────────────────────────────┤
│  Tier B: Community Compute                          │
│  - Small servers, SBCs, home labs                   │
│  - Stable, good bandwidth                           │
│  - Persistent services, mid-weight tasks            │
├─────────────────────────────────────────────────────┤
│  Tier A: Edge Compute                               │
│  - Phones, laptops, IoT devices                     │
│  - Unreliable but local                             │
│  - User-facing tasks, privacy-sensitive             │
└─────────────────────────────────────────────────────┘
```

The scheduler treats these as one unified fabric with smart placement rules.

---

## Current State (Phase 15 - Complete ✅)

### How It Works Today

**Task Lifecycle**:
```
Submitter → TaskSubmitted (gossip broadcast)
      ↓
Executors see task → decide whether to claim
      ↓
First executor claims → TaskClaimed (gossip)
      ↓
Executor runs CCL contract → TaskResult (gossip)
      ↓
Payment settled via ledger
```

**Claiming Logic** ([icn-compute/src/actor/mod.rs](../../icn/crates/icn-compute/src/actor/mod.rs)):
1. Check trust: `MIN_TRUST_EXECUTE = 0.3`
2. Check capacity: `at_capacity()` (max_concurrent_tasks)
3. Check capabilities: `executor.can_execute(&task)`
4. If all pass, claim and execute

**Limitations**:
- ❌ No resource profiles (just fuel limit)
- ❌ First-come-first-claim (no intelligent matching)
- ❌ No locality awareness
- ❌ Tasks are stateless (one-shot execution)
- ❌ No cooperative policies

---

## Phase 16A: Resource Profiles & Matching (FOUNDATION - Complete ✅)

**Goal**: Replace vague "capabilities" with concrete resource requirements.

### New Types

[icn-compute/src/scheduler.rs](../../icn/crates/icn-compute/src/scheduler.rs) introduces:

#### `ResourceProfile`
Describes what a task needs:
```rust
pub struct ResourceProfile {
    pub cpu_cores: Option<f64>,      // 0.5 = half a core
    pub memory_mb: Option<u64>,
    pub storage_mb: Option<u64>,
    pub network_mbps: Option<f64>,
    pub gpu_spec: Option<GpuSpec>,
    pub duration_estimate: Option<Duration>,
}
```

**Examples**:
```rust
// Lightweight task
ResourceProfile::minimal() // 0.1 CPU, 128 MB RAM

// Compute-heavy
ResourceProfile::compute_heavy(4.0, 8192) // 4 cores, 8 GB

// GPU task
ResourceProfile::gpu(24, "sm_70".into()) // 24 GB GPU, compute capability sm_70
```

#### `NodeCapacity`
Tracks available resources per executor:
```rust
pub struct NodeCapacity {
    pub cpu_cores_available: f64,
    pub memory_mb_available: u64,
    pub storage_mb_available: u64,
    pub network_mbps: f64,
    pub gpu_devices: Vec<GpuDevice>,
    pub updated_at: u64,
}

impl NodeCapacity {
    pub fn can_fit(&self, profile: &ResourceProfile) -> bool;
    pub fn reserve(&mut self, profile: &ResourceProfile) -> Result<()>;
    pub fn release(&mut self, profile: &ResourceProfile);
}
```

**Usage Flow**:
```rust
// Executor checks if task fits before claiming
let capacity = node_capacity.lock().await;
if !capacity.can_fit(&task.resource_profile) {
    return; // Skip this task
}

// Reserve resources
capacity.reserve(&task.resource_profile)?;

// ... execute task ...

// Release resources
capacity.release(&task.resource_profile);
```

### Integration Strategy

**Backward Compatible**:
- Existing tasks without `resource_profile` use default `ResourceProfile::minimal()`
- Executors without capacity tracking use legacy claiming logic
- Migration is opt-in

**Gossip Message** (future):
```rust
pub enum ComputeMessage {
    // ... existing variants
    NodeCapacity {
        did: String,
        capacity: NodeCapacity,
        updated_at: u64,
    },
}
```

Executors announce capacity every 30 seconds. Submitters see available resources before submitting tasks.

### Success Criteria

- ✅ Tasks can specify CPU/RAM/GPU needs
- ✅ Executors reject tasks exceeding capacity
- ✅ Resource reservation prevents double-allocation
- ✅ Backward compatible with Phase 15 tasks
- ✅ All tests pass (7 new scheduler tests)

---

## Phase 16B: Placement Scoring (NEXT - 2-3 weeks)

**Goal**: Replace "first to claim" with "best fit" scoring.

### Design

#### Placement Flow

```
1. Submitter → PlacementRequest (gossip)
   {
     task_hash,
     resource_profile,
     locality_hints,
     max_cost
   }

2. Executors → Compute placement score
   score = f(trust, capacity, queue_depth, locality, economics)

3. Deliberation Window (500ms)
   - Executors compare their scores
   - Highest score wins

4. Winner → TaskClaimed (gossip)
```

#### Scoring Algorithm

[icn-compute/src/scheduler.rs](../../icn/crates/icn-compute/src/scheduler.rs):

```rust
pub trait PlacementPolicy {
    fn score_task(
        &self,
        task_hash: &[u8; 32],
        profile: &ResourceProfile,
        submitter: &str,
        node_state: &NodeState,
        trust_score: f64,
    ) -> Option<PlacementOffer>;
}

impl PlacementPolicy for DefaultPlacementPolicy {
    fn score_task(...) -> Option<PlacementOffer> {
        // Trust gate
        if trust_score < MIN_TRUST_EXECUTE { return None; }

        // Capacity check
        if !node_state.capacity.can_fit(profile) { return None; }

        let mut score = 0.0;

        // Factor 1: Trust (0.4 weight)
        score += (trust_score * 0.4).min(0.4);

        // Factor 2: Available capacity (0.3 weight)
        let capacity_ratio = node_state.capacity.available_ratio();
        score += capacity_ratio * 0.3;

        // Factor 3: Queue depth (0.2 weight)
        let queue_penalty = (queue_depth as f64 / 10.0).min(1.0);
        score += (1.0 - queue_penalty) * 0.2;

        // Factor 4: Random jitter (0.1 weight)
        score += rand::thread_rng().gen::<f64>() * 0.1;

        Some(PlacementOffer {
            executor: node_state.did.clone(),
            score,
            cost: calculate_cost(),
            estimated_start: queue_depth_ms(),
        })
    }
}
```

#### Deliberation Window

Prevents race conditions:

```rust
// Executor receives PlacementRequest
async fn on_placement_request(&self, req: PlacementRequest) {
    let offer = self.policy.score_task(...)?;

    // Wait 500ms to collect other offers
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check if someone with higher score already claimed
    if self.task_manager.is_claimed(&req.task_hash) {
        return; // Someone beat us
    }

    // Broadcast our offer
    self.send_callback(ComputeMessage::PlacementOffer(offer));
}
```

### Integration with Phase 15

**Dual Mode**:
- `PlacementRequest` messages use new scoring logic
- `TaskSubmitted` messages use legacy instant-claim logic
- Executors support both protocols

**Migration Path**:
1. Deploy Phase 16B code to all nodes
2. Submitters start using `PlacementRequest` for new tasks
3. Eventually deprecate `TaskSubmitted` after 6 months

### Implementation Tasks

1. Add `PlacementRequest` and `PlacementOffer` to `ComputeMessage` enum
2. Extend `ComputeActor` with `placement_policy: Box<dyn PlacementPolicy>`
3. Implement deliberation window logic
4. Add `on_placement_request` and `on_placement_offer` handlers
5. Update supervisor to inject `DefaultPlacementPolicy`
6. Add metrics: `placement_offers_sent`, `placement_wins`, `placement_losses`
7. Tests: scoring, deliberation, race conditions

**Estimated Time**: 2-3 weeks

---

## Phase 16C: Locality Awareness (3-4 weeks)

**Goal**: Make network topology and data locality first-class scheduling inputs.

### Network Topology Discovery

```rust
pub struct NetworkTopology {
    pub local_region: String,
    pub latency_map: HashMap<String, Duration>,   // DID → RTT
    pub bandwidth_map: HashMap<String, f64>,      // DID → Mbps
}
```

**Discovery Protocol**:
1. Nodes periodically ping peers (every 60s)
2. Measure RTT and bandwidth
3. Store in `NetworkTopology`
4. Use in placement scoring

**Updated Scoring**:
```rust
// Factor 5: Network proximity (0.1 weight)
if let Some(latency) = topology.latency_to(&task.submitter) {
    let proximity_score = 1.0 - (latency.as_millis() as f64 / 1000.0).min(1.0);
    score += proximity_score * 0.1;
}
```

### Data Locality

```rust
pub struct DataRegistry {
    pub blob_locations: HashMap<Hash, Vec<String>>, // Hash → DIDs hosting it
}

pub enum LocalityHint {
    DataLocality(Vec<Hash>),      // Schedule near this data
    NetworkProximity(String),     // Low latency to this DID
    GeographicRegion(String),     // "us-west", "eu-central"
    ColocateWith(TaskHash),       // Run on same node as another task
}
```

**Example Use Case**: ML training

```rust
// Submitter
let training_data = vec![
    hash_of_dataset_shard_1,
    hash_of_dataset_shard_2,
    hash_of_dataset_shard_3,
];

let request = PlacementRequest {
    task_hash,
    resource_profile: ResourceProfile::gpu(40, "sm_80".into()),
    locality_hints: vec![
        LocalityHint::DataLocality(training_data),
        LocalityHint::GeographicRegion("us-west".into()),
    ],
    max_cost: Some(1000),
};

// Executor scoring
let local_data_ratio = task.data_refs.iter()
    .filter(|hash| node.has_blob(hash))
    .count() as f64 / task.data_refs.len() as f64;

score += local_data_ratio * 0.2; // Prefer executors with data
```

### Implementation Tasks

1. Add `NetworkTopology` and `DataRegistry` to `ComputeActor`
2. Implement periodic ping/measure loop
3. Extend `PlacementPolicy::score_task` with locality factors
4. Add `LocalityHint` variants to `PlacementRequest`
5. Integrate with icn-store for blob location tracking
6. Add metrics: `topology_measurements`, `data_locality_hits`
7. Tests: topology discovery, data locality scoring

**Estimated Time**: 3-4 weeks

---

## Phase 16D: Actor State & Migration (4-6 weeks)

**Goal**: Support stateful, long-running actors that can migrate between nodes.

### Key Shift: Tasks → Actors

**Tasks** (current):
- Stateless, one-shot execution
- CCL contract runs, returns result, done
- No messaging, no persistence

**Actors** (future):
- Stateful (hold data across invocations)
- Long-running (can execute for hours/days)
- Message-driven (async communication)
- Migratable (move between nodes without data loss)

### New Types

```rust
pub struct Actor {
    pub id: ActorId,
    pub owner: String,
    pub state: Vec<u8>,                // Serialized actor state
    pub code: TaskCode,                // Actor behavior (CCL contract)
    pub message_queue: Vec<ActorMessage>,
    pub checkpoint: Option<Checkpoint>,
}

pub struct ActorMessage {
    pub from: ActorId,
    pub to: ActorId,
    pub payload: Vec<u8>,
    pub sent_at: u64,
}

pub struct Checkpoint {
    pub state_snapshot: Vec<u8>,
    pub sequence_number: u64,
    pub timestamp: u64,
    pub signature: Vec<u8>,
}
```

### Actor Lifecycle

```
Spawn → Running → Checkpointing → Migrating → Running (new node)
   ↓                   ↓
Suspended          Failed → Recover from checkpoint
```

### Migration Protocol

```
1. Source node: Pause actor, create checkpoint
   checkpoint = Checkpoint {
       state_snapshot: bincode::serialize(&actor.state),
       sequence_number: actor.message_count,
       timestamp: now(),
       signature: sign(&keypair, &state_snapshot),
   }

2. Source → Dest: Send checkpoint via gossip
   ComputeMessage::ActorMigrate {
       actor_id,
       from: "did:icn:node-a",
       to: "did:icn:node-b",
       checkpoint,
   }

3. Dest: Validate checkpoint, restore state
   verify_signature(&checkpoint)?;
   let state: ActorState = bincode::deserialize(&checkpoint.state_snapshot)?;
   actor.restore(state);

4. Dest → Source: Acknowledge receipt
   ComputeMessage::MigrationAck { actor_id }

5. Source: Delete actor state
   self.actors.remove(&actor_id);

6. Dest: Resume actor execution
   actor.resume();
```

### Migration Triggers

```rust
pub enum MigrationTrigger {
    LoadBalancing,       // Current node overloaded
    Failure,             // Current node crashed
    Policy,              // Coop rules require relocation
    Locality,            // Move closer to data/users
    Economic,            // Cheaper execution elsewhere
}
```

**Example**: Load balancing

```rust
// Monitor executor load
if self.executing_tasks.len() > self.max_concurrent_tasks * 2 {
    // Find actor to migrate (pick lowest priority)
    let actor_id = self.find_migratable_actor();

    // Find best destination (high capacity, same region)
    let dest = self.find_migration_target(actor_id)?;

    // Initiate migration
    self.migrate_actor(actor_id, dest, MigrationTrigger::LoadBalancing).await?;
}
```

### Checkpoint Strategy

**Periodic Checkpoints**:
- Every N messages (e.g., N=100)
- Every M seconds (e.g., M=60)

**On-Demand Checkpoints**:
- Before migration
- Before shutdown
- On explicit request

**Storage**:
- Small states (<10 KB): Store in gossip
- Large states (>10 KB): Store in external blob storage (icn-store)

### CCL Extensions for Actors

```rust
// Actor-specific CCL primitives
actor_spawn(code: Hash, initial_state: Value) -> ActorId
actor_send(to: ActorId, message: Value) -> Result
actor_receive() -> Option<Message>
actor_checkpoint() -> CheckpointId
actor_migrate(to: DID) -> Result
```

**Example Actor** (counter service):

```json
{
  "name": "CounterActor",
  "state_vars": [
    { "name": "count", "type": "Int" }
  ],
  "rules": [
    {
      "name": "increment",
      "body": [
        { "Assign": { "var": "count", "value": { "Add": ["count", 1] } } },
        { "Return": { "value": { "Var": "count" } } }
      ]
    },
    {
      "name": "get",
      "body": [
        { "Return": { "value": { "Var": "count" } } }
      ]
    },
    {
      "name": "checkpoint",
      "body": [
        { "ActorCheckpoint": { "state": { "Var": "count" } } }
      ]
    }
  ]
}
```

### Implementation Tasks

1. Design actor state format and serialization
2. Implement checkpoint creation and verification
3. Add `ActorMigrate`, `ActorMessage`, `ActorSpawn` to `ComputeMessage`
4. Build migration protocol (pause, checkpoint, transfer, resume)
5. Extend CCL interpreter with actor primitives
6. Add actor manager to `ComputeActor`
7. Implement migration triggers (load, failure, policy)
8. Add metrics: `actors_spawned`, `actors_migrated`, `checkpoints_created`
9. Tests: migration protocol, checkpoint recovery, message delivery

**Estimated Time**: 4-6 weeks

---

## Phase 16E: Cooperative Scheduling Policies (3-4 weeks)

**Goal**: Let cooperatives define custom scheduling rules and resource pools.

### Per-Coop Policies

```rust
pub struct CoopSchedulingPolicy {
    pub coop_id: String,
    pub member_priority: HashMap<String, u8>,      // DID → priority class
    pub resource_quotas: HashMap<String, ResourceQuota>,
    pub cost_model: CostModel,
    pub scheduling_rules: Vec<SchedulingRule>,
}

pub struct ResourceQuota {
    pub max_cpu_hours_per_month: f64,
    pub max_credits_per_month: u64,
    pub max_concurrent_tasks: usize,
}

pub enum SchedulingRule {
    PriorityDeadline {
        priority: TaskPriority,
        max_wait: Duration,
    },
    MemberQuota {
        did: String,
        max_share: f64,
    },
    DataSovereignty {
        region: String,
    },
    TimeWindow {
        allowed_hours: Vec<u8>,
    },
}
```

### Example Policies

#### Timebank
```rust
CoopSchedulingPolicy {
    coop_id: "timebank:seattle",
    member_priority: {
        "did:icn:alice": 1,  // Normal member
        "did:icn:bob": 2,    // Admin
    },
    resource_quotas: {
        "did:icn:alice": ResourceQuota {
            max_cpu_hours_per_month: 10.0,
            max_credits_per_month: 100,
            max_concurrent_tasks: 2,
        },
    },
    scheduling_rules: vec![
        SchedulingRule::MemberQuota {
            did: "did:icn:alice",
            max_share: 0.1, // Max 10% of compute pool
        },
    ],
}
```

#### Research Coop
```rust
CoopSchedulingPolicy {
    coop_id: "research:biolab",
    scheduling_rules: vec![
        SchedulingRule::DataSovereignty {
            region: "eu-central", // GDPR compliance
        },
        SchedulingRule::PriorityDeadline {
            priority: TaskPriority::Critical,
            max_wait: Duration::from_secs(300), // 5 min max
        },
        SchedulingRule::TimeWindow {
            allowed_hours: vec![22, 23, 0, 1, 2, 3, 4, 5], // Batch jobs at night
        },
    ],
}
```

### Integration with Governance (Phase 13)

Cooperatives vote on scheduling policies via governance proposals:

```rust
// Governance proposal
Proposal {
    domain_id: "research:biolab",
    title: "Add Data Sovereignty Rule (EU-only processing)",
    kind: ProposalKind::ConfigChange,
    payload: ConfigChange::SchedulingPolicy {
        rules: vec![
            SchedulingRule::DataSovereignty { region: "eu-central" }
        ],
    },
}

// After proposal passes
governance.on_proposal_executed(proposal_id, |config_change| {
    match config_change {
        ConfigChange::SchedulingPolicy { rules } => {
            compute_actor.update_policy(domain_id, rules)?;
        }
    }
});
```

### Policy Enforcement

```rust
pub trait PolicyEngine {
    fn enforce_policy(
        &self,
        task: &ComputeTask,
        submitter: &str,
        coop_policy: &CoopSchedulingPolicy,
    ) -> Result<(), PolicyViolation>;
}

impl PolicyEngine for DefaultPolicyEngine {
    fn enforce_policy(&self, task, submitter, policy) -> Result<()> {
        // Check member quota
        if let Some(quota) = policy.resource_quotas.get(submitter) {
            if self.usage_tracker.cpu_hours_this_month(submitter) >= quota.max_cpu_hours_per_month {
                return Err(PolicyViolation::QuotaExceeded);
            }
        }

        // Check priority deadline
        for rule in &policy.scheduling_rules {
            match rule {
                SchedulingRule::PriorityDeadline { priority, max_wait } => {
                    if task.priority == *priority {
                        if task.deadline.is_none() {
                            task.deadline = Some(now() + max_wait.as_millis() as u64);
                        }
                    }
                }
                SchedulingRule::DataSovereignty { region } => {
                    // Reject if executor not in required region
                    if !self.is_in_region(region) {
                        return Err(PolicyViolation::RegionRestriction);
                    }
                }
                // ... other rules
            }
        }

        Ok(())
    }
}
```

### Implementation Tasks

1. Design policy data structures
2. Implement `PolicyEngine` trait and default implementation
3. Add `CoopSchedulingPolicy` to governance domain state
4. Integrate policy checks into placement scoring
5. Add usage tracking (CPU hours, credits spent)
6. Implement policy violation logging and metrics
7. Add governance proposal types for policy updates
8. Tests: quota enforcement, region restrictions, time windows

**Estimated Time**: 3-4 weeks

---

## Integration Example: End-to-End Flow

Here's how all phases work together for a real-world scenario:

### Scenario: ML Training Job

**Context**: Research cooperative wants to train a model on distributed dataset.

```rust
// 1. Submitter prepares task
let training_task = ComputeTask {
    id: "train-model-v2",
    submitter: "did:icn:researcher-alice",
    code: TaskCode::Ccl(training_contract_json),
    inputs: bincode::serialize(&TrainingParams {
        learning_rate: 0.001,
        epochs: 100,
    }),
    fuel_limit: FuelLimit(10_000_000), // 10M ops
    required_capabilities: vec![ExecutorCapability::Ccl],
    priority: TaskPriority::High,
    created_at: now(),
    deadline: Some(now() + 3600_000), // 1 hour deadline
    payment_rate: Some(50), // 50 credits per 1000 fuel
    payment_currency: Some("credits".into()),
};

// 2. Attach resource profile
let resource_profile = ResourceProfile {
    cpu_cores: Some(8.0),    // 8-core machine
    memory_mb: Some(32_768), // 32 GB RAM
    storage_mb: Some(50_000), // 50 GB for model checkpoints
    network_mbps: Some(1000.0),
    gpu_spec: Some(GpuSpec {
        memory_gb: 40,
        compute_capability: "sm_80".into(), // A100
        device_count: 1,
    }),
    duration_estimate: Some(Duration::from_secs(3000)), // ~50 min
};

// 3. Add locality hints
let dataset_shards = vec![
    hash_of_shard_1, hash_of_shard_2, hash_of_shard_3,
];

let locality_hints = vec![
    LocalityHint::DataLocality(dataset_shards),
    LocalityHint::GeographicRegion("eu-central".into()), // GDPR
];

// 4. Create placement request
let placement_req = PlacementRequest {
    task_hash: training_task.hash(),
    resource_profile,
    locality_hints,
    max_cost: Some(500_000), // Max 500k credits
    requested_at: now(),
};

// 5. Submit to network
compute_handle.submit_with_placement(training_task, placement_req).await?;

// 6. Network negotiation (Phase 16B)
// - Executors score the task
// - Node in Frankfurt (eu-central) with A100 GPU scores highest:
//   - Trust: 0.35 (has 0.85 trust score)
//   - Capacity: 0.28 (75% available)
//   - Queue: 0.18 (queue depth 1)
//   - Locality: 0.15 (has 2 of 3 shards)
//   - Jitter: 0.05
//   - TOTAL: 1.01 (capped at 1.0)

// 7. Winner claims task
// - Frankfurt node sends PlacementOffer
// - After 500ms deliberation, claims task
// - Reserves resources (8 CPU, 32 GB, 40 GB GPU)

// 8. Execution (Phase 16D - future)
// - Spawn Actor (long-running training)
// - Periodic checkpoints every 10 minutes
// - If node fails, migrate to backup with checkpoint
// - If load increases, migrate to less-busy node

// 9. Policy enforcement (Phase 16E - future)
// - Check coop policy: "EU data sovereignty"
// - Verify executor in eu-central region ✓
// - Check quota: Alice used 15/50 CPU-hours this month ✓
// - Check deadline: High priority tasks max 1 hour ✓

// 10. Completion
// - Actor finishes training after 48 minutes
// - Payment settled: 48 min × 8 cores × 50 credits/1000 fuel = 19,200 credits
// - Alice → Frankfurt node: 19,200 credits
// - Result published to gossip
```

### Benefits Over Phase 15

**Phase 15** (reactive claiming):
- ❌ First executor with spare capacity claims (might be slow phone)
- ❌ No awareness of dataset location (downloads 50 GB from Alice)
- ❌ No GDPR compliance check
- ❌ No quota enforcement
- ❌ If executor crashes, job lost

**Phases 16A-E** (intelligent scheduling):
- ✅ Best-fit executor chosen (Frankfurt A100 with 2/3 shards local)
- ✅ Data locality saves 33 GB transfer
- ✅ Policy enforces EU data sovereignty
- ✅ Quota prevents Alice from hogging resources
- ✅ Checkpointing enables migration on failure

---

## Testing Strategy

### Phase 16A (Foundation)
- ✅ Unit tests for resource profiles
- ✅ Unit tests for capacity matching
- ✅ GPU capacity reservation
- ✅ Resource validation

### Phase 16B (Placement Scoring)
- [ ] Scoring algorithm correctness
- [ ] Deliberation window timing
- [ ] Race condition handling (multiple executors claiming)
- [ ] Economic cost calculation
- [ ] Integration test: 5 executors, highest score wins

### Phase 16C (Locality)
- [ ] Topology discovery (ping/measure)
- [ ] Data locality scoring
- [ ] Geographic region filtering
- [ ] Integration test: Task prefers executor with local data

### Phase 16D (Actor Migration)
- [ ] Checkpoint creation and verification
- [ ] Migration protocol (pause, transfer, resume)
- [ ] Message delivery during migration
- [ ] Failure recovery from checkpoint
- [ ] Integration test: Migrate actor between 3 nodes under load

### Phase 16E (Policies)
- [ ] Quota enforcement
- [ ] Data sovereignty checks
- [ ] Time window restrictions
- [ ] Governance integration (policy updates via proposals)
- [ ] Integration test: Coop votes on policy, enforcement takes effect

---

## Metrics & Observability

### New Prometheus Metrics

**Phase 16A**:
- `icn_compute_capacity_cpu_available{did}` - CPU cores free
- `icn_compute_capacity_memory_available{did}` - RAM free (MB)
- `icn_compute_capacity_gpu_available{did}` - GPUs free

**Phase 16B**:
- `icn_compute_placement_offers_sent_total` - Offers broadcast
- `icn_compute_placement_wins_total` - Tasks won
- `icn_compute_placement_losses_total` - Tasks lost to higher scorer
- `icn_compute_placement_score` - Histogram of scores

**Phase 16C**:
- `icn_compute_topology_latency_ms{peer}` - Measured RTT
- `icn_compute_data_locality_ratio` - Fraction of data local
- `icn_compute_migrations_locality_total` - Moves for data proximity

**Phase 16D**:
- `icn_compute_actors_running{did}` - Active actors
- `icn_compute_actors_migrated_total{reason}` - By trigger
- `icn_compute_checkpoints_created_total` - Checkpoint frequency
- `icn_compute_checkpoint_size_bytes` - Histogram

**Phase 16E**:
- `icn_compute_policy_violations_total{rule}` - By rule type
- `icn_compute_quota_usage{did, resource}` - Current usage vs limit

---

## Open Questions

### Technical

1. **Checkpoint Storage**: Where do large actor states (>1 GB) go?
   - Option A: Distributed blob storage (icn-store)
   - Option B: External S3-compatible storage
   - Option C: Cooperative-run storage nodes

2. **Migration Consensus**: Do we need multi-signature migration approvals?
   - Current design: Source node unilaterally migrates
   - Alternative: Require dest node confirmation before source deletes state

3. **Policy Conflicts**: What if coop policy contradicts node policy?
   - Example: Coop says "EU-only", but submitter in US
   - Resolution: Reject task vs downgrade to best-effort

### Economic

4. **Migration Costs**: Who pays for migration bandwidth?
   - Option A: Task submitter (included in payment)
   - Option B: Source node (migration is their problem)
   - Option C: Split 50/50

5. **Actor Hosting Fees**: Should long-running actors pay rent?
   - Option A: Flat fee per day
   - Option B: Resource-based (CPU-hours + storage-GB-hours)
   - Option C: Free (covered by initial task payment)

### Social

6. **Policy Authorship**: Who can propose scheduling policies?
   - Option A: Any member via governance
   - Option B: Only admins
   - Option C: Requires supermajority vote

7. **Cross-Coop Tasks**: Can Coop A use Coop B's executors?
   - Option A: Yes, if both policies allow
   - Option B: No, strict isolation
   - Option C: Yes, but at premium price

---

## Success Criteria (Overall)

**Phase 16A-E Complete When**:

1. ✅ **Resource-aware**: Tasks specify CPU/RAM/GPU, executors enforce
2. ⏳ **Intelligent placement**: Scoring beats random assignment by 50%+ (benchmark)
3. ⏳ **Locality-optimized**: 80% reduction in unnecessary data transfer
4. ⏳ **Fault-tolerant**: Actors survive executor crashes via checkpoints
5. ⏳ **Policy-compliant**: 100% enforcement of coop rules (quotas, regions, etc.)
6. ⏳ **Pilot-validated**: One pilot coop runs production workloads for 3+ months

**Benchmark**: ML training job
- Phase 15: 120 min (random executor, downloads dataset)
- Phase 16C: 55 min (local data, optimized placement)
- **2.2x speedup**

---

## Next Steps

### Immediate (Phase 16B)

1. Design `PlacementRequest` and `PlacementOffer` message formats
2. Implement `DefaultPlacementPolicy` scoring logic
3. Add deliberation window to `ComputeActor`
4. Write integration test: 5 executors, task goes to highest score
5. Add Prometheus metrics for placement

**Target**: 2-3 weeks, ready for pilot testing

### Short-Term (Phase 16C)

6. Implement network topology discovery (ping/measure)
7. Add `DataRegistry` to track blob locations
8. Extend scoring with locality factors
9. Test with realistic workload (ML training, batch processing)

**Target**: 1 month after 16B

### Long-Term (Phases 16D-E)

10. Actor state and migration (requires CCL extensions)
11. Cooperative scheduling policies (requires governance integration)
12. Pilot deployment with real cooperative

**Target**: Driven by pilot feedback, 3-6 months

---

## References

- [Phase 15 Compute Layer](../../icn/crates/icn-compute/) - Current implementation
- [ROADMAP.md](../development/sessions/undated/ROADMAP.md) - Overall project roadmap
- [CLAUDE.md](../../CLAUDE.md) - Project guidance
- [Scheduler Module](../../icn/crates/icn-compute/src/scheduler.rs) - Phase 16A foundation types
- [Strategic Gap Analysis](../development/sessions/2026-01/2025-01-15-strategic-gap-analysis.md) - Missing pieces for pilot deployment

---

**Document Status**: Living document, updated as implementation progresses
**Last Updated**: 2025-11-23
**Next Review**: After Phase 16B completes
