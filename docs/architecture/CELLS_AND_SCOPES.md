# Cells & Scopes Architecture

**Status**: Living Document
**Last Updated**: 2026-01-29
**Related Issues**: See [Issue Hierarchy](#12-github-issue-hierarchy) below

---

## Executive Summary

ICN's **Cells & Scopes** architecture introduces two primitives that enable multi-scale cooperative computing:

- **Cell**: A high-availability clustering envelope — a named group of nodes that share identity, state, and capacity within a scope boundary.
- **ScopeLevel**: A five-tier hierarchy (`Local → Cell → Org → Federation → Commons`) that governs resource allocation, job placement, data replication, service discovery, and economic settlement.

These primitives extend the existing kernel/app separation by adding **spatial awareness** to the kernel without introducing domain semantics. The kernel routes, replicates, and meters by scope level; apps decide *what* each scope level *means* for their domain.

**Key Design Constraint**: Cells and scopes are kernel primitives. The kernel knows that scope `Federation` is "wider" than `Org`, but it does not know what an organization *is*. That interpretation stays in apps, consistent with the [Meaning Firewall](KERNEL_APP_SEPARATION.md#1-the-meaning-firewall).

---

## Table of Contents

1. [Core Definitions](#1-core-definitions)
   - [Cell](#11-cell)
   - [ScopeLevel](#12-scopelevel)
   - [Cell Identity](#13-cell-identity)
   - [Cell Types](#14-cell-types)
2. [ScopeLevel Type Specification](#2-scopelevel-type-specification)
3. [Capacity Policy](#3-capacity-policy)
4. [Job Placement (Scoped Routing)](#4-job-placement-scoped-routing)
5. [Storage & Replication](#5-storage--replication)
6. [Service Discovery](#6-service-discovery)
7. [Economics & Settlement](#7-economics--settlement)
8. [Identity Across Devices](#8-identity-across-devices)
9. [Gossip Topics](#9-gossip-topics)
10. [Integration with Kernel/App Separation](#10-integration-with-kernelapp-separation)
11. [Migration Plan](#11-migration-plan)
12. [GitHub Issue Hierarchy](#12-github-issue-hierarchy)

---

## 1. Core Definitions

### 1.1 Cell

A **Cell** is a named, HA-clustered group of nodes that:

- Shares a derived identity (`CellId`)
- Pools compute, storage, and network capacity
- Replicates state within its membership
- Presents a single logical endpoint to peers outside the cell

Cells are the basic unit of **co-location**. Nodes within a cell are assumed to have low-latency, high-bandwidth interconnects (same rack, same datacenter, same home network). Cross-cell communication uses the existing QUIC/TLS networking layer.

```
┌─────────────────────────── Cell "workshop" ──────────────────────────────┐
│                                                                          │
│   ┌──────────┐    ┌──────────┐    ┌──────────┐                          │
│   │  Node A  │◄──►│  Node B  │◄──►│  Node C  │   Intra-cell: fast,     │
│   │  (icnd)  │    │  (icnd)  │    │  (icnd)  │   replicated, pooled    │
│   └──────────┘    └──────────┘    └──────────┘                          │
│                                                                          │
│   Shared: CellId, capacity pool, replicated state                       │
└──────────────────────────────────────────────────────────────────────────┘
         │                                           │
         │  QUIC/TLS (inter-cell)                    │  QUIC/TLS
         ▼                                           ▼
┌─────────────────┐                       ┌─────────────────┐
│  Cell "garden"  │                       │  Cell "market"  │
└─────────────────┘                       └─────────────────┘
```

### 1.2 ScopeLevel

A **ScopeLevel** defines the radius of an operation — how far a request, a piece of data, or a service announcement should travel.

| Level | Value | Meaning (kernel) | Example (app interprets) |
|-------|-------|-------------------|--------------------------|
| `Local` | 0 | This node only | Dev testing, local cache |
| `Cell` | 1 | Nodes in this cell | HA replicas, pooled capacity |
| `Org` | 2 | Cells in this organization | Coop-wide services |
| `Federation` | 3 | Organizations in this federation | Cross-coop clearing |
| `Commons` | 4 | All reachable nodes | Public compute pool |

The kernel treats `ScopeLevel` as an **ordered enum** — it knows `Cell < Org < Federation < Commons` but never interprets *why* something is scoped to a particular level. That decision belongs to apps and their `PolicyOracle` implementations.

### 1.3 Cell Identity

Every cell has a deterministic, collision-resistant identity:

```
CellId = blake3(scope_id || cell_name || genesis_salt)
```

Where:
- `scope_id`: The parent scope identifier (org DID, federation ID, or a well-known commons root)
- `cell_name`: Human-readable name within the scope (e.g., `"workshop"`, `"garden"`)
- `genesis_salt`: 32-byte random value set at cell creation time (prevents pre-image attacks)

### 1.4 Cell Types

Cells are classified by their parent scope (app-level semantics, not kernel):

| Cell Type | Parent Scope | Description |
|-----------|-------------|-------------|
| **Personal** | Individual DID | Multi-device cell for one person |
| **Org** | Cooperative DID | Servers owned by one cooperative |
| **Federation** | Federation ID | Cross-coop shared infrastructure |
| **Commons** | Well-known root | Open participation, no org affiliation |

The kernel does not distinguish these types — it only sees `CellId` and `ScopeLevel`. Apps assign meaning via `PolicyOracle`.

---

## 2. ScopeLevel Type Specification

### 2.1 Rust Definition

```rust
/// File: icn/crates/icn-kernel-api/src/scope.rs (new file)

use serde::{Deserialize, Serialize};

/// Scope level for operations, data, and services.
///
/// Defines how far an operation should reach in the network hierarchy.
/// The kernel uses this for routing, replication, and capacity allocation
/// without interpreting the domain semantics of each level.
///
/// # Ordering
///
/// ScopeLevel implements Ord: Local < Cell < Org < Federation < Commons.
/// This ordering is used for hierarchical fallback in placement and discovery.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
    Serialize, Deserialize,
)]
#[repr(u8)]
pub enum ScopeLevel {
    /// This node only — no network involvement
    Local = 0,
    /// Nodes within the same cell (HA cluster)
    Cell = 1,
    /// Cells within the same organization
    Org = 2,
    /// Organizations within the same federation
    Federation = 3,
    /// All reachable nodes (public commons)
    Commons = 4,
}

impl ScopeLevel {
    /// All scope levels in ascending order.
    pub const ALL: [ScopeLevel; 5] = [
        ScopeLevel::Local,
        ScopeLevel::Cell,
        ScopeLevel::Org,
        ScopeLevel::Federation,
        ScopeLevel::Commons,
    ];

    /// Return the next wider scope, or None if already at Commons.
    pub fn widen(&self) -> Option<ScopeLevel> {
        match self {
            ScopeLevel::Local => Some(ScopeLevel::Cell),
            ScopeLevel::Cell => Some(ScopeLevel::Org),
            ScopeLevel::Org => Some(ScopeLevel::Federation),
            ScopeLevel::Federation => Some(ScopeLevel::Commons),
            ScopeLevel::Commons => None,
        }
    }

    /// Return the next narrower scope, or None if already at Local.
    pub fn narrow(&self) -> Option<ScopeLevel> {
        match self {
            ScopeLevel::Local => None,
            ScopeLevel::Cell => Some(ScopeLevel::Local),
            ScopeLevel::Org => Some(ScopeLevel::Cell),
            ScopeLevel::Federation => Some(ScopeLevel::Org),
            ScopeLevel::Commons => Some(ScopeLevel::Federation),
        }
    }

    /// Check if this scope includes another scope.
    ///
    /// A wider scope includes all narrower scopes.
    /// e.g., `Org.includes(Cell)` is true.
    pub fn includes(&self, other: ScopeLevel) -> bool {
        *self >= other
    }

    /// Numeric value (for serialization and constraint sets).
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Parse from numeric value.
    pub fn from_u8(v: u8) -> Option<ScopeLevel> {
        match v {
            0 => Some(ScopeLevel::Local),
            1 => Some(ScopeLevel::Cell),
            2 => Some(ScopeLevel::Org),
            3 => Some(ScopeLevel::Federation),
            4 => Some(ScopeLevel::Commons),
            _ => None,
        }
    }
}

impl std::fmt::Display for ScopeLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScopeLevel::Local => write!(f, "local"),
            ScopeLevel::Cell => write!(f, "cell"),
            ScopeLevel::Org => write!(f, "org"),
            ScopeLevel::Federation => write!(f, "federation"),
            ScopeLevel::Commons => write!(f, "commons"),
        }
    }
}
```

### 2.2 Cell Identity Type

```rust
/// File: icn/crates/icn-kernel-api/src/scope.rs

/// Unique cell identifier — deterministic hash of scope + name + salt.
///
/// Two cells with the same parent scope and name but different genesis
/// salts produce different CellIds (intentional for re-creation scenarios).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash,
    Serialize, Deserialize,
)]
pub struct CellId(pub [u8; 32]);

impl CellId {
    /// Derive a CellId from its components.
    ///
    /// # Arguments
    /// - `scope_id`: Parent scope identifier (org DID, federation ID, etc.)
    /// - `cell_name`: Human-readable name within the scope
    /// - `genesis_salt`: 32-byte random value from cell creation
    pub fn derive(scope_id: &[u8], cell_name: &str, genesis_salt: &[u8; 32]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(scope_id);
        hasher.update(cell_name.as_bytes());
        hasher.update(genesis_salt);
        CellId(*hasher.finalize().as_bytes())
    }
}

impl std::fmt::Display for CellId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cell:{}", bs58::encode(&self.0).into_string())
    }
}
```

### 2.3 Integration Points

`ScopeLevel` is consumed by existing kernel types:

| Existing Type | New Field | Purpose |
|--------------|-----------|---------|
| `PlacementRequest` | `allowed_scopes: Vec<ScopeLevel>` | Scoped job routing |
| `ReplicationPolicy` | extended with `ScopeLevel` variant | Per-scope replication |
| `ConstraintSet` | `custom: { "scope_level": Int(n) }` | Scope as policy output |
| `NodeCapacity` | `scope_budgets: CapacityBudget` | Per-scope resource limits |

---

## 3. Capacity Policy

### 3.1 CapacityBudget

Each node allocates its resources across scope levels using a `CapacityBudget`. This is a kernel-enforced resource partitioning — apps influence the fractions via `PolicyOracle`, but the kernel enforces the limits.

```rust
/// File: icn/crates/icn-compute/src/scheduler.rs (addition)

/// Per-scope capacity allocation.
///
/// Fractions must sum to ≤ 1.0. Remaining capacity is unallocated (buffer).
/// The kernel enforces these limits when accepting tasks at each scope level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapacityBudget {
    /// Fraction reserved for local-only tasks (0.0–1.0)
    pub local_reserve: f64,
    /// Fraction available for cell peers (0.0–1.0)
    pub cell_share: f64,
    /// Fraction available for organization-wide tasks (0.0–1.0)
    pub org_share: f64,
    /// Fraction available for federation tasks (0.0–1.0)
    pub federation_share: f64,
    /// Fraction available for commons tasks (0.0–1.0)
    pub commons_share: f64,
}

impl Default for CapacityBudget {
    fn default() -> Self {
        Self {
            local_reserve: 0.30,    // 30% for local work
            cell_share: 0.25,       // 25% for cell peers
            org_share: 0.20,        // 20% for org-wide
            federation_share: 0.15, // 15% for federation
            commons_share: 0.10,    // 10% for commons
        }
    }
}

impl CapacityBudget {
    /// Validate that allocations sum to ≤ 1.0 and all are non-negative.
    pub fn validate(&self) -> Result<(), &'static str> {
        let total = self.local_reserve
            + self.cell_share
            + self.org_share
            + self.federation_share
            + self.commons_share;

        if total > 1.0 + f64::EPSILON {
            return Err("Capacity budget fractions exceed 1.0");
        }

        if self.local_reserve < 0.0
            || self.cell_share < 0.0
            || self.org_share < 0.0
            || self.federation_share < 0.0
            || self.commons_share < 0.0
        {
            return Err("Capacity budget fractions must be non-negative");
        }

        Ok(())
    }

    /// Get the fraction available for a given scope level.
    ///
    /// A wider scope can use its own allocation plus all narrower allocations
    /// that are currently unused (spillover).
    pub fn fraction_for(&self, scope: ScopeLevel) -> f64 {
        match scope {
            ScopeLevel::Local => self.local_reserve,
            ScopeLevel::Cell => self.cell_share,
            ScopeLevel::Org => self.org_share,
            ScopeLevel::Federation => self.federation_share,
            ScopeLevel::Commons => self.commons_share,
        }
    }
}
```

### 3.2 Dynamic Adjustment

Capacity budgets are not static. A demand-feedback loop adjusts allocations:

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│  Demand Signal  │────►│  ScopePolicy     │────►│  CapacityBudget     │
│  (queue depths  │     │  Oracle (app)    │     │  (kernel enforces)  │
│   per scope)    │     │                  │     │                     │
└─────────────────┘     └──────────────────┘     └─────────────────────┘
         ▲                                                 │
         │                                                 │
         └─────────────────── feedback ────────────────────┘
```

1. **Kernel** measures queue depth and utilization per scope level
2. **App** (ScopePolicy oracle) observes demand and outputs adjusted `CapacityBudget`
3. **Kernel** applies the new budget at the next capacity announcement interval

### 3.3 Gossip: Capacity Announcements

Existing `NodeCapacityAnnounce` messages are extended with scope budgets:

```rust
// Extension to ComputeMessage::NodeCapacityAnnounce
NodeCapacityAnnounce {
    executor: String,
    capacity: NodeCapacity,
    cell_id: Option<CellId>,          // NEW: which cell this node belongs to
    scope_budgets: CapacityBudget,    // NEW: per-scope allocations
}
```

---

## 4. Job Placement (Scoped Routing)

### 4.1 Extended PlacementRequest

```rust
/// Extension to PlacementRequest (icn-compute/src/scheduler.rs)

pub struct PlacementRequest {
    pub task_hash: [u8; 32],
    pub resource_profile: ResourceProfile,
    pub locality_hints: Vec<LocalityHint>,
    pub max_cost: Option<u64>,
    pub requested_at: u64,

    // === NEW: Scope-aware fields ===

    /// Allowed scope levels for placement, in preference order.
    ///
    /// The scheduler tries the narrowest scope first, then widens.
    /// Empty means "any scope" (backward compatible).
    pub allowed_scopes: Vec<ScopeLevel>,

    /// Cell affinity — prefer executors in this cell.
    pub cell_affinity: Option<CellId>,

    /// Maximum scope the submitter is willing to pay for.
    ///
    /// Wider scopes may incur higher costs (federation clearing fees, etc.)
    pub max_scope: Option<ScopeLevel>,
}
```

### 4.2 Hierarchical Placement Flow

When a task is submitted, the scheduler attempts placement at progressively wider scopes:

```
Submit task with allowed_scopes = [Cell, Org, Federation]

Step 1: Try Cell scope
  → Query executors in submitter's cell
  → If offer received with acceptable score → PLACE
  → If no offer or score too low → widen

Step 2: Try Org scope
  → Query executors across cells in this org
  → Apply org-level capacity budgets
  → If offer received → PLACE (+ higher cost tier)
  → If no offer → widen

Step 3: Try Federation scope
  → Query executors in federated cooperatives
  → Apply federation capacity budgets + min trust threshold
  → If offer received → PLACE (+ clearing fees)
  → If no offer → REJECT (max scope reached)
```

```
┌──────────┐
│  Submit  │
│  Task    │
└────┬─────┘
     │
     ▼
┌──────────────────┐     ┌─────────┐
│ Try Cell scope   │────►│ Placed? │──Yes──► Done
└──────────────────┘     └────┬────┘
                              │ No
                              ▼
                    ┌──────────────────┐     ┌─────────┐
                    │ Try Org scope    │────►│ Placed? │──Yes──► Done
                    └──────────────────┘     └────┬────┘
                                                  │ No
                                                  ▼
                                       ┌──────────────────────┐     ┌─────────┐
                                       │ Try Federation scope │────►│ Placed? │──Yes──► Done
                                       └──────────────────────┘     └────┬────┘
                                                                        │ No
                                                                        ▼
                                                                   ┌──────────┐
                                                                   │ Try      │
                                                                   │ Commons  │
                                                                   └────┬─────┘
                                                                        │
                                                                   (or reject)
```

### 4.3 Score Adjustments by Scope

The placement score formula (from `DefaultPlacementPolicy`) gains a scope factor:

| Scope Match | Score Adjustment |
|------------|-----------------|
| Same cell as submitter | +0.10 (co-location bonus) |
| Same org, different cell | +0.05 |
| Same federation, different org | +0.00 (neutral) |
| Commons | -0.05 (prefer closer scopes) |

These adjustments layer on top of the existing trust, capacity, queue depth, and locality factors.

---

## 5. Storage & Replication

### 5.1 Scope-Aware Replication Policy

The existing `ReplicationPolicy` enum is extended with scope awareness:

```rust
/// Extended ReplicationPolicy (icn-kernel-api/src/state.rs)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicationPolicy {
    /// Single node, no replication
    LocalOnly,
    /// Consensus group, linearizable reads/writes
    ClusterStrong,
    /// Gossip/CRDT, eventually consistent
    FederationEventual,
    /// Durable archive, retained indefinitely
    Archive,

    // === NEW: Scope-aware variants ===

    /// Replicate within a specific scope level.
    ///
    /// The kernel replicates to `factor` nodes within the given scope.
    /// e.g., `Scoped { scope: Cell, factor: 3 }` = 3 replicas within the cell.
    Scoped {
        scope: ScopeLevel,
        factor: u8,
    },
}
```

### 5.2 Per-Object Replication Configuration

Individual objects (blobs, log entries, KV values) can specify their replication scope:

```rust
/// Per-object replication configuration
pub struct ObjectReplication {
    /// Primary replication policy
    pub policy: ReplicationPolicy,
    /// Minimum scope for durability (narrower scopes may lose data on cell failure)
    pub min_durability_scope: ScopeLevel,
    /// Maximum scope (don't replicate beyond this level for privacy)
    pub max_scope: ScopeLevel,
}
```

### 5.3 Data Locality Integration

Data replication scope directly influences job placement:

- If input blobs are `Scoped { scope: Cell, .. }`, the scheduler strongly prefers executors in that cell
- If results must be stored at `Org` scope, the scheduler accounts for post-execution replication cost
- The existing `LocalityHint::DataLocality` hint is extended to include scope information

---

## 6. Service Discovery

### 6.1 ServiceEndpoint

A signed, scoped service advertisement:

```rust
/// File: icn/crates/icn-kernel-api/src/naming.rs (addition)

/// A discoverable service endpoint.
///
/// Service providers publish these via gossip. Consumers query by
/// service_id and scope. The kernel routes queries to the narrowest
/// matching scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    /// Unique service identifier (e.g., "compute:ccl", "storage:blob")
    pub service_id: String,
    /// DID of the service provider
    pub provider: Did,
    /// Type of endpoint
    pub endpoint_type: EndpointType,
    /// Network addresses (QUIC, HTTP, gRPC, etc.)
    pub addresses: Vec<String>,
    /// Capabilities this endpoint offers
    pub capabilities: Vec<String>,
    /// Minimum trust score required to use this service
    pub trust_threshold: f64,
    /// Scope at which this service is visible
    pub scope_visibility: ScopeLevel,
    /// Cell this endpoint belongs to (if any)
    pub cell_id: Option<CellId>,
    /// When this endpoint was last refreshed
    pub updated_at: u64,
    /// TTL in seconds (endpoint is stale after this)
    pub ttl_secs: u64,
    /// Ed25519 signature over all fields above
    pub signature: Vec<u8>,
}

/// Endpoint transport type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EndpointType {
    /// QUIC-based direct connection
    Quic,
    /// HTTP/REST API
    Http,
    /// gRPC service
    Grpc,
    /// WebSocket stream
    WebSocket,
}
```

### 6.2 Gossip Topics

Service announcements use scoped gossip topics:

| Topic | Scope | Purpose |
|-------|-------|---------|
| `services:cell:announce` | Cell | Intra-cell service ads |
| `services:org:announce` | Org | Org-wide service ads |
| `services:federation:announce` | Federation | Cross-coop service ads |
| `services:commons:announce` | Commons | Public service ads |
| `services:query` | Any | Service lookup requests |

### 6.3 Query/Resolution Flow

```
Consumer wants service "compute:ccl" at Org scope:

1. Check local service cache
2. Query services:cell:announce → find cell-local providers
3. If no match or score too low → query services:org:announce
4. Return best-scoring endpoint within allowed scope
```

---

## 7. Economics & Settlement

### 7.1 ExecutionReceipt

Every task execution produces a metered, signed receipt:

```rust
/// File: icn/crates/icn-compute/src/types.rs (addition)

/// Metered execution receipt with multi-party attestation.
///
/// Generated by the executor after task completion, acknowledged by the
/// submitter, and optionally attested by a third-party verifier.
/// Used as input to the settlement pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// Hash of the executed task
    pub task_hash: [u8; 32],
    /// DID of the executor
    pub executor: Did,
    /// DID of the task submitter
    pub submitter: Did,
    /// Scope at which execution occurred
    pub scope: ScopeLevel,
    /// CPU time consumed (seconds, fractional)
    pub cpu_seconds: f64,
    /// Memory usage (MB-seconds)
    pub memory_mb_seconds: f64,
    /// Storage consumed (bytes)
    pub storage_bytes: u64,
    /// Network egress (bytes)
    pub egress_bytes: u64,
    /// Fuel units consumed
    pub fuel_used: u64,
    /// Computed cost in credits
    pub cost: u64,
    /// Executor's Ed25519 signature over metering fields
    pub executor_signature: Vec<u8>,
    /// Submitter's acknowledgment signature (optional, for dispute prevention)
    pub submitter_ack: Option<Vec<u8>>,
    /// Third-party attester DID (for high-value tasks)
    pub attester: Option<Did>,
    /// Attester's signature over the receipt
    pub attester_signature: Option<Vec<u8>>,
    /// Timestamp of receipt creation
    pub created_at: u64,
}

impl ExecutionReceipt {
    /// Compute the signing payload (deterministic serialization of metering fields).
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.task_hash);
        payload.extend_from_slice(self.executor.as_str().as_bytes());
        payload.extend_from_slice(self.submitter.as_str().as_bytes());
        payload.push(self.scope.as_u8());
        payload.extend_from_slice(&self.cpu_seconds.to_le_bytes());
        payload.extend_from_slice(&self.memory_mb_seconds.to_le_bytes());
        payload.extend_from_slice(&self.storage_bytes.to_le_bytes());
        payload.extend_from_slice(&self.egress_bytes.to_le_bytes());
        payload.extend_from_slice(&self.fuel_used.to_le_bytes());
        payload.extend_from_slice(&self.cost.to_le_bytes());
        payload
    }
}
```

### 7.2 Settlement by Scope

Settlement mechanisms vary by scope:

| Scope | Settlement Mechanism | Latency |
|-------|---------------------|---------|
| **Local** | No settlement (self-consumption) | — |
| **Cell** | Internal ledger transfer | Immediate |
| **Org** | Internal ledger transfer | Immediate |
| **Federation** | Mutual credit clearing between orgs | Batched (hourly/daily) |
| **Commons** | Commons credit pool (earn by contributing, spend by consuming) | Batched |

```
┌─────────────────────────────────────────────────────────────────────────┐
│                       Settlement Pipeline                               │
│                                                                         │
│  ExecutionReceipt                                                       │
│       │                                                                 │
│       ▼                                                                 │
│  ┌─────────────────────┐                                                │
│  │ Scope Classification│                                                │
│  └─────────┬───────────┘                                                │
│            │                                                            │
│    ┌───────┼──────────┬──────────────┐                                  │
│    ▼       ▼          ▼              ▼                                  │
│  Local   Cell/Org   Federation    Commons                               │
│  (noop)  (direct    (clearing     (commons                              │
│           ledger     house)        credit                               │
│           entry)                   pool)                                │
└─────────────────────────────────────────────────────────────────────────┘
```

### 7.3 Resource Contribution Accounting

Nodes that contribute resources to wider scopes earn credits:

- Contributing compute to **Org** scope earns internal cooperative credits
- Contributing to **Federation** scope earns clearing credit with the federation
- Contributing to **Commons** scope earns commons credits, redeemable for commons resources

The kernel tracks resource contributions via `ExecutionReceipt` metering. Apps interpret the economic meaning.

---

## 8. Identity Across Devices

### 8.1 Personal Cells

An individual's devices form a **personal cell**:

```
┌──────────────── Personal Cell (Alice) ──────────────────┐
│                                                          │
│   Principal DID: did:icn:alice                           │
│   CellId: cell:7kQ3...(derived from alice's DID)        │
│                                                          │
│   ┌───────────┐   ┌───────────┐   ┌───────────┐        │
│   │  Laptop   │   │  Phone    │   │  Server   │        │
│   │ (device1) │   │ (device2) │   │ (device3) │        │
│   └───────────┘   └───────────┘   └───────────┘        │
│                                                          │
│   All devices share the principal DID's authority         │
│   State is replicated within the personal cell           │
└──────────────────────────────────────────────────────────┘
```

- Each device has its own Ed25519 keypair (for DID-TLS binding)
- The principal DID delegates authority to device keys
- Cell-level replication keeps devices in sync
- Loss of a single device doesn't lose identity (other devices hold replicas)

### 8.2 Independent Developer Nodes

Nodes without organizational affiliation participate directly in the **Commons** scope:

- No `Org` or `Cell` membership required
- Contribute compute/storage to the commons pool
- Earn commons credits for contributions
- Can consume commons resources using earned credits
- Trust scores are built individually (no organizational endorsement)

---

## 9. Gossip Topics

### 9.1 New Topics

| Topic | Direction | Payload | Description |
|-------|-----------|---------|-------------|
| `cell:announce` | Broadcast | `CellAnnounce` | Cell membership advertisement |
| `cell:join` | Directed | `CellJoinRequest` | Request to join a cell |
| `cell:leave` | Broadcast | `CellLeaveNotice` | Node departing a cell |
| `services:<scope>:announce` | Scoped | `ServiceEndpoint` | Service advertisement at scope |
| `services:query` | Request/Response | `ServiceQuery` | Service lookup |
| `settlement:receipt` | Directed | `ExecutionReceipt` | Settlement input |
| `settlement:dispute` | Directed | `SettlementDispute` | Receipt dispute |

### 9.2 Cell Lifecycle Messages

```rust
/// Cell membership announcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellAnnounce {
    /// The cell being announced
    pub cell_id: CellId,
    /// Human-readable cell name
    pub cell_name: String,
    /// Parent scope identifier
    pub scope_id: String,
    /// Scope level of this cell's parent
    pub scope_level: ScopeLevel,
    /// Current cell members (DIDs)
    pub members: Vec<Did>,
    /// Aggregate capacity of the cell
    pub aggregate_capacity: NodeCapacity,
    /// When this announcement was generated
    pub announced_at: u64,
    /// Signature by the cell's genesis key
    pub signature: Vec<u8>,
}
```

---

## 10. Integration with Kernel/App Separation

### 10.1 CellService Trait

Apps that manage cell lifecycle implement this trait:

```rust
/// File: icn/crates/icn-kernel-api/src/services.rs (addition)

/// Abstract cell management service.
///
/// The kernel uses this to query cell membership and scope topology
/// without knowing the organizational semantics behind cells.
pub trait CellService: Send + Sync {
    /// Get the cell this node belongs to (if any)
    fn local_cell(&self) -> Option<CellId>;

    /// Get the scope level of a given cell
    fn cell_scope(&self, cell_id: &CellId) -> Option<ScopeLevel>;

    /// List members of a cell
    fn cell_members(&self, cell_id: &CellId) -> Vec<Did>;

    /// Check if a DID is in the same cell as the local node
    fn is_cell_peer(&self, did: &Did) -> bool;

    /// Check if a DID is in the same org (any cell in the org)
    fn is_org_peer(&self, did: &Did) -> bool;

    /// Get the scope relationship between local node and a peer
    fn peer_scope(&self, did: &Did) -> ScopeLevel;
}
```

### 10.2 ScopePolicy Oracle

A `PolicyOracle` implementation that converts scope decisions into `ConstraintSet`:

```rust
/// Example: ScopePolicyOracle (app layer)
impl PolicyOracle for ScopePolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let scope = self.cell_service.peer_scope(&request.actor);

        // ═══ MEANING FIREWALL ═══
        // Scope semantics (org membership, federation status) end here.
        // Below this line, only generic constraints.

        let constraints = ConstraintSet::new()
            .with_rate_limit(scope_to_rate_limit(scope))
            .with_custom("scope_level", ConstraintValue::Int(scope.as_u8() as i64))
            .with_custom("max_job_cost", ConstraintValue::Int(
                scope_to_max_cost(scope)
            ));

        PolicyDecision::Allow { constraints }
    }

    fn domain(&self) -> Domain {
        Domain::Scope  // new domain variant
    }
}
```

### 10.3 Meaning Firewall Compliance

The kernel sees:
- `ScopeLevel` as an ordered integer (0–4)
- `CellId` as an opaque 32-byte identifier
- `CapacityBudget` as five floats summing to ≤ 1.0
- `ServiceEndpoint` as a signed blob with a visibility level

The kernel does NOT see:
- What "organization" means (cooperative? company? household?)
- Why a cell exists (HA? privacy? geographic proximity?)
- Why a budget is split a particular way (policy decision)
- Why a service is scoped to a particular level (business logic)

---

## 11. Migration Plan

### Phase 1: Foundation (ScopeLevel + CellId)

1. Add `ScopeLevel` enum and `CellId` type to `icn-kernel-api`
2. Add `CellService` trait to `icn-kernel-api/src/services.rs`
3. Register `CellService` in `ServiceRegistry`
4. Unit tests for all new types

### Phase 2: Capacity & Placement

1. Add `CapacityBudget` to `icn-compute`
2. Extend `PlacementRequest` with `allowed_scopes` and `cell_affinity`
3. Implement hierarchical placement routing in `DefaultPlacementPolicy`
4. Add demand-feedback loop for capacity adjustment

### Phase 3: Discovery & Networking

1. Add `ServiceEndpoint` type to `icn-kernel-api`
2. Add scoped gossip topics for service discovery
3. Add service query API to gateway
4. Integration tests for discovery flow

### Phase 4: Settlement & Economics

1. Add `ExecutionReceipt` type to `icn-compute`
2. Implement receipt signing and verification
3. Add settlement entry creation in `icn-ledger`
4. Add cross-scope clearing in `icn-federation`

### Phase 5: Replication & Storage

1. Extend `ReplicationPolicy` with `Scoped` variant
2. Add per-object replication configuration
3. Implement dynamic replication factor adjustment
4. Integration tests for scope-aware replication

### Phase 6: Commons Pool

1. Add `CommonsPool` type for aggregate capacity tracking
2. Implement unaffiliated node participation protocol
3. Add commons credit earning and spending
4. Integration tests for commons contributions

---

## 12. GitHub Issue Hierarchy

All implementation work is tracked in GitHub issues following the [Issue Policy](../../.github/ISSUE_POLICY.md).

**Level 0 (Meta)**:
- `meta: Cells & Scopes Architecture` — tracks all epics

**Level 1 (Epics)**:
1. `feat(kernel-api): ScopeLevel primitive and Cell identity`
2. `feat(compute): Scope-aware capacity budgets and placement`
3. `feat(kernel-api): Service endpoint discovery and registry`
4. `feat(compute): ExecutionReceipt and settlement pipeline`
5. `feat(store): Scope-aware replication policy`
6. `feat(compute): Commons resource pool and contribution accounting`

See individual issues for acceptance criteria and sub-tasks.

---

## Appendix: Key Files

| File | Purpose |
|------|---------|
| `icn/crates/icn-kernel-api/src/lib.rs` | Kernel API trait exports |
| `icn/crates/icn-kernel-api/src/scope.rs` | ScopeLevel, CellId (NEW) |
| `icn/crates/icn-kernel-api/src/services.rs` | CellService trait (addition) |
| `icn/crates/icn-kernel-api/src/naming.rs` | ServiceEndpoint type (addition) |
| `icn/crates/icn-kernel-api/src/state.rs` | ReplicationPolicy extension |
| `icn/crates/icn-compute/src/scheduler.rs` | CapacityBudget, PlacementRequest extensions |
| `icn/crates/icn-compute/src/types.rs` | ExecutionReceipt (addition) |
| `icn/crates/icn-federation/src/types.rs` | Cross-scope clearing types |
| `docs/architecture/KERNEL_APP_SEPARATION.md` | Meaning Firewall reference |
