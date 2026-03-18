# Scope Scheduling: Capacity Slices and Preemption

**Status**: Specification  
**Version**: 1.0  
**Date**: 2026-02-11

---

## Overview

ICN uses a **scope hierarchy** to manage resource allocation and priority. Scopes represent jurisdictional boundaries where capacity can be pledged and priority can be claimed.

```
Commons < Local < Cell < Org < Federation
```

This document specifies how scopes interact with scheduling, preemption, and resource allocation.

---

## The Scope Hierarchy

### Scope Definitions

| Scope | Description | Trust Model |
|-------|-------------|-------------|
| **Commons** | Open participation, no membership required | Adversarial |
| **Local** | Your own node's resources | Strongest (self) |
| **Cell** | Small coordinated group | Coordinated |
| **Org** | Formal cooperative membership | Formal membership |
| **Federation** | Cross-cooperative treaties | Treaty-based |

### Priority Order

Higher scopes have higher priority **only when capacity is pledged**:

```
Commons (lowest) < Local < Cell < Org < Federation (highest)
```

### Key Invariants

1. **Commons is never authoritative** - Commons can propose, never decide
2. **Commons is always preemptible** - Any higher scope can evict commons work
3. **Priority requires pledged capacity** - No pledge = no priority claim
4. **Node remains sovereign** - Without pledges, only local matters

---

## Capacity Slices

### Definition

A **capacity slice** is a resource allocation pledged by a node to a higher scope:

```rust
pub struct CapacitySlice {
    pub scope: Scope,
    pub cpu_cores: f32,        // e.g., 2.0 cores
    pub memory_mb: u32,        // e.g., 4096 MB
    pub storage_gb: u32,       // e.g., 100 GB
    pub bandwidth_mbps: u32,   // e.g., 100 Mbps
    pub expires_at: Option<u64>,
}
```

### Pledge Semantics

When a node pledges capacity to a scope:

1. That capacity is **reserved** for the scope's work
2. The scope's scheduler can **claim** the capacity
3. Work at that scope level gets **priority** within the slice
4. Lower scopes cannot use the pledged capacity

### Example: Org Pledge

```
Node A pledges to Org "MyCoop":
  - 2 CPU cores
  - 4 GB RAM
  - 50 GB storage

MyCoop's scheduler can now:
  - Submit tasks that use up to 2 cores
  - Expect 4 GB RAM available
  - Store up to 50 GB of org data

Commons tasks CANNOT use this pledged capacity.
```

---

## Preemption Rules

### Preemption Within Pledged Capacity

Higher scopes can preempt lower scopes **within their pledged slice**:

```
Federation pledged work > Org pledged work > Cell pledged work > Local > Commons
```

### Preemption Logic

```rust
fn should_preempt(running: &Task, incoming: &Task) -> bool {
    // Higher scope always preempts within pledged capacity
    if incoming.scope > running.scope {
        if capacity_available_for(incoming.scope, incoming.resources) {
            return true;
        }
    }
    
    // Same scope: priority within scope
    if incoming.scope == running.scope {
        return incoming.priority > running.priority;
    }
    
    false
}
```

### Preemption Examples

| Running Task | Incoming Task | Result |
|--------------|---------------|--------|
| Commons indexing | Local backup | Preempt (local > commons) |
| Org service | Org service (higher prio) | Preempt (same scope, higher prio) |
| Cell compute | Org governance | Preempt if org has pledge |
| Federation settlement | Org service | No preempt (fed > org) |
| Local backup | Commons analytics | No preempt (local > commons) |

### Graceful Preemption

Preempted tasks receive:

1. **Signal** to checkpoint/pause
2. **Grace period** (configurable, default 30s)
3. **State preservation** if checkpointable
4. **Re-queue** at original scope level

---

## Scheduler Architecture

### Per-Scope Queues

```
┌─────────────────────────────────────────┐
│            Federation Queue              │  ← Highest priority
├─────────────────────────────────────────┤
│               Org Queue                  │
├─────────────────────────────────────────┤
│              Cell Queue                  │
├─────────────────────────────────────────┤
│              Local Queue                 │
├─────────────────────────────────────────┤
│             Commons Queue                │  ← Lowest priority
└─────────────────────────────────────────┘
```

### Scheduling Algorithm

```rust
fn schedule_next() -> Option<Task> {
    // Try federation first (if pledged)
    if let Some(task) = federation_queue.pop_if_capacity_available() {
        return Some(task);
    }
    
    // Then org (if pledged)
    if let Some(task) = org_queue.pop_if_capacity_available() {
        return Some(task);
    }
    
    // Then cell (if pledged)
    if let Some(task) = cell_queue.pop_if_capacity_available() {
        return Some(task);
    }
    
    // Then local (always available)
    if let Some(task) = local_queue.pop_if_capacity_available() {
        return Some(task);
    }
    
    // Finally commons (uses remaining capacity)
    commons_queue.pop_if_capacity_available()
}
```

### Capacity Accounting

```rust
struct CapacityManager {
    total: ResourceSet,
    pledges: HashMap<Scope, CapacitySlice>,
    in_use: HashMap<Scope, ResourceSet>,
}

impl CapacityManager {
    fn available_for(&self, scope: Scope) -> ResourceSet {
        match scope {
            Scope::Commons => {
                // Commons gets unpledged capacity only
                self.total - self.pledged_total()
            }
            _ => {
                // Higher scopes get their pledged slice
                self.pledges.get(&scope).map(|s| s.resources).unwrap_or_default()
            }
        }
    }
}
```

---

## Pledge Management

### Creating Pledges

```rust
pub struct PledgeRequest {
    pub node_id: Did,
    pub scope: Scope,
    pub scope_id: String,  // e.g., coop DID
    pub capacity: CapacitySlice,
    pub duration: Duration,
}
```

### Pledge Lifecycle

1. **Propose**: Node proposes pledge to scope
2. **Accept**: Scope governance accepts pledge
3. **Activate**: Capacity reserved, scheduler updated
4. **Renew**: Pledge extended before expiry
5. **Expire/Revoke**: Capacity returned to local

### Pledge Verification

Scope schedulers verify pledges before claiming:

```rust
fn verify_pledge(node: &Did, scope: &Scope) -> Option<CapacitySlice> {
    // Check pledge registry
    let pledge = pledge_registry.get(node, scope)?;
    
    // Verify not expired
    if pledge.is_expired() {
        return None;
    }
    
    // Verify scope membership
    if !scope.is_member(node) {
        return None;
    }
    
    Some(pledge.capacity)
}
```

---

## Service Level Agreements

### SLA Types

| Scope | SLA Model |
|-------|-----------|
| Commons | Best-effort, no guarantees |
| Local | Node operator's choice |
| Cell | Informal agreement |
| Org | Formal SLA with penalties |
| Federation | Treaty-defined SLA |

### SLA Metrics

```rust
pub struct ServiceLevelAgreement {
    pub availability: f64,      // e.g., 0.99 = 99%
    pub latency_p99_ms: u32,    // e.g., 100ms
    pub throughput_rps: u32,    // e.g., 1000 req/s
    pub penalty_credits: u64,   // Credits for SLA breach
}
```

### SLA Enforcement

For Org and Federation scopes:

1. **Monitor**: Track availability, latency, throughput
2. **Report**: Periodic SLA compliance reports
3. **Compensate**: Automatic credit transfer on breach
4. **Escalate**: Governance proposal for chronic breach

---

## Implementation Notes

### Data Structures

```rust
// Scope enum
pub enum Scope {
    Commons,
    Local,
    Cell { cell_id: String },
    Org { org_did: Did },
    Federation { fed_id: String },
}

impl Ord for Scope {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority_level().cmp(&other.priority_level())
    }
}

impl Scope {
    fn priority_level(&self) -> u8 {
        match self {
            Scope::Commons => 0,
            Scope::Local => 1,
            Scope::Cell { .. } => 2,
            Scope::Org { .. } => 3,
            Scope::Federation { .. } => 4,
        }
    }
}
```

### Scheduler Hooks

The scheduler must integrate with:

1. **Gossip** - Receive task submissions, broadcast results
2. **Trust graph** - Verify scope membership
3. **Ledger** - Record usage, settle capacity payments
4. **Governance** - Process pledge proposals

---

## Rationale

### Why Scopes?

Without scopes:
- All work competes equally for resources
- No way to guarantee capacity for cooperatives
- Commons abuse could starve legitimate work

With scopes:
- Cooperatives can guarantee resources for members
- Commons remains open but preemptible
- Fair sharing without centralized rationing

### Why Pledges?

Pledges create:
- **Accountability**: Node commits resources
- **Predictability**: Scope can plan capacity
- **Incentive alignment**: Pledge = stake in scope success

### Why Preemption?

Preemption enables:
- **Elastic commons**: Use spare capacity
- **Priority enforcement**: Critical work first
- **Graceful degradation**: Shed load under pressure

---

## References

- `docs/design/compute-classes.md` - LC/UC specification
- `docs/design/deterministic-core.md` - ICN-DC specification
- `docs/design/DETERMINISTIC_COMPUTE_SPRINT.md` - Implementation plan
